#![allow(dead_code)]

#[path = "../src/browser.rs"]
mod browser;
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

use client::Client;
use serde_json::{json, Value};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncRead, AsyncWrite};

#[cfg(windows)]
use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeServer, ServerOptions};

#[cfg(windows)]
type ServerStream = NamedPipeServer;

#[cfg(windows)]
async fn client_server_pair() -> (Client, ServerStream) {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = format!(r"\\.\pipe\codex-bridge-e2e-{suffix}");
    let server = ServerOptions::new().create(&path).unwrap();
    let client_stream = ClientOptions::new().open(&path).unwrap();
    server.connect().await.unwrap();
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

/// Mock server that answers `attach` and `executeCdp` requests for e2e tests
async fn mock_cdp_server(mut server: ServerStream, cdp_result: Value) {
    loop {
        let req = read_request(&mut server).await;
        let id = req["id"].as_u64().unwrap();
        let method = req["method"].as_str().unwrap_or("");

        match method {
            "attach" | "detach" => {
                write_result(&mut server, id, json!({})).await;
            }
            "executeCdp" => {
                write_result(&mut server, id, cdp_result.clone()).await;
            }
            _ => {
                protocol::encode_frame(
                    &mut server,
                    &json!({"jsonrpc": "2.0", "id": id, "error": {"code": -1, "message": format!("unknown: {method}")}}),
                )
                .await
                .unwrap();
            }
        }
    }
}

#[tokio::test]
async fn e2e_execute_cdp_generic_roundtrip() {
    let (client, server) = client_server_pair().await;
    let cdp_response = json!({"result": {"value": 42}});
    tokio::spawn(mock_cdp_server(server, cdp_response));

    let raw = browser::execute_cdp_generic(
        &client,
        "1",
        "Runtime.evaluate",
        Some(json!({"expression": "40+2", "returnByValue": true})),
    )
    .await
    .unwrap();

    assert!(raw.get().contains("42"), "unexpected result: {}", raw.get());
}

#[tokio::test]
async fn e2e_page_assets_resource_tree_roundtrip() {
    let (client, server) = client_server_pair().await;
    let tree_json = json!({
        "frame": {"id": "F1", "url": "https://example.com"},
        "resources": [
            {"url": "https://example.com/logo.png", "type": "Image", "mimeType": "image/png", "contentSize": 4096.0},
            {"url": "https://example.com/app.js", "type": "Script", "mimeType": "application/javascript"}
        ],
        "childFrames": []
    });
    tokio::spawn(mock_cdp_server(server, tree_json));

    let resources = browser::get_resource_tree(&client, "1").await.unwrap();
    assert_eq!(resources.len(), 2);
    assert_eq!(resources[0].url, "https://example.com/logo.png");
    assert_eq!(resources[0].resource_type, "Image");
    assert_eq!(resources[1].resource_type, "Script");
}

#[tokio::test]
async fn e2e_network_get_cookies_roundtrip() {
    let (client, server) = client_server_pair().await;
    let cookies_json = json!({
        "cookies": [
            {"name": "session", "value": "abc123", "domain": ".example.com", "path": "/",
             "httpOnly": true, "secure": true, "sameSite": "Lax"},
            {"name": "tracking", "value": "opt-out", "domain": "example.com", "path": "/",
             "httpOnly": false, "secure": false}
        ]
    });
    tokio::spawn(mock_cdp_server(server, cookies_json));

    let cookies = browser::get_cookies(&client, "1", None).await.unwrap();
    assert_eq!(cookies.len(), 2);
    assert_eq!(cookies[0].name, "session");
    assert_eq!(cookies[0].http_only, true);
    assert_eq!(cookies[1].name, "tracking");
    assert_eq!(cookies[1].secure, false);
}

#[tokio::test]
async fn e2e_network_set_cookie_roundtrip() {
    let (client, server) = client_server_pair().await;
    // setCookie just needs an empty success response
    tokio::spawn(mock_cdp_server(server, json!({})));

    browser::set_cookie(
        &client,
        "1",
        json!({"name": "theme", "value": "dark", "domain": "example.com"}),
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn e2e_cdp_error_propagates_to_caller() {
    let (client, server) = client_server_pair().await;
    // Return CDP-level error: executeCdp succeeds at RPC level but result contains error
    let cdp_error = json!({"error": "Target closed"});
    tokio::spawn(async move {
        let mut server = server;
        loop {
            let req = read_request(&mut server).await;
            let id = req["id"].as_u64().unwrap();
            match req["method"].as_str().unwrap_or("") {
                "attach" | "detach" => {
                    write_result(&mut server, id, json!({})).await;
                }
                "executeCdp" => {
                    write_result(&mut server, id, cdp_error.clone()).await;
                }
                _ => {
                    protocol::encode_frame(
                        &mut server,
                        &json!({"jsonrpc": "2.0", "id": id, "error": {"code": -1, "message": "unknown"}}),
                    )
                    .await
                    .unwrap();
                }
            }
        }
    });

    let result = browser::execute_cdp_generic(
        &client,
        "1",
        "Page.navigate",
        Some(json!({"url": "about:blank"})),
    )
    .await;

    // CDP-level errors are returned in the Ok variant (RPC succeeded, CDP returned error content)
    // The raw value contains the CDP error response
    assert!(result.is_ok(), "executeCdp RPC should succeed; CDP error is in the payload");
    let raw = result.unwrap();
    assert!(raw.get().contains("error"), "payload should contain CDP error: {}", raw.get());
}

#[tokio::test]
async fn e2e_blocked_cdp_method_returns_error() {
    let (client, _server) = client_server_pair().await;

    let result = browser::execute_cdp_generic(
        &client,
        "1",
        "Debugger.enable",
        None,
    )
    .await;

    assert!(result.is_err(), "blocked CDP method should return Err");
    let err = result.unwrap_err().to_string();
    assert!(err.contains("blocked"), "error should mention blocked: {err}");
}

#[tokio::test]
async fn e2e_resource_tree_parsing_fails_on_malformed_response() {
    let (client, server) = client_server_pair().await;
    // Missing required "frame" field
    let bad_tree = json!({"notA": "tree"});
    tokio::spawn(mock_cdp_server(server, bad_tree));

    let result = browser::get_resource_tree(&client, "1").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn e2e_cookies_parsing_fails_on_malformed_response() {
    let (client, server) = client_server_pair().await;
    // Missing "cookies" array
    let bad_cookies = json!({"not": "cookies"});
    tokio::spawn(mock_cdp_server(server, bad_cookies));

    let result = browser::get_cookies(&client, "1", None).await;
    assert!(result.is_err());
}
