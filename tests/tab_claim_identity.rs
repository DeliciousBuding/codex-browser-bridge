use codex_browser_bridge::{browser::claim_user_tab, client::Client, mcp::Server, protocol};
use serde_json::{json, Value};
use tokio::{
    io::{duplex, DuplexStream},
    time::{timeout, Duration},
};

const BOUND: Duration = Duration::from_secs(2);
const NO_WRITE_GRACE: Duration = Duration::from_millis(40);

fn setup() -> (Client, DuplexStream) {
    let (stream, peer) = duplex(16 * 1024);
    (Client::from_stream(stream).unwrap(), peer)
}

async fn request(peer: &mut DuplexStream) -> Value {
    serde_json::from_slice(
        &timeout(BOUND, protocol::decode_frame(peer))
            .await
            .expect("mock request deadline")
            .unwrap(),
    )
    .unwrap()
}

async fn respond(peer: &mut DuplexStream, request: &Value, result: Value) {
    timeout(
        BOUND,
        protocol::encode_frame(peer, &json!({"id": request["id"], "result": result})),
    )
    .await
    .expect("mock response deadline")
    .unwrap();
}

async fn no_request(peer: &mut DuplexStream) {
    assert!(timeout(NO_WRITE_GRACE, protocol::decode_frame(peer))
        .await
        .is_err());
}

async fn enable_focus(peer: &mut DuplexStream) {
    let focus = request(peer).await;
    assert_eq!(focus["method"], "executeCdp");
    assert_eq!(focus["params"]["target"]["tabId"], 7);
    assert_eq!(
        focus["params"]["method"],
        "Emulation.setFocusEmulationEnabled"
    );
    assert_eq!(focus["params"]["commandParams"], json!({"enabled": true}));
    respond(peer, &focus, json!({})).await;
}

#[tokio::test]
async fn stale_missing_reused_and_incomplete_snapshots_never_claim() {
    for tabs in [
        json!([]),
        json!([{"id": 8, "title": "Observed", "url": "https://example.com/seen"}]),
        json!([{"id": 7, "title": "Changed", "url": "https://example.com/seen"}]),
        json!([{"id": 7, "title": "Observed", "url": "https://example.com/reused"}]),
        json!([{"id": 7, "title": "Observed", "url": "https://example.com/seen#changed"}]),
        json!([{"id": 7, "url": "https://example.com/seen"}]),
        json!([{"id": 7, "title": "Observed"}]),
        json!([
            {"id": 7, "title": "Observed", "url": "https://example.com/seen"},
            {"id": "7", "title": "Observed", "url": "https://example.com/seen"}
        ]),
    ] {
        let (client, mut peer) = setup();
        let worker = client.clone();
        let task = tokio::spawn(async move {
            claim_user_tab(&worker, "7", "Observed", "https://example.com/seen").await
        });
        let r = request(&mut peer).await;
        assert_eq!(r["method"], "getUserTabs");
        respond(&mut peer, &r, tabs).await;
        let error = timeout(BOUND, task).await.unwrap().unwrap().unwrap_err();
        assert!(
            error.to_string().contains("snapshot") || error.to_string().contains("duplicate"),
            "{error}"
        );
        no_request(&mut peer).await;
        client.shutdown().await;
    }
}

#[tokio::test]
async fn matching_bare_and_wrapped_snapshots_claim_and_attach_exact_id() {
    for (tabs, title, url) in [
        (
            json!([{"id": 7, "title": "Observed", "url": "https://example.com/seen"}]),
            "Observed",
            "https://example.com/seen",
        ),
        (
            json!({"tabs": [{"id": "7", "title": "", "url": ""}]}),
            "",
            "",
        ),
    ] {
        let (client, mut peer) = setup();
        let worker = client.clone();
        let task = tokio::spawn(async move { claim_user_tab(&worker, "7", title, url).await });
        let r = request(&mut peer).await;
        assert_eq!(r["method"], "getUserTabs");
        respond(&mut peer, &r, tabs).await;
        let r = request(&mut peer).await;
        assert_eq!(r["method"], "claimUserTab");
        assert_eq!(r["params"]["tabId"], 7);
        // Do not send unsupported compare-and-swap fields to the extension.
        assert!(r["params"].get("expected_title").is_none());
        assert!(r["params"].get("expected_url").is_none());
        respond(
            &mut peer,
            &r,
            json!({"id": "7", "title": title, "url": url}),
        )
        .await;
        let r = request(&mut peer).await;
        assert_eq!(r["method"], "attach");
        assert_eq!(r["params"]["tabId"], 7);
        // Extension 1.26+ reads params.networkPolicy.timeoutMs on every attach.
        assert_eq!(r["params"]["networkPolicy"], json!({"constraints": []}));
        respond(&mut peer, &r, Value::Null).await;
        enable_focus(&mut peer).await;
        let tab = timeout(BOUND, task).await.unwrap().unwrap().unwrap();
        assert_eq!(tab.id, "7");
        assert_eq!(tab.title, title);
        assert_eq!(tab.url, url);
        no_request(&mut peer).await;
        client.shutdown().await;
    }
}

#[tokio::test]
async fn changed_or_malformed_claim_response_reports_uncertain_ownership_without_attach() {
    for claimed in [
        json!({"id": 8, "title": "Observed", "url": "https://example.com/seen"}),
        json!({"id": 7, "title": "Changed", "url": "https://example.com/seen"}),
        json!({"id": 7, "title": "Observed", "url": "https://example.com/reused"}),
        json!({"id": 7, "title": "Observed"}),
        Value::Null,
    ] {
        let (client, mut peer) = setup();
        let worker = client.clone();
        let task = tokio::spawn(async move {
            claim_user_tab(&worker, "7", "Observed", "https://example.com/seen").await
        });
        let r = request(&mut peer).await;
        respond(
            &mut peer,
            &r,
            json!([{"id": 7, "title": "Observed", "url": "https://example.com/seen"}]),
        )
        .await;
        let r = request(&mut peer).await;
        assert_eq!(r["method"], "claimUserTab");
        respond(&mut peer, &r, claimed).await;
        let error = timeout(BOUND, task).await.unwrap().unwrap().unwrap_err();
        assert!(
            error.to_string().contains("ownership may have changed"),
            "{error}"
        );
        no_request(&mut peer).await;
        client.shutdown().await;
    }
}

#[tokio::test]
async fn invalid_id_and_numeric_only_mcp_calls_fail_before_browser_io() {
    let (client, mut peer) = setup();
    assert!(claim_user_tab(&client, "not-a-number", "", "")
        .await
        .is_err());
    let server = Server::new(client.clone());
    for arguments in [
        json!({"tab_id": "7"}),
        json!({"tab_id": "7", "expected_title": "Observed"}),
        json!({"tab_id": "7", "expected_title": null, "expected_url": ""}),
    ] {
        let response = timeout(
            BOUND,
            server.handle_jsonrpc_line(
                &json!({
                    "jsonrpc": "2.0", "id": 1, "method": "tools/call",
                    "params": {"name": "codex_claim_tab", "arguments": arguments}
                })
                .to_string(),
            ),
        )
        .await
        .unwrap()
        .unwrap();
        let response: Value = serde_json::from_str(&response).unwrap();
        assert_eq!(response["result"]["isError"], true);
        assert!(response["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("numeric-only claims are no longer accepted"));
        no_request(&mut peer).await;
    }
    client.shutdown().await;
}

#[tokio::test]
async fn browser_denial_during_enumeration_or_claim_never_attaches_or_replays() {
    for deny_claim in [false, true] {
        let (client, mut peer) = setup();
        let worker = client.clone();
        let task = tokio::spawn(async move {
            claim_user_tab(&worker, "7", "Observed", "https://example.com/seen").await
        });
        let mut r = request(&mut peer).await;
        assert_eq!(r["method"], "getUserTabs");
        if deny_claim {
            respond(
                &mut peer,
                &r,
                json!([{"id": 7, "title": "Observed", "url": "https://example.com/seen"}]),
            )
            .await;
            r = request(&mut peer).await;
            assert_eq!(r["method"], "claimUserTab");
        }
        timeout(
            BOUND,
            protocol::encode_frame(
                &mut peer,
                &json!({"id": r["id"], "error": {"code": -32000, "message": "Not allowed"}}),
            ),
        )
        .await
        .unwrap()
        .unwrap();
        let error = timeout(BOUND, task).await.unwrap().unwrap().unwrap_err();
        assert!(error.to_string().contains("Not allowed"));
        no_request(&mut peer).await;
        client.shutdown().await;
    }
}

#[tokio::test]
async fn attach_denial_reports_completed_claim_without_replaying() {
    let (client, mut peer) = setup();
    let worker = client.clone();
    let task = tokio::spawn(async move {
        claim_user_tab(&worker, "7", "Observed", "https://example.com/seen").await
    });
    let observed = json!({"id": 7, "title": "Observed", "url": "https://example.com/seen"});
    let r = request(&mut peer).await;
    respond(&mut peer, &r, json!([observed])).await;
    let r = request(&mut peer).await;
    respond(&mut peer, &r, observed).await;
    let r = request(&mut peer).await;
    assert_eq!(r["method"], "attach");
    timeout(
        BOUND,
        protocol::encode_frame(
            &mut peer,
            &json!({"id": r["id"], "error": {"code": -32000, "message": "Not allowed"}}),
        ),
    )
    .await
    .unwrap()
    .unwrap();
    let error = timeout(BOUND, task).await.unwrap().unwrap().unwrap_err();
    assert!(error
        .to_string()
        .contains("tab was claimed but attach failed"));
    no_request(&mut peer).await;
    client.shutdown().await;
}

/// Regression: extension 1.26+ read `params.networkPolicy.timeoutMs` on every
/// attach and threw `Cannot read properties of undefined` when the field was
/// missing, which made every CDP call fail before the page was touched.
#[tokio::test]
async fn attach_always_carries_the_network_policy_field() {
    let (client, mut peer) = setup();
    client.set_network_policy(
        codex_browser_bridge::network_policy::NetworkPolicy::from_json(
            r#"{"timeout_ms":4000,"constraints":[{"kind":"domains","mode":"denylist","patterns":["tracker.example"]}]}"#,
        )
        .unwrap(),
    );
    let worker = client.clone();
    let task =
        tokio::spawn(async move { codex_browser_bridge::browser::get_title(&worker, "7").await });
    let detach = request(&mut peer).await;
    assert_eq!(detach["method"], "detach");
    respond(&mut peer, &detach, json!({})).await;
    let attach = request(&mut peer).await;
    assert_eq!(attach["method"], "attach");
    assert_eq!(attach["params"]["tabId"], 7);
    assert_eq!(attach["params"]["networkPolicy"]["timeoutMs"], 4000);
    assert_eq!(
        attach["params"]["networkPolicy"]["constraints"],
        json!([{"kind":"domains","mode":"denylist","patterns":["tracker.example"]}])
    );
    respond(&mut peer, &attach, Value::Null).await;
    enable_focus(&mut peer).await;
    let evaluate = request(&mut peer).await;
    assert_eq!(evaluate["method"], "executeCdp");
    respond(
        &mut peer,
        &evaluate,
        json!({"result": {"result": {"type": "string", "value": "ok"}}}),
    )
    .await;
    timeout(BOUND, task).await.unwrap().unwrap().unwrap();
    client.shutdown().await;
}
