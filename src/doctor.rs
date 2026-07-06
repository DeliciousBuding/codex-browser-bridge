use serde::Serialize;

use crate::discovery;

#[derive(Debug, Serialize)]
pub struct PipeDiagnostic {
    pub name: String,
    pub connected: bool,
    pub latency_ms: Option<u64>,
    pub browser: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DiagnosticResult {
    pub bridge_version: String,
    pub pipe_count: usize,
    pub pipes: Vec<PipeDiagnostic>,
    pub healthy: bool,
    pub warnings: Vec<String>,
}

/// Run self-diagnostics: enumerate pipes, probe each, report status.
pub async fn run_diagnostics() -> DiagnosticResult {
    let mut pipes = Vec::new();
    let mut warnings = Vec::new();

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
        pipe_count: pipes.len(),
        healthy: connected_count > 0 && warnings.is_empty(),
        pipes,
        warnings,
    }
}
