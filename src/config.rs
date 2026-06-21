//! Optional TOML config file for persistent defaults.
//!
//! Lookup order (first hit wins): `CODEX_BRIDGE_CONFIG` env path, then
//! `./.codex-browser-bridge.toml`. A missing file is fine — returns the default
//! (empty) config. A malformed file logs a warning and is ignored, so a typo
//! never bricks startup.
//!
//! Config precedence overall: CLI flags > config file > env > built-in default
//! (applied in `main.rs`).

use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Default, Deserialize)]
pub struct Config {
    /// Tool profile name: "basic" | "network" | "full".
    #[serde(default)]
    pub profile: Option<String>,
    /// Base directory `codex_file_input` may upload from.
    #[serde(default)]
    pub upload_base: Option<String>,
}

impl Config {
    /// Load config from the first available source, or `Default::default()`.
    pub fn load() -> Self {
        if let Some(cfg) = std::env::var_os("CODEX_BRIDGE_CONFIG")
            .as_deref()
            .map(Path::new)
            .and_then(Self::load_from)
        {
            return cfg;
        }
        Self::load_from(Path::new(".codex-browser-bridge.toml")).unwrap_or_default()
    }

    fn load_from(path: &Path) -> Option<Self> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(_) => return None, // missing file is the common, silent path
        };
        tracing::debug!(path = %path.display(), "loaded config file");
        match toml::from_str(&text) {
            Ok(cfg) => Some(cfg),
            Err(err) => {
                tracing::warn!(
                    path = %path.display(),
                    error = %err,
                    "failed to parse config file, ignoring"
                );
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_profile_and_upload_base() {
        let cfg: Config = toml::from_str(
            r#"
            profile = "network"
            upload_base = "C:/uploads"
            "#,
        )
        .unwrap();
        assert_eq!(cfg.profile.as_deref(), Some("network"));
        assert_eq!(cfg.upload_base.as_deref(), Some("C:/uploads"));
    }

    #[test]
    fn empty_file_yields_default() {
        let cfg: Config = toml::from_str("").unwrap();
        assert!(cfg.profile.is_none());
        assert!(cfg.upload_base.is_none());
    }

    #[test]
    fn unknown_keys_are_ignored() {
        // Extra keys must not break parsing (forward-compat for future fields).
        let cfg: Config = toml::from_str(
            r#"
            profile = "basic"
            future_field = true
            "#,
        )
        .unwrap();
        assert_eq!(cfg.profile.as_deref(), Some("basic"));
    }
}
