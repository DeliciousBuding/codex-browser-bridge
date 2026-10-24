use assert_cmd::Command;
use codex_browser_bridge::config::{Config, ConfigSource, MAX_CONTENT_BYTES, MIN_CONTENT_BYTES};
use predicates::str::contains;
use serde_json::Value;
use std::path::Path;
use std::time::Duration;

fn bridge(directory: &Path) -> Command {
    let mut command = Command::cargo_bin("codex-browser-bridge").unwrap();
    command
        .current_dir(directory)
        .timeout(Duration::from_secs(5));
    for variable in [
        "CODEX_BRIDGE_CONFIG",
        "CODEX_BRIDGE_PROFILE",
        "CODEX_BRIDGE_BROWSER_FAMILY",
        "CODEX_BRIDGE_BROWSER_INSTANCE",
        "CODEX_BRIDGE_UPLOAD_BASE",
        "CODEX_BRIDGE_MAX_TEXT_BYTES",
        "CODEX_BRIDGE_MAX_IMAGE_BYTES",
    ] {
        command.env_remove(variable);
    }
    // These tests must not reach a developer's browser even on regression.
    command.env(
        "CODEX_BRIDGE_SOCKET_DIR",
        "/nonexistent-configuration-test-sockets",
    );
    command
}

#[test]
fn missing_default_file_and_empty_explicit_file_are_valid() {
    let directory = tempfile::tempdir().unwrap();
    bridge(directory.path()).arg("tools").assert().success();
    let explicit = directory.path().join("empty.toml");
    std::fs::write(&explicit, "").unwrap();
    bridge(directory.path())
        .env("CODEX_BRIDGE_CONFIG", explicit)
        .arg("tools")
        .assert()
        .success();
}

#[test]
fn explicit_missing_file_never_falls_back_to_working_directory() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join(".codex-browser-bridge.toml"),
        "profile = 'basic'\n",
    )
    .unwrap();
    bridge(directory.path())
        .env("CODEX_BRIDGE_CONFIG", "missing.toml")
        .arg("tools")
        .assert()
        .failure()
        .stdout("")
        .stderr(contains("cannot read explicit configuration file"));
}

#[test]
fn explicit_unreadable_file_fails() {
    let directory = tempfile::tempdir().unwrap();
    bridge(directory.path())
        .env("CODEX_BRIDGE_CONFIG", directory.path())
        .arg("tools")
        .assert()
        .failure()
        .stdout("")
        .stderr(contains("cannot read explicit configuration file"));
}

#[test]
fn broken_toml_and_unknown_keys_fail_without_echoing_values() {
    let directory = tempfile::tempdir().unwrap();
    for content in [
        "profile = 'private-value' malformed",
        "browser_instnace = 'private-value'\n",
    ] {
        let path = directory.path().join("config.toml");
        std::fs::write(&path, content).unwrap();
        let output = bridge(directory.path())
            .env("CODEX_BRIDGE_CONFIG", path)
            .arg("tools")
            .assert()
            .failure()
            .stdout("")
            .stderr(contains("invalid TOML or unknown configuration key"))
            .get_output()
            .clone();
        assert!(!String::from_utf8_lossy(&output.stderr).contains("private-value"));
    }
}

#[test]
fn broken_default_file_fails_instead_of_becoming_defaults() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join(".codex-browser-bridge.toml"),
        "browser_instance = [",
    )
    .unwrap();
    bridge(directory.path())
        .arg("tools")
        .assert()
        .failure()
        .stderr(contains("invalid default configuration file"));
}

#[test]
fn file_values_are_validated_and_normalized() {
    let config = Config::parse("profile = ' NETWORK '\nbrowser_family = ' Edge '\nbrowser_instance = ' exact-id '\nmax_text_bytes = 1024\nmax_image_bytes = 8388608\n").unwrap();
    assert_eq!(config.profile.as_deref(), Some("network"));
    assert_eq!(config.browser_family.as_deref(), Some("edge"));
    assert_eq!(config.browser_instance.as_deref(), Some("exact-id"));
    assert_eq!(config.max_text_bytes, Some(MIN_CONTENT_BYTES));
    assert_eq!(config.max_image_bytes, Some(MAX_CONTENT_BYTES));
    for text in [
        "browser_instance = ' '",
        "browser_family = ''",
        "browser_family = 'wrong family'",
        "profile = 'unknown'",
        "profile = ''",
        "upload_base = ' '",
        "max_text_bytes = 0",
        "max_text_bytes = 1023",
        "max_image_bytes = 8388609",
        "max_image_bytes = -1",
    ] {
        assert!(Config::parse(text).is_err(), "accepted {text}");
    }
}

#[test]
fn invalid_environment_values_fail_before_tool_execution() {
    let directory = tempfile::tempdir().unwrap();
    for (variable, value) in [
        ("CODEX_BRIDGE_PROFILE", "unknown"),
        ("CODEX_BRIDGE_BROWSER_FAMILY", ""),
        ("CODEX_BRIDGE_BROWSER_INSTANCE", " "),
        ("CODEX_BRIDGE_UPLOAD_BASE", ""),
        ("CODEX_BRIDGE_MAX_TEXT_BYTES", "not-a-number"),
        ("CODEX_BRIDGE_MAX_TEXT_BYTES", "0"),
        ("CODEX_BRIDGE_MAX_IMAGE_BYTES", "8388609"),
    ] {
        bridge(directory.path())
            .env(variable, value)
            .arg("tools")
            .assert()
            .failure()
            .stdout("");
    }
}

#[test]
fn cli_rejects_empty_identity_and_out_of_range_limits() {
    let directory = tempfile::tempdir().unwrap();
    for (flag, value) in [
        ("--browser-instance", ""),
        ("--browser-family", " "),
        ("--upload-base", ""),
        ("--max-text-bytes", "0"),
        ("--max-image-bytes", "8388609"),
    ] {
        bridge(directory.path())
            .args([flag, value, "tools"])
            .assert()
            .failure()
            .stdout("");
    }
}

#[test]
fn selector_precedence_replaces_the_entire_identity_group() {
    let file = Config::parse("browser_family = 'edge'\nbrowser_instance = 'file-id'").unwrap();
    let environment =
        Config::parse("browser_family = 'brave'\nbrowser_instance = 'env-id'").unwrap();
    let selection = file
        .browser_selection(Some("chrome".into()), None, &environment)
        .unwrap();
    assert_eq!(selection.source, ConfigSource::Cli);
    assert_eq!(selection.selector.family.as_deref(), Some("chrome"));
    assert_eq!(selection.selector.instance_id, None);

    let selection = file
        .browser_selection(None, Some("cli-id".into()), &environment)
        .unwrap();
    assert_eq!(selection.selector.family, None);
    assert_eq!(selection.selector.instance_id.as_deref(), Some("cli-id"));

    let family_only = Config::parse("browser_family = 'chrome'").unwrap();
    let selection = file.browser_selection(None, None, &family_only).unwrap();
    assert_eq!(selection.source, ConfigSource::Environment);
    assert_eq!(selection.selector.family.as_deref(), Some("chrome"));
    assert_eq!(selection.selector.instance_id, None);

    let selection = file
        .browser_selection(None, None, &Config::default())
        .unwrap();
    assert_eq!(selection.source, ConfigSource::Config);
    assert_eq!(selection.selector.instance_id.as_deref(), Some("file-id"));
    let selection = Config::default()
        .browser_selection(None, None, &Config::default())
        .unwrap();
    assert_eq!(selection.source, ConfigSource::Default);
    assert!(selection.selector.is_empty());
    assert_eq!(
        serde_json::to_value(selection).unwrap()["source"],
        "default"
    );
}

#[test]
fn environment_and_cli_profile_precedence_is_preserved() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join(".codex-browser-bridge.toml"),
        "profile = 'basic'",
    )
    .unwrap();
    let output = bridge(directory.path())
        .env("CODEX_BRIDGE_PROFILE", "NETWORK")
        .args(["--profile", "full", "tools", "codex_execute_cdp"])
        .assert()
        .success()
        .get_output()
        .clone();
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["name"], "codex_execute_cdp");
}
