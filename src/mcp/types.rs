use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const DEFAULT_MAX_TEXT_CONTENT_BYTES: usize = 1_048_576;
const DEFAULT_MAX_IMAGE_CONTENT_BYTES: usize = 3_145_728;
const MAX_CONFIGURABLE_CONTENT_BYTES: usize = 8 * 1024 * 1024;
const MIN_CONFIGURABLE_CONTENT_BYTES: usize = 1024;

#[derive(Clone, Copy)]
pub(super) enum ToolHandler {
    ListTabs,
    CreateTab,
    CloseTab,
    UserTabs,
    ClaimTab,
    Navigate,
    Reload,
    NavigateBack,
    NavigateForward,
    WaitForLoad,
    DomSnapshot,
    Screenshot,
    Click,
    Fill,
    Evaluate,
    CuaClick,
    CuaType,
    CuaKeypress,
    CuaScroll,
    DomGetVisible,
    DomClick,
    NameSession,
    Finalize,
    GetInfo,
    ExecuteCdp,
    PageAssets,
    NetworkCookies,
    NetworkSetCookie,
    FileInput,
    Dialog,
    FindElement,
    ClickElement,
    NavAndWait,
    ClickAndWait,
    FormFill,
    Doctor,
    BringToFront,
    GetUrl,
    GetTitle,
    WaitForElement,
    Hover,
    PrintPdf,
    Storage,
    SelectOption,
    Drag,
    ScreenshotElement,
    DeleteCookies,
    EmulateDevice,
    NetworkMonitor,
    ConsoleLogs,
    WaitForUrl,
    PerformanceMetrics,
}

#[derive(Clone)]
pub(crate) struct Tool {
    pub(super) name: &'static str,
    pub(super) description: &'static str,
    pub(super) input_schema: Value,
    pub(super) handler: ToolHandler,
}

#[derive(Debug, Deserialize)]
pub(super) struct RpcRequest {
    pub(super) jsonrpc: Option<String>,
    pub(super) id: Option<Value>,
    pub(super) method: String,
    #[serde(default)]
    pub(super) params: Option<Value>,
}

#[derive(Debug, Serialize)]
pub(super) struct Content {
    #[serde(rename = "type")]
    kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<String>,
    #[serde(rename = "mimeType", skip_serializing_if = "Option::is_none")]
    mime_type: Option<&'static str>,
}

impl Tool {
    pub(super) fn new(
        name: &'static str,
        description: &'static str,
        input_schema: Value,
        handler: ToolHandler,
    ) -> Self {
        Self {
            name,
            description,
            input_schema,
            handler,
        }
    }
}

impl Content {
    pub(super) fn text(text: String) -> Self {
        let text = bounded_text(text, max_text_content_bytes());
        Self {
            kind: "text",
            text: Some(text),
            data: None,
            mime_type: None,
        }
    }

    pub(super) fn image(data: String, mime_type: &'static str) -> Self {
        let limit = max_image_content_bytes();
        if data.len() > limit {
            return Self::text(format!(
                "Image omitted because it is {} bytes base64, above CODEX_BRIDGE_MAX_IMAGE_BYTES={limit}. Retry with jpeg/webp quality or a narrower element screenshot.",
                data.len()
            ));
        }
        Self {
            kind: "image",
            text: None,
            data: Some(data),
            mime_type: Some(mime_type),
        }
    }

    pub(super) fn image_or_summary(data: String, mime_type: &'static str, label: &str) -> Self {
        let limit = max_image_content_bytes();
        if data.len() <= limit {
            return Self::image(data, mime_type);
        }
        Self::text(format!(
            "{label} omitted because it is {} bytes base64, above CODEX_BRIDGE_MAX_IMAGE_BYTES={limit}. Retry with jpeg/webp quality or a narrower element screenshot.",
            data.len()
        ))
    }
}

pub(super) fn bounded_text_for_mcp(text: String) -> String {
    bounded_text(text, max_text_content_bytes())
}

fn max_text_content_bytes() -> usize {
    configured_content_bytes(
        "CODEX_BRIDGE_MAX_TEXT_BYTES",
        DEFAULT_MAX_TEXT_CONTENT_BYTES,
    )
}

fn max_image_content_bytes() -> usize {
    configured_content_bytes(
        "CODEX_BRIDGE_MAX_IMAGE_BYTES",
        DEFAULT_MAX_IMAGE_CONTENT_BYTES,
    )
}

fn configured_content_bytes(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .map(|value| {
            value.clamp(
                MIN_CONFIGURABLE_CONTENT_BYTES,
                MAX_CONFIGURABLE_CONTENT_BYTES,
            )
        })
        .unwrap_or(default)
}

fn bounded_text(mut text: String, max_bytes: usize) -> String {
    let original_bytes = text.len();
    if original_bytes <= max_bytes {
        return text;
    }

    let mut marker = format!(
        "\n\n[truncated by codex-browser-bridge: original_bytes={original_bytes}, max_bytes={max_bytes}]"
    );
    if marker.len() > max_bytes {
        marker = "\n\n[truncated by codex-browser-bridge]".to_string();
    }
    if marker.len() > max_bytes {
        marker.truncate(max_bytes);
    }
    let body_limit = max_bytes.saturating_sub(marker.len());
    let mut cutoff = body_limit.min(text.len());
    while cutoff > 0 && !text.is_char_boundary(cutoff) {
        cutoff -= 1;
    }
    text.truncate(cutoff);
    text.push_str(&marker);
    text
}

// ---- arg extractors ----

pub(super) fn required_str<'a>(args: &'a Value, name: &str) -> anyhow::Result<&'a str> {
    args.get(name)
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing required argument: {name}"))
}

pub(super) fn required_string_value<'a>(args: &'a Value, name: &str) -> anyhow::Result<&'a str> {
    args.get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("missing required argument: {name}"))
}

pub(super) fn required_i64(args: &Value, name: &str) -> anyhow::Result<i64> {
    args.get(name)
        .and_then(Value::as_i64)
        .ok_or_else(|| anyhow::anyhow!("missing required argument: {name}"))
}

pub(super) fn optional_u64(args: &Value, name: &str) -> anyhow::Result<Option<u64>> {
    match args.get(name) {
        Some(value) => value
            .as_u64()
            .map(Some)
            .ok_or_else(|| anyhow::anyhow!("{name} must be a non-negative integer")),
        None => Ok(None),
    }
}

pub(super) fn optional_duration_ms(
    args: &Value,
    name: &str,
    max_ms: u64,
) -> anyhow::Result<Option<u64>> {
    let Some(value) = optional_u64(args, name)? else {
        return Ok(None);
    };
    if value > max_ms {
        return Err(anyhow::anyhow!("{name} must be <= {max_ms} milliseconds"));
    }
    Ok(Some(value))
}

pub(super) fn optional_bool(args: &Value, name: &str) -> anyhow::Result<Option<bool>> {
    match args.get(name) {
        Some(value) => value
            .as_bool()
            .map(Some)
            .ok_or_else(|| anyhow::anyhow!("{name} must be a boolean")),
        None => Ok(None),
    }
}

pub(super) fn optional_str_array(args: &Value, name: &str) -> anyhow::Result<Option<Vec<String>>> {
    let Some(value) = args.get(name) else {
        return Ok(None);
    };
    let values = value
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("{name} must be an array of strings"))?;
    let mut out = Vec::with_capacity(values.len());
    for (index, value) in values.iter().enumerate() {
        let item = value
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("{name}[{index}] must be a string"))?;
        out.push(item.to_string());
    }
    Ok(Some(out))
}

pub(super) fn required_string_vec(args: &Value, name: &str) -> anyhow::Result<Vec<String>> {
    let values = args
        .get(name)
        .and_then(Value::as_array)
        .filter(|values| !values.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing required argument: {name}"))?;
    let mut out = Vec::with_capacity(values.len());
    for (index, value) in values.iter().enumerate() {
        let item = value
            .as_str()
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| anyhow::anyhow!("{name}[{index}] must not be empty"))?;
        out.push(item.to_string());
    }
    Ok(out)
}

pub(super) fn required_str_array(args: &Value, name: &str) -> anyhow::Result<Vec<String>> {
    let values = args
        .get(name)
        .and_then(Value::as_array)
        .filter(|arr| !arr.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing required argument: {name}"))?;
    let mut out = Vec::with_capacity(values.len());
    for (index, value) in values.iter().enumerate() {
        let item = value
            .as_str()
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| anyhow::anyhow!("{name}[{index}] must be a non-empty string"))?;
        out.push(item.to_string());
    }
    Ok(out)
}

// ---- schema helpers ----

pub(super) fn object_schema() -> Value {
    json!({"type":"object","properties":{}})
}

pub(super) fn sanitize_for_log(s: &str) -> String {
    s.replace('\n', "\\n").replace('\r', "\\r")
}

pub(super) fn schema_value(raw: &str) -> Value {
    serde_json::from_str(raw).expect("tool schema is valid JSON")
}

// ---- response builders ----

pub(super) fn result_response(id: Value, result: Value) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "result": result }).to_string()
}

pub(super) fn error_response(id: Option<Value>, code: i64, message: &str) -> String {
    json!({ "jsonrpc": "2.0", "id": id.unwrap_or(Value::Null), "error": { "code": code, "message": message } }).to_string()
}

// ---- test helpers ----

#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn registered_tool_values_for_test() -> Vec<Value> {
    super::schema::tools_to_values(&super::schema::registered_tools())
}

#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn validate_required_str_for_test(args: &Value, name: &str) -> anyhow::Result<()> {
    required_str(args, name).map(|_| ())
}

#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn text_error_envelope_for_test(id: Value, message: &str) -> String {
    result_response(
        id,
        json!({
            "content": [Content::text(format!("Error: {message}"))],
            "isError": true
        }),
    )
}

#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn screenshot_content_for_test(data: String) -> Value {
    serde_json::to_value(vec![
        Content::image_or_summary(data.clone(), "image/png", "Screenshot"),
        Content::text(format!(
            "Screenshot captured for tab 7 ({} bytes base64)",
            data.len()
        )),
    ])
    .expect("content serializes")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn schemas_are_valid_json_schema_objects() {
        let schema = schema_value(
            r#"{"type":"object","properties":{"tab_id":{"type":"string"}},"required":["tab_id"]}"#,
        );
        assert_eq!(schema["type"], "object");
        assert_eq!(schema["required"][0], "tab_id");
    }

    #[test]
    fn required_str_rejects_empty() {
        assert!(required_str(&json!({"x": ""}), "x").is_err());
        assert!(required_str(&json!({"x": "  "}), "x").is_err());
        assert!(required_str(&json!({"x": "ok"}), "x").is_ok());
    }

    #[test]
    fn required_str_array_rejects_missing_and_empty() {
        assert!(required_str_array(&json!({}), "files").is_err());
        assert!(required_str_array(&json!({"files": []}), "files").is_err());
        assert!(required_str_array(&json!({"files": ["a", ""]}), "files").is_err());
        assert_eq!(
            required_str_array(&json!({"files": ["a", "b"]}), "files").unwrap(),
            vec!["a".to_string(), "b".to_string()]
        );
    }

    #[test]
    fn required_string_vec_rejects_empty_items() {
        assert!(required_string_vec(&json!({"keys": ["a", ""]}), "keys").is_err());
        assert!(required_string_vec(&json!({"keys": ["a", "b"]}), "keys").is_ok());
    }

    #[test]
    fn bounded_duration_rejects_extreme_values() {
        assert!(
            optional_duration_ms(&json!({"timeout_ms": u64::MAX}), "timeout_ms", 60_000).is_err()
        );
        assert!(optional_duration_ms(&json!({"timeout_ms": 1.5}), "timeout_ms", 60_000).is_err());
        assert_eq!(
            optional_duration_ms(&json!({"timeout_ms": 1_500}), "timeout_ms", 60_000).unwrap(),
            Some(1_500)
        );
    }

    #[test]
    fn optional_string_arrays_reject_malformed_values() {
        assert_eq!(optional_str_array(&json!({}), "types").unwrap(), None);
        assert_eq!(
            optional_str_array(&json!({"types": ["Image", "Script"]}), "types").unwrap(),
            Some(vec!["Image".to_string(), "Script".to_string()])
        );
        assert_eq!(
            optional_str_array(&json!({"types": []}), "types").unwrap(),
            Some(Vec::new())
        );
        assert!(optional_str_array(&json!({"types": "Image"}), "types").is_err());
        assert!(optional_str_array(&json!({"types": ["Image", 1]}), "types").is_err());
    }

    #[test]
    fn text_content_is_bounded_without_splitting_utf8() {
        let text = format!("{}您好", "a".repeat(DEFAULT_MAX_TEXT_CONTENT_BYTES));
        let content = Content::text(text);
        let value = serde_json::to_value(content).unwrap();
        let bounded = value["text"].as_str().unwrap();

        assert!(bounded.contains("truncated by codex-browser-bridge"));
        assert!(bounded.is_char_boundary(bounded.len()));
        assert!(bounded.len() < DEFAULT_MAX_TEXT_CONTENT_BYTES + 256);
    }

    #[test]
    fn oversized_image_content_becomes_text_summary() {
        let data = "a".repeat(DEFAULT_MAX_IMAGE_CONTENT_BYTES + 1);
        let content = Content::image_or_summary(data, "image/png", "Screenshot");
        let value = serde_json::to_value(content).unwrap();

        assert_eq!(value["type"], "text");
        assert!(value["data"].is_null());
        assert!(value["text"]
            .as_str()
            .unwrap()
            .contains("Screenshot omitted"));
    }

    #[test]
    fn direct_image_constructor_is_bounded() {
        let data = "a".repeat(DEFAULT_MAX_IMAGE_CONTENT_BYTES + 1);
        let content = Content::image(data, "image/png");
        let value = serde_json::to_value(content).unwrap();

        assert_eq!(value["type"], "text");
        assert!(value["text"].as_str().unwrap().contains("Image omitted"));
    }

    #[test]
    fn bounded_text_respects_tiny_limits() {
        let bounded = bounded_text("abcdef".repeat(500), 16);

        assert!(bounded.len() <= 16);
        assert!(bounded.is_char_boundary(bounded.len()));
    }
}
