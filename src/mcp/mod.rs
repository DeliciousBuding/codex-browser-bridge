use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::client::Client;

use self::profiles::ToolProfile;
use self::schema::{registered_tools, tools_to_values};
use self::types::{bounded_text_for_mcp, error_response, result_response, RpcRequest, Tool};

pub mod handlers;
pub mod profiles;
pub mod schema;
pub mod types;

const MAX_MCP_LINE_BYTES: usize = 1024 * 1024;

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
        let tools = all
            .into_iter()
            .filter(|t| profile.includes(t.name))
            .collect();
        Self {
            client,
            tools,
            profile,
        }
    }

    pub fn new_with_profile(client: Client, profile: ToolProfile) -> Self {
        let all = registered_tools();
        let tools = all
            .into_iter()
            .filter(|t| profile.includes(t.name))
            .collect();
        Self {
            client,
            tools,
            profile,
        }
    }

    pub async fn run_stdio(self) -> anyhow::Result<()> {
        let stdin = tokio::io::stdin();
        let mut stdout = tokio::io::stdout();
        let mut reader = BufReader::new(stdin);
        // Reusable read buffer to avoid per-line String allocations
        let mut buf = Vec::with_capacity(8192);

        loop {
            match read_mcp_line(&mut reader, &mut buf).await {
                Ok(0) => break, // EOF
                Ok(_) => {}
                Err(err) if err.kind() == std::io::ErrorKind::InvalidData => {
                    let response = error_response(None, -32600, "Invalid Request: line too large");
                    stdout.write_all(response.as_bytes()).await?;
                    stdout.write_all(b"\n").await?;
                    stdout.flush().await?;
                    break;
                }
                Err(err) => return Err(err.into()),
            }
            let line = match mcp_line_from_utf8(&buf) {
                Ok(line) => line,
                Err(()) => {
                    let response = error_response(None, -32700, "Parse error");
                    stdout.write_all(response.as_bytes()).await?;
                    stdout.write_all(b"\n").await?;
                    stdout.flush().await?;
                    continue;
                }
            };
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
                let text = bounded_text_for_mcp(
                    serde_json::to_string(&tabs).unwrap_or_else(|_| "[]".into()),
                );
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

async fn read_mcp_line<R>(reader: &mut R, buf: &mut Vec<u8>) -> std::io::Result<usize>
where
    R: tokio::io::AsyncBufRead + Unpin,
{
    buf.clear();
    loop {
        let available = reader.fill_buf().await?;
        if available.is_empty() {
            return Ok(if buf.is_empty() { 0 } else { buf.len() });
        }

        if let Some(pos) = available.iter().position(|byte| *byte == b'\n') {
            let take = pos + 1;
            validate_mcp_line_len_after(buf.len(), take)?;
            buf.extend_from_slice(&available[..take]);
            reader.consume(take);
            return Ok(buf.len());
        }

        let take = available.len();
        validate_mcp_line_len_after(buf.len(), take)?;
        buf.extend_from_slice(available);
        reader.consume(take);
    }
}

#[cfg(test)]
fn validate_mcp_line_len(bytes: &[u8]) -> std::result::Result<(), ()> {
    if bytes.len() > MAX_MCP_LINE_BYTES {
        Err(())
    } else {
        Ok(())
    }
}

fn validate_mcp_line_len_after(current: usize, additional: usize) -> std::io::Result<()> {
    match current.checked_add(additional) {
        Some(total) if total <= MAX_MCP_LINE_BYTES => Ok(()),
        _ => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "MCP line exceeds maximum size",
        )),
    }
}

fn mcp_line_from_utf8(buf: &[u8]) -> std::result::Result<&str, ()> {
    std::str::from_utf8(buf).map(str::trim).map_err(|_| ())
}

#[cfg(all(test, not(windows)))]
mod resources_prompts_tests {
    use super::*;
    use crate::client::Client;
    use crate::protocol;
    use tokio::io::{duplex, DuplexStream};

    fn test_server() -> Server {
        test_server_with_pipe().0
    }

    fn test_server_with_pipe() -> (Server, DuplexStream) {
        let (client_end, server_end) = duplex(2 * 1024 * 1024);
        (
            Server::new(Client::from_stream(client_end).unwrap()),
            server_end,
        )
    }

    async fn next_request(server: &mut DuplexStream) -> Value {
        let frame = protocol::decode_frame(server).await.unwrap();
        serde_json::from_slice(&frame).unwrap()
    }

    async fn reply_result(server: &mut DuplexStream, request: &Value, result: Value) {
        protocol::encode_frame(server, &json!({"id": request["id"], "result": result}))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn resources_read_tabs_text_is_bounded() {
        let (server, mut pipe) = test_server_with_pipe();
        let read_task = tokio::spawn(async move {
            server
                .handle_resources_read(json!(1), Some(json!({"uri": "codex://tabs"})))
                .await
        });

        let request = next_request(&mut pipe).await;
        assert_eq!(request["method"], "getTabs");
        reply_result(
            &mut pipe,
            &request,
            json!([{"id":1,"title":"x".repeat(2_000_000),"url":"https://example.com"}]),
        )
        .await;

        let response = read_task.await.unwrap();
        let v: Value = serde_json::from_str(&response).unwrap();
        let text = v["result"]["contents"][0]["text"].as_str().unwrap();

        assert!(text.contains("truncated by codex-browser-bridge"));
        assert!(text.len() < 1_050_000);
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
        let text = v["result"]["messages"][0]["content"]["text"]
            .as_str()
            .unwrap();
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

#[cfg(test)]
mod stdio_tests {
    use super::*;

    #[test]
    fn mcp_line_from_utf8_rejects_invalid_input() {
        assert!(mcp_line_from_utf8(b"{\"jsonrpc\":\"2.0\"}\n").is_ok());
        assert!(mcp_line_from_utf8(&[0xff, b'\n']).is_err());
    }
}

#[cfg(all(test, not(windows)))]
mod tools_call_tests {
    use super::*;
    use crate::client::Client;
    use serde_json::json;
    use tokio::io::duplex;

    fn test_server(profile: ToolProfile) -> Server {
        let (client_end, _server) = duplex(4096);
        Server::new_with_profile(Client::from_stream(client_end).unwrap(), profile)
    }

    async fn call(server: &Server, request: Value) -> Value {
        let response = server
            .handle_jsonrpc_line(&request.to_string())
            .await
            .expect("request has an id");
        serde_json::from_str(&response).expect("response is valid JSON")
    }

    #[tokio::test]
    async fn tools_call_rejects_invalid_params_shape() {
        let server = test_server(ToolProfile::Full);
        let response = call(
            &server,
            json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":[]}),
        )
        .await;

        assert_eq!(response["error"]["code"], -32602);
    }

    #[tokio::test]
    async fn tools_call_rejects_non_object_arguments() {
        let server = test_server(ToolProfile::Full);
        let response = call(
            &server,
            json!({
                "jsonrpc":"2.0",
                "id":1,
                "method":"tools/call",
                "params":{"name":"codex_find_element","arguments":[]}
            }),
        )
        .await;

        assert_eq!(response["error"]["code"], -32602);
    }

    #[tokio::test]
    async fn tools_call_reports_unknown_or_profile_filtered_tools() {
        let full = test_server(ToolProfile::Full);
        let unknown = call(
            &full,
            json!({
                "jsonrpc":"2.0",
                "id":1,
                "method":"tools/call",
                "params":{"name":"codex_nope","arguments":{}}
            }),
        )
        .await;
        assert_eq!(unknown["error"]["code"], -32601);

        let basic = test_server(ToolProfile::Basic);
        let filtered = call(
            &basic,
            json!({
                "jsonrpc":"2.0",
                "id":2,
                "method":"tools/call",
                "params":{"name":"codex_execute_cdp","arguments":{"tab_id":"1","method":"Runtime.evaluate"}}
            }),
        )
        .await;
        assert_eq!(filtered["error"]["code"], -32601);
    }

    #[tokio::test]
    async fn handler_validation_errors_return_mcp_tool_error() {
        let server = test_server(ToolProfile::Full);
        let response = call(
            &server,
            json!({
                "jsonrpc":"2.0",
                "id":1,
                "method":"tools/call",
                "params":{"name":"codex_find_element","arguments":{"tab_id":"1"}}
            }),
        )
        .await;

        assert_eq!(response["result"]["isError"], true);
        assert!(response["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("At least one of 'role' or 'name' is required"));
    }

    #[tokio::test]
    async fn handler_rejects_fractional_integers_and_malformed_string_arrays() {
        let server = test_server(ToolProfile::Full);
        let fractional = call(
            &server,
            json!({
                "jsonrpc":"2.0",
                "id":1,
                "method":"tools/call",
                "params":{
                    "name":"codex_find_element",
                    "arguments":{"tab_id":"1","role":"button","max_results":1.5}
                }
            }),
        )
        .await;
        assert_eq!(fractional["result"]["isError"], true);
        assert!(fractional["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("max_results must be a non-negative integer"));

        let malformed_array = call(
            &server,
            json!({
                "jsonrpc":"2.0",
                "id":2,
                "method":"tools/call",
                "params":{
                    "name":"codex_page_assets",
                    "arguments":{"tab_id":"1","types":["Image",1]}
                }
            }),
        )
        .await;
        assert_eq!(malformed_array["result"]["isError"], true);
        assert!(malformed_array["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("types[1] must be a string"));
    }

    #[tokio::test]
    async fn cookie_tools_reject_non_http_urls_before_cdp() {
        let server = test_server(ToolProfile::Full);

        let read = call(
            &server,
            json!({
                "jsonrpc":"2.0",
                "id":1,
                "method":"tools/call",
                "params":{
                    "name":"codex_network_cookies",
                    "arguments":{"tab_id":"1","urls":["file:///C:/secret"]}
                }
            }),
        )
        .await;
        assert_eq!(read["result"]["isError"], true);
        assert!(read["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("only http:// and https:// are allowed"));

        let delete = call(
            &server,
            json!({
                "jsonrpc":"2.0",
                "id":2,
                "method":"tools/call",
                "params":{
                    "name":"codex_delete_cookies",
                    "arguments":{"tab_id":"1","name":"sid","url":"chrome://settings"}
                }
            }),
        )
        .await;
        assert_eq!(delete["result"]["isError"], true);
        assert!(delete["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("only http:// and https:// are allowed"));
    }
}

#[cfg(test)]
mod stdio_limit_tests {
    use super::*;

    #[test]
    fn line_too_long_is_rejected_before_json_parse() {
        let line = " ".repeat(MAX_MCP_LINE_BYTES + 1);
        assert_eq!(validate_mcp_line_len(line.as_bytes()), Err(()));
    }
}
