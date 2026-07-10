use std::path::{Path, PathBuf};

use crate::error::{BridgeError, Result};

/// Validate and normalize URL strings accepted by navigation-like tools.
pub fn validate_url(raw_url: &str) -> Result<String> {
    let trimmed = raw_url.trim();
    let lower = trimmed.to_ascii_lowercase();

    if trimmed
        .chars()
        .any(|ch| ch.is_ascii_control() || ch.is_ascii_whitespace() || ch == '\\')
    {
        return Err(BridgeError::User(
            "blocked malformed URL; raw whitespace, control characters, and backslashes are not allowed"
                .into(),
        ));
    }

    let authority = if lower.starts_with("http://") {
        &trimmed["http://".len()..]
    } else if lower.starts_with("https://") {
        &trimmed["https://".len()..]
    } else {
        let scheme = trimmed
            .split_once(':')
            .map(|(scheme, _)| scheme)
            .filter(|scheme| !scheme.is_empty())
            .unwrap_or("missing");
        return Err(BridgeError::User(format!(
            "blocked URL scheme {scheme:?}; only http:// and https:// are allowed"
        )));
    };

    let host = authority.split(['/', '?', '#']).next().unwrap_or_default();
    if host.is_empty() {
        return Err(BridgeError::User(
            "blocked malformed URL; http:// and https:// URLs must include a host".into(),
        ));
    }

    Ok(trimmed.to_string())
}

pub fn validate_cookie_name(name: &str) -> Result<String> {
    let trimmed = name.trim();
    if trimmed.is_empty()
        || trimmed.chars().any(|ch| {
            ch.is_ascii_control()
                || ch.is_ascii_whitespace()
                || matches!(
                    ch,
                    '(' | ')'
                        | '<'
                        | '>'
                        | '@'
                        | ','
                        | ';'
                        | ':'
                        | '\\'
                        | '"'
                        | '/'
                        | '['
                        | ']'
                        | '?'
                        | '='
                        | '{'
                        | '}'
                )
        })
    {
        return Err(BridgeError::User(
            "blocked malformed cookie name; use an RFC6265 token without separators or control characters"
                .into(),
        ));
    }
    Ok(trimmed.to_string())
}

pub fn validate_cookie_value(value: &str) -> Result<String> {
    if value
        .chars()
        .any(|ch| ch.is_ascii_control() || matches!(ch, ';' | '\u{7f}'))
    {
        return Err(BridgeError::User(
            "blocked malformed cookie value; control characters and semicolons are not allowed"
                .into(),
        ));
    }
    Ok(value.to_string())
}

pub fn validate_cookie_domain(domain: &str) -> Result<String> {
    let trimmed = domain.trim();
    let host = trimmed.strip_prefix('.').unwrap_or(trimmed);
    if host.is_empty()
        || trimmed.contains("://")
        || trimmed.chars().any(|ch| {
            ch.is_ascii_control()
                || ch.is_ascii_whitespace()
                || matches!(ch, '/' | '\\' | ':' | '?' | '#')
        })
        || host.split('.').any(|label| {
            label.is_empty()
                || label.starts_with('-')
                || label.ends_with('-')
                || !label
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
        })
    {
        return Err(BridgeError::User(
            "blocked malformed cookie domain; use a host name without scheme, path, whitespace, or control characters"
                .into(),
        ));
    }
    Ok(trimmed.to_ascii_lowercase())
}

pub fn validate_cookie_path(path: &str) -> Result<String> {
    if !path.starts_with('/')
        || path
            .chars()
            .any(|ch| ch.is_ascii_control() || ch.is_ascii_whitespace())
    {
        return Err(BridgeError::User(
            "blocked malformed cookie path; path must start with '/' and contain no whitespace or control characters"
                .into(),
        ));
    }
    Ok(path.to_string())
}

pub fn validate_cookie_same_site(same_site: &str) -> Result<String> {
    match same_site {
        "Strict" | "Lax" | "None" => Ok(same_site.to_string()),
        _ => Err(BridgeError::User(
            "blocked malformed cookie sameSite; expected Strict, Lax, or None".into(),
        )),
    }
}

/// Validate a file path for upload safety.
///
/// Checks (in order):
/// 1. Must be absolute (no working-directory dependent uploads)
/// 2. Canonicalize (resolves `..`, `.`, symlinks, UNC prefixes on Windows)
/// 3. Must reside under `allowed_base` (path traversal prevention)
/// 4. Must be a regular file (not directory, not device)
/// 5. Must be ≤ 10 MB
/// 6. Must be readable
pub fn validate_file_path(path: &str, allowed_base: &Path) -> Result<PathBuf> {
    let raw_path = Path::new(path);
    if !raw_path.is_absolute() {
        return Err(BridgeError::User(format!(
            "Upload path must be absolute: '{}'",
            sanitize_for_log(path)
        )));
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cookie_name_rejects_separators_and_controls() {
        assert_eq!(validate_cookie_name(" sid ").unwrap(), "sid");
        assert!(validate_cookie_name("bad name").is_err());
        assert!(validate_cookie_name("bad;name").is_err());
        assert!(validate_cookie_name("bad\nname").is_err());
    }

    #[test]
    fn cookie_value_rejects_controls_and_semicolon() {
        assert_eq!(validate_cookie_value("abc=123").unwrap(), "abc=123");
        assert!(validate_cookie_value("abc;123").is_err());
        assert!(validate_cookie_value("abc\r123").is_err());
    }

    #[test]
    fn cookie_domain_rejects_schemes_paths_and_bad_labels() {
        assert_eq!(
            validate_cookie_domain(".Example.COM").unwrap(),
            ".example.com"
        );
        assert!(validate_cookie_domain("https://example.com").is_err());
        assert!(validate_cookie_domain("example.com/path").is_err());
        assert!(validate_cookie_domain("-bad.example").is_err());
        assert!(validate_cookie_domain("bad..example").is_err());
    }

    #[test]
    fn cookie_path_and_same_site_are_strict() {
        assert_eq!(validate_cookie_path("/account").unwrap(), "/account");
        assert!(validate_cookie_path("account").is_err());
        assert!(validate_cookie_path("/bad path").is_err());
        assert_eq!(validate_cookie_same_site("Lax").unwrap(), "Lax");
        assert!(validate_cookie_same_site("lax").is_err());
    }
}
