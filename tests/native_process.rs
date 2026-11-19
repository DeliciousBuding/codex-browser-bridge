use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Command, Stdio},
    sync::{mpsc, OnceLock},
    time::{Duration, Instant},
};

fn python_executable() -> &'static str {
    static PYTHON: OnceLock<String> = OnceLock::new();
    PYTHON.get_or_init(|| {
        let python = if cfg!(windows) { "python" } else { "python3" };
        let output = Command::new(python)
            .args(["-c", "import sys; print(sys.executable)"])
            .output()
            .unwrap();
        assert!(output.status.success());
        String::from_utf8(output.stdout).unwrap().trim().to_string()
    })
}

struct Fixture {
    root: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let python = python_executable();
        std::fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/native_mcp.py"),
            root.path().join("native_mcp.py"),
        )
        .unwrap();
        let config = json!({"mcpServers":{"cua_repl":{
            "command":if cfg!(windows){"python"}else{"python3"},
            "args":["native_mcp.py"],"cwd":root.path(),
            "startup_timeout_ms":5000,
            "env":{
                "CUA_REPL_NODE_REPL_PATH":python,
                "BRIDGE_NATIVE_FIXTURE_ROOT":root.path(),
                "BRIDGE_NATIVE_FIXTURE_LITERAL":"host-owned",
                "DISPLAY":":synthetic",
                "XAUTHORITY":"/synthetic/Xauthority",
                "WAYLAND_DISPLAY":"wayland-synthetic",
                "XDG_RUNTIME_DIR":"/synthetic/runtime",
                "DBUS_SESSION_BUS_ADDRESS":"unix:path=/synthetic/bus",
                "CUA_REPL_ENABLED_SURFACES":"browser,computer",
                "NODE_REPL_TRUSTED_SERVICES":json!({
                    "browser":"@oai/browser-desktop/service",
                    "sky":"@oai/sky/service"
                }).to_string(),
                "NODE_REPL_AUTH_TOKEN":"fixture-value"
            },
            "env_vars":[
                "BRIDGE_NATIVE_FIXTURE_FORWARDED",
                {"name":"BRIDGE_NATIVE_FIXTURE_LOCAL_FORWARDED","source":"local"},
                "BRIDGE_NATIVE_FIXTURE_HOLD_ON_EOF",
                "BRIDGE_NATIVE_FIXTURE_HANG_INITIALIZE",
                "BRIDGE_NATIVE_FIXTURE_HANG_CATALOG",
                "BRIDGE_NATIVE_FIXTURE_HANG_CLEANUP",
                "BRIDGE_NATIVE_FIXTURE_HANG_JS",
                "BRIDGE_NATIVE_FIXTURE_JS_ERROR",
                "BRIDGE_NATIVE_FIXTURE_PAGINATE_CATALOG",
                "BRIDGE_NATIVE_FIXTURE_LOOP_CATALOG",
                "BRIDGE_NATIVE_FIXTURE_IMAGE_RESULT",
                "NODE_REPL_AUTH_TOKEN"
            ],
            "enabled_tools":["js","js_reset","turn_ended"],
            "disabled_tools":["js_reset"]
        }}});
        std::fs::write(root.path().join("config.json"), config.to_string()).unwrap();
        Self { root }
    }

    fn command(&self) -> Command {
        self.command_for("computer")
    }

    fn command_for(&self, surface: &str) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_codex-browser-bridge"));
        command
            .args(["--surface", surface, "--native-config"])
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
    mcp_eof("computer", false);
}

#[test]
fn native_mcp_eof_terminates_descendants_after_launcher_exit() {
    mcp_eof("computer", true);
}

#[test]
fn native_browser_uses_node_repl_catalog_policy_and_cleanup() {
    mcp_eof("native-browser", true);
}

#[test]
fn native_browser_discovers_its_own_toml_launch_row() {
    let fixture = Fixture::new();
    let config_path = fixture.root.path().join("config.json");
    let mut config: Value = serde_json::from_slice(&std::fs::read(config_path).unwrap()).unwrap();
    let computer = &mut config["mcpServers"]["cua_repl"];
    let mut browser = computer.clone();
    browser["env"]
        .as_object_mut()
        .unwrap()
        .remove("CUA_REPL_NODE_REPL_PATH");
    computer["enabled"] = json!(false);
    let path = fixture.root.path().join("config.toml");
    let write_config = |servers: Value| {
        std::fs::write(
            &path,
            toml::to_string(&json!({"mcp_servers": servers})).unwrap(),
        )
        .unwrap();
    };
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_codex-browser-bridge"))
            .args(["--surface", "native-browser", "--mode", "doctor"])
            .env("CODEX_HOME", fixture.root.path())
            .output()
            .unwrap()
    };
    write_config(json!({"cua_repl": computer}));
    let missing = run();
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr)
        .contains("native node_repl launch configuration unavailable"));
    assert!(!fixture.root.path().join("pids.json").exists());

    write_config(json!({"cua_repl": computer, "node_repl": browser}));
    let doctor = run();
    assert!(
        doctor.status.success(),
        "{}",
        String::from_utf8_lossy(&doctor.stderr)
    );
    let report: Value = serde_json::from_slice(&doctor.stdout).unwrap();
    assert_eq!(report["tools"], json!(["js", "turn_ended"]));
    assert_eq!(report["cleanup_confirmed"], true);
    fixture.assert_processes_exited();
}

fn mcp_eof(surface: &str, launcher_exits: bool) {
    let fixture = Fixture::new();
    if surface == "native-browser" {
        let path = fixture.root.path().join("config.json");
        let mut config: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let mut row = config["mcpServers"]["cua_repl"].clone();
        row["env"]
            .as_object_mut()
            .unwrap()
            .remove("CUA_REPL_NODE_REPL_PATH");
        config["mcpServers"]["cua_repl"]["enabled"] = json!(false);
        config["mcpServers"]["node_repl"] = row;
        std::fs::write(path, config.to_string()).unwrap();
    }
    let mut command = fixture.command_for(surface);
    let diagnostic = fixture.root.path().join("stderr.log");
    command
        .args(["--mode", "mcp"])
        .env("RUST_LOG", "codex_browser_bridge::native=debug")
        .env("NO_COLOR", "1")
        .env("BRIDGE_NATIVE_FIXTURE_FORWARDED", "forwarded-name")
        .env("BRIDGE_NATIVE_FIXTURE_LOCAL_FORWARDED", "forwarded-local")
        .env("BRIDGE_NATIVE_FIXTURE_AMBIENT", "must-not-reach-native")
        .env("BRIDGE_NATIVE_FIXTURE_LITERAL", "ambient")
        .env("NODE_REPL_AUTH_TOKEN", format!("synthetic-{}", 1))
        .stderr(Stdio::from(std::fs::File::create(&diagnostic).unwrap()));
    if !launcher_exits {
        command.env("BRIDGE_NATIVE_FIXTURE_HOLD_ON_EOF", "1");
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
    let catalog = rx.recv_timeout(Duration::from_secs(3)).unwrap();
    assert_eq!(catalog["id"], 3);
    assert_eq!(
        catalog["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["js", "turn_ended"]
    );
    drop(input);
    assert!(wait(&mut child, Duration::from_secs(12)).success());
    reader.join().unwrap();
    fixture.assert_processes_exited();
    assert!(
        fixture.root.path().join("stdin-eof").exists(),
        "native stdin never received EOF"
    );
    let diagnostic = std::fs::read_to_string(diagnostic).unwrap();
    assert!(
        diagnostic.contains(if launcher_exits {
            "graceful=true"
        } else {
            "graceful=false"
        }),
        "launcher exit was not observed before terminating its descendant: {diagnostic}"
    );
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
    let environment: Value = serde_json::from_slice(
        &std::fs::read(fixture.root.path().join("environment.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        environment["BRIDGE_NATIVE_FIXTURE_FORWARDED"],
        "forwarded-name"
    );
    assert_eq!(
        environment["BRIDGE_NATIVE_FIXTURE_LOCAL_FORWARDED"],
        "forwarded-local"
    );
    assert_eq!(environment["BRIDGE_NATIVE_FIXTURE_LITERAL"], "host-owned");
    assert_eq!(environment["DISPLAY"], ":synthetic");
    assert_eq!(environment["XAUTHORITY"], "/synthetic/Xauthority");
    assert_eq!(environment["WAYLAND_DISPLAY"], "wayland-synthetic");
    assert_eq!(environment["XDG_RUNTIME_DIR"], "/synthetic/runtime");
    assert_eq!(
        environment["DBUS_SESSION_BUS_ADDRESS"],
        "unix:path=/synthetic/bus"
    );
    assert_eq!(environment["CUA_REPL_ENABLED_SURFACES"], "browser,computer");
    assert_eq!(
        environment["NODE_REPL_TRUSTED_SERVICES"],
        json!({
            "browser":"@oai/browser-desktop/service",
            "sky":"@oai/sky/service"
        })
        .to_string()
    );
    assert_eq!(environment["BRIDGE_NATIVE_FIXTURE_AMBIENT"], Value::Null);
    assert_eq!(environment["NODE_REPL_AUTH_TOKEN"], Value::Null);
    assert!(environment["PATH"]
        .as_str()
        .is_some_and(|value| !value.is_empty()));
    if cfg!(windows) {
        assert!(environment["PATHEXT"]
            .as_str()
            .is_some_and(|value| !value.is_empty()));
        assert!(environment["SYSTEMROOT"]
            .as_str()
            .is_some_and(|value| !value.is_empty()));
    }
}

#[test]
fn native_doctor_catalog_timeout_exits_and_terminates_the_owned_tree() {
    let fixture = Fixture::new();
    let log = fixture.root.path().join("catalog-timeout.stderr");
    let mut child = fixture
        .command()
        .args(["--mode", "doctor"])
        .env("BRIDGE_NATIVE_FIXTURE_HANG_CATALOG", "1")
        .stderr(Stdio::from(std::fs::File::create(&log).unwrap()))
        .spawn()
        .unwrap();
    assert!(!wait(&mut child, Duration::from_secs(25)).success());
    fixture.assert_processes_exited();
    assert!(fixture
        .frames()
        .iter()
        .any(|frame| frame["method"] == "tools/list"));
    let diagnostic = std::fs::read_to_string(log).unwrap();
    assert!(
        diagnostic.contains("native catalog pagination timed out"),
        "{diagnostic}"
    );
}

#[test]
fn native_doctor_rejects_repeated_catalog_cursor_and_cleans_up() {
    let fixture = Fixture::new();
    let output = fixture
        .command()
        .args(["--mode", "doctor"])
        .env("BRIDGE_NATIVE_FIXTURE_LOOP_CATALOG", "1")
        .stderr(Stdio::piped())
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("repeated pagination cursor"), "{stderr}");
    fixture.assert_processes_exited();
}

#[test]
fn native_cli_tools_rejects_repeated_catalog_cursor_and_cleans_up() {
    let fixture = Fixture::new();
    let output = fixture
        .command()
        .arg("tools")
        .env("BRIDGE_NATIVE_FIXTURE_LOOP_CATALOG", "1")
        .stderr(Stdio::piped())
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("repeated pagination cursor"), "{stderr}");
    fixture.assert_processes_exited();
}

#[test]
fn native_cli_rejects_disabled_tool_without_forwarding_it() {
    let fixture = Fixture::new();
    let output = fixture
        .command()
        .args(["call", "js_reset", "{}"])
        .stderr(Stdio::piped())
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("disabled by launch configuration"),
        "{stderr}"
    );
    assert!(
        fixture
            .frames()
            .iter()
            .all(|frame| frame["method"] != "tools/call"),
        "disabled tool reached the native service"
    );
    fixture.assert_processes_exited();
}

#[test]
fn native_cli_skips_a_disabled_turn_ended_cleanup_call() {
    let fixture = Fixture::new();
    let config_path = fixture.root.path().join("config.json");
    let mut config: Value = serde_json::from_slice(&std::fs::read(&config_path).unwrap()).unwrap();
    let row = &mut config["mcpServers"]["cua_repl"];
    row["enabled_tools"] = json!(["js"]);
    row["disabled_tools"] = json!([]);
    std::fs::write(&config_path, config.to_string()).unwrap();

    let output = fixture
        .command()
        .args(["call", "js", r#"{"code":"synthetic response"}"#])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        fixture.frames().iter().all(|frame| {
            frame["method"] != "tools/call" || frame["params"]["name"] != "turn_ended"
        }),
        "disabled turn_ended cleanup was sent to the native service"
    );
    fixture.assert_processes_exited();
}

#[test]
fn native_cli_image_dir_writes_native_image_bytes() {
    let fixture = Fixture::new();
    let image_dir = fixture.root.path().join("images");
    let output = fixture
        .command()
        .args(["--image-dir"])
        .arg(&image_dir)
        .args(["call", "js", r#"{"code":"synthetic image"}"#])
        .env("BRIDGE_NATIVE_FIXTURE_IMAGE_RESULT", "1")
        .output()
        .unwrap();
    assert!(output.status.success());
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["content"][0]["data"], "aGVsbG8=");
    let artifact = &result["bridge_artifacts"][0];
    assert_eq!(artifact["mimeType"], "image/png");
    assert_eq!(artifact["_meta"]["screenshotId"], "synthetic-image");
    let path = Path::new(artifact["path"].as_str().unwrap());
    assert_eq!(std::fs::read(path).unwrap(), b"hello");
    fixture.assert_processes_exited();
}

#[test]
fn native_cli_batch_image_dir_writes_native_image_bytes() {
    let fixture = Fixture::new();
    let image_dir = fixture.root.path().join("batch-images");
    let mut child = fixture
        .command()
        .args(["--image-dir"])
        .arg(&image_dir)
        .arg("batch")
        .env("BRIDGE_NATIVE_FIXTURE_IMAGE_RESULT", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"{\"name\":\"js\",\"arguments\":{\"code\":\"synthetic image\"}}\n")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let results = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(results.len(), 1);
    let result = &results[0];
    assert_eq!(result["content"][0]["data"], "aGVsbG8=");
    let artifact = &result["bridge_artifacts"][0];
    assert_eq!(artifact["content_index"], 0);
    assert_eq!(artifact["mimeType"], "image/png");
    assert_eq!(artifact["_meta"]["screenshotId"], "synthetic-image");
    let path = Path::new(artifact["path"].as_str().unwrap());
    assert_eq!(std::fs::read(path).unwrap(), b"hello");
    fixture.assert_processes_exited();
}

#[test]
fn native_ca_environment_paths_are_absolute_and_non_inheritable_values_are_scrubbed() {
    let fixture = Fixture::new();
    let output = fixture
        .command()
        .args(["--mode", "doctor"])
        .current_dir(fixture.root.path())
        .env("SSL_CERT_FILE", "relative-ca.pem")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let environment: Value = serde_json::from_slice(
        &std::fs::read(fixture.root.path().join("environment.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        environment["SSL_CERT_FILE"],
        fixture
            .root
            .path()
            .join("relative-ca.pem")
            .to_string_lossy()
            .as_ref()
    );
    assert_eq!(environment["NODE_REPL_AUTH_TOKEN"], Value::Null);
    fixture.assert_processes_exited();
}

#[test]
fn native_cli_and_doctor_apply_host_catalog_filters() {
    let fixture = Fixture::new();
    let tools = fixture
        .command()
        .arg("tools")
        .env("BRIDGE_NATIVE_FIXTURE_PAGINATE_CATALOG", "1")
        .output()
        .unwrap();
    assert!(tools.status.success());
    let tools: Value = serde_json::from_slice(&tools.stdout).unwrap();
    assert_eq!(
        tools
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["js", "turn_ended"]
    );
    assert!(fixture
        .frames()
        .iter()
        .any(|frame| { frame["method"] == "tools/list" && frame["params"]["cursor"] == "page-2" }));

    let before_doctor = fixture.frames().len();
    let doctor = fixture
        .command()
        .args(["--mode", "doctor"])
        .env("BRIDGE_NATIVE_FIXTURE_PAGINATE_CATALOG", "1")
        .output()
        .unwrap();
    assert!(doctor.status.success());
    let doctor: Value = serde_json::from_slice(&doctor.stdout).unwrap();
    assert_eq!(doctor["tools"], json!(["js", "turn_ended"]));
    assert!(fixture
        .frames()
        .into_iter()
        .skip(before_doctor)
        .any(|frame| { frame["method"] == "tools/list" && frame["params"]["cursor"] == "page-2" }));
    fixture.assert_processes_exited();
    let initialize_versions = fixture
        .frames()
        .into_iter()
        .filter(|frame| frame["method"] == "initialize")
        .map(|frame| frame["params"]["protocolVersion"].clone())
        .collect::<Vec<_>>();
    assert_eq!(
        initialize_versions,
        [json!("2025-06-18"), json!("2025-06-18")]
    );
}

#[test]
fn native_cli_reports_both_tool_error_and_unconfirmed_cleanup() {
    let fixture = Fixture::new();
    let log = fixture.root.path().join("stderr.log");
    let mut child = fixture
        .command()
        .args(["call", "js", r#"{"code":"synthetic error"}"#])
        .env("BRIDGE_NATIVE_FIXTURE_JS_ERROR", "1")
        .env("BRIDGE_NATIVE_FIXTURE_HANG_CLEANUP", "1")
        .stderr(Stdio::from(std::fs::File::create(&log).unwrap()))
        .spawn()
        .unwrap();
    assert!(!wait(&mut child, Duration::from_secs(24)).success());
    fixture.assert_processes_exited();
    let diagnostic = std::fs::read_to_string(log).unwrap();
    assert!(diagnostic.contains("native tool failed"), "{diagnostic}");
    assert!(
        diagnostic.contains("native cleanup was not confirmed"),
        "{diagnostic}"
    );
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
        vec!["--session-file", "session.json", "call", "js", "{}"],
        vec!["--keep-turn", "call", "js", "{}"],
        vec!["--upload-base", ".", "--mode", "discover"],
        vec!["--browser-instance", "inst-1", "--mode", "discover"],
        vec!["--max-image-bytes", "2048", "--mode", "discover"],
        vec!["--image-dir", "captures", "--mode", "mcp"],
        vec!["--image-dir", "captures", "--mode", "doctor"],
        vec!["--image-dir", "captures", "tools"],
    ] {
        let output = fixture.command().args(arguments).output().unwrap();
        assert!(!output.status.success());
        assert!(!fixture.root.path().join("pids.json").exists());
    }
}

#[test]
fn malformed_codex_config_toml_redacts_source_line() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("config.toml"),
        "secret_token = \"private-secret-value\"\n[broken\n",
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_codex-browser-bridge"))
        .args(["--surface", "native-browser", "--mode", "discover"])
        .env("CODEX_HOME", dir.path())
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("invalid Codex configuration TOML at line"),
        "{stderr}"
    );
    assert!(!stderr.contains("private-secret-value"), "{stderr}");
    assert!(!stderr.contains("[broken"), "{stderr}");
}

#[test]
fn native_surfaces_ignore_invalid_browser_toml_in_cwd() {
    let fixture = Fixture::new();
    std::fs::write(
        fixture.root.path().join(".codex-browser-bridge.toml"),
        "not_valid_toml = [\n",
    )
    .unwrap();
    let output = fixture
        .command()
        .current_dir(fixture.root.path())
        .args(["--mode", "discover"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn native_cli_mode_rejected_before_discovery_and_browser_rejects_native_flags() {
    let empty_home = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_codex-browser-bridge"))
        .args(["--surface", "native-browser", "--mode", "cli"])
        .env("CODEX_HOME", empty_home.path())
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("native CLI requires tools, call, or batch"),
        "{stderr}"
    );

    for (args, expected) in [
        (
            vec![
                "--surface",
                "browser",
                "--native-config",
                "x.json",
                "--mode",
                "discover",
            ],
            "--native-config requires --surface computer or --surface native-browser",
        ),
        (
            vec![
                "--surface",
                "browser",
                "--image-dir",
                "captures",
                "--mode",
                "discover",
            ],
            "--image-dir requires --surface computer or --surface native-browser",
        ),
    ] {
        let out = Command::new(env!("CARGO_BIN_EXE_codex-browser-bridge"))
            .args(args)
            .output()
            .unwrap();
        assert!(!out.status.success());
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(err.contains(expected), "{err}");
    }
}

#[test]
fn native_image_dir_conflict_error_identifies_the_required_command() {
    let fixture = Fixture::new();
    let output = fixture
        .command()
        .args(["--image-dir", "captures", "tools"])
        .stderr(Stdio::piped())
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--image-dir requires a native call or batch command"),
        "{stderr}"
    );
    assert!(!fixture.root.path().join("pids.json").exists());
}

#[cfg(windows)]
#[test]
fn native_launch_resolves_command_from_launch_row_path_and_preserves_pathext() {
    let fixture = Fixture::new();
    let config_path = fixture.root.path().join("config.json");
    let mut config: Value = serde_json::from_slice(&std::fs::read(&config_path).unwrap()).unwrap();
    let row = &mut config["mcpServers"]["cua_repl"];
    let python = row["env"]["CUA_REPL_NODE_REPL_PATH"]
        .as_str()
        .unwrap()
        .to_string();
    let wrapper = fixture.root.path().join("native-probe.cmd");
    std::fs::write(
        &wrapper,
        format!("@echo off\r\n\"{python}\" \"%~dp0native_mcp.py\" %*\r\n"),
    )
    .unwrap();
    row["command"] = json!("native-probe");
    row["env"]["PATH"] = json!(fixture.root.path());
    row["env"]["PATHEXT"] = json!(".SYNTHETIC;.CMD");
    std::fs::write(&config_path, config.to_string()).unwrap();

    let output = fixture
        .command()
        .args(["--mode", "doctor"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let environment: Value = serde_json::from_slice(
        &std::fs::read(fixture.root.path().join("environment.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        environment["PATH"],
        fixture.root.path().to_string_lossy().as_ref()
    );
    assert_eq!(environment["PATHEXT"], ".SYNTHETIC;.CMD");
    fixture.assert_processes_exited();
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
