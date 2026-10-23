#![cfg(unix)]

use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::Path;

use codex_browser_bridge::{client::Client, pipe::dial_transport, protocol};
use serde_json::{json, Value};
use tokio::net::{UnixListener, UnixStream};
use tokio::time::{timeout, Duration};

const WAIT: Duration = Duration::from_secs(3);

fn private_directory() -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    directory
}

fn listener(path: &Path) -> UnixListener {
    let listener = UnixListener::bind(path).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
    listener
}

async fn request(stream: &mut UnixStream) -> Value {
    let frame = timeout(WAIT, protocol::decode_frame(stream))
        .await
        .unwrap()
        .unwrap();
    serde_json::from_slice(&frame).unwrap()
}

async fn reply(stream: &mut UnixStream, request: &Value, result: Value) {
    timeout(
        WAIT,
        protocol::encode_frame(stream, &json!({"id": request["id"], "result": result})),
    )
    .await
    .unwrap()
    .unwrap();
}

fn identity() -> Value {
    json!({"type":"extension","family":"chrome","metadata":{"extensionInstanceId":"test-instance"}})
}

#[tokio::test]
async fn dropping_an_unbound_connected_candidate_closes_the_unix_socket() {
    use tokio::io::AsyncReadExt;
    let root = private_directory();
    let path = root.path().join("browser.sock");
    let listener = listener(&path);
    let server = tokio::spawn(async move {
        let (mut stream, _) = timeout(WAIT, listener.accept()).await.unwrap().unwrap();
        let health = request(&mut stream).await;
        reply(&mut stream, &health, identity()).await;
        let mut byte = [0u8; 1];
        assert_eq!(
            timeout(WAIT, stream.read(&mut byte))
                .await
                .unwrap()
                .unwrap(),
            0
        );
    });
    let candidate = timeout(WAIT, Client::connect(Some(path.to_str().unwrap())))
        .await
        .unwrap()
        .unwrap();
    // Simulate a rejected rebind before the candidate has been installed.
    drop(candidate);
    timeout(WAIT, server).await.unwrap().unwrap();
}

#[tokio::test]
async fn unix_connection_reuses_protocol_and_reconnects_without_replaying_mutations() {
    let root = private_directory();
    let path = root.path().join("browser.sock");
    let listener = listener(&path);
    let server = tokio::spawn(async move {
        let (mut first, _) = timeout(WAIT, listener.accept()).await.unwrap().unwrap();
        let health = request(&mut first).await;
        assert_eq!(health["method"], "getInfo");
        // Events may arrive before the matching getInfo response.
        protocol::encode_frame(&mut first, &json!({"method":"event","params":{}}))
            .await
            .unwrap();
        reply(&mut first, &health, identity()).await;
        let mutation = request(&mut first).await;
        assert_eq!(mutation["method"], "createTab");
        drop(first); // Ambiguous outcome must not replay this request.

        let (mut second, _) = timeout(WAIT, listener.accept()).await.unwrap().unwrap();
        let health = request(&mut second).await;
        assert_eq!(health["method"], "getInfo");
        reply(&mut second, &health, identity()).await;
        let tabs = request(&mut second).await;
        assert_eq!(tabs["method"], "getTabs");
        assert_eq!(tabs["params"]["session_id"], health["params"]["session_id"]);
        reply(&mut second, &tabs, json!([])).await;
    });
    let client = timeout(WAIT, Client::connect(Some(path.to_str().unwrap())))
        .await
        .unwrap()
        .unwrap();
    let err = timeout(WAIT, client.send_request("createTab", None))
        .await
        .unwrap()
        .unwrap_err();
    assert!(err.to_string().contains("not replayed"));
    let tabs = timeout(WAIT, client.send_request("getTabs", None))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(tabs.get(), "[]");
    client.shutdown().await;
    timeout(WAIT, server).await.unwrap().unwrap();
}

#[tokio::test]
async fn unix_handshake_rejects_non_extension_backends() {
    for info in [
        json!({}),
        json!({"type":"host","family":"chrome"}),
        json!({"type":"extension","family":""}),
    ] {
        let root = private_directory();
        let path = root.path().join("browser.sock");
        let listener = listener(&path);
        let server = tokio::spawn(async move {
            let (mut stream, _) = timeout(WAIT, listener.accept()).await.unwrap().unwrap();
            let health = request(&mut stream).await;
            reply(&mut stream, &health, info).await;
        });
        let result = timeout(WAIT, Client::connect(Some(path.to_str().unwrap())))
            .await
            .unwrap();
        assert!(result
            .err()
            .unwrap()
            .to_string()
            .contains("did not identify a browser extension"));
        timeout(WAIT, server).await.unwrap().unwrap();
    }
}

#[tokio::test]
async fn unix_dial_rejects_symlinks_regular_files_and_exposed_permissions() {
    let root = private_directory();
    let path = root.path().join("browser.sock");
    let _listener = listener(&path);
    let alias = root.path().join("alias.sock");
    symlink(&path, &alias).unwrap();
    assert!(dial_transport(alias.to_str().unwrap()).await.is_err());
    let regular = root.path().join("file.sock");
    std::fs::write(&regular, b"not a socket").unwrap();
    assert!(dial_transport(regular.to_str().unwrap()).await.is_err());
    for mode in [0o620, 0o640, 0o601, 0o666] {
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
        assert!(
            dial_transport(path.to_str().unwrap()).await.is_err(),
            "mode {mode:o}"
        );
    }
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o777)).unwrap();
    assert!(dial_transport(path.to_str().unwrap()).await.is_err());
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o1777)).unwrap();
    assert!(dial_transport(path.to_str().unwrap()).await.is_ok());
}
