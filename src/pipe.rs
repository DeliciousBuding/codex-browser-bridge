use crate::error::{BridgeError, Result};

/// Shared transport interface, including in-memory streams used by tests.
pub trait TransportIo: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send {}
impl<T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send> TransportIo for T {}

pub type TransportStream = Box<dyn TransportIo>;
/// Compatibility name for callers using the original named-pipe API.
pub type PipeStream = TransportStream;

const DIAL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

pub async fn dial_transport(path: &str) -> Result<TransportStream> {
    #[cfg(windows)]
    {
        dial_named_pipe_with_timeout(path, DIAL_TIMEOUT).await
    }
    #[cfg(unix)]
    {
        let before = validate_unix_socket(std::path::Path::new(path))?;
        let stream = tokio::time::timeout(DIAL_TIMEOUT, tokio::net::UnixStream::connect(path))
            .await
            .map_err(|_| BridgeError::Timeout(format!("dial socket {path}")))??;
        let after = validate_unix_socket(std::path::Path::new(path))?;
        use std::os::unix::fs::MetadataExt;
        if before.dev() != after.dev() || before.ino() != after.ino() {
            return Err(BridgeError::Connection(
                "socket changed while connecting".into(),
            ));
        }
        if stream.peer_cred()?.uid() != current_uid() {
            return Err(BridgeError::Connection(
                "socket peer is not the current user".into(),
            ));
        }
        Ok(Box::new(stream))
    }
    #[cfg(not(any(windows, unix)))]
    {
        let _ = path;
        Err(BridgeError::User(
            "browser bridge requires Windows named pipes or Unix sockets".into(),
        ))
    }
}

/// Compatibility entry point; on Unix `path` is a socket filesystem path.
pub async fn dial_named_pipe(path: &str) -> Result<PipeStream> {
    dial_transport(path).await
}

#[cfg(windows)]
async fn dial_named_pipe_with_timeout(
    path: &str,
    dial_timeout: std::time::Duration,
) -> Result<PipeStream> {
    let deadline = tokio::time::Instant::now() + dial_timeout;

    loop {
        match tokio::net::windows::named_pipe::ClientOptions::new().open(path) {
            Ok(stream) => return Ok(Box::new(stream)),
            Err(err) => {
                let now = tokio::time::Instant::now();
                if now >= deadline {
                    return Err(BridgeError::Timeout(format!("dial pipe {path}: {err}")));
                }

                tokio::time::sleep_until(
                    (now + std::time::Duration::from_millis(50)).min(deadline),
                )
                .await;
            }
        }
    }
}

#[cfg(unix)]
fn current_uid() -> u32 {
    // geteuid has no preconditions and does not access memory.
    unsafe { libc::geteuid() }
}

/// Reject writable/shared endpoint directories and symlinks in their ancestry.
/// Root-owned sticky directories such as /tmp are safe ancestors, but the
/// Directories must belong to this user or root. Official installations may
/// use mode 1777, whose sticky bit prevents other users replacing our socket.
#[cfg(unix)]
pub(crate) fn validate_socket_directory(path: &std::path::Path) -> Result<()> {
    use std::os::unix::fs::MetadataExt;
    if !path.is_absolute()
        || path
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return Err(BridgeError::User(
            "socket path must be absolute without parent traversal".into(),
        ));
    }
    for ancestor in path.ancestors() {
        let metadata = std::fs::symlink_metadata(ancestor)?;
        let mode = metadata.mode();
        let owner = metadata.uid();
        if !metadata.is_dir()
            || metadata.file_type().is_symlink()
            || (owner != current_uid() && owner != 0)
            || (mode & 0o022 != 0 && mode & 0o1000 == 0)
        {
            return Err(BridgeError::User(format!(
                "unsafe socket directory: {}",
                ancestor.display()
            )));
        }
    }
    Ok(())
}

#[cfg(unix)]
pub(crate) fn validate_unix_socket(path: &std::path::Path) -> Result<std::fs::Metadata> {
    use std::os::unix::fs::{FileTypeExt, MetadataExt};
    let parent = path
        .parent()
        .ok_or_else(|| BridgeError::User("socket has no parent directory".into()))?;
    validate_socket_directory(parent)?;
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.file_type().is_socket()
        || metadata.uid() != current_uid()
        || metadata.mode() & 0o077 != 0
    {
        return Err(BridgeError::User(format!(
            "unsafe socket endpoint: {}",
            path.display()
        )));
    }
    Ok(metadata)
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
            .err()
            .expect("missing pipe must fail");

        assert!(err.to_string().contains("timeout waiting for dial pipe"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn relative_socket_path_is_rejected() {
        let err = dial_transport("codex-browser-use-test")
            .await
            .err()
            .expect("relative path must fail");
        assert!(err.to_string().contains("absolute"));
    }
}
