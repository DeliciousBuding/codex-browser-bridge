use serde::{Deserialize, Serialize};
use serde_json::{json, value::RawValue, Value};
use std::io::{self, Write};
use tokio::time::{sleep, Duration, Instant};

use crate::client::Client;
use crate::error::{BridgeError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tab {
    #[serde(skip)]
    pub id: String,
    #[serde(rename = "id")]
    pub raw_id: Value,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserTab {
    #[serde(skip)]
    pub id: String,
    #[serde(rename = "id")]
    pub raw_id: Value,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub title: String,
    #[serde(default, rename = "lastOpened")]
    pub last_opened: String,
    #[serde(default, rename = "tabGroup")]
    pub tab_group: String,
}

impl Tab {
    pub fn normalize(mut self) -> Self {
        self.id = match &self.raw_id {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            other => other.to_string(),
        };
        self
    }
}

impl UserTab {
    pub fn normalize(mut self) -> Self {
        self.id = normalize_id_value(&self.raw_id);
        self
    }
}

pub async fn list_tabs(client: &Client) -> Result<Vec<Tab>> {
    let raw = client.send_request("getTabs", None).await?;
    decode_tabs(&raw)
}

pub async fn create_tab(client: &Client) -> Result<String> {
    let raw = client.send_request("createTab", None).await?;
    let tab: Tab =
        serde_json::from_str(raw.get()).map_err(|err| BridgeError::Protocol(err.to_string()))?;
    Ok(tab.normalize().id)
}

pub async fn list_user_tabs(client: &Client) -> Result<Vec<UserTab>> {
    let raw = client.send_request("getUserTabs", None).await?;
    decode_user_tabs(&raw)
}

pub async fn claim_user_tab(client: &Client, tab_id: &str) -> Result<Tab> {
    let id = parse_tab_id("claimUserTab", tab_id)?;
    let raw = client
        .send_request("claimUserTab", Some(json!({ "tabId": id })))
        .await?;
    let tab: Tab = serde_json::from_str(raw.get())
        .map_err(|err| BridgeError::Protocol(format!("decode claimUserTab: {err}")))?;
    let _ = client
        .send_request("attach", Some(json!({ "tabId": id })))
        .await;
    Ok(tab.normalize())
}

pub async fn close_tab(client: &Client, tab_id: &str) -> Result<()> {
    let id = parse_tab_id("close_tab", tab_id)?;
    match client.execute_cdp(id, "Page.close", None).await {
        Ok(_) => Ok(()),
        Err(err) if is_tab_gone_error(&err) => Ok(()),
        Err(err) => Err(err),
    }
}

pub async fn navigate(client: &Client, tab_id: &str, url: &str) -> Result<()> {
    validate_url(url)?;
    let id = parse_tab_id("navigate", tab_id)?;
    client
        .execute_cdp(id, "Page.navigate", Some(json!({ "url": url })))
        .await
        .map(|_| ())
}

pub async fn navigate_back(client: &Client, tab_id: &str) -> Result<()> {
    let id = parse_tab_id("navigate_back", tab_id)?;
    let raw = client
        .execute_cdp(id, "Page.getNavigationHistory", None)
        .await?;
    let entry_id = history_entry_id(&raw, HistoryDirection::Back)?;
    client
        .execute_cdp(
            id,
            "Page.navigateToHistoryEntry",
            Some(json!({ "entryId": entry_id })),
        )
        .await
        .map(|_| ())
}

pub async fn navigate_forward(client: &Client, tab_id: &str) -> Result<()> {
    let id = parse_tab_id("navigate_forward", tab_id)?;
    let raw = client
        .execute_cdp(id, "Page.getNavigationHistory", None)
        .await?;
    let entry_id = history_entry_id(&raw, HistoryDirection::Forward)?;
    client
        .execute_cdp(
            id,
            "Page.navigateToHistoryEntry",
            Some(json!({ "entryId": entry_id })),
        )
        .await
        .map(|_| ())
}

pub async fn reload(client: &Client, tab_id: &str) -> Result<()> {
    let id = parse_tab_id("reload", tab_id)?;
    client
        .execute_cdp(id, "Page.reload", None)
        .await
        .map(|_| ())
}

pub async fn wait_for_load(client: &Client, tab_id: &str, timeout_ms: u64) -> Result<String> {
    let id = parse_tab_id("wait_for_load", tab_id)?;
    let timeout_ms = if timeout_ms == 0 { 10_000 } else { timeout_ms };
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    let mut last = String::new();

    loop {
        if Instant::now() >= deadline {
            return Err(BridgeError::Timeout(format!(
                "readyState=complete after {timeout_ms}ms (last={last:?})"
            )));
        }

        match client
            .execute_cdp(
                id,
                "Runtime.evaluate",
                Some(json!({
                    "expression": "document.readyState",
                    "returnByValue": true
                })),
            )
            .await
        {
            Ok(raw) => {
                if let Some(state) = runtime_value_string(&raw)? {
                    last = state;
                    if last == "complete" {
                        return Ok(last);
                    }
                }
            }
            Err(err) if is_transient_load_error(&err) && Instant::now() < deadline => {}
            Err(err) => return Err(err),
        }

        sleep_until(deadline, Duration::from_millis(100)).await;
    }
}

pub async fn dom_snapshot(client: &Client, tab_id: &str) -> Result<String> {
    let id = parse_tab_id("snapshot", tab_id)?;
    match client
        .execute_cdp(id, "Accessibility.getFullAXTree", None)
        .await
    {
        Ok(raw) => Ok(raw.get().to_string()),
        Err(primary) => {
            let fallback = client
                .execute_cdp(
                    id,
                    "Runtime.evaluate",
                    Some(json!({
                        "expression": "document.body ? document.body.innerText : document.documentElement.innerText",
                        "returnByValue": true
                    })),
                )
                .await
                .map_err(|err| {
                    BridgeError::Protocol(format!("dom_snapshot failed: {primary} (fallback: {err})"))
                })?;
            let text =
                runtime_value_string(&fallback)?.unwrap_or_else(|| fallback.get().to_string());
            Ok(format!("/* fallback: plain text */\n{text}"))
        }
    }
}

pub async fn screenshot(client: &Client, tab_id: &str, full_page: bool) -> Result<String> {
    let _ = full_page;
    let id = parse_tab_id("screenshot", tab_id)?;
    let raw = client
        .execute_cdp(
            id,
            "Page.captureScreenshot",
            Some(json!({ "format": "png" })),
        )
        .await?;
    screenshot_data(&raw)
}

pub async fn evaluate(client: &Client, tab_id: &str, expression: &str) -> Result<Box<RawValue>> {
    let id = parse_tab_id("evaluate", tab_id)?;
    client
        .execute_cdp(
            id,
            "Runtime.evaluate",
            Some(json!({ "expression": expression, "returnByValue": true })),
        )
        .await
}

pub async fn click(client: &Client, tab_id: &str, selector: &str) -> Result<()> {
    let id = parse_tab_id("click", tab_id)?;
    let raw = client
        .execute_cdp(
            id,
            "Runtime.evaluate",
            Some(json!({
                "expression": build_click_script(selector),
                "returnByValue": true
            })),
        )
        .await?;
    parse_action_result(&raw, "click")
}

pub async fn fill(client: &Client, tab_id: &str, selector: &str, value: &str) -> Result<()> {
    let id = parse_tab_id("fill", tab_id)?;
    let raw = client
        .execute_cdp(
            id,
            "Runtime.evaluate",
            Some(json!({
                "expression": build_fill_script(selector, value),
                "returnByValue": true
            })),
        )
        .await?;
    parse_action_result(&raw, "fill")
}

pub async fn cua_click(client: &Client, tab_id: &str, x: i64, y: i64) -> Result<()> {
    let id = parse_tab_id("cua_click", tab_id)?;
    for event_type in ["mousePressed", "mouseReleased"] {
        client
            .execute_cdp(
                id,
                "Input.dispatchMouseEvent",
                Some(json!({
                    "type": event_type,
                    "x": x,
                    "y": y,
                    "button": "left",
                    "clickCount": 1
                })),
            )
            .await?;
    }
    Ok(())
}

pub async fn cua_type(client: &Client, tab_id: &str, text: &str) -> Result<()> {
    let id = parse_tab_id("cua_type", tab_id)?;
    if text.is_empty() {
        return Ok(());
    }
    client
        .execute_cdp(id, "Input.insertText", Some(json!({ "text": text })))
        .await
        .map(|_| ())
}

pub async fn cua_keypress(client: &Client, tab_id: &str, keys: &[String]) -> Result<()> {
    let id = parse_tab_id("cua_keypress", tab_id)?;
    for key in keys {
        for event_type in ["keyDown", "keyUp"] {
            client
                .execute_cdp(
                    id,
                    "Input.dispatchKeyEvent",
                    Some(json!({ "type": event_type, "key": key })),
                )
                .await?;
        }
    }
    Ok(())
}

pub async fn cua_scroll(
    client: &Client,
    tab_id: &str,
    x: i64,
    y: i64,
    scroll_x: i64,
    scroll_y: i64,
) -> Result<()> {
    let id = parse_tab_id("cua_scroll", tab_id)?;
    client
        .execute_cdp(
            id,
            "Input.dispatchMouseEvent",
            Some(json!({
                "type": "mouseWheel",
                "x": x,
                "y": y,
                "deltaX": scroll_x as f64,
                "deltaY": scroll_y as f64
            })),
        )
        .await
        .map(|_| ())
}

pub async fn get_visible_dom(client: &Client, tab_id: &str) -> Result<String> {
    let id = parse_tab_id("get_visible_dom", tab_id)?;
    let raw = client
        .execute_cdp(
            id,
            "Runtime.evaluate",
            Some(json!({
                "expression": VISIBLE_DOM_SCRIPT,
                "returnByValue": true
            })),
        )
        .await?;
    Ok(runtime_value_string(&raw)?.unwrap_or_else(|| raw.get().to_string()))
}

pub async fn dom_cua_click(client: &Client, tab_id: &str, node_id: &str) -> Result<()> {
    let id = parse_tab_id("dom_cua_click", tab_id)?;
    let node_id = parse_node_id(node_id)?;
    client
        .execute_cdp(
            id,
            "DOM.resolveNode",
            Some(json!({ "backendNodeId": node_id })),
        )
        .await?;
    let raw = client
        .execute_cdp(
            id,
            "DOM.getBoxModel",
            Some(json!({ "backendNodeId": node_id })),
        )
        .await?;
    let (x, y) = box_model_center(&raw)?;
    cua_click(client, tab_id, x as i64, y as i64).await
}

pub async fn dom_cua_type(client: &Client, tab_id: &str, text: &str) -> Result<()> {
    cua_type(client, tab_id, text).await
}

// ── Generic CDP ──────────────────────────────────────────────

/// CDP domains blocked from codex_execute_cdp for security.
const BLOCKED_CDP_DOMAINS: &[&str] = &[
    "Browser.",
    "Debugger.",
    "Profiler.",
    "Emulation.",
    "Security.",
    "Target.",
    "Tracing.",
    "Page.addScriptToEvaluateOnNewDocument",
    "Page.setDownloadBehavior",
    "Page.setWebLifecycleState",
    "Network.setRequestInterception",
    "Network.continueInterceptedRequest",
    "Storage.clearDataForOrigin",
];

/// Execute any CDP method with arbitrary params. The universal CDP escape hatch.
/// Blocks dangerous CDP domains (Browser, Debugger, Target, etc.) for security.
pub async fn execute_cdp_generic(
    client: &Client,
    tab_id: &str,
    method: &str,
    params: Option<Value>,
) -> Result<Box<RawValue>> {
    for blocked in BLOCKED_CDP_DOMAINS {
        if method.starts_with(blocked) || method == blocked.trim_end_matches('.') {
            return Err(BridgeError::User(format!(
                "blocked CDP method: {method} ({}is not allowed for security)",
                if blocked.ends_with('.') { "domain " } else { "" }
            )));
        }
    }
    let id = parse_tab_id("execute_cdp", tab_id)?;
    client.execute_cdp(id, method, params).await
}

// ── Page Assets (extension capability: pageAssets) ────────────

#[derive(Debug, Serialize)]
pub struct PageResource {
    pub url: String,
    #[serde(rename = "type")]
    pub resource_type: String,
    #[serde(rename = "mimeType")]
    pub mime_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failed: Option<bool>,
    #[serde(skip_serializing)]
    pub frame_id: String,
}

pub async fn get_resource_tree(client: &Client, tab_id: &str) -> Result<Vec<PageResource>> {
    let id = parse_tab_id("page_assets", tab_id)?;
    let raw = client
        .execute_cdp(id, "Page.getResourceTree", None)
        .await?;
    parse_resource_tree(&raw)
}

pub async fn get_resource_content(
    client: &Client,
    tab_id: &str,
    frame_id: &str,
    url: &str,
) -> Result<String> {
    let id = parse_tab_id("page_assets", tab_id)?;
    let raw = client
        .execute_cdp(
            id,
            "Page.getResourceContent",
            Some(json!({ "frameId": frame_id, "url": url })),
        )
        .await?;
    extract_resource_content(&raw)
}

// ── Network Cookies ───────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct Cookie {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires: Option<f64>,
    #[serde(rename = "httpOnly")]
    pub http_only: bool,
    pub secure: bool,
    #[serde(rename = "sameSite", skip_serializing_if = "Option::is_none")]
    pub same_site: Option<String>,
}

pub async fn get_cookies(
    client: &Client,
    tab_id: &str,
    urls: Option<&[String]>,
) -> Result<Vec<Cookie>> {
    let id = parse_tab_id("network_cookies", tab_id)?;
    let params = if let Some(url_list) = urls.filter(|list| !list.is_empty()) {
        json!({ "urls": url_list })
    } else {
        json!({})
    };
    let raw = client
        .execute_cdp(id, "Network.getCookies", Some(params))
        .await?;
    parse_cookies(&raw)
}

pub async fn set_cookie(
    client: &Client,
    tab_id: &str,
    params: Value,
) -> Result<()> {
    let id = parse_tab_id("network_set_cookie", tab_id)?;
    client
        .execute_cdp(id, "Network.setCookie", Some(params))
        .await
        .map(|_| ())
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct FrameTree {
    frame: Frame,
    #[serde(default)]
    #[serde(rename = "childFrames")]
    child_frames: Vec<FrameTree>,
    #[serde(default)]
    resources: Vec<ResourceEntry>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct Frame {
    id: String,
    url: String,
}

#[derive(Deserialize)]
struct ResourceEntry {
    url: String,
    #[serde(rename = "type")]
    resource_type: String,
    #[serde(rename = "mimeType")]
    mime_type: String,
    #[serde(default, rename = "contentSize")]
    content_size: Option<f64>,
}

fn parse_resource_tree(raw: &RawValue) -> Result<Vec<PageResource>> {
    let tree: FrameTree = serde_json::from_str(raw.get())
        .map_err(|err| BridgeError::Protocol(format!("parse resource tree: {err}")))?;

    let mut resources = Vec::new();
    collect_resources(&tree, &mut resources);
    Ok(resources)
}

fn collect_resources(tree: &FrameTree, out: &mut Vec<PageResource>) {
    let frame_id = tree.frame.id.clone();
    for r in &tree.resources {
        out.push(PageResource {
            url: r.url.clone(),
            resource_type: r.resource_type.clone(),
            mime_type: r.mime_type.clone(),
            content: None,
            size: r.content_size.map(|size| size as u64),
            failed: None,
            frame_id: frame_id.clone(),
        });
    }
    for child in &tree.child_frames {
        collect_resources(child, out);
    }
}

fn extract_resource_content(raw: &RawValue) -> Result<String> {
    #[derive(Deserialize)]
    struct ResourceContent {
        #[serde(default)]
        content: String,
    }

    let result: ResourceContent = serde_json::from_str(raw.get())
        .map_err(|err| BridgeError::Protocol(format!("parse resource content: {err}")))?;

    if result.content.is_empty() {
        return Err(BridgeError::Protocol("resource content is empty".into()));
    }
    Ok(result.content)
}

fn parse_cookies(raw: &RawValue) -> Result<Vec<Cookie>> {
    #[derive(Deserialize)]
    struct CookiesResult {
        cookies: Vec<Cookie>,
    }

    let result: CookiesResult = serde_json::from_str(raw.get())
        .map_err(|err| BridgeError::Protocol(format!("parse cookies: {err}")))?;

    Ok(result.cookies)
}

pub fn decode_tabs(raw: &RawValue) -> Result<Vec<Tab>> {
    let tabs: Vec<Tab> =
        serde_json::from_str(raw.get()).map_err(|err| BridgeError::Protocol(err.to_string()))?;
    Ok(tabs.into_iter().map(Tab::normalize).collect())
}

pub fn decode_user_tabs(raw: &RawValue) -> Result<Vec<UserTab>> {
    #[derive(Deserialize)]
    struct Wrapped {
        tabs: Option<Vec<UserTab>>,
    }

    if let Ok(result) = serde_json::from_str::<Wrapped>(raw.get()) {
        if let Some(tabs) = result.tabs {
            return Ok(tabs.into_iter().map(UserTab::normalize).collect());
        }
    }

    let tabs: Vec<UserTab> = serde_json::from_str(raw.get())
        .map_err(|err| BridgeError::Protocol(format!("decode getUserTabs: {err}")))?;
    Ok(tabs.into_iter().map(UserTab::normalize).collect())
}

pub fn parse_tab_id(action: &str, tab_id: &str) -> Result<i64> {
    tab_id
        .parse::<i64>()
        .map_err(|_| BridgeError::User(format!("{action} requires numeric tab_id, got {tab_id:?}")))
}

pub fn validate_url(raw_url: &str) -> Result<()> {
    let lower = raw_url.trim().to_ascii_lowercase();
    for scheme in [
        "file:",
        "javascript:",
        "data:",
        "vbscript:",
        "about:",
        "chrome:",
        "edge:",
    ] {
        if lower.starts_with(scheme) {
            return Err(BridgeError::User(format!("blocked URL scheme {scheme:?}")));
        }
    }
    Ok(())
}

fn normalize_id_value(raw_id: &Value) -> String {
    match raw_id {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        other => other.to_string(),
    }
}

#[derive(Clone, Copy)]
enum HistoryDirection {
    Back,
    Forward,
}

fn history_entry_id(raw: &RawValue, direction: HistoryDirection) -> Result<i64> {
    #[derive(Deserialize)]
    struct History {
        #[serde(rename = "currentIndex")]
        current_index: isize,
        entries: Vec<HistoryEntry>,
    }

    #[derive(Deserialize)]
    struct HistoryEntry {
        id: i64,
    }

    let history: History = serde_json::from_str(raw.get())
        .map_err(|err| BridgeError::Protocol(format!("decode navigation history: {err}")))?;
    match direction {
        HistoryDirection::Back => {
            if history.current_index <= 0 || history.current_index as usize >= history.entries.len()
            {
                return Err(BridgeError::User("no previous page in history".into()));
            }
            Ok(history.entries[history.current_index as usize - 1].id)
        }
        HistoryDirection::Forward => {
            if history.current_index < 0
                || history.current_index as usize >= history.entries.len().saturating_sub(1)
            {
                return Err(BridgeError::User("no next page in history".into()));
            }
            Ok(history.entries[history.current_index as usize + 1].id)
        }
    }
}

fn runtime_value_string(raw: &RawValue) -> Result<Option<String>> {
    #[derive(Deserialize)]
    struct EvalResult {
        result: Option<EvalValue>,
    }

    #[derive(Deserialize)]
    struct EvalValue {
        value: Option<Value>,
    }

    let result: EvalResult = serde_json::from_str(raw.get())
        .map_err(|err| BridgeError::Protocol(format!("decode Runtime.evaluate result: {err}")))?;
    Ok(result
        .result
        .and_then(|result| result.value)
        .map(|value| match value {
            Value::String(s) => s,
            other => other.to_string(),
        }))
}

fn screenshot_data(raw: &RawValue) -> Result<String> {
    #[derive(Deserialize)]
    struct Screenshot {
        data: String,
    }

    let result: Screenshot = serde_json::from_str(raw.get())
        .map_err(|err| BridgeError::Protocol(format!("parse screenshot response: {err}")))?;
    Ok(result.data)
}

fn parse_action_result(raw: &RawValue, action: &str) -> Result<()> {
    #[derive(Deserialize)]
    struct ActionResult {
        #[serde(default)]
        ok: bool,
        #[serde(default)]
        error: String,
    }

    let Some(value) = runtime_value_string(raw)? else {
        return Err(BridgeError::Protocol(format!(
            "{action}: missing result.value"
        )));
    };
    let result: ActionResult = serde_json::from_str(&value)
        .map_err(|err| BridgeError::Protocol(format!("decode {action} result: {err}")))?;
    if !result.error.is_empty() {
        return Err(BridgeError::User(format!("{action}: {}", result.error)));
    }
    if !result.ok {
        return Err(BridgeError::Protocol(format!(
            "{action}: result was not ok"
        )));
    }
    Ok(())
}

fn parse_node_id(node_id: &str) -> Result<i64> {
    node_id.parse::<i64>().map_err(|_| {
        BridgeError::User(format!(
            "dom_cua_click requires numeric node_id, got {node_id:?}"
        ))
    })
}

fn box_model_center(raw: &RawValue) -> Result<(f64, f64)> {
    #[derive(Deserialize)]
    struct BoxResponse {
        model: BoxModel,
    }

    #[derive(Deserialize)]
    struct BoxModel {
        content: Vec<f64>,
    }

    let result: BoxResponse = serde_json::from_str(raw.get())
        .map_err(|err| BridgeError::Protocol(format!("parse box model: {err}")))?;
    if result.model.content.len() < 8 {
        return Err(BridgeError::Protocol(format!(
            "box model has insufficient content quads: got {} elements",
            result.model.content.len()
        )));
    }
    let x = (result.model.content[0]
        + result.model.content[2]
        + result.model.content[4]
        + result.model.content[6])
        / 4.0;
    let y = (result.model.content[1]
        + result.model.content[3]
        + result.model.content[5]
        + result.model.content[7])
        / 4.0;
    Ok((x, y))
}

pub fn json_escaped(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into())
}

pub fn build_click_script(selector: &str) -> String {
    let s = json_escaped(selector);
    format!(
        "(function(){{try{{var el=document.querySelector({s});if(!el)return JSON.stringify({{error:'element not found: '+{s}}});el.click();return JSON.stringify({{ok:true}})}}catch(e){{return JSON.stringify({{error:String(e&&e.message||e)}})}}}})()"
    )
}

pub fn build_fill_script(selector: &str, value: &str) -> String {
    let s = json_escaped(selector);
    let v = json_escaped(value);
    format!(
        "(function(){{var el=document.querySelector({s});if(!el)return JSON.stringify({{error:'element not found: '+{s}}});el.focus();el.value={v};el.dispatchEvent(new Event('input',{{bubbles:true}}));el.dispatchEvent(new Event('change',{{bubbles:true}}));return JSON.stringify({{ok:true}})}})()"
    )
}

pub(crate) fn is_tab_gone_error(err: &BridgeError) -> bool {
    let msg = err.to_string().to_ascii_lowercase();
    [
        "target closed",
        "no target",
        "target does not exist",
        "cannot find target",
        "tab closed",
    ]
    .iter()
    .any(|needle| msg.contains(needle))
}

pub(crate) fn is_transient_load_error(err: &BridgeError) -> bool {
    let msg = err.to_string().to_ascii_lowercase();
    [
        "execution context destroyed",
        "cannot find context with specified id",
        "inspected target navigated",
        "target closed",
        "frame was detached",
    ]
    .iter()
    .any(|needle| msg.contains(needle))
}

async fn sleep_until(deadline: Instant, max: Duration) {
    let now = Instant::now();
    if now >= deadline {
        return;
    }
    let remaining = deadline - now;
    sleep(remaining.min(max)).await;
}

const VISIBLE_DOM_SCRIPT: &str = r#"(() => {
    function walk(node, depth) {
        if (depth > 5) return '';
        if (!node || node.nodeType !== 1) return '';
        const tag = node.tagName.toLowerCase();
        const id = node.id ? '#'+node.id : '';
        const cls = node.className ? '.'+String(node.className).replace(/\s+/g,'.') : '';
        const text = node.childNodes.length === 1 && node.childNodes[0].nodeType === 3 ? node.childNodes[0].textContent.trim() : '';
        const rect = node.getBoundingClientRect();
        const vis = rect.width > 0 && rect.height > 0;
        if (!vis) return '';
        let line = '  '.repeat(depth) + '<' + tag + id + cls + '>';
        if (text) line += ' ' + text.slice(0,80);
        line += '\n';
        for (const ch of node.children) line += walk(ch, depth+1);
        return line;
    }
    return walk(document.body, 0);
})()"#;

pub async fn run_cli(client: Client) -> anyhow::Result<()> {
    println!("Connected to Codex browser pipe");
    println!("Commands: tabs, create, close <id>, user-tabs, claim <id>, nav <id> <url>,");
    println!("          snapshot <id>, screenshot <id>, info, ping, try <method>, quit");

    let stdin = io::stdin();
    loop {
        print!("> ");
        io::stdout().flush()?;

        let mut line = String::new();
        if stdin.read_line(&mut line)? == 0 {
            println!("\nGoodbye.");
            return Ok(());
        }

        let args = split_args(line.trim_end_matches(['\r', '\n']));
        if args.is_empty() {
            continue;
        }

        match args[0].as_str() {
            "tabs" => match list_tabs(&client).await {
                Ok(tabs) => {
                    for tab in tabs {
                        println!("  [{}] {} - {}", tab.id, tab.title, tab.url);
                    }
                }
                Err(err) => println!("Error: {err}"),
            },
            "create" => match create_tab(&client).await {
                Ok(id) => println!("Created tab: {id}"),
                Err(err) => println!("Error: {err}"),
            },
            "close" => {
                if args.len() < 2 {
                    println!("Usage: close <tab_id>");
                    continue;
                }
                match close_tab(&client, &args[1]).await {
                    Ok(()) => println!("Closed tab {}", args[1]),
                    Err(err) => println!("Error: {err}"),
                }
            }
            "user-tabs" => match list_user_tabs(&client).await {
                Ok(tabs) => {
                    for tab in tabs {
                        println!(
                            "  [{}] {} - {} (group: {})",
                            tab.id, tab.title, tab.url, tab.tab_group
                        );
                    }
                }
                Err(err) => println!("Error: {err}"),
            },
            "claim" => {
                if args.len() < 2 {
                    println!("Usage: claim <tab_id>");
                    continue;
                }
                match claim_user_tab(&client, &args[1]).await {
                    Ok(tab) => println!("Claimed: [{}] {} - {}", tab.id, tab.title, tab.url),
                    Err(err) => println!("Error: {err}"),
                }
            }
            "nav" => {
                if args.len() < 3 {
                    println!("Usage: nav <tab_id> <url>");
                    continue;
                }
                match navigate(&client, &args[1], &args[2]).await {
                    Ok(()) => println!("Navigated tab {} to {}", args[1], args[2]),
                    Err(err) => println!("Error: {err}"),
                }
            }
            "snapshot" => {
                if args.len() < 2 {
                    println!("Usage: snapshot <tab_id>");
                    continue;
                }
                match dom_snapshot(&client, &args[1]).await {
                    Ok(snapshot) => println!("{snapshot}"),
                    Err(err) => println!("Error: {err}"),
                }
            }
            "screenshot" => {
                if args.len() < 2 {
                    println!("Usage: screenshot <tab_id>");
                    continue;
                }
                match screenshot(&client, &args[1], false).await {
                    Ok(data) => println!("Screenshot ({} bytes base64)", data.len()),
                    Err(err) => println!("Error: {err}"),
                }
            }
            "info" => match client.send_request("getInfo", None).await {
                Ok(info) => println!("{}", info.get()),
                Err(err) => println!("Error: {err}"),
            },
            "ping" => match client.send_request("ping", None).await {
                Ok(raw) => println!("{}", raw.get()),
                Err(err) => println!("Error: {err}"),
            },
            "try" => {
                if args.len() < 2 {
                    println!("Usage: try <method> [json_params]");
                    continue;
                }
                let params = if args.len() > 2 {
                    let json = args[2..].join(" ");
                    match serde_json::from_str::<Value>(&json) {
                        Ok(Value::Object(_)) => Some(serde_json::from_str(&json)?),
                        Ok(_) => {
                            println!("Invalid JSON params: expected object");
                            continue;
                        }
                        Err(err) => {
                            println!("Invalid JSON params: {err}");
                            continue;
                        }
                    }
                } else {
                    None
                };
                match client.send_request(&args[1], params).await {
                    Ok(raw) => println!("{}", raw.get()),
                    Err(err) => println!("Error: {err}"),
                }
            }
            "quit" | "exit" => return Ok(()),
            other => println!("Unknown command: {other}"),
        }
    }
}

pub fn split_args(s: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_quote = false;

    for ch in s.chars() {
        match ch {
            '"' => in_quote = !in_quote,
            ' ' if !in_quote => {
                if !current.is_empty() {
                    args.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(ch),
        }
    }

    if !current.is_empty() {
        args.push(current);
    }

    args
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::value::RawValue;

    #[test]
    fn blocks_dangerous_url_schemes() {
        for url in [
            "file:///c:/x",
            "javascript:alert(1)",
            "data:text/html,x",
            "about:blank",
        ] {
            assert!(validate_url(url).is_err(), "{url}");
        }
        assert!(validate_url("https://example.com").is_ok());
    }

    #[test]
    fn decodes_numeric_and_string_tab_ids() {
        let raw =
            RawValue::from_string(r#"[{"id":1,"url":"u"},{"id":"2","title":"t"}]"#.into()).unwrap();
        let tabs = decode_tabs(&raw).unwrap();
        assert_eq!(tabs[0].id, "1");
        assert_eq!(tabs[1].id, "2");
    }

    #[test]
    fn split_args_keeps_quoted_segments() {
        assert_eq!(
            split_args(r#"try method {"value": "hello world"}"#),
            vec!["try", "method", "{value:", "hello world}"]
        );
        assert_eq!(
            split_args(r#"nav 1 "https://example.com/a b""#),
            vec!["nav", "1", "https://example.com/a b"]
        );
    }

    #[test]
    fn parses_resource_tree() {
        let raw = RawValue::from_string(
            r#"{"frame":{"id":"F1","url":"https://example.com"},"resources":[{"url":"https://example.com/a.png","type":"Image","mimeType":"image/png","contentSize":1234.0},{"url":"https://example.com/style.css","type":"Stylesheet","mimeType":"text/css"}],"childFrames":[{"frame":{"id":"F2","url":"about:blank"},"resources":[{"url":"https://cdn.example.com/font.woff2","type":"Font","mimeType":"font/woff2"}]}]}"#.into()
        ).unwrap();
        let resources = parse_resource_tree(&raw).unwrap();
        assert_eq!(resources.len(), 3);
        assert_eq!(resources[0].url, "https://example.com/a.png");
        assert_eq!(resources[0].resource_type, "Image");
        assert_eq!(resources[0].size, Some(1234));
        assert_eq!(resources[2].resource_type, "Font");
        assert_eq!(resources[2].url, "https://cdn.example.com/font.woff2");
    }

    #[test]
    fn parses_cookies() {
        let raw = RawValue::from_string(
            r#"{"cookies":[{"name":"session","value":"abc123","domain":".example.com","path":"/","httpOnly":true,"secure":true,"sameSite":"Lax"},{"name":"theme","value":"dark","domain":"example.com","path":"/","httpOnly":false,"secure":false}]}"#.into()
        ).unwrap();
        let cookies = parse_cookies(&raw).unwrap();
        assert_eq!(cookies.len(), 2);
        assert_eq!(cookies[0].name, "session");
        assert_eq!(cookies[0].http_only, true);
        assert_eq!(cookies[0].same_site.as_deref(), Some("Lax"));
        assert_eq!(cookies[1].name, "theme");
        assert_eq!(cookies[1].secure, false);
    }

    #[test]
    fn parses_resource_content() {
        let raw = RawValue::from_string(
            r#"{"content":"body { color: red; }","base64Encoded":false}"#.into()
        ).unwrap();
        let content = extract_resource_content(&raw).unwrap();
        assert_eq!(content, "body { color: red; }");
    }

    #[test]
    fn resource_content_empty_is_error() {
        let raw = RawValue::from_string(
            r#"{"content":"","base64Encoded":false}"#.into()
        ).unwrap();
        assert!(extract_resource_content(&raw).is_err());
    }

    #[test]
    fn execute_cdp_blocks_dangerous_domains() {
        let blocked = ["Browser.grantPermissions", "Debugger.enable", "Target.createTarget", "Page.addScriptToEvaluateOnNewDocument"];
        for method in blocked {
            assert!(execute_cdp_generic_block_check(method));
        }
        let allowed = ["Runtime.evaluate", "Page.navigate", "Network.getCookies", "DOM.getDocument"];
        for method in allowed {
            assert!(!execute_cdp_generic_block_check(method));
        }
    }

    fn execute_cdp_generic_block_check(method: &str) -> bool {
        for blocked in BLOCKED_CDP_DOMAINS {
            if method.starts_with(blocked) || method == blocked.trim_end_matches('.') {
                return true;
            }
        }
        false
    }
}
