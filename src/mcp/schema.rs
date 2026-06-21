use serde_json::Value;

use super::types::{object_schema, schema_value, Tool, ToolHandler};

pub(super) fn registered_tools() -> Vec<Tool> {
    vec![
        Tool::new("codex_list_tabs", "[Tabs] List tabs owned by this bridge session. These are tabs created by or claimed by the bridge — not all browser tabs. Use codex_user_tabs to list all browser tabs available for claiming.", object_schema(), ToolHandler::ListTabs),
        Tool::new("codex_create_tab", "[Tabs] Create a new blank browser tab. The tab starts at about:blank; use codex_navigate afterward to load a URL.", object_schema(), ToolHandler::CreateTab),
        Tool::new("codex_close_tab", "[Tabs] Close a browser tab by ID. The tab must be owned by the current bridge session.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string","description":"Tab ID to close"}},"required":["tab_id"]}"#), ToolHandler::CloseTab),
        Tool::new("codex_user_tabs", "[Tabs] List all open tabs across all browser windows, including tabs NOT owned by the bridge. Use this to discover tabs available for claiming via codex_claim_tab. The tab IDs returned here can be passed to codex_claim_tab.", object_schema(), ToolHandler::UserTabs),
        Tool::new("codex_claim_tab", "[Tabs] Claim an existing user tab for automation. The tab_id must come from codex_user_tabs. After claiming, the tab can be controlled by other codex_* tools. This transfers ownership to the bridge session.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string","description":"Tab ID from codex_user_tabs to claim"}},"required":["tab_id"]}"#), ToolHandler::ClaimTab),
        Tool::new("codex_navigate", "[Navigation] Navigate a tab to a URL. Blocks dangerous schemes (file:, javascript:, data:, etc.).", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"url":{"type":"string","description":"Full URL to navigate to (https://...) "}},"required":["tab_id","url"]}"#), ToolHandler::Navigate),
        Tool::new("codex_reload", "[Navigation] Reload the current page in a tab.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"}},"required":["tab_id"]}"#), ToolHandler::Reload),
        Tool::new("codex_navigate_back", "[Navigation] Navigate a tab one entry back in its session history.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"}},"required":["tab_id"]}"#), ToolHandler::NavigateBack),
        Tool::new("codex_navigate_forward", "[Navigation] Navigate a tab one entry forward in its session history.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"}},"required":["tab_id"]}"#), ToolHandler::NavigateForward),
        Tool::new("codex_wait_for_load", "[Navigation] Wait for page load to complete by polling document.readyState. Useful after codex_navigate on slow or JS-heavy pages.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"timeout_ms":{"type":"integer","description":"Max wait in milliseconds. Defaults to 10000."}},"required":["tab_id"]}"#), ToolHandler::WaitForLoad),
        Tool::new("codex_dom_snapshot", "[DOM] Get the full accessibility tree of a tab. Returns structured accessibility nodes with IDs usable by codex_dom_click. For a simpler human-readable tree, use codex_dom_get_visible.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"}},"required":["tab_id"]}"#), ToolHandler::DomSnapshot),
        Tool::new("codex_screenshot", "[Page] Capture a viewport screenshot as a PNG image. full_page is reserved for future full-page capture (not yet implemented). If the call times out, the tab is likely background-throttled by Chrome — call codex_bring_to_front first, then retry.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"full_page":{"type":"boolean","description":"Reserved for future full-page capture. Always captures viewport currently."}},"required":["tab_id"]}"#), ToolHandler::Screenshot),
        Tool::new("codex_click", "[Input] Click an element by CSS selector. Uses JavaScript click(); prefer codex_dom_click or codex_cua_click for complex pages where JS click() may not trigger real event listeners.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"selector":{"type":"string","description":"CSS selector, e.g. #login-btn or .submit-button"}},"required":["tab_id","selector"]}"#), ToolHandler::Click),
        Tool::new("codex_fill", "[Input] Fill a form input by CSS selector. Sets the value, triggers input and change events. Returns clear error if selector not found.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"selector":{"type":"string"},"value":{"type":"string"}},"required":["tab_id","selector","value"]}"#), ToolHandler::Fill),
        Tool::new("codex_evaluate", "[Page] Execute arbitrary JavaScript in the page context and return the result as JSON. Use for data extraction, state inspection, or actions not covered by dedicated tools.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"expression":{"type":"string","description":"JavaScript to evaluate, e.g. \"document.title\" or \"JSON.stringify(window.__STATE__)\""}},"required":["tab_id","expression"]}"#), ToolHandler::Evaluate),
        Tool::new("codex_cua_click", "[Input] Click at exact screen coordinates (x, y). Sends real mouse events via CDP Input.dispatchMouseEvent — more reliable than JavaScript click() for complex UI.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"x":{"type":"integer"},"y":{"type":"integer"}},"required":["tab_id","x","y"]}"#), ToolHandler::CuaClick),
        Tool::new("codex_cua_type", "[Input] Type text at the current keyboard focus. For filling specific inputs, use codex_fill instead.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"text":{"type":"string"}},"required":["tab_id","text"]}"#), ToolHandler::CuaType),
        Tool::new("codex_cua_keypress", "[Input] Press a sequence of keyboard keys. Each key fires keyDown then keyUp events.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"keys":{"type":"array","items":{"type":"string"},"description":"Keys to press, e.g. [\"Enter\"] or [\"Control\", \"c\"]"}},"required":["tab_id","keys"]}"#), ToolHandler::CuaKeypress),
        Tool::new("codex_cua_scroll", "[Input] Scroll at the given coordinates by delta amounts.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"x":{"type":"integer"},"y":{"type":"integer"},"scroll_x":{"type":"integer"},"scroll_y":{"type":"integer"}},"required":["tab_id","x","y","scroll_x","scroll_y"]}"#), ToolHandler::CuaScroll),
        Tool::new("codex_dom_get_visible", "[DOM] Get a human-readable visible DOM tree (tag names, IDs, classes, text). Use for quick page structure inspection without the full accessibility tree. For node IDs usable with codex_dom_click, use codex_dom_snapshot instead.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"}},"required":["tab_id"]}"#), ToolHandler::DomGetVisible),
        Tool::new("codex_dom_click", "[DOM] Click a DOM node by its accessibility node ID (from codex_dom_snapshot). Uses real CDP mouse events at the element's bounding box center — more reliable than CSS selector click().", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"node_id":{"type":"string","description":"Accessibility node ID from codex_dom_snapshot output"}},"required":["tab_id","node_id"]}"#), ToolHandler::DomClick),
        Tool::new("codex_name_session", "[Session] Assign a human-readable name to this browser session for debugging.", schema_value(r#"{"type":"object","properties":{"name":{"type":"string"}},"required":["name"]}"#), ToolHandler::NameSession),
        Tool::new("codex_finalize", "[Session] Finalize the session: clean up all tabs owned by the bridge and release resources. Call when done with browser automation.", object_schema(), ToolHandler::Finalize),
        Tool::new("codex_get_info", "[Session] Get metadata about the Codex extension backend (version, capabilities, extension ID). Use for diagnostics.", object_schema(), ToolHandler::GetInfo),
        Tool::new("codex_execute_cdp", "[CDP] Execute a raw Chrome DevTools Protocol command. This is the universal escape hatch — use for CDP methods not covered by dedicated codex_* tools. Safety: blocks Browser.*, Debugger.*, Target.*, and other dangerous domains. Prefer dedicated tools when available (e.g. codex_network_cookies instead of execute_cdp with Network.getCookies).", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string","description":"Tab ID to execute on"},"method":{"type":"string","description":"CDP method, e.g. \"Runtime.evaluate\", \"Page.captureScreenshot\", \"Network.enable\""},"params":{"type":"object","description":"CDP method parameters as a JSON object"}},"required":["tab_id","method"]}"#), ToolHandler::ExecuteCdp),
        Tool::new("codex_page_assets", "[Page] List page resources (images, fonts, CSS, JS, etc.). Optionally fetch content as base64. More efficient than execute_cdp with Page.getResourceTree — uses the Codex extension's native pageAssets capability.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string","description":"Tab ID to inspect"},"include_content":{"type":"boolean","description":"Fetch each resource's content as base64. Defaults to false."},"types":{"type":"array","items":{"type":"string"},"description":"Filter: Image, Stylesheet, Script, Font, Document, Media, Manifest, Fetch, Other"}},"required":["tab_id"]}"#), ToolHandler::PageAssets),
        Tool::new("codex_network_cookies", "[Network] Read cookies for the current page or specific URLs. Cookie values are REDACTED by default for security (set redact_values: false to see raw values). Preferred over execute_cdp with Network.getCookies.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string","description":"Tab ID"},"urls":{"type":"array","items":{"type":"string"},"description":"Optional URL list to filter. Omit to get all cookies for the current page."},"redact_values":{"type":"boolean","description":"Redact cookie values for security. Default: true."}},"required":["tab_id"]}"#), ToolHandler::NetworkCookies),
        Tool::new("codex_network_set_cookie", "[Network] Set a browser cookie. Use this for cookie manipulation; for reading cookies, use codex_network_cookies.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string","description":"Tab ID"},"name":{"type":"string","description":"Cookie name"},"value":{"type":"string","description":"Cookie value"},"url":{"type":"string","description":"URL to associate cookie with"},"domain":{"type":"string","description":"Cookie domain"},"path":{"type":"string","description":"Cookie path"},"httpOnly":{"type":"boolean","description":"HttpOnly flag"},"secure":{"type":"boolean","description":"Secure flag"},"sameSite":{"type":"string","description":"Strict, Lax, or None"}},"required":["tab_id","name","value"]}"#), ToolHandler::NetworkSetCookie),
        Tool::new("codex_file_input", "[Input] Upload files to a <input type=file> element. First finds the element by CSS selector, then sets the specified files via DOM.setFileInputFiles. Paths must be absolute and within the allowed upload directory (set via CODEX_BRIDGE_UPLOAD_BASE env var, defaults to current directory). Security: path traversal blocked, only regular files, max 10 MB per file.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string","description":"Tab ID or 'active'"},"selector":{"type":"string","description":"CSS selector for the file input element"},"files":{"type":"array","items":{"type":"string"},"description":"Absolute file paths to upload"}},"required":["tab_id","selector","files"]}"#), ToolHandler::FileInput),
        Tool::new("codex_dialog", "[Page] Handle a JavaScript dialog (alert, confirm, prompt). Use action='accept' to accept (with optional prompt_text for prompt dialogs), or action='dismiss' to dismiss. Only one dialog can be active at a time per tab.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string","description":"Tab ID or 'active'"},"action":{"type":"string","enum":["accept","dismiss"],"description":"Accept or dismiss the dialog"},"prompt_text":{"type":"string","description":"Text to enter for prompt dialogs (only valid with accept)"}},"required":["tab_id","action"]}"#), ToolHandler::Dialog),
        Tool::new("codex_find_element", "[DOM] Find elements by ARIA role and/or accessible name in the page's accessibility tree. Returns matching elements with node IDs for use with codex_click_element. Provide at least one of role or name. Examples: role='button', name='submit', role='link' with name='login'. More reliable than CSS selectors.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string","description":"Tab ID or 'active'"},"role":{"type":"string","description":"ARIA role, case-insensitive exact match (e.g. 'button', 'link', 'textbox', 'checkbox', 'heading')"},"name":{"type":"string","description":"Accessible name, case-insensitive substring match"},"max_results":{"type":"integer","description":"Maximum results (default 10, max 50)"}},"required":["tab_id"]}"#), ToolHandler::FindElement),
        Tool::new("codex_click_element", "[Input] Click an element by its accessibility node ID from codex_find_element. Uses CDP DOM.resolveNode → DOM.getBoxModel → Input dispatch (no JS injection). Safer than CSS selector clicking.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string","description":"Tab ID or 'active'"},"node_id":{"type":"string","description":"Accessibility node ID from codex_find_element"}},"required":["tab_id","node_id"]}"#), ToolHandler::ClickElement),
        Tool::new("codex_nav_and_wait", "[Navigation] Navigate to a URL and wait for the page to load. Combines codex_navigate + codex_wait_for_load in one call — use this instead of two separate calls.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string","description":"Tab ID or 'active'"},"url":{"type":"string","description":"Full URL to navigate to"},"timeout_ms":{"type":"integer","description":"Max wait in ms. Defaults to 30000."}},"required":["tab_id","url"]}"#), ToolHandler::NavAndWait),
        Tool::new("codex_click_and_wait", "[Input] Click an element by CSS selector and wait for page load. Combines codex_click + codex_wait_for_load in one call.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string","description":"Tab ID or 'active'"},"selector":{"type":"string","description":"CSS selector to click"},"timeout_ms":{"type":"integer","description":"Max wait in ms. Defaults to 10000."}},"required":["tab_id","selector"]}"#), ToolHandler::ClickAndWait),
        Tool::new("codex_form_fill", "[Input] Fill multiple form fields at once. Accepts a map of CSS selector to value. Optionally clicks a submit button after filling. Sequential dispatch with configurable delay.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string","description":"Tab ID or 'active'"},"fields":{"type":"object","description":"Map of CSS selector to value"},"submit":{"type":"string","description":"Optional CSS selector for submit button to click after filling"},"delay_ms":{"type":"integer","description":"Delay between field inputs in ms. Defaults to 50."}},"required":["tab_id","fields"]}"#), ToolHandler::FormFill),
        Tool::new("codex_doctor", "[Session] Run self-diagnostics. Checks pipe connectivity, Chrome availability, and reports bridge version. Use before browser operations to verify the environment is ready. Returns diagnostic summary with per-pipe health.", object_schema(), ToolHandler::Doctor),
        Tool::new("codex_bring_to_front", "[Page] Activate a tab and bring it to the foreground via Page.bringToFront. Call this before screenshot or other CDP calls when a tab has been in the background — Chrome throttles/discards background tabs and CDP calls on a suspended tab can time out silently. Does not navigate or change page state.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string","description":"Tab ID or 'active' to activate"}},"required":["tab_id"]}"#), ToolHandler::BringToFront),
    ]
}

pub(super) fn tools_to_values(tools: &[Tool]) -> Vec<Value> {
    tools
        .iter()
        .map(|tool| {
            serde_json::json!({
                "name": tool.name,
                "description": tool.description,
                "inputSchema": tool.input_schema
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registered_tools_keep_go_order() {
        let names: Vec<_> = registered_tools()
            .into_iter()
            .map(|tool| tool.name)
            .collect();
        assert_eq!(names.first(), Some(&"codex_list_tabs"));
        assert_eq!(names.last(), Some(&"codex_bring_to_front"));
        assert_eq!(names.len(), 37);
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
    fn all_tool_schemas_are_valid() {
        for tool in registered_tools() {
            assert_eq!(tool.input_schema["type"], "object",
                "tool {} schema must be object type", tool.name);
        }
    }

    #[test]
    fn bring_to_front_schema_requires_tab_id() {
        let tools = registered_tools();
        let bf = tools.iter().find(|t| t.name == "codex_bring_to_front").unwrap();
        assert_eq!(bf.input_schema["type"], "object");
        let required: Vec<_> = bf.input_schema["required"].as_array().unwrap().iter().filter_map(|v| v.as_str()).collect();
        assert!(required.contains(&"tab_id"));
    }

    #[test]
    fn file_input_schema_requires_tab_id_selector_files() {
        let tools = registered_tools();
        let fi = tools.iter().find(|t| t.name == "codex_file_input").unwrap();
        assert_eq!(fi.input_schema["type"], "object");
        let required: Vec<_> = fi.input_schema["required"].as_array().unwrap().iter().filter_map(|v| v.as_str()).collect();
        assert!(required.contains(&"tab_id"));
        assert!(required.contains(&"selector"));
        assert!(required.contains(&"files"));
    }

    #[test]
    fn dialog_schema_requires_tab_id_action() {        let tools = registered_tools();
        let d = tools.iter().find(|t| t.name == "codex_dialog").unwrap();
        assert_eq!(d.input_schema["type"], "object");
        let required: Vec<_> = d.input_schema["required"].as_array().unwrap().iter().filter_map(|v| v.as_str()).collect();
        assert!(required.contains(&"tab_id"));
        assert!(required.contains(&"action"));
    }
}
