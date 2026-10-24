use serde::Serialize;
use serde_json::{json, value::RawValue, Value};
use tokio::time::{Duration, Instant};

use crate::browser;
use crate::client::Client;
use crate::registry::{self, BrowserSelector};
use crate::security;

/// Map a screenshot format string to its MIME type.
fn mime_for(format: &str) -> &'static str {
    match format {
        "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        _ => "image/png",
    }
}

use super::types::{
    optional_bool, optional_duration_ms, optional_str_array, optional_u64, required_alias_f64,
    required_f64, required_str, required_str_array, required_string_value, required_string_vec,
    sanitize_for_log, Content, ToolHandler,
};

const MAX_WAIT_MS: u64 = 60_000;
const MAX_CAPTURE_MS: u64 = 30_000;
const MAX_FORM_DELAY_MS: u64 = 1_000;
const DEFAULT_PAGE_ASSET_MAX_RESOURCES: u64 = 50;
const MAX_PAGE_ASSET_RESOURCES: u64 = 200;
const DEFAULT_PAGE_ASSET_MAX_TOTAL_BYTES: u64 = 1_048_576;
const MAX_PAGE_ASSET_TOTAL_BYTES: u64 = 5_242_880;
const MAX_PAGE_ASSET_PER_RESOURCE_BYTES: u64 = 1_048_576;
const PAGE_ASSET_FETCH_TIMEOUT_MS: u64 = 1_500;
const PAGE_ASSET_FETCH_TOTAL_TIMEOUT_MS: u64 = 10_000;

#[derive(Serialize)]
struct PageAssetsResponse {
    resources: Vec<browser::PageResource>,
    truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    limit_reason: Option<&'static str>,
}

impl super::Server {
    async fn handle_observe(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = browser::parse_tab_id("observe", required_str(&args, "tab_id")?)?;
        let action = required_str(&args, "action")?;
        let client = self.client_snapshot();
        let timeout_ms = optional_duration_ms(&args, "timeout_ms", MAX_WAIT_MS)?.unwrap_or(10_000);
        let cursor: Option<crate::observation::ObservationCursor> = args
            .get("cursor")
            .map(|value| serde_json::from_value(value.clone()))
            .transpose()?;
        let result = match action {
            "start" => serde_json::to_value(
                client
                    .start_observation(tab_id, Duration::from_millis(timeout_ms))
                    .await?,
            )?,
            "state" => serde_json::to_value(client.observation_snapshot(tab_id)?)?,
            "stop" => {
                client
                    .stop_file_chooser_interception(tab_id, Duration::from_millis(timeout_ms))
                    .await?;
                client.stop_observation(tab_id);
                json!({"stopped": true})
            }
            "events" | "logs" => {
                let requested = optional_str_array(&args, "methods")?.unwrap_or_default();
                if requested.len() > 16 || requested.iter().any(|s| s.len() > 128) {
                    anyhow::bail!("methods permits at most 16 names of 128 bytes");
                }
                let methods: Vec<&str> = if action == "logs" {
                    vec!["Runtime.consoleAPICalled", "Log.entryAdded"]
                } else {
                    requested.iter().map(String::as_str).collect()
                };
                let limit = optional_u64(&args, "limit")?.unwrap_or(100);
                let bytes = optional_u64(&args, "max_bytes")?.unwrap_or(256 * 1024);
                if !(1..=512).contains(&limit) || !(1..=262_144).contains(&bytes) {
                    anyhow::bail!("limit must be 1..512 and max_bytes 1..262144");
                }
                let batch = client.read_observed_events(
                    tab_id,
                    cursor.as_ref(),
                    &methods,
                    limit as usize,
                    bytes as usize,
                )?;
                let mut result = serde_json::to_value(&batch)?;
                if action == "logs" {
                    let logs: Vec<Value> = batch.events.iter().map(|entry| {
                        let event = &entry.event;
                        if event["method"] == "Log.entryAdded" {
                            json!({"sequence": entry.sequence, "document_epoch": entry.document_epoch,
                                "level": event.pointer("/params/entry/level"),
                                "timestamp": event.pointer("/params/entry/timestamp"),
                                "text": event.pointer("/params/entry/text")})
                        } else {
                            let args = event.pointer("/params/args").and_then(Value::as_array);
                            let text = args.into_iter().flatten().map(|arg| {
                                arg.get("value").map(|v| v.as_str().map(str::to_owned).unwrap_or_else(|| v.to_string()))
                                    .or_else(|| arg.get("description").and_then(Value::as_str).map(str::to_owned))
                                    .unwrap_or_else(|| "[unserializable]".into())
                            }).collect::<Vec<_>>().join(" ");
                            json!({"sequence":entry.sequence,"document_epoch":entry.document_epoch,
                                "level":event.pointer("/params/type"),"timestamp":event.pointer("/params/timestamp"),"text":text})
                        }
                    }).collect();
                    result["logs"] = json!(logs);
                }
                result
            }
            "wait_navigation" => {
                let cursor = cursor.as_ref().ok_or_else(|| {
                    anyhow::anyhow!(
                        "wait_navigation requires the cursor captured before the action"
                    )
                })?;
                let wait_until = serde_json::from_value(
                    args.get("wait_until")
                        .cloned()
                        .unwrap_or_else(|| json!("load")),
                )?;
                let url = args
                    .get("url")
                    .map(|v| {
                        v.as_str()
                            .ok_or_else(|| anyhow::anyhow!("url must be a string"))
                    })
                    .transpose()?;
                serde_json::to_value(
                    client
                        .wait_for_observed_navigation(
                            tab_id,
                            cursor,
                            url,
                            wait_until,
                            Duration::from_millis(timeout_ms),
                        )
                        .await?,
                )?
            }
            _ => anyhow::bail!("unsupported observation action"),
        };
        Ok(vec![Content::text(serde_json::to_string(&result)?)])
    }

    async fn handle_file_chooser(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = browser::parse_tab_id("file_chooser", required_str(&args, "tab_id")?)?;
        let action = required_str(&args, "action")?;
        let client = self.client_snapshot();
        let timeout_ms = optional_duration_ms(&args, "timeout_ms", MAX_WAIT_MS)?.unwrap_or(10_000);
        let deadline = Instant::now() + Duration::from_millis(timeout_ms);
        let result = match action {
            "start" => {
                client
                    .start_file_chooser_interception(tab_id, Duration::from_millis(timeout_ms))
                    .await?;
                json!({"armed":true,"path_scope":"browser_host"})
            }
            "state" => {
                let chooser = client.observation_snapshot(tab_id)?.file_chooser;
                if chooser.is_some() {
                    client
                        .stop_file_chooser_interception(tab_id, Duration::from_millis(timeout_ms))
                        .await?;
                }
                json!({"chooser":chooser,"path_scope":"browser_host"})
            }
            "stop" => {
                client
                    .stop_file_chooser_interception(tab_id, Duration::from_millis(timeout_ms))
                    .await?;
                if let Some(chooser) = client.observation_snapshot(tab_id)?.file_chooser {
                    client.close_observed_file_chooser(tab_id, &chooser.id);
                }
                json!({"armed":false})
            }
            "set_files" => {
                if optional_bool(&args, "colocated_browser")? != Some(true) {
                    anyhow::bail!("file upload requires explicit colocated_browser=true; remote browser-host paths cannot be verified locally");
                }
                let files = required_string_vec(&args, "files")?;
                if files.is_empty() {
                    anyhow::bail!("file chooser requires at least one file");
                }
                let allowed_base = std::env::var_os("CODEX_BRIDGE_UPLOAD_BASE")
                    .map(std::path::PathBuf::from)
                    .ok_or_else(|| {
                        anyhow::anyhow!("file chooser requires configured CODEX_BRIDGE_UPLOAD_BASE")
                    })?;
                let files = crate::file_results::validate_uploads(&files, &allowed_base)?;
                let chooser_id = required_str(&args, "chooser_id")?;
                let chooser = client.validate_file_chooser(tab_id, chooser_id)?;
                if chooser.mode != "selectMultiple" && files.len() != 1 {
                    anyhow::bail!("this chooser accepts one file only");
                }
                let raw = client
                    .execute_cdp_once_with_timeout(
                        tab_id,
                        "DOM.resolveNode",
                        Some(json!({"backendNodeId":chooser.backend_node_id})),
                        deadline.saturating_duration_since(Instant::now()),
                    )
                    .await?;
                let node: Value = serde_json::from_str(raw.get())?;
                let object_id = node
                    .pointer("/object/objectId")
                    .and_then(Value::as_str)
                    .ok_or_else(|| anyhow::anyhow!("chooser node could not be resolved"))?;
                let probe = client.execute_cdp_once_with_timeout(tab_id,"Runtime.callFunctionOn",Some(json!({"objectId":object_id,
                    "functionDeclaration":"function(){return {valid:this.isConnected && this.tagName==='INPUT' && this.type==='file' && !this.disabled,multiple:this.multiple};}","returnByValue":true})), deadline.saturating_duration_since(Instant::now())).await;
                let _ = client
                    .execute_cdp_once_with_timeout(
                        tab_id,
                        "Runtime.releaseObject",
                        Some(json!({"objectId":object_id})),
                        Duration::from_millis(250),
                    )
                    .await;
                let probe = probe?;
                let probe: Value = serde_json::from_str(probe.get())?;
                if probe
                    .pointer("/result/value/valid")
                    .and_then(Value::as_bool)
                    != Some(true)
                    || probe
                        .pointer("/result/value/multiple")
                        .and_then(Value::as_bool)
                        != Some(chooser.mode == "selectMultiple")
                {
                    anyhow::bail!("chooser input is stale, disabled, or changed mode");
                }
                if client
                    .observation_snapshot(tab_id)?
                    .main_frame_id
                    .as_deref()
                    != Some(chooser.frame_id.as_str())
                {
                    anyhow::bail!("file chooser child-frame/OOPIF uploads are unsupported");
                }
                client.validate_file_chooser(tab_id, chooser_id)?;
                let uploaded = client
                    .execute_cdp_once_with_timeout(
                        tab_id,
                        "DOM.setFileInputFiles",
                        Some(json!({"backendNodeId":chooser.backend_node_id,"files":files})),
                        deadline.saturating_duration_since(Instant::now()),
                    )
                    .await;
                client.close_observed_file_chooser(tab_id, chooser_id);
                let stopped = client
                    .stop_file_chooser_interception(tab_id, Duration::from_secs(2))
                    .await;
                uploaded?;
                stopped?;
                json!({"uploaded":files.len(),"path_scope":"browser_host","colocated_browser":true})
            }
            _ => anyhow::bail!("unsupported file chooser action"),
        };
        Ok(vec![Content::text(serde_json::to_string(&result)?)])
    }

    pub(super) async fn handle_tool_call(&self, id: Value, params: Option<Value>) -> String {
        use super::types::{error_response, result_response};

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
            ToolHandler::ListBrowsers => self.handle_list_browsers().await,
            ToolHandler::BindBrowser => self.handle_bind_browser(args).await,
            ToolHandler::BrowserHistory => self.handle_browser_history(args).await,
            ToolHandler::BrowserManagement => self.handle_browser_management(args).await,
            ToolHandler::BrowserViewport => self.handle_browser_viewport(args).await,
            ToolHandler::BrowserVisibility => self.handle_browser_visibility(args).await,
            ToolHandler::Clipboard => self.handle_clipboard(args).await,
            ToolHandler::WebMcp => self.handle_webmcp(args).await,
            ToolHandler::ContentExport => self.handle_content_export(args).await,
            ToolHandler::ListTabs => self.handle_list_tabs().await,
            ToolHandler::CreateTab => self.handle_create_tab().await,
            ToolHandler::CloseTab => self.handle_close_tab(args).await,
            ToolHandler::UserTabs => self.handle_user_tabs().await,
            ToolHandler::ClaimTab => self.handle_claim_tab(args).await,
            ToolHandler::MarkTab => self.handle_mark_tab(args).await,
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
            ToolHandler::CuaMove => self.handle_cua_move(args).await,
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
            ToolHandler::FileInput => self.handle_file_input(args).await,
            ToolHandler::Dialog => self.handle_dialog(args).await,
            ToolHandler::FindElement => self.handle_find_element(args).await,
            ToolHandler::ClickElement => self.handle_click_element(args).await,
            ToolHandler::Locator => self.handle_locator(args).await,
            ToolHandler::NavAndWait => self.handle_nav_and_wait(args).await,
            ToolHandler::ClickAndWait => self.handle_click_and_wait(args).await,
            ToolHandler::FormFill => self.handle_form_fill(args).await,
            ToolHandler::Doctor => self.handle_doctor().await,
            ToolHandler::BringToFront => self.handle_bring_to_front(args).await,
            ToolHandler::GetUrl => self.handle_get_url(args).await,
            ToolHandler::GetTitle => self.handle_get_title(args).await,
            ToolHandler::WaitForElement => self.handle_wait_for_element(args).await,
            ToolHandler::Hover => self.handle_hover(args).await,
            ToolHandler::PrintPdf => self.handle_print_pdf(args).await,
            ToolHandler::Storage => self.handle_storage(args).await,
            ToolHandler::SelectOption => self.handle_select_option(args).await,
            ToolHandler::Drag => self.handle_drag(args).await,
            ToolHandler::ScreenshotElement => self.handle_screenshot_element(args).await,
            ToolHandler::DeleteCookies => self.handle_delete_cookies(args).await,
            ToolHandler::EmulateDevice => self.handle_emulate_device(args).await,
            ToolHandler::NetworkMonitor => self.handle_network_monitor(args).await,
            ToolHandler::ConsoleLogs => self.handle_console_logs(args).await,
            ToolHandler::WaitForUrl => self.handle_wait_for_url(args).await,
            ToolHandler::PerformanceMetrics => self.handle_performance_metrics(args).await,
            ToolHandler::Observe => self.handle_observe(args).await,
            ToolHandler::FileChooser => self.handle_file_chooser(args).await,
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

    async fn handle_list_browsers(&self) -> anyhow::Result<Vec<Content>> {
        let inventory = registry::discover_browsers().await?;
        Ok(vec![Content::text(serde_json::to_string_pretty(
            &inventory,
        )?)])
    }

    async fn handle_bind_browser(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let browser_id = args
            .get("browser_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let family = args
            .get("family")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        if browser_id.is_none() && family.is_none() {
            anyhow::bail!("provide browser_id from codex_list_browsers, or family");
        }
        let end_previous_turn = optional_bool(&args, "end_previous_turn")?.unwrap_or(false);

        // Validate the replacement before any destructive cleanup on the old
        // turn. A missing/ambiguous target must leave existing tabs untouched.
        let selector = BrowserSelector::new(family, browser_id);
        let client = Client::connect_with_selector(None, selector).await?;
        let info = client.send_request("getInfo", None).await?;
        let browser = serde_json::from_str::<Value>(info.get())?;

        let previous = self.client_snapshot();
        let prior_session = if previous.has_live_connection().await {
            let tabs = browser::list_tabs(&previous).await?;
            if !tabs.is_empty() && !end_previous_turn {
                anyhow::bail!(
                    "the current browser session still owns {} tab(s); call codex_finalize first or pass end_previous_turn=true after confirming the official turn cleanup should run",
                    tabs.len()
                );
            }
            if end_previous_turn {
                if !previous.end_turn().await? {
                    anyhow::bail!("Previous browser turn cleanup was not confirmed; keeping the current binding");
                }
                json!({"cleanup": "turn_end", "turn_ended": true})
            } else {
                json!({
                    "cleanup": "connection_only",
                    "owned_tabs_seen": 0
                })
            }
        } else {
            json!({
                "cleanup": "connection_only",
                "owned_tabs_seen": null
            })
        };

        let old = self.replace_client(client);
        self.clipboard.lock().await.clear();
        self.webmcp.clear().await;
        self.page_assets.clear().await;
        old.shutdown().await;

        Ok(vec![Content::text(serde_json::to_string_pretty(&json!({
            "bound": true,
            "browser": browser,
            "previous_session": prior_session
        }))?)])
    }

    async fn handle_list_tabs(&self) -> anyhow::Result<Vec<Content>> {
        let client = self.client_snapshot();
        let tabs = browser::list_tabs(&client).await?;
        Ok(vec![Content::text(serde_json::to_string_pretty(&tabs)?)])
    }

    async fn handle_clipboard(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let action = required_str(&args, "action")?;
        let mut clipboard = self.clipboard.lock().await;
        let result = match action {
            "read" => serde_json::to_value(clipboard.read())?,
            "read_text" => json!(clipboard.read_text()),
            "write_text" => {
                clipboard.write_text(required_string_value(&args, "text")?)?;
                json!({"written": true, "bytes": clipboard.byte_len()})
            }
            "write" => {
                let items = args
                    .get("items")
                    .ok_or_else(|| anyhow::anyhow!("items is required for write"))?;
                clipboard.write(serde_json::from_value(items.clone())?)?;
                json!({"written": true, "bytes": clipboard.byte_len()})
            }
            _ => anyhow::bail!("clipboard action must be read, read_text, write, or write_text"),
        };
        Ok(vec![Content::text(serde_json::to_string(&result)?)])
    }

    async fn handle_webmcp(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let timeout_ms = optional_duration_ms(&args, "timeout_ms", 30_000)?.unwrap_or(10_000);
        let client = self.client_snapshot();
        let result = match required_str(&args, "action")? {
            "list" => {
                self.webmcp
                    .list(&client, required_str(&args, "tab_id")?, timeout_ms)
                    .await?
            }
            "invoke" => {
                self.webmcp
                    .invoke(
                        &client,
                        required_str(&args, "handle")?,
                        args.get("input").cloned().unwrap_or_else(|| json!({})),
                        timeout_ms,
                    )
                    .await?
            }
            _ => anyhow::bail!("WebMCP action must be list or invoke"),
        };
        Ok(vec![Content::text(serde_json::to_string(&result)?)])
    }

    async fn handle_content_export(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let kind = required_str(&args, "kind")?;
        let output_dir = std::path::Path::new(required_str(&args, "output_dir")?);
        let format = args
            .get("format")
            .map(|v| {
                v.as_str()
                    .ok_or_else(|| anyhow::anyhow!("format must be a string"))
            })
            .transpose()?;
        let result =
            crate::export::export(&self.client_snapshot(), tab_id, kind, format, output_dir)
                .await?;
        Ok(vec![Content::text(serde_json::to_string(&result)?)])
    }

    async fn handle_browser_history(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let mut params = serde_json::Map::new();
        if let Some(queries) = optional_str_array(&args, "queries")? {
            params.insert("queries".into(), json!(queries));
        }
        if let Some(limit) = optional_u64(&args, "limit")? {
            if limit == 0 {
                anyhow::bail!("limit must be greater than zero");
            }
            params.insert("limit".into(), json!(limit));
        }
        for key in ["from", "to"] {
            if let Some(value) = args.get(key).and_then(Value::as_str) {
                params.insert(key.into(), json!(value));
            }
        }
        let history =
            browser::get_user_history(&self.client_snapshot(), Value::Object(params)).await?;
        Ok(vec![Content::text(serde_json::to_string_pretty(&history)?)])
    }

    async fn handle_browser_management(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let action = args.get("action").and_then(Value::as_str).unwrap_or("call");
        let result = match action {
            "audit" => browser::browser_management_audit(&self.client_snapshot()).await?,
            "call" => {
                let namespace = required_str(&args, "namespace")?;
                let method = required_str(&args, "method")?;
                let call_args = args
                    .get("args")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                let confirm = optional_bool(&args, "confirm")?.unwrap_or(false);
                browser::browser_management(
                    &self.client_snapshot(),
                    namespace,
                    method,
                    call_args,
                    confirm,
                )
                .await?
            }
            other => anyhow::bail!("Invalid action '{other}': must be 'call' or 'audit'"),
        };
        Ok(vec![Content::text(serde_json::to_string_pretty(&result)?)])
    }

    async fn handle_browser_viewport(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let reset = optional_bool(&args, "reset")?.unwrap_or(false);
        if reset {
            browser::browser_viewport(&self.client_snapshot(), None, None).await?;
            return Ok(vec![Content::text("Browser viewport reset".into())]);
        }

        let width = optional_u64(&args, "width")?;
        let height = optional_u64(&args, "height")?;
        browser::browser_viewport(&self.client_snapshot(), width, height).await?;
        Ok(vec![Content::text(match (width, height) {
            (Some(width), Some(height)) => format!("Browser viewport set to {width}x{height}"),
            _ => "Browser viewport updated".into(),
        })])
    }

    async fn handle_browser_visibility(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let visible = optional_bool(&args, "visible")?;
        let result = browser::browser_visibility(&self.client_snapshot(), visible).await?;
        Ok(vec![Content::text(serde_json::to_string_pretty(&result)?)])
    }

    async fn handle_create_tab(&self) -> anyhow::Result<Vec<Content>> {
        let id = browser::create_tab(&self.client_snapshot()).await?;
        Ok(vec![Content::text(format!("Created tab: {id}"))])
    }

    async fn handle_close_tab(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        browser::close_tab(&self.client_snapshot(), tab_id).await?;
        Ok(vec![Content::text(format!("Closed tab {tab_id}"))])
    }

    async fn handle_user_tabs(&self) -> anyhow::Result<Vec<Content>> {
        let tabs = browser::list_user_tabs(&self.client_snapshot()).await?;
        Ok(vec![Content::text(serde_json::to_string_pretty(&tabs)?)])
    }

    async fn handle_claim_tab(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let expected_title = required_string_value(&args, "expected_title").map_err(|_| {
            anyhow::anyhow!("codex_claim_tab requires expected_title and expected_url from codex_user_tabs; numeric-only claims are no longer accepted")
        })?;
        let expected_url = required_string_value(&args, "expected_url").map_err(|_| {
            anyhow::anyhow!("codex_claim_tab requires expected_title and expected_url from codex_user_tabs; numeric-only claims are no longer accepted")
        })?;
        let tab = browser::claim_user_tab(
            &self.client_snapshot(),
            tab_id,
            expected_title,
            expected_url,
        )
        .await?;
        Ok(vec![Content::text(serde_json::to_string(&tab)?)])
    }

    async fn handle_mark_tab(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let status = required_str(&args, "status")?;
        if !matches!(status, "deliverable" | "handoff") {
            anyhow::bail!("status must be 'deliverable' or 'handoff'");
        }
        browser::mark_tab(&self.client_snapshot(), tab_id, status).await?;
        Ok(vec![Content::text(format!(
            "Tab {tab_id} marked as {status}"
        ))])
    }

    async fn handle_navigate(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let url = required_str(&args, "url")?;
        let url = security::validate_url(url)?;
        browser::navigate(&self.client_snapshot(), tab_id, &url).await?;
        Ok(vec![Content::text(format!(
            "Navigated tab {tab_id} to {url}"
        ))])
    }

    async fn handle_reload(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        browser::reload(&self.client_snapshot(), tab_id).await?;
        Ok(vec![Content::text(format!("Reloaded tab {tab_id}"))])
    }

    async fn handle_navigate_back(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        browser::navigate_back(&self.client_snapshot(), tab_id).await?;
        Ok(vec![Content::text(format!("Navigated tab {tab_id} back"))])
    }

    async fn handle_navigate_forward(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        browser::navigate_forward(&self.client_snapshot(), tab_id).await?;
        Ok(vec![Content::text(format!(
            "Navigated tab {tab_id} forward"
        ))])
    }

    async fn handle_wait_for_load(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let timeout_ms = optional_duration_ms(&args, "timeout_ms", MAX_WAIT_MS)?.unwrap_or(10_000);
        let state = browser::wait_for_load(&self.client_snapshot(), tab_id, timeout_ms).await?;
        Ok(vec![Content::text(format!(
            "Tab {tab_id} reached readyState={state}"
        ))])
    }

    async fn handle_dom_snapshot(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let snapshot = browser::dom_snapshot(&self.client_snapshot(), tab_id).await?;
        Ok(vec![Content::text(snapshot)])
    }

    async fn handle_screenshot(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let full_page = args
            .get("full_page")
            .or_else(|| args.get("fullPage"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let format = args.get("format").and_then(Value::as_str).unwrap_or("png");
        let quality = optional_u64(&args, "quality")?;
        let data = browser::screenshot_with_clip(
            &self.client_snapshot(),
            tab_id,
            full_page,
            format,
            quality,
            args.get("clip").cloned(),
        )
        .await?;
        Ok(vec![
            Content::image_or_summary(data.clone(), mime_for(format), "Screenshot"),
            Content::text(format!(
                "Screenshot captured for tab {tab_id} ({} bytes base64, {format})",
                data.len()
            )),
        ])
    }

    async fn handle_click(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let selector = required_str(&args, "selector")?;
        browser::click(&self.client_snapshot(), tab_id, selector).await?;
        Ok(vec![Content::text(format!(
            "Clicked {selector} in tab {tab_id}"
        ))])
    }

    async fn handle_fill(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let selector = required_str(&args, "selector")?;
        let value = required_string_value(&args, "value")?;
        browser::fill(&self.client_snapshot(), tab_id, selector, value).await?;
        Ok(vec![Content::text(format!(
            "Filled {selector} in tab {tab_id}"
        ))])
    }

    async fn handle_evaluate(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let expression = required_str(&args, "expression")?;
        let raw: Box<RawValue> =
            browser::evaluate(&self.client_snapshot(), tab_id, expression).await?;
        // Surface thrown JS exceptions as tool errors instead of silent empty/partial payloads.
        if let Some(message) = browser::evaluate_exception_message(&raw) {
            anyhow::bail!("evaluate threw: {message}");
        }
        Ok(vec![Content::text(raw.get().to_string())])
    }

    async fn handle_cua_click(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let x = required_f64(&args, "x")?;
        let y = required_f64(&args, "y")?;
        let button = u8::try_from(optional_u64(&args, "button")?.unwrap_or(1))
            .map_err(|_| anyhow::anyhow!("button must be between 1 and 5"))?;
        let click_count = u8::try_from(optional_u64(&args, "click_count")?.unwrap_or(1))
            .map_err(|_| anyhow::anyhow!("click_count must be between 1 and 3"))?;
        let keypress = optional_str_array(&args, "keypress")?.unwrap_or_default();
        browser::cua_click_with_count(
            &self.client_snapshot(),
            tab_id,
            x,
            y,
            button,
            click_count,
            &keypress,
        )
        .await?;
        Ok(vec![Content::text(format!(
            "CUA click button {button}, count {click_count} at ({x},{y}) in tab {tab_id}"
        ))])
    }

    async fn handle_cua_move(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let x = required_f64(&args, "x")?;
        let y = required_f64(&args, "y")?;
        let keys = optional_str_array(&args, "keys")?.unwrap_or_default();
        browser::cua_move(&self.client_snapshot(), tab_id, x, y, &keys).await?;
        Ok(vec![Content::text(format!(
            "CUA mouse moved to ({x},{y}) in tab {tab_id}"
        ))])
    }

    async fn handle_cua_type(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let text = required_str(&args, "text")?;
        browser::cua_type(&self.client_snapshot(), tab_id, text).await?;
        Ok(vec![Content::text(format!(
            "CUA typed text in tab {tab_id}"
        ))])
    }

    async fn handle_cua_keypress(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let keys = required_string_vec(&args, "keys")?;
        if let Some(action) = crate::clipboard::shortcut_action(&keys) {
            let id = browser::parse_tab_id("cua_keypress", tab_id)?;
            let clipboard = self.clipboard.lock().await;
            match action {
                crate::clipboard::ClipboardShortcut::Paste => {
                    clipboard.paste(&self.client_snapshot(), id).await?
                }
                crate::clipboard::ClipboardShortcut::PastePlainText => {
                    clipboard
                        .paste_plain_text(&self.client_snapshot(), id)
                        .await?
                }
            }
            return Ok(vec![Content::text(format!(
                "Virtual clipboard paste in tab {tab_id}"
            ))]);
        }
        browser::cua_keypress(&self.client_snapshot(), tab_id, &keys).await?;
        Ok(vec![Content::text(format!(
            "CUA keypress {keys:?} in tab {tab_id}"
        ))])
    }

    async fn handle_cua_scroll(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let x = required_f64(&args, "x")?;
        let y = required_f64(&args, "y")?;
        let scroll_x = required_alias_f64(&args, "scroll_x", "scrollX")?;
        let scroll_y = required_alias_f64(&args, "scroll_y", "scrollY")?;
        let keypress = optional_str_array(&args, "keypress")?.unwrap_or_default();
        browser::cua_scroll(
            &self.client_snapshot(),
            tab_id,
            x,
            y,
            scroll_x,
            scroll_y,
            &keypress,
        )
        .await?;
        Ok(vec![Content::text(format!(
            "CUA scroll at ({x},{y}) delta ({scroll_x},{scroll_y}) in tab {tab_id}"
        ))])
    }

    async fn handle_dom_get_visible(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let dom = browser::get_visible_dom(&self.client_snapshot(), tab_id).await?;
        Ok(vec![Content::text(dom)])
    }

    async fn handle_dom_click(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let node_id = required_str(&args, "node_id")?;
        let click_count = optional_u64(&args, "click_count")?.unwrap_or(1);
        browser::dom_cua_click_with_count(&self.client_snapshot(), tab_id, node_id, click_count)
            .await?;
        Ok(vec![Content::text(format!(
            "DOM click {click_count} node {node_id} in tab {tab_id}"
        ))])
    }

    async fn handle_name_session(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let name = required_str(&args, "name")?;
        self.client_snapshot()
            .send_request("nameSession", Some(json!({ "name": name })))
            .await?;
        Ok(vec![Content::text(format!("Session named: {name}"))])
    }

    async fn handle_finalize(&self) -> anyhow::Result<Vec<Content>> {
        let client = self.client_snapshot();
        // A resumed shell session may not have dialed yet; the turn it owns is
        // still live in the extension, so connect before ending it.
        client.connect_if_needed().await?;
        if !client.end_turn().await? {
            return Ok(vec![Content::text(
                "No live browser turn to end; remote tab cleanup was not confirmed.".to_string(),
            )]);
        }
        Ok(vec![Content::text(
            "Browser turn ended: unmarked agent tabs were closed, unmarked claimed user tabs were released, and deliverable/handoff marks were preserved."
                .to_string(),
        )])
    }

    async fn handle_get_info(&self) -> anyhow::Result<Vec<Content>> {
        let raw = self.client_snapshot().send_request("getInfo", None).await?;
        let mut info = serde_json::from_str::<Value>(raw.get()).unwrap_or_else(|_| {
            json!({
                "raw": raw.get()
            })
        });
        if let Some(obj) = info.as_object_mut() {
            let runtime_key = if obj.contains_key("bridge") {
                "codexBridge"
            } else {
                "bridge"
            };
            obj.insert(runtime_key.into(), self.bridge_runtime_info());
        } else {
            info = json!({
                "raw": info,
                "bridge": self.bridge_runtime_info()
            });
        }
        Ok(vec![Content::text(serde_json::to_string_pretty(&info)?)])
    }

    async fn handle_execute_cdp(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let method = required_str(&args, "method")?;
        let params = args.get("params").cloned();
        let raw: Box<RawValue> =
            browser::execute_cdp_generic(&self.client_snapshot(), tab_id, method, params).await?;
        Ok(vec![Content::text(raw.get().to_string())])
    }

    async fn handle_page_assets(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        if let Some(action) = args.get("action") {
            use crate::page_assets::{BundleOptions, DownloadMediaOptions};
            let action = action
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("action must be a string"))?;
            let client = self.client_snapshot();
            let result = match action {
                "list" => serde_json::to_value(self.page_assets.list(&client, tab_id).await?)?,
                "bundle" => {
                    let kinds = args
                        .get("kinds")
                        .map(|v| serde_json::from_value(v.clone()))
                        .transpose()?;
                    serde_json::to_value(
                        self.page_assets
                            .bundle(
                                &client,
                                tab_id,
                                BundleOptions {
                                    inventory_id: required_str(&args, "inventory_id")?.to_string(),
                                    output_dir: required_str(&args, "output_dir")?.into(),
                                    asset_ids: optional_str_array(&args, "asset_ids")?,
                                    kinds,
                                    max_resource_bytes: optional_u64(&args, "max_resource_bytes")?
                                        .unwrap_or(0)
                                        .try_into()?,
                                    max_total_bytes: optional_u64(&args, "max_total_bytes")?
                                        .unwrap_or(0)
                                        .try_into()?,
                                },
                            )
                            .await?,
                    )?
                }
                "download_media" => serde_json::to_value(
                    self.page_assets
                        .download_media(
                            &client,
                            tab_id,
                            DownloadMediaOptions {
                                output_dir: required_str(&args, "output_dir")?.into(),
                                url: args
                                    .get("url")
                                    .map(|v| {
                                        v.as_str()
                                            .map(str::to_owned)
                                            .ok_or_else(|| anyhow::anyhow!("url must be a string"))
                                    })
                                    .transpose()?,
                                selector: args
                                    .get("selector")
                                    .map(|v| {
                                        v.as_str().map(str::to_owned).ok_or_else(|| {
                                            anyhow::anyhow!("selector must be a string")
                                        })
                                    })
                                    .transpose()?,
                                max_bytes: optional_u64(&args, "max_resource_bytes")?
                                    .unwrap_or(0)
                                    .try_into()?,
                            },
                        )
                        .await?,
                )?,
                _ => anyhow::bail!("page_assets action must be list, bundle, or download_media"),
            };
            return Ok(vec![Content::text(serde_json::to_string(&result)?)]);
        }
        // Existing published no-action callers retain their bounded resource
        // response; inventory/file-result operations are explicit above.
        let include_content = optional_bool(&args, "include_content")?.unwrap_or(false);
        let types: Option<Vec<String>> = optional_str_array(&args, "types")?;

        let mut resources = browser::get_resource_tree(&self.client_snapshot(), tab_id).await?;

        if let Some(ref types) = types {
            resources.retain(|resource| {
                types
                    .iter()
                    .any(|type_filter| resource.resource_type.eq_ignore_ascii_case(type_filter))
            });
        }

        if include_content {
            let max_resources = optional_u64(&args, "max_resources")?
                .unwrap_or(DEFAULT_PAGE_ASSET_MAX_RESOURCES)
                .min(MAX_PAGE_ASSET_RESOURCES) as usize;
            let max_total_bytes = optional_u64(&args, "max_total_bytes")?
                .unwrap_or(DEFAULT_PAGE_ASSET_MAX_TOTAL_BYTES)
                .min(MAX_PAGE_ASSET_TOTAL_BYTES);
            let mut truncated = false;
            let mut limit_reason = None;
            let fetch_deadline =
                Instant::now() + Duration::from_millis(PAGE_ASSET_FETCH_TOTAL_TIMEOUT_MS);

            if resources.len() > max_resources {
                resources.truncate(max_resources);
                truncated = true;
                limit_reason = Some("max_resources");
            }

            let mut total_bytes = 0_u64;
            for resource in resources.iter_mut() {
                if Instant::now() >= fetch_deadline {
                    truncated = true;
                    limit_reason = Some("fetch_timeout");
                    resource.failed = Some(true);
                    break;
                }

                let Some(resource_size) = resource.size else {
                    truncated = true;
                    limit_reason = Some("unknown_resource_size");
                    resource.failed = Some(true);
                    continue;
                };
                if resource_size > MAX_PAGE_ASSET_PER_RESOURCE_BYTES {
                    truncated = true;
                    limit_reason = Some("max_resource_bytes");
                    resource.failed = Some(true);
                    continue;
                }
                if total_bytes.saturating_add(resource_size) > max_total_bytes {
                    truncated = true;
                    limit_reason = Some("max_total_bytes");
                    resource.failed = Some(true);
                    break;
                }

                let frame_id = resource.frame_id.clone();
                let remaining = fetch_deadline.saturating_duration_since(Instant::now());
                let per_resource_timeout =
                    remaining.min(Duration::from_millis(PAGE_ASSET_FETCH_TIMEOUT_MS));
                match browser::get_resource_content_with_timeout(
                    &self.client_snapshot(),
                    tab_id,
                    &frame_id,
                    &resource.url,
                    per_resource_timeout,
                )
                .await
                {
                    Ok(content) => {
                        let content_bytes = content.len() as u64;
                        if total_bytes.saturating_add(content_bytes) > max_total_bytes {
                            truncated = true;
                            limit_reason = Some("max_total_bytes");
                            resource.failed = Some(true);
                            break;
                        }
                        total_bytes += content_bytes;
                        resource.content = Some(content);
                    }
                    Err(err) => {
                        resource.failed = Some(true);
                        if matches!(err, crate::error::BridgeError::Timeout(_)) {
                            truncated = true;
                            limit_reason = Some("fetch_timeout");
                        }
                        tracing::debug!(
                            "resource content fetch failed for {} (frame={}): {err}",
                            sanitize_for_log(&resource.url),
                            sanitize_for_log(&frame_id),
                        );
                    }
                }
            }

            return Ok(vec![Content::text(serde_json::to_string_pretty(
                &PageAssetsResponse {
                    resources,
                    truncated,
                    limit_reason,
                },
            )?)]);
        }

        Ok(vec![Content::text(serde_json::to_string_pretty(
            &resources,
        )?)])
    }

    async fn handle_network_cookies(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let urls: Option<Vec<String>> = optional_str_array(&args, "urls")?;
        let redact = optional_bool(&args, "redact_values")?.unwrap_or(true);

        let urls = urls
            .map(|urls| {
                urls.into_iter()
                    .map(|url| security::validate_url(&url))
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?;

        let mut cookies =
            browser::get_cookies(&self.client_snapshot(), tab_id, urls.as_deref()).await?;

        if redact {
            for cookie in cookies.iter_mut() {
                cookie.value = "[redacted]".to_string();
            }
        }

        Ok(vec![Content::text(serde_json::to_string_pretty(&cookies)?)])
    }

    async fn handle_network_set_cookie(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let name = security::validate_cookie_name(required_str(&args, "name")?)?;
        let value = security::validate_cookie_value(required_str(&args, "value")?)?;

        let normalized_url = args
            .get("url")
            .and_then(Value::as_str)
            .map(security::validate_url)
            .transpose()?;

        let mut cookie_params = json!({
            "name": name,
            "value": value,
        });

        if let Some(obj) = cookie_params.as_object_mut() {
            if let Some(url) = normalized_url {
                obj.insert("url".into(), json!(url));
            }
            if let Some(domain) = args.get("domain").and_then(Value::as_str) {
                let domain = security::validate_cookie_domain(domain)?;
                obj.insert("domain".into(), json!(domain));
            }
            if let Some(path) = args.get("path").and_then(Value::as_str) {
                let path = security::validate_cookie_path(path)?;
                obj.insert("path".into(), json!(path));
            }
            if let Some(http_only) = args.get("httpOnly").and_then(Value::as_bool) {
                obj.insert("httpOnly".into(), json!(http_only));
            }
            if let Some(secure) = args.get("secure").and_then(Value::as_bool) {
                obj.insert("secure".into(), json!(secure));
            }
            if let Some(same_site) = args.get("sameSite").and_then(Value::as_str) {
                let same_site = security::validate_cookie_same_site(same_site)?;
                obj.insert("sameSite".into(), json!(same_site));
            }
        }

        browser::set_cookie(&self.client_snapshot(), tab_id, cookie_params).await?;
        Ok(vec![Content::text(format!(
            "Cookie '{name}' set successfully"
        ))])
    }

    async fn handle_file_input(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let selector = required_str(&args, "selector")?;
        let files = required_str_array(&args, "files")?;

        let upload_base = std::env::var("CODEX_BRIDGE_UPLOAD_BASE").map_err(|_| {
            anyhow::anyhow!("CODEX_BRIDGE_UPLOAD_BASE must be set to enable codex_file_input")
        })?;
        let allowed_base = std::fs::canonicalize(&upload_base).map_err(|err| {
            anyhow::anyhow!(
                "Cannot resolve CODEX_BRIDGE_UPLOAD_BASE '{}': {err}",
                sanitize_for_log(&upload_base)
            )
        })?;
        if !allowed_base.is_dir() {
            anyhow::bail!(
                "CODEX_BRIDGE_UPLOAD_BASE '{}' is not a directory",
                sanitize_for_log(&upload_base)
            );
        }

        // Validate every file path
        let validated: Vec<String> = files
            .iter()
            .map(|f| {
                security::validate_file_path(f, &allowed_base)
                    .map(|p| p.to_string_lossy().into_owned())
            })
            .collect::<Result<Vec<_>, _>>()?;

        browser::file_input(&self.client_snapshot(), tab_id, selector, &validated).await?;
        Ok(vec![Content::text(format!(
            "Uploaded {} file(s) to {selector} in tab {tab_id}",
            validated.len()
        ))])
    }

    async fn handle_dialog(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let action = required_str(&args, "action")?;

        let client = self.client_snapshot();
        let id = browser::parse_tab_id("dialog", tab_id)?;
        if action == "get" {
            let dialog = client.observation_snapshot(id)?.dialog;
            return Ok(vec![Content::text(serde_json::to_string(
                &json!({"dialog":dialog}),
            )?)]);
        }
        if action != "accept" && action != "dismiss" {
            anyhow::bail!("Invalid action '{action}': must be 'get', 'accept' or 'dismiss'");
        }

        let prompt_text = args
            .get("prompt_text")
            .and_then(Value::as_str)
            .map(|s| s.to_string());

        if prompt_text.is_some() && action != "accept" {
            anyhow::bail!("prompt_text is only valid with action='accept'");
        }

        let dialog_id = args
            .get("dialog_id")
            .map(|v| {
                v.as_str()
                    .ok_or_else(|| anyhow::anyhow!("dialog_id must be a string"))
            })
            .transpose()?;
        if let Some(dialog_id) = dialog_id {
            let dialog = client.validate_dialog(id, dialog_id)?;
            if prompt_text.is_some() && dialog.dialog_type != "prompt" {
                anyhow::bail!("prompt_text requires a prompt dialog");
            }
        }
        let handled = browser::handle_dialog(&client, tab_id, action, prompt_text.as_deref()).await;
        if let Some(dialog_id) = dialog_id {
            client.close_observed_dialog(id, dialog_id);
        }
        handled?;
        Ok(vec![Content::text(format!(
            "Dialog {action}ed in tab {tab_id}"
        ))])
    }

    async fn handle_find_element(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let role = args.get("role").and_then(Value::as_str);
        let name = args.get("name").and_then(Value::as_str);
        let max_results = optional_u64(&args, "max_results")?.unwrap_or(10).min(50) as usize; // cap at 50

        if role.is_none() && name.is_none() {
            anyhow::bail!("At least one of 'role' or 'name' is required");
        }

        let matches =
            browser::find_elements(&self.client_snapshot(), tab_id, role, name, max_results)
                .await?;

        Ok(vec![Content::text(serde_json::to_string_pretty(&matches)?)])
    }

    async fn handle_click_element(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let node_id = required_str(&args, "node_id")?;
        browser::click_ax_element(&self.client_snapshot(), tab_id, node_id).await?;
        Ok(vec![Content::text(format!(
            "Clicked element {node_id} in tab {tab_id}"
        ))])
    }

    async fn handle_locator(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let action = required_str(&args, "action")?;
        crate::locator::validate_spec(&args)?;
        let client = self.client_snapshot();
        if action == "wait" {
            let timeout_ms =
                optional_duration_ms(&args, "timeout_ms", MAX_WAIT_MS)?.unwrap_or(10_000);
            let state = args
                .get("state")
                .and_then(Value::as_str)
                .unwrap_or("visible");
            if !matches!(state, "visible" | "attached" | "hidden" | "detached") {
                anyhow::bail!("state must be visible, attached, hidden, or detached");
            }
            let deadline = Instant::now() + Duration::from_millis(timeout_ms);
            let mut probe = args.clone();
            probe["action"] = json!("probe");
            probe["wait_state"] = json!(state);
            loop {
                let result = tokio::time::timeout_at(
                    deadline,
                    browser::locator(&client, tab_id, probe.clone()),
                )
                .await
                .map_err(|_| anyhow::anyhow!("locator wait timed out ({state})"))??;
                if result.get("matched").and_then(Value::as_bool) == Some(true) {
                    return Ok(vec![Content::text(serde_json::to_string(
                        &json!({"wait_state":state,"matched":true,"probe":result}),
                    )?)]);
                }
                if Instant::now() >= deadline {
                    anyhow::bail!("locator wait timed out ({state})");
                }
                tokio::time::sleep_until(
                    (Instant::now() + Duration::from_millis(100)).min(deadline),
                )
                .await;
            }
        }
        if action == "press" && args.get("trial") != Some(&json!(true)) {
            let keys = required_string_vec(&args, "keys")?;
            if let Some(shortcut) = crate::clipboard::shortcut_action(&keys) {
                let mut focus = args.clone();
                focus["action"] = json!("focus");
                let result = browser::locator(&client, tab_id, focus).await?;
                if result.get("focused").and_then(Value::as_bool) != Some(true) {
                    anyhow::bail!("locator target did not receive focus");
                }
                let id = browser::parse_tab_id("locator.press", tab_id)?;
                let clipboard = self.clipboard.lock().await;
                match shortcut {
                    crate::clipboard::ClipboardShortcut::Paste => {
                        clipboard.paste(&client, id).await?
                    }
                    crate::clipboard::ClipboardShortcut::PastePlainText => {
                        clipboard.paste_plain_text(&client, id).await?
                    }
                }
                return Ok(vec![Content::text(serde_json::to_string(&result)?)]);
            }
        }
        let result = browser::locator(&client, tab_id, args.clone()).await?;
        Ok(vec![Content::text(serde_json::to_string(&result)?)])
    }

    async fn handle_nav_and_wait(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let url = required_str(&args, "url")?;
        let timeout_ms = optional_duration_ms(&args, "timeout_ms", MAX_WAIT_MS)?.unwrap_or(30_000);
        let wait_until = serde_json::from_value(
            args.get("wait_until")
                .cloned()
                .unwrap_or_else(|| json!("load")),
        )?;
        browser::navigation_action_and_wait(
            &self.client_snapshot(),
            tab_id,
            Some(url),
            None,
            timeout_ms,
            wait_until,
        )
        .await?;
        Ok(vec![Content::text(format!(
            "Navigated to {url} and reached {wait_until:?} in tab {tab_id}"
        ))])
    }

    async fn handle_click_and_wait(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let selector = required_str(&args, "selector")?;
        let timeout_ms = optional_duration_ms(&args, "timeout_ms", MAX_WAIT_MS)?.unwrap_or(10_000);
        let wait_until = serde_json::from_value(
            args.get("wait_until")
                .cloned()
                .unwrap_or_else(|| json!("load")),
        )?;
        browser::navigation_action_and_wait(
            &self.client_snapshot(),
            tab_id,
            None,
            Some(selector),
            timeout_ms,
            wait_until,
        )
        .await?;
        Ok(vec![Content::text(format!(
            "Clicked {selector} and waited in tab {tab_id}"
        ))])
    }

    async fn handle_form_fill(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let fields = args
            .get("fields")
            .ok_or_else(|| anyhow::anyhow!("missing required argument: fields"))?;
        let submit = args.get("submit").and_then(Value::as_str);
        let delay_ms = optional_duration_ms(&args, "delay_ms", MAX_FORM_DELAY_MS)?.unwrap_or(50);
        browser::form_fill(&self.client_snapshot(), tab_id, fields, submit, delay_ms).await?;
        Ok(vec![Content::text(format!("Form filled in tab {tab_id}"))])
    }

    async fn handle_doctor(&self) -> anyhow::Result<Vec<Content>> {
        let result = crate::doctor::run_diagnostics().await;
        Ok(vec![Content::text(serde_json::to_string_pretty(&result)?)])
    }

    async fn handle_bring_to_front(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        browser::bring_to_front(&self.client_snapshot(), tab_id).await?;
        Ok(vec![Content::text(format!(
            "Tab {tab_id} brought to front"
        ))])
    }

    async fn handle_get_url(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let url = browser::get_url(&self.client_snapshot(), tab_id).await?;
        Ok(vec![Content::text(url)])
    }

    async fn handle_get_title(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let title = browser::get_title(&self.client_snapshot(), tab_id).await?;
        Ok(vec![Content::text(title)])
    }

    async fn handle_wait_for_element(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let selector = required_str(&args, "selector")?;
        let timeout_ms = optional_duration_ms(&args, "timeout_ms", MAX_WAIT_MS)?.unwrap_or(10_000);
        browser::wait_for_element(&self.client_snapshot(), tab_id, selector, timeout_ms).await?;
        Ok(vec![Content::text(format!(
            "Element {selector} found in tab {tab_id}"
        ))])
    }

    async fn handle_hover(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let selector = required_str(&args, "selector")?;
        browser::hover(&self.client_snapshot(), tab_id, selector).await?;
        Ok(vec![Content::text(format!(
            "Hovered {selector} in tab {tab_id}"
        ))])
    }

    async fn handle_print_pdf(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let base64_len = browser::print_pdf(&self.client_snapshot(), tab_id).await?;
        Ok(vec![Content::text(format!(
            "PDF generated for tab {tab_id} ({} bytes base64). Content is streamed and not embedded in the MCP response.",
            base64_len
        ))])
    }

    async fn handle_storage(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let key = required_str(&args, "key")?;
        let action = args.get("action").and_then(Value::as_str).unwrap_or("get");
        let storage_type = args
            .get("storage_type")
            .and_then(Value::as_str)
            .unwrap_or("local");
        match action {
            "get" => {
                let val = browser::storage_get(&self.client_snapshot(), tab_id, key, storage_type)
                    .await?;
                Ok(vec![Content::text(val.unwrap_or_else(|| "null".into()))])
            }
            "set" => {
                let value = required_string_value(&args, "value")?;
                browser::storage_set(&self.client_snapshot(), tab_id, key, value, storage_type)
                    .await?;
                Ok(vec![Content::text(format!(
                    "{storage_type}Storage[{key}] set"
                ))])
            }
            other => anyhow::bail!("Invalid action '{other}': must be 'get' or 'set'"),
        }
    }

    async fn handle_select_option(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let selector = required_str(&args, "selector")?;
        let value = required_string_value(&args, "value")?;
        browser::select_option(&self.client_snapshot(), tab_id, selector, value).await?;
        Ok(vec![Content::text(format!(
            "Selected {value} on {selector} in tab {tab_id}"
        ))])
    }

    async fn handle_drag(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let legacy = ["from_x", "from_y", "to_x", "to_y"]
            .iter()
            .any(|name| args.get(*name).is_some());
        let path = if let Some(path) = args.get("path") {
            if legacy {
                anyhow::bail!("path cannot be combined with legacy from_x/from_y/to_x/to_y");
            }
            let points = path
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("path must be an array of {{x,y}} points"))?;
            if points.len() > 1_000 {
                anyhow::bail!("path supports at most 1000 points");
            }
            points
                .iter()
                .enumerate()
                .map(|(index, point)| {
                    let x = point
                        .get("x")
                        .and_then(Value::as_f64)
                        .ok_or_else(|| anyhow::anyhow!("path[{index}].x must be a number"))?;
                    let y = point
                        .get("y")
                        .and_then(Value::as_f64)
                        .ok_or_else(|| anyhow::anyhow!("path[{index}].y must be a number"))?;
                    Ok((x, y))
                })
                .collect::<anyhow::Result<Vec<_>>>()?
        } else {
            vec![
                (
                    required_f64(&args, "from_x")?,
                    required_f64(&args, "from_y")?,
                ),
                (required_f64(&args, "to_x")?, required_f64(&args, "to_y")?),
            ]
        };
        let keys = optional_str_array(&args, "keys")?.unwrap_or_default();
        browser::drag_path(&self.client_snapshot(), tab_id, path.clone(), &keys).await?;
        Ok(vec![Content::text(format!(
            "Dragged through {} points in tab {tab_id}",
            path.len()
        ))])
    }

    async fn handle_screenshot_element(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let selector = required_str(&args, "selector")?;
        let data = browser::screenshot_element(&self.client_snapshot(), tab_id, selector).await?;
        Ok(vec![
            Content::image_or_summary(data.clone(), "image/png", "Element screenshot"),
            Content::text(format!(
                "Element screenshot of {selector} in tab {tab_id} ({} bytes base64)",
                data.len()
            )),
        ])
    }

    async fn handle_delete_cookies(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let name = security::validate_cookie_name(required_str(&args, "name")?)?;
        let mut params = json!({ "name": name });
        if let Some(obj) = params.as_object_mut() {
            if let Some(url) = args.get("url").and_then(Value::as_str) {
                let url = security::validate_url(url)?;
                obj.insert("url".into(), json!(url));
            }
            if let Some(domain) = args.get("domain").and_then(Value::as_str) {
                let domain = security::validate_cookie_domain(domain)?;
                obj.insert("domain".into(), json!(domain));
            }
            if let Some(path) = args.get("path").and_then(Value::as_str) {
                let path = security::validate_cookie_path(path)?;
                obj.insert("path".into(), json!(path));
            }
        }
        browser::delete_cookies(&self.client_snapshot(), tab_id, params).await?;
        Ok(vec![Content::text(format!("Deleted cookies named {name}"))])
    }

    async fn handle_emulate_device(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let reset = optional_bool(&args, "reset")?.unwrap_or(false);
        if reset {
            browser::reset_device(&self.client_snapshot(), tab_id).await?;
            return Ok(vec![Content::text(format!(
                "Device emulation cleared for tab {tab_id}"
            ))]);
        }
        let width = optional_u64(&args, "width")?.unwrap_or(390) as i64;
        let height = optional_u64(&args, "height")?.unwrap_or(844) as i64;
        let mobile = optional_bool(&args, "mobile")?.unwrap_or(true);
        let user_agent = args
            .get("user_agent")
            .and_then(Value::as_str)
            .unwrap_or(
                "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1",
            );
        browser::emulate_device(
            &self.client_snapshot(),
            tab_id,
            width,
            height,
            user_agent,
            mobile,
        )
        .await?;
        Ok(vec![Content::text(format!(
            "Emulating {width}x{height} (mobile={mobile}) in tab {tab_id}. Call with reset=true to clear."
        ))])
    }

    async fn handle_network_monitor(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let duration_ms =
            optional_duration_ms(&args, "duration_ms", MAX_CAPTURE_MS)?.unwrap_or(5_000);
        let filter = args
            .get("url_filter")
            .map(|v| {
                v.as_str().filter(|s| s.len() <= 2048).ok_or_else(|| {
                    anyhow::anyhow!("url_filter must be a string of at most 2048 bytes")
                })
            })
            .transpose()?;
        let mut result =
            browser::network_monitor(&self.client_snapshot(), tab_id, duration_ms).await?;
        if let Some(filter) = filter {
            if let Some(requests) = result.get_mut("requests").and_then(Value::as_array_mut) {
                requests.retain(|r| {
                    r.get("url")
                        .and_then(Value::as_str)
                        .is_some_and(|url| url.contains(filter))
                });
                let count = requests.len();
                result["request_count"] = json!(count);
                result["url_filter"] = json!(filter);
            }
        }
        Ok(vec![Content::text(serde_json::to_string_pretty(&result)?)])
    }

    async fn handle_console_logs(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let duration_ms =
            optional_duration_ms(&args, "duration_ms", MAX_CAPTURE_MS)?.unwrap_or(5_000);
        let result = browser::console_logs(&self.client_snapshot(), tab_id, duration_ms).await?;
        Ok(vec![Content::text(serde_json::to_string_pretty(&result)?)])
    }

    async fn handle_wait_for_url(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let pattern = required_str(&args, "pattern")?;
        let timeout_ms = optional_duration_ms(&args, "timeout_ms", MAX_WAIT_MS)?.unwrap_or(10_000);
        browser::wait_for_url(&self.client_snapshot(), tab_id, pattern, timeout_ms).await?;
        Ok(vec![Content::text(format!(
            "URL matched {pattern} in tab {tab_id}"
        ))])
    }

    async fn handle_performance_metrics(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let result = browser::performance_metrics(&self.client_snapshot(), tab_id).await?;
        Ok(vec![Content::text(serde_json::to_string_pretty(&result)?)])
    }
}
