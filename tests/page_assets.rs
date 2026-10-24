use codex_browser_bridge::page_assets::validate_output_dir;
use tempfile::tempdir;
#[test]
fn output_dir_requires_existing_absolute_real_dir() {
    let d = tempdir().unwrap();
    assert!(validate_output_dir(d.path()).is_ok());
    assert!(validate_output_dir(d.path().join("missing").as_path()).is_err());
    assert!(validate_output_dir(std::path::Path::new("relative")).is_err());
}
#[cfg(unix)]
#[test]
fn output_dir_rejects_symlink() {
    let d = tempdir().unwrap();
    let t = d.path().join("target");
    let l = d.path().join("link");
    std::fs::create_dir(&t).unwrap();
    std::os::unix::fs::symlink(&t, &l).unwrap();
    assert!(validate_output_dir(&l).is_err());
}

use codex_browser_bridge::{
    client::Client,
    page_assets::{AssetKind, BundleOptions, PageAssetsRegistry},
    protocol,
};
use serde_json::{json, Value};
use tokio::{
    io::{duplex, DuplexStream},
    time::{timeout, Duration},
};

async fn next(peer: &mut DuplexStream) -> Value {
    let frame = timeout(Duration::from_secs(2), protocol::decode_frame(peer))
        .await
        .unwrap()
        .unwrap();
    serde_json::from_slice(&frame).unwrap()
}
async fn reply(peer: &mut DuplexStream, r: &Value, result: Value) {
    protocol::encode_frame(peer, &json!({"id":r["id"],"result":result}))
        .await
        .unwrap();
}
fn frame() -> Value {
    json!({"frameTree":{"frame":{"id":"root","loaderId":"document-1","url":"https://example.com/"},"resources":[{"url":"https://example.com/image.png","type":"Image","mimeType":"image/png","contentSize":3}]}})
}

#[tokio::test]
async fn list_and_bundle_use_cached_content_and_keep_bytes_out_of_agent_result() {
    let (stream, mut peer) = duplex(16384);
    let client = Client::from_stream(stream).unwrap();
    client.mark_attached(7).await;
    let registry = PageAssetsRegistry::default();
    let task = tokio::spawn({
        let registry = registry.clone();
        let c = client.clone();
        async move { registry.list(&c, "7").await }
    });
    let r = next(&mut peer).await;
    assert_eq!(r["params"]["method"], "Page.getResourceTree");
    reply(&mut peer, &r, frame()).await;
    let r = next(&mut peer).await;
    assert_eq!(r["params"]["method"], "Runtime.evaluate");
    reply(&mut peer,&r,json!({"result":{"value":{"assets":[],"inlineSvgs":[{"markup":"<svg/>","name":"icon"}],"truncated":false}}})).await;
    let r = next(&mut peer).await;
    assert_eq!(r["params"]["method"], "Page.getFrameTree");
    reply(&mut peer, &r, frame()).await;
    let inventory = timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(!inventory.partial);
    assert_eq!(inventory.assets.len(), 3);
    let text = serde_json::to_string(&inventory).unwrap();
    assert!(!text.contains("<svg"));
    let dir = tempdir().unwrap();
    let task = tokio::spawn({
        let c = client.clone();
        let registry = registry.clone();
        let path = dir.path().to_owned();
        async move {
            registry
                .bundle(
                    &c,
                    "7",
                    BundleOptions {
                        inventory_id: inventory.id,
                        output_dir: path,
                        kinds: Some(vec![AssetKind::Image]),
                        ..Default::default()
                    },
                )
                .await
        }
    });
    let r = next(&mut peer).await;
    assert_eq!(r["params"]["method"], "Page.getFrameTree");
    reply(&mut peer, &r, frame()).await;
    let r = next(&mut peer).await;
    assert_eq!(r["params"]["method"], "Page.getResourceContent");
    assert_eq!(r["params"]["commandParams"]["frameId"], "root");
    reply(
        &mut peer,
        &r,
        json!({"content":"YWJj","base64Encoded":true}),
    )
    .await;
    let r = next(&mut peer).await;
    assert_eq!(r["params"]["method"], "Page.getFrameTree");
    reply(&mut peer, &r, frame()).await;
    let result = timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(result.assets.len(), 2);
    assert_eq!(
        result.path_scope,
        codex_browser_bridge::file_results::PathScope::BridgeHost
    );
    assert!(!result.browser_download);
    assert!(result.manifest_path.is_file());
    assert_eq!(std::fs::read(&result.assets[0].path).unwrap(), b"abc");
    assert!(!serde_json::to_string(&result).unwrap().contains("YWJj"));
}

#[tokio::test]
async fn invalid_directory_and_unknown_inventory_do_not_write_pipe() {
    let (stream, mut peer) = duplex(8192);
    let client = Client::from_stream(stream).unwrap();
    let registry = PageAssetsRegistry::default();
    assert!(registry
        .bundle(
            &client,
            "7",
            BundleOptions {
                output_dir: "relative".into(),
                ..Default::default()
            }
        )
        .await
        .is_err());
    let dir = tempdir().unwrap();
    assert!(registry
        .bundle(
            &client,
            "7",
            BundleOptions {
                output_dir: dir.path().to_owned(),
                inventory_id: "unknown".into(),
                ..Default::default()
            }
        )
        .await
        .is_err());
    assert!(
        timeout(Duration::from_millis(50), protocol::decode_frame(&mut peer))
            .await
            .is_err()
    );
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}

async fn listed(registry: &PageAssetsRegistry, client: &Client, peer: &mut DuplexStream) -> String {
    let task = tokio::spawn({
        let r = registry.clone();
        let c = client.clone();
        async move { r.list(&c, "7").await }
    });
    let r = next(peer).await;
    reply(peer, &r, frame()).await;
    let r = next(peer).await;
    reply(
        peer,
        &r,
        json!({"result":{"value":{"assets":[],"inlineSvgs":[],"truncated":false}}}),
    )
    .await;
    let r = next(peer).await;
    reply(peer, &r, frame()).await;
    timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap()
        .id
}

#[tokio::test]
async fn stale_inventory_fails_before_resource_read_and_creates_no_output() {
    let (stream, mut peer) = duplex(16384);
    let c = Client::from_stream(stream).unwrap();
    c.mark_attached(7).await;
    let registry = PageAssetsRegistry::default();
    let id = listed(&registry, &c, &mut peer).await;
    let dir = tempdir().unwrap();
    let task = tokio::spawn({
        let r = registry.clone();
        let p = dir.path().to_owned();
        async move {
            r.bundle(
                &c,
                "7",
                BundleOptions {
                    inventory_id: id,
                    output_dir: p,
                    ..Default::default()
                },
            )
            .await
        }
    });
    let request = next(&mut peer).await;
    assert_eq!(request["params"]["method"], "Page.getFrameTree");
    let mut changed = frame();
    changed["frameTree"]["frame"]["loaderId"] = json!("document-2");
    reply(&mut peer, &request, changed).await;
    assert!(timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap()
        .is_err());
    assert!(!matches!(
        timeout(Duration::from_millis(50), protocol::decode_frame(&mut peer)).await,
        Ok(Ok(_))
    ));
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[tokio::test]
async fn oversized_cached_content_leaves_no_partial_output() {
    let (stream, mut peer) = duplex(16384);
    let c = Client::from_stream(stream).unwrap();
    c.mark_attached(7).await;
    let registry = PageAssetsRegistry::default();
    let id = listed(&registry, &c, &mut peer).await;
    let dir = tempdir().unwrap();
    let task = tokio::spawn({
        let r = registry.clone();
        let p = dir.path().to_owned();
        async move {
            r.bundle(
                &c,
                "7",
                BundleOptions {
                    inventory_id: id,
                    output_dir: p,
                    max_resource_bytes: 4,
                    max_total_bytes: 4,
                    ..Default::default()
                },
            )
            .await
        }
    });
    let request = next(&mut peer).await;
    reply(&mut peer, &request, frame()).await;
    let request = next(&mut peer).await;
    assert_eq!(request["params"]["method"], "Page.getResourceContent");
    reply(
        &mut peer,
        &request,
        json!({"content":"too-long","base64Encoded":false}),
    )
    .await;
    assert!(timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap()
        .is_err());
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[cfg(unix)]
#[test]
fn output_directory_rejects_symlink_ancestor() {
    let d = tempdir().unwrap();
    let target = d.path().join("target");
    std::fs::create_dir(&target).unwrap();
    std::fs::create_dir(target.join("child")).unwrap();
    let link = d.path().join("link");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    assert!(validate_output_dir(&link.join("child")).is_err());
}
