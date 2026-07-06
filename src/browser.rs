use base64::{engine::general_purpose, Engine as _};
use serde::{Deserialize, Serialize};
use serde_json::{json, value::RawValue, Value};
use tokio::time::{sleep, Duration, Instant};

use crate::client::Client;
use crate::error::{BridgeError, Result};

const MAX_WAIT_MS: u64 = 60_000;
const MAX_CAPTURE_MS: u64 = 30_000;
const MAX_FORM_DELAY_MS: u64 = 1_000;

fn bounded_duration_ms(name: &str, value: u64, default_ms: u64, max_ms: u64) -> Result<u64> {
    let value = if value == 0 { default_ms } else { value };
    if value > max_ms {
        return Err(BridgeError::User(format!(
            "{name} must be <= {max_ms} milliseconds"
        )));
    }
    Ok(value)
}

fn deadline_from_now(timeout_ms: u64) -> Result<Instant> {
    Instant::now()
        .checked_add(Duration::from_millis(timeout_ms))
        .ok_or_else(|| BridgeError::User("timeout_ms is too large".into()))
}

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
    // Mark as attached so subsequent CDP calls skip the detach+attach cycle
    client.mark_attached(id).await;
    Ok(tab.normalize())
}

pub async fn close_tab(client: &Client, tab_id: &str) -> Result<()> {
    let id = parse_tab_id("close_tab", tab_id)?;
    match client.execute_cdp(id, "Page.close", None).await {
        Ok(_) => {
            client.retire_tab_state(id).await;
            Ok(())
        }
        Err(err) if is_tab_gone_error(&err) => {
            client.retire_tab_state(id).await;
            Ok(())
        }
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
    let timeout_ms = bounded_duration_ms("timeout_ms", timeout_ms, 10_000, MAX_WAIT_MS)?;
    let deadline = deadline_from_now(timeout_ms)?;
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

pub async fn screenshot(
    client: &Client,
    tab_id: &str,
    full_page: bool,
    format: &str,
    quality: Option<u64>,
) -> Result<String> {
    let _ = full_page;
    let id = parse_tab_id("screenshot", tab_id)?;
    let mut params = serde_json::Map::new();
    params.insert("format".into(), json!(format));
    if format == "jpeg" {
        params.insert("quality".into(), json!(quality.unwrap_or(80).min(100)));
    }
    let raw = client
        .execute_cdp(
            id,
            "Page.captureScreenshot",
            Some(serde_json::Value::Object(params)),
        )
        .await?;
    screenshot_data(&raw)
}

/// Bring a tab to the foreground via `Page.bringToFront`.
///
/// Needed when a background tab has been throttled or discarded by Chrome — CDP
/// calls (especially screenshot) on a suspended tab can time out silently.
/// Calling this restores the tab's rendering pipeline so subsequent CDP calls
/// respond normally. Does not navigate or change page state.
pub async fn bring_to_front(client: &Client, tab_id: &str) -> Result<()> {
    let id = parse_tab_id("bring_to_front", tab_id)?;
    client
        .execute_cdp(id, "Page.bringToFront", None)
        .await
        .map(|_| ())
}

/// Current tab URL via `location.href`.
pub async fn get_url(client: &Client, tab_id: &str) -> Result<String> {
    let id = parse_tab_id("get_url", tab_id)?;
    let raw = client
        .execute_cdp(
            id,
            "Runtime.evaluate",
            Some(json!({ "expression": "location.href", "returnByValue": true })),
        )
        .await?;
    runtime_value_string(&raw)?
        .ok_or_else(|| BridgeError::Protocol("empty url from location.href".into()))
}

/// Current page title via `document.title`.
pub async fn get_title(client: &Client, tab_id: &str) -> Result<String> {
    let id = parse_tab_id("get_title", tab_id)?;
    let raw = client
        .execute_cdp(
            id,
            "Runtime.evaluate",
            Some(json!({ "expression": "document.title", "returnByValue": true })),
        )
        .await?;
    Ok(runtime_value_string(&raw)?.unwrap_or_default())
}

/// Poll until a CSS selector matches. Essential for SPAs where `wait_for_load`
/// returns immediately (URL unchanged) but content renders asynchronously.
pub async fn wait_for_element(
    client: &Client,
    tab_id: &str,
    selector: &str,
    timeout_ms: u64,
) -> Result<()> {
    let id = parse_tab_id("wait_for_element", tab_id)?;
    let timeout_ms = bounded_duration_ms("timeout_ms", timeout_ms, 10_000, MAX_WAIT_MS)?;
    let deadline = deadline_from_now(timeout_ms)?;
    let escaped = selector.replace('\\', "\\\\").replace('`', "\\`");
    let expr = format!("document.querySelector(`{escaped}`) !== null");

    loop {
        if Instant::now() >= deadline {
            return Err(BridgeError::Timeout(format!(
                "element {selector:?} not found after {timeout_ms}ms"
            )));
        }
        match client
            .execute_cdp(
                id,
                "Runtime.evaluate",
                Some(json!({ "expression": expr, "returnByValue": true })),
            )
            .await
        {
            Ok(raw) => {
                if let Some(found) = runtime_value_string(&raw)? {
                    if found == "true" {
                        return Ok(());
                    }
                }
            }
            Err(err) if is_transient_load_error(&err) && Instant::now() < deadline => {}
            Err(err) => return Err(err),
        }
        sleep_until(deadline, Duration::from_millis(100)).await;
    }
}

/// Poll until `location.href` contains `pattern`. For SPAs that change the URL
/// on route change without a full navigation.
pub async fn wait_for_url(
    client: &Client,
    tab_id: &str,
    pattern: &str,
    timeout_ms: u64,
) -> Result<()> {
    let id = parse_tab_id("wait_for_url", tab_id)?;
    let timeout_ms = bounded_duration_ms("timeout_ms", timeout_ms, 10_000, MAX_WAIT_MS)?;
    let deadline = deadline_from_now(timeout_ms)?;
    let pat_json = serde_json::to_string(pattern).unwrap_or_else(|_| "null".into());
    let expr = format!("location.href.indexOf({pat_json}) >= 0");

    loop {
        if Instant::now() >= deadline {
            return Err(BridgeError::Timeout(format!(
                "URL did not contain {pattern:?} after {timeout_ms}ms"
            )));
        }
        match client
            .execute_cdp(
                id,
                "Runtime.evaluate",
                Some(json!({ "expression": expr, "returnByValue": true })),
            )
            .await
        {
            Ok(raw) => {
                if let Some(found) = runtime_value_string(&raw)? {
                    if found == "true" {
                        return Ok(());
                    }
                }
            }
            Err(err) if is_transient_load_error(&err) && Instant::now() < deadline => {}
            Err(err) => return Err(err),
        }
        sleep_until(deadline, Duration::from_millis(100)).await;
    }
}

/// Hover over an element by CSS selector. Dispatches mouseover + mousemove via JS.
pub async fn hover(client: &Client, tab_id: &str, selector: &str) -> Result<()> {
    let id = parse_tab_id("hover", tab_id)?;
    let escaped = selector.replace('\\', "\\\\").replace('`', "\\`");
    let expr = format!(
        "(function(){{var e=document.querySelector(`{escaped}`);if(!e)return{{ok:false,error:'element not found'}};e.dispatchEvent(new MouseEvent('mouseover',{{bubbles:true}}));e.dispatchEvent(new MouseEvent('mousemove',{{bubbles:true}}));return{{ok:true}}}})()"
    );
    let raw = client
        .execute_cdp(
            id,
            "Runtime.evaluate",
            Some(json!({ "expression": expr, "returnByValue": true })),
        )
        .await?;
    parse_action_result(&raw, "hover")
}

/// Render the page to PDF via `Page.printToPDF`. Returns base64-encoded PDF.
pub async fn print_pdf(client: &Client, tab_id: &str) -> Result<String> {
    let id = parse_tab_id("print_pdf", tab_id)?;
    let raw = client
        .execute_cdp(
            id,
            "Page.printToPDF",
            Some(json!({ "format": "A4", "printBackground": true })),
        )
        .await?;
    #[derive(Deserialize)]
    struct Pdf {
        data: String,
    }
    let pdf: Pdf = serde_json::from_str(raw.get())
        .map_err(|err| BridgeError::Protocol(format!("parse printToPDF response: {err}")))?;
    Ok(pdf.data)
}

// ── Storage ─────────────────────────────────────────────────────

/// Read a Web Storage key (local or session). Returns None if the key is unset.
pub async fn storage_get(
    client: &Client,
    tab_id: &str,
    key: &str,
    storage_type: &str,
) -> Result<Option<String>> {
    let id = parse_tab_id("storage_get", tab_id)?;
    let key_json = serde_json::to_string(key).unwrap_or_else(|_| "null".into());
    let store = if storage_type == "session" {
        "sessionStorage"
    } else {
        "localStorage"
    };
    let raw = client
        .execute_cdp(
            id,
            "Runtime.evaluate",
            Some(json!({ "expression": format!("{store}.getItem({key_json})"), "returnByValue": true })),
        )
        .await?;
    Ok(runtime_value_string(&raw)?.filter(|s| s != "null"))
}

/// Write a Web Storage key (local or session).
pub async fn storage_set(
    client: &Client,
    tab_id: &str,
    key: &str,
    value: &str,
    storage_type: &str,
) -> Result<()> {
    let id = parse_tab_id("storage_set", tab_id)?;
    let key_json = serde_json::to_string(key).unwrap_or_else(|_| "null".into());
    let val_json = serde_json::to_string(value).unwrap_or_else(|_| "null".into());
    let store = if storage_type == "session" {
        "sessionStorage"
    } else {
        "localStorage"
    };
    let raw = client
        .execute_cdp(
            id,
            "Runtime.evaluate",
            Some(json!({ "expression": format!("{store}.setItem({key_json}, {val_json}); true"), "returnByValue": true })),
        )
        .await?;
    runtime_value_string(&raw)?;
    Ok(())
}

// ── Form & advanced interaction ──────────────────────────────────

/// Set a `<select>` element's value and fire change/input. Plain codex_fill
/// does not reliably trigger change handlers on select elements.
pub async fn select_option(
    client: &Client,
    tab_id: &str,
    selector: &str,
    value: &str,
) -> Result<()> {
    let id = parse_tab_id("select_option", tab_id)?;
    let sel = serde_json::to_string(selector).unwrap_or_else(|_| "null".into());
    let val = serde_json::to_string(value).unwrap_or_else(|_| "null".into());
    let expr = format!(
        "(function(){{var s=document.querySelector({sel});if(!s)return{{ok:false,error:'element not found'}};s.value={val};s.dispatchEvent(new Event('input',{{bubbles:true}}));s.dispatchEvent(new Event('change',{{bubbles:true}}));return{{ok:true}}}})()"
    );
    let raw = client
        .execute_cdp(
            id,
            "Runtime.evaluate",
            Some(json!({ "expression": expr, "returnByValue": true })),
        )
        .await?;
    parse_action_result(&raw, "select_option")
}

/// Drag from one point to another via CDP mouse events (down → interpolated moves → up).
pub async fn drag(
    client: &Client,
    tab_id: &str,
    from_x: i64,
    from_y: i64,
    to_x: i64,
    to_y: i64,
) -> Result<()> {
    let id = parse_tab_id("drag", tab_id)?;
    client
        .execute_cdp(
            id,
            "Input.dispatchMouseEvent",
            Some(json!({ "type": "mousePressed", "x": from_x, "y": from_y, "button": "left", "clickCount": 1 })),
        )
        .await?;
    let steps = 5;
    for i in 1..=steps {
        let t = i as f64 / steps as f64;
        let x = from_x as f64 + (to_x as f64 - from_x as f64) * t;
        let y = from_y as f64 + (to_y as f64 - from_y as f64) * t;
        client
            .execute_cdp(
                id,
                "Input.dispatchMouseEvent",
                Some(json!({ "type": "mouseMoved", "x": x as i64, "y": y as i64 })),
            )
            .await?;
    }
    client
        .execute_cdp(
            id,
            "Input.dispatchMouseEvent",
            Some(json!({ "type": "mouseReleased", "x": to_x, "y": to_y, "button": "left", "clickCount": 1 })),
        )
        .await
        .map(|_| ())
}

/// Screenshot a single element by clipping to its bounding rect.
pub async fn screenshot_element(client: &Client, tab_id: &str, selector: &str) -> Result<String> {
    let id = parse_tab_id("screenshot_element", tab_id)?;
    let sel = serde_json::to_string(selector).unwrap_or_else(|_| "null".into());
    let expr = format!(
        "(function(){{var e=document.querySelector({sel});if(!e)return null;var r=e.getBoundingClientRect();if(r.width===0||r.height===0)return null;return JSON.stringify({{x:r.x,y:r.y,width:r.width,height:r.height}})}})()"
    );
    let raw = client
        .execute_cdp(
            id,
            "Runtime.evaluate",
            Some(json!({ "expression": expr, "returnByValue": true })),
        )
        .await?;
    let rect_str = runtime_value_string(&raw)?
        .filter(|s| s != "null")
        .ok_or_else(|| {
            BridgeError::User(format!("element {selector:?} not found or has zero size"))
        })?;
    #[derive(Deserialize)]
    struct Rect {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    }
    let rect: Rect = serde_json::from_str(&rect_str)
        .map_err(|err| BridgeError::Protocol(format!("parse element rect: {err}")))?;
    let raw = client
        .execute_cdp(
            id,
            "Page.captureScreenshot",
            Some(json!({
                "format": "png",
                "clip": { "x": rect.x, "y": rect.y, "width": rect.width, "height": rect.height, "scale": 1.0 }
            })),
        )
        .await?;
    screenshot_data(&raw)
}

/// Delete cookies by name (optionally scoped by url/domain/path).
pub async fn delete_cookies(client: &Client, tab_id: &str, params: Value) -> Result<()> {
    let id = parse_tab_id("delete_cookies", tab_id)?;
    client
        .execute_cdp(id, "Network.deleteCookies", Some(params))
        .await
        .map(|_| ())
}

/// Emulate a device viewport (width/height/userAgent/mobile).
pub async fn emulate_device(
    client: &Client,
    tab_id: &str,
    width: i64,
    height: i64,
    user_agent: &str,
    mobile: bool,
) -> Result<()> {
    let id = parse_tab_id("emulate_device", tab_id)?;
    client
        .execute_cdp(
            id,
            "Emulation.setDeviceMetricsOverride",
            Some(json!({
                "width": width,
                "height": height,
                "deviceScaleFactor": 1,
                "mobile": mobile,
                "userAgent": user_agent
            })),
        )
        .await
        .map(|_| ())
}

/// Clear device emulation (revert to real viewport).
pub async fn reset_device(client: &Client, tab_id: &str) -> Result<()> {
    let id = parse_tab_id("reset_device", tab_id)?;
    client
        .execute_cdp(id, "Emulation.clearDeviceMetricsOverride", None)
        .await
        .map(|_| ())
}

// ── Event capture (requires event subscription architecture) ────

/// A paired network request entry: sent request + (optional) received response.
#[derive(Serialize)]
struct NetEntry {
    request_id: String,
    url: String,
    method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    resource_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mime_type: Option<String>,
}

/// Capture `Network.*` events for a duration and pair request↔response into a
/// structured, agent-friendly list. Enables Network domain, collects for
/// `duration_ms`, then disables.
pub async fn network_monitor(client: &Client, tab_id: &str, duration_ms: u64) -> Result<Value> {
    let id = parse_tab_id("network_monitor", tab_id)?;
    let duration_ms = bounded_duration_ms("duration_ms", duration_ms, 5_000, MAX_CAPTURE_MS)?;
    let (sub_id, mut rx) = client.subscribe_events("Network.", 512).await;
    if let Err(err) = client.execute_cdp(id, "Network.enable", None).await {
        client.unsubscribe_events(sub_id).await;
        return Err(err);
    }
    tokio::time::sleep(Duration::from_millis(duration_ms)).await;
    client.execute_cdp(id, "Network.disable", None).await.ok();
    client.unsubscribe_events(sub_id).await;

    let events: Vec<Value> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
    let raw_count = events.len();
    let requests = pair_network_events(&events);
    Ok(json!({
        "duration_ms": duration_ms,
        "raw_event_count": raw_count,
        "request_count": requests.len(),
        "requests": requests
    }))
}

/// Pair `Network.requestWillBeSent` + `Network.responseReceived` events by
/// `requestId` into a stable (first-seen) ordered list. Extracted from
/// `network_monitor` so the pairing logic is testable without a live CDP
/// session — feed it a Vec of event frames, assert the merged entries.
fn pair_network_events(events: &[Value]) -> Vec<NetEntry> {
    let mut entries: std::collections::HashMap<String, NetEntry> = std::collections::HashMap::new();
    let mut order: Vec<String> = Vec::new();
    for v in events {
        let method = v.get("method").and_then(|m| m.as_str()).unwrap_or("");
        let params = v.get("params");
        match method {
            "Network.requestWillBeSent" => {
                if let Some(req) = params.and_then(|p| p.get("request")) {
                    let rid = params
                        .and_then(|p| p.get("requestId"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    if rid.is_empty() {
                        continue;
                    }
                    let entry = NetEntry {
                        request_id: rid.clone(),
                        url: req
                            .get("url")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        method: req
                            .get("method")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        resource_type: params
                            .and_then(|p| p.get("type"))
                            .and_then(|v| v.as_str())
                            .map(String::from),
                        status: None,
                        mime_type: None,
                    };
                    if !entries.contains_key(&rid) {
                        order.push(rid.clone());
                    }
                    entries.insert(rid, entry);
                }
            }
            "Network.responseReceived" => {
                let rid = params
                    .and_then(|p| p.get("requestId"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if let Some(resp) = params.and_then(|p| p.get("response")) {
                    if let Some(e) = entries.get_mut(&rid) {
                        e.status = resp.get("status").and_then(|v| v.as_i64());
                        e.mime_type = resp
                            .get("mimeType")
                            .and_then(|v| v.as_str())
                            .map(String::from);
                    }
                }
            }
            _ => {}
        }
    }
    order
        .into_iter()
        .filter_map(|rid| entries.remove(&rid))
        .collect()
}

/// Capture `console.*` log calls for a duration. Enables Runtime, collects
/// `Runtime.consoleAPICalled` events, then disables. Returns raw log params.
pub async fn console_logs(client: &Client, tab_id: &str, duration_ms: u64) -> Result<Value> {
    let id = parse_tab_id("console_logs", tab_id)?;
    let duration_ms = bounded_duration_ms("duration_ms", duration_ms, 5_000, MAX_CAPTURE_MS)?;
    let (sub_id, mut rx) = client
        .subscribe_events("Runtime.consoleAPICalled", 512)
        .await;
    if let Err(err) = client.execute_cdp(id, "Runtime.enable", None).await {
        client.unsubscribe_events(sub_id).await;
        return Err(err);
    }
    tokio::time::sleep(Duration::from_millis(duration_ms)).await;
    client.execute_cdp(id, "Runtime.disable", None).await.ok();
    client.unsubscribe_events(sub_id).await;
    let mut logs = Vec::new();
    while let Ok(v) = rx.try_recv() {
        if let Some(params) = v.get("params") {
            logs.push(params.clone());
        }
    }
    Ok(json!({
        "duration_ms": duration_ms,
        "log_count": logs.len(),
        "logs": logs
    }))
}

/// Get Performance metrics via `Performance.getMetrics` — DOM node count, JS
/// heap size, document count, event listener count, etc. Enables/disables the
/// Performance domain around the read.
pub async fn performance_metrics(client: &Client, tab_id: &str) -> Result<Value> {
    let id = parse_tab_id("performance_metrics", tab_id)?;
    client.execute_cdp(id, "Performance.enable", None).await?;
    let raw = client.execute_cdp(id, "Performance.getMetrics", None).await;
    client
        .execute_cdp(id, "Performance.disable", None)
        .await
        .ok();
    let raw = raw?;
    serde_json::from_str(raw.get())
        .map_err(|err| BridgeError::Protocol(format!("parse performance metrics: {err}")))
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

const ALLOWED_CDP_METHODS: &[&str] = &[
    "Accessibility.getFullAXTree",
    "CSS.getComputedStyleForNode",
    "CSS.getMatchedStylesForNode",
    "DOM.describeNode",
    "DOM.getAttributes",
    "DOM.getBoxModel",
    "DOM.getDocument",
    "DOM.querySelector",
    "DOM.querySelectorAll",
    "DOM.resolveNode",
    "Log.clear",
    "Log.disable",
    "Network.disable",
    "Page.getFrameTree",
    "Page.getLayoutMetrics",
    "Performance.disable",
    "Performance.getMetrics",
    "Runtime.getProperties",
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
            return blocked_cdp_method(method, blocked);
        }
    }
    validate_generic_cdp_method(method)?;
    let id = parse_tab_id("execute_cdp", tab_id)?;
    client.execute_cdp(id, method, params).await
}

fn validate_generic_cdp_method(method: &str) -> Result<()> {
    if ALLOWED_CDP_METHODS.contains(&method) {
        return Ok(());
    }
    Err(BridgeError::User(format!(
        "blocked CDP method: {method} (method is not in the generic CDP allowlist)"
    )))
}

fn blocked_cdp_method<T>(method: &str, blocked: &str) -> Result<T> {
    Err(BridgeError::User(format!(
        "blocked CDP method: {method} ({}is not allowed for security)",
        if blocked.ends_with('.') {
            "domain "
        } else {
            ""
        }
    )))
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
    let raw = client.execute_cdp(id, "Page.getResourceTree", None).await?;
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

pub async fn get_resource_content_with_timeout(
    client: &Client,
    tab_id: &str,
    frame_id: &str,
    url: &str,
    timeout: Duration,
) -> Result<String> {
    let id = parse_tab_id("page_assets", tab_id)?;
    let raw = client
        .execute_cdp_with_timeout(
            id,
            "Page.getResourceContent",
            Some(json!({ "frameId": frame_id, "url": url })),
            timeout,
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

pub async fn set_cookie(client: &Client, tab_id: &str, params: Value) -> Result<()> {
    let id = parse_tab_id("network_set_cookie", tab_id)?;
    client
        .execute_cdp(id, "Network.setCookie", Some(params))
        .await
        .map(|_| ())
}

// ── Locator ─────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct AxMatch {
    pub node_id: String,
    pub role: String,
    pub name: String,
    pub backend_node_id: Option<i64>,
}

/// Find elements by ARIA role and/or accessible name in the AX tree.
pub async fn find_elements(
    client: &Client,
    tab_id: &str,
    role: Option<&str>,
    name: Option<&str>,
    max_results: usize,
) -> Result<Vec<AxMatch>> {
    let raw = dom_snapshot(client, tab_id).await?;
    let nodes = parse_ax_tree(&raw)?;
    let mut matches = Vec::new();

    for node in nodes {
        if let Some(role_filter) = role {
            if !node.role.eq_ignore_ascii_case(role_filter) {
                continue;
            }
        }
        if let Some(name_filter) = name {
            if !node
                .name
                .to_ascii_lowercase()
                .contains(&name_filter.to_ascii_lowercase())
            {
                continue;
            }
        }
        matches.push(AxMatch {
            node_id: node.node_id,
            role: node.role,
            name: node.name,
            backend_node_id: node.backend_dom_node_id,
        });
        if matches.len() >= max_results {
            break;
        }
    }

    Ok(matches)
}

/// Click an element by its accessibility backend node ID.
/// Reuses the existing DOM.resolveNode → DOM.getBoxModel → Input dispatch pipeline.
pub async fn click_ax_element(client: &Client, tab_id: &str, node_id: &str) -> Result<()> {
    dom_cua_click(client, tab_id, node_id).await
}

// AX tree parsing — defensive: all fields optional

#[derive(Deserialize)]
struct AxNode {
    #[serde(rename = "nodeId")]
    node_id: String,
    #[serde(default)]
    role: Option<AxValue>,
    #[serde(default)]
    name: Option<AxValue>,
    #[serde(rename = "backendDOMNodeId", default)]
    backend_dom_node_id: Option<i64>,
}

#[derive(Deserialize)]
struct AxValue {
    value: Option<String>,
}

#[derive(Deserialize)]
struct AxTree {
    nodes: Vec<AxNode>,
}

struct ParsedAxNode {
    node_id: String,
    role: String,
    name: String,
    backend_dom_node_id: Option<i64>,
}

fn parse_ax_tree(raw: &str) -> Result<Vec<ParsedAxNode>> {
    let tree: AxTree = serde_json::from_str(raw)
        .map_err(|err| BridgeError::Protocol(format!("parse AX tree: {err}")))?;
    Ok(tree
        .nodes
        .into_iter()
        .map(|n| ParsedAxNode {
            node_id: n.node_id,
            role: n.role.and_then(|r| r.value).unwrap_or_default(),
            name: n.name.and_then(|n| n.value).unwrap_or_default(),
            backend_dom_node_id: n.backend_dom_node_id,
        })
        .collect())
}

// ── Composite ───────────────────────────────────────────────────

/// Navigate to URL and wait for page load. Reduces 2 MCP calls to 1.
pub async fn nav_and_wait(client: &Client, tab_id: &str, url: &str, timeout_ms: u64) -> Result<()> {
    navigate(client, tab_id, url).await?;
    wait_for_load(client, tab_id, timeout_ms).await?;
    Ok(())
}

/// Click element by selector and wait for page load.
pub async fn click_and_wait(
    client: &Client,
    tab_id: &str,
    selector: &str,
    timeout_ms: u64,
) -> Result<()> {
    click(client, tab_id, selector).await?;
    wait_for_load(client, tab_id, timeout_ms).await?;
    Ok(())
}

/// Fill multiple form fields at once, optionally submitting.
pub async fn form_fill(
    client: &Client,
    tab_id: &str,
    fields: &Value,
    submit: Option<&str>,
    delay_ms: u64,
) -> Result<()> {
    let obj = fields.as_object().ok_or_else(|| {
        BridgeError::User("fields must be an object mapping selector to value".into())
    })?;
    let delay_ms = bounded_duration_ms("delay_ms", delay_ms, 0, MAX_FORM_DELAY_MS)?;
    for (selector, value) in obj {
        if let Some(val_str) = value.as_str() {
            fill(client, tab_id, selector, val_str).await?;
            if delay_ms > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
            }
        }
    }
    if let Some(submit_sel) = submit {
        click(client, tab_id, submit_sel).await?;
    }
    Ok(())
}

/// Upload files to a `<input type="file">` element via CDP.
/// First resolves the CSS selector via Runtime.evaluate, then calls DOM.setFileInputFiles.
/// `paths` must be validated absolute paths (use `security::validate_file_path` beforehand).
pub async fn file_input(
    client: &Client,
    tab_id: &str,
    selector: &str,
    paths: &[String],
) -> Result<()> {
    let id = parse_tab_id("file_input", tab_id)?;

    // 1. Resolve the file input element via Runtime.evaluate
    let escaped_selector = selector.replace('\\', "\\\\").replace('\'', "\\'");
    let raw = client
        .execute_cdp(
            id,
            "Runtime.evaluate",
            Some(json!({
                "expression": format!(
                    "document.querySelector('{}')",
                    escaped_selector
                ),
                "returnByValue": false,
            })),
        )
        .await?;

    // 2. Extract objectId from result
    #[derive(Deserialize)]
    struct EvaluateResult {
        result: EvaluateResultInner,
    }
    #[derive(Deserialize)]
    struct EvaluateResultInner {
        #[serde(rename = "objectId")]
        object_id: Option<String>,
        #[serde(rename = "type")]
        _type: String,
        #[serde(default)]
        subtype: Option<String>,
    }

    let eval: EvaluateResult = serde_json::from_str(raw.get())
        .map_err(|err| BridgeError::Protocol(format!("parse Runtime.evaluate result: {err}")))?;

    let object_id = eval
        .result
        .object_id
        .filter(|_| eval.result._type == "object" && eval.result.subtype.as_deref() == Some("node"))
        .ok_or_else(|| BridgeError::User(format!("File input not found: {selector}")))?;

    // 3. Set files on the resolved node
    client
        .execute_cdp(
            id,
            "DOM.setFileInputFiles",
            Some(json!({
                "objectId": object_id,
                "files": paths,
            })),
        )
        .await
        .map(|_| ())
}

// ── Dialog ──────────────────────────────────────────────────────

/// Handle a JavaScript dialog (alert, confirm, prompt) via CDP Page.handleJavaScriptDialog.
pub async fn handle_dialog(
    client: &Client,
    tab_id: &str,
    action: &str,
    prompt_text: Option<&str>,
) -> Result<()> {
    let id = parse_tab_id("dialog", tab_id)?;
    let mut params = serde_json::Map::new();
    params.insert("accept".into(), json!(action == "accept"));
    if let Some(text) = prompt_text {
        params.insert("promptText".into(), json!(text));
    }
    client
        .execute_cdp(
            id,
            "Page.handleJavaScriptDialog",
            Some(serde_json::Value::Object(params)),
        )
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
            size: normalize_resource_size(r.content_size),
            failed: None,
            frame_id: frame_id.clone(),
        });
    }
    for child in &tree.child_frames {
        collect_resources(child, out);
    }
}

fn normalize_resource_size(size: Option<f64>) -> Option<u64> {
    let size = size?;
    if !size.is_finite() || size <= 0.0 || size > u64::MAX as f64 {
        return None;
    }
    Some(size.ceil() as u64)
}

fn extract_resource_content(raw: &RawValue) -> Result<String> {
    #[derive(Deserialize)]
    struct ResourceContent {
        #[serde(default)]
        content: String,
        #[serde(default, rename = "base64Encoded")]
        base64_encoded: bool,
    }

    let result: ResourceContent = serde_json::from_str(raw.get())
        .map_err(|err| BridgeError::Protocol(format!("parse resource content: {err}")))?;

    if result.content.is_empty() {
        return Err(BridgeError::Protocol("resource content is empty".into()));
    }
    if result.base64_encoded {
        Ok(result.content)
    } else {
        Ok(general_purpose::STANDARD.encode(result.content.as_bytes()))
    }
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

use crate::security::validate_url;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ax_tree_extracts_nodes_with_defaults() {
        let raw = r#"{"nodes":[
            {"nodeId":"1","role":{"value":"button"},"name":{"value":"Login"},"backendDOMNodeId":42},
            {"nodeId":"2","role":{"value":"link"},"name":{"value":"Home"}},
            {"nodeId":"3"}
        ]}"#;
        let nodes = parse_ax_tree(raw).unwrap();
        assert_eq!(nodes.len(), 3);
        assert_eq!(nodes[0].node_id, "1");
        assert_eq!(nodes[0].role, "button");
        assert_eq!(nodes[0].name, "Login");
        assert_eq!(nodes[0].backend_dom_node_id, Some(42));
        // node with no role/name/backendId defaults to empty/None
        assert_eq!(nodes[2].node_id, "3");
        assert_eq!(nodes[2].role, "");
        assert_eq!(nodes[2].name, "");
        assert_eq!(nodes[2].backend_dom_node_id, None);
    }

    #[test]
    fn parse_ax_tree_rejects_invalid_json() {
        assert!(parse_ax_tree("not json").is_err());
        assert!(parse_ax_tree(r#"{"wrong":1}"#).is_err());
    }

    #[test]
    fn parse_ax_tree_empty_nodes() {
        let nodes = parse_ax_tree(r#"{"nodes":[]}"#).unwrap();
        assert!(nodes.is_empty());
    }

    #[test]
    fn generic_cdp_blocks_methods_that_bypass_dedicated_safety_tools() {
        for method in [
            "DOM.setFileInputFiles",
            "Network.getRequestPostData",
            "Network.getResponseBody",
            "Network.enable",
            "Network.getCookies",
            "Network.setCookie",
            "Page.close",
            "Page.captureScreenshot",
            "Page.getResourceContent",
            "Page.navigate",
            "Page.navigateToHistoryEntry",
            "Page.printToPDF",
            "Performance.enable",
            "Runtime.callFunctionOn",
            "Runtime.evaluate",
        ] {
            assert!(validate_generic_cdp_method(method).is_err(), "{method}");
        }

        assert!(validate_generic_cdp_method("DOM.getDocument").is_ok());
    }

    // ── network_monitor event pairing (pair_network_events) ──

    #[test]
    fn pair_network_events_merges_request_and_response() {
        let events = vec![
            json!({
                "method": "Network.requestWillBeSent",
                "params": {
                    "requestId": "100.1",
                    "request": {"url": "https://example.com/api", "method": "GET"},
                    "type": "Fetch"
                }
            }),
            json!({
                "method": "Network.responseReceived",
                "params": {
                    "requestId": "100.1",
                    "response": {"status": 200, "mimeType": "application/json"}
                }
            }),
        ];
        let got = pair_network_events(&events);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].request_id, "100.1");
        assert_eq!(got[0].url, "https://example.com/api");
        assert_eq!(got[0].method, "GET");
        assert_eq!(got[0].resource_type.as_deref(), Some("Fetch"));
        assert_eq!(got[0].status, Some(200));
        assert_eq!(got[0].mime_type.as_deref(), Some("application/json"));
    }

    #[test]
    fn pair_network_events_preserves_first_seen_order_and_drops_orphans() {
        // Requests keep first-seen order; a response without a prior request is dropped.
        let events = vec![
            json!({"method":"Network.requestWillBeSent","params":{"requestId":"A","request":{"url":"https://a","method":"GET"}}}),
            json!({"method":"Network.requestWillBeSent","params":{"requestId":"B","request":{"url":"https://b","method":"POST"}}}),
            json!({"method":"Network.responseReceived","params":{"requestId":"A","response":{"status":304}}}),
            json!({"method":"Network.responseReceived","params":{"requestId":"Z","response":{"status":500}}}),
        ];
        let got = pair_network_events(&events);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].request_id, "A");
        assert_eq!(got[0].status, Some(304));
        assert_eq!(got[1].request_id, "B");
        assert!(got[1].status.is_none()); // no response arrived
    }

    #[test]
    fn pair_network_events_ignores_empty_request_id() {
        let events = vec![
            json!({"method":"Network.requestWillBeSent","params":{"requestId":"","request":{"url":"https://x","method":"GET"}}}),
        ];
        assert!(pair_network_events(&events).is_empty());
    }

    // ── storage value parsing (runtime_value_string) ──

    #[test]
    fn runtime_value_string_extracts_string_value() {
        let raw = RawValue::from_string(r#"{"result":{"value":"hello"}}"#.into()).unwrap();
        assert_eq!(
            runtime_value_string(&raw).unwrap(),
            Some("hello".to_string())
        );
    }

    #[test]
    fn runtime_value_string_coerces_non_string() {
        // Non-string evaluate results (e.g. a number) stringify.
        let raw = RawValue::from_string(r#"{"result":{"value":42}}"#.into()).unwrap();
        assert_eq!(runtime_value_string(&raw).unwrap(), Some("42".to_string()));
    }

    #[test]
    fn runtime_value_string_none_when_missing() {
        let raw = RawValue::from_string(r#"{"result":{}}"#.into()).unwrap();
        assert_eq!(runtime_value_string(&raw).unwrap(), None);
    }

    #[test]
    fn resource_content_is_normalized_to_base64() {
        let text =
            RawValue::from_string(r#"{"content":"hello","base64Encoded":false}"#.to_string())
                .unwrap();
        assert_eq!(extract_resource_content(&text).unwrap(), "aGVsbG8=");

        let encoded =
            RawValue::from_string(r#"{"content":"aGVsbG8=","base64Encoded":true}"#.to_string())
                .unwrap();
        assert_eq!(extract_resource_content(&encoded).unwrap(), "aGVsbG8=");
    }

    #[test]
    fn resource_tree_treats_invalid_content_size_as_unknown() {
        let raw = RawValue::from_string(
            r#"{"frame":{"id":"root","url":"https://example.com"},"resources":[
                {"url":"https://example.com/negative.bin","type":"Other","mimeType":"application/octet-stream","contentSize":-1},
                {"url":"https://example.com/zero.bin","type":"Other","mimeType":"application/octet-stream","contentSize":0},
                {"url":"https://example.com/ok.css","type":"Stylesheet","mimeType":"text/css","contentSize":12.7}
            ]}"#
                .to_string(),
        )
        .unwrap();

        let resources = parse_resource_tree(&raw).unwrap();

        assert_eq!(resources[0].size, None);
        assert_eq!(resources[1].size, None);
        assert_eq!(resources[2].size, Some(13));
    }

    #[test]
    fn resource_size_normalization_rejects_non_finite_values() {
        assert_eq!(normalize_resource_size(None), None);
        assert_eq!(normalize_resource_size(Some(f64::NAN)), None);
        assert_eq!(normalize_resource_size(Some(f64::INFINITY)), None);
        assert_eq!(normalize_resource_size(Some(1.2)), Some(2));
    }

    #[test]
    fn generic_cdp_rejects_navigation_cookie_and_unknown_methods() {
        for method in [
            "Page.navigate",
            "Page.navigateToHistoryEntry",
            "Network.getCookies",
            "Storage.getCookies",
            "Browser.getVersion",
        ] {
            assert!(validate_generic_cdp_method(method).is_err(), "{method}");
        }
    }

    #[test]
    fn generic_cdp_allows_known_low_level_methods() {
        for method in [
            "DOM.getDocument",
            "Page.getLayoutMetrics",
            "Performance.getMetrics",
            "Runtime.getProperties",
        ] {
            assert!(validate_generic_cdp_method(method).is_ok(), "{method}");
        }
    }
}
