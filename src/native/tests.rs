use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[test]
fn native_launch_preserves_host_policy_environment_and_working_directory() {
    let directory = tempfile::tempdir().unwrap();
    let executable = std::env::current_exe().unwrap();
    let services = json!({
        "browser":"@oai/browser-desktop/service",
        "sky":"@installed/sky/service",
        "host":"@installed/host/service"
    });
    let configuration = json!({"mcpServers":{"cua_repl":{
        "command":executable,"args":["native-entry.mjs"],"cwd":directory.path(),
        "env":{
            "CUA_REPL_NODE_REPL_PATH":executable,
            "CUA_REPL_ENABLED_SURFACES":"browser,computer",
            "NODE_REPL_TRUSTED_SERVICES":services.to_string(),
            "NATIVE_SANDBOX_CONTEXT":"host-owned"
        },
        "env_vars":[
            "HOST_FORWARDED_ENV",
            {"name":"HOST_LOCAL_FORWARDED_ENV","source":"local"}
        ],
        "startup_timeout_ms":4500,
        "tool_timeout_sec":12.5,
        "enabled_tools":["js","turn_ended"],
        "disabled_tools":["js_reset"]
    }}});
    let path = directory.path().join("native.json");
    std::fs::write(&path, configuration.to_string()).unwrap();
    let launch = Launch::read(NativeServer::Computer, &path).unwrap();
    assert_eq!(launch.command, executable.to_string_lossy());
    assert_eq!(launch.args, ["native-entry.mjs"]);
    assert_eq!(launch.cwd.as_deref(), Some(directory.path()));
    assert_eq!(launch.env["NATIVE_SANDBOX_CONTEXT"], "host-owned");
    assert_eq!(launch.env["CUA_REPL_ENABLED_SURFACES"], "browser,computer");
    let selected: Value = serde_json::from_str(&launch.env["NODE_REPL_TRUSTED_SERVICES"]).unwrap();
    assert_eq!(selected, services);
    assert_eq!(launch.env_vars.len(), 2);
    assert_eq!(
        launch.startup_timeout().unwrap(),
        Duration::from_millis(4500)
    );
    assert_eq!(launch.tool_timeout().unwrap(), Duration::from_millis(12500));
    let policy = launch.tool_policy();
    assert!(policy.allows("js"));
    assert!(policy.allows("turn_ended"));
    assert!(!policy.allows("js_reset"));
    assert!(!policy.allows("js_add_node_module_dir"));
}

#[test]
fn tool_timeout_defaults_to_codex_cli_values() {
    let directory = tempfile::tempdir().unwrap();
    let configuration = json!({"mcpServers":{"cua_repl":{
        "command":std::env::current_exe().unwrap(),
        "env":{"CUA_REPL_NODE_REPL_PATH":std::env::current_exe().unwrap()}
    }}});
    let path = directory.path().join("native.json");
    std::fs::write(&path, configuration.to_string()).unwrap();
    let launch = Launch::read(NativeServer::Computer, &path).unwrap();
    assert_eq!(launch.startup_timeout().unwrap(), Duration::from_secs(30));
    assert_eq!(launch.tool_timeout().unwrap(), Duration::from_secs(300));
}

#[test]
fn native_launch_rejects_invalid_absolute_command_and_timeouts() {
    let directory = tempfile::tempdir().unwrap();
    let executable = std::env::current_exe().unwrap();
    for (name, row, expected) in [
        (
            "missing-command",
            json!({
                "command":directory.path().join("missing.exe"),
                "env":{"CUA_REPL_NODE_REPL_PATH":executable}
            }),
            "installed executable",
        ),
        (
            "negative-timeout",
            json!({
                "command":executable,
                "env":{"CUA_REPL_NODE_REPL_PATH":executable},
                "tool_timeout_sec":-1
            }),
            "finite and nonnegative",
        ),
        (
            "disabled",
            json!({
                "command":executable,
                "env":{"CUA_REPL_NODE_REPL_PATH":executable},
                "enabled":false
            }),
            "disabled",
        ),
    ] {
        let path = directory.path().join(format!("{name}.json"));
        std::fs::write(&path, json!({"mcpServers":{"cua_repl":row}}).to_string()).unwrap();
        let error = Launch::read(NativeServer::Computer, &path).unwrap_err();
        assert!(error.to_string().contains(expected), "{name}: {error:#}");
    }
}

#[cfg(windows)]
#[test]
fn windows_program_resolution_uses_constructed_path() {
    let directory = tempfile::tempdir().unwrap();
    let executable = directory.path().join("native-probe.exe");
    std::fs::copy(std::env::current_exe().unwrap(), &executable).unwrap();
    let environment = BTreeMap::from([(
        OsString::from("PATH"),
        directory.path().as_os_str().to_os_string(),
    )]);
    let resolved = resolve_program(
        "native-probe",
        &environment,
        &std::env::current_dir().unwrap(),
    );
    assert_eq!(
        std::fs::canonicalize(resolved).unwrap(),
        std::fs::canonicalize(executable).unwrap()
    );

    let empty = tempfile::tempdir().unwrap();
    let environment = BTreeMap::from([(
        OsString::from("PATH"),
        empty.path().as_os_str().to_os_string(),
    )]);
    assert_eq!(
        resolve_program(
            "native-probe",
            &environment,
            &std::env::current_dir().unwrap()
        ),
        OsString::from("native-probe")
    );
}

#[test]
fn parse_context_accepts_object_and_json_string_forms() {
    for value in [
        json!({"session_id":"session","turn_id":"turn","thread_id":"thread"}),
        json!(r#"{"session_id":"session","turn_id":"turn"}"#),
    ] {
        assert_eq!(parse_context(&value).unwrap()["session_id"], "session");
    }
    let error = parse_context(&json!({"session_id":"session","turn_id":""})).unwrap_err();
    assert!(error.to_string().contains("turn_id"), "{error:#}");
}

#[test]
fn native_launch_rejects_remote_env_vars_for_local_stdio() {
    let directory = tempfile::tempdir().unwrap();
    let executable = std::env::current_exe().unwrap();
    let configuration = json!({"mcpServers":{"cua_repl":{
        "command":executable,
        "env":{"CUA_REPL_NODE_REPL_PATH":executable},
        "env_vars":[{"name":"REMOTE_ONLY","source":"remote"}]
    }}});
    let path = directory.path().join("native.json");
    std::fs::write(&path, configuration.to_string()).unwrap();
    let error = Launch::read(NativeServer::Computer, &path).unwrap_err();
    assert!(error.to_string().contains("requires remote MCP stdio"));
}

#[test]
fn tool_policy_filters_catalog_after_allowlist_then_denylist() {
    let policy = ToolPolicy {
        enabled: Some(BTreeSet::from(["js".to_string(), "js_reset".to_string()])),
        disabled: BTreeSet::from(["js_reset".to_string()]),
    };
    let mut catalog = json!({"tools":[
        {"name":"js"},
        {"name":"js_reset"},
        {"name":"turn_ended"}
    ],"nextCursor":"next"});
    policy.filter_catalog(&mut catalog);
    assert_eq!(
        catalog,
        json!({"tools":[{"name":"js"}],"nextCursor":"next"})
    );
}

async fn send<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>(
    transport: &mut JsonTransport<R, W>,
    row: Value,
) {
    tokio::time::timeout(Duration::from_secs(2), transport.send(row))
        .await
        .unwrap()
        .unwrap();
}

async fn receive<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>(
    transport: &mut JsonTransport<R, W>,
) -> Value {
    tokio::time::timeout(Duration::from_secs(2), transport.receive())
        .await
        .unwrap()
        .unwrap()
        .unwrap()
}

fn assert_cleanup_metadata(actual: &Value, execution: &Value) {
    let mut expected = execution.clone();
    expected.as_object_mut().unwrap().remove("progressToken");
    expected["callId"] = actual["callId"].clone();
    Uuid::parse_str(actual["callId"].as_str().unwrap()).unwrap();
    assert_ne!(actual["callId"], execution["callId"]);
    assert_eq!(*actual, expected);
}

#[tokio::test]
async fn relay_preserves_unknown_fields_in_known_mcp_messages() {
    let (host_io, mut client_io) = tokio::io::duplex(32768);
    let (bridge_io, fixture_io) = tokio::io::duplex(32768);
    let (hr, hw) = tokio::io::split(host_io);
    let (nr, nw) = tokio::io::split(bridge_io);
    let task = tokio::spawn(async move {
        relay(
            &mut JsonTransport::new(hr, hw),
            &mut JsonTransport::new(nr, nw),
            std::future::pending(),
            &ToolPolicy::default(),
        )
        .await
    });
    let (fr, mut fw) = tokio::io::split(fixture_io);
    let mut fixture = BufReader::new(fr);
    let initialize = json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
        "protocolVersion":"2025-03-26","capabilities":{"nativeCapability":{"required":true}},
        "clientInfo":{"name":"synthetic","version":"1"},"nativeHandshake":{"keep":true}
    }});
    client_io
        .write_all(format!("{initialize}\n").as_bytes())
        .await
        .unwrap();
    let mut line = String::new();
    timeout_read_line(&mut fixture, &mut line).await;
    assert_eq!(serde_json::from_str::<Value>(&line).unwrap(), initialize);
    let initialized = json!({"jsonrpc":"2.0","id":1,"result":{
        "protocolVersion":"2025-03-26","capabilities":{"tools":{},"nativeCapability":{"required":true}},
        "serverInfo":{"name":"synthetic-native","version":"1"},
        "instructions":"Native startup instructions"
    }});
    fw.write_all(format!("{initialized}\n").as_bytes())
        .await
        .unwrap();
    let mut client = BufReader::new(client_io);
    line.clear();
    timeout_read_line(&mut client, &mut line).await;
    assert_eq!(serde_json::from_str::<Value>(&line).unwrap(), initialized);

    let list = json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}});
    client
        .get_mut()
        .write_all(format!("{list}\n").as_bytes())
        .await
        .unwrap();
    line.clear();
    timeout_read_line(&mut fixture, &mut line).await;
    assert_eq!(serde_json::from_str::<Value>(&line).unwrap(), list);
    let catalog = json!({"jsonrpc":"2.0","id":2,"result":{"tools":[{
        "name":"js","description":"Native JS instructions","inputSchema":{"type":"object"},
        "nativeTool":{"keep":true},"annotations":{"nativeHint":"required"}
    }]}});
    fw.write_all(format!("{catalog}\n").as_bytes())
        .await
        .unwrap();
    line.clear();
    timeout_read_line(&mut client, &mut line).await;
    assert_eq!(serde_json::from_str::<Value>(&line).unwrap(), catalog);

    let request = json!({"jsonrpc":"2.0","id":"external","method":"tools/call",
        "nativeEnvelope":{"required":true},
        "params":{"name":"js","arguments":{"code":"synthetic"},
            "nativeTimeoutMs":5000}});
    client
        .get_mut()
        .write_all(format!("{request}\n").as_bytes())
        .await
        .unwrap();
    line.clear();
    timeout_read_line(&mut fixture, &mut line).await;
    let forwarded: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(forwarded["params"]["nativeTimeoutMs"], 5000);
    assert_eq!(forwarded["nativeEnvelope"], request["nativeEnvelope"]);
    let result = json!({"jsonrpc":"2.0","id":"external","result":{
        "content":[{"type":"text","text":"Native per-session instructions and result"}],
        "_meta":{"nativeExtension":true}
    }});
    fw.write_all(format!("{result}\n").as_bytes())
        .await
        .unwrap();
    line.clear();
    timeout_read_line(&mut client, &mut line).await;
    assert_eq!(serde_json::from_str::<Value>(&line).unwrap(), result);
    task.abort();
    let _ = task.await;
}

#[tokio::test]
async fn relay_rejects_bad_call_metadata_without_closing_the_session() {
    let (host_io, client_io) = tokio::io::duplex(32768);
    let (native_io, fixture_io) = tokio::io::duplex(32768);
    let (hr, hw) = tokio::io::split(host_io);
    let (nr, nw) = tokio::io::split(native_io);
    let task = tokio::spawn(async move {
        relay(
            &mut JsonTransport::new(hr, hw),
            &mut JsonTransport::new(nr, nw),
            std::future::pending(),
            &ToolPolicy::default(),
        )
        .await
    });
    let (cr, cw) = tokio::io::split(client_io);
    let (fr, fw) = tokio::io::split(fixture_io);
    let mut client = JsonTransport::new(cr, cw);
    let mut fixture = JsonTransport::new(fr, fw);
    for meta in [
        json!([]),
        json!({"x-codex-turn-metadata":{"session_id":"s"}}),
    ] {
        send(
            &mut client,
            json!({"jsonrpc":"2.0","id":7,"method":"tools/call",
            "params":{"name":"js","arguments":{},"_meta":meta}}),
        )
        .await;
        let error = receive(&mut client).await;
        assert_eq!(error["id"], 7);
        assert_eq!(error["error"]["code"], -32602);
    }
    let ping = json!({"jsonrpc":"2.0","id":8,"method":"ping","params":{}});
    send(&mut client, ping.clone()).await;
    assert_eq!(receive(&mut fixture).await, ping);
    drop(client);
    tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn relay_filters_catalog_and_rejects_disabled_tool_calls() {
    let (host_io, client_io) = tokio::io::duplex(32768);
    let (native_io, fixture_io) = tokio::io::duplex(32768);
    let (hr, hw) = tokio::io::split(host_io);
    let (nr, nw) = tokio::io::split(native_io);
    let policy = ToolPolicy {
        enabled: Some(BTreeSet::from([
            "js".to_string(),
            "js_add_node_module_dir".to_string(),
            "js_reset".to_string(),
            "turn_ended".to_string(),
        ])),
        disabled: BTreeSet::from(["js_reset".to_string(), "turn_ended".to_string()]),
    };
    let task = tokio::spawn(async move {
        relay(
            &mut JsonTransport::new(hr, hw),
            &mut JsonTransport::new(nr, nw),
            std::future::pending(),
            &policy,
        )
        .await
    });
    let (cr, cw) = tokio::io::split(client_io);
    let (fr, fw) = tokio::io::split(fixture_io);
    let mut client = JsonTransport::new(cr, cw);
    let mut fixture = JsonTransport::new(fr, fw);

    let list = json!({"jsonrpc":"2.0","id":4,"method":"tools/list","params":{}});
    send(&mut client, list.clone()).await;
    assert_eq!(receive(&mut fixture).await, list);
    send(
        &mut fixture,
        json!({"jsonrpc":"2.0","id":4,"result":{"tools":[
            {"name":"js","inputSchema":{"type":"object"}},
            {"name":"js_reset","inputSchema":{"type":"object"}},
            {"name":"turn_ended","inputSchema":{"type":"object"}}
        ]}}),
    )
    .await;
    assert_eq!(
        receive(&mut client).await,
        json!({"jsonrpc":"2.0","id":4,"result":{"tools":[
            {"name":"js","inputSchema":{"type":"object"}}
        ]}})
    );

    send(
        &mut client,
        json!({"jsonrpc":"2.0","id":5,"method":"tools/call",
            "params":{"name":"js_reset","arguments":{}}}),
    )
    .await;
    let error = receive(&mut client).await;
    assert_eq!(error["id"], 5);
    assert_eq!(error["error"]["code"], -32602);
    assert!(error["error"]["message"]
        .as_str()
        .unwrap()
        .contains("disabled"));

    send(
        &mut client,
        json!({"jsonrpc":"2.0","method":"tools/call","params":{"name":"js_reset"}}),
    )
    .await;
    send(
        &mut client,
        json!({"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":false}}),
    )
    .await;
    let malformed = receive(&mut client).await;
    assert_eq!(malformed["id"], 6);
    assert_eq!(malformed["error"]["code"], -32602);

    send(
        &mut client,
        json!({"jsonrpc":"2.0","id":"call-2","method":"tools/call",
        "params":{"name":"js_add_node_module_dir","arguments":{"path":"/tmp"},
            "_meta":{"x-codex-turn-metadata":{
                "session_id":"session","thread_id":"thread","turn_id":"turn"
            }}}}),
    )
    .await;
    let forwarded = receive(&mut fixture).await;
    assert_eq!(forwarded["params"]["name"], "js_add_node_module_dir");
    Uuid::parse_str(forwarded["params"]["_meta"]["callId"].as_str().unwrap()).unwrap();
    assert_eq!(forwarded["params"]["_meta"]["sessionId"], "session");
    assert_eq!(forwarded["params"]["_meta"]["threadId"], "thread");
    send(
        &mut fixture,
        json!({"jsonrpc":"2.0","id":"call-2","result":{"content":[]}}),
    )
    .await;
    assert_eq!(receive(&mut client).await["id"], "call-2");

    drop(client);
    tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn relay_skips_disabled_turn_ended_cleanup() {
    let (host_io, client_io) = tokio::io::duplex(32768);
    let (native_io, fixture_io) = tokio::io::duplex(32768);
    let (hr, hw) = tokio::io::split(host_io);
    let (nr, nw) = tokio::io::split(native_io);
    let policy = ToolPolicy {
        enabled: Some(BTreeSet::from(["js".to_string(), "turn_ended".to_string()])),
        disabled: BTreeSet::from(["turn_ended".to_string()]),
    };
    let task = tokio::spawn(async move {
        relay(
            &mut JsonTransport::new(hr, hw),
            &mut JsonTransport::new(nr, nw),
            std::future::pending(),
            &policy,
        )
        .await
    });
    let (cr, cw) = tokio::io::split(client_io);
    let (fr, fw) = tokio::io::split(fixture_io);
    let mut client = JsonTransport::new(cr, cw);
    let mut fixture = JsonTransport::new(fr, fw);
    send(
        &mut client,
        json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
        "params":{"name":"js","arguments":{"code":"synthetic"},
            "_meta":{"x-codex-turn-metadata":{
                "session_id":"session","turn_id":"turn"
            }}}}),
    )
    .await;
    assert_eq!(receive(&mut fixture).await["params"]["name"], "js");
    send(
        &mut fixture,
        json!({"jsonrpc":"2.0","id":1,"result":{"content":[]}}),
    )
    .await;
    assert_eq!(receive(&mut client).await["id"], 1);

    drop(client);
    let row = tokio::time::timeout(Duration::from_secs(2), fixture.receive())
        .await
        .unwrap()
        .unwrap();
    assert!(
        row.is_none(),
        "disabled turn_ended cleanup was forwarded to native"
    );
    tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn relay_keeps_a_partial_input_line_while_forwarding_native_events() {
    let (host_io, mut client_io) = tokio::io::duplex(32768);
    let (native_io, fixture_io) = tokio::io::duplex(32768);
    let (hr, hw) = tokio::io::split(host_io);
    let (nr, nw) = tokio::io::split(native_io);
    let task = tokio::spawn(async move {
        relay(
            &mut JsonTransport::new(hr, hw),
            &mut JsonTransport::new(nr, nw),
            std::future::pending(),
            &ToolPolicy::default(),
        )
        .await
    });
    let (fr, fw) = tokio::io::split(fixture_io);
    let mut fixture = JsonTransport::new(fr, fw);
    client_io.write_all(b"{\"jsonrpc\":\"2.0\",").await.unwrap();
    let event =
        json!({"jsonrpc":"2.0","method":"notifications/native-event","params":{"keep":true}});
    send(&mut fixture, event.clone()).await;
    let mut client = BufReader::new(client_io);
    let mut line = String::new();
    timeout_read_line(&mut client, &mut line).await;
    assert_eq!(serde_json::from_str::<Value>(&line).unwrap(), event);
    client
        .get_mut()
        .write_all(b"\"id\":3,\"method\":\"ping\"}\n")
        .await
        .unwrap();
    assert_eq!(
        receive(&mut fixture).await,
        json!({"jsonrpc":"2.0","id":3,"method":"ping"})
    );
    drop(client);
    tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn relay_reports_interruption_of_a_partial_client_write_as_an_error() {
    let (host_io, _client_io) = tokio::io::duplex(8);
    let (native_io, fixture_io) = tokio::io::duplex(32768);
    let (hr, hw) = tokio::io::split(host_io);
    let (nr, nw) = tokio::io::split(native_io);
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        relay(
            &mut JsonTransport::new(hr, hw),
            &mut JsonTransport::new(nr, nw),
            async {
                let _ = stop_rx.await;
            },
            &ToolPolicy::default(),
        )
        .await
    });
    let (fr, fw) = tokio::io::split(fixture_io);
    let mut fixture = JsonTransport::new(fr, fw);
    send(
        &mut fixture,
        json!({"jsonrpc":"2.0","id":1,"result":{"native": "x".repeat(4096)}}),
    )
    .await;
    tokio::time::sleep(Duration::from_millis(20)).await;
    stop_tx.send(()).unwrap();
    let error = tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap_err();
    assert!(error.to_string().contains("client stream may be truncated"));
}

async fn timeout_read_line<R: tokio::io::AsyncRead + Unpin>(
    reader: &mut BufReader<R>,
    line: &mut String,
) {
    tokio::time::timeout(Duration::from_secs(2), reader.read_line(line))
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn relay_keeps_approval_progress_extensions_and_execution_context() {
    for action in ["accept", "decline", "cancel"] {
        let (host_io, client_io) = tokio::io::duplex(32768);
        let (bridge_io, fixture_io) = tokio::io::duplex(32768);
        let (hr, hw) = tokio::io::split(host_io);
        let (nr, nw) = tokio::io::split(bridge_io);
        let task = tokio::spawn(async move {
            relay(
                &mut JsonTransport::new(hr, hw),
                &mut JsonTransport::new(nr, nw),
                std::future::pending(),
                &ToolPolicy::default(),
            )
            .await
        });
        let (cr, cw) = tokio::io::split(client_io);
        let (fr, fw) = tokio::io::split(fixture_io);
        let mut client = JsonTransport::new(cr, cw);
        let mut fixture = JsonTransport::new(fr, fw);
        let meta = json!({"sessionId":"external-session","threadId":"external-thread",
            "windowId":"external-window","callId":"external-call","progressToken":"host-progress",
            "x-codex-turn-metadata":{"session_id":"actual-session","turn_id":"actual-turn"},
            "native_extension":{"opaque":true}});
        let call = json!({"jsonrpc":"2.0","id":"host-request","method":"tools/call",
            "params":{"name":"js","arguments":{"code":"synthetic"},"_meta":meta}});
        send(&mut client, call.clone()).await;
        assert_eq!(receive(&mut fixture).await, call);
        let progress = json!({"jsonrpc":"2.0","method":"notifications/progress",
            "params":{"progressToken":"host-progress","progress":1.0,"total":2.0,
            "_meta":{"native_event":"preserved"}}});
        send(&mut fixture, progress.clone()).await;
        assert_eq!(receive(&mut client).await, progress);
        let approval = json!({"jsonrpc":"2.0","id":"native-request","method":"elicitation/create",
            "params":{"mode":"form","message":"Synthetic approval",
            "requestedSchema":{"type":"object","properties":{}},
            "_meta":{"persist":["session","always"],"connector_id":"computer-use"}}});
        send(&mut fixture, approval.clone()).await;
        assert_eq!(receive(&mut client).await, approval);
        let decision = json!({"jsonrpc":"2.0","id":"native-request",
            "result":{"action":action,"content":{},"_meta":{"persist":"session"}}});
        send(&mut client, decision.clone()).await;
        assert_eq!(receive(&mut fixture).await, decision);
        let event = json!({"jsonrpc":"2.0","method":"notifications/native-event",
            "params":{"payload":[1,2],"_meta":{"sequence":12}}});
        send(&mut fixture, event.clone()).await;
        assert_eq!(receive(&mut client).await, event);
        send(&mut client, event.clone()).await;
        assert_eq!(receive(&mut fixture).await, event);
        let result = json!({"jsonrpc":"2.0","id":"host-request","result":{
            "content":[{"type":"image","mimeType":"image/png","data":"aGVsbG8=",
            "_meta":{"screenshotId":"native-region"}}],"_meta":{"result":"native"}}});
        send(&mut fixture, result.clone()).await;
        assert_eq!(receive(&mut client).await, result);
        let catalog = json!({"jsonrpc":"2.0","id":4,"method":"tools/list","params":{}});
        send(&mut client, catalog.clone()).await;
        assert_eq!(receive(&mut fixture).await, catalog);
        drop(client);
        let pending_catalog = receive(&mut fixture).await;
        assert_eq!(pending_catalog["method"], "notifications/cancelled");
        assert_eq!(pending_catalog["params"]["requestId"], 4);
        let cleanup = receive(&mut fixture).await;
        assert_eq!(cleanup["params"]["name"], "turn_ended");
        assert_eq!(
            cleanup["params"]["arguments"],
            json!({"hook_event_name":"Stop","session_id":"actual-session","turn_id":"actual-turn"})
        );
        assert_cleanup_metadata(&cleanup["params"]["_meta"], &meta);
        send(
            &mut fixture,
            json!({"jsonrpc":"2.0","id":cleanup["id"],"result":{"content":[]}}),
        )
        .await;
        tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }
}

#[tokio::test]
async fn relay_external_cancellation_is_not_repeated_at_eof() {
    let (host_io, client_io) = tokio::io::duplex(32768);
    let (native_io, fixture_io) = tokio::io::duplex(32768);
    let (hr, hw) = tokio::io::split(host_io);
    let (nr, nw) = tokio::io::split(native_io);
    let task = tokio::spawn(async move {
        relay(
            &mut JsonTransport::new(hr, hw),
            &mut JsonTransport::new(nr, nw),
            std::future::pending(),
            &ToolPolicy::default(),
        )
        .await
    });
    let (cr, cw) = tokio::io::split(client_io);
    let (fr, fw) = tokio::io::split(fixture_io);
    let mut client = JsonTransport::new(cr, cw);
    let mut fixture = JsonTransport::new(fr, fw);
    send(
        &mut client,
        json!({"jsonrpc":"2.0","id":42,"method":"tools/call",
        "params":{"name":"js","arguments":{}}}),
    )
    .await;
    let call = receive(&mut fixture).await;
    let cancel = json!({"jsonrpc":"2.0","method":"notifications/cancelled",
        "params":{"requestId":42,"reason":"host stopped","_meta":{"native":"context"}}});
    send(&mut client, cancel.clone()).await;
    assert_eq!(receive(&mut fixture).await, cancel);
    drop(client);
    let cleanup = receive(&mut fixture).await;
    assert_eq!(cleanup["params"]["name"], "turn_ended");
    assert_cleanup_metadata(&cleanup["params"]["_meta"], &call["params"]["_meta"]);
    send(
        &mut fixture,
        json!({"jsonrpc":"2.0","id":cleanup["id"],"result":{"content":[]}}),
    )
    .await;
    tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn relay_treats_eof_with_pending_tool_as_interrupt() {
    let (host_io, client_io) = tokio::io::duplex(32768);
    let (native_io, fixture_io) = tokio::io::duplex(32768);
    let (hr, hw) = tokio::io::split(host_io);
    let (nr, nw) = tokio::io::split(native_io);
    let task = tokio::spawn(async move {
        relay(
            &mut JsonTransport::new(hr, hw),
            &mut JsonTransport::new(nr, nw),
            std::future::pending(),
            &ToolPolicy::default(),
        )
        .await
    });
    let (cr, cw) = tokio::io::split(client_io);
    let (fr, fw) = tokio::io::split(fixture_io);
    let mut client = JsonTransport::new(cr, cw);
    let mut fixture = JsonTransport::new(fr, fw);
    send(
        &mut client,
        json!({"jsonrpc":"2.0","id":42,"method":"tools/call","params":{"name":"js"}}),
    )
    .await;
    assert_eq!(receive(&mut fixture).await["id"], 42);
    drop(client);
    let cancel = receive(&mut fixture).await;
    assert_eq!(cancel["params"]["requestId"], 42);
    let cleanup = receive(&mut fixture).await;
    assert_eq!(
        cleanup["params"]["arguments"]["hook_event_name"],
        "Interrupt"
    );
    send(
        &mut fixture,
        json!({"jsonrpc":"2.0","id":"ping","method":"ping"}),
    )
    .await;
    assert_eq!(receive(&mut fixture).await["result"], json!({}));
    send(
        &mut fixture,
        json!({"jsonrpc":"2.0","id":cleanup["id"],"result":{"content":[]}}),
    )
    .await;
    let error = tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap_err();
    assert!(error
        .to_string()
        .contains("outcome unknown and not replayed"));
}

#[tokio::test]
async fn relay_interrupts_a_backpressured_write_and_retires_the_stream() {
    let (host_io, client_io) = tokio::io::duplex(32768);
    let (native_io, _fixture_io) = tokio::io::duplex(8);
    let (hr, hw) = tokio::io::split(host_io);
    let (nr, nw) = tokio::io::split(native_io);
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        relay(
            &mut JsonTransport::new(hr, hw),
            &mut JsonTransport::new(nr, nw),
            async {
                let _ = stop_rx.await;
            },
            &ToolPolicy::default(),
        )
        .await
    });
    let (cr, cw) = tokio::io::split(client_io);
    let mut client = JsonTransport::new(cr, cw);
    send(
        &mut client,
        json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
        "params":{"name":"js","arguments":{"code":"x".repeat(16000)}}}),
    )
    .await;
    tokio::time::sleep(Duration::from_millis(20)).await;
    stop_tx.send(()).unwrap();
    let error = tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap_err();
    assert!(error.to_string().contains("unknown and not replayed"));
}

#[tokio::test]
async fn relay_forwards_opposite_direction_while_one_write_is_backpressured() {
    let (host_io, client_io) = tokio::io::duplex(64);
    let (native_io, fixture_io) = tokio::io::duplex(32768);
    let (hr, hw) = tokio::io::split(host_io);
    let (nr, nw) = tokio::io::split(native_io);
    let task = tokio::spawn(async move {
        relay(
            &mut JsonTransport::new(hr, hw),
            &mut JsonTransport::new(nr, nw),
            std::future::pending(),
            &ToolPolicy::default(),
        )
        .await
    });
    let (cr, cw) = tokio::io::split(client_io);
    let (fr, fw) = tokio::io::split(fixture_io);
    let mut client = JsonTransport::new(cr, cw);
    let mut fixture = JsonTransport::new(fr, fw);

    // Native sends a large frame that saturates the 64-byte host duplex buffer.
    let large_event = json!({
        "jsonrpc": "2.0",
        "method": "notifications/native-event",
        "params": {"payload": "x".repeat(4096)}
    });
    send(&mut fixture, large_event.clone()).await;
    tokio::time::sleep(Duration::from_millis(20)).await;

    // While host write is backpressured, client -> native frames must still flow immediately.
    let cancel = json!({
        "jsonrpc": "2.0",
        "method": "notifications/cancelled",
        "params": {"requestId": 99, "reason": "user cancelled"}
    });
    send(&mut client, cancel.clone()).await;
    assert_eq!(receive(&mut fixture).await, cancel);

    // Now drain the large event from client and close cleanly.
    assert_eq!(receive(&mut client).await, large_event);
    drop(client);
    tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn noninteractive_client_never_accepts_native_application_approval() {
    let (native_io, client_io) = tokio::io::duplex(32768);
    let count = Arc::new(AtomicUsize::new(0));
    let fixture_count = count.clone();
    let fixture = tokio::spawn(async move {
        let (fr, fw) = tokio::io::split(native_io);
        let mut transport = JsonTransport::new(fr, fw);
        let init = receive(&mut transport).await;
        assert_eq!(init["method"], "initialize");
        send(
            &mut transport,
            json!({
                "jsonrpc": "2.0",
                "id": init["id"],
                "result": {
                    "protocolVersion": "2025-03-26",
                    "capabilities": {"tools": {}},
                    "serverInfo": {"name": "synthetic", "version": "1"}
                }
            }),
        )
        .await;
        let initialized = receive(&mut transport).await;
        assert_eq!(initialized["method"], "notifications/initialized");

        let call = receive(&mut transport).await;
        assert_eq!(call["method"], "tools/call");
        fixture_count.fetch_add(1, Ordering::SeqCst);

        send(
            &mut transport,
            json!({
                "jsonrpc": "2.0",
                "id": "approval-1",
                "method": "elicitation/create",
                "params": {
                    "mode": "form",
                    "message": "Synthetic app approval",
                    "requestedSchema": {"type": "object", "properties": {}},
                    "_meta": {"persist": ["session"], "connector_id": "computer-use"}
                }
            }),
        )
        .await;
        let decision = receive(&mut transport).await;
        assert_eq!(decision["id"], "approval-1");
        assert_eq!(decision["result"]["action"], "cancel");

        send(
            &mut transport,
            json!({
                "jsonrpc": "2.0",
                "id": call["id"],
                "error": {
                    "code": -32603,
                    "message": "elicitation cancelled by non-interactive client"
                }
            }),
        )
        .await;
    });

    let (cr, cw) = tokio::io::split(client_io);
    let mut worker = Worker::from_transport(
        JsonTransport::new(cr, cw),
        None,
        ToolPolicy::default(),
        DEFAULT_START_TIMEOUT,
        DEFAULT_TOOL_TIMEOUT,
    );
    worker.initialize("synthetic-cli").await.unwrap();
    let err = worker
        .tool("js", json!({"code": "await cua.listWindows();"}))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("elicitation cancelled"));
    assert_eq!(count.load(Ordering::SeqCst), 1);
    fixture.await.unwrap();
}

#[test]
fn save_images_preserves_order_and_metadata() {
    let directory = tempfile::tempdir().unwrap();
    let value = json!({"content":[
        {"type":"text","text":"before"},
        {"type":"image","mimeType":"image/png","data":"aGVsbG8=","_meta":{"screenshotId":"first"}},
        {"type":"image","mimeType":"image/webp","data":"d29ybGQ=","_meta":{"screenshotId":"second"}}
    ]});
    let artifacts = save_images(&value, directory.path()).unwrap();
    assert_eq!(artifacts.len(), 2);
    assert_eq!(artifacts[0]["content_index"], 1);
    assert_eq!(artifacts[0]["_meta"]["screenshotId"], "first");
    assert_eq!(artifacts[1]["content_index"], 2);
    assert_eq!(artifacts[1]["_meta"]["screenshotId"], "second");
    let first = PathBuf::from(artifacts[0]["path"].as_str().unwrap());
    let second = PathBuf::from(artifacts[1]["path"].as_str().unwrap());
    assert_eq!(std::fs::read(first).unwrap(), b"hello");
    assert_eq!(std::fs::read(second).unwrap(), b"world");
}

#[tokio::test]
async fn relay_filters_late_tools_list_response_after_cancellation() {
    let (host_client, host_bridge) = tokio::io::duplex(32768);
    let (native_bridge, native_server) = tokio::io::duplex(32768);
    let (hr, hw) = tokio::io::split(host_bridge);
    let (nr, nw) = tokio::io::split(native_bridge);
    let mut host_transport = JsonTransport::new(hr, hw);
    let mut native_transport = JsonTransport::new(nr, nw);
    let policy = ToolPolicy {
        enabled: Some(BTreeSet::from(["js".to_string()])),
        disabled: BTreeSet::new(),
    };
    let relay_task = tokio::spawn(async move {
        relay(
            &mut host_transport,
            &mut native_transport,
            std::future::pending::<()>(),
            &policy,
        )
        .await
    });

    let (hcr, hcw) = tokio::io::split(host_client);
    let mut client = JsonTransport::new(hcr, hcw);
    let (nsr, nsw) = tokio::io::split(native_server);
    let mut server = JsonTransport::new(nsr, nsw);

    send(
        &mut client,
        json!({"jsonrpc":"2.0","id":11,"method":"tools/list","params":{}}),
    )
    .await;
    assert_eq!(receive(&mut server).await["method"], "tools/list");
    send(
        &mut client,
        json!({"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":11}}),
    )
    .await;
    assert_eq!(
        receive(&mut server).await["method"],
        "notifications/cancelled"
    );
    send(
        &mut server,
        json!({
            "jsonrpc":"2.0","id":11,
            "result":{"tools":[{"name":"js"},{"name":"turn_ended"}]}
        }),
    )
    .await;
    let reply = receive(&mut client).await;
    let names: Vec<_> = reply["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["js"]);
    drop(client);
    relay_task.await.unwrap().unwrap();
}

#[tokio::test]
async fn relay_skips_duplicate_turn_ended_after_explicit_host_turn_ended() {
    let (host_client, host_bridge) = tokio::io::duplex(32768);
    let (native_bridge, native_server) = tokio::io::duplex(32768);
    let (hr, hw) = tokio::io::split(host_bridge);
    let (nr, nw) = tokio::io::split(native_bridge);
    let mut host_transport = JsonTransport::new(hr, hw);
    let mut native_transport = JsonTransport::new(nr, nw);
    let policy = ToolPolicy::default();
    let relay_task = tokio::spawn(async move {
        relay(
            &mut host_transport,
            &mut native_transport,
            std::future::pending::<()>(),
            &policy,
        )
        .await
    });

    let (hcr, hcw) = tokio::io::split(host_client);
    let mut client = JsonTransport::new(hcr, hcw);
    let (nsr, nsw) = tokio::io::split(native_server);
    let mut server = JsonTransport::new(nsr, nsw);

    send(
        &mut client,
        json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"js","arguments":{"code":"1+1"}}}),
    )
    .await;
    let js_req = receive(&mut server).await;
    send(
        &mut server,
        json!({"jsonrpc":"2.0","id":1,"result":{"content":[]}}),
    )
    .await;
    let _ = receive(&mut client).await;

    let ctx = js_req["params"]["_meta"]["x-codex-turn-metadata"].clone();
    send(
        &mut client,
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"turn_ended","arguments":{"hook_event_name":"Stop","session_id":ctx["session_id"],"turn_id":ctx["turn_id"]}}}),
    )
    .await;
    assert_eq!(receive(&mut server).await["params"]["name"], "turn_ended");
    send(
        &mut server,
        json!({"jsonrpc":"2.0","id":2,"result":{"content":[]}}),
    )
    .await;
    let _ = receive(&mut client).await;

    drop(client);
    assert!(
        tokio::time::timeout(Duration::from_millis(200), server.receive())
            .await
            .unwrap()
            .unwrap()
            .is_none(),
        "EOF must not synthesize a duplicate turn_ended"
    );
    relay_task.await.unwrap().unwrap();
}

#[tokio::test]
async fn cleanup_tolerates_missing_turn_ended_tool() {
    let (native_bridge, native_server) = tokio::io::duplex(32768);
    let (nr, nw) = tokio::io::split(native_bridge);
    let mut native_transport = JsonTransport::new(nr, nw);
    let (nsr, nsw) = tokio::io::split(native_server);
    let mut server = JsonTransport::new(nsr, nsw);

    let responder = tokio::spawn(async move {
        let req = receive(&mut server).await;
        assert_eq!(req["params"]["name"], "turn_ended");
        send(
            &mut server,
            json!({"jsonrpc":"2.0","id":req["id"],"error":{"code":-32601,"message":"unknown tool turn_ended"}}),
        )
        .await;
    });

    let meta = json!({"x-codex-turn-metadata":{"session_id":"s1","turn_id":"t1"}});
    cleanup_session(
        &mut native_transport,
        false,
        Vec::new(),
        Some(meta),
        &ToolPolicy::default(),
        "Stop",
    )
    .await
    .unwrap();
    responder.await.unwrap();
}

#[tokio::test]
async fn relay_confirms_turn_end_only_on_success_and_for_the_same_execution() {
    for (response, new_execution) in [
        (
            json!({"error":{"code":-32603,"message":"hook rejected","data":{"retryable":false}}}),
            false,
        ),
        (json!({"result":{"isError":true,"content":[]}}), false),
        (json!({"result":{"content":[]}}), true),
    ] {
        let (host_client, host_bridge) = tokio::io::duplex(32768);
        let (native_bridge, native_server) = tokio::io::duplex(32768);
        let (hr, hw) = tokio::io::split(host_bridge);
        let (nr, nw) = tokio::io::split(native_bridge);
        let task = tokio::spawn(async move {
            relay(
                &mut JsonTransport::new(hr, hw),
                &mut JsonTransport::new(nr, nw),
                std::future::pending(),
                &ToolPolicy::default(),
            )
            .await
        });
        let (cr, cw) = tokio::io::split(host_client);
        let mut client = JsonTransport::new(cr, cw);
        let (sr, sw) = tokio::io::split(native_server);
        let mut server = JsonTransport::new(sr, sw);
        send(&mut client, json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"js","arguments":{"code":"1"}}})).await;
        let first = receive(&mut server).await;
        send(
            &mut server,
            json!({"jsonrpc":"2.0","id":1,"result":{"content":[]}}),
        )
        .await;
        receive(&mut client).await;
        let context = first["params"]["_meta"]["x-codex-turn-metadata"].clone();
        send(&mut client, json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"turn_ended","arguments":{"session_id":context["session_id"],"turn_id":context["turn_id"]}}})).await;
        receive(&mut server).await;
        let execution = if new_execution {
            // A later js in the same turn must survive an older hook's ack.
            send(&mut client, json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"js","arguments":{"code":"2"}}})).await;
            let later = receive(&mut server).await;
            assert_eq!(later["params"]["_meta"]["x-codex-turn-metadata"], context);
            send(
                &mut server,
                json!({"jsonrpc":"2.0","id":3,"result":{"content":[]}}),
            )
            .await;
            receive(&mut client).await;
            later
        } else {
            first
        };
        let mut response = response;
        response["jsonrpc"] = json!("2.0");
        response["id"] = json!(2);
        send(&mut server, response.clone()).await;
        assert_eq!(receive(&mut client).await, response);
        drop(client);
        let cleanup = receive(&mut server).await;
        assert_eq!(cleanup["params"]["name"], "turn_ended");
        assert_eq!(
            cleanup["params"]["arguments"]["turn_id"],
            context["turn_id"]
        );
        assert_cleanup_metadata(&cleanup["params"]["_meta"], &execution["params"]["_meta"]);
        send(
            &mut server,
            json!({"jsonrpc":"2.0","id":cleanup["id"],"result":{"content":[]}}),
        )
        .await;
        tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }
}

#[tokio::test]
async fn relay_does_not_replay_an_unanswered_explicit_turn_end() {
    let (host_client, host_bridge) = tokio::io::duplex(32768);
    let (native_bridge, native_server) = tokio::io::duplex(32768);
    let (hr, hw) = tokio::io::split(host_bridge);
    let (nr, nw) = tokio::io::split(native_bridge);
    let task = tokio::spawn(async move {
        relay(
            &mut JsonTransport::new(hr, hw),
            &mut JsonTransport::new(nr, nw),
            std::future::pending(),
            &ToolPolicy::default(),
        )
        .await
    });
    let (cr, cw) = tokio::io::split(host_client);
    let mut client = JsonTransport::new(cr, cw);
    let (sr, sw) = tokio::io::split(native_server);
    let mut server = JsonTransport::new(sr, sw);
    send(
        &mut client,
        json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"js","arguments":{}}}),
    )
    .await;
    receive(&mut server).await;
    send(
        &mut server,
        json!({"jsonrpc":"2.0","id":1,"result":{"content":[]}}),
    )
    .await;
    receive(&mut client).await;
    send(&mut client, json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"turn_ended","arguments":{}}})).await;
    receive(&mut server).await;
    send(&mut client, json!({"jsonrpc":"2.0","id":2,"method":"ping"})).await;
    assert_eq!(receive(&mut client).await["error"]["code"], -32600);
    // Later work in the same turn does not make an unanswered hook safe to replay.
    send(
        &mut client,
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"js","arguments":{}}}),
    )
    .await;
    receive(&mut server).await;
    send(
        &mut server,
        json!({"jsonrpc":"2.0","id":3,"result":{"content":[]}}),
    )
    .await;
    receive(&mut client).await;
    drop(client);
    let cancel = receive(&mut server).await;
    assert_eq!(cancel["method"], "notifications/cancelled");
    assert_eq!(cancel["params"]["requestId"], 2);
    assert!(
        tokio::time::timeout(Duration::from_secs(2), server.receive())
            .await
            .unwrap()
            .unwrap()
            .is_none()
    );
    let error = tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap_err();
    assert!(error
        .to_string()
        .contains("turn_ended outcome unknown and not replayed"));
}

#[tokio::test]
async fn relay_rotates_default_turn_after_explicit_host_turn_ended() {
    let (host_client, host_bridge) = tokio::io::duplex(32768);
    let (native_bridge, native_server) = tokio::io::duplex(32768);
    let (hr, hw) = tokio::io::split(host_bridge);
    let (nr, nw) = tokio::io::split(native_bridge);
    let mut host_transport = JsonTransport::new(hr, hw);
    let mut native_transport = JsonTransport::new(nr, nw);
    let relay_task = tokio::spawn(async move {
        relay(
            &mut host_transport,
            &mut native_transport,
            std::future::pending::<()>(),
            &ToolPolicy::default(),
        )
        .await
    });

    let (hcr, hcw) = tokio::io::split(host_client);
    let mut client = JsonTransport::new(hcr, hcw);
    let (nsr, nsw) = tokio::io::split(native_server);
    let mut server = JsonTransport::new(nsr, nsw);

    send(
        &mut client,
        json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"js","arguments":{}}}),
    )
    .await;
    let first = receive(&mut server).await;
    let first_context = first["params"]["_meta"]["x-codex-turn-metadata"].clone();
    send(
        &mut server,
        json!({"jsonrpc":"2.0","id":1,"result":{"content":[]}}),
    )
    .await;
    let _ = receive(&mut client).await;

    send(
        &mut client,
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"turn_ended","arguments":{"hook_event_name":"Stop","session_id":first_context["session_id"],"turn_id":first_context["turn_id"]}}}),
    )
    .await;
    assert_eq!(receive(&mut server).await["params"]["name"], "turn_ended");
    send(
        &mut server,
        json!({"jsonrpc":"2.0","id":2,"result":{"content":[]}}),
    )
    .await;
    let _ = receive(&mut client).await;

    send(
        &mut client,
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"js","arguments":{}}}),
    )
    .await;
    let second = receive(&mut server).await;
    let second_context = second["params"]["_meta"]["x-codex-turn-metadata"].clone();
    assert_eq!(second_context["session_id"], first_context["session_id"]);
    assert_ne!(second_context["turn_id"], first_context["turn_id"]);
    send(
        &mut server,
        json!({"jsonrpc":"2.0","id":3,"result":{"content":[]}}),
    )
    .await;
    let _ = receive(&mut client).await;

    drop(client);
    let cleanup = receive(&mut server).await;
    assert_eq!(cleanup["params"]["name"], "turn_ended");
    assert_eq!(
        cleanup["params"]["arguments"]["turn_id"],
        second_context["turn_id"]
    );
    send(
        &mut server,
        json!({"jsonrpc":"2.0","id":cleanup["id"],"result":{"content":[]}}),
    )
    .await;
    tokio::time::timeout(Duration::from_secs(2), relay_task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

#[test]
fn prepare_tool_result_omits_inline_images_but_keeps_artifacts() {
    let directory = tempfile::tempdir().unwrap();
    let mut value = json!({"content":[
        {"type":"text","text":"before"},
        {"type":"image","mimeType":"image/png","data":"aGVsbG8=","_meta":{"screenshotId":"first"}}
    ]});
    prepare_tool_result(&mut value, Some(directory.path()), true).unwrap();
    let artifacts = value["bridge_artifacts"].as_array().unwrap();
    assert_eq!(artifacts.len(), 1);
    assert_eq!(artifacts[0]["mimeType"], "image/png");
    assert_eq!(artifacts[0]["bytes"], 5);
    assert!(artifacts[0]["path"].as_str().unwrap().ends_with(".png"));
    assert!(value["content"][1].get("data").is_none());
    assert_eq!(value["content"][1]["mimeType"], "image/png");
    assert_eq!(value["content"][1]["_meta"]["screenshotId"], "first");
    assert_eq!(value["content"][0]["text"], "before");

    let mut inline = json!({"content":[{"type":"image","mimeType":"image/png","data":"aGVsbG8="}]});
    prepare_tool_result(&mut inline, None, false).unwrap();
    assert_eq!(inline["content"][0]["data"], "aGVsbG8=");
}

#[tokio::test]
async fn relay_rejects_requests_beyond_the_active_request_limit() {
    let (host_client, host_bridge) = tokio::io::duplex(1024 * 1024);
    let (native_bridge, native_server) = tokio::io::duplex(32768);
    let (hr, hw) = tokio::io::split(host_bridge);
    let (nr, nw) = tokio::io::split(native_bridge);
    let relay_task = tokio::spawn(async move {
        relay(
            &mut JsonTransport::new(hr, hw),
            &mut JsonTransport::new(nr, nw),
            std::future::pending::<()>(),
            &ToolPolicy::default(),
        )
        .await
    });
    let (nsr, nsw) = tokio::io::split(native_server);
    let mut server = JsonTransport::new(nsr, nsw);
    let drain = tokio::spawn(async move { while let Ok(Some(_)) = server.receive().await {} });

    let (hcr, hcw) = tokio::io::split(host_client);
    let mut client = JsonTransport::new(hcr, hcw);
    for id in 0..MAX_ACTIVE_REQUESTS {
        send(
            &mut client,
            json!({"jsonrpc":"2.0","id":id,"method":"tools/call",
                "params":{"name":"js","arguments":{}}}),
        )
        .await;
    }
    send(
        &mut client,
        json!({"jsonrpc":"2.0","id":"overflow","method":"tools/call",
            "params":{"name":"js","arguments":{}}}),
    )
    .await;
    let error = tokio::time::timeout(Duration::from_secs(10), client.receive())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(error["id"], "overflow");
    assert_eq!(error["error"]["code"], -32603);
    assert!(error["error"]["message"]
        .as_str()
        .unwrap()
        .contains("active request limit"));

    relay_task.abort();
    let _ = relay_task.await;
    drain.abort();
    let _ = drain.await;
}
