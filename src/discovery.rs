use serde::Serialize;
#[cfg(windows)]
use tokio::process::Command;
#[cfg(windows)]
use tokio::time::{timeout, Duration};

use crate::error::{BridgeError, Result};

const CODEX_PIPE_PREFIX: &str = "codex-browser-use";
#[cfg(windows)]
const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(15);
#[cfg(windows)]
const PIPE_ENUMERATION_SCRIPT: &str = "$d='\\\\.\\pipe\\'; [System.IO.Directory]::GetFileSystemEntries($d) | Where-Object { $_ -like '*codex-browser*' } | ForEach-Object { $_.Substring($d.Length) }";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PipeInfo {
    pub name: String,
    pub uuid: String,
}

#[cfg(windows)]
pub async fn discover_codex_pipes() -> Result<Vec<PipeInfo>> {
    discover_codex_pipes_with_timeout(DISCOVERY_TIMEOUT).await
}

#[cfg(not(windows))]
pub async fn discover_codex_pipes() -> Result<Vec<PipeInfo>> {
    Err(BridgeError::User(
        "codex-browser-bridge pipe discovery only supports Windows named pipes".into(),
    ))
}

#[cfg(windows)]
async fn discover_codex_pipes_with_timeout(timeout_duration: Duration) -> Result<Vec<PipeInfo>> {
    let mut command = Command::new("powershell");
    command
        .kill_on_drop(true)
        .args(["-NoProfile", "-Command", PIPE_ENUMERATION_SCRIPT]);
    let stdout = run_pipe_enumeration(command, timeout_duration).await?;
    Ok(parse_pipe_list(&stdout))
}

#[cfg(windows)]
async fn run_pipe_enumeration(mut command: Command, timeout_duration: Duration) -> Result<String> {
    let output = timeout(timeout_duration, command.output())
        .await
        .map_err(|_| BridgeError::Discovery("pipe enumeration timed out".into()))?
        .map_err(|err| BridgeError::Discovery(format!("enumerate pipes: {err}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(BridgeError::Discovery(format!("enumerate pipes: {stderr}")));
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub fn parse_pipe_list(output: &str) -> Vec<PipeInfo> {
    output
        .lines()
        .filter_map(|line| {
            let name = line.trim();
            if !name.starts_with(CODEX_PIPE_PREFIX) {
                return None;
            }
            let uuid = extract_uuid(name)?;
            Some(PipeInfo {
                name: name.to_string(),
                uuid,
            })
        })
        .collect()
}

pub fn extract_uuid(name: &str) -> Option<String> {
    let mut rest = name.strip_prefix(CODEX_PIPE_PREFIX)?;
    if rest.starts_with('-') || rest.starts_with('\\') {
        rest = &rest[1..];
    }
    if rest.is_empty() {
        None
    } else {
        Some(rest.to_string())
    }
}

pub fn pipe_path(name: &str) -> String {
    format!(r"\\.\pipe\{name}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_uuid_like_go_version() {
        assert_eq!(
            extract_uuid("codex-browser-use-abc12345-6789-4def-9abc-123456789abc"),
            Some("abc12345-6789-4def-9abc-123456789abc".into())
        );
        assert_eq!(
            extract_uuid(r"codex-browser-use\abc12345-6789-4def-9abc-123456789abc"),
            Some("abc12345-6789-4def-9abc-123456789abc".into())
        );
        assert_eq!(extract_uuid("codex-browser-use"), None);
        assert_eq!(extract_uuid("codex-browser-use-x"), Some("x".into()));
    }

    #[test]
    fn parses_old_and_new_pipe_names_and_ignores_unrelated_pipes() {
        let got = parse_pipe_list("InputPipe_1\r\ncodex-browser-use\r\ncodex-browser-use-abc12345-6789-4def-9abc-123456789abc\r\n   codex-browser-use-second-pipe   \r\ncodex-browser-use\\third-pipe\r\nunrelated-pipe\r\ncodex-browser-extra-foo\r\n");
        assert_eq!(
            got,
            vec![
                PipeInfo {
                    name: "codex-browser-use-abc12345-6789-4def-9abc-123456789abc".into(),
                    uuid: "abc12345-6789-4def-9abc-123456789abc".into()
                },
                PipeInfo {
                    name: "codex-browser-use-second-pipe".into(),
                    uuid: "second-pipe".into()
                },
                PipeInfo {
                    name: r"codex-browser-use\third-pipe".into(),
                    uuid: "third-pipe".into()
                }
            ]
        );
    }

    #[test]
    fn pipe_path_prefixes_windows_pipe_namespace() {
        assert_eq!(
            pipe_path("codex-browser-use-foo"),
            r"\\.\pipe\codex-browser-use-foo"
        );
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn powershell_enumeration_timeout_is_reported() {
        let mut command = Command::new("powershell");
        command
            .kill_on_drop(true)
            .args(["-NoProfile", "-Command", "Start-Sleep -Seconds 2"]);

        let err = run_pipe_enumeration(command, Duration::from_millis(10))
            .await
            .unwrap_err();

        assert!(err.to_string().contains("pipe enumeration timed out"));
    }

    #[cfg(not(windows))]
    #[tokio::test]
    async fn discovery_reports_windows_only_on_other_platforms() {
        let err = discover_codex_pipes().await.unwrap_err();

        assert!(err
            .to_string()
            .contains("only supports Windows named pipes"));
    }
}
