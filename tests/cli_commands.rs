use assert_cmd::Command;
use codex_browser_bridge::protocol;
use serde_json::Value;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite};

fn bridge() -> Command {
    let mut command = Command::cargo_bin("codex-browser-bridge").unwrap();
    command
        .env_remove("CODEX_BRIDGE_PROFILE")
        .env_remove("CODEX_BRIDGE_BROWSER_FAMILY")
        .env_remove("CODEX_BRIDGE_BROWSER_INSTANCE")
        .env("CODEX_BRIDGE_CONFIG", "nonexistent-cli-test-config.toml")
        // Keep the suite hermetic: a developer machine may have a live extension,
        // and these tests must never create or close real browser tabs.
        .env(
            "CODEX_BRIDGE_SOCKET_DIR",
            "/nonexistent-codex-browser-bridge-test-sockets",
        )
        .timeout(Duration::from_secs(5));
    command
}

#[derive(Debug)]
struct MockSession {
    session_id: String,
    turn_id: String,
}

async fn serve_mock_extension<S>(
    mut stream: S,
    session_tx: tokio::sync::oneshot::Sender<MockSession>,
) where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut session_tx = Some(session_tx);
    loop {
        let frame = match protocol::decode_frame(&mut stream).await {
            Ok(frame) => frame,
            Err(_) => return,
        };
        let request: Value = serde_json::from_slice(&frame).unwrap();
        let id = request["id"].clone();
        let method = request["method"].as_str().unwrap();
        let response = match method {
            "getInfo" => serde_json::json!({
                "id": id,
                "result": {"type": "extension", "family": "chrome"}
            }),
            "createTab" => serde_json::json!({
                "id": id,
                "result": {"id": 42, "url": "about:blank", "title": ""}
            }),
            _ => serde_json::json!({"id": id, "result": {}}),
        };
        if method == "createTab" {
            let session = MockSession {
                session_id: request["params"]["session_id"].as_str().unwrap().into(),
                turn_id: request["params"]["turn_id"].as_str().unwrap().into(),
            };
            session_tx.take().unwrap().send(session).unwrap();
        }
        protocol::encode_frame(&mut stream, &response)
            .await
            .unwrap();
    }
}

#[cfg(windows)]
async fn start_mock_extension(
    endpoint: String,
) -> (
    tokio::task::JoinHandle<()>,
    tokio::sync::oneshot::Receiver<MockSession>,
) {
    use tokio::net::windows::named_pipe::ServerOptions;

    let (session_tx, session_rx) = tokio::sync::oneshot::channel();
    let server = ServerOptions::new().create(&endpoint).unwrap();
    let handle = tokio::spawn(async move {
        server.connect().await.unwrap();
        serve_mock_extension(server, session_tx).await
    });
    (handle, session_rx)
}

#[cfg(unix)]
async fn start_mock_extension(
    endpoint: String,
) -> (
    tokio::task::JoinHandle<()>,
    tokio::sync::oneshot::Receiver<MockSession>,
) {
    let (session_tx, session_rx) = tokio::sync::oneshot::channel();
    let listener = tokio::net::UnixListener::bind(&endpoint).unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&endpoint, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    let handle = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        serve_mock_extension(stream, session_tx).await
    });
    (handle, session_rx)
}

#[test]
fn tools_lists_names_without_connecting_or_dumping_schemas() {
    let output = bridge()
        .arg("tools")
        .assert()
        .success()
        .get_output()
        .clone();
    assert!(output.stderr.is_empty());
    let names: Vec<String> = serde_json::from_slice(&output.stdout).unwrap();
    assert!(names.iter().any(|name| name == "codex_list_tabs"));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("inputSchema"));
    assert_eq!(
        output.stdout.iter().filter(|byte| **byte == b'\n').count(),
        1
    );
}

#[test]
fn tools_exposes_exactly_one_registered_schema_and_respects_profile() {
    let output = bridge()
        .args(["tools", "codex_locator"])
        .assert()
        .success()
        .get_output()
        .clone();
    let tool: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(tool["name"], "codex_locator");
    assert!(tool["inputSchema"].is_object());
    let output = bridge()
        .args(["--profile", "basic", "tools", "codex_execute_cdp"])
        .assert()
        .failure()
        .get_output()
        .clone();
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("active profile"));
}

#[test]
fn call_reuses_mcp_parameter_validation_without_browser_connection() {
    let output = bridge()
        .args(["call", "codex_locator"])
        .assert()
        .failure()
        .get_output()
        .clone();
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["isError"], true);
    assert!(value.get("jsonrpc").is_none());
    assert!(String::from_utf8_lossy(&output.stderr).contains("tool call failed"));
}

#[test]
fn unknown_tool_is_structured_error_and_nonzero_exit() {
    let output = bridge()
        .args(["call", "not_a_tool"])
        .assert()
        .failure()
        .get_output()
        .clone();
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["error"]["code"], -32601);
}

#[test]
fn invalid_arguments_stay_on_stderr() {
    for arguments in ["[]", "null", "not JSON"] {
        let output = bridge()
            .args(["call", "codex_list_tabs", arguments])
            .assert()
            .failure()
            .get_output()
            .clone();
        assert!(output.stdout.is_empty());
        assert!(!output.stderr.is_empty());
    }
}

#[test]
fn batch_is_fail_fast_and_blank_input_is_noop() {
    let output = bridge()
        .arg("batch")
        .write_stdin("\n{\"name\":\"not_a_tool\"}\n{\"name\":\"another_unknown_tool\"}\n")
        .assert()
        .failure()
        .get_output()
        .clone();
    assert_eq!(
        output.stdout.iter().filter(|byte| **byte == b'\n').count(),
        1
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("line 2"));
    bridge()
        .arg("batch")
        .write_stdin("\n \n")
        .assert()
        .success()
        .stdout("");
}

#[test]
fn batch_rejects_invalid_input_before_browser_connection() {
    for input in [
        "not JSON",
        "{\"name\":\"codex_list_tabs\",\"arguments\":[]}",
        "{\"name\":\"codex_list_tabs\",\"unexpected\":true}",
    ] {
        let output = bridge()
            .arg("batch")
            .write_stdin(input)
            .assert()
            .failure()
            .get_output()
            .clone();
        assert!(output.stdout.is_empty());
        assert!(!output.stderr.is_empty());
    }
    let output = bridge()
        .arg("batch")
        .write_stdin("x".repeat(1024 * 1024 + 1))
        .assert()
        .failure()
        .get_output()
        .clone();
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("exceeds 1048576 bytes"));
}

#[test]
fn output_limit_applies_to_tools_and_is_valid_json() {
    let output = bridge()
        .args(["tools", "--max-output-bytes", "256"])
        .assert()
        .success()
        .get_output()
        .clone();
    assert!(output.stdout.len() <= 256);
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["truncated"], true);
    bridge()
        .args(["tools", "--max-output-bytes", "1"])
        .assert()
        .code(2);
}

#[test]
fn help_keeps_legacy_mode_and_documents_session_cleanup() {
    let output = bridge()
        .arg("--help")
        .assert()
        .success()
        .get_output()
        .clone();
    let help = String::from_utf8(output.stdout).unwrap();
    assert!(help.contains("--mode"));
    assert!(help.contains("batch"));
    let output = bridge()
        .args(["batch", "--help"])
        .assert()
        .success()
        .get_output()
        .clone();
    let help = String::from_utf8(output.stdout).unwrap();
    assert!(help.contains("--session-file"));
    assert!(help.contains("--keep-turn"));
    assert!(help.contains("--finalize"));
}

#[test]
fn one_shot_tab_creation_requires_cleanup_or_a_session_file() {
    // No browser connection is attempted: the guard runs before any dial.
    let output = bridge()
        .args(["call", "codex_create_tab"])
        .assert()
        .failure()
        .get_output()
        .clone();
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--session-file"), "{stderr}");
    assert!(stderr.contains("--keep-turn"), "{stderr}");
}

#[tokio::test]
async fn tab_creation_with_a_session_file_persists_the_turn() {
    let dir = tempfile::tempdir().unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let session = dir.path().join("session.json");
    let session_for_command = session.clone();
    #[cfg(windows)]
    let endpoint = format!(r"\\.\pipe\codex-browser-bridge-test-{}", std::process::id());
    #[cfg(unix)]
    let endpoint = dir.path().join("mock-extension.sock").display().to_string();

    let (server, seen_rx) = start_mock_extension(endpoint.clone()).await;
    let output = tokio::task::spawn_blocking(move || {
        bridge()
            .args([
                "--pipe",
                &endpoint,
                "--session-file",
                session_for_command.to_str().unwrap(),
                "call",
                "codex_create_tab",
            ])
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["content"][0]["text"], "Created tab: 42", "{stdout}");
    assert!(!stderr.contains("--finalize"), "{stderr}");

    let seen = tokio::time::timeout(Duration::from_secs(5), seen_rx)
        .await
        .unwrap_or_else(|_| {
            panic!("mock did not observe createTab; stdout={stdout}; stderr={stderr}")
        })
        .unwrap();
    server.abort();

    let saved: Value = serde_json::from_slice(&std::fs::read(&session).unwrap()).unwrap();
    assert_eq!(saved["version"], 1);
    assert_eq!(saved["session_id"], seen.session_id);
    assert_eq!(saved["turn_id"], seen.turn_id);
}

#[test]
fn rejected_command_does_not_create_a_session_file() {
    let dir = tempfile::tempdir().unwrap();
    let session = dir.path().join("session.json");
    let output = bridge()
        .args([
            "--session-file",
            session.to_str().unwrap(),
            "call",
            "codex_list_tabs",
            "[]",
        ])
        .assert()
        .failure()
        .get_output()
        .clone();

    assert!(output.stdout.is_empty());
    assert!(
        !session.exists(),
        "a rejected command created a session file"
    );
}

#[test]
fn rejected_finalize_preserves_an_existing_session_file() {
    let dir = tempfile::tempdir().unwrap();
    let session = dir.path().join("session.json");
    let original = br#"{"version":1,"session_id":"existing-session","turn_id":"existing-turn"}"#;
    std::fs::write(&session, original).unwrap();

    let output = bridge()
        .args([
            "--session-file",
            session.to_str().unwrap(),
            "--finalize",
            "call",
            "codex_list_tabs",
            "[]",
        ])
        .assert()
        .failure()
        .get_output()
        .clone();

    assert!(output.stdout.is_empty());
    assert_eq!(std::fs::read(&session).unwrap(), original);
}

#[tokio::test]
async fn session_file_write_failure_is_observable() {
    let dir = tempfile::tempdir().unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let session = dir.path().join("missing").join("session.json");
    #[cfg(windows)]
    let endpoint = format!(
        r"\\.\pipe\codex-browser-bridge-write-failure-test-{}",
        std::process::id()
    );
    #[cfg(unix)]
    let endpoint = dir.path().join("mock-extension.sock").display().to_string();

    let (server, _seen_rx) = start_mock_extension(endpoint.clone()).await;
    let output = tokio::task::spawn_blocking(move || {
        bridge()
            .args([
                "--pipe",
                &endpoint,
                "--session-file",
                session.to_str().unwrap(),
                "call",
                "codex_create_tab",
            ])
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    server.abort();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(
        stderr.contains("could not persist browser session file"),
        "{stderr}"
    );
}

#[test]
fn environment_profile_overrides_the_config_file() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.toml");
    std::fs::write(&config, "profile = \"basic\"\n").unwrap();

    let output = bridge()
        .env("CODEX_BRIDGE_CONFIG", &config)
        .env("CODEX_BRIDGE_PROFILE", "full")
        .args(["tools", "codex_execute_cdp"])
        .assert()
        .success()
        .get_output()
        .clone();
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["name"], "codex_execute_cdp");
}

#[test]
fn lifecycle_flags_require_a_call_or_batch_command() {
    for flags in [["--finalize"].as_slice(), ["--keep-turn"].as_slice()] {
        let output = bridge()
            .args(flags)
            .arg("tools")
            .assert()
            .failure()
            .get_output()
            .clone();
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("require a call or batch"),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let dir = tempfile::tempdir().unwrap();
    let session = dir.path().join("session.json");
    let output = bridge()
        .args(["--session-file", session.to_str().unwrap(), "--mode", "mcp"])
        .write_stdin("")
        .assert()
        .failure()
        .get_output()
        .clone();
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("require a call or batch"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn finalize_and_keep_turn_are_mutually_exclusive() {
    let output = bridge()
        .args(["--finalize", "--keep-turn", "batch"])
        .assert()
        .failure()
        .get_output()
        .clone();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("keep-turn"), "{stderr}");
}
