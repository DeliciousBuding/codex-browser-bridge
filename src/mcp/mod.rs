use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::client::Client;

use self::profiles::ToolProfile;
use self::schema::{registered_tools, tools_to_values};
use self::types::{
    bounded_text_for_mcp, content_limits, error_response, result_response, RpcRequest, Tool,
};

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

    pub(crate) fn bridge_runtime_info(&self) -> Value {
        Self::runtime_info_for(
            self.profile,
            self.tools.len(),
            std::env::var_os("CODEX_BRIDGE_UPLOAD_BASE").is_some(),
        )
    }

    fn runtime_info_for(
        profile: ToolProfile,
        tool_count: usize,
        upload_base_configured: bool,
    ) -> Value {
        let limits = content_limits();
        json!({
            "bridgeVersion": env!("CARGO_PKG_VERSION"),
            "profile": profile.as_str(),
            "toolCount": tool_count,
            "uploadBaseConfigured": upload_base_configured,
            "responseLimits": {
                "maxTextBytes": limits.max_text_bytes,
                "maxImageBytes": limits.max_image_bytes,
                "maxConfigurableBytes": 8 * 1024 * 1024
            }
        })
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
    use tokio::time::{timeout, Duration};

    const PIPE_REQUEST_TIMEOUT: Duration = Duration::from_secs(2);

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
        let frame = timeout(PIPE_REQUEST_TIMEOUT, protocol::decode_frame(server))
            .await
            .expect("timed out waiting for extension request")
            .unwrap();
        serde_json::from_slice(&frame).unwrap()
    }

    async fn expect_task<T>(task: tokio::task::JoinHandle<T>, reason: &str) -> T {
        timeout(PIPE_REQUEST_TIMEOUT, task)
            .await
            .unwrap_or_else(|_| panic!("{reason}: task timed out"))
            .unwrap_or_else(|err| panic!("{reason}: task panicked: {err}"))
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

        let response = expect_task(read_task, "resources/read tabs response").await;
        let v: Value = serde_json::from_str(&response).unwrap();
        let text = v["result"]["contents"][0]["text"].as_str().unwrap();

        assert!(text.contains("truncated by codex-browser-bridge"));
        assert!(text.len() < 1_050_000);
    }

    #[tokio::test]
    async fn lazy_server_answers_metadata_without_browser_pipe() {
        let server = Server::new(Client::lazy(None));

        let initialize = server
            .handle_jsonrpc_line(r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#)
            .await
            .unwrap();
        let initialize: Value = serde_json::from_str(&initialize).unwrap();
        assert_eq!(
            initialize["result"]["serverInfo"]["name"],
            "codex-browser-bridge"
        );

        let tools = server
            .handle_jsonrpc_line(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#)
            .await
            .unwrap();
        let tools: Value = serde_json::from_str(&tools).unwrap();
        assert!(tools["result"]["tools"].as_array().unwrap().len() >= 30);
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

#[cfg(test)]
mod runtime_info_tests {
    use super::*;

    #[test]
    fn bridge_runtime_info_omits_raw_upload_path() {
        let info = Server::runtime_info_for(ToolProfile::Basic, 34, true);

        assert_eq!(info["profile"], "basic");
        assert_eq!(info["toolCount"], 34);
        assert_eq!(info["uploadBaseConfigured"], true);
        assert!(info.get("uploadBase").is_none());
        assert!(info["responseLimits"]["maxImageBytes"].as_u64().unwrap() > 0);
    }
}

#[cfg(all(test, not(windows)))]
mod tools_call_tests {
    use super::*;
    use crate::client::Client;
    use crate::protocol;
    use serde_json::json;
    use tokio::io::{duplex, DuplexStream};
    use tokio::time::{timeout, Duration};

    const PIPE_REQUEST_TIMEOUT: Duration = Duration::from_secs(2);
    const NO_REQUEST_GRACE: Duration = Duration::from_millis(50);

    fn test_server(profile: ToolProfile) -> Server {
        let (client_end, _server) = duplex(4096);
        Server::new_with_profile(Client::from_stream(client_end).unwrap(), profile)
    }

    fn test_server_with_pipe(profile: ToolProfile) -> (Server, DuplexStream) {
        let (client_end, server_end) = duplex(4096);
        (
            Server::new_with_profile(Client::from_stream(client_end).unwrap(), profile),
            server_end,
        )
    }

    async fn next_request(server: &mut DuplexStream) -> Value {
        let frame = timeout(PIPE_REQUEST_TIMEOUT, protocol::decode_frame(server))
            .await
            .expect("timed out waiting for extension request")
            .unwrap();
        serde_json::from_slice(&frame).unwrap()
    }

    async fn no_request(server: &mut DuplexStream, reason: &str) {
        if let Ok(request) = timeout(NO_REQUEST_GRACE, next_request(server)).await {
            panic!("{reason}: unexpected pipe request {request}");
        }
    }

    async fn expect_task<T>(task: tokio::task::JoinHandle<T>, reason: &str) -> T {
        timeout(PIPE_REQUEST_TIMEOUT, task)
            .await
            .unwrap_or_else(|_| panic!("{reason}: task timed out"))
            .unwrap_or_else(|err| panic!("{reason}: task panicked: {err}"))
    }

    async fn reply_result(server: &mut DuplexStream, request: &Value, result: Value) {
        protocol::encode_frame(server, &json!({"id": request["id"], "result": result}))
            .await
            .unwrap();
    }

    async fn complete_attach_sequence(server: &mut DuplexStream, tab_id: i64) {
        let detach = next_request(server).await;
        assert_eq!(detach["method"], "detach");
        assert_eq!(detach["params"]["tabId"], tab_id);
        reply_result(server, &detach, json!({})).await;

        let attach = next_request(server).await;
        assert_eq!(attach["method"], "attach");
        assert_eq!(attach["params"]["tabId"], tab_id);
        reply_result(server, &attach, json!({})).await;
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
    async fn get_info_returns_bridge_and_extension_metadata() {
        let (server, mut pipe) = test_server_with_pipe(ToolProfile::Network);
        let call_task = tokio::spawn(async move {
            call(
                &server,
                json!({
                    "jsonrpc":"2.0",
                    "id":1,
                    "method":"tools/call",
                    "params":{"name":"codex_get_info","arguments":{}}
                }),
            )
            .await
        });

        let request = next_request(&mut pipe).await;
        assert_eq!(request["method"], "getInfo");
        reply_result(
            &mut pipe,
            &request,
            json!({"browserVersion":"Chrome/126","extensionId":"abc"}),
        )
        .await;

        let response = expect_task(call_task, "get_info metadata response").await;
        let text = response["result"]["content"][0]["text"].as_str().unwrap();
        let info: Value = serde_json::from_str(text).unwrap();

        assert_eq!(info["bridge"]["profile"], "network");
        assert_eq!(info["bridge"]["bridgeVersion"], env!("CARGO_PKG_VERSION"));
        assert!(info["bridge"]["toolCount"].as_u64().unwrap() > 0);
        assert!(
            info["bridge"]["responseLimits"]["maxTextBytes"]
                .as_u64()
                .unwrap()
                > 0
        );
        assert_eq!(info["browserVersion"], "Chrome/126");
        assert_eq!(info["extensionId"], "abc");
    }

    #[tokio::test]
    async fn get_info_preserves_extension_bridge_field() {
        let (server, mut pipe) = test_server_with_pipe(ToolProfile::Network);
        let call_task = tokio::spawn(async move {
            call(
                &server,
                json!({
                    "jsonrpc":"2.0",
                    "id":1,
                    "method":"tools/call",
                    "params":{"name":"codex_get_info","arguments":{}}
                }),
            )
            .await
        });

        let request = next_request(&mut pipe).await;
        reply_result(
            &mut pipe,
            &request,
            json!({"bridge":{"extensionOwned":true}}),
        )
        .await;

        let response = expect_task(call_task, "get_info bridge-field response").await;
        let text = response["result"]["content"][0]["text"].as_str().unwrap();
        let info: Value = serde_json::from_str(text).unwrap();

        assert_eq!(info["bridge"]["extensionOwned"], true);
        assert_eq!(info["codexBridge"]["profile"], "network");
    }

    #[tokio::test]
    async fn page_assets_skips_unknown_size_content_fetches() {
        let (server, mut pipe) = test_server_with_pipe(ToolProfile::Full);
        let call_task = tokio::spawn(async move {
            call(
                &server,
                json!({
                    "jsonrpc":"2.0",
                    "id":1,
                    "method":"tools/call",
                    "params":{
                        "name":"codex_page_assets",
                        "arguments":{"tab_id":"7","include_content":true}
                    }
                }),
            )
            .await
        });

        complete_attach_sequence(&mut pipe, 7).await;

        let request = next_request(&mut pipe).await;
        assert_eq!(request["params"]["method"], "Page.getResourceTree");
        reply_result(
            &mut pipe,
            &request,
            json!({
                "frame": {"id":"frame-1","url":"https://example.com"},
                "resources": [{
                    "url":"https://example.com/app.js",
                    "type":"Script",
                    "mimeType":"application/javascript"
                }]
            }),
        )
        .await;

        no_request(
            &mut pipe,
            "unknown-size resources should not request Page.getResourceContent",
        )
        .await;

        let response = expect_task(call_task, "page assets unknown-size response").await;
        let text = response["result"]["content"][0]["text"].as_str().unwrap();
        let body: Value = serde_json::from_str(text).unwrap();
        assert_eq!(body["truncated"], true);
        assert_eq!(body["limit_reason"], "unknown_resource_size");
        assert_eq!(body["resources"][0]["failed"], true);
    }

    #[tokio::test]
    async fn page_assets_marks_resource_failed_when_postfetch_total_limit_trips() {
        let (server, mut pipe) = test_server_with_pipe(ToolProfile::Full);
        let call_task = tokio::spawn(async move {
            call(
                &server,
                json!({
                    "jsonrpc":"2.0",
                    "id":1,
                    "method":"tools/call",
                    "params":{
                        "name":"codex_page_assets",
                        "arguments":{"tab_id":"7","include_content":true,"max_total_bytes":4}
                    }
                }),
            )
            .await
        });

        complete_attach_sequence(&mut pipe, 7).await;

        let tree = next_request(&mut pipe).await;
        assert_eq!(tree["params"]["method"], "Page.getResourceTree");
        reply_result(
            &mut pipe,
            &tree,
            json!({
                "frame": {"id":"frame-1","url":"https://example.com"},
                "resources": [{
                    "url":"https://example.com/app.js",
                    "type":"Script",
                    "mimeType":"application/javascript",
                    "contentSize": 4
                }]
            }),
        )
        .await;

        let content = next_request(&mut pipe).await;
        assert_eq!(content["params"]["method"], "Page.getResourceContent");
        reply_result(
            &mut pipe,
            &content,
            json!({
                "content": "too-large",
                "base64Encoded": false
            }),
        )
        .await;

        let response = expect_task(call_task, "page assets postfetch total limit").await;
        let text = response["result"]["content"][0]["text"].as_str().unwrap();
        let body: Value = serde_json::from_str(text).unwrap();
        assert_eq!(body["truncated"], true);
        assert_eq!(body["limit_reason"], "max_total_bytes");
        assert_eq!(body["resources"][0]["failed"], true);
        assert!(body["resources"][0].get("content").is_none());
    }

    #[tokio::test]
    async fn file_input_requires_explicit_upload_base_before_pipe_use() {
        if std::env::var_os("CODEX_BRIDGE_UPLOAD_BASE").is_some() {
            eprintln!("skipping upload-base absence test because env is set");
            return;
        }

        let (server, mut pipe) = test_server_with_pipe(ToolProfile::Full);
        let call_task = tokio::spawn(async move {
            call(
                &server,
                json!({
                    "jsonrpc":"2.0",
                    "id":1,
                    "method":"tools/call",
                    "params":{
                        "name":"codex_file_input",
                        "arguments":{"tab_id":"7","selector":"#file","files":["C:/tmp/a.txt"]}
                    }
                }),
            )
            .await
        });

        no_request(&mut pipe, "validation should fail before pipe use").await;
        let response = expect_task(call_task, "file input upload-base validation").await;
        assert_eq!(response["result"]["isError"], true);
        assert!(response["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("CODEX_BRIDGE_UPLOAD_BASE must be set"));
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
                    "params":{"name":"codex_execute_cdp","arguments":{"tab_id":"1","method":"DOM.getDocument"}}
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

    #[tokio::test]
    async fn cookie_tools_reject_malformed_cookie_fields_before_cdp() {
        let (server, mut pipe) = test_server_with_pipe(ToolProfile::Full);

        let set_cookie = call(
            &server,
            json!({
                "jsonrpc":"2.0",
                "id":1,
                "method":"tools/call",
                "params":{
                    "name":"codex_network_set_cookie",
                    "arguments":{
                        "tab_id":"1",
                        "name":"bad;name",
                        "value":"ok",
                        "domain":"example.com",
                        "path":"/",
                        "sameSite":"Lax"
                    }
                }
            }),
        )
        .await;
        assert_eq!(set_cookie["result"]["isError"], true);
        assert!(set_cookie["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("malformed cookie name"));
        no_request(&mut pipe, "invalid cookie name should fail before pipe use").await;

        let set_same_site = call(
            &server,
            json!({
                "jsonrpc":"2.0",
                "id":2,
                "method":"tools/call",
                "params":{
                    "name":"codex_network_set_cookie",
                    "arguments":{
                        "tab_id":"1",
                        "name":"sid",
                        "value":"ok",
                        "domain":"example.com",
                        "path":"/",
                        "sameSite":"lax"
                    }
                }
            }),
        )
        .await;
        assert_eq!(set_same_site["result"]["isError"], true);
        assert!(set_same_site["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("malformed cookie sameSite"));
        no_request(&mut pipe, "invalid sameSite should fail before pipe use").await;

        let delete = call(
            &server,
            json!({
                "jsonrpc":"2.0",
                "id":3,
                "method":"tools/call",
                "params":{
                    "name":"codex_delete_cookies",
                    "arguments":{"tab_id":"1","name":"sid","domain":"https://example.com","path":"/"}
                }
            }),
        )
        .await;
        assert_eq!(delete["result"]["isError"], true);
        assert!(delete["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("malformed cookie domain"));
        no_request(
            &mut pipe,
            "invalid cookie domain should fail before pipe use",
        )
        .await;
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
