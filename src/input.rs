//! Bounded, failure-safe keyboard chords shared by CUA and locator actions.

use std::collections::HashSet;
use std::time::Duration;
use tokio::time::{timeout_at, Instant};

use serde_json::{json, Value};

use crate::{
    client::Client,
    error::{BridgeError, Result},
};

const OP_TIMEOUT: Duration = Duration::from_secs(3);
const CLEANUP_TIMEOUT: Duration = Duration::from_secs(2);
const CLEANUP_EVENT_TIMEOUT: Duration = Duration::from_millis(250);

#[derive(Clone, Debug)]
struct KeySpec {
    key: String,
    code: String,
    vk: i64,
    modifier: u8,
    text: Option<String>,
}

fn normalize(name: &str) -> Result<KeySpec> {
    let n = name;
    let l = n.to_ascii_lowercase();
    let shifted_digit = [
        ("!", "Digit1", 49),
        ("@", "Digit2", 50),
        ("#", "Digit3", 51),
        ("$", "Digit4", 52),
        ("%", "Digit5", 53),
        ("^", "Digit6", 54),
        ("&", "Digit7", 55),
        ("*", "Digit8", 56),
        ("(", "Digit9", 57),
        (")", "Digit0", 48),
    ];
    if let Some((key, code, vk)) = shifted_digit.iter().find(|entry| entry.0 == n) {
        return Ok(KeySpec {
            key: (*key).into(),
            code: (*code).into(),
            vk: *vk,
            modifier: 0,
            text: Some((*key).into()),
        });
    }
    match l.as_str() {
        "ctrl" | "control" | "controlleft" => {
            return Ok(KeySpec {
                key: "Control".into(),
                code: "ControlLeft".into(),
                vk: 17,
                modifier: 2,
                text: None,
            })
        }
        "shift" | "shiftleft" => {
            return Ok(KeySpec {
                key: "Shift".into(),
                code: "ShiftLeft".into(),
                vk: 16,
                modifier: 8,
                text: None,
            })
        }
        "alt" | "altleft" | "option" => {
            return Ok(KeySpec {
                key: "Alt".into(),
                code: "AltLeft".into(),
                vk: 18,
                modifier: 1,
                text: None,
            })
        }
        "meta" | "cmd" | "command" | "win" => {
            return Ok(KeySpec {
                key: "Meta".into(),
                code: "MetaLeft".into(),
                vk: 91,
                modifier: 4,
                text: None,
            })
        }
        "controlright" | "shiftright" | "altright" | "metaright" => {
            let (key, code, vk, modifier) = match l.as_str() {
                "controlright" => ("Control", "ControlRight", 17, 2),
                "shiftright" => ("Shift", "ShiftRight", 16, 8),
                "altright" => ("Alt", "AltRight", 18, 1),
                _ => ("Meta", "MetaRight", 92, 4),
            };
            return Ok(KeySpec {
                key: key.into(),
                code: code.into(),
                vk,
                modifier,
                text: None,
            });
        }
        _ => {}
    }
    let (key, code, vk, text, _unmodified): (String, String, i64, Option<String>, Option<String>) = match l.as_str() {
        "controlormeta" => return Err(BridgeError::User(
            "ControlOrMeta requires an explicit remote browser platform; use Control or Meta when the platform is unknown".into(),
        )),
        "return" | "enter" => (
            "Enter".into(),
            "Enter".into(),
            13,
            Some("\r".into()),
            Some("\r".into()),
        ),
        "tab" => ("Tab".into(), "Tab".into(), 9, None, None),
        "backspace" => ("Backspace".into(), "Backspace".into(), 8, None, None),
        "esc" | "escape" => ("Escape".into(), "Escape".into(), 27, None, None),
        "delete" => ("Delete".into(), "Delete".into(), 46, None, None),
        "insert" => ("Insert".into(), "Insert".into(), 45, None, None),
        "left" | "arrowleft" => ("ArrowLeft".into(), "ArrowLeft".into(), 37, None, None),
        "right" | "arrowright" => ("ArrowRight".into(), "ArrowRight".into(), 39, None, None),
        "up" | "arrowup" => ("ArrowUp".into(), "ArrowUp".into(), 38, None, None),
        "down" | "arrowdown" => ("ArrowDown".into(), "ArrowDown".into(), 40, None, None),
        "home" => ("Home".into(), "Home".into(), 36, None, None),
        "end" => ("End".into(), "End".into(), 35, None, None),
        "pageup" => ("PageUp".into(), "PageUp".into(), 33, None, None),
        "pagedown" => ("PageDown".into(), "PageDown".into(), 34, None, None),
        "space" | " " => (
            " ".into(),
            "Space".into(),
            32,
            Some(" ".into()),
            Some(" ".into()),
        ),
        "backquote" | "grave" | "`" | "~" => (
            if l == "~" { "~".into() } else { "`".into() },
            "Backquote".into(), 192,
            Some(if l == "~" { "~".into() } else { "`".into() }),
            Some(if l == "~" { "~".into() } else { "`".into() }),
        ),
        "minus" | "-" | "_" => (
            if l == "_" { "_".into() } else { "-".into() },
            "Minus".into(), 189,
            Some(if l == "_" { "_".into() } else { "-".into() }),
            Some(if l == "_" { "_".into() } else { "-".into() }),
        ),
        "equal" | "=" | "+" => (
            if l == "+" { "+".into() } else { "=".into() },
            "Equal".into(), 187,
            Some(if l == "+" { "+".into() } else { "=".into() }),
            Some(if l == "+" { "+".into() } else { "=".into() }),
        ),
        "bracketleft" | "[" | "{" => (
            if l == "{" { "{".into() } else { "[".into() },
            "BracketLeft".into(), 219,
            Some(if l == "{" { "{".into() } else { "[".into() }),
            Some(if l == "{" { "{".into() } else { "[".into() }),
        ),
        "bracketright" | "]" | "}" => (
            if l == "}" { "}".into() } else { "]".into() },
            "BracketRight".into(), 221,
            Some(if l == "}" { "}".into() } else { "]".into() }),
            Some(if l == "}" { "}".into() } else { "]".into() }),
        ),
        "backslash" | "\\" | "|" => (
            if l == "|" { "|".into() } else { "\\".into() },
            "Backslash".into(), 220,
            Some(if l == "|" { "|".into() } else { "\\".into() }),
            Some(if l == "|" { "|".into() } else { "\\".into() }),
        ),
        "semicolon" | ";" | ":" => (
            if l == ":" { ":".into() } else { ";".into() },
            "Semicolon".into(), 186,
            Some(if l == ":" { ":".into() } else { ";".into() }),
            Some(if l == ":" { ":".into() } else { ";".into() }),
        ),
        "quote" | "'" | "\"" => (
            if l == "\"" { "\"".into() } else { "'".into() },
            "Quote".into(), 222,
            Some(if l == "\"" { "\"".into() } else { "'".into() }),
            Some(if l == "\"" { "\"".into() } else { "'".into() }),
        ),
        "comma" | "," | "<" => (
            if l == "<" { "<".into() } else { ",".into() },
            "Comma".into(), 188,
            Some(if l == "<" { "<".into() } else { ",".into() }),
            Some(if l == "<" { "<".into() } else { ",".into() }),
        ),
        "period" | "." | ">" => (
            if l == ">" { ">".into() } else { ".".into() },
            "Period".into(), 190,
            Some(if l == ">" { ">".into() } else { ".".into() }),
            Some(if l == ">" { ">".into() } else { ".".into() }),
        ),
        "slash" | "/" | "?" => (
            if l == "?" { "?".into() } else { "/".into() },
            "Slash".into(), 191,
            Some(if l == "?" { "?".into() } else { "/".into() }),
            Some(if l == "?" { "?".into() } else { "/".into() }),
        ),
        "numpadenter" | "kpenter" => ("Enter".into(), "NumpadEnter".into(), 13, Some("\r".into()), Some("\r".into())),
        "numpadmultiply" | "kpmultiply" => ("*".into(), "NumpadMultiply".into(), 106, Some("*".into()), Some("*".into())),
        "numpadadd" | "kpadd" => ("+".into(), "NumpadAdd".into(), 107, Some("+".into()), Some("+".into())),
        "numpadsubtract" | "kpsubtract" => ("-".into(), "NumpadSubtract".into(), 109, Some("-".into()), Some("-".into())),
        "numpaddecimal" | "kpdecimal" => (".".into(), "NumpadDecimal".into(), 110, Some(".".into()), Some(".".into())),
        "numpaddivide" | "kpdivide" => ("/".into(), "NumpadDivide".into(), 111, Some("/".into()), Some("/".into())),
        _ if l.starts_with('f')
            && l[1..]
                .parse::<u8>()
                .ok()
                .is_some_and(|v| (1..=24).contains(&v)) =>
        {
            let v = l[1..].parse::<u8>().unwrap();
            (format!("F{v}"), format!("F{v}"), 111 + v as i64, None, None)
        }
        _ if n.len() == 1 && n.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) => {
            let c = n.chars().next().unwrap();
            (
                c.to_ascii_lowercase().to_string(),
                format!("Key{}", c.to_ascii_uppercase()),
                c.to_ascii_uppercase() as i64,
                Some(c.to_string()),
                Some(c.to_ascii_lowercase().to_string()),
            )
        }
        _ if n.len() == 1 && n.chars().next().is_some_and(|c| c.is_ascii_digit()) => {
            let c = n.chars().next().unwrap();
            (
                c.to_string(),
                format!("Digit{c}"),
                c as i64,
                Some(c.to_string()),
                Some(c.to_string()),
            )
        }
        _ if l.starts_with("key") && l.len() == 4 && l.as_bytes()[3].is_ascii_alphabetic() => {
            let c = l.as_bytes()[3] as char;
            (
                c.to_string(),
                format!("Key{}", c.to_ascii_uppercase()),
                c.to_ascii_uppercase() as i64,
                Some(c.to_string()),
                Some(c.to_string()),
            )
        }
        _ if l.starts_with("digit") && l.len() == 6 && l.as_bytes()[5].is_ascii_digit() => {
            let c = l.as_bytes()[5] as char;
            (
                c.to_string(),
                format!("Digit{c}"),
                c as i64,
                Some(c.to_string()),
                Some(c.to_string()),
            )
        }
        _ if l.starts_with("numpad") && l.len() == 7 && l.as_bytes()[6].is_ascii_digit() => {
            let c = l.as_bytes()[6] as char;
            (c.to_string(), format!("Numpad{c}"), 96 + (c as i64 - '0' as i64), Some(c.to_string()), Some(c.to_string()))
        }
        _ if l.starts_with("kp") && l.len() == 3 && l.as_bytes()[2].is_ascii_digit() => {
            let c = l.as_bytes()[2] as char;
            (c.to_string(), format!("Numpad{c}"), 96 + (c as i64 - '0' as i64), Some(c.to_string()), Some(c.to_string()))
        }
        _ => return Err(BridgeError::User(format!("unsupported key {:?}", name))),
    };
    Ok(KeySpec {
        key,
        code,
        vk,
        modifier: 0,
        text,
    })
}

/// Convert official pointer modifier names into the CDP modifier bitmask.
///
/// Pointer operations carry modifiers directly in each CDP mouse event. They
/// do not synthesize keyboard events, which avoids leaving a physical key held
/// if a click or drag fails partway through.
pub fn modifier_mask(names: &[String]) -> Result<i64> {
    let mut mask = 0u8;
    let mut seen = HashSet::new();
    for name in names {
        let spec = normalize(name)?;
        if spec.modifier == 0 {
            return Err(BridgeError::User(format!(
                "pointer modifier {name:?} is not Alt, Control, Meta, or Shift"
            )));
        }
        if !seen.insert(spec.modifier) {
            return Err(BridgeError::User(format!(
                "pointer modifiers contain duplicate {name:?}"
            )));
        }
        mask |= spec.modifier;
    }
    Ok(i64::from(mask))
}

fn shifted_key(spec: &KeySpec, shifted: bool) -> String {
    if !shifted {
        return spec.key.clone();
    }
    if spec.code.starts_with("Key") {
        return spec.key.to_ascii_uppercase();
    }
    if spec.code.starts_with("Digit") {
        return match spec.key.as_str() {
            "0" => ")",
            "1" => "!",
            "2" => "@",
            "3" => "#",
            "4" => "$",
            "5" => "%",
            "6" => "^",
            "7" => "&",
            "8" => "*",
            "9" => "(",
            _ => &spec.key,
        }
        .into();
    }
    match spec.code.as_str() {
        "Backquote" => "~",
        "Minus" => "_",
        "Equal" => "+",
        "BracketLeft" => "{",
        "BracketRight" => "}",
        "Backslash" => "|",
        "Semicolon" => ":",
        "Quote" => "\"",
        "Comma" => "<",
        "Period" => ">",
        "Slash" => "?",
        _ => &spec.key,
    }
    .into()
}

fn event(spec: &KeySpec, kind: &str, modifiers: u8, shifted: bool) -> Value {
    let key = shifted_key(spec, shifted);
    let mut text = if spec.key == "Enter" {
        Some("\r".to_string())
    } else if spec.modifier == 0 && key.chars().count() == 1 {
        Some(key.clone())
    } else {
        spec.text.clone()
    };
    if modifiers & (1 | 2 | 4) != 0 {
        text = None;
    }
    let event_type = if kind == "keyUp" {
        "keyUp"
    } else if text.is_some() {
        "keyDown"
    } else {
        "rawKeyDown"
    };
    let location = if spec.code.starts_with("Numpad") {
        3
    } else if spec.modifier != 0 && spec.code.ends_with("Left") {
        1
    } else if spec.modifier != 0 && spec.code.ends_with("Right") {
        2
    } else {
        0
    };
    let mut v = json!({"type":event_type,"key":key,"code":spec.code,"windowsVirtualKeyCode":spec.vk,"modifiers":modifiers,"location":location,"isKeypad":location == 3});
    if event_type == "keyDown" {
        if let Some(t) = text {
            v["text"] = json!(t);
            v["unmodifiedText"] = json!(t);
        }
    }
    v
}

async fn send(
    client: &Client,
    tab: i64,
    spec: &KeySpec,
    kind: &str,
    modifiers: u8,
    shifted: bool,
    timeout: Duration,
) -> Result<()> {
    client
        .execute_cdp_with_timeout(
            tab,
            "Input.dispatchKeyEvent",
            Some(event(spec, kind, modifiers, shifted)),
            timeout,
        )
        .await
        .map(|_| ())
}

async fn send_until(
    client: &Client,
    tab: i64,
    spec: &KeySpec,
    kind: &str,
    modifiers: u8,
    shifted: bool,
    deadline: Instant,
) -> Result<()> {
    let timeout = deadline.saturating_duration_since(Instant::now());
    if timeout.is_zero() {
        return Err(BridgeError::Timeout("keypress operation deadline".into()));
    }
    timeout_at(
        deadline,
        send(client, tab, spec, kind, modifiers, shifted, timeout),
    )
    .await
    .map_err(|_| BridgeError::Timeout("keypress operation deadline".into()))?
}

fn expand_keys(keys: &[String]) -> Result<Vec<String>> {
    let mut expanded = Vec::new();
    for value in keys {
        if value.is_empty() || (value.len() > 1 && value.ends_with('+')) {
            return Err(BridgeError::User(format!(
                "malformed key combination {value:?}"
            )));
        }
        let mut token = String::new();
        for ch in value.chars() {
            if ch == '+' && !token.is_empty() {
                expanded.push(std::mem::take(&mut token));
            } else {
                token.push(ch);
            }
        }
        if !token.is_empty() {
            expanded.push(token);
        } else if value == "+" {
            expanded.push("+".into());
        }
    }
    Ok(expanded)
}

/// Press a single validated key combination. Modifiers are held while the
/// final key is pressed and are always released in reverse order on failure.
/// Normal dispatches share a three-second deadline; failure cleanup has its own
/// two-second deadline and 250 ms per release, so it still runs after timeout.
/// ControlOrMeta is rejected until the remote browser platform is known.
pub async fn keypress(client: &Client, tab: i64, keys: &[String]) -> Result<()> {
    if keys.is_empty() {
        return Err(BridgeError::User(
            "keypress requires at least one key".into(),
        ));
    }
    let expanded = expand_keys(keys)?;
    let specs: Vec<_> = expanded
        .iter()
        .map(|k| normalize(k))
        .collect::<Result<_>>()?;
    let mut seen = HashSet::new();
    for s in &specs {
        if !seen.insert(s.code.clone()) {
            return Err(BridgeError::User(
                "keypress contains duplicate physical key".into(),
            ));
        }
    }
    let modifiers: Vec<_> = specs.iter().filter(|s| s.modifier != 0).cloned().collect();
    let finals: Vec<_> = specs.iter().filter(|s| s.modifier == 0).cloned().collect();
    if finals.len() != 1 {
        return Err(BridgeError::User(
            "a key combination must contain exactly one non-modifier key".into(),
        ));
    }
    let final_key = &finals[0];
    let mut mask = 0u8;
    let mut pressed: Vec<&KeySpec> = Vec::new();
    let deadline = Instant::now() + OP_TIMEOUT;
    let result: Result<()> = async {
        for m in &modifiers {
            mask |= m.modifier;
            pressed.push(m);
            send_until(client, tab, m, "keyDown", mask, false, deadline).await?;
        }
        pressed.push(final_key);
        send_until(
            client,
            tab,
            final_key,
            "keyDown",
            mask,
            modifiers.iter().any(|m| m.modifier == 8),
            deadline,
        )
        .await?;
        send_until(
            client,
            tab,
            final_key,
            "keyUp",
            mask,
            mask & 8 != 0,
            deadline,
        )
        .await?;
        pressed.pop();
        while let Some(m) = pressed.last().copied() {
            mask = pressed[..pressed.len() - 1]
                .iter()
                .fold(0, |bits, key| bits | key.modifier);
            send_until(client, tab, m, "keyUp", mask, false, deadline).await?;
            pressed.pop();
        }
        Ok(())
    }
    .await;
    if result.is_ok() {
        return Ok(());
    }
    let original = result.unwrap_err();
    let mut cleanup_err = None;
    let deadline = Instant::now() + CLEANUP_TIMEOUT;
    while let Some(m) = pressed.pop() {
        let cleanup_mask = pressed.iter().fold(0, |bits, key| bits | key.modifier);
        let attempt_deadline = std::cmp::min(deadline, Instant::now() + CLEANUP_EVENT_TIMEOUT);
        if let Err(e) = send_until(
            client,
            tab,
            m,
            "keyUp",
            cleanup_mask,
            cleanup_mask & 8 != 0,
            attempt_deadline,
        )
        .await
        {
            cleanup_err = Some(e);
        }
    }
    if let Some(e) = cleanup_err {
        Err(BridgeError::Protocol(format!(
            "{original}; key release cleanup failed: {e}"
        )))
    } else {
        Err(original)
    }
}
