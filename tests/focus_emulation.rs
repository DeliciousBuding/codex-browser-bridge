use codex_browser_bridge::{browser, client::Client, pointer, protocol};
use serde_json::{json, Value};
use tokio::{
    io::{duplex, DuplexStream},
    time::{timeout, Duration},
};

const BOUND: Duration = Duration::from_secs(2);

fn setup() -> (Client, DuplexStream) {
    let (stream, peer) = duplex(16 * 1024);
    (Client::from_stream(stream).unwrap(), peer)
}

async fn request(peer: &mut DuplexStream) -> Value {
    serde_json::from_slice(
        &timeout(BOUND, protocol::decode_frame(peer))
            .await
            .expect("bounded request")
            .unwrap(),
    )
    .unwrap()
}

async fn reply(peer: &mut DuplexStream, request: &Value, result: Value) {
    timeout(
        BOUND,
        protocol::encode_frame(peer, &json!({"id": request["id"], "result": result})),
    )
    .await
    .unwrap()
    .unwrap();
}

async fn attach(peer: &mut DuplexStream) -> Value {
    let detach = request(peer).await;
    assert_eq!(detach["method"], "detach");
    reply(peer, &detach, json!({})).await;
    let attach = request(peer).await;
    assert_eq!(attach["method"], "attach");
    assert_eq!(attach["params"]["tabId"], 7);
    assert_eq!(
        attach["params"]["networkPolicy"],
        json!({"constraints": []})
    );
    reply(peer, &attach, Value::Null).await;
    let focus = request(peer).await;
    assert_eq!(focus["method"], "executeCdp");
    assert_eq!(focus["params"]["target"]["tabId"], 7);
    assert_eq!(
        focus["params"]["method"],
        "Emulation.setFocusEmulationEnabled"
    );
    assert_eq!(focus["params"]["commandParams"], json!({"enabled": true}));
    focus
}

async fn no_request(peer: &mut DuplexStream) {
    assert!(
        timeout(Duration::from_millis(60), protocol::decode_frame(peer))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn background_click_uses_native_focus_emulation_without_foreground_requests() {
    let (client, mut peer) = setup();
    let worker = client.clone();
    let task = tokio::spawn(async move { pointer::click(&worker, 7, 10.0, 20.0, 1, 1, &[]).await });
    let focus = attach(&mut peer).await;
    reply(&mut peer, &focus, json!({})).await;
    for (kind, buttons) in [("mousePressed", 1), ("mouseReleased", 0)] {
        let input = request(&mut peer).await;
        assert_eq!(input["params"]["method"], "Input.dispatchMouseEvent");
        assert_eq!(input["params"]["commandParams"]["type"], kind);
        assert_eq!(input["params"]["commandParams"]["buttons"], buttons);
        reply(&mut peer, &input, json!({})).await;
    }
    timeout(BOUND, task).await.unwrap().unwrap().unwrap();
    no_request(&mut peer).await;
    client.shutdown().await;
}

#[tokio::test]
async fn focus_setup_rejection_detaches_and_never_dispatches_or_replays_input() {
    for action in 0..6 {
        let (client, mut peer) = setup();
        let worker = client.clone();
        let task = tokio::spawn(async move {
            match action {
                0 => browser::cua_type(&worker, "7", "probe").await,
                1 => pointer::click(&worker, 7, 10.0, 20.0, 1, 1, &[]).await,
                2 => browser::cua_keypress(&worker, "7", &["Control".into(), "a".into()]).await,
                3 => pointer::drag(&worker, 7, &[(10.0, 20.0), (30.0, 40.0)], &[]).await,
                4 => pointer::scroll(&worker, 7, 10.0, 20.0, 0.0, 100.0, &[]).await,
                _ => pointer::move_to(&worker, 7, 10.0, 20.0, &[]).await,
            }
        });
        let focus = attach(&mut peer).await;
        protocol::encode_frame(
            &mut peer,
            &json!({"id": focus["id"], "error": {"code": -32000, "message": "Not allowed"}}),
        )
        .await
        .unwrap();
        let cleanup = request(&mut peer).await;
        assert_eq!(cleanup["method"], "detach");
        reply(&mut peer, &cleanup, json!({})).await;
        let error = timeout(BOUND, task).await.unwrap().unwrap().unwrap_err();
        assert!(error
            .to_string()
            .contains("background focus emulation failed"));
        no_request(&mut peer).await;

        // A failed setup must not leave a usable sticky attachment behind.
        let worker = client.clone();
        let task = tokio::spawn(async move { browser::cua_type(&worker, "7", "next").await });
        let focus = attach(&mut peer).await;
        reply(&mut peer, &focus, json!({})).await;
        let input = request(&mut peer).await;
        assert_eq!(input["params"]["method"], "Input.insertText");
        assert_eq!(input["params"]["commandParams"]["text"], "next");
        reply(&mut peer, &input, json!({})).await;
        timeout(BOUND, task).await.unwrap().unwrap().unwrap();
        no_request(&mut peer).await;
        client.shutdown().await;
    }
}

#[tokio::test]
async fn sticky_calls_reuse_focus_setup_and_reattach_reapplies_it() {
    let (client, mut peer) = setup();
    for index in 0..3 {
        if index == 2 {
            client.invalidate_attachment(7).await;
        }
        let worker = client.clone();
        let task =
            tokio::spawn(async move { worker.execute_cdp(7, "Page.getLayoutMetrics", None).await });
        if index != 1 {
            let focus = attach(&mut peer).await;
            reply(&mut peer, &focus, json!({})).await;
        }
        let metrics = request(&mut peer).await;
        assert_eq!(metrics["params"]["method"], "Page.getLayoutMetrics");
        reply(
            &mut peer,
            &metrics,
            json!({"cssContentSize": {"width": 800, "height": 600}}),
        )
        .await;
        timeout(BOUND, task).await.unwrap().unwrap().unwrap();
    }
    no_request(&mut peer).await;
    client.shutdown().await;
}
