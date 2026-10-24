use assert_cmd::Command;
use codex_browser_bridge::{protocol, registry::BrowserSelector, session::*};
use serde_json::{json, Value};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::time::timeout;

const WAIT: Duration = Duration::from_secs(5);

fn identity() -> BrowserIdentity {
    BrowserIdentity {
        family: "edge".into(),
        instance_id: "instance-a".into(),
    }
}

fn command(root: &std::path::Path) -> Command {
    let mut command = Command::cargo_bin("codex-browser-bridge").unwrap();
    command
        .current_dir(root)
        .env_remove("CODEX_BRIDGE_CONFIG")
        .env_remove("CODEX_BRIDGE_BROWSER_FAMILY")
        .env_remove("CODEX_BRIDGE_BROWSER_INSTANCE")
        .env_remove("CODEX_BRIDGE_PROFILE")
        .timeout(WAIT);
    command
}

#[test]
fn continuation_lock_and_selector_conflict_fail_before_browser_access() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session.json");
    save_session(&path, identity(), "saved-session", "saved-turn").unwrap();
    let held = SessionLock::acquire(&path).unwrap();
    command(dir.path())
        .args([
            "--session-file",
            path.to_str().unwrap(),
            "call",
            "codex_get_info",
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains("already in use"));
    drop(held);
    command(dir.path())
        .args([
            "--browser-instance",
            "instance-b",
            "--session-file",
            path.to_str().unwrap(),
            "call",
            "codex_get_info",
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains("conflicts"));
    assert_eq!(load_session(&path).unwrap().unwrap().turn_id, "saved-turn");
    assert!(SessionLock::acquire(&path).is_ok());
    assert!(identity().selector(&BrowserSelector::default()).is_ok());
}

#[test]
fn killed_process_releases_os_lock_and_preserves_continuation() {
    use std::process::{Command as Process, Stdio};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session.json");
    save_session(&path, identity(), "saved-session", "saved-turn").unwrap();
    let mut child = Process::new(assert_cmd::cargo::cargo_bin("codex-browser-bridge"))
        .current_dir(dir.path())
        .env_remove("CODEX_BRIDGE_CONFIG")
        .env_remove("CODEX_BRIDGE_BROWSER_FAMILY")
        .env_remove("CODEX_BRIDGE_BROWSER_INSTANCE")
        .env_remove("CODEX_BRIDGE_PROFILE")
        .args(["--session-file", path.to_str().unwrap(), "batch"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + WAIT;
    loop {
        if SessionLock::acquire(&path).is_err() {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "child did not acquire lock"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(SessionLock::acquire(&path).is_ok());
    assert_eq!(load_session(&path).unwrap().unwrap().turn_id, "saved-turn");
}

async fn read<S: AsyncRead + Unpin>(stream: &mut S) -> Value {
    serde_json::from_slice(
        &timeout(WAIT, protocol::decode_frame(stream))
            .await
            .unwrap()
            .unwrap(),
    )
    .unwrap()
}

async fn reply<S: AsyncWrite + Unpin>(stream: &mut S, req: &Value, result: Value) {
    timeout(
        WAIT,
        protocol::encode_frame(stream, &json!({"id":req["id"], "result":result})),
    )
    .await
    .unwrap()
    .unwrap();
}

async fn endpoint(
    root: &std::path::Path,
    changed: bool,
) -> (String, tokio::task::JoinHandle<Vec<Value>>) {
    #[cfg(windows)]
    let (name, accept) = {
        let name = format!(r"\\.\pipe\bridge-session-test-{}", uuid::Uuid::new_v4());
        let server = tokio::net::windows::named_pipe::ServerOptions::new()
            .create(&name)
            .unwrap();
        (name, async move {
            server.connect().await.unwrap();
            server
        })
    };
    #[cfg(unix)]
    let (name, accept) = {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(root, std::fs::Permissions::from_mode(0o700)).unwrap();
        let path = root.join("extension.sock");
        let server = tokio::net::UnixListener::bind(&path).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        (path.display().to_string(), async move {
            server.accept().await.unwrap().0
        })
    };
    #[cfg(windows)]
    let _ = root;
    let task = tokio::spawn(async move {
        let mut stream = timeout(WAIT, accept).await.unwrap();
        let first = read(&mut stream).await;
        assert_eq!(first["method"], "getInfo");
        assert_ne!(first["params"]["session_id"], "saved-session");
        let info = json!({"type":"extension","family":"edge","metadata":{"extensionInstanceId":if changed {"instance-b"} else {"instance-a"}}});
        reply(&mut stream, &first, info.clone()).await;
        if changed {
            // The identity check must drop this endpoint before any saved IDs.
            assert!(timeout(WAIT, protocol::decode_frame(&mut stream))
                .await
                .unwrap()
                .is_err());
            return vec![first];
        }
        let health = read(&mut stream).await;
        assert_eq!(health["params"]["session_id"], "saved-session");
        reply(&mut stream, &health, info.clone()).await;
        let request = read(&mut stream).await;
        assert_eq!(request["params"]["turn_id"], "saved-turn");
        reply(&mut stream, &request, info).await;
        vec![first, health, request]
    });
    (name, task)
}

#[tokio::test]
async fn explicit_endpoint_resume_verifies_identity_before_saved_ids() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session.json");
    save_session(&path, identity(), "saved-session", "saved-turn").unwrap();
    let (name, task) = endpoint(dir.path(), false).await;
    let root = dir.path().to_path_buf();
    let output = tokio::task::spawn_blocking(move || {
        command(&root)
            .args([
                "--pipe",
                &name,
                "--session-file",
                path.to_str().unwrap(),
                "call",
                "codex_get_info",
            ])
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(timeout(WAIT, task).await.unwrap().unwrap().len(), 3);
}

#[tokio::test]
async fn changed_endpoint_never_receives_saved_session_or_mutation() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session.json");
    save_session(&path, identity(), "saved-session", "saved-turn").unwrap();
    let (name, task) = endpoint(dir.path(), true).await;
    let root = dir.path().to_path_buf();
    let path_for_command = path.clone();
    let output = tokio::task::spawn_blocking(move || {
        command(&root)
            .args([
                "--pipe",
                &name,
                "--session-file",
                path_for_command.to_str().unwrap(),
                "call",
                "codex_create_tab",
            ])
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    assert!(!output.status.success());
    assert_eq!(timeout(WAIT, task).await.unwrap().unwrap().len(), 1);
    assert_eq!(load_session(&path).unwrap().unwrap().turn_id, "saved-turn");
}
