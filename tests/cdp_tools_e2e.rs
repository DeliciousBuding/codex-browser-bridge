#![cfg(not(windows))]

use codex_browser_bridge::client::Client;
use codex_browser_bridge::{browser, protocol};
use serde_json::{json, Value};
use tokio::io::{duplex, DuplexStream};
use tokio::time::{timeout, Duration};

async fn next_request(server: &mut DuplexStream) -> Value {
    let frame = protocol::decode_frame(server).await.unwrap();
    serde_json::from_slice(&frame).unwrap()
}

async fn reply_result(server: &mut DuplexStream, request: &Value, result: Value) {
    protocol::encode_frame(server, &json!({"id": request["id"], "result": result}))
        .await
        .unwrap();
}

fn test_client() -> (Client, DuplexStream) {
    let (client_end, server_end) = duplex(8192);
    (Client::from_stream(client_end).unwrap(), server_end)
}

#[tokio::test]
async fn browser_calls_use_expected_cdp_sequence_and_sticky_attach() {
    let (client, mut server) = test_client();

    let title_task = tokio::spawn({
        let client = client.clone();
        async move { browser::get_title(&client, "7").await }
    });

    let detach = next_request(&mut server).await;
    assert_eq!(detach["method"], "detach");
    assert_eq!(detach["params"]["tabId"], 7);
    reply_result(&mut server, &detach, json!({})).await;

    let attach = next_request(&mut server).await;
    assert_eq!(attach["method"], "attach");
    assert_eq!(attach["params"]["tabId"], 7);
    reply_result(&mut server, &attach, json!({})).await;

    let evaluate = next_request(&mut server).await;
    assert_eq!(evaluate["method"], "executeCdp");
    assert_eq!(evaluate["params"]["target"]["tabId"], 7);
    assert_eq!(evaluate["params"]["method"], "Runtime.evaluate");
    assert_eq!(
        evaluate["params"]["commandParams"]["expression"],
        "document.title"
    );
    reply_result(
        &mut server,
        &evaluate,
        json!({"result":{"value":"Bridge Test"}}),
    )
    .await;

    assert_eq!(title_task.await.unwrap().unwrap(), "Bridge Test");

    let screenshot_task = tokio::spawn({
        let client = client.clone();
        async move { browser::screenshot(&client, "7", false, "jpeg", Some(72)).await }
    });

    let screenshot = next_request(&mut server).await;
    assert_eq!(
        screenshot["method"], "executeCdp",
        "sticky attach should skip detach/attach"
    );
    assert_eq!(screenshot["params"]["target"]["tabId"], 7);
    assert_eq!(screenshot["params"]["method"], "Page.captureScreenshot");
    assert_eq!(screenshot["params"]["commandParams"]["format"], "jpeg");
    assert_eq!(screenshot["params"]["commandParams"]["quality"], 72);
    reply_result(&mut server, &screenshot, json!({"data":"ZmFrZQ=="})).await;

    assert_eq!(screenshot_task.await.unwrap().unwrap(), "ZmFrZQ==");
}

#[tokio::test]
async fn blocked_navigation_and_cdp_methods_do_not_touch_pipe() {
    let (client, mut server) = test_client();

    assert!(browser::navigate(&client, "7", "file:///C:/secret.txt")
        .await
        .is_err());
    assert!(
        browser::execute_cdp_generic(&client, "7", "Page.navigateToHistoryEntry", None)
            .await
            .is_err()
    );

    let read = timeout(
        Duration::from_millis(50),
        protocol::decode_frame(&mut server),
    )
    .await;
    assert!(
        read.is_err(),
        "blocked calls should fail before writing a frame"
    );
}

#[tokio::test]
async fn print_pdf_uses_bounded_stream_and_closes_handle() {
    let (client, mut server) = test_client();

    let pdf_task = tokio::spawn({
        let client = client.clone();
        async move { browser::print_pdf(&client, "7").await }
    });

    let detach = next_request(&mut server).await;
    assert_eq!(detach["method"], "detach");
    reply_result(&mut server, &detach, json!({})).await;

    let attach = next_request(&mut server).await;
    assert_eq!(attach["method"], "attach");
    reply_result(&mut server, &attach, json!({})).await;

    let print = next_request(&mut server).await;
    assert_eq!(print["method"], "executeCdp");
    assert_eq!(print["params"]["method"], "Page.printToPDF");
    assert_eq!(
        print["params"]["commandParams"]["transferMode"],
        "ReturnAsStream"
    );
    reply_result(&mut server, &print, json!({"stream":"pdf-stream"})).await;

    let read1 = next_request(&mut server).await;
    assert_eq!(read1["params"]["method"], "IO.read");
    assert_eq!(read1["params"]["commandParams"]["handle"], "pdf-stream");
    reply_result(
        &mut server,
        &read1,
        json!({"data":"ZmFrZQ==","base64Encoded":true,"eof":false}),
    )
    .await;

    let read2 = next_request(&mut server).await;
    assert_eq!(read2["params"]["method"], "IO.read");
    reply_result(
        &mut server,
        &read2,
        json!({"data":"cGRm","base64Encoded":true,"eof":true}),
    )
    .await;

    let close = next_request(&mut server).await;
    assert_eq!(close["params"]["method"], "IO.close");
    assert_eq!(close["params"]["commandParams"]["handle"], "pdf-stream");
    reply_result(&mut server, &close, json!({})).await;

    assert_eq!(pdf_task.await.unwrap().unwrap(), 12);
}
