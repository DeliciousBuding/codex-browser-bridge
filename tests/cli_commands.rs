use assert_cmd::Command;
use serde_json::Value;
use std::time::Duration;

fn bridge() -> Command {
    let mut command = Command::cargo_bin("codex-browser-bridge").unwrap();
    command
        .env_remove("CODEX_BRIDGE_PROFILE")
        .env_remove("CODEX_BRIDGE_BROWSER_FAMILY")
        .env_remove("CODEX_BRIDGE_BROWSER_INSTANCE")
        .env("CODEX_BRIDGE_CONFIG", "nonexistent-cli-test-config.toml")
        .timeout(Duration::from_secs(5));
    command
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
    assert!(help.contains("--finalize"));
    assert!(help.contains("No session is persisted"));
}
