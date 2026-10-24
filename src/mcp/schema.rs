use serde_json::Value;

use super::types::{object_schema, schema_value, Tool, ToolHandler, ToolHints};

pub(super) fn registered_tools() -> Vec<Tool> {
    vec![
        Tool::new("codex_observe", "[Observation] Start or read bounded tab/document observation: frames, navigation epoch, active dialog, persistent events and rendered logs. Start before the action to capture earlier events. Cursor gaps and truncation are explicit; untagged events are not mixed into tab history. wait_navigation requires a cursor armed before the action. stop clears local observation without disabling shared CDP domains.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"action":{"type":"string","enum":["start","state","events","logs","wait_navigation","stop"]},"cursor":{"type":"object","properties":{"generation":{"type":"string"},"sequence":{"type":"integer","minimum":0}},"required":["generation","sequence"]},"methods":{"type":"array","items":{"type":"string"},"maxItems":16},"limit":{"type":"integer","minimum":1,"maximum":512},"max_bytes":{"type":"integer","minimum":1,"maximum":262144},"url":{"type":"string","description":"Optional exact navigation URL; redirects may end elsewhere."},"wait_until":{"type":"string","enum":["commit","domcontentloaded","load"]},"timeout_ms":{"type":"integer","minimum":1,"maximum":60000}},"required":["tab_id","action"]}"#), ToolHandler::Observe).with_hints(ToolHints::interaction()),
        Tool::new("codex_file_chooser", "[Files] Arm file-chooser interception before clicking input, inspect document-bound chooser ID/multiple mode, set files or stop. Upload requires colocated_browser=true and configured CODEX_BRIDGE_UPLOAD_BASE. Remote browser-host uploads and browser-level download completion are unsupported.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"action":{"type":"string","enum":["start","state","set_files","stop"]},"chooser_id":{"type":"string"},"files":{"type":"array","items":{"type":"string"},"maxItems":32,"minItems":1},"timeout_ms":{"type":"integer","minimum":1,"maximum":60000},"colocated_browser":{"type":"boolean","description":"Required true for uploads; uses configured CODEX_BRIDGE_UPLOAD_BASE. Remote browser-host paths cannot be verified locally."}},"required":["tab_id","action"]}"#), ToolHandler::FileChooser).with_hints(ToolHints::destructive()),
        Tool::new("codex_list_browsers", "[Browser] List live ChatGPT browser-extension instances with stable browser_id, family, name, extension version, profile when available, capabilities, and pipe transport metadata. Use this before browser work when more than one Edge/Chrome/profile instance may be running. Never infer that two instances are interchangeable from a shared installed browser name.", object_schema(), ToolHandler::ListBrowsers).with_hints(ToolHints::read_only()),
        Tool::new("codex_bind_browser", "[Browser] Bind this MCP session to exactly one live browser instance. Pass browser_id from codex_list_browsers, or family only when that family has one live instance. Rebinding refuses an old turn with owned tabs unless end_previous_turn=true explicitly authorizes official turn cleanup. The target is validated before cleanup; old tab IDs are invalid after a successful rebind.", schema_value(r#"{"type":"object","properties":{"end_previous_turn":{"type":"boolean","description":"Explicitly end the old browser turn before switching; may close unmarked agent tabs. Default false."},"browser_id":{"type":"string","description":"Exact browser_id from codex_list_browsers"},"family":{"type":"string","description":"Chromium family such as edge or chrome; valid only when exactly one instance of that family is live"}}}"#), ToolHandler::BindBrowser).with_hints(ToolHints::destructive()),
        Tool::new("codex_browser_history", "[Browser] Read recent browser history through the ChatGPT browser extension. Supports substring queries, ISO date bounds, and a result limit. Treat titles and URLs as sensitive user data.", schema_value(r#"{"type":"object","properties":{"queries":{"type":"array","items":{"type":"string"},"description":"Optional history search terms"},"from":{"type":"string","description":"Optional inclusive ISO 8601 lower bound"},"to":{"type":"string","description":"Optional inclusive ISO 8601 upper bound"},"limit":{"type":"integer","minimum":1,"description":"Maximum entries"}}}"#), ToolHandler::BrowserHistory).with_hints(ToolHints::read_only()),
        Tool::new("codex_browser_management", "[Browser] Use the official Chromium management capability for user-requested organization of windows, tabs, tab groups, and bookmarks. action=call forwards namespace/method/args; action=audit returns the model change audit trail. This capability can modify browser state and must only be used when the user explicitly requested the change. Methods that remove or discard state (remove, delete, clear, close, discard) additionally require confirm=true.", schema_value(r#"{"type":"object","properties":{"action":{"type":"string","enum":["call","audit"],"description":"call or audit. Default call."},"namespace":{"type":"string","enum":["windows","tabs","tabGroups","bookmarks"]},"method":{"type":"string","description":"Capability method, for example query, get, create, update, move, group, search"},"args":{"type":"array","items":{},"description":"Ordered method arguments"},"confirm":{"type":"boolean","description":"Required for remove/delete/clear/close/discard methods. Set only when the user explicitly asked to remove browser state."}}}"#), ToolHandler::BrowserManagement).with_hints(ToolHints::destructive()),
        Tool::new("codex_browser_viewport", "[Browser] Set an explicit browser viewport override or reset it. Use only for requested responsive/breakpoint testing; reset the override before finishing unless the user asked to keep it.", schema_value(r#"{"type":"object","properties":{"width":{"type":"integer","minimum":1,"description":"Viewport width; required with height"},"height":{"type":"integer","minimum":1,"description":"Viewport height; required with width"},"reset":{"type":"boolean","description":"Reset the viewport override instead of setting one"}}}"#), ToolHandler::BrowserViewport).with_hints(ToolHints::idempotent_write()),
        Tool::new("codex_browser_visibility", "[Browser] Query or set whether the selected browser is visibly presented to the user. Omit visible to query; pass true/false to show/hide. Only use when live viewing is requested or useful.", schema_value(r#"{"type":"object","properties":{"visible":{"type":"boolean","description":"Omit to query; true to show; false to hide"}}}"#), ToolHandler::BrowserVisibility).with_hints(ToolHints::interaction()),
        Tool::new("codex_clipboard", "[Clipboard] Read or write the bridge's browser-session virtual clipboard. This is not the operating-system clipboard. action=read/read_text/write/write_text; clipboard data is bounded.", schema_value(r#"{"type":"object","properties":{"action":{"type":"string","enum":["read","read_text","write","write_text"]},"text":{"type":"string"},"items":{"type":"array","items":{"type":"object"}}},"required":["action"]}"#), ToolHandler::Clipboard).with_hints(ToolHints::destructive()),
        Tool::new("codex_webmcp", "[WebMCP] List or invoke tools declared by the selected tab's top-level same-origin document. Handles are document/registration scoped and expire on navigation or toolchange; experimental and never an arbitrary tool-name passthrough.", schema_value(r#"{"type":"object","properties":{"action":{"type":"string","enum":["list","invoke"]},"tab_id":{"type":"string"},"handle":{"type":"string"},"input":{"type":"object"},"timeout_ms":{"type":"integer","minimum":1,"maximum":30000}},"required":["action"]}"#), ToolHandler::WebMcp).with_hints(ToolHints::interaction()),
        Tool::new("codex_content_export", "[Export] Export Google Workspace or YouTube page content through the already-authorized tab into a user-selected existing local directory. This is local file output, not ChatGPT library export.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"kind":{"type":"string","enum":["gsuite","youtube_transcript"]},"format":{"type":"string","enum":["pdf","md","xlsx","csv","docx","pptx"]},"output_dir":{"type":"string"}},"required":["tab_id","kind","output_dir"]}"#), ToolHandler::ContentExport).with_hints(ToolHints::destructive()),
        Tool::new("codex_list_tabs", "[Tabs] List tabs owned by this bridge session. These are tabs created by or claimed by the bridge — not all browser tabs. Use codex_user_tabs to list all browser tabs available for claiming.", object_schema(), ToolHandler::ListTabs).with_hints(ToolHints::read_only()),
        Tool::new("codex_create_tab", "[Tabs] Create a new blank browser tab. The tab starts at about:blank; use codex_navigate afterward to load a URL.", object_schema(), ToolHandler::CreateTab).with_hints(ToolHints::interaction()),
        Tool::new("codex_close_tab", "[Tabs] Close a browser tab by ID. The tab must be owned by the current bridge session.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string","description":"Tab ID to close"}},"required":["tab_id"]}"#), ToolHandler::CloseTab).with_hints(ToolHints::destructive()),
        Tool::new("codex_user_tabs", "[Tabs] List open tabs in the currently bound browser instance, including unclaimed tabs. For codex_claim_tab pass the ID, exact title and exact URL from the same fresh entry as tab_id, expected_title and expected_url.", object_schema(), ToolHandler::UserTabs).with_hints(ToolHints::read_only()),
        Tool::new("codex_claim_tab", "[Tabs] Claim an existing user tab using its exact ID/title/URL snapshot from codex_user_tabs. All three are required; numeric-only claims are rejected. Fresh enumeration rejects missing or changed tabs before ownership transfer. The native claim is not atomic: a change during claim is reported as uncertain ownership and is not attached or retried.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string","description":"Tab ID from codex_user_tabs to claim"},"expected_title":{"type":"string","description":"Exact title from the same codex_user_tabs entry, including an empty title"},"expected_url":{"type":"string","description":"Exact URL from the same codex_user_tabs entry, without normalization"}},"required":["tab_id","expected_title","expected_url"]}"#), ToolHandler::ClaimTab).with_hints(ToolHints::interaction()),
        Tool::new("codex_mark_tab", "[Tabs] Mark an owned tab as a deliverable or as a handoff for later work. This mirrors the official browser turn-cleanup contract; the latest mark wins.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"status":{"type":"string","enum":["deliverable","handoff"]}},"required":["tab_id","status"]}"#), ToolHandler::MarkTab).with_hints(ToolHints::idempotent_write()),
        Tool::new("codex_navigate", "[Navigation] Navigate a tab to a URL. Blocks dangerous schemes (file:, javascript:, data:, etc.).", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"url":{"type":"string","description":"Full URL to navigate to (https://...) "}},"required":["tab_id","url"]}"#), ToolHandler::Navigate).with_hints(ToolHints::idempotent_write()),
        Tool::new("codex_reload", "[Navigation] Reload the current page in a tab.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"}},"required":["tab_id"]}"#), ToolHandler::Reload).with_hints(ToolHints::idempotent_write()),
        Tool::new("codex_navigate_back", "[Navigation] Navigate a tab one entry back in its session history.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"}},"required":["tab_id"]}"#), ToolHandler::NavigateBack).with_hints(ToolHints::interaction()),
        Tool::new("codex_navigate_forward", "[Navigation] Navigate a tab one entry forward in its session history.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"}},"required":["tab_id"]}"#), ToolHandler::NavigateForward).with_hints(ToolHints::interaction()),
        Tool::new("codex_wait_for_load", "[Navigation] Wait for page load to complete by polling document.readyState. Useful after codex_navigate on slow or JS-heavy pages.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"timeout_ms":{"type":"integer","description":"Max wait in milliseconds. Defaults to 10000."}},"required":["tab_id"]}"#), ToolHandler::WaitForLoad).with_hints(ToolHints::read_only()),
        Tool::new("codex_dom_snapshot", "[AX diagnostic] Get the full Chrome accessibility tree of a tab as JSON. This is not the official Playwright DOM snapshot. If AX is unavailable, returns a structured {mode:'text-fallback',text,primary_error} JSON string; never silently changes output shape. For actions, pass a node's backendDOMNodeId to codex_dom_click; do not pass the AX nodeId field.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"}},"required":["tab_id"]}"#), ToolHandler::DomSnapshot).with_hints(ToolHints::read_only()),
        Tool::new("codex_screenshot", "[Page] Capture a viewport or full-page screenshot as PNG (default), JPEG, or WebP. JPEG supports quality 0-100. Oversized image payloads return a text summary instead of partial base64. If the call times out, the tab is likely background-throttled — call codex_bring_to_front first, then retry.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"format":{"type":"string","enum":["png","jpeg","webp"],"description":"Image format. Default png."},"quality":{"type":"integer","description":"JPEG quality 0-100. Ignored for png/webp. Default 80."},"full_page":{"type":"boolean","description":"Capture the full document height instead of only the viewport."},"clip":{"type":"object","properties":{"x":{"type":"number","minimum":0},"y":{"type":"number","minimum":0},"width":{"type":"number","exclusiveMinimum":0},"height":{"type":"number","exclusiveMinimum":0},"scale":{"type":"number","minimum":0.1,"maximum":4}},"required":["x","y","width","height","scale"],"description":"Explicit CSS clip, mutually exclusive with full_page; maximum 32 million scaled pixels."}},"required":["tab_id"]}"#), ToolHandler::Screenshot).with_hints(ToolHints::read_only()),
        Tool::new("codex_click", "[Input] Resolve the first element for a CSS selector with the shared locator engine, then click it with a trusted CDP pointer. Prefer codex_locator for strict matching and locator options.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"selector":{"type":"string","description":"CSS selector, e.g. #login-btn or .submit-button"}},"required":["tab_id","selector"]}"#), ToolHandler::Click).with_hints(ToolHints::interaction()),
        Tool::new("codex_fill", "[Input] Fill a form input by CSS selector. Sets the value, triggers input and change events. Returns clear error if selector not found.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"selector":{"type":"string"},"value":{"type":"string"}},"required":["tab_id","selector","value"]}"#), ToolHandler::Fill).with_hints(ToolHints::interaction()),
        Tool::new("codex_evaluate", "[Page] Execute arbitrary JavaScript in the page context and return bounded JSON. This is a bridge-specific unrestricted page evaluate, not the official read-only Playwright evaluate/frame/locator scope.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"expression":{"type":"string","description":"JavaScript to evaluate, e.g. \"document.title\" or \"JSON.stringify(window.__STATE__)\""}},"required":["tab_id","expression"]}"#), ToolHandler::Evaluate).with_hints(ToolHints::interaction()),
        Tool::new("codex_cua_click", "[Input] Official CUAAPI.click/DoubleClick coordinate semantics through CDP. button uses 1 left, 2 middle, 3 right, 4 back, 5 forward; keypress holds Alt/Control/Meta/Shift; click_count=2 sends the official double-click sequence. Commands are never replayed after an ambiguous transport failure.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"x":{"type":"number"},"y":{"type":"number"},"button":{"type":"integer","minimum":1,"maximum":5,"description":"1=left, 2=middle, 3=right, 4=back, 5=forward. Default 1."},"keypress":{"type":"array","items":{"type":"string","enum":["Alt","Control","Meta","Shift"]},"description":"Modifier keys held during the click."},"click_count":{"type":"integer","minimum":1,"maximum":3,"description":"Click repetition count. Default 1; 2 emits a browser double-click sequence."}},"required":["tab_id","x","y"]}"#), ToolHandler::CuaClick).with_hints(ToolHints::interaction()),
        Tool::new("codex_cua_move", "[Input] Official CUAAPI.move semantics: move to viewport coordinates while holding optional modifier keys.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"x":{"type":"number"},"y":{"type":"number"},"keys":{"type":"array","items":{"type":"string","enum":["Alt","Control","Meta","Shift"]},"description":"Modifier keys held during the move."}},"required":["tab_id","x","y"]}"#), ToolHandler::CuaMove).with_hints(ToolHints::interaction()),
        Tool::new("codex_cua_type", "[Input] Type text at the current keyboard focus. For filling specific inputs, use codex_fill instead.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"text":{"type":"string"}},"required":["tab_id","text"]}"#), ToolHandler::CuaType).with_hints(ToolHints::interaction()),
        Tool::new("codex_cua_keypress", "[Input] Press a sequence of keyboard keys. Each key fires keyDown then keyUp events.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"keys":{"type":"array","items":{"type":"string"},"description":"Keys to press, e.g. [\"Enter\"] or [\"Control\", \"c\"]"}},"required":["tab_id","keys"]}"#), ToolHandler::CuaKeypress).with_hints(ToolHints::interaction()),
        Tool::new("codex_cua_scroll", "[Input] Official CUAAPI.scroll semantics. Accepts scrollX/scrollY or scroll_x/scroll_y and optional held modifiers.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"x":{"type":"number"},"y":{"type":"number"},"scroll_x":{"type":"number"},"scroll_y":{"type":"number"},"scrollX":{"type":"number"},"scrollY":{"type":"number"},"keypress":{"type":"array","items":{"type":"string","enum":["Alt","Control","Meta","Shift"]},"description":"Modifier keys held during scroll."}},"required":["tab_id","x","y"]}"#), ToolHandler::CuaScroll).with_hints(ToolHints::interaction()),
        Tool::new("codex_dom_get_visible", "[DOM diagnostic] Get a human-readable visible DOM summary (tag names, IDs, classes, text). It does not return official DomCUA actionable refs; use codex_find_element/backendDOMNodeId for a click target.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"}},"required":["tab_id"]}"#), ToolHandler::DomGetVisible).with_hints(ToolHints::read_only()),
        Tool::new("codex_dom_click", "[DOM] Click a DOM node by its backend node ID. Prefer node_id from codex_find_element; in a raw AX snapshot use backendDOMNodeId, not nodeId. Uses real CDP mouse events at the element's bounding box center. Set click_count=2 for double-click.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"node_id":{"type":"string","description":"Numeric backend DOM node ID from codex_find_element or backendDOMNodeId"},"click_count":{"type":"integer","minimum":1,"maximum":3,"description":"Click repetition count. Default 1; use 2 for double-click"}},"required":["tab_id","node_id"]}"#), ToolHandler::DomClick).with_hints(ToolHints::interaction()),
        Tool::new("codex_name_session", "[Session] Assign a human-readable name to this browser session for debugging.", schema_value(r#"{"type":"object","properties":{"name":{"type":"string"}},"required":["name"]}"#), ToolHandler::NameSession).with_hints(ToolHints::idempotent_write()),
        Tool::new("codex_finalize", "[Session] End the current browser turn: close unmarked agent tabs, release unmarked claimed user tabs, and preserve deliverable/handoff marks. This is official turn cleanup, not transport shutdown.", object_schema(), ToolHandler::Finalize).with_hints(ToolHints::destructive()),
        Tool::new("codex_get_info", "[Session] Get ChatGPT browser-extension backend metadata plus a bridge runtime metadata field: version, active profile, tool count, upload-base configured status, response caps, extension capabilities, and extension ID. Use for diagnostics and agent self-orientation.", object_schema(), ToolHandler::GetInfo).with_hints(ToolHints::read_only()),
        Tool::new("codex_execute_cdp", "[CDP] Execute an explicitly allowlisted Chrome DevTools Protocol diagnostic command. Use for inspect/diagnostic methods not covered by dedicated codex_* tools. Safety: raw domains are not wildcard-open; navigation, cookies, screenshots, PDF, file upload, page resource content, event-producing enable calls, arbitrary Runtime JS, and destructive methods must use bounded dedicated tools. Text output is bounded by CODEX_BRIDGE_MAX_TEXT_BYTES.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string","description":"Tab ID to execute on"},"method":{"type":"string","description":"Explicitly allowlisted CDP method, e.g. \"DOM.getDocument\", \"Page.getLayoutMetrics\", \"Performance.getMetrics\""},"params":{"type":"object","description":"CDP method parameters as a JSON object"}},"required":["tab_id","method"]}"#), ToolHandler::ExecuteCdp).with_hints(ToolHints::read_only()),
        Tool::new("codex_page_assets", "[Page assets] action=list returns a document-bound inventory including observed resources and inline SVG summaries. action=bundle exports selected image/font/stylesheet/video assets with a manifest into an existing output_dir. action=download_media saves observed media only. Implemented through CDP, not a native capability passthrough. Omit action only for the published legacy bounded resource response.", schema_value(r#"{"type":"object","properties":{"action":{"type":"string","enum":["list","bundle","download_media"]},"inventory_id":{"type":"string"},"output_dir":{"type":"string"},"asset_ids":{"type":"array","items":{"type":"string"}},"kinds":{"type":"array","items":{"type":"string","enum":["image","font","stylesheet","video"]}},"url":{"type":"string"},"selector":{"type":"string"},"max_resource_bytes":{"type":"integer","minimum":1,"maximum":4194304},"tab_id":{"type":"string","description":"Tab ID to inspect"},"include_content":{"type":"boolean","description":"Fetch known-size resources as base64. Defaults to false."},"types":{"type":"array","items":{"type":"string"},"description":"Filter: Image, Stylesheet, Script, Font, Document, Media, Manifest, Fetch, Other"},"max_resources":{"type":"integer","description":"Max resources to fetch when include_content=true. Default 50, max 200."},"max_total_bytes":{"type":"integer","description":"Max total base64 content bytes when include_content=true. Default 1048576, max 5242880."}},"required":["tab_id"]}"#), ToolHandler::PageAssets).with_hints(ToolHints::destructive()),
        Tool::new("codex_network_cookies", "[Network] Read cookies for the current page or specific URLs. Cookie values are REDACTED by default for security (set redact_values: false to see raw values). Preferred over execute_cdp with Network.getCookies.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string","description":"Tab ID"},"urls":{"type":"array","items":{"type":"string"},"description":"Optional URL list to filter. Omit to get all cookies for the current page."},"redact_values":{"type":"boolean","description":"Redact cookie values for security. Default: true."}},"required":["tab_id"]}"#), ToolHandler::NetworkCookies).with_hints(ToolHints::read_only()),
        Tool::new("codex_network_set_cookie", "[Network] Set a browser cookie. Use this for cookie manipulation; for reading cookies, use codex_network_cookies.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string","description":"Tab ID"},"name":{"type":"string","description":"Cookie name"},"value":{"type":"string","description":"Cookie value"},"url":{"type":"string","description":"URL to associate cookie with"},"domain":{"type":"string","description":"Cookie domain"},"path":{"type":"string","description":"Cookie path"},"httpOnly":{"type":"boolean","description":"HttpOnly flag"},"secure":{"type":"boolean","description":"Secure flag"},"sameSite":{"type":"string","description":"Strict, Lax, or None"}},"required":["tab_id","name","value"]}"#), ToolHandler::NetworkSetCookie).with_hints(ToolHints::destructive()),
        Tool::new("codex_file_input", "[Input] Upload files to a <input type=file> element. First finds the element by CSS selector, then sets the specified files via DOM.setFileInputFiles. Requires CODEX_BRIDGE_UPLOAD_BASE; paths must be absolute and within that allowed upload directory. Security: path traversal blocked, only regular files, max 10 MB per file.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string","description":"Numeric tab ID"},"selector":{"type":"string","description":"CSS selector for the file input element"},"files":{"type":"array","items":{"type":"string"},"description":"Absolute file paths to upload"}},"required":["tab_id","selector","files"]}"#), ToolHandler::FileInput).with_hints(ToolHints::idempotent_write()),
        Tool::new("codex_dialog", "[Page] Get, accept or dismiss a JavaScript dialog. Start codex_observe before triggering it; get returns buffered active dialog with ID. Optional dialog_id rejects stale actions. Legacy accept/dismiss without ID retain direct CDP behavior.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string","description":"Numeric tab ID"},"action":{"type":"string","enum":["get","accept","dismiss"],"description":"Accept or dismiss the dialog"},"prompt_text":{"type":"string","description":"Text to enter for prompt dialogs (only valid with accept)"},"dialog_id":{"type":"string","description":"Optional current dialog ID; stale IDs rejected before dispatch."}},"required":["tab_id","action"]}"#), ToolHandler::Dialog).with_hints(ToolHints::interaction()),
        Tool::new("codex_find_element", "[DOM] Find elements by ARIA role and/or accessible name in the page's accessibility tree. Returns an actionable numeric node_id when the AX node has a backend DOM node, plus ax_node_id and actionable=false for AX-only entries. More reliable than CSS selectors.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string","description":"Numeric tab ID"},"role":{"type":"string","description":"ARIA role, case-insensitive exact match (e.g. 'button', 'link', 'textbox', 'checkbox', 'heading')"},"name":{"type":"string","description":"Accessible name, case-insensitive substring match"},"max_results":{"type":"integer","description":"Maximum results (default 10, max 50)"}},"required":["tab_id"]}"#), ToolHandler::FindElement).with_hints(ToolHints::read_only()),
        Tool::new("codex_click_element", "[Input] Click an element by the numeric actionable node_id returned by codex_find_element. Uses CDP DOM.getBoxModel → Input dispatch (no JS injection). Safer than CSS selector clicking.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string","description":"Numeric tab ID"},"node_id":{"type":"string","description":"Actionable backend node_id from codex_find_element"}},"required":["tab_id","node_id"]}"#), ToolHandler::ClickElement).with_hints(ToolHints::interaction()),
        Tool::new("codex_locator", "[Locator] Bounded declarative reads/actions. Composition and/or/has/has_not, limited safe regex, exact select descriptors, sequential typing, trusted set_checked and actionability preparation force/trial. Mutations never replay. all returns nth specs, not persistent handles. Same-origin frames only; transformed/cross-origin/OOPIF and full ARIA/Playwright not claimed.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"action":{"type":"string","enum":["count","text","inner_text","text_content","all_text","all_text_contents","attribute","value","visible","enabled","checked","click","dblclick","focus","press","fill","select_option","set_checked","wait","all","type","press_sequentially"]},"selector":{"type":"string","description":"CSS selector"},"role":{"type":"string","description":"ARIA role such as button, link, textbox, heading"},"name":{"oneOf":[{"type":"string"},{"type":"object","properties":{"regex":{"type":"string","maxLength":256},"flags":{"type":"string"}},"required":["regex"],"additionalProperties":false}]},"text":{"oneOf":[{"type":"string"},{"type":"object","properties":{"regex":{"type":"string","maxLength":256},"flags":{"type":"string"}},"required":["regex"],"additionalProperties":false}]},"test_id":{"oneOf":[{"type":"string"},{"type":"object","properties":{"regex":{"type":"string","maxLength":256},"flags":{"type":"string"}},"required":["regex"],"additionalProperties":false}]},"placeholder":{"oneOf":[{"type":"string"},{"type":"object","properties":{"regex":{"type":"string","maxLength":256},"flags":{"type":"string"}},"required":["regex"],"additionalProperties":false}]},"label":{"oneOf":[{"type":"string"},{"type":"object","properties":{"regex":{"type":"string","maxLength":256},"flags":{"type":"string"}},"required":["regex"],"additionalProperties":false}]},"frame_selectors":{"type":"array","items":{"type":"string"},"maxItems":8,"description":"Nested same-origin frame selectors; cross-origin frames fail explicitly"},"scopes":{"type":"array","items":{"type":"object"},"maxItems":16,"description":"Ordered descendant locator specs, each with the same basic match fields"},"has_text":{"oneOf":[{"type":"string"},{"type":"object","properties":{"regex":{"type":"string","maxLength":256},"flags":{"type":"string"}},"required":["regex"],"additionalProperties":false}]},"has_not_text":{"oneOf":[{"type":"string"},{"type":"object","properties":{"regex":{"type":"string","maxLength":256},"flags":{"type":"string"}},"required":["regex"],"additionalProperties":false}]},"visible":{"type":"boolean","description":"Filter matched elements by visibility"},"exact":{"type":"boolean","description":"Exact case-sensitive text/name match. Default false."},"strict":{"type":"boolean","description":"Fail when multiple elements match and nth is omitted. Default true."},"nth":{"type":"integer","minimum":-1,"description":"Zero-based index, or -1 for last"},"max_results":{"type":"integer","description":"all_text result cap. Default 50, max 200."},"attribute":{"type":"string","description":"Attribute name for action=attribute"},"value":{"type":"string","description":"String for fill/type/press_sequentially or legacy select value."},"checked":{"type":"boolean","description":"Desired state for set_checked"},"keys":{"type":"array","items":{"type":"string"},"description":"Keys for action=press"},"button":{"type":"string","enum":["left","middle","right"],"description":"Pointer button for click/dblclick. Default left."},"modifiers":{"type":"array","items":{"type":"string","enum":["Alt","Control","Meta","Shift"]},"description":"Modifier keys held during click/dblclick."},"state":{"type":"string","enum":["visible","attached","hidden","detached"],"description":"Wait condition. Default visible."},"timeout_ms":{"type":"integer","minimum":1,"maximum":60000,"description":"Preparation/postcondition deadline plus fixed trusted-dispatch cleanup window."},"has":{"type":"object","description":"Nested bounded descendant locator spec."},"has_not":{"type":"object","description":"Nested bounded descendant locator spec."},"and":{"type":"array","items":{"type":"object"},"maxItems":16},"or":{"type":"array","items":{"type":"object"},"maxItems":16},"force":{"type":"boolean","description":"Skip pointer visibility/enabled/hit checks, not invalid box/viewport/frame geometry; checked actions retain editable checks."},"trial":{"type":"boolean","description":"Prepare/check without dispatching input."},"options":{"oneOf":[{"type":"string"},{"type":"object","properties":{"value":{"type":"string"},"label":{"type":"string"},"index":{"type":"integer","minimum":0}}},{"type":"array","items":{"oneOf":[{"type":"string"},{"type":"object"}]},"maxItems":200}],"description":"Exact select descriptors; multiple requires multiple select."},"delay_ms":{"type":"integer","minimum":0,"maximum":1000}},"required":["tab_id","action"]}"#), ToolHandler::Locator).with_hints(ToolHints::interaction()),
        Tool::new("codex_nav_and_wait", "[Navigation] Navigate to a URL and wait for the page to load. Combines codex_navigate + codex_wait_for_load in one call — use this instead of two separate calls.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string","description":"Numeric tab ID"},"url":{"type":"string","description":"Full URL to navigate to"},"timeout_ms":{"type":"integer","description":"Max wait in ms. Defaults to 30000."},"wait_until":{"type":"string","enum":["commit","domcontentloaded","load"],"description":"Arm before the action and wait for a later navigation. Default load; networkidle unsupported."}},"required":["tab_id","url"]}"#), ToolHandler::NavAndWait).with_hints(ToolHints::idempotent_write()),
        Tool::new("codex_click_and_wait", "[Input] Click the first CSS match with a trusted CDP pointer and wait for page load. Combines the shared locator/pointer path with codex_wait_for_load.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string","description":"Numeric tab ID"},"selector":{"type":"string","description":"CSS selector to click"},"timeout_ms":{"type":"integer","description":"Max wait in ms. Defaults to 10000."},"wait_until":{"type":"string","enum":["commit","domcontentloaded","load"],"description":"Arm before the action and wait for a later navigation. Default load; networkidle unsupported."}},"required":["tab_id","selector"]}"#), ToolHandler::ClickAndWait).with_hints(ToolHints::interaction()),
        Tool::new("codex_form_fill", "[Input] Fill multiple form fields at once. Accepts a map of CSS selector to value. Optionally clicks a submit button after filling. Sequential dispatch with configurable delay.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string","description":"Numeric tab ID"},"fields":{"type":"object","description":"Map of CSS selector to value"},"submit":{"type":"string","description":"Optional CSS selector for submit button to click after filling"},"delay_ms":{"type":"integer","description":"Delay between field inputs in ms. Defaults to 50."}},"required":["tab_id","fields"]}"#), ToolHandler::FormFill).with_hints(ToolHints::interaction()),
        Tool::new("codex_doctor", "[Session] Run self-diagnostics. Checks pipe connectivity, browser-extension availability, install path, and bridge version. Use when browser connectivity is unknown or a browser tool fails. Returns a diagnostic summary plus a bounded per-pipe sample.", object_schema(), ToolHandler::Doctor).with_hints(ToolHints::read_only()).with_meta(serde_json::json!({"anthropic/alwaysLoad": true})),
        Tool::new("codex_bring_to_front", "[Page] Activate a tab and bring it to the foreground via Page.bringToFront. Call this before screenshot or other CDP calls when a tab has been in the background — Chrome throttles/discards background tabs and CDP calls on a suspended tab can time out silently. Does not navigate or change page state.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string","description":"Numeric tab ID to activate"}},"required":["tab_id"]}"#), ToolHandler::BringToFront).with_hints(ToolHints::idempotent_write()),
        Tool::new("codex_get_url", "[Page] Get the current URL of a tab via location.href. Cheaper than codex_evaluate for this common read.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"}},"required":["tab_id"]}"#), ToolHandler::GetUrl).with_hints(ToolHints::read_only()),
        Tool::new("codex_get_title", "[Page] Get the current document.title of a tab.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"}},"required":["tab_id"]}"#), ToolHandler::GetTitle).with_hints(ToolHints::read_only()),
        Tool::new("codex_wait_for_element", "[Navigation] Poll until a CSS selector matches. Use this instead of codex_wait_for_load on SPAs where the URL does not change but content renders asynchronously. Returns error on timeout.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"selector":{"type":"string","description":"CSS selector to wait for"},"timeout_ms":{"type":"integer","description":"Max wait in ms. Defaults to 10000."}},"required":["tab_id","selector"]}"#), ToolHandler::WaitForElement).with_hints(ToolHints::read_only()),
        Tool::new("codex_hover", "[Input] Resolve the first CSS match and move a trusted CDP pointer over its hit-tested center. Useful for dropdowns, tooltips, and hover-revealed controls.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"selector":{"type":"string"}},"required":["tab_id","selector"]}"#), ToolHandler::Hover).with_hints(ToolHints::interaction()),
        Tool::new("codex_print_pdf", "[Page] Render the current page to PDF via Page.printToPDF (A4, backgrounds on). Uses CDP ReturnAsStream with bounded IO.read chunks and returns only a size summary; PDF bytes are not embedded in the response.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"}},"required":["tab_id"]}"#), ToolHandler::PrintPdf).with_hints(ToolHints::read_only()),
        Tool::new("codex_storage", "[Network] Read or write Web Storage. action='get' returns the value (or null); action='set' writes it. storage_type: 'local' (default) or 'session'. Useful for login state, tokens, and SPA app state stored client-side.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"key":{"type":"string","description":"Storage key"},"value":{"type":"string","description":"Value to set (action=set only)"},"action":{"type":"string","enum":["get","set"],"description":"Read or write. Default: get"},"storage_type":{"type":"string","enum":["local","session"],"description":"localStorage or sessionStorage. Default: local."}},"required":["tab_id","key"]}"#), ToolHandler::Storage).with_hints(ToolHints::destructive()),
        Tool::new("codex_select_option", "[Locator] Set the first CSS-matched native select to an exact option value through the shared locator engine. Missing options fail before dispatching input/change. Descriptor and multi-select forms remain partial.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"selector":{"type":"string","description":"CSS selector for the select element"},"value":{"type":"string","description":"Exact option value to select"}},"required":["tab_id","selector","value"]}"#), ToolHandler::SelectOption).with_hints(ToolHints::interaction()),
        Tool::new("codex_drag", "[Input] Official CUAAPI.drag path semantics through CDP: press at path[0], move through every supplied point, then release. Legacy from/to coordinates remain accepted as a two-point path. Commands are not replayed.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"path":{"type":"array","minItems":1,"maxItems":1000,"items":{"type":"object","properties":{"x":{"type":"number"},"y":{"type":"number"}},"required":["x","y"]}},"keys":{"type":"array","items":{"type":"string","enum":["Alt","Control","Meta","Shift"]}},"from_x":{"type":"number"},"from_y":{"type":"number"},"to_x":{"type":"number"},"to_y":{"type":"number"}},"required":["tab_id"]}"#), ToolHandler::Drag).with_hints(ToolHints::interaction()),
        Tool::new("codex_screenshot_element", "[Page] Capture a screenshot clipped to a single element's bounding box. Use to verify one component's rendering without the full page. Oversized image payloads return a text summary instead of partial base64.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"selector":{"type":"string","description":"CSS selector for the element to capture"}},"required":["tab_id","selector"]}"#), ToolHandler::ScreenshotElement).with_hints(ToolHints::read_only()),
        Tool::new("codex_delete_cookies", "[Network] Delete cookies by name via Network.deleteCookies. Optionally scope by url/domain/path. Use for logout or account-switch testing.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"name":{"type":"string","description":"Cookie name to delete"},"url":{"type":"string","description":"Optional URL scope"},"domain":{"type":"string","description":"Optional domain scope"},"path":{"type":"string","description":"Optional path scope"}},"required":["tab_id","name"]}"#), ToolHandler::DeleteCookies).with_hints(ToolHints::destructive()),
        Tool::new("codex_emulate_device", "[Page] Override the viewport to emulate a device via Emulation.setDeviceMetricsOverride. Defaults to iPhone (390x844). Pass reset=true (Emulation.clearDeviceMetricsOverride) to revert to the real viewport.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"width":{"type":"integer","description":"Viewport width. Default 390."},"height":{"type":"integer","description":"Viewport height. Default 844."},"mobile":{"type":"boolean","description":"Treat as mobile. Default true."},"user_agent":{"type":"string","description":"User-Agent string. Default iPhone Safari."},"reset":{"type":"boolean","description":"Clear emulation and revert to real viewport."}},"required":["tab_id"]}"#), ToolHandler::EmulateDevice).with_hints(ToolHints::idempotent_write()),
        Tool::new("codex_network_monitor", "[Network] Bounded tab-scoped request/response capture paired by request_id. Optional case-sensitive url_filter substring filters paired URLs. Persistent observation producers are not disabled by timed capture. Reports raw/queued/captured/dropped counts.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"duration_ms":{"type":"integer","description":"Capture window in ms. Default 5000."},"url_filter":{"type":"string","maxLength":2048}},"required":["tab_id"]}"#), ToolHandler::NetworkMonitor).with_hints(ToolHints::read_only()),
        Tool::new("codex_console_logs", "[Page] Capture console.* output for a duration. Enables Runtime, collects Runtime.consoleAPICalled events with a byte-bounded subscription, then disables. Returns raw log entries plus raw/queued/captured/dropped event counts.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"duration_ms":{"type":"integer","description":"Capture window in ms. Default 5000."}},"required":["tab_id"]}"#), ToolHandler::ConsoleLogs).with_hints(ToolHints::read_only()),
        Tool::new("codex_wait_for_url", "[Navigation] Poll until location.href contains a substring. For SPAs that change the URL on route change without a full page navigation. Returns error on timeout.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"},"pattern":{"type":"string","description":"Substring to match in the URL"},"timeout_ms":{"type":"integer","description":"Max wait in ms. Default 10000."}},"required":["tab_id","pattern"]}"#), ToolHandler::WaitForUrl).with_hints(ToolHints::read_only()),
        Tool::new("codex_performance_metrics", "[Page] Get Chrome Performance metrics via Performance.getMetrics — DOM node count, JS heap size, document count, event listener count, etc. Use to diagnose page weight and memory.", schema_value(r#"{"type":"object","properties":{"tab_id":{"type":"string"}},"required":["tab_id"]}"#), ToolHandler::PerformanceMetrics).with_hints(ToolHints::read_only()),
    ]
}

pub(super) fn tools_to_values(tools: &[Tool]) -> Vec<Value> {
    tools
        .iter()
        .map(|tool| {
            let mut value = serde_json::json!({
                "name": tool.name,
                "description": tool.description,
                "inputSchema": tool.input_schema
            });
            if let Some(annotations) = &tool.annotations {
                value
                    .as_object_mut()
                    .expect("tool value is an object")
                    .insert("annotations".into(), annotations.clone());
            }
            if let Some(meta) = &tool.meta {
                value
                    .as_object_mut()
                    .expect("tool value is an object")
                    .insert("_meta".into(), meta.clone());
            }
            value
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
        assert_eq!(names.first(), Some(&"codex_observe"));
        assert_eq!(names.get(1), Some(&"codex_file_chooser"));
        assert_eq!(names.last(), Some(&"codex_performance_metrics"));
        assert_eq!(names.len(), 66);
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
    fn claim_tab_requires_snapshot_and_is_not_safe_to_replay() {
        let tools = registered_tools();
        let claim = tools.iter().find(|t| t.name == "codex_claim_tab").unwrap();
        assert_eq!(
            claim.input_schema["required"],
            serde_json::json!(["tab_id", "expected_title", "expected_url"])
        );
        assert_eq!(claim.annotations.as_ref().unwrap()["idempotentHint"], false);
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
    fn all_tools_carry_behavior_annotations() {
        for tool in registered_tools() {
            let annotations = tool
                .annotations
                .as_ref()
                .unwrap_or_else(|| panic!("tool {} is missing annotations", tool.name));
            assert!(annotations["readOnlyHint"].is_boolean(), "{}", tool.name);
            assert!(annotations["destructiveHint"].is_boolean(), "{}", tool.name);
            assert!(annotations["idempotentHint"].is_boolean(), "{}", tool.name);
            assert_eq!(
                annotations["openWorldHint"], true,
                "{} must stay open-world: every tool drives a live browser",
                tool.name
            );
            if annotations["readOnlyHint"] == serde_json::json!(true) {
                assert_eq!(
                    annotations["destructiveHint"],
                    serde_json::json!(false),
                    "{} cannot be read-only and destructive",
                    tool.name
                );
            }
        }
    }

    #[test]
    fn annotation_categories_match_tool_semantics() {
        let tools = registered_tools();
        let find = |name: &str| {
            tools
                .iter()
                .find(|tool| tool.name == name)
                .unwrap_or_else(|| panic!("missing tool {name}"))
        };

        let list_tabs = find("codex_list_tabs").annotations.as_ref().unwrap();
        assert_eq!(list_tabs["readOnlyHint"], serde_json::json!(true));

        let close_tab = find("codex_close_tab").annotations.as_ref().unwrap();
        assert_eq!(close_tab["destructiveHint"], serde_json::json!(true));

        let click = find("codex_click").annotations.as_ref().unwrap();
        assert_eq!(click["readOnlyHint"], serde_json::json!(false));
        assert_eq!(click["idempotentHint"], serde_json::json!(false));

        let navigate = find("codex_navigate").annotations.as_ref().unwrap();
        assert_eq!(navigate["idempotentHint"], serde_json::json!(true));
    }

    #[test]
    fn tools_to_values_serialize_annotations() {
        let values = tools_to_values(&registered_tools());
        assert_eq!(values.len(), registered_tools().len());
        for value in &values {
            assert!(
                value["annotations"]["openWorldHint"].is_boolean(),
                "tool {} lost its annotations in serialization",
                value["name"]
            );
        }
    }

    #[test]
    fn only_doctor_loads_upfront_in_claude_code() {
        let values = tools_to_values(&registered_tools());
        let always_loaded: Vec<_> = values
            .iter()
            .filter(|tool| {
                tool["_meta"]["anthropic/alwaysLoad"]
                    .as_bool()
                    .unwrap_or(false)
            })
            .map(|tool| tool["name"].as_str().unwrap())
            .collect();

        assert_eq!(always_loaded, vec!["codex_doctor"]);
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
