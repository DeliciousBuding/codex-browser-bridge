#[allow(dead_code)]
#[path = "../src/browser.rs"]
mod browser;
#[allow(dead_code)]
#[path = "../src/client.rs"]
mod client;
#[allow(dead_code)]
#[path = "../src/discovery.rs"]
mod discovery;

#[allow(dead_code)]
#[path = "../src/error.rs"]
mod error;
#[allow(dead_code)]
#[path = "../src/mcp.rs"]
mod mcp;
#[allow(dead_code)]
#[path = "../src/pipe.rs"]
mod pipe;
#[allow(dead_code)]
#[path = "../src/protocol.rs"]
mod protocol;

use serde_json::{json, Value};

#[test]
fn tools_list_matches_go_mcp_registry_names_and_order() {
    let tools = mcp::registered_tool_values_for_test();
    let names: Vec<_> = tools
        .iter()
        .map(|tool| tool["name"].as_str().expect("tool name"))
        .collect();

    assert_eq!(
        names,
        vec![
            "codex_list_tabs",
            "codex_create_tab",
            "codex_close_tab",
            "codex_user_tabs",
            "codex_claim_tab",
            "codex_navigate",
            "codex_reload",
            "codex_navigate_back",
            "codex_navigate_forward",
            "codex_wait_for_load",
            "codex_dom_snapshot",
            "codex_screenshot",
            "codex_click",
            "codex_fill",
            "codex_evaluate",
            "codex_cua_click",
            "codex_cua_type",
            "codex_cua_keypress",
            "codex_cua_scroll",
            "codex_dom_get_visible",
            "codex_dom_click",
            "codex_name_session",
            "codex_finalize",
            "codex_get_info",
            "codex_execute_cdp",
            "codex_page_assets",
            "codex_network_cookies",
            "codex_network_set_cookie",
        ]
    );
}

#[test]
fn tool_schemas_preserve_required_fields_and_types() {
    let tools = mcp::registered_tool_values_for_test();
    let find = |name: &str| -> &Value {
        tools
            .iter()
            .find(|tool| tool["name"] == name)
            .expect("registered tool")
    };

    assert_eq!(
        find("codex_screenshot")["inputSchema"],
        json!({
            "type": "object",
            "properties": {
                "tab_id": {"type": "string"},
                "fullPage": {"type": "boolean"}
            },
            "required": ["tab_id"]
        })
    );
    assert_eq!(
        find("codex_cua_keypress")["inputSchema"]["properties"]["keys"],
        json!({"type": "array", "items": {"type": "string"}})
    );
    assert_eq!(
        find("codex_fill")["inputSchema"]["required"],
        json!(["tab_id", "selector", "value"])
    );
}

#[test]
fn strict_required_string_validation_rejects_missing_empty_and_wrong_type() {
    for args in [
        json!({}),
        json!({"tab_id": ""}),
        json!({"tab_id": "  "}),
        json!({"tab_id": 3}),
    ] {
        let err = mcp::validate_required_str_for_test(&args, "tab_id").unwrap_err();
        assert_eq!(err.to_string(), "missing required argument: tab_id");
    }

    mcp::validate_required_str_for_test(&json!({"tab_id": "3"}), "tab_id").unwrap();
}

#[test]
fn tool_error_envelope_uses_mcp_result_content_with_is_error() {
    let value: Value = serde_json::from_str(&mcp::text_error_envelope_for_test(
        json!(4),
        "missing required argument: tab_id",
    ))
    .unwrap();

    assert_eq!(value["jsonrpc"], "2.0");
    assert_eq!(value["id"], 4);
    assert_eq!(value["result"]["isError"], true);
    assert_eq!(
        value["result"]["content"],
        json!([{"type": "text", "text": "Error: missing required argument: tab_id"}])
    );
}

#[test]
fn screenshot_content_uses_image_block_then_text_summary() {
    let content = mcp::screenshot_content_for_test("iVBORw0KGgo=".to_string());

    assert_eq!(
        content,
        json!([
            {"type": "image", "data": "iVBORw0KGgo=", "mimeType": "image/png"},
            {"type": "text", "text": "Screenshot captured for tab 7 (12 bytes base64)"}
        ])
    );
}
