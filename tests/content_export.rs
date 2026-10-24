use std::fs;

use codex_browser_bridge::export::validate_export_directory;
use codex_browser_bridge::{client::Client, export::export, protocol};
use serde_json::{json, Value};
use tempfile::tempdir;
use tokio::{
    io::{duplex, DuplexStream},
    time::{timeout, Duration},
};

async fn request(peer: &mut DuplexStream) -> Value {
    serde_json::from_slice(
        &timeout(Duration::from_secs(2), protocol::decode_frame(peer))
            .await
            .unwrap()
            .unwrap(),
    )
    .unwrap()
}
async fn respond(peer: &mut DuplexStream, request: &Value, result: Value) {
    protocol::encode_frame(peer, &json!({"id":request["id"],"result":result}))
        .await
        .unwrap();
}
async fn authorize(peer: &mut DuplexStream) {
    let r = request(peer).await;
    assert_eq!(r["method"], "getTabs");
    respond(
        peer,
        &r,
        json!([{"id":7,"url":"https://docs.google.com/document/d/example/edit"}]),
    )
    .await;
    let r = request(peer).await;
    assert_eq!(r["params"]["method"], "Runtime.evaluate");
    respond(peer, &r, json!({"result":{"value":json!({"url":"https://docs.google.com/document/d/example/edit","title":"Example"}).to_string()}})).await;
}

#[tokio::test]
async fn export_fetch_is_page_credentialed_bounded_and_result_is_bridge_host_not_download() {
    let (stream, mut peer) = duplex(16384);
    let client = Client::from_stream(stream).unwrap();
    client.mark_attached(7).await;
    let dir = tempdir().unwrap();
    let path = dir.path().to_owned();
    let task =
        tokio::spawn(async move { export(&client, "7", "gsuite", Some("pdf"), &path).await });
    authorize(&mut peer).await;
    let r = request(&mut peer).await;
    let script = r["params"]["commandParams"]["expression"].as_str().unwrap();
    assert!(script.contains("credentials:'include'"));
    assert!(script.contains("AbortController"));
    assert!(script.contains("reader.cancel()"));
    assert!(script.contains("length>10485760"));
    respond(
        &mut peer,
        &r,
        json!({"result":{"value":"{\"base64\":\"YWJj\"}"}}),
    )
    .await;
    let result = timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(result["path_scope"], "bridge_host");
    assert_eq!(result["browser_download"], false);
    assert_eq!(result["bytes"], 3);
    assert_eq!(fs::read(result["path"].as_str().unwrap()).unwrap(), b"abc");
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[tokio::test]
async fn export_bad_base64_and_page_error_never_publish_files() {
    for response in [json!({"base64":"invalid@@"}), json!({"error":"HTTP 403"})] {
        let (stream, mut peer) = duplex(16384);
        let client = Client::from_stream(stream).unwrap();
        client.mark_attached(7).await;
        let dir = tempdir().unwrap();
        let path = dir.path().to_owned();
        let task =
            tokio::spawn(async move { export(&client, "7", "gsuite", Some("pdf"), &path).await });
        authorize(&mut peer).await;
        let r = request(&mut peer).await;
        respond(
            &mut peer,
            &r,
            json!({"result":{"value":response.to_string()}}),
        )
        .await;
        assert!(timeout(Duration::from_secs(2), task)
            .await
            .unwrap()
            .unwrap()
            .is_err());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
    }
}

#[tokio::test]
async fn invalid_export_output_root_fails_before_any_browser_request() {
    let (stream, mut peer) = duplex(8192);
    let client = Client::from_stream(stream).unwrap();
    assert!(export(
        &client,
        "7",
        "gsuite",
        Some("pdf"),
        std::path::Path::new("relative")
    )
    .await
    .is_err());
    assert!(
        timeout(Duration::from_millis(50), protocol::decode_frame(&mut peer))
            .await
            .is_err()
    );
}

#[test]
fn export_destination_requires_existing_absolute_directory() {
    let relative = std::path::Path::new("exports");
    assert!(validate_export_directory(relative).is_err());

    let missing = tempdir().unwrap().path().join("does-not-exist");
    assert!(validate_export_directory(&missing).is_err());
}

#[test]
fn export_destination_accepts_real_directory_and_returns_canonical_path() {
    let dir = tempdir().unwrap();
    let path = validate_export_directory(dir.path()).unwrap();
    assert!(path.is_absolute());
    assert!(path.is_dir());
    assert_eq!(fs::canonicalize(dir.path()).unwrap(), path);
}

#[cfg(unix)]
#[test]
fn export_destination_rejects_symlink() {
    let dir = tempdir().unwrap();
    let target = dir.path().join("target");
    let link = dir.path().join("link");
    fs::create_dir(&target).unwrap();
    std::os::unix::fs::symlink(&target, &link).unwrap();
    assert!(validate_export_directory(&link).is_err());
}

#[cfg(unix)]
#[test]
fn export_destination_rejects_symlinked_ancestor() {
    let dir = tempdir().unwrap();
    let target = dir.path().join("target");
    let link = dir.path().join("link");
    fs::create_dir(&target).unwrap();
    fs::create_dir(target.join("exports")).unwrap();
    std::os::unix::fs::symlink(&target, &link).unwrap();
    assert!(validate_export_directory(&link.join("exports")).is_err());
}

#[cfg(windows)]
#[test]
fn export_destination_rejects_symlink() {
    // Creating symlinks may require a developer-mode privilege on Windows;
    // the implementation still rejects them when supplied by the caller.
    let dir = tempdir().unwrap();
    let target = dir.path().join("target");
    let link = dir.path().join("link");
    fs::create_dir(&target).unwrap();
    if std::os::windows::fs::symlink_dir(&target, &link).is_ok() {
        assert!(validate_export_directory(&link).is_err());
    }
}

#[cfg(windows)]
#[test]
fn export_destination_rejects_symlinked_ancestor() {
    let dir = tempdir().unwrap();
    let target = dir.path().join("target");
    let link = dir.path().join("link");
    fs::create_dir(&target).unwrap();
    fs::create_dir(target.join("exports")).unwrap();
    if std::os::windows::fs::symlink_dir(&target, &link).is_ok() {
        assert!(validate_export_directory(&link.join("exports")).is_err());
    }
}
