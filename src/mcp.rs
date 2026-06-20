use serde::{Deserialize, Serialize};
use serde_json::{json, value::RawValue, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::browser;
use crate::client::Client;

#[derive(Clone)]
pub struct Server {
    client: Client,
    tools: Vec<Tool>,
}

#[derive(Clone)]
struct Tool {
    name: &'static str,
    description: &'static str,
    input_schema: Value,
    handler: ToolHandler,
}

#[derive(Clone, Copy)]
enum ToolHandler {
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
}

#[derive(Debug, Deserialize)]
struct RpcRequest {
    jsonrpc: Option<String>,
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Option<Value>,
}

#[derive(Debug, Serialize)]
struct Content {
    #[serde(rename = "type")]
    kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<String>,
    #[serde(rename = "mimeType", skip_serializing_if = "Option::is_none")]
    mime_type: Option<&'static str>,
}

impl Server {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            tools: registered_tools(),
        }
    }

    pub async fn run_stdio(self) -> anyhow::Result<()> {
        let stdin = tokio::io::stdin();
        let mut stdout = tokio::io::stdout();
        let mut lines = BufReader::new(stdin).lines();

        while let Some(line) = lines.next_line().await? {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Some(response) = self.handle_jsonrpc_line(line).await {
                stdout.write_all(response.as_bytes()).await?;
                stdout.write_all(b"\n").await?;
                stdout.flush().await?;
            }
        }

        Ok(())
    }

    pub async fn handle_jsonrpc_line(&self, line: &str) -> Option<String> {
        let value: Value = match serde_json::from_str(line) {
            Ok(value) => value,
            Err(_) => return Some(error_response(None, -32700, "Parse error")),
        };
        if !value.is_object() {
            return Some(error_response(None, -32600, "Invalid Request"));
        }
        let req: RpcRequest = match serde_json::from_value(value) {
            Ok(req) => req,
            Err(_) => return Some(error_response(None, -32600, "Invalid Request")),
        };
        let id = req.id.clone()?;
        if req.jsonrpc.as_deref() != Some("2.0") || req.method.is_empty() {
            return Some(error_response(Some(id), -32600, "Invalid Request"));
        }

        match req.method.as_str() {
            "initialize" => Some(result_response(
                id,
                json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": "codex-browser-bridge", "version": env!("CARGO_PKG_VERSION") }
                }),
            )),
            "tools/list" => Some(result_response(id, json!({ "tools": self.tool_list() }))),
            "tools/call" => Some(self.handle_tool_call(id, req.params).await),
            "ping" => Some(result_response(id, json!({}))),
            "notifications/initialized" => None,
            other => Some(error_response(
                Some(id),
                -32601,
                &format!("Unknown method: {other}"),
            )),
        }
    }

    async fn handle_tool_call(&self, id: Value, params: Option<Value>) -> String {
        let Some(params) = params.and_then(|value| value.as_object().cloned()) else {
            return error_response(Some(id), -32602, "Invalid params");
        };
        let Some(name) = params
            .get("name")
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty())
        else {
            return error_response(Some(id), -32602, "Invalid params");
        };
        let args = params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));
        if !args.is_object() {
            return error_response(Some(id), -32602, "Invalid params");
        }
        let Some(tool) = self.tools.iter().find(|tool| tool.name == name) else {
            return error_response(Some(id), -32601, &format!("Tool not found: {name}"));
        };

        let result = match tool.handler {
            ToolHandler::ListTabs => self.handle_list_tabs().await,
            ToolHandler::CreateTab => self.handle_create_tab().await,
            ToolHandler::CloseTab => self.handle_close_tab(args).await,
            ToolHandler::UserTabs => self.handle_user_tabs().await,
            ToolHandler::ClaimTab => self.handle_claim_tab(args).await,
            ToolHandler::Navigate => self.handle_navigate(args).await,
            ToolHandler::Reload => self.handle_reload(args).await,
            ToolHandler::NavigateBack => self.handle_navigate_back(args).await,
            ToolHandler::NavigateForward => self.handle_navigate_forward(args).await,
            ToolHandler::WaitForLoad => self.handle_wait_for_load(args).await,
            ToolHandler::DomSnapshot => self.handle_dom_snapshot(args).await,
            ToolHandler::Screenshot => self.handle_screenshot(args).await,
            ToolHandler::Click => self.handle_click(args).await,
            ToolHandler::Fill => self.handle_fill(args).await,
            ToolHandler::Evaluate => self.handle_evaluate(args).await,
            ToolHandler::CuaClick => self.handle_cua_click(args).await,
            ToolHandler::CuaType => self.handle_cua_type(args).await,
            ToolHandler::CuaKeypress => self.handle_cua_keypress(args).await,
            ToolHandler::CuaScroll => self.handle_cua_scroll(args).await,
            ToolHandler::DomGetVisible => self.handle_dom_get_visible(args).await,
            ToolHandler::DomClick => self.handle_dom_click(args).await,
            ToolHandler::NameSession => self.handle_name_session(args).await,
            ToolHandler::Finalize => self.handle_finalize().await,
            ToolHandler::GetInfo => self.handle_get_info().await,
            ToolHandler::ExecuteCdp => self.handle_execute_cdp(args).await,
            ToolHandler::PageAssets => self.handle_page_assets(args).await,
            ToolHandler::NetworkCookies => self.handle_network_cookies(args).await,
            ToolHandler::NetworkSetCookie => self.handle_network_set_cookie(args).await,
        };

        match result {
            Ok(content) => result_response(id, json!({ "content": content })),
            Err(err) => result_response(
                id,
                json!({
                    "content": [Content::text(format!("Error: {err}"))],
                    "isError": true
                }),
            ),
        }
    }

    async fn handle_list_tabs(&self) -> anyhow::Result<Vec<Content>> {
        let tabs = browser::list_tabs(&self.client).await?;
        Ok(vec![Content::text(serde_json::to_string_pretty(&tabs)?)])
    }

    async fn handle_create_tab(&self) -> anyhow::Result<Vec<Content>> {
        let id = browser::create_tab(&self.client).await?;
        Ok(vec![Content::text(format!("Created tab: {id}"))])
    }

    async fn handle_close_tab(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        browser::close_tab(&self.client, tab_id).await?;
        Ok(vec![Content::text(format!("Closed tab {tab_id}"))])
    }

    async fn handle_user_tabs(&self) -> anyhow::Result<Vec<Content>> {
        let tabs = browser::list_user_tabs(&self.client).await?;
        Ok(vec![Content::text(serde_json::to_string_pretty(&tabs)?)])
    }

    async fn handle_claim_tab(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let tab = browser::claim_user_tab(&self.client, tab_id).await?;
        Ok(vec![Content::text(serde_json::to_string(&tab)?)])
    }

    async fn handle_navigate(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let url = required_str(&args, "url")?;
        browser::navigate(&self.client, tab_id, url).await?;
        Ok(vec![Content::text(format!(
            "Navigated tab {tab_id} to {url}"
        ))])
    }

    async fn handle_reload(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        browser::reload(&self.client, tab_id).await?;
        Ok(vec![Content::text(format!("Reloaded tab {tab_id}"))])
    }

    async fn handle_navigate_back(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        browser::navigate_back(&self.client, tab_id).await?;
        Ok(vec![Content::text(format!("Navigated tab {tab_id} back"))])
    }

    async fn handle_navigate_forward(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        browser::navigate_forward(&self.client, tab_id).await?;
        Ok(vec![Content::text(format!(
            "Navigated tab {tab_id} forward"
        ))])
    }

    async fn handle_wait_for_load(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let timeout_ms = optional_u64(&args, "timeout_ms")?.unwrap_or(10_000);
        let state = browser::wait_for_load(&self.client, tab_id, timeout_ms).await?;
        Ok(vec![Content::text(format!(
            "Tab {tab_id} reached readyState={state}"
        ))])
    }

    async fn handle_dom_snapshot(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let snapshot = browser::dom_snapshot(&self.client, tab_id).await?;
        Ok(vec![Content::text(snapshot)])
    }

    async fn handle_screenshot(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let full_page = optional_bool(&args, "fullPage")?.unwrap_or(false);
        let data = browser::screenshot(&self.client, tab_id, full_page).await?;
        Ok(vec![
            Content::image(data.clone(), "image/png"),
            Content::text(format!(
                "Screenshot captured for tab {tab_id} ({} bytes base64)",
                data.len()
            )),
        ])
    }

    async fn handle_click(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let selector = required_str(&args, "selector")?;
        browser::click(&self.client, tab_id, selector).await?;
        Ok(vec![Content::text(format!(
            "Clicked {selector} in tab {tab_id}"
        ))])
    }

    async fn handle_fill(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let selector = required_str(&args, "selector")?;
        let value = required_string_value(&args, "value")?;
        browser::fill(&self.client, tab_id, selector, value).await?;
        Ok(vec![Content::text(format!(
            "Filled {selector} in tab {tab_id}"
        ))])
    }

    async fn handle_evaluate(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let expression = required_str(&args, "expression")?;
        let raw: Box<RawValue> = browser::evaluate(&self.client, tab_id, expression).await?;
        Ok(vec![Content::text(raw.get().to_string())])
    }

    async fn handle_cua_click(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let x = required_i64(&args, "x")?;
        let y = required_i64(&args, "y")?;
        browser::cua_click(&self.client, tab_id, x, y).await?;
        Ok(vec![Content::text(format!(
            "CUA click at ({x},{y}) in tab {tab_id}"
        ))])
    }

    async fn handle_cua_type(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let text = required_str(&args, "text")?;
        browser::cua_type(&self.client, tab_id, text).await?;
        Ok(vec![Content::text(format!(
            "CUA typed text in tab {tab_id}"
        ))])
    }

    async fn handle_cua_keypress(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let keys = required_string_vec(&args, "keys")?;
        browser::cua_keypress(&self.client, tab_id, &keys).await?;
        Ok(vec![Content::text(format!(
            "CUA keypress {keys:?} in tab {tab_id}"
        ))])
    }

    async fn handle_cua_scroll(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let x = required_i64(&args, "x")?;
        let y = required_i64(&args, "y")?;
        let scroll_x = required_i64(&args, "scroll_x")?;
        let scroll_y = required_i64(&args, "scroll_y")?;
        browser::cua_scroll(&self.client, tab_id, x, y, scroll_x, scroll_y).await?;
        Ok(vec![Content::text(format!(
            "CUA scroll at ({x},{y}) delta ({scroll_x},{scroll_y}) in tab {tab_id}"
        ))])
    }

    async fn handle_dom_get_visible(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let dom = browser::get_visible_dom(&self.client, tab_id).await?;
        Ok(vec![Content::text(dom)])
    }

    async fn handle_dom_click(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let node_id = required_str(&args, "node_id")?;
        browser::dom_cua_click(&self.client, tab_id, node_id).await?;
        Ok(vec![Content::text(format!(
            "DOM click node {node_id} in tab {tab_id}"
        ))])
    }

    async fn handle_name_session(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let name = required_str(&args, "name")?;
        self.client
            .send_request("nameSession", Some(json!({ "name": name })))
            .await?;
        Ok(vec![Content::text(format!("Session named: {name}"))])
    }

    async fn handle_finalize(&self) -> anyhow::Result<Vec<Content>> {
        self.client
            .send_request("finalizeTabs", Some(json!({ "keep": [] })))
            .await?;
        Ok(vec![Content::text("Tabs finalized".to_string())])
    }

    async fn handle_get_info(&self) -> anyhow::Result<Vec<Content>> {
        let raw = self.client.send_request("getInfo", None).await?;
        Ok(vec![Content::text(raw.get().to_string())])
    }

    async fn handle_execute_cdp(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let method = required_str(&args, "method")?;
        let params = args.get("params").cloned();
        let raw: Box<RawValue> =
            browser::execute_cdp_generic(&self.client, tab_id, method, params).await?;
        Ok(vec![Content::text(raw.get().to_string())])
    }

    async fn handle_page_assets(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let include_content = optional_bool(&args, "include_content")?.unwrap_or(false);
        let types: Option<Vec<String>> = optional_str_array(&args, "types");

        let mut resources = browser::get_resource_tree(&self.client, tab_id).await?;

        if let Some(ref types) = types {
            resources.retain(|resource| {
                types
                    .iter()
                    .any(|type_filter| resource.resource_type.eq_ignore_ascii_case(type_filter))
            });
        }

        if include_content {
            for resource in resources.iter_mut() {
                let frame_id = resource.frame_id.clone();
                match browser::get_resource_content(
                    &self.client,
                    tab_id,
                    &frame_id,
                    &resource.url,
                )
                .await
                {
                    Ok(content) => {
                        resource.content = Some(content);
                    }
                    Err(err) => {
                        resource.failed = Some(true);
                        tracing::debug!(
                            "resource content fetch failed for {} (frame={}): {err}",
                            sanitize_for_log(&resource.url),
                            sanitize_for_log(&frame_id),
                        );
                    }
                }
            }
        }

        Ok(vec![Content::text(
            serde_json::to_string_pretty(&resources)?,
        )])
    }

    async fn handle_network_cookies(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let urls: Option<Vec<String>> = optional_str_array(&args, "urls");
        let redact = optional_bool(&args, "redact_values")?.unwrap_or(true);

        let mut cookies =
            browser::get_cookies(&self.client, tab_id, urls.as_deref()).await?;

        if redact {
            for cookie in cookies.iter_mut() {
                cookie.value = "[redacted]".to_string();
            }
        }

        Ok(vec![Content::text(serde_json::to_string_pretty(&cookies)?)])
    }

    async fn handle_network_set_cookie(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let name = required_str(&args, "name")?;
        let value = required_str(&args, "value")?;

        if let Some(url) = args.get("url").and_then(Value::as_str) {
            browser::validate_url(url)?;
        }

        let mut cookie_params = json!({
            "name": name,
            "value": value,
        });

        if let Some(obj) = cookie_params.as_object_mut() {
            if let Some(url) = args.get("url").and_then(Value::as_str) {
                obj.insert("url".into(), json!(url));
            }
            if let Some(domain) = args.get("domain").and_then(Value::as_str) {
                obj.insert("domain".into(), json!(domain));
            }
            if let Some(path) = args.get("path").and_then(Value::as_str) {
                obj.insert("path".into(), json!(path));
            }
            if let Some(http_only) = args.get("httpOnly").and_then(Value::as_bool) {
                obj.insert("httpOnly".into(), json!(http_only));
            }
            if let Some(secure) = args.get("secure").and_then(Value::as_bool) {
                obj.insert("secure".into(), json!(secure));
            }
            if let Some(same_site) = args.get("sameSite").and_then(Value::as_str) {
                obj.insert("sameSite".into(), json!(same_site));
            }
        }

        browser::set_cookie(&self.client, tab_id, cookie_params).await?;
        Ok(vec![Content::text(format!(
            "Cookie '{name}' set successfully"
        ))])
    }

    pub fn tool_list(&self) -> Vec<Value> {
        tools_to_values(&self.tools)
    }
}

impl Tool {
    fn new(
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
    fn text(text: String) -> Self {
        Self {
            kind: "text",
            text: Some(text),
            data: None,
            mime_type: None,
        }
    }

    fn image(data: String, mime_type: &'static str) -> Self {
        Self {
            kind: "image",
            text: None,
            data: Some(data),
            mime_type: Some(mime_type),
        }
    }
}

fn registered_tools() -> Vec<Tool> {
    vec![
        Tool::new("codex_list_tabs", "List all open browser tabs via Codex Chrome Extension", object_schema(), ToolHandler::ListTabs),
        Tool::new("codex_create_tab", "Create a new browser tab", object_schema(), ToolHandler::CreateTab),
        Tool::new("codex_close_tab", "Close a browser tab", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string","description":"Tab ID to close"}},"required":["tab_id"]}"#), ToolHandler::CloseTab),
        Tool::new("codex_user_tabs", "List user's open tabs across browser windows", object_schema(), ToolHandler::UserTabs),
        Tool::new("codex_claim_tab", "Claim a user tab for automation control", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"}},"required":["tab_id"]}"#), ToolHandler::ClaimTab),
        Tool::new("codex_navigate", "Navigate a tab to a URL", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"url":{"type":"string"}},"required":["tab_id","url"]}"#), ToolHandler::Navigate),
        Tool::new("codex_reload", "Reload a tab", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"}},"required":["tab_id"]}"#), ToolHandler::Reload),
        Tool::new("codex_navigate_back", "Navigate a tab back one entry in its history", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"}},"required":["tab_id"]}"#), ToolHandler::NavigateBack),
        Tool::new("codex_navigate_forward", "Navigate a tab forward one entry in its history", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"}},"required":["tab_id"]}"#), ToolHandler::NavigateForward),
        Tool::new("codex_wait_for_load", "Poll document.readyState until it equals \"complete\" or timeout (ms) elapses. Useful after navigation on slow pages.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"timeout_ms":{"type":"number","description":"Timeout in milliseconds. Defaults to 10000."}},"required":["tab_id"]}"#), ToolHandler::WaitForLoad),
        Tool::new("codex_dom_snapshot", "Get accessibility tree DOM snapshot of a tab", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"}},"required":["tab_id"]}"#), ToolHandler::DomSnapshot),
        Tool::new("codex_screenshot", "Capture a screenshot. fullPage parameter is reserved (not yet implemented - always captures viewport). Returns image content viewable by the agent.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"fullPage":{"type":"boolean"}},"required":["tab_id"]}"#), ToolHandler::Screenshot),
        Tool::new("codex_click", "Click an element via Playwright selector", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"selector":{"type":"string"}},"required":["tab_id","selector"]}"#), ToolHandler::Click),
        Tool::new("codex_fill", "Fill a form input via Playwright selector", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"selector":{"type":"string"},"value":{"type":"string"}},"required":["tab_id","selector","value"]}"#), ToolHandler::Fill),
        Tool::new("codex_evaluate", "Evaluate JavaScript in the page context", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"expression":{"type":"string"}},"required":["tab_id","expression"]}"#), ToolHandler::Evaluate),
        Tool::new("codex_cua_click", "Click at screen coordinates (CUA)", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"x":{"type":"integer"},"y":{"type":"integer"}},"required":["tab_id","x","y"]}"#), ToolHandler::CuaClick),
        Tool::new("codex_cua_type", "Type text at current focus (CUA)", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"text":{"type":"string"}},"required":["tab_id","text"]}"#), ToolHandler::CuaType),
        Tool::new("codex_cua_keypress", "Press keyboard keys (CUA)", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"keys":{"type":"array","items":{"type":"string"}}},"required":["tab_id","keys"]}"#), ToolHandler::CuaKeypress),
        Tool::new("codex_cua_scroll", "Scroll at coordinates (CUA)", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"x":{"type":"integer"},"y":{"type":"integer"},"scroll_x":{"type":"integer"},"scroll_y":{"type":"integer"}},"required":["tab_id","x","y","scroll_x","scroll_y"]}"#), ToolHandler::CuaScroll),
        Tool::new("codex_dom_get_visible", "Get a simplified visible DOM tree (human-readable; use codex_dom_snapshot for node IDs usable with codex_dom_click)", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"}},"required":["tab_id"]}"#), ToolHandler::DomGetVisible),
        Tool::new("codex_dom_click", "Click a DOM node by its accessibility node ID from codex_dom_snapshot", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"node_id":{"type":"string"}},"required":["tab_id","node_id"]}"#), ToolHandler::DomClick),
        Tool::new("codex_name_session", "Name the browser automation session", schema_value(r#"{"type":"object","properties":{"name":{"type":"string"}},"required":["name"]}"#), ToolHandler::NameSession),
        Tool::new("codex_finalize", "Finalize and clean up tabs after session", object_schema(), ToolHandler::Finalize),
        Tool::new("codex_get_info", "Get backend info from the Codex extension", object_schema(), ToolHandler::GetInfo),
        Tool::new("codex_execute_cdp", "Execute a raw Chrome DevTools Protocol command. Pass method (e.g. \"Page.navigate\", \"Runtime.evaluate\", \"Network.getCookies\", \"DOM.getDocument\") and optional params object. Use this to access any CDP domain not covered by dedicated codex_* tools.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string","description":"Tab ID to execute on"},"method":{"type":"string","description":"CDP method name, e.g. Page.captureScreenshot, Network.getCookies, DOM.getDocument"},"params":{"type":"object","description":"CDP method parameters as a JSON object"}},"required":["tab_id","method"]}"#), ToolHandler::ExecuteCdp),
        Tool::new("codex_page_assets", "List all page resources (images, fonts, CSS, JS, etc.) observed by the browser. Optionally fetch resource content as base64. Filter by resource type. Uses the Codex extension's pageAssets capability.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string","description":"Tab ID to inspect"},"include_content":{"type":"boolean","description":"Also fetch each resource's content as base64. Defaults to false."},"types":{"type":"array","items":{"type":"string"},"description":"Filter by resource type. Common values: Image, Stylesheet, Script, Font, Document, Media, Manifest, Fetch, Other."}},"required":["tab_id"]}"#), ToolHandler::PageAssets),
        Tool::new("codex_network_cookies", "Get cookies for the current page URL or specific URLs. Uses CDP Network.getCookies. Cookie values are redacted by default; use redact_values: false to see raw values.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string","description":"Tab ID"},"urls":{"type":"array","items":{"type":"string"},"description":"Optional list of URLs to filter cookies by"},"redact_values":{"type":"boolean","description":"Redact cookie values. Defaults to true for security."}},"required":["tab_id"]}"#), ToolHandler::NetworkCookies),
        Tool::new("codex_network_set_cookie", "Set a browser cookie on the current page. Uses CDP Network.setCookie.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string","description":"Tab ID"},"name":{"type":"string","description":"Cookie name"},"value":{"type":"string","description":"Cookie value"},"url":{"type":"string","description":"URL to associate the cookie with. Optional."},"domain":{"type":"string","description":"Cookie domain. Optional."},"path":{"type":"string","description":"Cookie path. Optional."},"httpOnly":{"type":"boolean","description":"HttpOnly flag. Optional."},"secure":{"type":"boolean","description":"Secure flag. Optional."},"sameSite":{"type":"string","description":"SameSite value: Strict, Lax, or None. Optional."}},"required":["tab_id","name","value"]}"#), ToolHandler::NetworkSetCookie),
    ]
}

fn tools_to_values(tools: &[Tool]) -> Vec<Value> {
    tools
        .iter()
        .map(|tool| {
            json!({
                "name": tool.name,
                "description": tool.description,
                "inputSchema": tool.input_schema
            })
        })
        .collect()
}

#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn registered_tool_values_for_test() -> Vec<Value> {
    tools_to_values(&registered_tools())
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

fn required_str<'a>(args: &'a Value, name: &str) -> anyhow::Result<&'a str> {
    args.get(name)
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing required argument: {name}"))
}

fn required_string_value<'a>(args: &'a Value, name: &str) -> anyhow::Result<&'a str> {
    args.get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("missing required argument: {name}"))
}

fn required_i64(args: &Value, name: &str) -> anyhow::Result<i64> {
    args.get(name)
        .and_then(Value::as_i64)
        .ok_or_else(|| anyhow::anyhow!("missing required argument: {name}"))
}

fn optional_u64(args: &Value, name: &str) -> anyhow::Result<Option<u64>> {
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

fn optional_bool(args: &Value, name: &str) -> anyhow::Result<Option<bool>> {
    match args.get(name) {
        Some(value) => value
            .as_bool()
            .map(Some)
            .ok_or_else(|| anyhow::anyhow!("{name} must be a boolean")),
        None => Ok(None),
    }
}

fn optional_str_array(args: &Value, name: &str) -> Option<Vec<String>> {
    args.get(name)
        .and_then(|value| value.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
}

fn required_string_vec(args: &Value, name: &str) -> anyhow::Result<Vec<String>> {
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

fn object_schema() -> Value {
    json!({"type":"object","properties":{}})
}

fn sanitize_for_log(s: &str) -> String {
    s.replace('\n', "\\n").replace('\r', "\\r")
}

fn schema_value(raw: &str) -> Value {
    serde_json::from_str(raw).expect("tool schema is valid JSON")
}

fn result_response(id: Value, result: Value) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "result": result }).to_string()
}

fn error_response(id: Option<Value>, code: i64, message: &str) -> String {
    json!({ "jsonrpc": "2.0", "id": id.unwrap_or(Value::Null), "error": { "code": code, "message": message } }).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schemas_are_valid_json_schema_objects() {
        let schema = schema_value(
            r#"{"type":"object","properties":{"tab_id":{"type":"string"}},"required":["tab_id"]}"#,
        );
        assert_eq!(schema["type"], "object");
        assert_eq!(schema["required"][0], "tab_id");
    }

    #[test]
    fn registered_tools_keep_go_order() {
        let names: Vec<_> = registered_tools()
            .into_iter()
            .map(|tool| tool.name)
            .collect();
        assert_eq!(names.first(), Some(&"codex_list_tabs"));
        assert_eq!(names.last(), Some(&"codex_network_set_cookie"));
        assert_eq!(names.len(), 28);
    }

    #[test]
    fn execute_cdp_schema_requires_tab_id_and_method() {
        let tools = registered_tools();
        let cdp = tools.iter().find(|t| t.name == "codex_execute_cdp").unwrap();
        assert_eq!(cdp.input_schema["type"], "object");
        let required: Vec<_> = cdp.input_schema["required"].as_array().unwrap().iter().filter_map(|v| v.as_str()).collect();
        assert!(required.contains(&"tab_id"), "required should contain tab_id");
        assert!(required.contains(&"method"), "required should contain method");
    }

    #[test]
    fn page_assets_schema_requires_tab_id() {
        let tools = registered_tools();
        let pa = tools.iter().find(|t| t.name == "codex_page_assets").unwrap();
        assert_eq!(pa.input_schema["type"], "object");
        let required = pa.input_schema["required"].as_array().unwrap();
        assert_eq!(required[0], "tab_id");
    }

    #[test]
    fn network_cookies_schema_requires_tab_id() {
        let tools = registered_tools();
        let nc = tools.iter().find(|t| t.name == "codex_network_cookies").unwrap();
        assert_eq!(nc.input_schema["type"], "object");
        let required = nc.input_schema["required"].as_array().unwrap();
        assert_eq!(required[0], "tab_id");
    }

    #[test]
    fn network_set_cookie_schema_requires_name_value() {
        let tools = registered_tools();
        let nsc = tools.iter().find(|t| t.name == "codex_network_set_cookie").unwrap();
        assert_eq!(nsc.input_schema["type"], "object");
        let required: Vec<_> = nsc.input_schema["required"].as_array().unwrap().iter().filter_map(|v| v.as_str()).collect();
        assert!(required.contains(&"tab_id"));
        assert!(required.contains(&"name"));
        assert!(required.contains(&"value"));
    }

    #[test]
    fn required_str_rejects_empty() {
        assert!(required_str(&json!({"x": ""}), "x").is_err());
        assert!(required_str(&json!({"x": "  "}), "x").is_err());
        assert!(required_str(&json!({"x": "ok"}), "x").is_ok());
    }

    #[test]
    fn required_string_vec_rejects_empty_items() {
        assert!(required_string_vec(&json!({"keys": ["a", ""]}), "keys").is_err());
        assert!(required_string_vec(&json!({"keys": ["a", "b"]}), "keys").is_ok());
    }

    #[test]
    fn all_tool_schemas_are_valid() {
        for tool in registered_tools() {
            assert_eq!(tool.input_schema["type"], "object",
                "tool {} schema must be object type", tool.name);
        }
    }
}
