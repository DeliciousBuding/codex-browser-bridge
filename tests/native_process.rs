use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};

struct Fixture {
    root: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let python = if cfg!(windows) { "python" } else { "python3" };
        let output = Command::new(python)
            .args(["-c", "import sys; print(sys.executable)"])
            .output()
            .unwrap();
        assert!(output.status.success());
        let python = String::from_utf8(output.stdout).unwrap().trim().to_string();
        std::fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/native_mcp.py"),
            root.path().join("native_mcp.py"),
        )
        .unwrap();
        let config = json!({"mcpServers":{"cua_repl":{
            "command":python,"args":["native_mcp.py"],"cwd":root.path(),
            "env":{"CUA_REPL_NODE_REPL_PATH":python,"BRIDGE_NATIVE_FIXTURE_ROOT":root.path()}
        }}});
        std::fs::write(root.path().join("config.json"), config.to_string()).unwrap();
        Self { root }
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_codex-browser-bridge"));
        command
            .args(["--surface", "computer", "--native-config"])
            .arg(self.root.path().join("config.json"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        command
    }

    fn assert_processes_exited(&self) {
        let pids: Vec<u32> =
            serde_json::from_slice(&std::fs::read(self.root.path().join("pids.json")).unwrap())
                .unwrap();
        let end = Instant::now() + Duration::from_secs(3);
        while pids.iter().any(|pid| process_alive(*pid)) && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(
            pids.iter().all(|pid| !process_alive(*pid)),
            "native process tree survived"
        );
    }

    fn frames(&self) -> Vec<Value> {
        std::fs::read_to_string(self.root.path().join("frames.jsonl"))
            .unwrap_or_default()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    #[cfg(unix)]
    fn wait_for_frame(&self, predicate: impl Fn(&Value) -> bool) -> Value {
        let end = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(frame) = self.frames().into_iter().find(&predicate) {
                return frame;
            }
            assert!(
                Instant::now() < end,
                "native fixture did not receive request"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::write(self.root.path().join("release"), b"");
        if let Ok(bytes) = std::fs::read(self.root.path().join("pids.json")) {
            if let Ok(pids) = serde_json::from_slice::<Vec<u32>>(&bytes) {
                let end = Instant::now() + Duration::from_secs(3);
                while pids.iter().any(|pid| process_alive(*pid)) && Instant::now() < end {
                    std::thread::sleep(Duration::from_millis(20));
                }
            }
        }
    }
}

#[cfg(unix)]
fn process_alive(pid: u32) -> bool {
    // A reparented zombie has exited and cannot perform input.
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap_or_default();
    if stat
        .rsplit_once(')')
        .is_some_and(|(_, tail)| tail.trim_start().starts_with('Z'))
    {
        return false;
    }
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

#[cfg(windows)]
fn process_alive(pid: u32) -> bool {
    use windows::Win32::{
        Foundation::CloseHandle,
        System::Threading::{GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
    };
    unsafe {
        let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return false;
        };
        let mut code = 0;
        let running = GetExitCodeProcess(handle, &mut code).is_ok() && code == 259;
        let _ = CloseHandle(handle);
        running
    }
}

fn wait(child: &mut std::process::Child, timeout: Duration) -> std::process::ExitStatus {
    let end = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status;
        }
        if Instant::now() >= end {
            let _ = child.kill();
            let _ = child.wait();
            panic!("adapter did not exit within {timeout:?}");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn native_mcp_eof_keeps_actual_context_and_terminates_the_owned_tree() {
    mcp_eof(false);
}

#[test]
fn native_mcp_eof_terminates_descendants_after_launcher_exit() {
    mcp_eof(true);
}

fn mcp_eof(launcher_exits: bool) {
    let fixture = Fixture::new();
    let mut command = fixture.command();
    command.args(["--mode", "mcp"]);
    if launcher_exits {
        command.env("BRIDGE_NATIVE_FIXTURE_EXIT_ON_EOF", "1");
    }
    let mut child = command.spawn().unwrap();
    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            if tx
                .send(serde_json::from_str::<Value>(&line.unwrap()).unwrap())
                .is_err()
            {
                break;
            }
        }
    });
    let mut input = child.stdin.take().unwrap();
    let mut send = |row: Value| {
        writeln!(input, "{row}").unwrap();
        input.flush().unwrap();
    };
    send(
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
            "protocolVersion":"2025-03-26","capabilities":{},
            "clientInfo":{"name":"synthetic-host","version":"1"}
        }}),
    );
    assert_eq!(rx.recv_timeout(Duration::from_secs(3)).unwrap()["id"], 1);
    send(json!({"jsonrpc":"2.0","method":"notifications/initialized"}));
    let meta = json!({"sessionId":"external-session","threadId":"external-thread",
        "x-codex-turn-metadata":{"session_id":"actual-session","turn_id":"actual-turn"},
        "callId":"external-call","progressToken":"external-progress","opaque":{"keep":true}});
    send(json!({"jsonrpc":"2.0","id":2,"method":"tools/call",
        "params":{"name":"js","arguments":{"code":"synthetic"},"_meta":meta}}));
    assert_eq!(rx.recv_timeout(Duration::from_secs(3)).unwrap()["id"], 2);
    send(json!({"jsonrpc":"2.0","id":3,"method":"tools/list","params":{}}));
    assert_eq!(rx.recv_timeout(Duration::from_secs(3)).unwrap()["id"], 3);
    drop(input);
    assert!(wait(&mut child, Duration::from_secs(12)).success());
    reader.join().unwrap();
    fixture.assert_processes_exited();
    let frames = fixture.frames();
    let cleanup = frames.last().unwrap();
    assert_eq!(cleanup["params"]["name"], "turn_ended");
    assert_eq!(
        cleanup["params"]["_meta"]["x-codex-turn-metadata"],
        meta["x-codex-turn-metadata"]
    );
    assert_eq!(cleanup["params"]["_meta"]["opaque"], meta["opaque"]);
    assert!(cleanup["params"]["_meta"].get("progressToken").is_none());
    assert_ne!(cleanup["params"]["_meta"]["callId"], meta["callId"]);
    assert_eq!(
        cleanup["params"]["arguments"]["session_id"],
        "actual-session"
    );
    assert_eq!(cleanup["params"]["arguments"]["turn_id"], "actual-turn");
    assert_eq!(
        std::fs::read_to_string(fixture.root.path().join("cwd.txt")).unwrap(),
        fixture.root.path().to_string_lossy()
    );
}

#[test]
fn native_doctor_catalog_timeout_exits_and_terminates_the_owned_tree() {
    let fixture = Fixture::new();
    let mut child = fixture
        .command()
        .args(["--mode", "doctor"])
        .env("BRIDGE_NATIVE_FIXTURE_HANG_CATALOG", "1")
        .spawn()
        .unwrap();
    assert!(!wait(&mut child, Duration::from_secs(24)).success());
    fixture.assert_processes_exited();
}

#[cfg(unix)]
#[test]
fn native_cli_sigterm_cancels_active_call_before_interrupt_cleanup() {
    let fixture = Fixture::new();
    let mut child = fixture
        .command()
        .args(["call", "js", r#"{"code":"synthetic pending input"}"#])
        .env("BRIDGE_NATIVE_FIXTURE_HANG_JS", "1")
        .spawn()
        .unwrap();
    let call = fixture
        .wait_for_frame(|row| row["method"] == "tools/call" && row["params"]["name"] == "js");
    assert_eq!(unsafe { libc::kill(child.id() as i32, libc::SIGTERM) }, 0);
    assert!(!wait(&mut child, Duration::from_secs(15)).success());
    fixture.assert_processes_exited();
    let frames = fixture.frames();
    let cancel_index = frames
        .iter()
        .position(|row| row["method"] == "notifications/cancelled")
        .expect("matching cancellation missing");
    assert_eq!(frames[cancel_index]["params"]["requestId"], call["id"]);
    let cleanup = &frames[cancel_index + 1];
    assert_eq!(cleanup["params"]["name"], "turn_ended");
    assert_eq!(
        cleanup["params"]["arguments"]["hook_event_name"],
        "Interrupt"
    );
    assert_eq!(
        cleanup["params"]["_meta"]["x-codex-turn-metadata"],
        call["params"]["_meta"]["x-codex-turn-metadata"]
    );
    assert_ne!(
        cleanup["params"]["_meta"]["callId"],
        call["params"]["_meta"]["callId"]
    );
    assert_eq!(
        frames
            .iter()
            .filter(|row| row["method"] == "tools/call" && row["params"]["name"] == "js")
            .count(),
        1,
        "interrupted input was replayed"
    );
}

#[test]
fn native_rejects_browser_options_and_mode_command_conflicts_before_startup() {
    let fixture = Fixture::new();
    for arguments in [
        vec!["--mode", "doctor", "tools"],
        vec!["--pipe", "unused"],
        vec!["--profile", "full"],
        vec!["--browser-family", "chrome"],
        vec!["--max-text-bytes", "1234"],
        vec!["--max-output-bytes", "16384"],
        vec!["--network-policy", "{}"],
        vec!["--finalize", "call", "js", "{}"],
    ] {
        let output = fixture.command().args(arguments).output().unwrap();
        assert!(!output.status.success());
        assert!(!fixture.root.path().join("pids.json").exists());
    }
}

#[cfg(unix)]
#[test]
fn native_cli_sigterm_during_startup_terminates_tree_without_inventing_a_turn() {
    let fixture = Fixture::new();
    let mut child = fixture
        .command()
        .args(["tools"])
        .env("BRIDGE_NATIVE_FIXTURE_HANG_INITIALIZE", "1")
        .spawn()
        .unwrap();
    fixture.wait_for_frame(|row| row["method"] == "initialize");
    assert_eq!(unsafe { libc::kill(child.id() as i32, libc::SIGTERM) }, 0);
    assert!(!wait(&mut child, Duration::from_secs(15)).success());
    fixture.assert_processes_exited();
    assert!(
        fixture
            .frames()
            .iter()
            .all(|row| row["method"] != "tools/call"),
        "startup interruption invented an executing turn"
    );
}
