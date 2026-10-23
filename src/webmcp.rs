//! Experimental, top-document-only native WebMCP adapter.
//!
//! The native API contract follows the bundled browser client's page adapter:
//! `document.modelContext.getTools()`, toolchange, tool.window/origin, and
//! executeTool(tool, input [, { signal }]). No browser policy is reimplemented.
use std::collections::VecDeque;

use serde_json::{json, Value};
use tokio::sync::Mutex;
use tokio::time::{timeout, Duration};
use uuid::Uuid;

use crate::browser::parse_tab_id;
use crate::client::Client;
use crate::error::{BridgeError, Result};

const MAX_TOOLS: usize = 100;
const MAX_DESCRIPTOR_BYTES: usize = 64 * 1024;
const MAX_INPUT_BYTES: usize = 64 * 1024;
const MAX_TABS: usize = 16;
const MAX_HANDLES: usize = 512;
const MAX_TIMEOUT_MS: u64 = 30_000;
const STALE: &str = "WebMCP registration is stale or unknown; list tools again before invoking";

#[derive(Clone)]
struct Snapshot {
    tab: i64,
    epoch: Value,
    generation: String,
    handles: Vec<String>,
}

/// Bounded, process-local snapshots. Page-provided names/schemas are data, not
/// host tool registrations. Keep one registry per Server/client ownership scope.
#[derive(Default)]
pub struct WebMcpRegistry {
    snapshots: Mutex<VecDeque<Snapshot>>,
}

impl WebMcpRegistry {
    /// Drop all document-bound handles (used at turn/session cleanup).
    pub async fn clear(&self) {
        self.snapshots.lock().await.clear();
    }

    /// Fetch only the selected tab's current top-document, same-origin tools.
    /// A fresh list supersedes previous handles for that tab.
    pub async fn list(&self, client: &Client, tab_id: &str, timeout_ms: u64) -> Result<Value> {
        let budget = budget(timeout_ms)?;
        let tab = parse_tab_id("webmcp", tab_id)?;
        timeout(budget, async {
            let mut snapshots = self.snapshots.lock().await;
            snapshots.retain(|snapshot| snapshot.tab != tab);
            let epoch = document_epoch(client, tab, budget).await?;
            let generation = Uuid::new_v4().to_string();
            let params = json!({"generation":generation,"epoch":epoch,"maxTools":MAX_TOOLS,"maxBytes":MAX_DESCRIPTOR_BYTES});
            let expression = format!("({LIST_SCRIPT})({params})");
            let result = evaluate_once(client, tab, expression, budget).await?;
            if document_epoch(client, tab, budget).await? != epoch {
                return Err(stale());
            }
            let tools = result.get("tools").and_then(Value::as_array)
                .ok_or_else(|| BridgeError::Protocol("invalid WebMCP tool listing".into()))?;
            if tools.len() > MAX_TOOLS || serde_json::to_vec(tools).map_err(protocol)?.len() > MAX_DESCRIPTOR_BYTES {
                return Err(BridgeError::User("WebMCP descriptor limits exceeded".into()));
            }
            let mut handles = Vec::with_capacity(tools.len());
            for tool in tools {
                let handle = tool.get("handle").and_then(Value::as_str)
                    .filter(|handle| !handle.is_empty() && handle.len() <= 128)
                    .ok_or_else(|| BridgeError::Protocol("invalid WebMCP registration handle".into()))?;
                if handles.iter().any(|current| current == handle) {
                    return Err(BridgeError::Protocol("duplicate WebMCP registration handle".into()));
                }
                handles.push(handle.to_owned());
            }
            while snapshots.len() >= MAX_TABS || snapshots.iter().map(|s| s.handles.len()).sum::<usize>() + handles.len() > MAX_HANDLES {
                snapshots.pop_front();
            }
            snapshots.push_back(Snapshot { tab, epoch, generation, handles });
            Ok(json!({"experimental":true,"scope":"top-document-same-origin","tools":tools}))
        }).await.map_err(|_| BridgeError::Timeout("WebMCP list".into()))?
    }

    /// Invoke an exact handle returned by list, never an arbitrary tool name.
    /// On any failed/ambiguous attempt all handles for the snapshot retire.
    /// Mutation is sent once directly, bypassing sticky CDP re-attach retries.
    pub async fn invoke(
        &self,
        client: &Client,
        handle: &str,
        input: Value,
        timeout_ms: u64,
    ) -> Result<Value> {
        let budget = budget(timeout_ms)?;
        if serde_json::to_vec(&input).map_err(protocol)?.len() > MAX_INPUT_BYTES {
            return Err(BridgeError::User("WebMCP input exceeds 64 KiB".into()));
        }
        timeout(budget, async {
            let mut snapshots = self.snapshots.lock().await;
            let index = snapshots.iter().position(|s| s.handles.iter().any(|h| h == handle)).ok_or_else(stale)?;
            // Remove before awaiting: cancellation/timeout cannot leave an
            // ambiguous mutation's handle available for an accidental retry.
            let snapshot = snapshots.remove(index).expect("located snapshot");
            if document_epoch(client, snapshot.tab, budget).await? != snapshot.epoch {
                return Err(stale());
            }
            let params = json!({"handle":handle,"generation":snapshot.generation,"input":input,"timeoutMs":budget.as_millis() as u64});
            let expression = format!("({INVOKE_SCRIPT})({params})");
            let result = evaluate_once(client, snapshot.tab, expression, budget).await?;
            snapshots.push_back(snapshot);
            Ok(result)
        }).await.map_err(|_| BridgeError::Timeout("WebMCP invoke (outcome may be unknown; not replayed)".into()))?
    }
}

fn stale() -> BridgeError {
    BridgeError::User(STALE.into())
}
fn protocol(error: serde_json::Error) -> BridgeError {
    BridgeError::Protocol(error.to_string())
}
fn budget(value: u64) -> Result<Duration> {
    let value = if value == 0 { 10_000 } else { value };
    if value > MAX_TIMEOUT_MS {
        return Err(BridgeError::User(
            "WebMCP timeout_ms must be <= 30000".into(),
        ));
    }
    Ok(Duration::from_millis(value))
}

async fn document_epoch(client: &Client, tab: i64, budget: Duration) -> Result<Value> {
    // This read-only call may attach/re-attach. The invocation itself never
    // goes through execute_cdp's retry path.
    let raw = client
        .execute_cdp_with_timeout(tab, "Page.getFrameTree", None, budget)
        .await?;
    let value: Value = serde_json::from_str(raw.get()).map_err(protocol)?;
    let frame = value
        .pointer("/frameTree/frame")
        .ok_or_else(|| BridgeError::Protocol("missing WebMCP top document frame".into()))?;
    for field in ["id", "loaderId", "url"] {
        if frame
            .get(field)
            .and_then(Value::as_str)
            .is_none_or(str::is_empty)
        {
            return Err(BridgeError::Protocol(format!(
                "missing WebMCP document {field}"
            )));
        }
    }
    Ok(json!({"id":frame["id"],"loaderId":frame["loaderId"],"url":frame["url"]}))
}

async fn evaluate_once(
    client: &Client,
    tab: i64,
    expression: String,
    budget: Duration,
) -> Result<Value> {
    let raw = client.send_request_with_timeout("executeCdp", Some(json!({
        "target":{"tabId":tab},"method":"Runtime.evaluate",
        "commandParams":{"expression":expression,"awaitPromise":true,"returnByValue":true,"userGesture":false,"timeout":budget.as_millis() as u64}
    })), budget).await?;
    let value: Value = serde_json::from_str(raw.get()).map_err(protocol)?;
    if let Some(error) = value.get("error") {
        return Err(BridgeError::Cdp {
            method: "Runtime.evaluate".into(),
            code: error["code"].as_i64().unwrap_or(-1),
            message: error["message"]
                .as_str()
                .unwrap_or("WebMCP CDP failed")
                .chars()
                .take(512)
                .collect(),
        });
    }
    if let Some(exception) = value.get("exceptionDetails") {
        let message = exception
            .pointer("/exception/description")
            .or_else(|| exception.get("text"))
            .and_then(Value::as_str)
            .unwrap_or("WebMCP page evaluation failed");
        return Err(BridgeError::User(message.chars().take(512).collect()));
    }
    value.pointer("/result/value").cloned().ok_or_else(|| {
        BridgeError::Protocol("WebMCP evaluation returned no serializable value".into())
    })
}

const LIST_SCRIPT: &str = r#"async function(p) {
  const key = '__codexBridgeWebMcpV1', ctx = document.modelContext;
  if (!ctx || typeof ctx.getTools !== 'function') return {tools:[]};
  if (globalThis.origin === 'null' || !/^https?:$/.test(location.protocol)) throw new Error('WebMCP requires a non-opaque HTTP(S) document');
  if (typeof ctx.addEventListener !== 'function') throw new Error('Unsupported WebMCP: registration change events unavailable');
  if (location.href !== p.epoch.url) throw new Error('WebMCP document changed; list again');
  const previous = globalThis[key];
  if (previous) previous.invalidate();
  const state = {document, ctx, url:location.href, origin:globalThis.origin, generation:p.generation, handles:new Map(), valid:true};
  state.invalidate = () => { state.valid = false; state.handles.clear(); if (typeof ctx.removeEventListener === 'function') ctx.removeEventListener('toolchange', state.invalidate); removeEventListener('pagehide', state.invalidate); };
  ctx.addEventListener('toolchange', state.invalidate, {once:true});
  addEventListener('pagehide', state.invalidate, {once:true});
  globalThis[key] = state;
  try {
  const tools = (await ctx.getTools()).filter(t => t.window === window && t.origin === state.origin);
  if (!state.valid || document !== state.document || location.href !== state.url || globalThis[key] !== state) throw new Error('WebMCP registration changed; list again');
  if (tools.length > p.maxTools) throw new Error('WebMCP tool count limit exceeded');
  const descriptor = t => JSON.stringify([t.name,t.origin,t.title,t.description,typeof t.inputSchema === 'string' ? JSON.parse(t.inputSchema) : t.inputSchema ?? null,t.annotations]);
  const names = new Set();
  const result = tools.map(t => {
    if (typeof t.name !== 'string' || !t.name || names.has(t.name)) throw new Error('Invalid or duplicate WebMCP tool name');
    names.add(t.name);
    const handle = crypto.randomUUID();
    state.handles.set(handle,{tool:t,descriptor:descriptor(t)});
    return {handle,name:t.name,title:t.title,description:t.description,inputSchema:typeof t.inputSchema === 'string' ? JSON.parse(t.inputSchema) : t.inputSchema ?? null,annotations:t.annotations,origin:t.origin};
  });
  if (new TextEncoder().encode(JSON.stringify(result)).length > p.maxBytes) { state.invalidate(); throw new Error('WebMCP descriptor byte limit exceeded'); }
  return {tools:result};
  } catch(error) { state.invalidate(); throw error; }
}"#;

const INVOKE_SCRIPT: &str = r#"async function(p) {
  const state = globalThis.__codexBridgeWebMcpV1, ctx = document.modelContext;
  const stale = () => { throw new Error('WebMCP registration is stale; list tools again'); };
  if (!state || !state.valid || state.generation !== p.generation || state.document !== document || state.ctx !== ctx || state.url !== location.href || state.origin !== globalThis.origin) stale();
  const entry = state.handles.get(p.handle);
  if (!entry || typeof ctx.executeTool !== 'function') stale();
  const descriptor = t => JSON.stringify([t.name,t.origin,t.title,t.description,typeof t.inputSchema === 'string' ? JSON.parse(t.inputSchema) : t.inputSchema ?? null,t.annotations]);
  const tools = await ctx.getTools();
  if (!state.valid || globalThis.__codexBridgeWebMcpV1 !== state || state.document !== document || state.url !== location.href || !tools.some(t => t.window === window && t.window === entry.tool.window && t.name === entry.tool.name && t.origin === state.origin && descriptor(t) === entry.descriptor)) stale();
  const arity = ctx.executeTool.length;
  if (arity !== 1 && arity !== 2) throw new Error('Unsupported native WebMCP executeTool signature');
  return await ctx.executeTool(entry.tool, arity === 2 ? JSON.stringify(p.input) : p.input, {signal:AbortSignal.timeout(p.timeoutMs)});
}"#;
