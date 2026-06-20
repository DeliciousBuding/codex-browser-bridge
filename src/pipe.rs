use crate::error::Result;

#[cfg(windows)]
pub type PipeStream = tokio::net::windows::named_pipe::NamedPipeClient;

#[cfg(windows)]
const DIAL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

#[cfg(windows)]
pub async fn dial_named_pipe(path: &str) -> Result<PipeStream> {
    dial_named_pipe_with_timeout(path, DIAL_TIMEOUT).await
}

#[cfg(windows)]
async fn dial_named_pipe_with_timeout(
    path: &str,
    dial_timeout: std::time::Duration,
) -> Result<PipeStream> {
    let deadline = tokio::time::Instant::now() + dial_timeout;

    loop {
        match tokio::net::windows::named_pipe::ClientOptions::new().open(path) {
            Ok(stream) => return Ok(stream),
            Err(err) => {
                let now = tokio::time::Instant::now();
                if now >= deadline {
                    return Err(crate::error::BridgeError::Timeout(format!(
                        "dial pipe {path}: {err}"
                    )));
                }

                tokio::time::sleep_until(
                    (now + std::time::Duration::from_millis(50)).min(deadline),
                )
                .await;
            }
        }
    }
}

// ── Non-Windows: no-op stubs ──────────────────────────────────

#[cfg(not(windows))]
pub type PipeStream = tokio::io::DuplexStream;

#[cfg(not(windows))]
pub async fn dial_named_pipe(path: &str) -> Result<PipeStream> {
    let _ = path;
    Err(crate::error::BridgeError::User(
        "codex-browser-bridge only supports Windows named pipes. \
         On macOS/Linux, consider using WSL or a Windows VM.".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[tokio::test]
    async fn dial_named_pipe_reports_timeout() {
        let path = r"\\.\pipe\codex-browser-use-test-timeout-missing";

        let err = dial_named_pipe_with_timeout(path, std::time::Duration::from_millis(10))
            .await
            .unwrap_err();

        assert!(err.to_string().contains("timeout waiting for dial pipe"));
    }

    #[cfg(not(windows))]
    #[tokio::test]
    async fn dial_named_pipe_reports_windows_only_on_other_platforms() {
        let err = dial_named_pipe("codex-browser-use-test").await.unwrap_err();

        assert!(err
            .to_string()
            .contains("only supports Windows named pipes"));
    }
}
