use codex_browser_bridge::{client::Client, pointer, protocol};
use serde_json::{json, Value};
use tokio::{
    io::{duplex, DuplexStream},
    time::{timeout, Duration},
};

async fn next(peer: &mut DuplexStream) -> Value {
    let frame = timeout(Duration::from_secs(2), protocol::decode_frame(peer))
        .await
        .expect("pointer request timed out")
        .unwrap();
    serde_json::from_slice(&frame).unwrap()
}

async fn reply(peer: &mut DuplexStream, request: &Value, result: Value) {
    protocol::encode_frame(peer, &json!({"id":request["id"],"result":result}))
        .await
        .unwrap();
}

async fn next_mouse(peer: &mut DuplexStream) -> (Value, Value) {
    let request = next(peer).await;
    assert_eq!(request["method"], "executeCdp");
    assert_eq!(request["params"]["target"]["tabId"], 7);
    assert_eq!(request["params"]["method"], "Input.dispatchMouseEvent");
    let command = request["params"]["commandParams"].clone();
    (request, command)
}

fn client_pair() -> (Client, DuplexStream) {
    let (stream, peer) = duplex(16384);
    (Client::from_stream(stream).unwrap(), peer)
}

#[tokio::test]
async fn click_matches_official_button_modifier_and_double_click_shape() {
    let (client, mut peer) = client_pair();
    client.mark_attached(7).await;
    let task = tokio::spawn(async move {
        pointer::click(
            &client,
            7,
            12.5,
            33.25,
            3,
            2,
            &["Control".into(), "Shift".into()],
        )
        .await
    });

    for (kind, count) in [
        ("mousePressed", 1),
        ("mouseReleased", 1),
        ("mousePressed", 2),
        ("mouseReleased", 2),
    ] {
        let (request, mouse) = next_mouse(&mut peer).await;
        assert_eq!(mouse["type"], kind);
        assert_eq!(mouse["button"], "right");
        assert_eq!(mouse["modifiers"], 10);
        assert_eq!(mouse["clickCount"], count);
        assert_eq!(mouse["buttons"], if kind == "mousePressed" { 2 } else { 0 });
        reply(&mut peer, &request, json!({})).await;
    }
    timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn move_and_scroll_carry_official_modifier_mask() {
    let (client, mut peer) = client_pair();
    client.mark_attached(7).await;

    let worker = client.clone();
    let task =
        tokio::spawn(async move { pointer::move_to(&worker, 7, 4.0, 5.0, &["Alt".into()]).await });
    let (request, mouse) = next_mouse(&mut peer).await;
    assert_eq!(mouse["type"], "mouseMoved");
    assert_eq!(mouse["modifiers"], 1);
    assert_eq!(mouse["x"], 4.0);
    assert_eq!(mouse["y"], 5.0);
    assert_eq!(mouse["button"], "none");
    reply(&mut peer, &request, json!({})).await;
    task.await.unwrap().unwrap();

    let worker = client.clone();
    let task = tokio::spawn(async move {
        pointer::scroll(&worker, 7, 8.0, 9.0, -20.0, 30.0, &["Meta".into()]).await
    });
    let request = next(&mut peer).await;
    assert_eq!(request["params"]["method"], "Input.dispatchMouseEvent");
    assert_eq!(request["params"]["commandParams"]["type"], "mouseMoved");
    reply(&mut peer, &request, json!({})).await;
    let request = next(&mut peer).await;
    assert_eq!(request["params"]["method"], "Input.synthesizeScrollGesture");
    assert_eq!(request["params"]["commandParams"]["xDistance"], 20.0);
    assert_eq!(request["params"]["commandParams"]["yDistance"], -30.0);
    reply(&mut peer, &request, json!({})).await;
    task.await.unwrap().unwrap();
}

#[tokio::test]
async fn drag_uses_every_supplied_path_point_and_releases() {
    let (client, mut peer) = client_pair();
    client.mark_attached(7).await;
    let task = tokio::spawn(async move {
        pointer::drag(
            &client,
            7,
            &[(1.0, 2.0), (3.0, 4.0), (5.0, 6.0)],
            &["Shift".into()],
        )
        .await
    });

    let (request, mouse) = next_mouse(&mut peer).await;
    assert_eq!(mouse["type"], "mousePressed");
    assert_eq!(mouse["x"], 1.0);
    assert_eq!(mouse["y"], 2.0);
    reply(&mut peer, &request, json!({})).await;

    let (request, mouse) = next_mouse(&mut peer).await;
    assert_eq!(mouse["type"], "mouseMoved");
    assert_eq!(mouse["x"], 3.0);
    assert_eq!(mouse["y"], 4.0);
    assert_eq!(mouse["buttons"], 1);
    reply(&mut peer, &request, json!({})).await;

    let (request, mouse) = next_mouse(&mut peer).await;
    assert_eq!(mouse["type"], "mouseMoved");
    assert_eq!(mouse["x"], 5.0);
    assert_eq!(mouse["y"], 6.0);
    assert_eq!(mouse["buttons"], 1);
    reply(&mut peer, &request, json!({})).await;

    let (request, mouse) = next_mouse(&mut peer).await;
    assert_eq!(mouse["type"], "mouseReleased");
    assert_eq!(mouse["x"], 5.0);
    assert_eq!(mouse["y"], 6.0);
    assert_eq!(mouse["buttons"], 0);
    reply(&mut peer, &request, json!({})).await;

    timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn drag_press_failure_releases_the_button() {
    let (client, mut peer) = client_pair();
    client.mark_attached(7).await;
    let task =
        tokio::spawn(
            async move { pointer::drag(&client, 7, &[(1.0, 2.0), (3.0, 4.0)], &[]).await },
        );

    let (request, mouse) = next_mouse(&mut peer).await;
    assert_eq!(mouse["type"], "mousePressed");
    reply(
        &mut peer,
        &request,
        json!({"error":{"code":-1,"message":"injected press failure"}}),
    )
    .await;
    let (request, mouse) = next_mouse(&mut peer).await;
    assert_eq!(mouse["type"], "mouseReleased");
    assert_eq!(mouse["x"], 1.0);
    assert_eq!(mouse["y"], 2.0);
    reply(&mut peer, &request, json!({})).await;

    let error = timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap_err();
    assert!(error.to_string().contains("injected press failure"));
}

#[tokio::test]
async fn invalid_pointer_options_fail_before_any_browser_request() {
    let (client, mut peer) = client_pair();
    for (x, y, button, modifiers) in [
        (-1.0, 2.0, 1, vec![]),
        (1.0, f64::NAN, 1, vec![]),
        (1.0, 2.0, 6, vec![]),
        (
            1.0,
            2.0,
            1,
            vec!["Control".to_string(), "Control".to_string()],
        ),
        (1.0, 2.0, 1, vec!["a".to_string()]),
    ] {
        assert!(pointer::click(&client, 7, x, y, button, 1, &modifiers)
            .await
            .is_err());
    }
    assert!(
        timeout(Duration::from_millis(50), protocol::decode_frame(&mut peer))
            .await
            .is_err()
    );
}
