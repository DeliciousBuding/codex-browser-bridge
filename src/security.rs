use std::path::{Path, PathBuf};

use crate::error::{BridgeError, Result};

/// Validate a URL scheme against dangerous protocols.
/// Moved from browser.rs — centralized security module.
pub fn validate_url(raw_url: &str) -> Result<()> {
    let lower = raw_url.trim().to_ascii_lowercase();
    for scheme in [
        "file:",
        "javascript:",
        "data:",
        "vbscript:",
        "about:",
        "chrome:",
        "edge:",
    ] {
        if lower.starts_with(scheme) {
            return Err(BridgeError::User(format!("blocked URL scheme {scheme:?}")));
        }
    }
    Ok(())
}

/// Validate a file path for upload safety.
///
/// Checks (in order):
/// 1. Canonicalize (resolves `..`, `.`, symlinks, UNC prefixes on Windows)
/// 2. Must reside under `allowed_base` (path traversal prevention)
/// 3. Must be a regular file (not directory, not device)
/// 4. Must be ≤ 10 MB
/// 5. Must be readable
pub fn validate_file_path(path: &str, allowed_base: &Path) -> Result<PathBuf> {
    let canonical = std::fs::canonicalize(path).map_err(|e| {
        BridgeError::User(format!(
            "Cannot resolve path '{}' (use absolute paths within the allowed directory): {e}",
            sanitize_for_log(path)
        ))
    })?;

    if !canonical.starts_with(allowed_base) {
        return Err(BridgeError::User(format!(
            "Path traversal denied: '{}' resolves outside the allowed upload directory",
            sanitize_for_log(path)
        )));
    }

    let meta = canonical.metadata().map_err(|e| {
        BridgeError::User(format!(
            "Cannot stat file '{}': {e}",
            sanitize_for_log(path)
        ))
    })?;

    if !meta.is_file() {
        return Err(BridgeError::User(format!(
            "Not a regular file: '{}'",
            sanitize_for_log(path)
        )));
    }

    const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024; // 10 MB
    if meta.len() > MAX_FILE_SIZE {
        return Err(BridgeError::User(format!(
            "File too large: {} bytes (max {} bytes)",
            meta.len(),
            MAX_FILE_SIZE
        )));
    }

    // Readability check
    std::fs::File::open(&canonical).map_err(|e| {
        BridgeError::User(format!(
            "File not readable '{}': {e}",
            sanitize_for_log(path)
        ))
    })?;

    Ok(canonical)
}

fn sanitize_for_log(s: &str) -> String {
    s.replace('\n', "\\n").replace('\r', "\\r")
}
