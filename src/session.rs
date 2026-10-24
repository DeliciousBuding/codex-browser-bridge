//! Instance-bound CLI continuation and OS-backed single-owner access.

use anyhow::{bail, Context, Result};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};

use crate::registry::BrowserSelector;

const VERSION: u32 = 2;
const MAX_SESSION_BYTES: u64 = 16 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BrowserIdentity {
    pub family: String,
    pub instance_id: String,
}

impl BrowserIdentity {
    pub fn from_info(info: &serde_json::Value) -> Result<Self> {
        if info["type"] != "extension" {
            bail!("session continuation requires an identified browser extension");
        }
        let family = info["family"].as_str().context("browser omitted family")?;
        let instance_id = info["metadata"]["extensionInstanceId"]
            .as_str()
            .context("browser omitted extensionInstanceId; continuation is unavailable")?;
        let selector = BrowserSelector::new(Some(family.into()), Some(instance_id.into()));
        selector.validate()?;
        Ok(Self {
            family: selector.family.unwrap(),
            instance_id: selector.instance_id.unwrap(),
        })
    }

    pub fn selector(&self, requested: &BrowserSelector) -> Result<BrowserSelector> {
        if requested
            .family
            .as_deref()
            .is_some_and(|v| v != self.family)
            || requested
                .instance_id
                .as_deref()
                .is_some_and(|v| v != self.instance_id)
        {
            bail!("browser selector conflicts with the saved session identity; use the original instance or a different session file");
        }
        Ok(BrowserSelector::new(
            Some(self.family.clone()),
            Some(self.instance_id.clone()),
        ))
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SessionFile {
    version: u32,
    pub browser: BrowserIdentity,
    pub session_id: String,
    pub turn_id: String,
}

pub fn load_session(path: &Path) -> Result<Option<SessionFile>> {
    use std::io::Read;
    let file = match File::open(path) {
        Ok(file) => file,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err).context("cannot read session file"),
    };
    let mut bytes = Vec::new();
    file.take(MAX_SESSION_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_SESSION_BYTES {
        bail!("session file exceeds 16 KiB; preserve it and recover the original turn explicitly");
    }
    let parsed: SessionFile = serde_json::from_slice(&bytes).map_err(|_| {
        anyhow::anyhow!("invalid or legacy session file; preserve it and finalize the original browser turn before starting a new continuation")
    })?;
    if parsed.version != VERSION || parsed.session_id.is_empty() || parsed.turn_id.is_empty() {
        bail!("unsupported or empty session identity; recover the original turn explicitly");
    }
    parsed
        .browser
        .selector(&BrowserSelector::default())?
        .validate()?;
    Ok(Some(parsed))
}

pub fn save_session(
    path: &Path,
    browser: BrowserIdentity,
    session_id: &str,
    turn_id: &str,
) -> Result<()> {
    browser.selector(&BrowserSelector::default())?.validate()?;
    if session_id.is_empty() || turn_id.is_empty() {
        bail!("cannot persist empty session or turn identity");
    }
    let payload = serde_json::to_vec(&SessionFile {
        version: VERSION,
        browser,
        session_id: session_id.into(),
        turn_id: turn_id.into(),
    })?;
    if payload.len() as u64 > MAX_SESSION_BYTES {
        bail!("session identity exceeds 16 KiB");
    }
    let temporary = path.with_extension(format!(
        "tmp-{}-{}",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    let result = (|| {
        use std::io::Write;
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        file.write_all(&payload)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

pub fn clear_session(path: &Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err).context("could not remove browser session file"),
    }
}

/// Keep the sidecar inode stable: unlinking an unlocked sidecar races waiters
/// and permits two independent locks. Process exit releases the OS lock; the
/// empty sidecar is not an occupied lock and contains no session state.
pub struct SessionLock {
    _file: File,
    pub path: PathBuf,
}

impl SessionLock {
    pub fn acquire(path: &Path) -> Result<Self> {
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let parent = parent
            .canonicalize()
            .context("session file parent directory must exist")?;
        let filename = path.file_name().context("session file must name a file")?;
        let path = parent.join(filename);
        if std::fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) {
            bail!("session file symlinks are not supported; use its actual path");
        }
        let mut sidecar = path.as_os_str().to_os_string();
        sidecar.push(".lock");
        let sidecar = PathBuf::from(sidecar);
        if std::fs::symlink_metadata(&sidecar).is_ok_and(|m| m.file_type().is_symlink()) {
            bail!("session lock symlinks are not supported");
        }
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(sidecar).context("cannot open session lock")?;
        file.try_lock_exclusive().context(
            "session file is already in use or cannot be locked; wait for its owner to exit",
        )?;
        Ok(Self { _file: file, path })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity() -> BrowserIdentity {
        BrowserIdentity {
            family: "edge".into(),
            instance_id: "instance-a".into(),
        }
    }

    #[test]
    fn roundtrip_updates_atomically_and_keeps_identity() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.json");
        let guard = SessionLock::acquire(&path).unwrap();
        assert!(load_session(&path).unwrap().is_none());
        save_session(&path, identity(), "s1", "t1").unwrap();
        save_session(&path, identity(), "s2", "t2").unwrap();
        let saved = load_session(&path).unwrap().unwrap();
        assert_eq!(saved.browser, identity());
        assert_eq!(saved.turn_id, "t2");
        assert_eq!(
            saved
                .browser
                .selector(&BrowserSelector::default())
                .unwrap()
                .instance_id
                .as_deref(),
            Some("instance-a")
        );
        assert!(saved
            .browser
            .selector(&BrowserSelector::new(Some("chrome".into()), None))
            .is_err());
        clear_session(&path).unwrap();
        drop(guard);
        assert!(SessionLock::acquire(&path).is_ok());
        assert!(std::fs::read_dir(dir.path()).unwrap().all(|e| !e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains("tmp-")));
    }

    #[test]
    fn corrupt_legacy_oversize_and_empty_identity_are_not_new_sessions() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.json");
        for bytes in [
            b"not json".to_vec(),
            br#"{"version":1,"session_id":"s","turn_id":"t"}"#.to_vec(),
            vec![b' '; MAX_SESSION_BYTES as usize + 1],
        ] {
            std::fs::write(&path, bytes).unwrap();
            assert!(load_session(&path).is_err());
        }
        assert!(save_session(&path, identity(), "", "t").is_err());
    }

    #[test]
    fn competing_owners_fail_and_drop_releases_without_unlink() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.json");
        let owner = SessionLock::acquire(&path).unwrap();
        assert!(SessionLock::acquire(&path).is_err());
        drop(owner);
        assert!(SessionLock::acquire(&path).is_ok());
    }
}
