//! Discover and identify live browser-extension instances.
//!
//! Named-pipe names are transport details and are not stable browser
//! identities. The extension's `extensionInstanceId` is the stable selector
//! exposed back to agents; the pipe name is retained only for diagnostics and
//! for dialing the exact candidate that produced the metadata.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tokio::time::{timeout, Duration, Instant};
use uuid::Uuid;

use crate::client::verify_pipe_stream;
use crate::discovery;
use crate::error::{BridgeError, Result};
use crate::pipe::dial_transport;

const BROWSE_BUDGET: Duration = Duration::from_secs(10);
const GET_INFO_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_CONCURRENT_PROBES: usize = 16;

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct BrowserSelector {
    /// Chromium browser family, for example `chrome`, `edge`, or `brave`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
    /// Stable `metadata.extensionInstanceId` returned by `getInfo`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance_id: Option<String>,
}

impl BrowserSelector {
    pub fn new(family: Option<String>, instance_id: Option<String>) -> Self {
        Self {
            family: family.map(|value| value.trim().to_ascii_lowercase()),
            instance_id: instance_id.map(|value| value.trim().to_string()),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.family.is_none() && self.instance_id.is_none()
    }

    pub fn validate(&self) -> Result<()> {
        if let Some(family) = self.family.as_deref() {
            if family.is_empty()
                || !family
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
            {
                return Err(BridgeError::User(
                    "browser family must contain only ASCII letters, digits, '-' or '_'".into(),
                ));
            }
        }
        if let Some(instance_id) = self.instance_id.as_deref() {
            if instance_id.is_empty() {
                return Err(BridgeError::User(
                    "browser instance id must not be empty".into(),
                ));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct BrowserTransport {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub name: String,
    pub uuid: String,
    pub path: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct BrowserDescriptor {
    /// Stable public browser identifier. Always `extensionInstanceId`.
    #[serde(rename = "browser_id")]
    pub id: String,
    pub family: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extension_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_name: Option<String>,
    pub transport: BrowserTransport,
    pub capabilities: Value,
}

#[derive(Debug, Serialize)]
pub struct BrowserInventory {
    pub pipe_count: usize,
    pub probed_pipe_count: usize,
    pub unprobed_pipe_count: usize,
    pub failed_pipe_count: usize,
    pub browsers: Vec<BrowserDescriptor>,
}

impl BrowserDescriptor {
    pub(crate) fn from_info(info: &Value, pipe: discovery::PipeInfo) -> Result<Self> {
        if info.get("type").and_then(Value::as_str) != Some("extension") {
            return Err(BridgeError::Protocol(
                "getInfo did not identify a browser extension backend".into(),
            ));
        }

        let metadata = info.get("metadata");
        let family = info
            .get("family")
            .and_then(Value::as_str)
            .map(str::to_ascii_lowercase)
            .ok_or_else(|| BridgeError::Protocol("getInfo omitted browser family".into()))?;
        let extension_instance_id = metadata
            .and_then(|value| value.get("extensionInstanceId"))
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                BridgeError::Protocol(
                    "getInfo omitted metadata.extensionInstanceId; use --pipe only for an explicit legacy transport override"
                        .into(),
                )
            })?;

        Ok(Self {
            id: extension_instance_id.to_string(),
            family,
            name: info
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("Chromium")
                .to_string(),
            version: info
                .get("version")
                .and_then(Value::as_str)
                .map(str::to_string),
            extension_id: metadata
                .and_then(|value| value.get("extensionId"))
                .and_then(Value::as_str)
                .map(str::to_string),
            profile_name: info
                .get("profileName")
                .or_else(|| metadata.and_then(|value| value.get("profileName")))
                .and_then(Value::as_str)
                .map(str::to_string),
            transport: BrowserTransport {
                kind: discovery::transport_kind(),
                path: discovery::transport_path(&pipe.name),
                name: pipe.name.clone(),
                uuid: pipe.uuid,
            },
            capabilities: info.get("capabilities").cloned().unwrap_or(Value::Null),
        })
    }
}

/// Probe every discoverable browser-extension pipe, concurrently and with a
/// bounded overall deadline. Failed and stale pipe names are counted but are
/// not returned as browser choices.
pub async fn discover_browsers() -> Result<BrowserInventory> {
    let pipes = discovery::discover_codex_pipes().await?;
    let pipe_count = pipes.len();
    let deadline = Instant::now() + BROWSE_BUDGET;
    let permits = Arc::new(Semaphore::new(MAX_CONCURRENT_PROBES));
    let mut tasks = JoinSet::new();

    for pipe in pipes {
        let permits = Arc::clone(&permits);
        tasks.spawn(async move {
            let Ok(_permit) = permits.acquire_owned().await else {
                return None;
            };
            probe_pipe(pipe).await
        });
    }

    let mut browsers = Vec::new();
    let mut probed_pipe_count = 0;
    let mut failed_pipe_count = 0;

    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        let joined = match timeout(remaining, tasks.join_next()).await {
            Ok(Some(joined)) => joined,
            Ok(None) | Err(_) => break,
        };
        probed_pipe_count += 1;
        match joined {
            Ok(Some(browser)) => {
                // Keep duplicate instance ids. select_browser() treats them as
                // an ambiguity instead of choosing whichever pipe replied first.
                browsers.push(browser);
            }
            Ok(None) | Err(_) => failed_pipe_count += 1,
        }
    }

    let unprobed_pipe_count = pipe_count.saturating_sub(probed_pipe_count);
    tasks.abort_all();
    browsers.sort_by(|left, right| {
        left.family
            .cmp(&right.family)
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| left.id.cmp(&right.id))
    });

    Ok(BrowserInventory {
        pipe_count,
        probed_pipe_count,
        unprobed_pipe_count,
        failed_pipe_count,
        browsers,
    })
}

pub fn select_browser<'a>(
    browsers: &'a [BrowserDescriptor],
    selector: &BrowserSelector,
) -> Result<&'a BrowserDescriptor> {
    selector.validate()?;

    if let Some(instance_id) = selector.instance_id.as_deref() {
        let mut matches = browsers
            .iter()
            .filter(|browser| browser.id == instance_id)
            .filter(|browser| {
                selector
                    .family
                    .as_deref()
                    .is_none_or(|family| browser.family == family)
            });
        return match (matches.next(), matches.next()) {
            (Some(browser), None) => Ok(browser),
            (None, _) => Err(BridgeError::User(format!(
                "browser instance {instance_id:?} is not connected"
            ))),
            (Some(_), Some(_)) => Err(BridgeError::User(format!(
                "multiple live pipes reported browser instance {instance_id:?}"
            ))),
        };
    }

    if let Some(family) = selector.family.as_deref() {
        let matches: Vec<_> = browsers
            .iter()
            .filter(|browser| browser.family == family)
            .collect();
        return match matches.as_slice() {
            [browser] => Ok(*browser),
            [] => Err(BridgeError::User(format!(
                "no connected browser found for family {family:?}"
            ))),
            _ => Err(BridgeError::User(format!(
                "browser family {family:?} has {} connected instances; select one by browser_id: {}",
                matches.len(),
                matches
                    .iter()
                    .map(|browser| browser.id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))),
        };
    }

    match browsers {
        [browser] => Ok(browser),
        [] => Err(BridgeError::User(
            "no connected ChatGPT browser-extension instance found".into(),
        )),
        _ => Err(BridgeError::User(format!(
            "{} connected browser instances found; select one with --browser-instance, CODEX_BRIDGE_BROWSER_INSTANCE, config browser_instance, or codex_bind_browser",
            browsers.len()
        ))),
    }
}

/// An implicit choice is safe only after every discovered endpoint has been
/// identified. A failed probe is unknown, not proof that no browser is there.
/// Explicit instance selectors can proceed because dialing re-verifies that
/// identity; they never fall back to another profile.
pub fn select_browser_from_inventory<'a>(
    inventory: &'a BrowserInventory,
    selector: &BrowserSelector,
) -> Result<&'a BrowserDescriptor> {
    selector.validate()?;
    if selector.instance_id.is_none()
        && (inventory.unprobed_pipe_count != 0
            || inventory.failed_pipe_count != 0
            || inventory.probed_pipe_count != inventory.pipe_count)
    {
        return Err(BridgeError::User(format!(
            "browser inventory is incomplete ({} unprobed, {} failed endpoints); retry discovery or select an explicit browser_id",
            inventory.unprobed_pipe_count, inventory.failed_pipe_count
        )));
    }
    select_browser(&inventory.browsers, selector)
}

async fn probe_pipe(pipe: discovery::PipeInfo) -> Option<BrowserDescriptor> {
    let path = discovery::pipe_path(&pipe.name);
    let mut stream = dial_transport(&path).await.ok()?;
    let session_id = Uuid::new_v4().to_string();
    let turn_id = Uuid::new_v4().to_string();
    let raw = timeout(
        GET_INFO_TIMEOUT,
        verify_pipe_stream(&mut stream, &session_id, &turn_id),
    )
    .await
    .ok()?
    .ok()?;
    let info = serde_json::from_str::<Value>(raw.get()).ok()?;
    BrowserDescriptor::from_info(&info, pipe).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn browser(id: &str, family: &str) -> BrowserDescriptor {
        BrowserDescriptor {
            id: id.into(),
            family: family.into(),
            name: family.into(),
            version: None,
            extension_id: None,
            profile_name: None,
            transport: BrowserTransport {
                kind: discovery::transport_kind(),
                path: discovery::transport_path(&format!("codex-browser-use\\{id}")),
                name: format!("codex-browser-use\\{id}"),
                uuid: id.into(),
            },
            capabilities: Value::Null,
        }
    }

    #[test]
    fn parses_current_extension_identity() {
        let info = serde_json::json!({
            "type": "extension",
            "family": "edge",
            "name": "Edge",
            "version": "1.26.1020.11845",
            "metadata": {
                "extensionId": "extension-id",
                "extensionInstanceId": "instance-id",
                "profileName": "Work"
            },
            "capabilities": {"tab": [{"id": "pageAssets"}]}
        });
        let descriptor = BrowserDescriptor::from_info(
            &info,
            discovery::PipeInfo {
                name: "codex-browser-use\\pipe-id".into(),
                uuid: "pipe-id".into(),
            },
        )
        .unwrap();

        assert_eq!(descriptor.id, "instance-id");
        assert_eq!(descriptor.family, "edge");
        assert_eq!(descriptor.profile_name.as_deref(), Some("Work"));
        assert_eq!(descriptor.transport.uuid, "pipe-id");
    }

    #[test]
    fn rejects_transport_only_identity() {
        let info = serde_json::json!({
            "type": "extension",
            "family": "edge",
            "name": "Edge",
            "metadata": {"extensionId": "extension-id"}
        });
        let err = BrowserDescriptor::from_info(
            &info,
            discovery::PipeInfo {
                name: "codex-browser-use\\pipe-id".into(),
                uuid: "pipe-id".into(),
            },
        )
        .unwrap_err();

        assert!(err.to_string().contains("extensionInstanceId"));
    }

    #[test]
    fn ambiguous_family_requires_an_instance() {
        let browsers = vec![browser("one", "edge"), browser("two", "edge")];
        let err = select_browser(&browsers, &BrowserSelector::new(Some("edge".into()), None))
            .unwrap_err();

        assert!(err.to_string().contains("2 connected instances"));
    }

    #[test]
    fn instance_id_selects_the_exact_browser() {
        let browsers = vec![browser("one", "edge"), browser("two", "edge")];
        let selected = select_browser(
            &browsers,
            &BrowserSelector::new(Some("edge".into()), Some("two".into())),
        )
        .unwrap();

        assert_eq!(selected.id, "two");
    }

    #[test]
    fn a_single_browser_is_the_only_implicit_choice() {
        let browsers = vec![browser("one", "chrome")];
        assert_eq!(
            select_browser(&browsers, &BrowserSelector::default())
                .unwrap()
                .id,
            "one"
        );
    }
}
