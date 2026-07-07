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
        Tool::new("codex_dom_snapshot", "[DOM] Get the full accessibility tree of a tab. Returns structured accessibility nodes with IDs usable by codex_dom_click. Large text responses are bounded by CODEX_BRIDGE_MAX_TEXT_BYTES; for a simpler human-readable tree, use codex_dom_get_visible.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"}},"required":["tab_id"]}"#), ToolHandler::DomSnapshot),
        Tool::new("codex_screenshot", "[Page] Capture a viewport screenshot as a PNG image (default) or JPEG/WebP. JPEG supports a quality param (0-100, default 80) to cut size for token-sensitive agents. Oversized image payloads return a text summary instead of partial base64. If the call times out, the tab is likely background-throttled — call codex_bring_to_front first, then retry.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"format":{"type":"string","enum":["png","jpeg","webp"],"description":"Image format. Default png."},"quality":{"type":"integer","description":"JPEG quality 0-100. Ignored for png/webp. Default 80."},"full_page":{"type":"boolean","description":"Reserved. Always captures viewport."}},"required":["tab_id"]}"#), ToolHandler::Screenshot),
        Tool::new("codex_click", "[Input] Click an element by CSS selector. Uses JavaScript click(); prefer codex_dom_click or codex_cua_click for complex pages where JS click() may not trigger real event listeners.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"selector":{"type":"string","description":"CSS selector, e.g. #login-btn or .submit-button"}},"required":["tab_id","selector"]}"#), ToolHandler::Click),
        Tool::new("codex_fill", "[Input] Fill a form input by CSS selector. Sets the value, triggers input and change events. Returns clear error if selector not found.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"selector":{"type":"string"},"value":{"type":"string"}},"required":["tab_id","selector","value"]}"#), ToolHandler::Fill),
        Tool::new("codex_evaluate", "[Page] Execute arbitrary JavaScript in the page context and return the result as bounded JSON text. Use for data extraction, state inspection, or actions not covered by dedicated tools.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"expression":{"type":"string","description":"JavaScript to evaluate, e.g. \"document.title\" or \"JSON.stringify(window.__STATE__)\""}},"required":["tab_id","expression"]}"#), ToolHandler::Evaluate),
        Tool::new("codex_cua_click", "[Input] Click at exact screen coordinates (x, y). Sends real mouse events via CDP Input.dispatchMouseEvent — more reliable than JavaScript click() for complex UI.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"x":{"type":"integer"},"y":{"type":"integer"}},"required":["tab_id","x","y"]}"#), ToolHandler::CuaClick),
        Tool::new("codex_cua_type", "[Input] Type text at the current keyboard focus. For filling specific inputs, use codex_fill instead.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"text":{"type":"string"}},"required":["tab_id","text"]}"#), ToolHandler::CuaType),
        Tool::new("codex_cua_keypress", "[Input] Press a sequence of keyboard keys. Each key fires keyDown then keyUp events.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"keys":{"type":"array","items":{"type":"string"},"description":"Keys to press, e.g. [\"Enter\"] or [\"Control\", \"c\"]"}},"required":["tab_id","keys"]}"#), ToolHandler::CuaKeypress),
        Tool::new("codex_cua_scroll", "[Input] Scroll at the given coordinates by delta amounts.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"x":{"type":"integer"},"y":{"type":"integer"},"scroll_x":{"type":"integer"},"scroll_y":{"type":"integer"}},"required":["tab_id","x","y","scroll_x","scroll_y"]}"#), ToolHandler::CuaScroll),
        Tool::new("codex_dom_get_visible", "[DOM] Get a human-readable visible DOM tree (tag names, IDs, classes, text). Use for quick page structure inspection without the full accessibility tree. For node IDs usable with codex_dom_click, use codex_dom_snapshot instead.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"}},"required":["tab_id"]}"#), ToolHandler::DomGetVisible),
        Tool::new("codex_dom_click", "[DOM] Click a DOM node by its accessibility node ID (from codex_dom_snapshot). Uses real CDP mouse events at the element's bounding box center — more reliable than CSS selector click().", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"node_id":{"type":"string","description":"Accessibility node ID from codex_dom_snapshot output"}},"required":["tab_id","node_id"]}"#), ToolHandler::DomClick),
        Tool::new("codex_name_session", "[Session] Assign a human-readable name to this browser session for debugging.", schema_value(r#"{"type":"object","properties":{"name":{"type":"string"}},"required":["name"]}"#), ToolHandler::NameSession),
        Tool::new("codex_finalize", "[Session] Finalize the session: clean up all tabs owned by the bridge and release resources. Call when done with browser automation.", object_schema(), ToolHandler::Finalize),
        Tool::new("codex_get_info", "[Session] Get Codex extension backend metadata plus a bridge runtime metadata field: version, active profile, tool count, upload-base configured status, response caps, extension capabilities, and extension ID. Use for diagnostics and agent self-orientation.", object_schema(), ToolHandler::GetInfo),
        Tool::new("codex_execute_cdp", "[CDP] Execute an explicitly allowlisted Chrome DevTools Protocol diagnostic command. Use for inspect/diagnostic methods not covered by dedicated codex_* tools. Safety: raw domains are not wildcard-open; navigation, cookies, screenshots, PDF, file upload, page resource content, event-producing enable calls, arbitrary Runtime JS, and destructive methods must use bounded dedicated tools. Text output is bounded by CODEX_BRIDGE_MAX_TEXT_BYTES.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string","description":"Tab ID to execute on"},"method":{"type":"string","description":"Explicitly allowlisted CDP method, e.g. \"DOM.getDocument\", \"Page.getLayoutMetrics\", \"Performance.getMetrics\""},"params":{"type":"object","description":"CDP method parameters as a JSON object"}},"required":["tab_id","method"]}"#), ToolHandler::ExecuteCdp),
        Tool::new("codex_page_assets", "[Page] List page resources (images, fonts, CSS, JS, etc.). Optionally fetch known-size content as base64 with bounded max_resources/max_total_bytes plus per-resource and total fetch timeouts. More efficient than execute_cdp with Page.getResourceTree — uses the Codex extension's native pageAssets capability.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string","description":"Tab ID to inspect"},"include_content":{"type":"boolean","description":"Fetch known-size resources as base64. Defaults to false."},"types":{"type":"array","items":{"type":"string"},"description":"Filter: Image, Stylesheet, Script, Font, Document, Media, Manifest, Fetch, Other"},"max_resources":{"type":"integer","description":"Max resources to fetch when include_content=true. Default 50, max 200."},"max_total_bytes":{"type":"integer","description":"Max total base64 content bytes when include_content=true. Default 1048576, max 5242880."}},"required":["tab_id"]}"#), ToolHandler::PageAssets),
        Tool::new("codex_network_cookies", "[Network] Read cookies for the current page or specific URLs. Cookie values are REDACTED by default for security (set redact_values: false to see raw values). Preferred over execute_cdp with Network.getCookies.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string","description":"Tab ID"},"urls":{"type":"array","items":{"type":"string"},"description":"Optional URL list to filter. Omit to get all cookies for the current page."},"redact_values":{"type":"boolean","description":"Redact cookie values for security. Default: true."}},"required":["tab_id"]}"#), ToolHandler::NetworkCookies),
        Tool::new("codex_network_set_cookie", "[Network] Set a browser cookie. Use this for cookie manipulation; for reading cookies, use codex_network_cookies.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string","description":"Tab ID"},"name":{"type":"string","description":"Cookie name"},"value":{"type":"string","description":"Cookie value"},"url":{"type":"string","description":"URL to associate cookie with"},"domain":{"type":"string","description":"Cookie domain"},"path":{"type":"string","description":"Cookie path"},"httpOnly":{"type":"boolean","description":"HttpOnly flag"},"secure":{"type":"boolean","description":"Secure flag"},"sameSite":{"type":"string","description":"Strict, Lax, or None"}},"required":["tab_id","name","value"]}"#), ToolHandler::NetworkSetCookie),
        Tool::new("codex_file_input", "[Input] Upload files to a <input type=file> element. First finds the element by CSS selector, then sets the specified files via DOM.setFileInputFiles. Requires CODEX_BRIDGE_UPLOAD_BASE; paths must be absolute and within that allowed upload directory. Security: path traversal blocked, only regular files, max 10 MB per file.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string","description":"Numeric tab ID"},"selector":{"type":"string","description":"CSS selector for the file input element"},"files":{"type":"array","items":{"type":"string"},"description":"Absolute file paths to upload"}},"required":["tab_id","selector","files"]}"#), ToolHandler::FileInput),
        Tool::new("codex_dialog", "[Page] Handle a JavaScript dialog (alert, confirm, prompt). Use action='accept' to accept (with optional prompt_text for prompt dialogs), or action='dismiss' to dismiss. Only one dialog can be active at a time per tab.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string","description":"Numeric tab ID"},"action":{"type":"string","enum":["accept","dismiss"],"description":"Accept or dismiss the dialog"},"prompt_text":{"type":"string","description":"Text to enter for prompt dialogs (only valid with accept)"}},"required":["tab_id","action"]}"#), ToolHandler::Dialog),
        Tool::new("codex_find_element", "[DOM] Find elements by ARIA role and/or accessible name in the page's accessibility tree. Returns matching elements with node IDs for use with codex_click_element. Provide at least one of role or name. Examples: role='button', name='submit', role='link' with name='login'. More reliable than CSS selectors.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string","description":"Numeric tab ID"},"role":{"type":"string","description":"ARIA role, case-insensitive exact match (e.g. 'button', 'link', 'textbox', 'checkbox', 'heading')"},"name":{"type":"string","description":"Accessible name, case-insensitive substring match"},"max_results":{"type":"integer","description":"Maximum results (default 10, max 50)"}},"required":["tab_id"]}"#), ToolHandler::FindElement),
        Tool::new("codex_click_element", "[Input] Click an element by its accessibility node ID from codex_find_element. Uses CDP DOM.resolveNode → DOM.getBoxModel → Input dispatch (no JS injection). Safer than CSS selector clicking.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string","description":"Numeric tab ID"},"node_id":{"type":"string","description":"Accessibility node ID from codex_find_element"}},"required":["tab_id","node_id"]}"#), ToolHandler::ClickElement),
        Tool::new("codex_nav_and_wait", "[Navigation] Navigate to a URL and wait for the page to load. Combines codex_navigate + codex_wait_for_load in one call — use this instead of two separate calls.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string","description":"Numeric tab ID"},"url":{"type":"string","description":"Full URL to navigate to"},"timeout_ms":{"type":"integer","description":"Max wait in ms. Defaults to 30000."}},"required":["tab_id","url"]}"#), ToolHandler::NavAndWait),
        Tool::new("codex_click_and_wait", "[Input] Click an element by CSS selector and wait for page load. Combines codex_click + codex_wait_for_load in one call.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string","description":"Numeric tab ID"},"selector":{"type":"string","description":"CSS selector to click"},"timeout_ms":{"type":"integer","description":"Max wait in ms. Defaults to 10000."}},"required":["tab_id","selector"]}"#), ToolHandler::ClickAndWait),
        Tool::new("codex_form_fill", "[Input] Fill multiple form fields at once. Accepts a map of CSS selector to value. Optionally clicks a submit button after filling. Sequential dispatch with configurable delay.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string","description":"Numeric tab ID"},"fields":{"type":"object","description":"Map of CSS selector to value"},"submit":{"type":"string","description":"Optional CSS selector for submit button to click after filling"},"delay_ms":{"type":"integer","description":"Delay between field inputs in ms. Defaults to 50."}},"required":["tab_id","fields"]}"#), ToolHandler::FormFill),
        Tool::new("codex_doctor", "[Session] Run self-diagnostics. Checks pipe connectivity, Chrome availability, install path, and bridge version. Use before browser operations to verify the environment is ready. Returns a diagnostic summary plus a bounded per-pipe sample.", object_schema(), ToolHandler::Doctor),
        Tool::new("codex_bring_to_front", "[Page] Activate a tab and bring it to the foreground via Page.bringToFront. Call this before screenshot or other CDP calls when a tab has been in the background — Chrome throttles/discards background tabs and CDP calls on a suspended tab can time out silently. Does not navigate or change page state.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string","description":"Numeric tab ID to activate"}},"required":["tab_id"]}"#), ToolHandler::BringToFront),
        Tool::new("codex_get_url", "[Page] Get the current URL of a tab via location.href. Cheaper than codex_evaluate for this common read.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"}},"required":["tab_id"]}"#), ToolHandler::GetUrl),
        Tool::new("codex_get_title", "[Page] Get the current document.title of a tab.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"}},"required":["tab_id"]}"#), ToolHandler::GetTitle),
        Tool::new("codex_wait_for_element", "[Navigation] Poll until a CSS selector matches. Use this instead of codex_wait_for_load on SPAs where the URL does not change but content renders asynchronously. Returns error on timeout.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"selector":{"type":"string","description":"CSS selector to wait for"},"timeout_ms":{"type":"integer","description":"Max wait in ms. Defaults to 10000."}},"required":["tab_id","selector"]}"#), ToolHandler::WaitForElement),
        Tool::new("codex_hover", "[Input] Hover over an element by CSS selector. Dispatches mouseover + mousemove. Needed for dropdown menus, tooltips, and hover-revealed cards that do not respond to click.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"selector":{"type":"string"}},"required":["tab_id","selector"]}"#), ToolHandler::Hover),
        Tool::new("codex_print_pdf", "[Page] Render the current page to PDF via Page.printToPDF (A4, backgrounds on). Uses CDP ReturnAsStream with bounded IO.read chunks and returns only a size summary; PDF bytes are not embedded in the response.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"}},"required":["tab_id"]}"#), ToolHandler::PrintPdf),
        Tool::new("codex_storage", "[Network] Read or write Web Storage. action='get' returns the value (or null); action='set' writes it. storage_type: 'local' (default) or 'session'. Useful for login state, tokens, and SPA app state stored client-side.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"key":{"type":"string","description":"Storage key"},"value":{"type":"string","description":"Value to set (action=set only)"},"action":{"type":"string","enum":["get","set"],"description":"Read or write. Default: get"},"storage_type":{"type":"string","enum":["local","session"],"description":"localStorage or sessionStorage. Default: local."}},"required":["tab_id","key"]}"#), ToolHandler::Storage),
        Tool::new("codex_select_option", "[Input] Set a select element value and fire change/input events. Use instead of codex_fill for select tags — plain fill does not reliably trigger change handlers.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"selector":{"type":"string","description":"CSS selector for the select element"},"value":{"type":"string","description":"Option value to select"}},"required":["tab_id","selector","value"]}"#), ToolHandler::SelectOption),
        Tool::new("codex_drag", "[Input] Drag from one point to another via CDP mouse events (mouseDown, interpolated mouseMove, mouseUp). For sliders, sortable lists, drag-drop.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"from_x":{"type":"integer"},"from_y":{"type":"integer"},"to_x":{"type":"integer"},"to_y":{"type":"integer"}},"required":["tab_id","from_x","from_y","to_x","to_y"]}"#), ToolHandler::Drag),
        Tool::new("codex_screenshot_element", "[Page] Capture a screenshot clipped to a single element's bounding box. Use to verify one component's rendering without the full page. Oversized image payloads return a text summary instead of partial base64.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"selector":{"type":"string","description":"CSS selector for the element to capture"}},"required":["tab_id","selector"]}"#), ToolHandler::ScreenshotElement),
        Tool::new("codex_delete_cookies", "[Network] Delete cookies by name via Network.deleteCookies. Optionally scope by url/domain/path. Use for logout or account-switch testing.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"name":{"type":"string","description":"Cookie name to delete"},"url":{"type":"string","description":"Optional URL scope"},"domain":{"type":"string","description":"Optional domain scope"},"path":{"type":"string","description":"Optional path scope"}},"required":["tab_id","name"]}"#), ToolHandler::DeleteCookies),
        Tool::new("codex_emulate_device", "[Page] Override the viewport to emulate a device via Emulation.setDeviceMetricsOverride. Defaults to iPhone (390x844). Pass reset=true (Emulation.clearDeviceMetricsOverride) to revert to the real viewport.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"width":{"type":"integer","description":"Viewport width. Default 390."},"height":{"type":"integer","description":"Viewport height. Default 844."},"mobile":{"type":"boolean","description":"Treat as mobile. Default true."},"user_agent":{"type":"string","description":"User-Agent string. Default iPhone Safari."},"reset":{"type":"boolean","description":"Clear emulation and revert to real viewport."}},"required":["tab_id"]}"#), ToolHandler::EmulateDevice),
        Tool::new("codex_network_monitor", "[Network] Capture network requests for a duration, pairing request and response into a structured list. Each entry: {request_id, url, method, status, mime_type}. Enables Network domain, collects for duration_ms, then disables. Use to debug API calls, inspect XHR/fetch traffic, or reverse-engineer endpoints.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"duration_ms":{"type":"integer","description":"Capture window in ms. Default 5000."}},"required":["tab_id"]}"#), ToolHandler::NetworkMonitor),
        Tool::new("codex_console_logs", "[Page] Capture console.* output for a duration. Enables Runtime, collects Runtime.consoleAPICalled events, then disables. Returns raw log entries. Use to debug frontend errors and log output.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"duration_ms":{"type":"integer","description":"Capture window in ms. Default 5000."}},"required":["tab_id"]}"#), ToolHandler::ConsoleLogs),
        Tool::new("codex_wait_for_url", "[Navigation] Poll until location.href contains a substring. For SPAs that change the URL on route change without a full page navigation. Returns error on timeout.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"pattern":{"type":"string","description":"Substring to match in the URL"},"timeout_ms":{"type":"integer","description":"Max wait in ms. Default 10000."}},"required":["tab_id","pattern"]}"#), ToolHandler::WaitForUrl),
        Tool::new("codex_performance_metrics", "[Page] Get Chrome Performance metrics via Performance.getMetrics — DOM node count, JS heap size, document count, event listener count, etc. Use to diagnose page weight and memory.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"}},"required":["tab_id"]}"#), ToolHandler::PerformanceMetrics),
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
        assert_eq!(names.last(), Some(&"codex_performance_metrics"));
        assert_eq!(names.len(), 52);
    }

    #[test]
    fn execute_cdp_schema_requires_tab_id_and_method() {
        let tools = registered_tools();
        let cdp = tools
            .iter()
            .find(|t| t.name == "codex_execute_cdp")
            .unwrap();
        assert_eq!(cdp.input_schema["type"], "object");
        let required: Vec<_> = cdp.input_schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        assert!(
            required.contains(&"tab_id"),
            "required should contain tab_id"
        );
        assert!(
            required.contains(&"method"),
            "required should contain method"
        );
    }

    #[test]
    fn page_assets_schema_requires_tab_id() {
        let tools = registered_tools();
        let pa = tools
            .iter()
            .find(|t| t.name == "codex_page_assets")
            .unwrap();
        assert_eq!(pa.input_schema["type"], "object");
        let required = pa.input_schema["required"].as_array().unwrap();
        assert_eq!(required[0], "tab_id");
    }

    #[test]
    fn network_cookies_schema_requires_tab_id() {
        let tools = registered_tools();
        let nc = tools
            .iter()
            .find(|t| t.name == "codex_network_cookies")
            .unwrap();
        assert_eq!(nc.input_schema["type"], "object");
        let required = nc.input_schema["required"].as_array().unwrap();
        assert_eq!(required[0], "tab_id");
    }

    #[test]
    fn network_set_cookie_schema_requires_name_value() {
        let tools = registered_tools();
        let nsc = tools
            .iter()
            .find(|t| t.name == "codex_network_set_cookie")
            .unwrap();
        assert_eq!(nsc.input_schema["type"], "object");
        let required: Vec<_> = nsc.input_schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        assert!(required.contains(&"tab_id"));
        assert!(required.contains(&"name"));
        assert!(required.contains(&"value"));
    }

    #[test]
    fn all_tool_schemas_are_valid() {
        for tool in registered_tools() {
            assert_eq!(
                tool.input_schema["type"], "object",
                "tool {} schema must be object type",
                tool.name
            );
        }
    }

    #[test]
    fn bring_to_front_schema_requires_tab_id() {
        let tools = registered_tools();
        let bf = tools
            .iter()
            .find(|t| t.name == "codex_bring_to_front")
            .unwrap();
        assert_eq!(bf.input_schema["type"], "object");
        let required: Vec<_> = bf.input_schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        assert!(required.contains(&"tab_id"));
    }

    #[test]
    fn file_input_schema_requires_tab_id_selector_files() {
        let tools = registered_tools();
        let fi = tools.iter().find(|t| t.name == "codex_file_input").unwrap();
        assert_eq!(fi.input_schema["type"], "object");
        let required: Vec<_> = fi.input_schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        assert!(required.contains(&"tab_id"));
        assert!(required.contains(&"selector"));
        assert!(required.contains(&"files"));
    }

    #[test]
    fn dialog_schema_requires_tab_id_action() {
        let tools = registered_tools();
        let d = tools.iter().find(|t| t.name == "codex_dialog").unwrap();
        assert_eq!(d.input_schema["type"], "object");
        let required: Vec<_> = d.input_schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        assert!(required.contains(&"tab_id"));
        assert!(required.contains(&"action"));
    }
}
