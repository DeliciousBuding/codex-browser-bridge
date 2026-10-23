use std::fs;

use codex_browser_bridge::export::validate_export_directory;
use tempfile::tempdir;

#[test]
fn export_destination_requires_existing_absolute_directory() {
    let relative = std::path::Path::new("exports");
    assert!(validate_export_directory(relative).is_err());

    let missing = tempdir().unwrap().path().join("does-not-exist");
    assert!(validate_export_directory(&missing).is_err());
}

#[test]
fn export_destination_accepts_real_directory_and_returns_canonical_path() {
    let dir = tempdir().unwrap();
    let path = validate_export_directory(dir.path()).unwrap();
    assert!(path.is_absolute());
    assert!(path.is_dir());
    assert_eq!(fs::canonicalize(dir.path()).unwrap(), path);
}

#[cfg(unix)]
#[test]
fn export_destination_rejects_symlink() {
    let dir = tempdir().unwrap();
    let target = dir.path().join("target");
    let link = dir.path().join("link");
    fs::create_dir(&target).unwrap();
    std::os::unix::fs::symlink(&target, &link).unwrap();
    assert!(validate_export_directory(&link).is_err());
}

#[cfg(unix)]
#[test]
fn export_destination_rejects_symlinked_ancestor() {
    let dir = tempdir().unwrap();
    let target = dir.path().join("target");
    let link = dir.path().join("link");
    fs::create_dir(&target).unwrap();
    fs::create_dir(target.join("exports")).unwrap();
    std::os::unix::fs::symlink(&target, &link).unwrap();
    assert!(validate_export_directory(&link.join("exports")).is_err());
}

#[cfg(windows)]
#[test]
fn export_destination_rejects_symlink() {
    // Creating symlinks may require a developer-mode privilege on Windows;
    // the implementation still rejects them when supplied by the caller.
    let dir = tempdir().unwrap();
    let target = dir.path().join("target");
    let link = dir.path().join("link");
    fs::create_dir(&target).unwrap();
    if std::os::windows::fs::symlink_dir(&target, &link).is_ok() {
        assert!(validate_export_directory(&link).is_err());
    }
}

#[cfg(windows)]
#[test]
fn export_destination_rejects_symlinked_ancestor() {
    let dir = tempdir().unwrap();
    let target = dir.path().join("target");
    let link = dir.path().join("link");
    fs::create_dir(&target).unwrap();
    fs::create_dir(target.join("exports")).unwrap();
    if std::os::windows::fs::symlink_dir(&target, &link).is_ok() {
        assert!(validate_export_directory(&link.join("exports")).is_err());
    }
}
