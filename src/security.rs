use std::path::{Path, PathBuf};

use crate::error::{BridgeError, Result};

/// Validate URL schemes accepted by navigation-like tools.
pub fn validate_url(raw_url: &str) -> Result<()> {
    let trimmed = raw_url.trim();
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        return Ok(());
    }

    let scheme = trimmed
        .split_once(':')
        .map(|(scheme, _)| scheme)
        .filter(|scheme| !scheme.is_empty())
        .unwrap_or("missing");
    Err(BridgeError::User(format!(
        "blocked URL scheme {scheme:?}; only http:// and https:// are allowed"
    )))
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
