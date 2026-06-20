use serde_json::{json, value::RawValue, Value};

use crate::browser;
use crate::security;

use super::types::{
    optional_bool, optional_str_array, optional_u64, required_i64, required_str,
    required_str_array, required_string_value, required_string_vec, sanitize_for_log, Content,
    ToolHandler,
};

impl super::Server {
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
            ToolHandler::FileInput => self.handle_file_input(args).await,
            ToolHandler::Dialog => self.handle_dialog(args).await,
            ToolHandler::FindElement => self.handle_find_element(args).await,
            ToolHandler::ClickElement => self.handle_click_element(args).await,
            ToolHandler::NavAndWait => self.handle_nav_and_wait(args).await,
            ToolHandler::ClickAndWait => self.handle_click_and_wait(args).await,
            ToolHandler::FormFill => self.handle_form_fill(args).await,
            ToolHandler::Doctor => self.handle_doctor().await,
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
        let full_page = args
            .get("full_page")
            .or_else(|| args.get("fullPage"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
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
        self.client.clear_attachments().await;
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
            security::validate_url(url)?;
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

    async fn handle_file_input(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let selector = required_str(&args, "selector")?;
        let files = required_str_array(&args, "files")?;

        // Determine upload base: env var or server's current directory
        let allowed_base = std::env::var("CODEX_BRIDGE_UPLOAD_BASE")
            .ok()
            .map(std::path::PathBuf::from)
            .or_else(|| std::env::current_dir().ok())
            .ok_or_else(|| anyhow::anyhow!("Cannot determine upload base directory"))?;

        // Validate every file path
        let validated: Vec<String> = files
            .iter()
            .map(|f| {
                security::validate_file_path(f, &allowed_base)
                    .map(|p| p.to_string_lossy().into_owned())
            })
            .collect::<Result<Vec<_>, _>>()?;

        browser::file_input(&self.client, tab_id, selector, &validated).await?;
        Ok(vec![Content::text(format!(
            "Uploaded {} file(s) to {selector} in tab {tab_id}",
            validated.len()
        ))])
    }

    async fn handle_dialog(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let action = required_str(&args, "action")?;

        if action != "accept" && action != "dismiss" {
            anyhow::bail!("Invalid action '{action}': must be 'accept' or 'dismiss'");
        }

        let prompt_text = args
            .get("prompt_text")
            .and_then(Value::as_str)
            .map(|s| s.to_string());

        if prompt_text.is_some() && action != "accept" {
            anyhow::bail!("prompt_text is only valid with action='accept'");
        }

        browser::handle_dialog(
            &self.client,
            tab_id,
            action,
            prompt_text.as_deref(),
        )
        .await?;
        Ok(vec![Content::text(format!(
            "Dialog {action}ed in tab {tab_id}"
        ))])
    }

    async fn handle_find_element(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let role = args.get("role").and_then(Value::as_str);
        let name = args.get("name").and_then(Value::as_str);
        let max_results = optional_u64(&args, "max_results")?
            .unwrap_or(10)
            .min(50) as usize; // cap at 50

        if role.is_none() && name.is_none() {
            anyhow::bail!("At least one of 'role' or 'name' is required");
        }

        let matches = browser::find_elements(
            &self.client, tab_id, role, name, max_results,
        ).await?;

        Ok(vec![Content::text(
            serde_json::to_string_pretty(&matches)?,
        )])
    }

    async fn handle_click_element(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let node_id = required_str(&args, "node_id")?;
        browser::click_ax_element(&self.client, tab_id, node_id).await?;
        Ok(vec![Content::text(format!(
            "Clicked element {node_id} in tab {tab_id}"
        ))])
    }

    async fn handle_nav_and_wait(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let url = required_str(&args, "url")?;
        let timeout_ms = optional_u64(&args, "timeout_ms")?.unwrap_or(30_000);
        browser::nav_and_wait(&self.client, tab_id, url, timeout_ms).await?;
        Ok(vec![Content::text(format!(
            "Navigated to {url} and loaded in tab {tab_id}"
        ))])
    }

    async fn handle_click_and_wait(&self, args: Value) -> anyhow::Result<Vec<Content>> {
        let tab_id = required_str(&args, "tab_id")?;
        let selector = required_str(&args, "selector")?;
        let timeout_ms = optional_u64(&args, "timeout_ms")?.unwrap_or(10_000);
        browser::click_and_wait(&self.client, tab_id, selector, timeout_ms).await?;
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
        let delay_ms = optional_u64(&args, "delay_ms")?.unwrap_or(50);
        browser::form_fill(&self.client, tab_id, fields, submit, delay_ms).await?;
        Ok(vec![Content::text(format!(
            "Form filled in tab {tab_id}"
        ))])
    }

    async fn handle_doctor(&self) -> anyhow::Result<Vec<Content>> {
        let result = crate::doctor::run_diagnostics().await;
        Ok(vec![Content::text(
            serde_json::to_string_pretty(&result)?,
        )])
    }
}
