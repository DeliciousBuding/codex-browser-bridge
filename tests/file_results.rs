use codex_browser_bridge::file_results::{
    validate_output_directory, validate_uploads, FileBudget, FileLimits, PathScope, StagedFiles,
};
use std::{fs, time::Duration};
use tempfile::tempdir;

fn budget(files: usize, each: usize, total: usize) -> FileBudget {
    FileBudget::new(FileLimits {
        max_files: files,
        max_file_bytes: each,
        max_total_bytes: total,
        max_duration: Duration::from_secs(1),
    })
    .unwrap()
}

#[test]
fn staged_file_is_private_until_atomic_publication_and_scope_is_explicit() {
    let dir = tempdir().unwrap();
    let mut transaction = StagedFiles::new(dir.path(), budget(1, 3, 3)).unwrap();
    transaction.write("content", b"abc").unwrap();
    assert!(!dir.path().join("result.txt").exists());
    let result = transaction.publish_file("content", "result.txt").unwrap();
    assert_eq!(fs::read(result).unwrap(), b"abc");
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    assert_eq!(
        serde_json::to_value(PathScope::BridgeHost).unwrap(),
        "bridge_host"
    );
}

#[test]
fn existing_single_file_is_never_overwritten_and_stage_rolls_back() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("result.txt"), b"original").unwrap();
    let mut transaction = StagedFiles::new(dir.path(), budget(1, 3, 3)).unwrap();
    transaction.write("content", b"abc").unwrap();
    assert!(transaction.publish_file("content", "result.txt").is_err());
    assert_eq!(
        fs::read(dir.path().join("result.txt")).unwrap(),
        b"original"
    );
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn existing_even_empty_directory_is_never_replaced() {
    let dir = tempdir().unwrap();
    fs::create_dir(dir.path().join("bundle")).unwrap();
    let mut transaction = StagedFiles::new(dir.path(), budget(1, 3, 3)).unwrap();
    transaction.write("content", b"abc").unwrap();
    assert!(transaction.publish_bundle("bundle").is_err());
    assert_eq!(fs::read_dir(dir.path().join("bundle")).unwrap().count(), 0);
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn bundle_is_visible_only_when_complete() {
    let dir = tempdir().unwrap();
    let mut transaction = StagedFiles::new(dir.path(), budget(2, 8, 11)).unwrap();
    transaction.write("asset.png", b"abc").unwrap();
    transaction.write("manifest.json", b"manifest").unwrap();
    assert!(!dir.path().join("bundle").exists());
    let published = transaction.publish_bundle("bundle").unwrap();
    assert_eq!(fs::read_dir(published).unwrap().count(), 2);
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn byte_count_and_file_count_failures_remove_only_the_transaction() {
    for (limits, bytes) in [((1, 2, 10), b"abc".as_slice()), ((1, 3, 2), b"abc")] {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("keep"), b"keep").unwrap();
        let mut transaction =
            StagedFiles::new(dir.path(), budget(limits.0, limits.1, limits.2)).unwrap();
        assert!(transaction.write("content", bytes).is_err());
        drop(transaction);
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }
    let dir = tempdir().unwrap();
    let mut transaction = StagedFiles::new(dir.path(), budget(1, 3, 10)).unwrap();
    transaction.write("first", b"abc").unwrap();
    assert!(transaction.write("second", b"a").is_err());
    drop(transaction);
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[test]
fn name_traversal_and_duplicate_names_are_rejected_without_overwrite() {
    let dir = tempdir().unwrap();
    let mut transaction = StagedFiles::new(dir.path(), budget(4, 3, 12)).unwrap();
    for name in [
        "../escape",
        "nested/file",
        "C:\\escape",
        "",
        ".hidden",
        "trailing.",
    ] {
        assert!(transaction.write(name, b"abc").is_err());
    }
    transaction.write("content", b"abc").unwrap();
    assert!(transaction.write("content", b"new").is_err());
    assert_eq!(
        fs::read(transaction.publish_file("content", "result.txt").unwrap()).unwrap(),
        b"abc"
    );
}

#[test]
fn invalid_hard_limits_are_rejected() {
    for limits in [
        (0, 1, 1),
        (514, 1, 1),
        (1, 16 * 1024 * 1024 + 1, 1),
        (1, 1, 20 * 1024 * 1024 + 1),
    ] {
        assert!(FileBudget::new(FileLimits {
            max_files: limits.0,
            max_file_bytes: limits.1,
            max_total_bytes: limits.2,
            max_duration: Duration::from_secs(1)
        })
        .is_err());
    }
}

#[test]
fn upload_validates_configured_root_count_size_and_total() {
    let dir = tempdir().unwrap();
    let one = dir.path().join("one.txt");
    fs::write(&one, b"abc").unwrap();
    let paths = vec![one.to_string_lossy().into_owned()];
    assert_eq!(
        validate_uploads(&paths, dir.path()).unwrap(),
        vec![fs::canonicalize(one)
            .unwrap()
            .to_string_lossy()
            .into_owned()]
    );
    assert!(validate_uploads(&vec![paths[0].clone(); 33], dir.path()).is_err());
    assert!(validate_uploads(&[dir.path().to_string_lossy().into_owned()], dir.path()).is_err());
    let outside = tempdir().unwrap();
    fs::write(outside.path().join("other.txt"), b"other").unwrap();
    assert!(validate_uploads(
        &[outside
            .path()
            .join("other.txt")
            .to_string_lossy()
            .into_owned()],
        dir.path()
    )
    .is_err());
    let large = dir.path().join("large");
    fs::File::create(&large)
        .unwrap()
        .set_len(10 * 1024 * 1024 + 1)
        .unwrap();
    assert!(validate_uploads(&[large.to_string_lossy().into_owned()], dir.path()).is_err());
    fs::File::create(&large)
        .unwrap()
        .set_len(10 * 1024 * 1024)
        .unwrap();
    assert!(validate_uploads(&vec![large.to_string_lossy().into_owned(); 3], dir.path()).is_err());
}

#[test]
fn output_root_and_parent_traversal_are_rejected_consistently() {
    let dir = tempdir().unwrap();
    let root = dir.path().ancestors().last().unwrap();
    assert!(validate_output_directory(root).is_err());
    assert!(validate_output_directory(&dir.path().join("..")).is_err());
}

#[cfg(unix)]
#[test]
fn upload_symlink_within_root_and_symlinked_ancestor_are_rejected() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("content"), b"abc").unwrap();
    std::os::unix::fs::symlink(dir.path().join("content"), dir.path().join("link")).unwrap();
    assert!(validate_uploads(
        &[dir.path().join("link").to_string_lossy().into_owned()],
        dir.path()
    )
    .is_err());
    std::os::unix::fs::symlink(dir.path(), dir.path().join("alias")).unwrap();
    assert!(validate_uploads(
        &[dir
            .path()
            .join("alias/content")
            .to_string_lossy()
            .into_owned()],
        dir.path()
    )
    .is_err());
}
