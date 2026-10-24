use codex_browser_bridge::{client::Client, locator, protocol};
use serde_json::{json, Value};
use tokio::{
    io::{duplex, DuplexStream},
    time::{timeout, Duration},
};

fn setup() -> (Client, DuplexStream) {
    let (stream, peer) = duplex(262144);
    (Client::from_stream(stream).unwrap(), peer)
}

async fn next(peer: &mut DuplexStream) -> Value {
    let frame = timeout(Duration::from_secs(2), protocol::decode_frame(peer))
        .await
        .expect("locator request timed out")
        .unwrap();
    serde_json::from_slice(&frame).unwrap()
}

async fn reply(peer: &mut DuplexStream, request: &Value, result: Value) {
    protocol::encode_frame(peer, &json!({"id":request["id"],"result":result}))
        .await
        .unwrap();
}

fn expression(request: &Value) -> &str {
    request["params"]["commandParams"]["expression"]
        .as_str()
        .unwrap()
}

async fn locator_reply(peer: &mut DuplexStream, request: &Value, result: Value) {
    assert_eq!(request["method"], "executeCdp");
    assert_eq!(request["params"]["method"], "Runtime.evaluate");
    reply(
        peer,
        request,
        json!({"result":{"type":"string","value":result.to_string()}}),
    )
    .await;
}

fn target(checked: bool) -> Value {
    json!({"ok":true,"rect":{"x":40.0,"y":20.0},"bounds":{"x":10.0,"y":10.0,"width":60.0,"height":20.0},"value":checked,"needs_click":true,"target_changed":false})
}

async fn foreground(peer: &mut DuplexStream) {
    let request = next(peer).await;
    assert_eq!(expression(&request), "document.visibilityState");
    reply(
        peer,
        &request,
        json!({"result":{"type":"string","value":"visible"}}),
    )
    .await;
}

async fn cleanup(peer: &mut DuplexStream) {
    let request = next(peer).await;
    assert!(expression(&request).contains("\"action\":\"release_target\""));
    locator_reply(peer, &request, json!({"ok":true})).await;
}

#[tokio::test]
async fn checked_awaits_async_postcondition_after_exactly_one_trusted_click() {
    let (client, mut peer) = setup();
    client.mark_attached(7).await;
    let worker = client.clone();
    let task = tokio::spawn(async move {
        locator::run(
            &worker,
            "7",
            json!({"selector":"input","action":"set_checked","checked":true,"timeout_ms":2000}),
        )
        .await
    });
    let request = next(&mut peer).await;
    locator_reply(
        &mut peer,
        &request,
        json!({"ok":false,"retryable":true,"error":"matched element is hidden"}),
    )
    .await;
    for _ in 0..2 {
        let request = next(&mut peer).await;
        locator_reply(&mut peer, &request, target(false)).await;
    }
    foreground(&mut peer).await;
    for kind in ["mousePressed", "mouseReleased"] {
        let request = next(&mut peer).await;
        assert_eq!(request["params"]["method"], "Input.dispatchMouseEvent");
        assert_eq!(request["params"]["commandParams"]["type"], kind);
        reply(&mut peer, &request, json!({})).await;
    }
    for checked in [false, false, true] {
        let request = next(&mut peer).await;
        assert!(expression(&request).contains("\"action\":\"checked_probe\""));
        locator_reply(&mut peer, &request, json!({"ok":true,"value":checked})).await;
    }
    cleanup(&mut peer).await;
    let result = timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(result["value"], true);
    assert_eq!(result["verified"], true);
    assert!(
        timeout(Duration::from_millis(75), protocol::decode_frame(&mut peer))
            .await
            .is_err(),
        "input must not be replayed"
    );
}

#[tokio::test]
async fn trial_waits_for_stability_without_any_input_dispatch() {
    let (client, mut peer) = setup();
    client.mark_attached(7).await;
    let worker = client.clone();
    let task = tokio::spawn(async move {
        locator::run(
            &worker,
            "7",
            json!({"selector":"button","action":"click","trial":true,"timeout_ms":2000}),
        )
        .await
    });
    for x in [40.0, 41.0, 41.0] {
        let request = next(&mut peer).await;
        let mut value = target(false);
        value["rect"]["x"] = json!(x);
        locator_reply(&mut peer, &request, value).await;
    }
    cleanup(&mut peer).await;
    assert_eq!(
        timeout(Duration::from_secs(2), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap()["trial"],
        true
    );
    assert!(
        timeout(Duration::from_millis(75), protocol::decode_frame(&mut peer))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn failed_postcondition_has_bounded_timeout_and_never_reclicks() {
    let (client, mut peer) = setup();
    client.mark_attached(7).await;
    let worker = client.clone();
    let task = tokio::spawn(async move {
        locator::run(&worker,"7",json!({"selector":"input","action":"set_checked","checked":true,"force":true,"timeout_ms":350})).await
    });
    let request = next(&mut peer).await;
    locator_reply(&mut peer, &request, target(false)).await;
    foreground(&mut peer).await;
    for kind in ["mousePressed", "mouseReleased"] {
        let request = next(&mut peer).await;
        assert_eq!(request["params"]["commandParams"]["type"], kind);
        reply(&mut peer, &request, json!({})).await;
    }
    loop {
        let request = next(&mut peer).await;
        if expression(&request).contains("\"action\":\"release_target\"") {
            locator_reply(&mut peer, &request, json!({"ok":true})).await;
            break;
        }
        assert!(expression(&request).contains("\"action\":\"checked_probe\""));
        locator_reply(&mut peer, &request, json!({"ok":true,"value":false})).await;
    }
    assert!(timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap_err()
        .to_string()
        .contains("postcondition"));
    assert!(
        timeout(Duration::from_millis(75), protocol::decode_frame(&mut peer))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn sequential_text_appends_through_shared_keys_and_unicode_insert_text() {
    let (client, mut peer) = setup();
    client.mark_attached(7).await;
    let worker = client.clone();
    let task = tokio::spawn(async move {
        locator::run(
            &worker,
            "7",
            json!({"selector":"input","action":"type","value":"a中","timeout_ms":2000}),
        )
        .await
    });
    foreground(&mut peer).await;
    let request = next(&mut peer).await;
    locator_reply(&mut peer, &request, json!({"ok":true,"focused":true})).await;
    for kind in ["keyDown", "keyUp"] {
        let request = next(&mut peer).await;
        assert_eq!(request["params"]["method"], "Input.dispatchKeyEvent");
        assert_eq!(request["params"]["commandParams"]["type"], kind);
        assert_eq!(request["params"]["commandParams"]["key"], "a");
        reply(&mut peer, &request, json!({})).await;
    }
    let request = next(&mut peer).await;
    assert_eq!(request["params"]["method"], "Input.insertText");
    assert_eq!(request["params"]["commandParams"]["text"], "中");
    reply(&mut peer, &request, json!({})).await;
    assert_eq!(
        timeout(Duration::from_secs(2), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap()["characters"],
        2
    );
}

#[tokio::test]
async fn sequential_uppercase_uses_a_real_shift_chord() {
    let (client, mut peer) = setup();
    client.mark_attached(7).await;
    let task = tokio::spawn(async move {
        locator::run(
            &client,
            "7",
            json!({"selector":"input","action":"press_sequentially","value":"A","timeout_ms":2000}),
        )
        .await
    });
    foreground(&mut peer).await;
    let request = next(&mut peer).await;
    locator_reply(&mut peer, &request, json!({"ok":true,"focused":true})).await;
    for (kind, key, mask) in [
        ("rawKeyDown", "Shift", 8),
        ("keyDown", "A", 8),
        ("keyUp", "A", 8),
        ("keyUp", "Shift", 0),
    ] {
        let request = next(&mut peer).await;
        assert_eq!(request["params"]["method"], "Input.dispatchKeyEvent");
        assert_eq!(request["params"]["commandParams"]["type"], kind);
        assert_eq!(request["params"]["commandParams"]["key"], key);
        assert_eq!(request["params"]["commandParams"]["modifiers"], mask);
        if kind == "keyDown" {
            assert_eq!(request["params"]["commandParams"]["text"], "A");
        }
        reply(&mut peer, &request, json!({})).await;
    }
    assert_eq!(
        timeout(Duration::from_secs(2), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap()["characters"],
        1
    );
}

#[tokio::test]
async fn invalid_composition_fails_before_transport() {
    let (client, mut peer) = setup();
    let result = locator::run(
        &client,
        "7",
        json!({"action":"count","has":{"frame_selectors":["iframe"]}}),
    )
    .await;
    assert!(result.is_err());
    assert!(
        timeout(Duration::from_millis(75), protocol::decode_frame(&mut peer))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn force_bypasses_stability_and_dispatch_failure_does_not_replay() {
    let (client, mut peer) = setup();
    client.mark_attached(7).await;
    let worker = client.clone();
    let task = tokio::spawn(async move {
        locator::run(
            &worker,
            "7",
            json!({"selector":"button","action":"click","force":true,"timeout_ms":2000}),
        )
        .await
    });
    let request = next(&mut peer).await;
    locator_reply(&mut peer, &request, target(false)).await;
    foreground(&mut peer).await;
    let request = next(&mut peer).await;
    assert_eq!(request["params"]["commandParams"]["type"], "mousePressed");
    protocol::encode_frame(
        &mut peer,
        &json!({"id":request["id"],"error":{"code":-32000,"message":"dispatch outcome unknown"}}),
    )
    .await
    .unwrap();
    let request = next(&mut peer).await;
    assert_eq!(request["params"]["commandParams"]["type"], "mouseReleased");
    reply(&mut peer, &request, json!({})).await;
    cleanup(&mut peer).await;
    assert!(timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap_err()
        .to_string()
        .contains("unknown"));
    assert!(
        timeout(Duration::from_millis(75), protocol::decode_frame(&mut peer))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn dom_mutation_unknown_outcome_is_never_retried() {
    let (client, mut peer) = setup();
    client.mark_attached(7).await;
    let worker = client.clone();
    let task = tokio::spawn(async move {
        locator::run(
            &worker,
            "7",
            json!({"selector":"input","action":"fill","value":"new","timeout_ms":2000}),
        )
        .await
    });
    let request = next(&mut peer).await;
    protocol::encode_frame(
        &mut peer,
        &json!({"id":request["id"],"error":{"code":-32000,"message":"session not found"}}),
    )
    .await
    .unwrap();
    assert!(timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap_err()
        .to_string()
        .contains("not replayed"));
    assert!(
        timeout(Duration::from_millis(75), protocol::decode_frame(&mut peer))
            .await
            .is_err()
    );
}
