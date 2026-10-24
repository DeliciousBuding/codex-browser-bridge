use codex_browser_bridge::{client::Client, pointer, protocol};
use serde_json::{json, Value};
use tokio::{
    io::{duplex, DuplexStream},
    time::{timeout, Duration},
};

const WAIT: Duration = Duration::from_secs(5);

async fn next(peer: &mut DuplexStream) -> Value {
    let frame = timeout(WAIT, protocol::decode_frame(peer))
        .await
        .expect("bounded modal test read")
        .unwrap();
    serde_json::from_slice(&frame).unwrap()
}

async fn reply(peer: &mut DuplexStream, request: &Value, result: Value) {
    timeout(
        WAIT,
        protocol::encode_frame(peer, &json!({"id":request["id"],"result":result})),
    )
    .await
    .unwrap()
    .unwrap();
}

async fn event(peer: &mut DuplexStream, tab: i64, method: &str, params: Value) {
    timeout(WAIT,protocol::encode_frame(peer,&json!({"method":"onCDPEvent","params":{"source":{"tabId":tab},"method":method,"params":params}}))).await.unwrap().unwrap();
}

async fn start(client: &Client, peer: &mut DuplexStream) {
    let worker = client.clone();
    let task = tokio::spawn(async move { worker.start_observation(7, WAIT).await });
    for method in [
        "detach",
        "attach",
        "Page.enable",
        "Page.setLifecycleEventsEnabled",
        "Runtime.enable",
        "Log.enable",
        "Network.enable",
        "Page.getFrameTree",
        "Runtime.evaluate",
    ] {
        let request = next(peer).await;
        if matches!(method, "detach" | "attach") {
            assert_eq!(request["method"], method);
        } else {
            assert_eq!(request["params"]["method"], method);
        }
        let result = match method {
            "Page.getFrameTree" => {
                json!({"frameTree":{"frame":{"id":"main","loaderId":"a","url":"https://example.com/"}}})
            }
            "Runtime.evaluate" => json!({"result":{"type":"string","value":"complete"}}),
            _ => json!({}),
        };
        reply(peer, &request, result).await;
    }
    timeout(WAIT, task).await.unwrap().unwrap().unwrap();
}

async fn begin_click(
    client: &Client,
    peer: &mut DuplexStream,
) -> tokio::task::JoinHandle<codex_browser_bridge::error::Result<()>> {
    let worker = client.clone();
    let task = tokio::spawn(async move { pointer::click(&worker, 7, 10.0, 20.0, 1, 1, &[]).await });
    let request = next(peer).await;
    assert_eq!(request["params"]["method"], "Runtime.evaluate");
    reply(
        peer,
        &request,
        json!({"result":{"type":"string","value":"visible"}}),
    )
    .await;
    let request = next(peer).await;
    assert_eq!(request["params"]["commandParams"]["type"], "mousePressed");
    reply(peer, &request, json!({})).await;
    task
}

#[tokio::test]
async fn new_dialog_on_release_confirms_delivery_without_replaying_release() {
    let (stream, mut peer) = duplex(128 * 1024);
    let client = Client::from_stream(stream).unwrap();
    start(&client, &mut peer).await;
    let task = begin_click(&client, &mut peer).await;
    let release = next(&mut peer).await;
    assert_eq!(release["params"]["commandParams"]["type"], "mouseReleased");
    event(
        &mut peer,
        7,
        "Page.javascriptDialogOpening",
        json!({"type":"alert","message":"clicked","url":"https://example.com/"}),
    )
    .await;
    // Deliberately do not ACK: modal blocked Runtime command processing.
    timeout(WAIT, task).await.unwrap().unwrap().unwrap();
    assert!(client.observation_snapshot(7).unwrap().dialog.is_some());
    assert!(
        timeout(Duration::from_millis(75), protocol::decode_frame(&mut peer))
            .await
            .is_err(),
        "confirmed release must not be replayed"
    );
}

#[tokio::test]
async fn new_file_chooser_on_release_confirms_delivery_without_replaying_release() {
    let (stream, mut peer) = duplex(128 * 1024);
    let client = Client::from_stream(stream).unwrap();
    start(&client, &mut peer).await;
    let task = begin_click(&client, &mut peer).await;
    let release = next(&mut peer).await;
    assert_eq!(release["params"]["commandParams"]["type"], "mouseReleased");
    event(
        &mut peer,
        7,
        "Page.fileChooserOpened",
        json!({"backendNodeId":42,"frameId":"main","mode":"selectSingle"}),
    )
    .await;
    timeout(WAIT, task).await.unwrap().unwrap().unwrap();
    assert!(client
        .observation_snapshot(7)
        .unwrap()
        .file_chooser
        .is_some());
    assert!(
        timeout(Duration::from_millis(75), protocol::decode_frame(&mut peer))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn preexisting_dialog_cannot_confirm_a_silent_release() {
    let (stream, mut peer) = duplex(128 * 1024);
    let client = Client::from_stream(stream).unwrap();
    start(&client, &mut peer).await;
    event(
        &mut peer,
        7,
        "Page.javascriptDialogOpening",
        json!({"type":"alert","message":"earlier"}),
    )
    .await;
    // Wait for authoritative observation before the release precondition.
    timeout(WAIT, async {
        loop {
            if client.observation_snapshot(7).unwrap().dialog.is_some() {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let task = begin_click(&client, &mut peer).await;
    let release = next(&mut peer).await;
    assert_eq!(release["params"]["commandParams"]["type"], "mouseReleased");
    // Unchanged modal is not evidence. Existing failure cleanup remains.
    let cleanup = next(&mut peer).await;
    assert_eq!(cleanup["params"]["commandParams"]["type"], "mouseReleased");
    reply(&mut peer, &cleanup, json!({})).await;
    assert!(timeout(WAIT, task).await.unwrap().unwrap().is_err());
}

#[tokio::test]
async fn unobserved_tab_does_not_turn_modal_events_into_success() {
    let (stream, mut peer) = duplex(128 * 1024);
    let client = Client::from_stream(stream).unwrap();
    client.mark_attached(7).await;
    let task = begin_click(&client, &mut peer).await;
    let release = next(&mut peer).await;
    assert_eq!(release["params"]["commandParams"]["type"], "mouseReleased");
    event(
        &mut peer,
        7,
        "Page.javascriptDialogOpening",
        json!({"type":"alert","message":"unobserved"}),
    )
    .await;
    let cleanup = next(&mut peer).await;
    assert_eq!(cleanup["params"]["commandParams"]["type"], "mouseReleased");
    reply(&mut peer, &cleanup, json!({})).await;
    assert!(timeout(WAIT, task).await.unwrap().unwrap().is_err());
}

#[tokio::test]
async fn neighbor_tab_modal_cannot_confirm_this_tabs_release() {
    let (stream, mut peer) = duplex(128 * 1024);
    let client = Client::from_stream(stream).unwrap();
    start(&client, &mut peer).await;
    let task = begin_click(&client, &mut peer).await;
    let release = next(&mut peer).await;
    assert_eq!(release["params"]["commandParams"]["type"], "mouseReleased");
    event(
        &mut peer,
        8,
        "Page.javascriptDialogOpening",
        json!({"type":"alert","message":"other tab"}),
    )
    .await;
    let cleanup = next(&mut peer).await;
    assert_eq!(cleanup["params"]["commandParams"]["type"], "mouseReleased");
    reply(&mut peer, &cleanup, json!({})).await;
    assert!(timeout(WAIT, task).await.unwrap().unwrap().is_err());
}

#[tokio::test]
async fn modal_opened_on_press_is_not_evidence_that_release_was_delivered() {
    let (stream, mut peer) = duplex(128 * 1024);
    let client = Client::from_stream(stream).unwrap();
    start(&client, &mut peer).await;
    let worker = client.clone();
    let task = tokio::spawn(async move { pointer::click(&worker, 7, 10.0, 20.0, 1, 1, &[]).await });
    let request = next(&mut peer).await;
    reply(
        &mut peer,
        &request,
        json!({"result":{"type":"string","value":"visible"}}),
    )
    .await;
    let press = next(&mut peer).await;
    assert_eq!(press["params"]["commandParams"]["type"], "mousePressed");
    event(
        &mut peer,
        7,
        "Page.javascriptDialogOpening",
        json!({"type":"alert","message":"mousedown"}),
    )
    .await;
    timeout(WAIT, async {
        loop {
            if client.observation_snapshot(7).unwrap().dialog.is_some() {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    reply(&mut peer, &press, json!({})).await;
    let release = next(&mut peer).await;
    assert_eq!(release["params"]["commandParams"]["type"], "mouseReleased");
    let cleanup = next(&mut peer).await;
    assert_eq!(cleanup["params"]["commandParams"]["type"], "mouseReleased");
    reply(&mut peer, &cleanup, json!({})).await;
    assert!(timeout(WAIT, task).await.unwrap().unwrap().is_err());
}
