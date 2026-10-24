//! Strict TOML defaults and browser-selector precedence.
//!
//! An explicit `CODEX_BRIDGE_CONFIG` path is authoritative and must be readable.
//! Only a missing working-directory `.codex-browser-bridge.toml` is optional.
//! Invalid configuration is never replaced with defaults. CLI > environment >
//! file > defaults applies to settings; family and instance form one selector.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::registry::BrowserSelector;

pub const MIN_CONTENT_BYTES: usize = 1024;
pub const MAX_CONTENT_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Tool profile name: "basic" | "network" | "full" (not a browser profile).
    pub profile: Option<String>,
    /// Preferred Chromium browser family, for example "edge" or "chrome".
    pub browser_family: Option<String>,
    /// Exact extensionInstanceId when multiple instances of a family are live.
    pub browser_instance: Option<String>,
    /// Base directory `codex_file_input` may upload from.
    pub upload_base: Option<String>,
    /// Maximum bytes per MCP text content item (1024..=8 MiB).
    pub max_text_bytes: Option<usize>,
    /// Maximum bytes per MCP base64 image content item (1024..=8 MiB).
    pub max_image_bytes: Option<usize>,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConfigSource {
    Cli,
    Environment,
    Config,
    Default,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct BrowserSelection {
    pub selector: BrowserSelector,
    pub source: ConfigSource,
}

impl Config {
    pub fn load() -> Result<Self> {
        if let Some(path) = std::env::var_os("CODEX_BRIDGE_CONFIG") {
            return Self::load_from(Path::new(&path));
        }
        let path = Path::new(".codex-browser-bridge.toml");
        match std::fs::read_to_string(path) {
            Ok(text) => Self::parse(&text).context("invalid default configuration file"),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(err) => Err(err).context("cannot read default configuration file"),
        }
    }

    /// Load a required configuration file. Paths are explicit; no fallback.
    pub fn load_from(path: &Path) -> Result<Self> {
        let text =
            std::fs::read_to_string(path).context("cannot read explicit configuration file")?;
        Self::parse(&text).context("invalid explicit configuration file")
    }

    pub fn parse(text: &str) -> Result<Self> {
        // TOML errors may include source lines or values. Configuration can
        // contain private paths; report only the location, not its contents.
        let mut config: Self = toml::from_str(text).map_err(|err: toml::de::Error| {
            let line = err.span().map(|span| {
                text.as_bytes()[..span.start.min(text.len())]
                    .iter()
                    .filter(|b| **b == b'\n')
                    .count()
                    + 1
            });
            anyhow::anyhow!(
                "invalid TOML or unknown configuration key{}",
                line.map(|line| format!(" at line {line}"))
                    .unwrap_or_default()
            )
        })?;
        config.normalize_and_validate()?;
        Ok(config)
    }

    /// Read explicit environment settings without silently discarding errors.
    pub fn from_env() -> Result<Self> {
        let mut config = Self {
            profile: env_string("CODEX_BRIDGE_PROFILE")?,
            browser_family: env_string("CODEX_BRIDGE_BROWSER_FAMILY")?,
            browser_instance: env_string("CODEX_BRIDGE_BROWSER_INSTANCE")?,
            upload_base: env_string("CODEX_BRIDGE_UPLOAD_BASE")?,
            max_text_bytes: env_usize("CODEX_BRIDGE_MAX_TEXT_BYTES")?,
            max_image_bytes: env_usize("CODEX_BRIDGE_MAX_IMAGE_BYTES")?,
        };
        config
            .normalize_and_validate()
            .context("invalid environment configuration")?;
        Ok(config)
    }

    fn normalize_and_validate(&mut self) -> Result<()> {
        let selector =
            BrowserSelector::new(self.browser_family.clone(), self.browser_instance.clone());
        selector.validate()?;
        self.browser_family = selector.family;
        self.browser_instance = selector.instance_id;
        if let Some(profile) = self.profile.as_mut() {
            *profile = profile.trim().to_ascii_lowercase();
            if !matches!(profile.as_str(), "basic" | "network" | "full") {
                bail!("profile must be basic, network, or full");
            }
        }
        if self
            .upload_base
            .as_ref()
            .is_some_and(|base| base.trim().is_empty())
        {
            bail!("upload_base must not be empty");
        }
        if let Some(limit) = self.max_text_bytes {
            validate_content_bytes("max_text_bytes", limit)?;
        }
        if let Some(limit) = self.max_image_bytes {
            validate_content_bytes("max_image_bytes", limit)?;
        }
        Ok(())
    }

    /// A higher-precedence family OR instance replaces the whole lower layer.
    /// In particular, `--browser-family chrome` never retains a file's Edge ID.
    pub fn browser_selection(
        &self,
        cli_family: Option<String>,
        cli_instance: Option<String>,
        environment: &Self,
    ) -> Result<BrowserSelection> {
        let (family, instance, source) = if cli_family.is_some() || cli_instance.is_some() {
            (cli_family, cli_instance, ConfigSource::Cli)
        } else if environment.browser_family.is_some() || environment.browser_instance.is_some() {
            (
                environment.browser_family.clone(),
                environment.browser_instance.clone(),
                ConfigSource::Environment,
            )
        } else if self.browser_family.is_some() || self.browser_instance.is_some() {
            (
                self.browser_family.clone(),
                self.browser_instance.clone(),
                ConfigSource::Config,
            )
        } else {
            (None, None, ConfigSource::Default)
        };
        let selector = BrowserSelector::new(family, instance);
        selector.validate()?;
        Ok(BrowserSelection { selector, source })
    }
}

pub fn validate_content_bytes(name: &str, value: usize) -> Result<()> {
    if !(MIN_CONTENT_BYTES..=MAX_CONTENT_BYTES).contains(&value) {
        bail!("{name} must be between {MIN_CONTENT_BYTES} and {MAX_CONTENT_BYTES} bytes");
    }
    Ok(())
}

fn env_string(name: &str) -> Result<Option<String>> {
    match std::env::var(name) {
        Ok(value) => Ok(Some(value)),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => bail!("{name} must be valid Unicode"),
    }
}

fn env_usize(name: &str) -> Result<Option<usize>> {
    env_string(name)?
        .map(|value| {
            value
                .parse()
                .with_context(|| format!("{name} must be a non-negative integer"))
        })
        .transpose()
}
