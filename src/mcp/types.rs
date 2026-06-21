use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

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
        Self {
            kind: "text",
            text: Some(text),
            data: None,
            mime_type: None,
        }
    }

    pub(super) fn image(data: String, mime_type: &'static str) -> Self {
        Self {
            kind: "image",
            text: None,
            data: Some(data),
            mime_type: Some(mime_type),
        }
    }
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
            .or_else(|| {
                value
                    .as_f64()
                    .filter(|value| *value >= 0.0)
                    .map(|value| value as u64)
            })
            .map(Some)
            .ok_or_else(|| anyhow::anyhow!("{name} must be a number")),
        None => Ok(None),
    }
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

pub(super) fn optional_str_array(args: &Value, name: &str) -> Option<Vec<String>> {
    args.get(name)
        .and_then(|value| value.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
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
        Content::image(data.clone(), "image/png"),
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
}
