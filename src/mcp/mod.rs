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

    pub fn tool_list(&self) -> Vec<Value> {
        tools_to_values(&self.tools)
    }
}
