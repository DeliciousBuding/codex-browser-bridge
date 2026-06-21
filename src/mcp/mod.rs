use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::client::Client;

use self::types::{error_response, result_response, RpcRequest, Tool};
use self::schema::{registered_tools, tools_to_values};
use self::profiles::ToolProfile;

pub mod types;
pub mod schema;
pub mod handlers;
pub mod profiles;

#[derive(Clone)]
pub struct Server {
    pub(crate) client: Client,
    pub(crate) tools: Vec<Tool>,
    #[allow(dead_code)]
    profile: ToolProfile,
}

impl Server {
    pub fn new(client: Client) -> Self {
        let profile = ToolProfile::from_env();
        let all = registered_tools();
        let tools = all.into_iter().filter(|t| profile.includes(t.name)).collect();
        Self { client, tools, profile }
    }

    pub fn new_with_profile(client: Client, profile: ToolProfile) -> Self {
        let all = registered_tools();
        let tools = all.into_iter().filter(|t| profile.includes(t.name)).collect();
        Self { client, tools, profile }
    }

    pub async fn run_stdio(self) -> anyhow::Result<()> {
        let stdin = tokio::io::stdin();
        let mut stdout = tokio::io::stdout();
        let mut reader = BufReader::new(stdin);
        // Reusable read buffer to avoid per-line String allocations
        let mut buf = Vec::with_capacity(8192);

        loop {
            buf.clear();
            match reader.read_until(b'\n', &mut buf).await {
                Ok(0) => break, // EOF
                Ok(_) => {}
                Err(err) => return Err(err.into()),
            }
            let line = std::str::from_utf8(&buf).unwrap_or("").trim();
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
                    "capabilities": {
                        "tools": {},
                        "resources": {},
                        "prompts": {}
                    },
                    "serverInfo": { "name": "codex-browser-bridge", "version": env!("CARGO_PKG_VERSION") }
                }),
            )),
            "tools/list" => Some(result_response(id, json!({ "tools": self.tool_list() }))),
            "tools/call" => Some(self.handle_tool_call(id, req.params).await),
            "resources/list" => Some(self.handle_resources_list(id)),
            "resources/read" => Some(self.handle_resources_read(id, req.params).await),
            "prompts/list" => Some(self.handle_prompts_list(id)),
            "prompts/get" => Some(self.handle_prompts_get(id, req.params).await),
            "ping" => Some(result_response(id, json!({}))),
            "notifications/initialized" => None,
            other => Some(error_response(
                Some(id),
                -32601,
                &format!("Unknown method: {other}"),
            )),
        }
    }

    pub fn tool_list(&self) -> Vec<Value> {
        tools_to_values(&self.tools)
    }

    // ── MCP resources (agent-readable state) ──

    fn handle_resources_list(&self, id: Value) -> String {
        result_response(
            id,
            json!({
                "resources": [{
                    "uri": "codex://tabs",
                    "name": "tabs",
                    "description": "Currently open browser tabs (read-only snapshot via getTabs)",
                    "mimeType": "application/json"
                }]
            }),
        )
    }

    async fn handle_resources_read(&self, id: Value, params: Option<Value>) -> String {
        let uri = params
            .as_ref()
            .and_then(|p| p.get("uri"))
            .and_then(|u| u.as_str())
            .unwrap_or("");
        if uri != "codex://tabs" {
            return error_response(Some(id), -32602, &format!("unknown resource uri: {uri}"));
        }
        match crate::browser::list_tabs(&self.client).await {
            Ok(tabs) => {
                let text = serde_json::to_string(&tabs).unwrap_or_else(|_| "[]".into());
                result_response(
                    id,
                    json!({
                        "contents": [{
                            "uri": "codex://tabs",
                            "mimeType": "application/json",
                            "text": text
                        }]
                    }),
                )
            }
            Err(err) => error_response(Some(id), -32603, &format!("read tabs failed: {err}")),
        }
    }

    // ── MCP prompts (reusable workflow templates) ──

    fn handle_prompts_list(&self, id: Value) -> String {
        result_response(
            id,
            json!({
                "prompts": [
                    {
                        "name": "login",
                        "description": "Guide logging into a website: navigate, read form, fill, submit, verify.",
                        "arguments": [
                            {"name": "url", "description": "Login page URL", "required": true}
                        ]
                    },
                    {
                        "name": "extract-table",
                        "description": "Extract a <table> on the current page into structured rows.",
                        "arguments": []
                    }
                ]
            }),
        )
    }

    async fn handle_prompts_get(&self, id: Value, params: Option<Value>) -> String {
        let name = params
            .as_ref()
            .and_then(|p| p.get("name"))
            .and_then(|n| n.as_str())
            .unwrap_or("")
            .to_string();
        let args = params
            .and_then(|p| p.get("arguments").cloned())
            .unwrap_or_else(|| json!({}));
        let (description, text) = match name.as_str() {
            "login" => {
                let url = args.get("url").and_then(|u| u.as_str()).unwrap_or("<login url>");
                (
                    "Login workflow",
                    format!(
                        "Log into the site at {url}. Suggested steps:\n\
                         1. codex_nav_and_wait to {url}\n\
                         2. codex_dom_snapshot to read the form fields and their selectors\n\
                         3. codex_form_fill with username + password selectors\n\
                         4. codex_click_and_wait on the submit button\n\
                         5. codex_get_url and codex_screenshot to verify the login landed\n\
                         Never hardcode credentials — ask the user if they are not provided."
                    ),
                )
            }
            "extract-table" => (
                "Table extraction workflow",
                "Extract a table on the current page. Suggested steps:\n\
                 1. codex_dom_snapshot to confirm a <table> is present\n\
                 2. codex_evaluate with a querySelectorAll('table tr') script that maps each row to an object of header to cell\n\
                 3. Return the rows as a JSON array of objects."
                    .to_string(),
            ),
            other => {
                return error_response(Some(id), -32602, &format!("unknown prompt: {other}"));
            }
        };
        result_response(
            id,
            json!({
                "description": description,
                "messages": [{
                    "role": "user",
                    "content": {"type": "text", "text": text}
                }]
            }),
        )
    }
}

#[cfg(all(test, not(windows)))]
mod resources_prompts_tests {
    use super::*;
    use crate::client::Client;
    use tokio::io::duplex;

    fn test_server() -> Server {
        let (client_end, _server) = duplex(4096);
        Server::new(Client::from_stream(client_end).unwrap())
    }

    #[tokio::test]
    async fn resources_list_advertises_tabs() {
        let server = test_server();
        let resp = server.handle_resources_list(json!(1));
        let v: Value = serde_json::from_str(&resp).unwrap();
        let uris: Vec<&str> = v["result"]["resources"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["uri"].as_str().unwrap())
            .collect();
        assert!(uris.contains(&"codex://tabs"));
    }

    #[tokio::test]
    async fn resources_read_unknown_uri_returns_error() {
        let server = test_server();
        let resp = server
            .handle_resources_read(json!(1), Some(json!({"uri": "codex://bogus"})))
            .await;
        let v: Value = serde_json::from_str(&resp).unwrap();
        assert_eq!(v["error"]["code"], -32602);
    }

    #[tokio::test]
    async fn prompts_list_advertises_login_and_extract_table() {
        let server = test_server();
        let resp = server.handle_prompts_list(json!(1));
        let v: Value = serde_json::from_str(&resp).unwrap();
        let names: Vec<&str> = v["result"]["prompts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"login"));
        assert!(names.contains(&"extract-table"));
    }

    #[tokio::test]
    async fn prompts_get_login_includes_url_and_steps() {
        let server = test_server();
        let resp = server
            .handle_prompts_get(
                json!(1),
                Some(json!({"name": "login", "arguments": {"url": "https://example.com/login"}})),
            )
            .await;
        let v: Value = serde_json::from_str(&resp).unwrap();
        let text = v["result"]["messages"][0]["content"]["text"].as_str().unwrap();
        assert!(text.contains("https://example.com/login"));
        assert!(text.contains("codex_form_fill"));
    }

    #[tokio::test]
    async fn prompts_get_unknown_returns_error() {
        let server = test_server();
        let resp = server
            .handle_prompts_get(json!(1), Some(json!({"name": "nope"})))
            .await;
        let v: Value = serde_json::from_str(&resp).unwrap();
        assert_eq!(v["error"]["code"], -32602);
    }
}
