#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolProfile {
    Basic,
    Network,
    Full,
}

impl ToolProfile {
    pub fn from_env() -> Self {
        match std::env::var("CODEX_BRIDGE_PROFILE").ok().as_deref() {
            Some("basic") => ToolProfile::Basic,
            Some("network") => ToolProfile::Network,
            Some("full") | None => ToolProfile::Full,
            Some(other) => {
                eprintln!(
                    "Warning: unknown CODEX_BRIDGE_PROFILE '{}', using 'full'",
                    other
                );
                ToolProfile::Full
            }
        }
    }

    pub fn includes(&self, tool_name: &str) -> bool {
        match self {
            ToolProfile::Basic => BASIC_TOOLS.contains(&tool_name),
            ToolProfile::Network => NETWORK_TOOLS.contains(&tool_name),
            ToolProfile::Full => true,
        }
    }
}

// Basic: essential tab + nav + dom + screenshot + interaction (~32 tools)
const BASIC_TOOLS: &[&str] = &[
    "codex_list_tabs", "codex_create_tab", "codex_close_tab",
    "codex_user_tabs", "codex_claim_tab",
    "codex_navigate", "codex_reload", "codex_navigate_back",
    "codex_navigate_forward", "codex_wait_for_load",
    "codex_nav_and_wait", "codex_wait_for_element", "codex_wait_for_url",
    "codex_dom_snapshot", "codex_dom_get_visible", "codex_dom_click",
    "codex_find_element", "codex_click_element",
    "codex_screenshot", "codex_bring_to_front", "codex_screenshot_element",
    "codex_click", "codex_fill", "codex_form_fill", "codex_select_option",
    "codex_hover",
    "codex_evaluate", "codex_get_url", "codex_get_title", "codex_cua_scroll",
    "codex_name_session", "codex_finalize", "codex_get_info",
];

// Network: Basic + cookies + CDP + file upload + dialog + storage + pdf + emulate (~47 tools)
const NETWORK_TOOLS: &[&str] = &[
    "codex_list_tabs", "codex_create_tab", "codex_close_tab",
    "codex_user_tabs", "codex_claim_tab",
    "codex_navigate", "codex_reload", "codex_navigate_back",
    "codex_navigate_forward", "codex_wait_for_load",
    "codex_nav_and_wait", "codex_wait_for_element", "codex_wait_for_url",
    "codex_dom_snapshot", "codex_dom_get_visible", "codex_dom_click",
    "codex_find_element", "codex_click_element",
    "codex_screenshot", "codex_bring_to_front", "codex_screenshot_element",
    "codex_click", "codex_fill", "codex_form_fill", "codex_select_option",
    "codex_click_and_wait", "codex_hover", "codex_drag",
    "codex_evaluate", "codex_get_url", "codex_get_title",
    "codex_cua_click", "codex_cua_type", "codex_cua_keypress",
    "codex_cua_scroll",
    "codex_network_cookies", "codex_network_set_cookie", "codex_delete_cookies",
    "codex_file_input", "codex_dialog", "codex_storage", "codex_print_pdf",
    "codex_emulate_device", "codex_network_monitor", "codex_console_logs",
    "codex_performance_metrics",
    "codex_name_session", "codex_finalize", "codex_get_info",
    "codex_doctor",
];
