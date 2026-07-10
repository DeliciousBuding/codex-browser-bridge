use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;
use serde_json::{json, Value};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tokio::time::{timeout, Duration, Instant};

use crate::discovery;

const COMMAND_NAME: &str = "codex-browser-bridge";
const MAX_REPORTED_PIPES: usize = 24;
const MAX_CONCURRENT_PIPE_PROBES: usize = 16;
const DOCTOR_PROBE_BUDGET: Duration = Duration::from_secs(10);
const GET_INFO_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Clone, Debug, Serialize)]
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
    pub mcp_spawn_ready: bool,
    pub suggested_mcp_config: Option<Value>,
    pub notes: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct DiagnosticResult {
    pub bridge_version: String,
    pub install: InstallDiagnostic,
    pub pipe_count: usize,
    pub probed_pipe_count: usize,
    pub unprobed_pipe_count: usize,
    pub connected_pipe_count: usize,
    pub failed_pipe_count: usize,
    pub omitted_pipe_count: usize,
    pub pipes_truncated: bool,
    pub pipes: Vec<PipeDiagnostic>,
    pub healthy: bool,
    pub warnings: Vec<String>,
}

/// Run self-diagnostics: enumerate pipes, probe each, report status.
pub async fn run_diagnostics() -> DiagnosticResult {
    let mut pipes = Vec::new();
    let mut warnings = Vec::new();
    let mut pipe_count = 0;
    let mut unprobed_pipe_count = 0;
    let install = collect_install_diagnostic();

    match discovery::discover_codex_pipes().await {
        Ok(discovered) => {
            pipe_count = discovered.len();
            let result = probe_discovered_pipes(discovered).await;
            pipes = result.pipes;
            unprobed_pipe_count = result.unprobed_pipe_count;
        }
        Err(err) => {
            warnings.push(format!("Pipe discovery failed: {err}"));
        }
    }

    let probed_pipe_count = pipes.len();
    let connected_count = pipes.iter().filter(|p| p.connected).count();
    let failed_count = probed_pipe_count.saturating_sub(connected_count);
    if pipe_count == 0 {
        warnings.push(
            "No Codex browser pipes found. Start Codex Desktop, Chrome, and the Codex Chrome extension."
                .into(),
        );
    }
    if connected_count == 0 && pipe_count > 0 {
        if unprobed_pipe_count > 0 {
            warnings.push(
                "No probed pipes responded before the doctor probe budget expired. Try restarting Codex Desktop."
                    .into(),
            );
        } else {
            warnings
                .push("No pipes responded to health check. Try restarting Codex Desktop.".into());
        }
    }
    let (pipes, omitted_pipe_count) = reported_pipes(pipes, MAX_REPORTED_PIPES);

    DiagnosticResult {
        bridge_version: env!("CARGO_PKG_VERSION").to_string(),
        install,
        pipe_count,
        probed_pipe_count,
        unprobed_pipe_count,
        connected_pipe_count: connected_count,
        failed_pipe_count: failed_count,
        omitted_pipe_count,
        pipes_truncated: omitted_pipe_count > 0,
        healthy: connected_count > 0 && warnings.is_empty(),
        pipes,
        warnings,
    }
}

struct ProbeResult {
    pipes: Vec<PipeDiagnostic>,
    unprobed_pipe_count: usize,
}

async fn probe_discovered_pipes(discovered: Vec<discovery::PipeInfo>) -> ProbeResult {
    let total = discovered.len();
    let deadline = Instant::now() + DOCTOR_PROBE_BUDGET;
    let permits = Arc::new(Semaphore::new(MAX_CONCURRENT_PIPE_PROBES));
    let mut tasks = JoinSet::new();

    for pipe in discovered {
        let permits = Arc::clone(&permits);
        tasks.spawn(async move {
            let Ok(_permit) = permits.acquire_owned().await else {
                return None;
            };
            Some(probe_pipe(pipe).await)
        });
    }

    let mut pipes = Vec::with_capacity(total.min(MAX_REPORTED_PIPES));
    while pipes.len() < total {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }

        match timeout(remaining, tasks.join_next()).await {
            Ok(Some(Ok(Some(pipe)))) => pipes.push(pipe),
            Ok(Some(Ok(None))) | Ok(Some(Err(_))) => {}
            Ok(None) | Err(_) => break,
        }
    }

    tasks.abort_all();
    ProbeResult {
        unprobed_pipe_count: total.saturating_sub(pipes.len()),
        pipes,
    }
}

async fn probe_pipe(pipe: discovery::PipeInfo) -> PipeDiagnostic {
    let start = Instant::now();
    let path = discovery::pipe_path(&pipe.name);
    let (connected, browser) = match crate::pipe::dial_named_pipe(&path).await {
        Ok(stream) => {
            let client = crate::client::Client::from_stream(stream);
            match client {
                Ok(c) => match timeout(GET_INFO_TIMEOUT, c.send_request("getInfo", None)).await {
                    Ok(Ok(raw)) => {
                        let info_str = raw.get();
                        let browser = serde_json::from_str::<serde_json::Value>(info_str)
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
                },
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
    PipeDiagnostic {
        name: pipe.name,
        connected,
        latency_ms,
        browser,
    }
}

fn reported_pipes(
    pipes: Vec<PipeDiagnostic>,
    max_reported_pipes: usize,
) -> (Vec<PipeDiagnostic>, usize) {
    if pipes.len() <= max_reported_pipes {
        return (pipes, 0);
    }

    let total = pipes.len();
    let mut reported = Vec::with_capacity(max_reported_pipes);
    reported.extend(
        pipes
            .iter()
            .filter(|pipe| pipe.connected)
            .take(max_reported_pipes)
            .cloned(),
    );
    if reported.len() < max_reported_pipes {
        reported.extend(
            pipes
                .iter()
                .filter(|pipe| !pipe.connected)
                .take(max_reported_pipes - reported.len())
                .cloned(),
        );
    }

    let omitted = total.saturating_sub(reported.len());
    (reported, omitted)
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
    let mcp_spawn_ready =
        path_lookup.is_some() && !matches!(path_lookup_matches_current_exe, Some(false));

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
        mcp_spawn_ready,
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

    fn pipe(name: &str, connected: bool) -> PipeDiagnostic {
        PipeDiagnostic {
            name: name.to_string(),
            connected,
            latency_ms: connected.then_some(1),
            browser: None,
        }
    }

    #[test]
    fn reported_pipes_are_bounded_and_prioritize_connected_entries() {
        let pipes = vec![
            pipe("failed-1", false),
            pipe("connected-1", true),
            pipe("failed-2", false),
            pipe("connected-2", true),
            pipe("failed-3", false),
        ];

        let (reported, omitted) = reported_pipes(pipes, 3);

        assert_eq!(omitted, 2);
        assert_eq!(
            reported
                .iter()
                .map(|pipe| pipe.name.as_str())
                .collect::<Vec<_>>(),
            vec!["connected-1", "connected-2", "failed-1"]
        );
    }

    #[test]
    fn reported_pipes_do_not_mark_small_lists_truncated() {
        let pipes = vec![pipe("failed-1", false), pipe("connected-1", true)];

        let (reported, omitted) = reported_pipes(pipes, 24);

        assert_eq!(omitted, 0);
        assert_eq!(reported.len(), 2);
    }

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
        assert!(!diag.mcp_spawn_ready);
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
        assert!(diag.mcp_spawn_ready);
        assert!(diag.notes.is_empty());

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn install_diagnostic_reports_path_lookup_mismatch_as_not_spawn_ready() {
        let root = std::env::temp_dir().join(format!(
            "codex-browser-bridge-doctor-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&root).unwrap();
        let path_exe = root.join(if cfg!(windows) {
            "codex-browser-bridge.exe"
        } else {
            "codex-browser-bridge"
        });
        fs::write(&path_exe, b"").unwrap();

        let diag = install_diagnostic_for(
            Some(PathBuf::from(r"C:\Other\codex-browser-bridge.exe")),
            Some(root.as_os_str().to_os_string()),
            Some(".EXE;.CMD".into()),
        );

        assert_eq!(diag.path_lookup_matches_current_exe, Some(false));
        assert!(!diag.mcp_spawn_ready);
        assert!(diag
            .notes
            .iter()
            .any(|note| note.contains("different codex-browser-bridge command")));

        fs::remove_dir_all(root).ok();
    }
}
