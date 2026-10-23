use codex_browser_bridge::protocol::{decode_frame, encode_frame};
use codex_browser_bridge::{client::Client, error::BridgeError, webmcp::WebMcpRegistry};
use serde_json::json;
use serde_json::Value;
use tokio::{
    io::DuplexStream,
    time::{timeout, Duration},
};

fn frame(loader: &str) -> Value {
    json!({"frameTree":{"frame":{"id":"f","loaderId":loader,"url":"https://example.com/"}}})
}

async fn reply(stream: &mut DuplexStream, method: &str, result: Value) -> Value {
    let bytes = timeout(Duration::from_secs(2), decode_frame(stream))
        .await
        .unwrap()
        .unwrap();
    let request: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(request["method"], "executeCdp");
    assert_eq!(request["params"]["target"], json!({"tabId":7}));
    assert_eq!(request["params"]["method"], method);
    timeout(
        Duration::from_secs(2),
        encode_frame(stream, &json!({"id":request["id"],"result":result})),
    )
    .await
    .unwrap()
    .unwrap();
    request
}

async fn list_reply(stream: &mut DuplexStream) -> String {
    reply(stream, "Page.getFrameTree", frame("loader-1")).await;
    let request = reply(stream, "Runtime.evaluate", json!({"result":{"value":{"tools":[{"handle":"h","name":"act","inputSchema":{"type":"object"}}]}}})).await;
    reply(stream, "Page.getFrameTree", frame("loader-1")).await;
    request["params"]["commandParams"]["expression"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[tokio::test]
async fn document_epoch_change_rejects_handle_without_invoking() {
    let (local, mut remote) = tokio::io::duplex(256 * 1024);
    let client = Client::from_stream(local).unwrap();
    client.mark_attached(7).await;
    let server = tokio::spawn(async move {
        list_reply(&mut remote).await;
        reply(&mut remote, "Page.getFrameTree", frame("loader-2")).await;
        assert!(
            timeout(Duration::from_millis(100), decode_frame(&mut remote))
                .await
                .is_err()
        );
    });
    let registry = WebMcpRegistry::default();
    registry.list(&client, "7", 1000).await.unwrap();
    let error = registry
        .invoke(&client, "h", json!({}), 1000)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("stale or unknown"));
    assert!(registry
        .invoke(&client, "h", json!({}), 1000)
        .await
        .unwrap_err()
        .to_string()
        .contains("stale or unknown"));
    timeout(Duration::from_secs(2), server)
        .await
        .unwrap()
        .unwrap();
    client.shutdown().await;
}

#[tokio::test]
async fn ambiguous_mutation_is_not_replayed_and_handle_is_retired() {
    let (local, mut remote) = tokio::io::duplex(256 * 1024);
    let client = Client::from_stream(local).unwrap();
    client.mark_attached(7).await;
    let server = tokio::spawn(async move {
        list_reply(&mut remote).await;
        reply(&mut remote, "Page.getFrameTree", frame("loader-1")).await;
        reply(
            &mut remote,
            "Runtime.evaluate",
            json!({"error":{"code":-32000,"message":"not attached: execution outcome unknown"}}),
        )
        .await;
        assert!(
            timeout(Duration::from_millis(100), decode_frame(&mut remote))
                .await
                .is_err()
        );
    });
    let registry = WebMcpRegistry::default();
    registry.list(&client, "7", 1000).await.unwrap();
    assert!(matches!(
        registry
            .invoke(&client, "h", json!({}), 1000)
            .await
            .unwrap_err(),
        BridgeError::Cdp { .. }
    ));
    assert!(registry
        .invoke(&client, "h", json!({}), 1000)
        .await
        .unwrap_err()
        .to_string()
        .contains("stale or unknown"));
    timeout(Duration::from_secs(2), server)
        .await
        .unwrap()
        .unwrap();
    client.shutdown().await;
}

#[tokio::test]
async fn invocation_timeout_retires_handle_without_silent_replay() {
    let (local, mut remote) = tokio::io::duplex(256 * 1024);
    let client = Client::from_stream(local).unwrap();
    client.mark_attached(7).await;
    let server = tokio::spawn(async move {
        list_reply(&mut remote).await;
        reply(&mut remote, "Page.getFrameTree", frame("loader-1")).await;
        let bytes = timeout(Duration::from_secs(2), decode_frame(&mut remote))
            .await
            .unwrap()
            .unwrap();
        let request: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(request["params"]["method"], "Runtime.evaluate");
        // The browser may have executed the request. Deliberately never reply.
        assert!(
            timeout(Duration::from_millis(150), decode_frame(&mut remote))
                .await
                .is_err()
        );
    });
    let registry = WebMcpRegistry::default();
    registry.list(&client, "7", 1000).await.unwrap();
    assert!(matches!(
        registry
            .invoke(&client, "h", json!({}), 50)
            .await
            .unwrap_err(),
        BridgeError::Timeout(_)
    ));
    assert!(registry
        .invoke(&client, "h", json!({}), 50)
        .await
        .unwrap_err()
        .to_string()
        .contains("stale or unknown"));
    timeout(Duration::from_secs(2), server)
        .await
        .unwrap()
        .unwrap();
    client.shutdown().await;
}

#[tokio::test]
async fn native_page_scripts_enforce_registration_identity() {
    let (local, mut remote) = tokio::io::duplex(256 * 1024);
    let client = Client::from_stream(local).unwrap();
    client.mark_attached(7).await;
    let server = tokio::spawn(async move {
        let list = list_reply(&mut remote).await;
        reply(&mut remote, "Page.getFrameTree", frame("loader-1")).await;
        let request = reply(
            &mut remote,
            "Runtime.evaluate",
            json!({"result":{"value":{"ok":true}}}),
        )
        .await;
        (
            list,
            request["params"]["commandParams"]["expression"]
                .as_str()
                .unwrap()
                .to_owned(),
        )
    });
    let registry = WebMcpRegistry::default();
    registry.list(&client, "7", 1000).await.unwrap();
    assert_eq!(
        registry
            .invoke(&client, "h", json!({"text":"quotes' and spaces"}), 1000)
            .await
            .unwrap(),
        json!({"ok":true})
    );
    let (list, invoke) = timeout(Duration::from_secs(2), server)
        .await
        .unwrap()
        .unwrap();
    client.shutdown().await;
    let fixture = format!(
        r#"
const assert = require('node:assert/strict');
const deadline = setTimeout(()=>process.exit(98),2000);
const list = {list}; const invoke = {invoke};
globalThis.window = globalThis;
globalThis.location = {{href:'https://example.com/',protocol:'https:'}};
globalThis.origin = 'https://example.com';
let pagehide;
globalThis.addEventListener = (name, fn) => {{ if(name==='pagehide') pagehide=fn; }};
globalThis.removeEventListener = () => {{}};
let listeners = [], calls = 0, tools;
const ctx = {{getTools:async()=>tools,addEventListener:(name,fn)=>{{assert.equal(name,'toolchange');listeners.push(fn);}},removeEventListener:()=>{{}},executeTool:async function(tool, input){{calls++;assert.equal(tool.name,'act');assert.equal(typeof input,'string');return JSON.parse(input);}}}};
globalThis.document = {{modelContext:ctx}};
const makeTool = () => ({{name:'act',origin,window,inputSchema:'{{"type":"object"}}'}});
const call = handle => eval(invoke.replace('"handle":"h"', '"handle":'+JSON.stringify(handle)));
(async()=>{{
  tools=[makeTool(),{{...makeTool(),name:'foreign',origin:'https://other.example'}}];
  const snapshot=await eval(list); assert.equal(snapshot.tools.length,1);
  let h=snapshot.tools[0].handle;
  assert.deepEqual(await call(h),{{text:"quotes' and spaces"}});assert.equal(calls,1);
  for(const listener of listeners) listener();
  tools=[makeTool()]; // Same descriptor/name, new registration must still fail.
  await assert.rejects(()=>call(h),/stale/);assert.equal(calls,1);
  h=(await eval(list)).tools[0].handle;
  tools[0].description='changed';await assert.rejects(()=>call(h),/stale/);assert.equal(calls,1);
  h=(await eval(list)).tools[0].handle;
  pagehide();await assert.rejects(()=>call(h),/stale/);assert.equal(calls,1);
  h=(await eval(list)).tools[0].handle;
  globalThis.document={{modelContext:ctx}};await assert.rejects(()=>call(h),/stale/);assert.equal(calls,1);
  h=(await eval(list)).tools[0].handle;
  location.href='https://example.com/next';await assert.rejects(()=>call(h),/stale/);assert.equal(calls,1);
  location.href='https://example.com/';
  h=(await eval(list)).tools[0].handle;
  ctx.executeTool=async function(tool){{calls++;return arguments[1];}};
  assert.deepEqual(await call(h),{{text:"quotes' and spaces"}});assert.equal(calls,2);
}})().then(()=>clearTimeout(deadline)).catch(error=>{{clearTimeout(deadline);console.error(error);process.exitCode=1;}});
"#,
        list = serde_json::to_string(&list).unwrap(),
        invoke = serde_json::to_string(&invoke).unwrap()
    );
    // Node is already part of the project's installer/CI toolchain. Keep Rust
    // consumers without Node able to run the pure wire-contract tests above.
    match std::process::Command::new("node")
        .args(["-e", &fixture])
        .output()
    {
        Ok(result) => assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("Node unavailable; native page JS fixture skipped")
        }
        Err(error) => panic!("cannot run WebMCP JS fixture: {error}"),
    }
}

#[tokio::test]
async fn rejects_unbounded_timeout_before_connecting() {
    let registry = WebMcpRegistry::default();
    let err = registry
        .list(&Client::lazy(None), "1", 30_001)
        .await
        .unwrap_err();
    assert!(matches!(err, BridgeError::User(message) if message.contains("<= 30000")));
}

#[tokio::test]
async fn rejects_non_numeric_tab_without_guessing_current_tab() {
    let registry = WebMcpRegistry::default();
    let err = registry
        .list(&Client::lazy(None), "active", 100)
        .await
        .unwrap_err();
    assert!(matches!(err, BridgeError::User(message) if message.contains("numeric tab_id")));
}

#[tokio::test]
async fn invoke_requires_exact_handle_and_bounds_input() {
    let registry = WebMcpRegistry::default();
    let err = registry
        .invoke(
            &Client::lazy(None),
            "not-a-page-handle",
            json!({"x":"y"}),
            100,
        )
        .await
        .unwrap_err();
    assert!(err.to_string().contains("stale or unknown"));
    let oversized = json!("x".repeat(64 * 1024 + 1));
    let err = registry
        .invoke(&Client::lazy(None), "h", oversized, 100)
        .await
        .unwrap_err();
    assert!(matches!(err, BridgeError::User(message) if message.contains("64 KiB")));
}
