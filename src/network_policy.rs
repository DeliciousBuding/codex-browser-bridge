//! Browser Use network policy carried on `attach`.
//!
//! Extension 1.26+ reads `params.networkPolicy` while attaching a tab and
//! throws `Cannot read properties of undefined (reading 'timeoutMs')` when the
//! field is absent. The official client always sends `{constraints: []}`, which
//! the extension treats as "no policy installed" and returns early.
//!
//! The shapes below mirror the extension's accepted contract exactly:
//! `{timeoutMs?: 1..=30000, constraints: [{kind, mode, patterns}]}`. Budgets
//! (policy count, pattern count, input bytes) are the extension's own limits,
//! enforced here first so a rejected policy fails before attach detaches the
//! debugger.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::error::{BridgeError, Result};

/// Extension `Hp`: maximum number of constraint entries.
pub const MAX_CONSTRAINTS: usize = 128;
/// Extension `Gp`: maximum number of patterns across all constraints.
pub const MAX_PATTERNS: usize = 4096;
/// Extension `Tc`: maximum total bytes of pattern text.
pub const MAX_PATTERN_BYTES: usize = 64 * 1024;
/// Extension accepts 1..=30000 milliseconds for the install acknowledgement.
pub const MIN_TIMEOUT_MS: u64 = 1;
pub const MAX_TIMEOUT_MS: u64 = 30_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PatternKind {
    Domains,
    Origins,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConstraintMode {
    Allowlist,
    Denylist,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Constraint {
    pub kind: PatternKind,
    pub mode: ConstraintMode,
    pub patterns: Vec<String>,
}

/// A complete `networkPolicy` payload. `constraints` empty means "no policy":
/// the extension normalizes it to `defaultAction=allow` with no rules and
/// returns without touching declarativeNetRequest.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NetworkPolicy {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    #[serde(default)]
    pub constraints: Vec<Constraint>,
}

impl NetworkPolicy {
    /// Parse from a JSON string (CLI flag or `CODEX_BRIDGE_NETWORK_POLICY`).
    pub fn from_json(text: &str) -> Result<Self> {
        let policy: Self = serde_json::from_str(text).map_err(|err| {
            // The message may echo private patterns; report the location only.
            let line = err
                .to_string()
                .split_once(" at line ")
                .map(|(_, rest)| rest.to_string());
            BridgeError::User(match line {
                Some(line) => format!("invalid network policy JSON at line {}", trim_line(&line)),
                None => "invalid network policy JSON: expected {\"timeout_ms\": 1..30000, \"constraints\": [{\"kind\": \"domains\"|\"origins\", \"mode\": \"allowlist\"|\"denylist\", \"patterns\": [...]}]}".into(),
            })
        })?;
        policy.validate()?;
        Ok(policy)
    }

    /// Reject anything the extension would reject, before attach detaches the
    /// current debugger session.
    pub fn validate(&self) -> Result<()> {
        if let Some(timeout_ms) = self.timeout_ms {
            if !(MIN_TIMEOUT_MS..=MAX_TIMEOUT_MS).contains(&timeout_ms) {
                return Err(BridgeError::User(format!(
                    "network policy timeout_ms must be between {MIN_TIMEOUT_MS} and {MAX_TIMEOUT_MS} milliseconds"
                )));
            }
        }
        if self.constraints.len() > MAX_CONSTRAINTS {
            return Err(BridgeError::User(format!(
                "network policy permits at most {MAX_CONSTRAINTS} constraints, got {}",
                self.constraints.len()
            )));
        }
        let mut patterns = 0usize;
        let mut bytes = 0usize;
        for constraint in &self.constraints {
            for pattern in &constraint.patterns {
                if pattern.trim().is_empty() {
                    return Err(BridgeError::User(
                        "network policy patterns must not be blank".into(),
                    ));
                }
                if pattern.contains('\0') || pattern.chars().any(char::is_whitespace) {
                    return Err(BridgeError::User(
                        "network policy patterns must not contain whitespace or NUL".into(),
                    ));
                }
                patterns += 1;
                bytes += pattern.len();
            }
        }
        if patterns > MAX_PATTERNS {
            return Err(BridgeError::User(format!(
                "network policy permits at most {MAX_PATTERNS} patterns, got {patterns}"
            )));
        }
        if bytes > MAX_PATTERN_BYTES {
            return Err(BridgeError::User(format!(
                "network policy pattern text permits at most {MAX_PATTERN_BYTES} bytes, got {bytes}"
            )));
        }
        Ok(())
    }

    /// The exact `networkPolicy` object the extension expects.
    pub fn to_payload(&self) -> Value {
        let mut payload = json!({ "constraints": self.constraints });
        if let Some(timeout_ms) = self.timeout_ms {
            payload["timeoutMs"] = json!(timeout_ms);
        }
        payload
    }
}

fn trim_line(line: &str) -> String {
    line.trim()
        .split(|c: char| c.is_whitespace())
        .next()
        .unwrap_or_default()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_policy_matches_official_default() {
        let policy = NetworkPolicy::default();
        policy.validate().unwrap();
        assert_eq!(policy.to_payload(), json!({"constraints": []}));
    }

    #[test]
    fn accepts_domains_and_origins() {
        let policy = NetworkPolicy::from_json(
            r#"{"timeout_ms":5000,"constraints":[
                {"kind":"domains","mode":"denylist","patterns":["example.com","*.example.org"]},
                {"kind":"origins","mode":"allowlist","patterns":["https://api.example.com:8443"]}
            ]}"#,
        )
        .unwrap();
        let payload = policy.to_payload();
        assert_eq!(payload["timeoutMs"], 5000);
        assert_eq!(payload["constraints"][1]["kind"], "origins");
        assert_eq!(payload["constraints"][1]["mode"], "allowlist");
    }

    #[test]
    fn rejects_unknown_fields_and_bad_enums() {
        assert!(NetworkPolicy::from_json(r#"{"constraint":[]}"#).is_err());
        assert!(NetworkPolicy::from_json(
            r#"{"constraints":[{"kind":"hosts","mode":"allowlist","patterns":[]}]}"#
        )
        .is_err());
    }

    #[test]
    fn enforces_extension_budgets() {
        let too_many_constraints = NetworkPolicy {
            timeout_ms: None,
            constraints: (0..=MAX_CONSTRAINTS)
                .map(|i| Constraint {
                    kind: PatternKind::Domains,
                    mode: ConstraintMode::Denylist,
                    patterns: vec![format!("h{i}.example")],
                })
                .collect(),
        };
        assert!(too_many_constraints.validate().is_err());

        let too_many_patterns = NetworkPolicy {
            timeout_ms: None,
            constraints: vec![Constraint {
                kind: PatternKind::Domains,
                mode: ConstraintMode::Denylist,
                patterns: (0..=MAX_PATTERNS)
                    .map(|i| format!("h{i}.example"))
                    .collect(),
            }],
        };
        assert!(too_many_patterns.validate().is_err());

        for timeout_ms in [0, MAX_TIMEOUT_MS + 1] {
            let bad = NetworkPolicy {
                timeout_ms: Some(timeout_ms),
                constraints: Vec::new(),
            };
            assert!(
                bad.validate().is_err(),
                "timeout {timeout_ms} must be rejected"
            );
        }
    }

    #[test]
    fn rejects_blank_or_whitespace_patterns() {
        for pattern in ["", "   ", "a b"] {
            let policy = NetworkPolicy {
                timeout_ms: None,
                constraints: vec![Constraint {
                    kind: PatternKind::Domains,
                    mode: ConstraintMode::Denylist,
                    patterns: vec![pattern.into()],
                }],
            };
            assert!(
                policy.validate().is_err(),
                "pattern {pattern:?} must be rejected"
            );
        }
    }
}
