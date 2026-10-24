use codex_browser_bridge::{client::Client, input, protocol};
use serde_json::{json, Value};
use tokio::io::{duplex, DuplexStream};
use tokio::time::{timeout, Duration};

fn setup() -> (Client, DuplexStream) {
    let (client, peer) = duplex(16384);
    (Client::from_stream(client).unwrap(), peer)
}

async fn next(peer: &mut DuplexStream) -> Value {
    let frame = timeout(Duration::from_secs(2), protocol::decode_frame(peer))
        .await
        .expect("browser request timed out")
        .unwrap();
    serde_json::from_slice(&frame).unwrap()
}

async fn reply(peer: &mut DuplexStream, request: &Value, result: Value) {
    protocol::encode_frame(peer, &json!({"id":request["id"],"result":result}))
        .await
        .unwrap();
}

async fn event(peer: &mut DuplexStream) -> (Value, Value) {
    let request = next(peer).await;
    assert_eq!(request["method"], "executeCdp");
    assert_eq!(request["params"]["target"]["tabId"], 7);
    assert_eq!(request["params"]["method"], "Input.dispatchKeyEvent");
    let event = request["params"]["commandParams"].clone();
    (request, event)
}

fn keys(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).into()).collect()
}

#[tokio::test]
async fn control_c_is_a_chord_with_one_attach_and_reverse_release() {
    let (client, mut peer) = setup();
    let task =
        tokio::spawn(async move { input::keypress(&client, 7, &keys(&["Ctrl", "c"])).await });
    for method in ["detach", "attach"] {
        let request = next(&mut peer).await;
        assert_eq!(request["method"], method);
        assert_eq!(request["params"]["tabId"], 7);
        reply(&mut peer, &request, json!({})).await;
    }
    for (kind, key, code, virtual_key, modifiers) in [
        ("rawKeyDown", "Control", "ControlLeft", 17, 2),
        ("rawKeyDown", "c", "KeyC", 67, 2),
        ("keyUp", "c", "KeyC", 67, 2),
        ("keyUp", "Control", "ControlLeft", 17, 0),
    ] {
        let (request, event) = event(&mut peer).await;
        assert_eq!(event["type"], kind);
        assert_eq!(event["key"], key);
        assert_eq!(event["code"], code);
        assert_eq!(event["windowsVirtualKeyCode"], virtual_key);
        assert_eq!(event["modifiers"], modifiers);
        assert!(event.get("text").is_none(), "shortcut must not insert c");
        reply(&mut peer, &request, json!({})).await;
    }
    timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn multiple_modifiers_accumulate_and_release_individually() {
    let (client, mut peer) = setup();
    client.mark_attached(7).await;
    let task = tokio::spawn(async move {
        input::keypress(
            &client,
            7,
            &keys(&["Control", "Shift", "Alt", "Meta", "ArrowLeft"]),
        )
        .await
    });
    for (kind, key, mask) in [
        ("rawKeyDown", "Control", 2),
        ("rawKeyDown", "Shift", 10),
        ("rawKeyDown", "Alt", 11),
        ("rawKeyDown", "Meta", 15),
        ("rawKeyDown", "ArrowLeft", 15),
        ("keyUp", "ArrowLeft", 15),
        ("keyUp", "Meta", 11),
        ("keyUp", "Alt", 10),
        ("keyUp", "Shift", 2),
        ("keyUp", "Control", 0),
    ] {
        let (request, event) = event(&mut peer).await;
        assert_eq!(event["type"], kind);
        assert_eq!(event["key"], key);
        assert_eq!(event["modifiers"], mask);
        reply(&mut peer, &request, json!({})).await;
    }
    timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn enter_down_has_carriage_return_text_but_up_does_not() {
    let (client, mut peer) = setup();
    client.mark_attached(7).await;
    let task = tokio::spawn(async move { input::keypress(&client, 7, &keys(&["Return"])).await });
    let (request, down) = event(&mut peer).await;
    assert_eq!(down["key"], "Enter");
    assert_eq!(down["code"], "Enter");
    assert_eq!(down["text"], "\r");
    assert_eq!(down["unmodifiedText"], "\r");
    assert_eq!(down["windowsVirtualKeyCode"], 13);
    reply(&mut peer, &request, json!({})).await;
    let (request, up) = event(&mut peer).await;
    assert_eq!(up["type"], "keyUp");
    assert!(up.get("text").is_none());
    reply(&mut peer, &request, json!({})).await;
    timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn common_control_keys_function_keys_and_physical_names_have_cdp_codes() {
    let (client, mut peer) = setup();
    client.mark_attached(7).await;
    for (name, key, code, virtual_key, down_kind) in [
        ("Tab", "Tab", "Tab", 9, "rawKeyDown"),
        ("Backspace", "Backspace", "Backspace", 8, "rawKeyDown"),
        ("Esc", "Escape", "Escape", 27, "rawKeyDown"),
        ("Left", "ArrowLeft", "ArrowLeft", 37, "rawKeyDown"),
        ("F1", "F1", "F1", 112, "rawKeyDown"),
        ("F24", "F24", "F24", 135, "rawKeyDown"),
        ("KeyZ", "z", "KeyZ", 90, "keyDown"),
        ("Digit9", "9", "Digit9", 57, "keyDown"),
    ] {
        let worker = client.clone();
        let combination = keys(&[name]);
        let task = tokio::spawn(async move { input::keypress(&worker, 7, &combination).await });
        for kind in [down_kind, "keyUp"] {
            let (request, event) = event(&mut peer).await;
            assert_eq!(event["type"], kind);
            assert_eq!(event["key"], key);
            assert_eq!(event["code"], code);
            assert_eq!(event["windowsVirtualKeyCode"], virtual_key);
            assert_eq!(
                event["location"], 0,
                "navigation keys are not left/right keyboard locations"
            );
            reply(&mut peer, &request, json!({})).await;
        }
        timeout(Duration::from_secs(2), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }
}

#[tokio::test]
async fn shift_letter_and_digit_produce_correct_text() {
    for (letter, expected, code, virtual_key) in [("a", "A", "KeyA", 65), ("1", "!", "Digit1", 49)]
    {
        let (client, mut peer) = setup();
        client.mark_attached(7).await;
        let combination = keys(&["Shift", letter]);
        let task = tokio::spawn(async move { input::keypress(&client, 7, &combination).await });
        for index in 0..4 {
            let (request, event) = event(&mut peer).await;
            if index == 1 {
                assert_eq!(event["key"], expected);
                assert_eq!(event["text"], expected);
                assert_eq!(event["unmodifiedText"], expected);
                assert_eq!(event["code"], code);
                assert_eq!(event["windowsVirtualKeyCode"], virtual_key);
            }
            reply(&mut peer, &request, json!({})).await;
        }
        timeout(Duration::from_secs(2), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }
}

#[tokio::test]
async fn unknown_or_duplicate_key_fails_before_any_transport_request() {
    let (client, mut peer) = setup();
    for names in [
        vec![],
        vec!["Control", "NotAKey"],
        vec!["Ctrl", "Control"],
        vec!["F25"],
        vec!["中文"],
    ] {
        assert!(input::keypress(&client, 7, &keys(&names)).await.is_err());
    }
    assert!(
        timeout(Duration::from_millis(50), protocol::decode_frame(&mut peer))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn failed_keydown_releases_possibly_delivered_key_and_modifier() {
    let (client, mut peer) = setup();
    client.mark_attached(7).await;
    let task =
        tokio::spawn(async move { input::keypress(&client, 7, &keys(&["Control", "c"])).await });
    for (index, kind, key, mask) in [
        (0, "rawKeyDown", "Control", 2),
        (1, "rawKeyDown", "c", 2),
        (2, "keyUp", "c", 2),
        (3, "keyUp", "Control", 0),
    ] {
        let (request, event) = event(&mut peer).await;
        assert_eq!(event["type"], kind);
        assert_eq!(event["key"], key);
        assert_eq!(event["modifiers"], mask);
        let result = if index == 1 {
            json!({"error":{"code":-1,"message":"injected keydown failure"}})
        } else {
            json!({})
        };
        reply(&mut peer, &request, result).await;
    }
    let error = timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap_err();
    assert!(error.to_string().contains("injected keydown failure"));
}

#[tokio::test]
async fn session_invalid_keydown_is_not_replayed() {
    let (client, mut peer) = setup();
    client.mark_attached(7).await;
    let task = tokio::spawn(async move { input::keypress(&client, 7, &keys(&["Enter"])).await });

    let (request, down) = event(&mut peer).await;
    assert_eq!(down["type"], "keyDown");
    protocol::encode_frame(
        &mut peer,
        &json!({
            "id": request["id"],
            "error": {"code": -1, "message": "Debugger unattached"}
        }),
    )
    .await
    .unwrap();

    // The failed keydown invalidates the cached attachment. Cleanup must
    // re-attach and send the keyup, but must never send a second keydown.
    for method in ["detach", "attach"] {
        let request = next(&mut peer).await;
        assert_eq!(request["method"], method);
        reply(&mut peer, &request, json!({})).await;
    }
    let (request, up) = event(&mut peer).await;
    assert_eq!(up["type"], "keyUp");
    reply(&mut peer, &request, json!({})).await;

    let error = timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap_err();
    assert!(
        error.to_string().contains("outcome unknown")
            || error.to_string().contains("Debugger unattached"),
        "{error}"
    );
    if let Ok(Ok(frame)) = timeout(
        Duration::from_millis(200),
        protocol::decode_frame(&mut peer),
    )
    .await
    {
        panic!(
            "keydown was replayed after an invalid session: {}",
            String::from_utf8_lossy(&frame)
        );
    }
}

#[tokio::test]
async fn failed_keyup_is_retried_and_cleanup_continues_past_release_error() {
    let (client, mut peer) = setup();
    client.mark_attached(7).await;
    let task =
        tokio::spawn(async move { input::keypress(&client, 7, &keys(&["Control", "c"])).await });
    for (index, kind, key) in [
        (0, "rawKeyDown", "Control"),
        (1, "rawKeyDown", "c"),
        (2, "keyUp", "c"),
        (3, "keyUp", "c"),
        (4, "keyUp", "Control"),
    ] {
        let (request, event) = event(&mut peer).await;
        assert_eq!(event["type"], kind);
        assert_eq!(event["key"], key);
        reply(
            &mut peer,
            &request,
            if index == 2 || index == 3 {
                json!({"error":{"code":-1,"message":"injected keyup failure"}})
            } else {
                json!({})
            },
        )
        .await;
    }
    let error = timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap_err();
    assert!(error.to_string().contains("key release cleanup failed"));
    assert!(error.to_string().contains("injected keyup failure"));
}

#[tokio::test]
async fn silent_cleanup_is_bounded_and_still_releases_the_modifier() {
    let (client, mut peer) = setup();
    client.mark_attached(7).await;
    let task =
        tokio::spawn(async move { input::keypress(&client, 7, &keys(&["Control", "c"])).await });
    let (request, _) = event(&mut peer).await;
    reply(&mut peer, &request, json!({})).await;
    let (request, _) = event(&mut peer).await;
    reply(
        &mut peer,
        &request,
        json!({"error":{"code":-1,"message":"injected keydown failure"}}),
    )
    .await;
    let (_, release) = event(&mut peer).await;
    assert_eq!(release["key"], "c"); // Deliberately never acknowledge this release.
    let (request, release) = event(&mut peer).await;
    assert_eq!(release["key"], "Control");
    assert_eq!(release["modifiers"], 0);
    reply(&mut peer, &request, json!({})).await;
    let error = timeout(Duration::from_secs(3), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap_err();
    assert!(error.to_string().contains("key release cleanup failed"));
}

#[tokio::test]
async fn plus_separated_chord_is_expanded_and_keeps_one_final_key() {
    let (client, mut peer) = setup();
    client.mark_attached(7).await;
    let task =
        tokio::spawn(async move { input::keypress(&client, 7, &["Ctrl+a".to_string()]).await });
    for (kind, key, modifiers) in [
        ("rawKeyDown", "Control", 2),
        ("rawKeyDown", "a", 2),
        ("keyUp", "a", 2),
        ("keyUp", "Control", 0),
    ] {
        let (request, event) = event(&mut peer).await;
        assert_eq!(event["type"], kind);
        assert_eq!(event["key"], key);
        assert_eq!(event["modifiers"], modifiers);
        reply(&mut peer, &request, json!({})).await;
    }
    timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn punctuation_and_numpad_include_physical_metadata() {
    let (client, mut peer) = setup();
    client.mark_attached(7).await;
    let task =
        tokio::spawn(async move { input::keypress(&client, 7, &["Shift+1".to_string()]).await });
    for (kind, key, code, virtual_key, location, keypad) in [
        ("rawKeyDown", "Shift", "ShiftLeft", 16, 1, false),
        ("keyDown", "!", "Digit1", 49, 0, false),
        ("keyUp", "!", "Digit1", 49, 0, false),
        ("keyUp", "Shift", "ShiftLeft", 16, 1, false),
    ] {
        let (request, event) = event(&mut peer).await;
        assert_eq!(event["type"], kind);
        assert_eq!(event["key"], key);
        assert_eq!(event["code"], code);
        assert_eq!(event["windowsVirtualKeyCode"], virtual_key);
        assert_eq!(event["location"], location);
        assert_eq!(event["isKeypad"], keypad);
        reply(&mut peer, &request, json!({})).await;
    }
    timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    let (client, mut peer) = setup();
    client.mark_attached(7).await;
    let task =
        tokio::spawn(async move { input::keypress(&client, 7, &["Numpad7".to_string()]).await });
    for kind in ["keyDown", "keyUp"] {
        let (request, event) = event(&mut peer).await;
        assert_eq!(event["type"], kind);
        assert_eq!(event["key"], "7");
        assert_eq!(event["code"], "Numpad7");
        assert_eq!(event["windowsVirtualKeyCode"], 103);
        assert_eq!(event["location"], 3);
        assert_eq!(event["isKeypad"], true);
        reply(&mut peer, &request, json!({})).await;
    }
    timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn uncertain_control_or_meta_and_multiple_non_modifiers_fail_before_send() {
    let (client, mut peer) = setup();
    for keys in [
        vec!["ControlOrMeta".to_string(), "a".to_string()],
        vec!["ControlOrMeta+a".to_string()],
        vec!["Control".to_string(), "a".to_string(), "b".to_string()],
    ] {
        let error = input::keypress(&client, 7, &keys).await.unwrap_err();
        assert!(
            error.to_string().contains("ControlOrMeta")
                || error.to_string().contains("exactly one non-modifier")
        );
    }
    assert!(
        timeout(Duration::from_millis(50), protocol::decode_frame(&mut peer))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn slow_chord_shares_one_operation_deadline_and_still_cleans_up() {
    let (client, mut peer) = setup();
    client.mark_attached(7).await;
    let started = tokio::time::Instant::now();
    let task = tokio::spawn(async move {
        input::keypress(&client, 7, &keys(&["Control", "Shift", "a"])).await
    });
    for key in ["Control", "Shift"] {
        let (request, down) = event(&mut peer).await;
        assert_eq!(down["type"], "rawKeyDown");
        assert_eq!(down["key"], key);
        tokio::time::sleep(Duration::from_millis(1100)).await;
        reply(&mut peer, &request, json!({})).await;
    }
    let (_, down) = event(&mut peer).await;
    assert_eq!(down["key"], "A");
    // Leave the final keyDown unacknowledged. The two preceding 1.1s delays
    // must consume the same 3s budget, not grant this request another 3s.
    for key in ["A", "Shift", "Control"] {
        let (request, up) = event(&mut peer).await;
        assert_eq!(up["type"], "keyUp");
        assert_eq!(up["key"], key);
        reply(&mut peer, &request, json!({})).await;
    }
    let error = timeout(Duration::from_secs(1), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap_err();
    assert!(error.to_string().contains("deadline") || error.to_string().contains("timeout"));
    assert!(started.elapsed() >= Duration::from_millis(2700));
    assert!(started.elapsed() < Duration::from_millis(4500));
}
