use codex_browser_bridge::{
    browser::{
        build_click_script, build_fill_script, decode_tabs, decode_user_tabs, json_escaped,
        parse_tab_id,
    },
    browser_test_support::{is_tab_gone_error, is_transient_load_error},
    error::BridgeError,
    security::validate_url,
};
use serde_json::value::RawValue;

#[test]
fn parse_tab_id_requires_numeric_values() {
    assert_eq!(parse_tab_id("navigate", "123").unwrap(), 123);

    let err = parse_tab_id("navigate", "abc").unwrap_err();
    assert_eq!(
        err.to_string(),
        r#"navigate requires numeric tab_id, got "abc""#
    );
}

#[test]
fn validate_url_blocks_dangerous_schemes_case_insensitively() {
    for url in [
        " file:///c:/secret ",
        "JavaScript:alert(1)",
        "DATA:text/html,x",
        "vbscript:msgbox(1)",
        "about:blank",
        "chrome://settings",
        "edge://settings",
    ] {
        assert!(validate_url(url).is_err(), "{url}");
    }

    assert!(validate_url("https://example.com").is_ok());
    assert!(validate_url("http://localhost:3000").is_ok());
}

#[test]
fn validate_url_allows_only_http_and_https() {
    for url in [
        "ftp://example.com/file",
        "blob:https://example.com/id",
        "filesystem:https://example.com/tmp",
        "view-source:https://example.com",
        "chrome-extension://extension/page.html",
        "example.com/no-scheme",
    ] {
        assert!(validate_url(url).is_err(), "{url}");
    }

    assert!(validate_url("HTTPS://EXAMPLE.COM/path").is_ok());
}

#[test]
fn decode_tabs_normalizes_string_and_numeric_ids() {
    let raw =
        RawValue::from_string(r#"[{"id":1,"url":"u"},{"id":"2","title":"t"}]"#.into()).unwrap();

    let tabs = decode_tabs(&raw).unwrap();

    assert_eq!(tabs[0].id, "1");
    assert_eq!(tabs[0].url, "u");
    assert_eq!(tabs[1].id, "2");
    assert_eq!(tabs[1].title, "t");
}

#[test]
fn decode_user_tabs_accepts_wrapped_or_bare_arrays() {
    let wrapped = RawValue::from_string(
        r#"{"tabs":[{"id":7,"title":"A","url":"https://a","lastOpened":"now","tabGroup":"g"}]}"#
            .into(),
    )
    .unwrap();
    let bare = RawValue::from_string(r#"[{"id":"8","title":"B"}]"#.into()).unwrap();

    let wrapped_tabs = decode_user_tabs(&wrapped).unwrap();
    let bare_tabs = decode_user_tabs(&bare).unwrap();

    assert_eq!(wrapped_tabs[0].id, "7");
    assert_eq!(wrapped_tabs[0].last_opened, "now");
    assert_eq!(wrapped_tabs[0].tab_group, "g");
    assert_eq!(bare_tabs[0].id, "8");
}

#[test]
fn json_escaped_is_safe_for_javascript_string_literals() {
    let escaped = json_escaped(
        r#"button[data-name="a\b
c"]"#,
    );

    assert_eq!(escaped, "\"button[data-name=\\\"a\\\\b\\nc\\\"]\"");
}

#[test]
fn click_and_fill_scripts_embed_escaped_selector_and_value() {
    let click = build_click_script(r#"button[data-x="yes"]"#);
    let fill = build_fill_script("#name", "A\nB\\C");

    assert!(click.contains(r#"document.querySelector("button[data-x=\"yes\"]")"#));
    assert!(click.contains("el.click()"));
    assert!(fill.contains(r##"document.querySelector("#name")"##));
    assert!(fill.contains("\"A\\nB\\\\C\""));
    assert!(fill.contains("dispatchEvent(new Event('input',{bubbles:true}))"));
    assert!(fill.contains("dispatchEvent(new Event('change',{bubbles:true}))"));
}

#[test]
fn error_classifiers_match_go_browser_api_messages() {
    let tab_gone = BridgeError::Rpc {
        method: "executeCdp".into(),
        message: "Target closed while dispatching".into(),
    };
    let transient = BridgeError::Protocol("Execution context destroyed after navigation".into());
    let other = BridgeError::Protocol("permission denied".into());

    assert!(is_tab_gone_error(&tab_gone));
    assert!(is_transient_load_error(&transient));
    assert!(!is_tab_gone_error(&other));
    assert!(!is_transient_load_error(&other));
}
