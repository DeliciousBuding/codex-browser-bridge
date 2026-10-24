//! Bridge-host file results. This boundary never configures browser downloads.
//!
//! Callers choose an existing directory. Content is staged privately, bounded,
//! synced, then published without overwriting an existing result. A bundle is
//! published as one directory; a single file is published with an atomic hard
//! link. Failed/cancelled transactions remove only their own staged files.
//! Component validation is best effort, not protection against a malicious
//! process concurrently replacing directories. The caller controls its root.
use crate::error::{BridgeError, Result};
use serde::Serialize;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Component, Path, PathBuf},
    time::{Duration, Instant},
};
use uuid::Uuid;

pub const MAX_FILES: usize = 513;
pub const MAX_FILE_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_TOTAL_BYTES: usize = 20 * 1024 * 1024;
pub const MAX_DURATION: Duration = Duration::from_secs(30);

/// A returned local path is on the bridge host, not necessarily the browser host.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PathScope {
    BridgeHost,
}

/// Validate uploads only after the caller explicitly asserts that bridge and
/// browser share this filesystem. Remote browser paths cannot be verified here.
/// The root must come from operator configuration, never an arbitrary tool arg.
pub fn validate_uploads(paths: &[String], allowed_base: &Path) -> Result<Vec<String>> {
    if paths.len() > 32 {
        return Err(user("uploads permit at most 32 files"));
    }
    let root = validate_output_directory(allowed_base)?;
    let mut total = 0u64;
    let mut validated = Vec::with_capacity(paths.len());
    for path in paths {
        let raw = Path::new(path);
        if !raw.is_absolute()
            || raw
                .components()
                .any(|c| matches!(c, Component::CurDir | Component::ParentDir))
        {
            return Err(user(
                "upload paths must be explicit absolute paths without traversal",
            ));
        }
        for component in raw.ancestors() {
            let metadata = fs::symlink_metadata(component)
                .map_err(|_| user("cannot inspect upload path components"))?;
            if unsafe_metadata(&metadata) {
                return Err(user(
                    "upload path components may not be symlinks or reparse points",
                ));
            }
        }
        let canonical = crate::security::validate_file_path(path, &root)?;
        total = total
            .checked_add(fs::metadata(&canonical)?.len())
            .ok_or_else(|| user("upload byte overflow"))?;
        if total > 20 * 1024 * 1024 {
            return Err(user("uploads exceed 20 MiB total"));
        }
        validated.push(canonical.to_string_lossy().into_owned());
    }
    Ok(validated)
}

/// Validate all lexical components before canonicalization. Filesystem roots,
/// parent traversal, symlinks and Windows reparse points are not destinations.
pub fn validate_output_directory(path: &Path) -> Result<PathBuf> {
    if !path.is_absolute()
        || !path.components().any(|c| matches!(c, Component::Normal(_)))
        || path
            .components()
            .any(|c| matches!(c, Component::CurDir | Component::ParentDir))
    {
        return Err(user("output_dir must be an existing explicit absolute directory, not a filesystem root or traversal"));
    }
    for component in path.ancestors() {
        let meta = fs::symlink_metadata(component)
            .map_err(|_| user("output_dir must already exist and have inspectable components"))?;
        if !meta.is_dir() || unsafe_metadata(&meta) {
            return Err(user("output_dir and its ancestors must be real directories, not symlinks or reparse points"));
        }
    }
    fs::canonicalize(path).map_err(|_| user("cannot resolve output_dir"))
}

fn unsafe_metadata(meta: &fs::Metadata) -> bool {
    if meta.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        meta.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    false
}

#[derive(Debug, Clone, Copy)]
pub struct FileLimits {
    pub max_files: usize,
    pub max_file_bytes: usize,
    pub max_total_bytes: usize,
    pub max_duration: Duration,
}

/// Shared count, decoded-byte and elapsed-time accounting. Start before CDP
/// reads so resource acquisition and publication consume the same budget.
pub struct FileBudget {
    limits: FileLimits,
    started: Instant,
    files: usize,
    bytes: usize,
}

impl FileBudget {
    pub fn new(limits: FileLimits) -> Result<Self> {
        if limits.max_files == 0
            || limits.max_files > MAX_FILES
            || limits.max_file_bytes == 0
            || limits.max_file_bytes > MAX_FILE_BYTES
            || limits.max_total_bytes == 0
            || limits.max_total_bytes > MAX_TOTAL_BYTES
            || limits.max_duration.is_zero()
            || limits.max_duration > MAX_DURATION
        {
            return Err(user(
                "invalid file-result limits or limits exceed hard caps",
            ));
        }
        Ok(Self {
            limits,
            started: Instant::now(),
            files: 0,
            bytes: 0,
        })
    }

    pub fn check_elapsed(&self) -> Result<()> {
        if self.started.elapsed() >= self.limits.max_duration {
            Err(BridgeError::Timeout(
                "file result budget exhausted; no result published".into(),
            ))
        } else {
            Ok(())
        }
    }

    pub fn account(&mut self, bytes: usize) -> Result<()> {
        self.check_elapsed()?;
        let total = self
            .bytes
            .checked_add(bytes)
            .ok_or_else(|| user("file-result byte overflow"))?;
        if self.files >= self.limits.max_files
            || bytes > self.limits.max_file_bytes
            || total > self.limits.max_total_bytes
        {
            return Err(user(
                "file result exceeds count, per-file or total-byte budget",
            ));
        }
        self.files += 1;
        self.bytes = total;
        Ok(())
    }
}

/// One transaction owns only direct child files in a randomly named stage.
pub struct StagedFiles {
    root: PathBuf,
    stage: PathBuf,
    files: Vec<PathBuf>,
    budget: FileBudget,
    published: bool,
}

impl StagedFiles {
    pub fn new(output_dir: &Path, budget: FileBudget) -> Result<Self> {
        budget.check_elapsed()?;
        let root = validate_output_directory(output_dir)?;
        let stage = root.join(format!(".codex-stage-{}", Uuid::new_v4()));
        let builder = fs::DirBuilder::new();
        #[cfg(unix)]
        let mut builder = builder;
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&stage)?;
        Ok(Self {
            root,
            stage,
            files: Vec::new(),
            budget,
            published: false,
        })
    }

    pub fn destination(&self, name: &str) -> Result<PathBuf> {
        validate_name(name)?;
        Ok(self.root.join(name))
    }

    pub fn write(&mut self, name: &str, bytes: &[u8]) -> Result<()> {
        validate_name(name)?;
        self.budget.account(bytes.len())?;
        self.check_root()?;
        let path = self.stage.join(name);
        let mut options = OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&path)?;
        self.files.push(path);
        for chunk in bytes.chunks(64 * 1024) {
            self.budget.check_elapsed()?;
            file.write_all(chunk)?;
        }
        file.sync_all()?;
        self.budget.check_elapsed()
    }

    pub fn publish_bundle(mut self, name: &str) -> Result<PathBuf> {
        let destination = self.destination(name)?;
        self.budget.check_elapsed()?;
        self.check_root()?;
        rename_no_replace(&self.stage, &destination)?;
        if let Err(error) = self.budget.check_elapsed() {
            self.files = self
                .files
                .iter()
                .filter_map(|file| file.file_name().map(|name| destination.join(name)))
                .collect();
            self.stage = destination;
            return Err(error);
        }
        self.published = true;
        Ok(destination)
    }

    pub fn publish_file(self, staged_name: &str, name: &str) -> Result<PathBuf> {
        validate_name(staged_name)?;
        let destination = self.destination(name)?;
        let source = self.stage.join(staged_name);
        if !self.files.contains(&source) || self.files.len() != 1 {
            return Err(user(
                "single-file publication requires exactly one staged file",
            ));
        }
        self.budget.check_elapsed()?;
        self.check_root()?;
        // Hard link publication is atomic and fails if destination exists on
        // Windows and Unix. No fallback may overwrite an existing result.
        fs::hard_link(source, &destination)?;
        if let Err(error) = self.budget.check_elapsed() {
            fs::remove_file(&destination)?;
            return Err(error);
        }
        Ok(destination)
    }

    fn check_root(&self) -> Result<()> {
        if validate_output_directory(&self.root)? != self.root
            || !fs::symlink_metadata(&self.stage).is_ok_and(|m| m.is_dir() && !unsafe_metadata(&m))
        {
            return Err(user("output directory changed during file publication"));
        }
        Ok(())
    }
}

impl Drop for StagedFiles {
    fn drop(&mut self) {
        if !self.published && self.check_root().is_ok() {
            for file in &self.files {
                let _ = fs::remove_file(file);
            }
            let _ = fs::remove_dir(&self.stage);
        }
    }
}

fn validate_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name.len() > 200
        || name.starts_with('.')
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        || name.ends_with('.')
    {
        return Err(user(
            "file-result names must be bounded generated basenames",
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn rename_no_replace(source: &Path, destination: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    extern "system" {
        fn MoveFileExW(source: *const u16, destination: *const u16, flags: u32) -> i32;
    }
    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    // std::fs::rename may replace an empty directory on newer Windows. Use
    // MoveFileExW with no REPLACE_EXISTING flag for atomic no-clobber rename.
    // SAFETY: both UTF-16 buffers are NUL terminated and live during this call.
    let code = unsafe { MoveFileExW(source.as_ptr(), destination.as_ptr(), 0) };
    if code != 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(target_os = "linux")]
fn rename_no_replace(source: &Path, destination: &Path) -> std::io::Result<()> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};
    let source = CString::new(source.as_os_str().as_bytes())?;
    let destination = CString::new(destination.as_os_str().as_bytes())?;
    // SAFETY: both C strings are valid for the duration of the syscall.
    let code = unsafe {
        libc::syscall(
            libc::SYS_renameat2,
            libc::AT_FDCWD,
            source.as_ptr(),
            libc::AT_FDCWD,
            destination.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    if code == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(target_os = "macos")]
fn rename_no_replace(source: &Path, destination: &Path) -> std::io::Result<()> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};
    let source = CString::new(source.as_os_str().as_bytes())?;
    let destination = CString::new(destination.as_os_str().as_bytes())?;
    // SAFETY: both C strings are valid; RENAME_EXCL rejects an existing path.
    let code =
        unsafe { libc::renamex_np(source.as_ptr(), destination.as_ptr(), libc::RENAME_EXCL) };
    if code == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
fn rename_no_replace(_: &Path, _: &Path) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "atomic no-overwrite directory publication is unavailable on this platform",
    ))
}

fn user(message: &str) -> BridgeError {
    BridgeError::User(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elapsed_budget_prevents_file_and_bundle_publication_and_removes_stage() {
        for bundle in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let budget = FileBudget::new(FileLimits {
                max_files: 1,
                max_file_bytes: 3,
                max_total_bytes: 3,
                max_duration: MAX_DURATION,
            })
            .unwrap();
            let mut transaction = StagedFiles::new(directory.path(), budget).unwrap();
            transaction.write("content", b"abc").unwrap();
            // Expire an already-populated transaction without a short setup
            // deadline or sleep: CI load must not determine test behavior.
            transaction.budget.started = Instant::now()
                .checked_sub(MAX_DURATION + Duration::from_secs(1))
                .unwrap();
            let result = if bundle {
                transaction.publish_bundle("bundle")
            } else {
                transaction.publish_file("content", "result.txt")
            };
            assert!(matches!(result, Err(BridgeError::Timeout(_))));
            assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
        }
    }
}
