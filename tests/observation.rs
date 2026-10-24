use codex_browser_bridge::client::Client;
use codex_browser_bridge::observation::LoadState;
use codex_browser_bridge::protocol::{decode_frame, encode_frame};
use serde_json::{json, Value};
use tokio::io::{duplex, DuplexStream};
use tokio::time::{timeout, Duration};

const DEADLINE: Duration = Duration::from_secs(3);

async fn request(server: &mut DuplexStream) -> Value {
    let bytes = timeout(DEADLINE, decode_frame(server))
        .await
        .expect("bounded mock read")
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

async fn reply(server: &mut DuplexStream, request: &Value, result: Value) {
    timeout(
        DEADLINE,
        encode_frame(server, &json!({"id": request["id"], "result": result})),
    )
    .await
    .expect("bounded mock write")
    .unwrap();
}

async fn event(server: &mut DuplexStream, tab: i64, method: &str, params: Value) {
    timeout(DEADLINE, encode_frame(server, &json!({"method":"onCDPEvent", "params":{"source":{"tabId":tab}, "method":method, "params":params}}))).await.expect("bounded event write").unwrap();
}

async fn start(server: &mut DuplexStream, events: bool) {
    for method in ["detach", "attach"] {
        let req = request(server).await;
        assert_eq!(req["method"], method);
        reply(server, &req, json!({})).await;
    }
    for method in [
        "Page.enable",
        "Page.setLifecycleEventsEnabled",
        "Runtime.enable",
        "Log.enable",
        "Network.enable",
        "Page.getFrameTree",
        "Runtime.evaluate",
    ] {
        let req = request(server).await;
        assert_eq!(req["method"], "executeCdp");
        assert_eq!(req["params"]["method"], method);
        assert_eq!(req["params"]["target"]["tabId"], 7);
        if events && method == "Runtime.enable" {
            // Runtime.enable may replay cached messages before its ACK.
            event(
                server,
                7,
                "Runtime.consoleAPICalled",
                json!({"type":"log", "timestamp":123, "args":[{"value":"earlier"}]}),
            )
            .await;
            event(
                server,
                8,
                "Runtime.consoleAPICalled",
                json!({"args":[{"value":"not ours"}]}),
            )
            .await;
            encode_frame(server, &json!({"method":"Runtime.consoleAPICalled", "params":{"args":[{"value":"untagged"}]}})).await.unwrap();
        }
        if events && method == "Log.enable" {
            event(
                server,
                7,
                "Log.entryAdded",
                json!({"entry":{"level":"warning", "text":"browser entry", "timestamp":124}}),
            )
            .await;
        }
        let result = match method {
            "Page.getFrameTree" => {
                json!({"frameTree":{"frame":{"id":"main", "loaderId":"a", "url":"https://example.com/"}, "childFrames":[{"frame":{"id":"child", "parentId":"main", "loaderId":"sub", "url":"https://example.com/frame"}}]}})
            }
            "Runtime.evaluate" => json!({"result":{"type":"string", "value":"complete"}}),
            _ => json!({}),
        };
        reply(server, &req, result).await;
    }
}

#[tokio::test]
async fn persistent_observation_stores_events_before_enable_ack_and_between_calls() {
    let (transport, mut server) = duplex(128 * 1024);
    let client = Client::from_stream(transport).unwrap();
    let observer = tokio::spawn({
        let client = client.clone();
        async move { client.start_observation(7, DEADLINE).await }
    });
    start(&mut server, true).await;
    let initial = timeout(DEADLINE, observer).await.unwrap().unwrap().unwrap();
    assert!(initial.enabled && initial.load && initial.domcontentloaded);
    assert_eq!(initial.frames.len(), 2);
    let logs = client
        .read_observed_events(
            7,
            None,
            &["Runtime.consoleAPICalled", "Log.entryAdded"],
            10,
            64 * 1024,
        )
        .unwrap();
    assert_eq!(logs.events.len(), 2);
    assert_eq!(logs.dropped_event_count, 0);
    assert_eq!(
        logs.events[0].event["params"]["args"][0]["value"],
        "earlier"
    );
    event(
        &mut server,
        7,
        "Page.frameNavigated",
        json!({"frame":{"id":"main", "loaderId":"b", "url":"https://example.com/next"}}),
    )
    .await;
    event(
        &mut server,
        7,
        "Page.lifecycleEvent",
        json!({"frameId":"main", "loaderId":"b", "name":"load"}),
    )
    .await;
    let navigated = client
        .wait_for_observed_navigation(
            7,
            &initial.cursor,
            Some("https://example.com/next"),
            LoadState::Load,
            DEADLINE,
        )
        .await
        .unwrap();
    assert_eq!(navigated.document_epoch, initial.document_epoch + 1);
    assert_eq!(navigated.frames.len(), 1);
    let tail = client
        .read_observed_events(7, Some(&logs.cursor), &[], 10, 64 * 1024)
        .unwrap();
    assert_eq!(tail.events.len(), 2);
    client.retire_tab_state(7).await;
    assert!(client.observation_snapshot(7).is_err());
    client.shutdown().await;
}

#[tokio::test]
async fn observation_start_reuses_enabled_state_and_dialog_ids_are_stale_after_close() {
    let (transport, mut server) = duplex(128 * 1024);
    let client = Client::from_stream(transport).unwrap();
    let observer = tokio::spawn({
        let client = client.clone();
        async move { client.start_observation(7, DEADLINE).await }
    });
    start(&mut server, false).await;
    let initial = timeout(DEADLINE, observer).await.unwrap().unwrap().unwrap();
    let again = client.start_observation(7, DEADLINE).await.unwrap();
    assert_eq!(again.cursor, initial.cursor);
    assert!(
        timeout(Duration::from_millis(30), decode_frame(&mut server))
            .await
            .is_err()
    );
    event(
        &mut server,
        7,
        "Page.javascriptDialogOpening",
        json!({"type":"confirm", "message":"continue?"}),
    )
    .await;
    event(
        &mut server,
        7,
        "Page.fileChooserOpened",
        json!({"backendNodeId":42, "frameId":"main", "mode":"selectMultiple"}),
    )
    .await;
    // A subsequent answered request provides a deterministic read-loop barrier.
    let barrier = tokio::spawn({
        let client = client.clone();
        async move { client.send_request("getInfo", None).await }
    });
    let req = request(&mut server).await;
    reply(
        &mut server,
        &req,
        json!({"type":"extension", "family":"chrome"}),
    )
    .await;
    timeout(DEADLINE, barrier).await.unwrap().unwrap().unwrap();
    let state = client.observation_snapshot(7).unwrap();
    let dialog = state.dialog.unwrap();
    let chooser = state.file_chooser.unwrap();
    assert!(client.validate_dialog(7, &dialog.id).is_ok());
    assert_eq!(
        client
            .validate_file_chooser(7, &chooser.id)
            .unwrap()
            .backend_node_id,
        42
    );
    client.close_observed_dialog(7, &dialog.id);
    client.close_observed_file_chooser(7, &chooser.id);
    assert!(client.validate_dialog(7, &dialog.id).is_err());
    assert!(client.validate_file_chooser(7, &chooser.id).is_err());
    client.invalidate_attachment(7).await;
    assert!(client
        .read_observed_events(7, Some(&initial.cursor), &[], 10, 64 * 1024)
        .is_err());
    client.shutdown().await;
}

#[tokio::test]
async fn failed_or_timed_out_start_does_not_leave_active_observation() {
    let (transport, mut server) = duplex(4096);
    let client = Client::from_stream(transport).unwrap();
    let observer = tokio::spawn({
        let client = client.clone();
        async move { client.start_observation(7, Duration::from_millis(80)).await }
    });
    let req = request(&mut server).await;
    assert_eq!(req["method"], "detach");
    reply(&mut server, &req, json!({})).await;
    let req = request(&mut server).await;
    assert_eq!(req["method"], "attach");
    reply(&mut server, &req, json!({})).await;
    assert_eq!(
        request(&mut server).await["params"]["method"],
        "Page.enable"
    );
    assert!(timeout(DEADLINE, observer).await.unwrap().unwrap().is_err());
    assert!(client.observation_snapshot(7).is_err());
    client.shutdown().await;
}

#[tokio::test]
async fn chooser_interception_is_disarmed_before_turn_cleanup() {
    let (transport, mut server) = duplex(128 * 1024);
    let client = Client::from_stream(transport).unwrap();
    let observer = tokio::spawn({
        let client = client.clone();
        async move { client.start_file_chooser_interception(7, DEADLINE).await }
    });
    start(&mut server, false).await;
    let req = request(&mut server).await;
    assert_eq!(
        req["params"]["method"],
        "Page.setInterceptFileChooserDialog"
    );
    assert_eq!(req["params"]["commandParams"]["enabled"], true);
    reply(&mut server, &req, json!({})).await;
    timeout(DEADLINE, observer).await.unwrap().unwrap().unwrap();
    event(
        &mut server,
        7,
        "Page.fileChooserOpened",
        json!({"backendNodeId":88, "frameId":"main", "mode":"selectSingle"}),
    )
    .await;
    let stopping = tokio::spawn({
        let client = client.clone();
        async move { client.stop_file_chooser_interception(7, DEADLINE).await }
    });
    let req = request(&mut server).await;
    assert_eq!(req["params"]["commandParams"]["enabled"], false);
    reply(&mut server, &req, json!({})).await;
    timeout(DEADLINE, stopping).await.unwrap().unwrap().unwrap();
    assert_eq!(
        client
            .observation_snapshot(7)
            .unwrap()
            .file_chooser
            .unwrap()
            .backend_node_id,
        88
    );
    let rearming = tokio::spawn({
        let client = client.clone();
        async move { client.start_file_chooser_interception(7, DEADLINE).await }
    });
    let req = request(&mut server).await;
    assert_eq!(req["params"]["commandParams"]["enabled"], true);
    reply(&mut server, &req, json!({})).await;
    timeout(DEADLINE, rearming).await.unwrap().unwrap().unwrap();
    assert!(client
        .observation_snapshot(7)
        .unwrap()
        .file_chooser
        .is_none());
    let ending = tokio::spawn({
        let client = client.clone();
        async move { client.end_turn().await }
    });
    let req = request(&mut server).await;
    assert_eq!(
        req["params"]["method"],
        "Page.setInterceptFileChooserDialog"
    );
    assert_eq!(req["params"]["commandParams"]["enabled"], false);
    reply(&mut server, &req, json!({})).await;
    let req = request(&mut server).await;
    assert_eq!(req["method"], "detach");
    reply(&mut server, &req, json!({})).await;
    let req = request(&mut server).await;
    assert_eq!(req["method"], "turnEnded");
    reply(&mut server, &req, json!({})).await;
    assert!(timeout(DEADLINE, ending).await.unwrap().unwrap().unwrap());
    assert!(client.observation_snapshot(7).is_err());
    client.shutdown().await;
}

#[tokio::test]
async fn lost_interceptor_ack_still_attempts_disarm_on_shutdown() {
    let (transport, mut server) = duplex(128 * 1024);
    let client = Client::from_stream(transport).unwrap();
    let observer = tokio::spawn({
        let client = client.clone();
        async move {
            client
                .start_file_chooser_interception(7, Duration::from_millis(200))
                .await
        }
    });
    start(&mut server, false).await;
    let req = request(&mut server).await;
    assert_eq!(req["params"]["commandParams"]["enabled"], true);
    assert!(timeout(DEADLINE, observer).await.unwrap().unwrap().is_err());
    let shutdown = tokio::spawn({
        let client = client.clone();
        async move { client.shutdown().await }
    });
    let req = request(&mut server).await;
    assert_eq!(
        req["params"]["method"],
        "Page.setInterceptFileChooserDialog"
    );
    assert_eq!(req["params"]["commandParams"]["enabled"], false);
    reply(&mut server, &req, json!({})).await;
    timeout(DEADLINE, shutdown).await.unwrap().unwrap();
    assert!(client.observation_snapshot(7).is_err());
}
