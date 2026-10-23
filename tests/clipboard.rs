use codex_browser_bridge::{
    clipboard::{
        paste_script, shortcut_action, ClipboardEntry, ClipboardItem, ClipboardShortcut,
        PresentationStyle, VirtualClipboard, MAX_CLIPBOARD_BYTES,
    },
    protocol,
};
use serde_json::{json, Value};
use tokio::io::{duplex, DuplexStream};
use tokio::time::{timeout, Duration};

fn text_item(text: &str) -> ClipboardItem {
    ClipboardItem {
        entries: vec![ClipboardEntry {
            mime_type: "text/plain".into(),
            text: Some(text.into()),
            base64: None,
        }],
        presentation_style: Some(PresentationStyle::Unspecified),
    }
}

#[test]
fn binary_roundtrip_and_read_text() {
    let mut clipboard = VirtualClipboard::new();
    clipboard
        .write(vec![
            text_item("hello"),
            ClipboardItem {
                entries: vec![ClipboardEntry {
                    mime_type: "image/png".into(),
                    text: None,
                    base64: Some("AAEC".into()),
                }],
                presentation_style: None,
            },
        ])
        .unwrap();
    assert_eq!(clipboard.read_text(), "hello");
    assert_eq!(
        clipboard.read()[1].entries[0].base64.as_deref(),
        Some("AAEC")
    );
    assert_eq!(clipboard.byte_len(), 8);
}

#[test]
fn invalid_mime_and_base64_and_exactly_one_are_rejected() {
    let mut c = VirtualClipboard::new();
    for entry in [
        ClipboardEntry {
            mime_type: "textplain".into(),
            text: Some("x".into()),
            base64: None,
        },
        ClipboardEntry {
            mime_type: "text/plain".into(),
            text: None,
            base64: Some("%%%".into()),
        },
        ClipboardEntry {
            mime_type: "text/plain".into(),
            text: Some("x".into()),
            base64: Some("eA==".into()),
        },
    ] {
        assert!(c
            .write(vec![ClipboardItem {
                entries: vec![entry],
                presentation_style: None
            }])
            .is_err());
    }
}

#[test]
fn clipboard_is_bounded_and_clear_drops_data() {
    let mut c = VirtualClipboard::new();
    let huge = "x".repeat(MAX_CLIPBOARD_BYTES + 1);
    assert!(c.write_text(huge).is_err());
    c.write_text("ok").unwrap();
    c.clear();
    assert!(c.read().is_empty());
    assert_eq!(c.read_text(), "");
}

#[test]
fn paste_script_contains_event_and_prevent_default_guard() {
    let script =
        paste_script(&json!([{ "entries": [{ "mime_type": "text/plain", "text": "a" }] }]));
    assert!(script.contains("new view.ClipboardEvent(\"paste\""));
    assert!(script.contains("if(!active.dispatchEvent(ev)) return true"));
    assert!(script.contains("new view.File"));
    assert!(!script.contains("navigator.clipboard"));
}

#[test]
fn shortcut_recognizer_only_handles_paste_chords() {
    let keys = |values: &[&str]| values.iter().map(|s| (*s).to_string()).collect::<Vec<_>>();
    assert_eq!(
        shortcut_action(&keys(&["Ctrl", "v"])),
        Some(ClipboardShortcut::Paste)
    );
    assert_eq!(
        shortcut_action(&keys(&["Meta", "Shift", "V"])),
        Some(ClipboardShortcut::PastePlainText)
    );
    assert_eq!(shortcut_action(&keys(&["Ctrl", "c"])), None);
    assert_eq!(shortcut_action(&keys(&["Alt", "v"])), None);
}

fn setup() -> (codex_browser_bridge::client::Client, DuplexStream) {
    let (client, peer) = duplex(32 * 1024);
    (
        codex_browser_bridge::client::Client::from_stream(client).unwrap(),
        peer,
    )
}

async fn next(peer: &mut DuplexStream) -> Value {
    let bytes = timeout(Duration::from_secs(2), protocol::decode_frame(peer))
        .await
        .unwrap()
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn paste_sends_runtime_evaluate_wire() {
    let (client, mut peer) = setup();
    client.mark_attached(7).await;
    let mut clipboard = VirtualClipboard::new();
    clipboard.write_text("paste me").unwrap();
    let task = tokio::spawn(async move { clipboard.paste(&client, 7).await });
    let health = next(&mut peer).await;
    assert_eq!(health["params"]["method"], "Runtime.evaluate");
    assert_eq!(health["params"]["commandParams"]["expression"], "true");
    protocol::encode_frame(
        &mut peer,
        &json!({"id": health["id"], "result": {"result": {"value": true}}}),
    )
    .await
    .unwrap();
    let request = next(&mut peer).await;
    assert_eq!(request["method"], "executeCdp");
    assert_eq!(request["params"]["target"]["tabId"], 7);
    assert_eq!(request["params"]["method"], "Runtime.evaluate");
    let expression = request["params"]["commandParams"]["expression"]
        .as_str()
        .unwrap();
    assert!(expression.contains("ClipboardEvent"));
    assert!(expression.contains("paste me"));
    protocol::encode_frame(
        &mut peer,
        &json!({"id": request["id"], "result": {"result": {"value": true}}}),
    )
    .await
    .unwrap();
    timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}
