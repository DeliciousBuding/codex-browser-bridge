//! Result-oriented, bounded locator composition and actionability.
//!
//! Retries apply only to preparation probes. Once input or a DOM mutation is
//! sent it is never replayed; set_checked verifies the same element after its
//! single trusted pointer click. Cross-origin and transformed frames fail
//! explicitly rather than pretending this is the Playwright engine.

use std::time::Duration;

use serde_json::{json, Value};
use tokio::time::{sleep_until, timeout_at, Instant};

use crate::{
    browser::{evaluate_exception_message, parse_tab_id, runtime_value_string},
    client::Client,
    error::{BridgeError, Result},
};

const SCRIPT: &str = include_str!("locator.js");
const POLL: Duration = Duration::from_millis(100);
const MAX_SPEC_BYTES: usize = 64 * 1024;
const MATCH_FIELDS: &[&str] = &[
    "name",
    "text",
    "test_id",
    "placeholder",
    "label",
    "has_text",
    "has_not_text",
];

fn invalid(message: impl Into<String>) -> BridgeError {
    BridgeError::User(message.into())
}

/// Validate declarative recursive matchers before any page operation.
pub fn validate_spec(spec: &Value) -> Result<()> {
    if spec.to_string().len() > MAX_SPEC_BYTES {
        return Err(invalid("locator spec exceeds 65536 bytes"));
    }
    validate_query(spec, 0, &mut 0)?;
    for name in ["force", "trial", "checked"] {
        if spec.get(name).is_some_and(|v| !v.is_boolean()) {
            return Err(invalid(format!("{name} must be a boolean")));
        }
    }
    duration_field(spec, "timeout_ms", 10_000, 1, 60_000)?;
    duration_field(spec, "delay_ms", 0, 0, 1_000)?;
    if let Some(button) = spec.get("button") {
        if !matches!(button.as_str(), Some("left" | "middle" | "right")) {
            return Err(invalid("button must be left, middle or right"));
        }
    }
    crate::input::modifier_mask(&string_array(spec, "modifiers", false)?)?;
    if spec.get("action").and_then(Value::as_str) == Some("press") {
        string_array(spec, "keys", true)?;
    }
    if matches!(
        spec.get("action").and_then(Value::as_str),
        Some("type" | "press_sequentially")
    ) {
        let text = spec
            .get("value")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("type requires string value"))?;
        if text.chars().count() > 4096 {
            return Err(invalid("sequential text supports at most 4096 characters"));
        }
    }
    if let Some(options) = spec.get("options") {
        let descriptors: Vec<_> = options
            .as_array()
            .map(|a| a.iter().collect())
            .unwrap_or_else(|| vec![options]);
        if descriptors.len() > 200 {
            return Err(invalid("options supports at most 200 descriptors"));
        }
        for descriptor in descriptors {
            if descriptor.is_string() {
                continue;
            }
            let object = descriptor.as_object().ok_or_else(|| {
                invalid("option must be a string or value/label/index descriptor")
            })?;
            if object.is_empty()
                || object
                    .keys()
                    .any(|key| !matches!(key.as_str(), "value" | "label" | "index"))
            {
                return Err(invalid(
                    "option descriptor supports value, label and index only",
                ));
            }
            for field in ["value", "label"] {
                if object.get(field).is_some_and(|v| !v.is_string()) {
                    return Err(invalid(format!("option {field} must be a string")));
                }
            }
            if object.get("index").is_some_and(|v| v.as_u64().is_none()) {
                return Err(invalid("option index must be a nonnegative integer"));
            }
        }
    }
    Ok(())
}

fn validate_query(query: &Value, depth: usize, count: &mut usize) -> Result<()> {
    *count += 1;
    if depth > 8 || *count > 128 {
        return Err(invalid(
            "locator composition exceeds depth 8 or 128 queries",
        ));
    }
    let object = query
        .as_object()
        .ok_or_else(|| invalid("locator query must be an object"))?;
    if depth > 0
        && object
            .keys()
            .any(|key| matches!(key.as_str(), "frame_selectors" | "scopes" | "action"))
    {
        return Err(invalid(
            "nested matchers cannot cross frames, use scopes, or perform actions",
        ));
    }
    for field in ["selector", "role"] {
        if object.get(field).is_some_and(|v| !v.is_string()) {
            return Err(invalid(format!("{field} must be a string")));
        }
    }
    for field in MATCH_FIELDS {
        if let Some(matcher) = object.get(*field) {
            validate_matcher(field, matcher)?;
        }
    }
    for field in ["exact", "strict", "visible"] {
        if object.get(field).is_some_and(|v| !v.is_boolean()) {
            return Err(invalid(format!("{field} must be a boolean")));
        }
    }
    if object
        .get("nth")
        .is_some_and(|v| v.as_i64().is_none_or(|n| n < -1))
    {
        return Err(invalid("nth must be -1 or a nonnegative integer"));
    }
    if let Some(frames) = object.get("frame_selectors") {
        let frames = frames
            .as_array()
            .ok_or_else(|| invalid("frame_selectors must be an array"))?;
        if frames.len() > 8 || frames.iter().any(|v| !v.is_string()) {
            return Err(invalid("frame_selectors supports at most 8 strings"));
        }
    }
    for field in ["and", "or", "scopes"] {
        if let Some(children) = object.get(field) {
            let children = children
                .as_array()
                .ok_or_else(|| invalid(format!("{field} must be an array of locator objects")))?;
            if children.is_empty() || children.len() > 16 {
                return Err(invalid(format!("{field} supports 1..16 locator objects")));
            }
            for child in children {
                validate_query(child, depth + 1, count)?;
            }
        }
    }
    for field in ["has", "has_not"] {
        if let Some(child) = object.get(field) {
            validate_query(child, depth + 1, count)?;
        }
    }
    Ok(())
}

fn validate_matcher(field: &str, matcher: &Value) -> Result<()> {
    if matcher.is_string() {
        return Ok(());
    }
    let object = matcher
        .as_object()
        .ok_or_else(|| invalid(format!("{field} must be a string or regex matcher")))?;
    let pattern = object
        .get("regex")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid(format!("{field} regex must be a string")))?;
    let flags = match object.get("flags") {
        Some(value) => value
            .as_str()
            .ok_or_else(|| invalid("regex flags must be a string"))?,
        None => "",
    };
    // Small explicitly documented subset: no grouping/alternation, backrefs,
    // counted repetition or chained quantifiers. Reject rather than evaluating
    // attacker-controlled catastrophic patterns in an unrestricted renderer.
    let mut escaped = false;
    let mut quantifiers = 0;
    for c in pattern.chars() {
        if escaped {
            if c.is_ascii_digit() && c != '0' {
                return Err(invalid("regex backreferences are unsupported"));
            }
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if "()|{}".contains(c) {
            return Err(invalid(
                "regex grouping, alternation and counted repetition are unsupported",
            ));
        } else if "*+?".contains(c) {
            quantifiers += 1;
        }
    }
    if pattern.len() > 256
        || quantifiers > 1
        || flags.chars().any(|c| !"imsu".contains(c))
        || flags
            .chars()
            .enumerate()
            .any(|(i, c)| flags[..i].contains(c))
        || object.keys().any(|key| key != "regex" && key != "flags")
    {
        return Err(invalid(
            "regex supports <=256 bytes, one quantifier and unique imsu flags",
        ));
    }
    Ok(())
}

fn duration_field(spec: &Value, field: &str, default: u64, min: u64, max: u64) -> Result<u64> {
    match spec.get(field) {
        None => Ok(default),
        Some(value) => value
            .as_u64()
            .filter(|v| (min..=max).contains(v))
            .ok_or_else(|| {
                invalid(format!(
                    "{field} must be an integer between {min} and {max}"
                ))
            }),
    }
}

async fn evaluate(client: &Client, tab: i64, spec: &Value, deadline: Instant) -> Result<Value> {
    let raw = client.execute_cdp_once_with_timeout(tab, "Runtime.evaluate", Some(json!({
        "expression": SCRIPT.replace("__CODEX_LOCATOR_SPEC__", &spec.to_string()), "returnByValue": true
    })), deadline.saturating_duration_since(Instant::now())).await?;
    if let Some(message) = evaluate_exception_message(&raw) {
        return Err(invalid(format!("locator evaluation failed: {message}")));
    }
    let encoded = runtime_value_string(&raw)?
        .ok_or_else(|| BridgeError::Protocol("locator returned no value".into()))?;
    serde_json::from_str(&encoded)
        .map_err(|err| BridgeError::Protocol(format!("decode locator result: {err}")))
}

fn result_error(value: &Value) -> BridgeError {
    invalid(
        value
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("locator failed"),
    )
}

async fn pause(deadline: Instant) {
    sleep_until(std::cmp::min(deadline, Instant::now() + POLL)).await;
}

/// Resolve/read or perform an action with one bounded preparation loop.
pub async fn run(client: &Client, tab_id: &str, mut spec: Value) -> Result<Value> {
    validate_spec(&spec)?;
    let tab = parse_tab_id("locator", tab_id)?;
    let timeout_ms = duration_field(&spec, "timeout_ms", 10_000, 1, 60_000)?;
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    let action = spec
        .get("action")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    if !matches!(
        action.as_str(),
        "count"
            | "all"
            | "text"
            | "inner_text"
            | "text_content"
            | "all_text"
            | "all_text_contents"
            | "attribute"
            | "value"
            | "visible"
            | "enabled"
            | "checked"
            | "click"
            | "dblclick"
            | "pointer"
            | "set_checked"
            | "focus"
            | "press"
            | "type"
            | "press_sequentially"
            | "fill"
            | "select_option"
            | "probe"
    ) {
        return Err(invalid(format!("unsupported locator action: {action}")));
    }
    if spec.get("target_token").is_some() {
        return Err(invalid("target_token is an internal locator field"));
    }
    let pointer = matches!(
        action.as_str(),
        "click" | "dblclick" | "pointer" | "set_checked"
    );
    let actionable = pointer
        || matches!(
            action.as_str(),
            "fill" | "select_option" | "focus" | "press" | "type" | "press_sequentially"
        );
    let token = format!("__codex_locator_{}", uuid::Uuid::new_v4().simple());
    if pointer {
        spec["target_token"] = json!(token);
    }
    let result = run_until(client, tab, &spec, &action, pointer, actionable, deadline).await;
    if pointer {
        let cleanup_deadline = Instant::now() + Duration::from_millis(250);
        let _ = evaluate(
            client,
            tab,
            &json!({"action":"release_target", "target_token":token}),
            cleanup_deadline,
        )
        .await;
    }
    result
}

async fn run_until(
    client: &Client,
    tab: i64,
    spec: &Value,
    action: &str,
    pointer: bool,
    actionable: bool,
    deadline: Instant,
) -> Result<Value> {
    if matches!(action, "press" | "type" | "press_sequentially")
        && spec.get("trial") != Some(&json!(true))
    {
        crate::pointer::ensure_foreground(client, tab, deadline).await?;
    }
    let mut previous: Option<Value> = None;
    let mut last_error = "target did not become actionable".to_string();
    let mut result = loop {
        if Instant::now() >= deadline {
            return Err(BridgeError::Timeout(format!(
                "locator {action}: {last_error}"
            )));
        }
        let result = timeout_at(deadline, evaluate(client, tab, spec, deadline))
            .await
            .map_err(|_| BridgeError::Timeout(format!("locator {action}")))??;
        if result.get("ok").and_then(Value::as_bool) == Some(false) {
            if !actionable || result.get("retryable") != Some(&json!(true)) {
                return Err(result_error(&result));
            }
            last_error = result
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("not actionable")
                .to_string();
            previous = None;
            pause(deadline).await;
            continue;
        }
        let needs_pointer = pointer && result.get("needs_click") != Some(&json!(false));
        if needs_pointer && spec.get("force") != Some(&json!(true)) {
            let geometry = json!({"rect":result.get("rect"), "bounds":result.get("bounds")});
            if previous.as_ref() != Some(&geometry)
                || result.get("target_changed") == Some(&json!(true))
            {
                previous = Some(geometry);
                last_error = "target geometry is not stable".into();
                pause(deadline).await;
                continue;
            }
        }
        break result;
    };
    if spec.get("trial") == Some(&json!(true)) {
        result["trial"] = json!(true);
        return Ok(result);
    }
    if pointer && action != "pointer" && result.get("needs_click") != Some(&json!(false)) {
        let x = result
            .pointer("/rect/x")
            .and_then(Value::as_f64)
            .ok_or_else(|| BridgeError::Protocol("locator omitted rect.x".into()))?;
        let y = result
            .pointer("/rect/y")
            .and_then(Value::as_f64)
            .ok_or_else(|| BridgeError::Protocol("locator omitted rect.y".into()))?;
        let button = match spec.get("button").and_then(Value::as_str).unwrap_or("left") {
            "left" => 1,
            "middle" => 2,
            "right" => 3,
            _ => return Err(invalid("button must be left, middle or right")),
        };
        let modifiers = string_array(spec, "modifiers", false)?;
        // Never place the input dispatcher in the retry loop, including failure.
        crate::pointer::click(
            client,
            tab,
            x,
            y,
            button,
            if action == "dblclick" { 2 } else { 1 },
            &modifiers,
        )
        .await?;
        if action == "set_checked" {
            let mut probe = spec.clone();
            probe["action"] = json!("checked_probe");
            loop {
                if Instant::now() >= deadline {
                    return Err(BridgeError::Timeout(
                        "set_checked postcondition; click was not replayed".into(),
                    ));
                }
                let state = timeout_at(deadline, evaluate(client, tab, &probe, deadline))
                    .await
                    .map_err(|_| BridgeError::Timeout("set_checked postcondition".into()))??;
                if state.get("ok") == Some(&json!(false)) {
                    return Err(result_error(&state));
                }
                if state.get("value") == spec.get("checked") {
                    result["value"] = state["value"].clone();
                    result["verified"] = json!(true);
                    break;
                }
                pause(deadline).await;
            }
        }
        result["dispatched"] = json!(true);
    } else if action == "press" {
        if result.get("focused") != Some(&json!(true)) {
            return Err(invalid("matched element could not receive keyboard focus"));
        }
        crate::input::keypress(client, tab, &string_array(spec, "keys", true)?).await?;
        result["dispatched"] = json!(true);
    } else if matches!(action, "type" | "press_sequentially") {
        if result.get("focused") != Some(&json!(true)) {
            return Err(invalid("matched element could not receive keyboard focus"));
        }
        let text = spec
            .get("value")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("type requires string value"))?;
        let delay = Duration::from_millis(duration_field(spec, "delay_ms", 0, 0, 1000)?);
        if text.chars().count() > 4096 {
            return Err(invalid("sequential text supports at most 4096 characters"));
        }
        for ch in text.chars() {
            if Instant::now() >= deadline {
                return Err(BridgeError::Timeout(
                    "sequential text; partial input was not replayed".into(),
                ));
            }
            if ch.is_ascii() && !ch.is_ascii_control() {
                // Let the shared dispatcher finish its bounded release cleanup;
                // canceling it at the locator deadline could strand a held key.
                let keys = if ch.is_ascii_uppercase() {
                    vec!["Shift".to_string(), ch.to_string()]
                } else {
                    vec![ch.to_string()]
                };
                crate::input::keypress(client, tab, &keys).await?;
            } else {
                client
                    .execute_cdp_once_with_timeout(
                        tab,
                        "Input.insertText",
                        Some(json!({"text":ch.to_string()})),
                        deadline.saturating_duration_since(Instant::now()),
                    )
                    .await?;
            }
            if !delay.is_zero() {
                sleep_until(std::cmp::min(deadline, Instant::now() + delay)).await;
            }
        }
        result["dispatched"] = json!(true);
        result["characters"] = json!(text.chars().count());
    }
    Ok(result)
}

fn string_array(spec: &Value, field: &str, required: bool) -> Result<Vec<String>> {
    match spec.get(field) {
        None if !required => Ok(vec![]),
        Some(Value::Array(values)) => values
            .iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| invalid(format!("{field} must contain strings")))
            })
            .collect(),
        _ => Err(invalid(format!("{field} must be an array of strings"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recursive_matchers_and_select_descriptors_are_validated_before_io() {
        assert!(validate_spec(&json!({"and":[{"role":"button"},{"has":{"text":{"regex":"^Save.*$","flags":"i"}}}],"options":[{"label":"Second","index":1},"a"]})).is_ok());
        for spec in [
            json!({"name":{"regex":"(a+)+$"}}),
            json!({"name":{"regex":"a.*.*b"}}),
            json!({"name":{"regex":"ok","flags":"ii"}}),
            json!({"options":{"index":-1}}),
            json!({"has":{"frame_selectors":["iframe"]}}),
            json!({"timeout_ms":0}),
        ] {
            assert!(validate_spec(&spec).is_err(), "{spec}");
        }
    }
}
