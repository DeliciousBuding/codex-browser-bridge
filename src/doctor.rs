use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{json, Value};

use crate::discovery;

const COMMAND_NAME: &str = "codex-browser-bridge";

#[derive(Debug, Serialize)]
pub struct PipeDiagnostic {
    pub name: String,
    pub connected: bool,
    pub latency_ms: Option<u64>,
    pub browser: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct InstallDiagnostic {
    pub current_exe: Option<String>,
    pub path_lookup: Option<String>,
    pub path_lookup_matches_current_exe: Option<bool>,
    pub suggested_mcp_config: Option<Value>,
    pub notes: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct DiagnosticResult {
    pub bridge_version: String,
    pub install: InstallDiagnostic,
    pub pipe_count: usize,
    pub pipes: Vec<PipeDiagnostic>,
    pub healthy: bool,
    pub warnings: Vec<String>,
}

/// Run self-diagnostics: enumerate pipes, probe each, report status.
pub async fn run_diagnostics() -> DiagnosticResult {
    let mut pipes = Vec::new();
    let mut warnings = Vec::new();
    let install = collect_install_diagnostic();

    match discovery::discover_codex_pipes().await {
        Ok(discovered) => {
            for pipe in &discovered {
                let start = std::time::Instant::now();
                let path = discovery::pipe_path(&pipe.name);
                let (connected, browser) = match crate::pipe::dial_named_pipe(&path).await {
                    Ok(stream) => {
                        // Quick getInfo probe
                        let client = crate::client::Client::from_stream(stream);
                        match client {
                            Ok(c) => {
                                let result = tokio::time::timeout(
                                    std::time::Duration::from_secs(3),
                                    c.send_request("getInfo", None),
                                )
                                .await;
                                match result {
                                    Ok(Ok(raw)) => {
                                        // Try to extract browser version
                                        let info_str = raw.get();
                                        let browser =
                                            serde_json::from_str::<serde_json::Value>(info_str)
                                                .ok()
                                                .and_then(|v| {
                                                    v.get("browserVersion")
                                                        .or_else(|| v.get("product"))
                                                        .and_then(|v| v.as_str())
                                                        .map(String::from)
                                                });
                                        (true, browser)
                                    }
                                    _ => (false, None),
                                }
                            }
                            Err(_) => (false, None),
                        }
                    }
                    Err(_) => (false, None),
                };
                let latency_ms = if connected {
                    Some(start.elapsed().as_millis() as u64)
                } else {
                    None
                };
                pipes.push(PipeDiagnostic {
                    name: pipe.name.clone(),
                    connected,
                    latency_ms,
                    browser,
                });
            }
        }
        Err(err) => {
            warnings.push(format!("Pipe discovery failed: {err}"));
        }
    }

    let connected_count = pipes.iter().filter(|p| p.connected).count();
    if pipes.is_empty() {
        warnings.push(
            "No Codex browser pipes found. Start Codex Desktop, Chrome, and the Codex Chrome extension."
                .into(),
        );
    }
    if connected_count == 0 && !pipes.is_empty() {
        warnings.push("No pipes responded to health check. Try restarting Codex Desktop.".into());
    }

    DiagnosticResult {
        bridge_version: env!("CARGO_PKG_VERSION").to_string(),
        install,
        pipe_count: pipes.len(),
        healthy: connected_count > 0 && warnings.is_empty(),
        pipes,
        warnings,
    }
}

fn collect_install_diagnostic() -> InstallDiagnostic {
    install_diagnostic_for(
        std::env::current_exe().ok(),
        std::env::var_os("PATH"),
        std::env::var_os("PATHEXT"),
    )
}

fn install_diagnostic_for(
    current_exe: Option<PathBuf>,
    path_var: Option<std::ffi::OsString>,
    path_ext: Option<std::ffi::OsString>,
) -> InstallDiagnostic {
    let current_exe = current_exe.map(path_to_string);
    let path_lookup = find_command_on_path(COMMAND_NAME, path_var.as_deref(), path_ext.as_deref())
        .map(path_to_string);
    let suggested_mcp_config = current_exe
        .as_deref()
        .or(path_lookup.as_deref())
        .map(suggested_mcp_config);
    let path_lookup_matches_current_exe = match (&current_exe, &path_lookup) {
        (Some(current), Some(found)) => Some(paths_match(current, found)),
        _ => None,
    };

    let mut notes = Vec::new();
    if path_lookup.is_none() {
        notes.push(
            "Command was not found on PATH; GUI clients and scheduled agents should use the absolute command path."
                .to_string(),
        );
    }
    if matches!(path_lookup_matches_current_exe, Some(false)) {
        notes.push(
            "PATH resolves a different codex-browser-bridge command than the running process."
                .to_string(),
        );
    }

    InstallDiagnostic {
        current_exe,
        path_lookup,
        path_lookup_matches_current_exe,
        suggested_mcp_config,
        notes,
    }
}

fn suggested_mcp_config(command: &str) -> Value {
    json!({
        "mcpServers": {
            "codex-browser": {
                "command": command,
                "args": ["--mode", "mcp"],
                "transport": "stdio",
                "env": {
                    "CODEX_BRIDGE_PROFILE": "full"
                }
            }
        }
    })
}

fn find_command_on_path(
    command: &str,
    path_var: Option<&OsStr>,
    path_ext: Option<&OsStr>,
) -> Option<PathBuf> {
    let path_var = path_var?;
    let candidates = command_candidates(command, path_ext);
    for dir in std::env::split_paths(path_var) {
        for candidate in &candidates {
            let path = dir.join(candidate);
            if path.is_file() {
                return Some(path);
            }
        }
    }
    None
}

fn command_candidates(command: &str, path_ext: Option<&OsStr>) -> Vec<String> {
    if Path::new(command).extension().is_some() {
        return vec![command.to_string()];
    }
    let mut candidates = vec![command.to_string()];
    if cfg!(windows) {
        let exts = path_ext
            .and_then(|value| value.to_str())
            .unwrap_or(".COM;.EXE;.BAT;.CMD");
        for ext in exts.split(';').filter(|ext| !ext.trim().is_empty()) {
            candidates.push(format!("{command}{}", ext.trim().to_ascii_lowercase()));
            candidates.push(format!("{command}{}", ext.trim().to_ascii_uppercase()));
        }
    }
    candidates.sort();
    candidates.dedup();
    candidates
}

fn path_to_string(path: PathBuf) -> String {
    path.to_string_lossy().into_owned()
}

fn paths_match(left: &str, right: &str) -> bool {
    if cfg!(windows) {
        left.eq_ignore_ascii_case(right)
    } else {
        left == right
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn install_diagnostic_prefers_current_exe_for_mcp_config() {
        let current = PathBuf::from(r"C:\Tools\codex-browser-bridge.exe");
        let diag = install_diagnostic_for(Some(current.clone()), None, None);

        assert_eq!(
            diag.current_exe.as_deref(),
            Some(r"C:\Tools\codex-browser-bridge.exe")
        );
        assert!(diag.path_lookup.is_none());
        assert!(diag
            .notes
            .iter()
            .any(|note| note.contains("absolute command path")));
        assert_eq!(
            diag.suggested_mcp_config.unwrap()["mcpServers"]["codex-browser"]["command"],
            current.to_string_lossy().as_ref()
        );
    }

    #[test]
    fn install_diagnostic_reports_path_lookup_match() {
        let root = std::env::temp_dir().join(format!(
            "codex-browser-bridge-doctor-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&root).unwrap();
        let exe = root.join(if cfg!(windows) {
            "codex-browser-bridge.exe"
        } else {
            "codex-browser-bridge"
        });
        fs::write(&exe, b"").unwrap();

        let diag = install_diagnostic_for(
            Some(exe.clone()),
            Some(root.as_os_str().to_os_string()),
            Some(".EXE;.CMD".into()),
        );

        assert!(paths_match(
            diag.path_lookup.as_deref().unwrap(),
            exe.to_string_lossy().as_ref()
        ));
        assert_eq!(diag.path_lookup_matches_current_exe, Some(true));
        assert!(diag.notes.is_empty());

        fs::remove_dir_all(root).ok();
    }
}
