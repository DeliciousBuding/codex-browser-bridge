#![allow(dead_code)]

#[path = "../src/client.rs"]
mod client;
#[path = "../src/discovery.rs"]
mod discovery;
#[path = "../src/error.rs"]
mod error;
#[path = "../src/pipe.rs"]
mod pipe;
#[path = "../src/protocol.rs"]
mod protocol;

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use client::Client;
use serde_json::{json, Value};
use tokio::io::{AsyncRead, AsyncWrite};

#[cfg(windows)]
use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeServer, ServerOptions};

#[cfg(not(windows))]
use tokio::io::DuplexStream;

#[cfg(windows)]
type ServerStream = NamedPipeServer;

#[cfg(not(windows))]
type ServerStream = DuplexStream;

#[cfg(windows)]
async fn client_server_pair() -> (Client, ServerStream) {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = format!(r"\\.\pipe\codex-browser-bridge-test-{suffix}");
    let server = ServerOptions::new().create(&path).unwrap();
    let client_stream = ClientOptions::new().open(&path).unwrap();
    server.connect().await.unwrap();
    (Client::from_stream(client_stream).unwrap(), server)
}

#[cfg(not(windows))]
async fn client_server_pair() -> (Client, ServerStream) {
    let (client_stream, server) = tokio::io::duplex(4096);
    (Client::from_stream(client_stream).unwrap(), server)
}

async fn read_request<S>(server: &mut S) -> Value
where
    S: AsyncRead + Unpin,
{
    let frame = protocol::decode_frame(server).await.unwrap();
    serde_json::from_slice(&frame).unwrap()
}

async fn write_result<S>(server: &mut S, id: u64, result: Value)
where
    S: AsyncWrite + Unpin,
{
    protocol::encode_frame(
        server,
        &json!({"jsonrpc": "2.0", "id": id, "result": result}),
    )
    .await
    .unwrap();
}

async fn write_error<S>(server: &mut S, id: u64, code: i64, message: &str)
where
    S: AsyncWrite + Unpin,
{
    protocol::encode_frame(
        server,
        &json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": code, "message": message }
        }),
    )
    .await
    .unwrap();
}

async fn write_response_without_result<S>(server: &mut S, id: u64)
where
    S: AsyncWrite + Unpin,
{
    protocol::encode_frame(server, &json!({"jsonrpc": "2.0", "id": id}))
        .await
        .unwrap();
}

#[tokio::test]
async fn request_timeout_removes_pending_entry() {
    let (client, mut server) = client_server_pair().await;
    let pending_client = client.clone();
    let request = tokio::spawn(async move {
        pending_client
            .send_request_with_timeout("slowMethod", None, Duration::from_millis(25))
            .await
    });

    let req = read_request(&mut server).await;
    assert_eq!(req["method"], "slowMethod");
    assert_eq!(client.pending_len_for_test().await, 1);

    let err = request.await.unwrap().unwrap_err();
    assert!(err
        .to_string()
        .contains("timeout waiting for slowMethod response"));
    assert_eq!(client.pending_len_for_test().await, 0);
}

#[tokio::test]
async fn response_without_result_is_accepted_like_go_raw_message_zero_value() {
    let (client, mut server) = client_server_pair().await;
    let request_client = client.clone();
    let request = tokio::spawn(async move {
        request_client
            .send_request_with_timeout(
                "finalizeTabs",
                Some(json!({"keep": []})),
                Duration::from_secs(1),
            )
            .await
    });

    let req = read_request(&mut server).await;
    assert_eq!(req["method"], "finalizeTabs");
    write_response_without_result(&mut server, req["id"].as_u64().unwrap()).await;

    let raw = request.await.unwrap().unwrap();
    assert_eq!(raw.get(), "null");
}

#[tokio::test]
async fn concurrent_requests_are_framed_and_matched_by_id() {
    let (client, mut server) = client_server_pair().await;
    let first_client = client.clone();
    let second_client = client.clone();

    let first = tokio::spawn(async move {
        first_client
            .send_request_with_timeout("first", Some(json!({"n": 1})), Duration::from_secs(1))
            .await
    });
    let second = tokio::spawn(async move {
        second_client
            .send_request_with_timeout("second", Some(json!({"n": 2})), Duration::from_secs(1))
            .await
    });

    let req_a = read_request(&mut server).await;
    let req_b = read_request(&mut server).await;
    let id_a = req_a["id"].as_u64().unwrap();
    let id_b = req_b["id"].as_u64().unwrap();

    assert_ne!(id_a, id_b);
    assert!(req_a["params"]["session_id"].is_string());
    assert!(req_a["params"]["turn_id"].is_string());
    assert!(req_b["params"]["session_id"].is_string());
    assert!(req_b["params"]["turn_id"].is_string());

    write_result(&mut server, id_b, json!({"ok": "b"})).await;
    write_result(&mut server, id_a, json!({"ok": "a"})).await;

    let first_result = first.await.unwrap().unwrap();
    let second_result = second.await.unwrap().unwrap();
    assert_eq!(first_result.get(), r#"{"ok":"a"}"#);
    assert_eq!(second_result.get(), r#"{"ok":"b"}"#);
}

#[tokio::test]
async fn execute_cdp_detaches_attaches_and_retries_once_when_debugger_is_not_attached() {
    let (client, mut server) = client_server_pair().await;
    let cdp_client = client.clone();
    let call = tokio::spawn(async move {
        cdp_client
            .execute_cdp(42, "Runtime.evaluate", Some(json!({"expression": "1 + 1"})))
            .await
    });

    let expected = [
        "detach",
        "attach",
        "executeCdp",
        "detach",
        "attach",
        "executeCdp",
    ];

    for (index, method) in expected.iter().enumerate() {
        let req = read_request(&mut server).await;
        assert_eq!(req["method"], *method);
        let id = req["id"].as_u64().unwrap();
        match *method {
            "executeCdp" if index == 2 => {
                assert_eq!(req["params"]["target"]["tabId"], 42);
                assert_eq!(req["params"]["method"], "Runtime.evaluate");
                assert_eq!(req["params"]["commandParams"]["expression"], "1 + 1");
                write_error(
                    &mut server,
                    id,
                    -32000,
                    "Debugger is not attached to the tab",
                )
                .await;
            }
            "executeCdp" => {
                assert_eq!(req["params"]["target"]["tabId"], 42);
                write_result(&mut server, id, json!({"result": {"value": 2}})).await;
            }
            _ => {
                assert_eq!(req["params"]["tabId"], 42);
                write_result(&mut server, id, json!({})).await;
            }
        }
    }

    let raw = call.await.unwrap().unwrap();
    assert_eq!(raw.get(), r#"{"result":{"value":2}}"#);
}
