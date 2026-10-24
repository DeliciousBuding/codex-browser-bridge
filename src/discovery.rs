use serde::Serialize;
#[cfg(windows)]
use tokio::process::Command;
#[cfg(windows)]
use tokio::time::{timeout, Duration};

#[cfg(not(unix))]
use crate::error::BridgeError;
use crate::error::Result;

const CODEX_PIPE_PREFIX: &str = "codex-browser-use";
pub const DEFAULT_UNIX_SOCKET_DIRECTORY: &str = "/tmp/codex-browser-use";
#[cfg(unix)]
pub const UNIX_SOCKET_DIRECTORY: &str = DEFAULT_UNIX_SOCKET_DIRECTORY;

/// Directories polled for extension sockets, highest priority first.
///
/// `CODEX_BRIDGE_SOCKET_DIR` is an explicit override: when set, only that
/// directory is polled, which keeps sandboxes and tests isolated from a real
/// browser on the same machine. Otherwise `$XDG_RUNTIME_DIR/codex-browser-use`
/// is tried first and the historical `/tmp` location stays as a fallback.
#[cfg(unix)]
pub fn unix_socket_directories() -> Vec<std::path::PathBuf> {
    unix_socket_directories_with(|name| std::env::var_os(name))
}

#[cfg(unix)]
fn unix_socket_directories_with(
    read: impl Fn(&str) -> Option<std::ffi::OsString>,
) -> Vec<std::path::PathBuf> {
    use std::path::PathBuf;
    if let Some(value) = read("CODEX_BRIDGE_SOCKET_DIR") {
        if !value.is_empty() {
            return vec![PathBuf::from(value)];
        }
    }
    let mut directories = Vec::new();
    if let Some(value) = read("XDG_RUNTIME_DIR") {
        if !value.is_empty() {
            directories.push(PathBuf::from(value).join("codex-browser-use"));
        }
    }
    directories.push(PathBuf::from(DEFAULT_UNIX_SOCKET_DIRECTORY));
    directories.dedup();
    directories
}

pub const fn transport_kind() -> &'static str {
    if cfg!(windows) {
        "windows_named_pipe"
    } else if cfg!(unix) {
        "unix_socket"
    } else {
        "unsupported"
    }
}
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

#[cfg(unix)]
pub async fn discover_codex_pipes() -> Result<Vec<PipeInfo>> {
    let mut sockets = Vec::new();
    for (index, directory) in unix_socket_directories().iter().enumerate() {
        match discover_unix_sockets_in(directory) {
            Ok(mut found) => sockets.append(&mut found),
            // The highest-priority directory is authoritative: an unsafe or
            // unreadable endpoint there must not be silently skipped. Lower
            // priority directories are best-effort fallbacks.
            Err(err) if index == 0 => return Err(err),
            Err(_) => {}
        }
    }
    sockets.sort_by(|left, right| left.name.cmp(&right.name));
    sockets.dedup_by(|left, right| left.name == right.name);
    Ok(sockets)
}

#[cfg(not(any(windows, unix)))]
pub async fn discover_codex_pipes() -> Result<Vec<PipeInfo>> {
    Err(BridgeError::User(
        "browser bridge requires Windows named pipes or Unix sockets".into(),
    ))
}

#[cfg(unix)]
fn discover_unix_sockets_in(directory: &std::path::Path) -> Result<Vec<PipeInfo>> {
    if let Err(err) = std::fs::symlink_metadata(directory) {
        if err.kind() == std::io::ErrorKind::NotFound {
            return Ok(Vec::new());
        }
        return Err(err.into());
    }
    crate::pipe::validate_socket_directory(directory)?;
    let mut sockets = Vec::new();
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("sock") {
            continue;
        }
        let Some(uuid) = path
            .file_stem()
            .and_then(|value| value.to_str())
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        if crate::pipe::validate_unix_socket(&path).is_err() {
            continue;
        }
        sockets.push(PipeInfo {
            name: path.to_string_lossy().into_owned(),
            uuid: uuid.to_string(),
        });
    }
    sockets.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(sockets)
}

#[cfg(windows)]
async fn discover_codex_pipes_with_timeout(timeout_duration: Duration) -> Result<Vec<PipeInfo>> {
    let mut command = Command::new("powershell");
    command
        .kill_on_drop(true)
        .creation_flags(0x08000000) // CREATE_NO_WINDOW: never flash a console for discovery.
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
    transport_path(name)
}

/// Resolve a discovered transport name or explicit endpoint override.
pub fn transport_path(name: &str) -> String {
    #[cfg(windows)]
    {
        if name.starts_with(r"\\.\pipe\") {
            name.into()
        } else {
            format!(r"\\.\pipe\{name}")
        }
    }
    #[cfg(unix)]
    {
        if std::path::Path::new(name).is_absolute() {
            name.into()
        } else {
            let directory = unix_socket_directories()
                .into_iter()
                .next()
                .unwrap_or_else(|| std::path::PathBuf::from(DEFAULT_UNIX_SOCKET_DIRECTORY));
            format!("{}/{name}", directory.display())
        }
    }
    #[cfg(not(any(windows, unix)))]
    {
        name.into()
    }
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

    #[cfg(windows)]
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

    #[cfg(unix)]
    #[test]
    fn socket_path_keeps_absolute_overrides() {
        let primary = unix_socket_directories()
            .into_iter()
            .next()
            .expect("at least the default socket directory");
        assert_eq!(
            pipe_path("instance.sock"),
            primary.join("instance.sock").to_str().unwrap()
        );
        assert_eq!(
            pipe_path("/tmp/custom/instance.sock"),
            "/tmp/custom/instance.sock"
        );
    }

    #[cfg(unix)]
    #[test]
    fn explicit_socket_directory_replaces_the_fallback_list() {
        let directories = unix_socket_directories_with(|name| match name {
            "CODEX_BRIDGE_SOCKET_DIR" => Some("/run/codex-test".into()),
            _ => None,
        });
        assert_eq!(
            directories,
            vec![std::path::PathBuf::from("/run/codex-test")]
        );
    }

    #[cfg(unix)]
    #[test]
    fn runtime_directory_precedes_the_default_socket_directory() {
        let directories = unix_socket_directories_with(|name| match name {
            "XDG_RUNTIME_DIR" => Some("/run/user/7".into()),
            _ => None,
        });
        assert_eq!(
            directories,
            vec![
                std::path::PathBuf::from("/run/user/7/codex-browser-use"),
                std::path::PathBuf::from(DEFAULT_UNIX_SOCKET_DIRECTORY),
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn unix_discovery_only_returns_private_real_sockets() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        use std::os::unix::net::UnixListener;
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let valid = root.path().join("valid.sock");
        let _valid_listener = UnixListener::bind(&valid).unwrap();
        std::fs::set_permissions(&valid, std::fs::Permissions::from_mode(0o600)).unwrap();
        let shared = root.path().join("shared.sock");
        let _shared_listener = UnixListener::bind(&shared).unwrap();
        std::fs::set_permissions(&shared, std::fs::Permissions::from_mode(0o666)).unwrap();
        std::fs::write(root.path().join("file.sock"), b"not a socket").unwrap();
        symlink(&valid, root.path().join("link.sock")).unwrap();
        let _other_listener = UnixListener::bind(root.path().join("other.extension")).unwrap();

        let sockets = discover_unix_sockets_in(root.path()).unwrap();
        assert_eq!(sockets.len(), 1);
        assert_eq!(sockets[0].name, valid.to_str().unwrap());
        assert_eq!(sockets[0].uuid, "valid");
        assert!(discover_unix_sockets_in(&root.path().join("missing"))
            .unwrap()
            .is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn unix_discovery_rejects_unsafe_directories_but_accepts_official_sticky_mode() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let root = tempfile::tempdir().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let directory = root.path().join("sockets");
        std::fs::create_dir(&directory).unwrap();
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o777)).unwrap();
        assert!(discover_unix_sockets_in(&directory).is_err());
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o1777)).unwrap();
        assert!(discover_unix_sockets_in(&directory).unwrap().is_empty());
        let alias = root.path().join("alias");
        symlink(&directory, &alias).unwrap();
        assert!(discover_unix_sockets_in(&alias).is_err());
        let nested = directory.join("nested");
        std::fs::create_dir(&nested).unwrap();
        assert!(discover_unix_sockets_in(&alias.join("nested")).is_err());
    }
}
