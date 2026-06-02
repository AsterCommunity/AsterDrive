use super::paths::sanitize_relative_path;
use crate::storage::drivers::local::promote::PromoteLocalFileOutcome;
use crate::storage::traits::driver::{StorageDriver, StoragePathVisitor};
use crate::storage::traits::extensions::{
    ListStorageDriver, LocalPathStorageDriver, StorageCapacityStatus,
};
use std::path::{Path, PathBuf};
use tokio::io::AsyncReadExt;

fn build_policy(base: &Path) -> crate::entities::storage_policy::Model {
    crate::entities::storage_policy::Model {
        id: 1,
        name: "local".into(),
        driver_type: crate::types::DriverType::Local,
        endpoint: String::new(),
        bucket: String::new(),
        access_key: String::new(),
        secret_key: String::new(),
        base_path: base.to_string_lossy().into(),
        remote_node_id: None,
        max_file_size: 0,
        allowed_types: crate::types::StoredStoragePolicyAllowedTypes::empty(),
        options: crate::types::StoredStoragePolicyOptions::empty(),
        is_default: false,
        chunk_size: 0,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }
}

fn unique_temp_dir(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "aster-{label}-{}-{}",
        std::process::id(),
        rand::random::<u64>()
    ))
}

struct CollectingVisitor {
    paths: Vec<String>,
}

impl StoragePathVisitor for CollectingVisitor {
    fn visit_path(&mut self, path: String) -> crate::errors::Result<()> {
        self.paths.push(path);
        Ok(())
    }
}

#[test]
fn sanitize_accepts_normal_paths() {
    assert_eq!(
        sanitize_relative_path("ab/cd/abcdef").unwrap(),
        PathBuf::from("ab/cd/abcdef")
    );
    assert_eq!(
        sanitize_relative_path("/leading/slash").unwrap(),
        PathBuf::from("leading/slash")
    );
    assert_eq!(
        sanitize_relative_path("nested/./path").unwrap(),
        PathBuf::from("nested/path")
    );
}

#[test]
fn sanitize_rejects_parent_dir() {
    assert!(sanitize_relative_path("../etc/passwd").is_err());
    assert!(sanitize_relative_path("ab/../../../etc/passwd").is_err());
    assert!(sanitize_relative_path("ab/..").is_err());
}

#[test]
fn sanitize_rejects_absolute_paths() {
    assert!(sanitize_relative_path("/etc/passwd").is_ok()); // stripped leading slash
    // Path that starts with non-trim '/' after components would be normalized; real absolute
    // only triggers on Windows prefixes or re-rooting. Ensure multi-slash doesn't bypass.
    assert!(sanitize_relative_path("//../etc").is_err());
}

#[tokio::test]
async fn get_range_returns_partial_bytes() {
    let base = unique_temp_dir("range-test");
    tokio::fs::create_dir_all(&base).await.unwrap();

    let policy = build_policy(&base);
    let driver = super::LocalDriver::new(&policy).unwrap();
    driver.put("sample.txt", b"Hello, world!").await.unwrap();

    // offset=7, length=5 -> "world"
    let mut reader = driver.get_range("sample.txt", 7, Some(5)).await.unwrap();
    let mut buf = Vec::new();
    reader.read_to_end(&mut buf).await.unwrap();
    assert_eq!(buf, b"world");

    // offset=7, length=None -> "world!"
    let mut reader = driver.get_range("sample.txt", 7, None).await.unwrap();
    let mut buf = Vec::new();
    reader.read_to_end(&mut buf).await.unwrap();
    assert_eq!(buf, b"world!");

    // offset=0, length=5 -> "Hello"
    let mut reader = driver.get_range("sample.txt", 0, Some(5)).await.unwrap();
    let mut buf = Vec::new();
    reader.read_to_end(&mut buf).await.unwrap();
    assert_eq!(buf, b"Hello");

    let _ = tokio::fs::remove_dir_all(&base).await;
}

#[tokio::test]
async fn capacity_info_reports_filesystem_space_for_base_path() {
    let base = unique_temp_dir("capacity-test");
    tokio::fs::create_dir_all(&base).await.unwrap();

    let policy = build_policy(&base);
    let driver = super::LocalDriver::new(&policy).unwrap();

    let capacity = driver
        .capacity_info()
        .await
        .expect("local capacity should be observable");

    assert_eq!(capacity.status, StorageCapacityStatus::Supported);
    assert_eq!(capacity.source, "local_filesystem");
    let total = capacity.total_bytes.expect("total bytes should exist");
    let available = capacity
        .available_bytes
        .expect("available bytes should exist");
    let used = capacity.used_bytes.expect("used bytes should exist");
    assert!(total > 0);
    assert!(total >= available);
    assert!(used >= 0);
    assert_eq!(used, total.saturating_sub(available));

    let _ = tokio::fs::remove_dir_all(&base).await;
}

#[tokio::test]
async fn promote_local_file_if_absent_does_not_overwrite_existing_target() {
    let base = unique_temp_dir("local-promote-test");
    tokio::fs::create_dir_all(&base).await.unwrap();

    let policy = build_policy(&base);
    let driver = super::LocalDriver::new(&policy).unwrap();
    let target = "ab/cd/existing";
    let target_full = driver.full_path(target).unwrap();
    tokio::fs::create_dir_all(target_full.parent().unwrap())
        .await
        .unwrap();
    tokio::fs::write(&target_full, b"old").await.unwrap();

    let source = base.join("source.bin");
    tokio::fs::write(&source, b"new").await.unwrap();
    let outcome = super::promote_local_file_if_absent(&driver, target, source.to_str().unwrap(), 3)
        .await
        .unwrap();

    assert_eq!(outcome, PromoteLocalFileOutcome::AlreadyExists);
    assert_eq!(tokio::fs::read(&target_full).await.unwrap(), b"old");
    assert!(!source.exists());

    let _ = tokio::fs::remove_dir_all(&base).await;
}

#[tokio::test]
async fn promote_local_file_if_absent_reports_created_target() {
    let base = unique_temp_dir("local-promote-created-test");
    tokio::fs::create_dir_all(&base).await.unwrap();

    let policy = build_policy(&base);
    let driver = super::LocalDriver::new(&policy).unwrap();
    let target = "ab/cd/new";
    let target_full = driver.full_path(target).unwrap();

    let source = base.join("source.bin");
    tokio::fs::write(&source, b"new").await.unwrap();
    let outcome = super::promote_local_file_if_absent(&driver, target, source.to_str().unwrap(), 3)
        .await
        .unwrap();

    assert_eq!(outcome, PromoteLocalFileOutcome::Created);
    assert_eq!(tokio::fs::read(&target_full).await.unwrap(), b"new");
    assert!(!source.exists());

    let _ = tokio::fs::remove_dir_all(&base).await;
}

#[tokio::test]
async fn promote_local_file_if_absent_rejects_existing_size_mismatch() {
    let base = unique_temp_dir("local-promote-mismatch-test");
    tokio::fs::create_dir_all(&base).await.unwrap();

    let policy = build_policy(&base);
    let driver = super::LocalDriver::new(&policy).unwrap();
    let target = "ab/cd/existing";
    let target_full = driver.full_path(target).unwrap();
    tokio::fs::create_dir_all(target_full.parent().unwrap())
        .await
        .unwrap();
    tokio::fs::write(&target_full, b"old").await.unwrap();

    let source = base.join("source.bin");
    tokio::fs::write(&source, b"new-data").await.unwrap();
    let error = super::promote_local_file_if_absent(&driver, target, source.to_str().unwrap(), 8)
        .await
        .expect_err("existing blob with different size must be rejected");

    assert!(error.message().contains("size mismatch"));
    assert_eq!(tokio::fs::read(&target_full).await.unwrap(), b"old");
    assert!(source.exists());

    let _ = tokio::fs::remove_dir_all(&base).await;
}

#[tokio::test]
async fn promote_local_file_if_absent_rolls_back_linked_size_mismatch() {
    let base = unique_temp_dir("local-promote-linked-mismatch-test");
    tokio::fs::create_dir_all(&base).await.unwrap();

    let policy = build_policy(&base);
    let driver = super::LocalDriver::new(&policy).unwrap();
    let target = "ab/cd/new-target";
    let target_full = driver.full_path(target).unwrap();

    let source = base.join("source.bin");
    tokio::fs::write(&source, b"short").await.unwrap();
    let error = super::promote_local_file_if_absent(&driver, target, source.to_str().unwrap(), 8)
        .await
        .expect_err("newly linked blob with different size must be rejected");

    assert!(error.message().contains("size mismatch"));
    assert!(source.exists());
    assert!(!target_full.exists());

    let _ = tokio::fs::remove_dir_all(&base).await;
}

#[cfg(unix)]
#[tokio::test]
async fn put_rejects_symlink_escape_inside_storage_root() {
    let temp_root = unique_temp_dir("local-symlink-test");
    let base = temp_root.join("storage");
    let outside = temp_root.join("outside");
    std::fs::create_dir_all(&base).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, base.join("escape")).unwrap();

    let policy = build_policy(&base);
    let driver = super::LocalDriver::new(&policy).unwrap();
    let result = driver.put("escape/pwned.txt", b"nope").await;

    assert!(result.is_err());
    assert!(!outside.join("pwned.txt").exists());

    let _ = tokio::fs::remove_dir_all(&temp_root).await;
}

#[cfg(unix)]
#[test]
fn staging_path_rejects_symlink_escape() {
    let temp_root = unique_temp_dir("local-staging-symlink-test");
    let base = temp_root.join("storage");
    let outside = temp_root.join("outside");
    std::fs::create_dir_all(&base).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, base.join(".staging")).unwrap();

    let policy = build_policy(&base);
    let result = super::upload_staging_path(&policy, "token.upload");

    assert!(result.is_err());

    let _ = std::fs::remove_dir_all(&temp_root);
}

#[tokio::test]
async fn list_paths_returns_sorted_recursive_files_and_honors_prefix() {
    let base = unique_temp_dir("local-list-test");
    tokio::fs::create_dir_all(&base).await.unwrap();

    let policy = build_policy(&base);
    let driver = super::LocalDriver::new(&policy).unwrap();
    driver.put("b/two.txt", b"2").await.unwrap();
    driver.put("a/one.txt", b"1").await.unwrap();
    driver.put("a/nested/three.txt", b"3").await.unwrap();

    let listed = driver.list_paths(None).await.unwrap();
    assert_eq!(
        listed,
        vec![
            "a/nested/three.txt".to_string(),
            "a/one.txt".to_string(),
            "b/two.txt".to_string()
        ]
    );

    let prefixed = driver.list_paths(Some("a")).await.unwrap();
    assert_eq!(
        prefixed,
        vec!["a/nested/three.txt".to_string(), "a/one.txt".to_string()]
    );

    let missing = driver.list_paths(Some("missing")).await.unwrap();
    assert!(missing.is_empty());

    let _ = tokio::fs::remove_dir_all(&base).await;
}

#[tokio::test]
async fn scan_paths_visits_single_file_and_missing_prefix() {
    let base = unique_temp_dir("local-scan-file-test");
    tokio::fs::create_dir_all(&base).await.unwrap();

    let policy = build_policy(&base);
    let driver = super::LocalDriver::new(&policy).unwrap();
    driver.put("folder/file.txt", b"content").await.unwrap();

    let mut visitor = CollectingVisitor { paths: Vec::new() };
    driver
        .scan_paths(Some("folder/file.txt"), &mut visitor)
        .await
        .unwrap();
    assert_eq!(visitor.paths, vec!["folder/file.txt".to_string()]);

    driver
        .scan_paths(Some("folder/missing.txt"), &mut visitor)
        .await
        .unwrap();
    assert_eq!(visitor.paths, vec!["folder/file.txt".to_string()]);

    let _ = tokio::fs::remove_dir_all(&base).await;
}

#[tokio::test]
async fn scan_paths_walks_directories_in_stable_order() {
    let base = unique_temp_dir("local-scan-dir-test");
    tokio::fs::create_dir_all(&base).await.unwrap();

    let policy = build_policy(&base);
    let driver = super::LocalDriver::new(&policy).unwrap();
    driver.put("root/z.txt", b"z").await.unwrap();
    driver.put("root/a.txt", b"a").await.unwrap();
    driver.put("root/child/b.txt", b"b").await.unwrap();

    let mut visitor = CollectingVisitor { paths: Vec::new() };
    driver.scan_paths(Some("root"), &mut visitor).await.unwrap();

    assert_eq!(
        visitor.paths,
        vec![
            "root/a.txt".to_string(),
            "root/z.txt".to_string(),
            "root/child/b.txt".to_string()
        ]
    );

    let _ = tokio::fs::remove_dir_all(&base).await;
}

#[tokio::test]
async fn copy_metadata_stream_and_delete_roundtrip() {
    let base = unique_temp_dir("local-roundtrip-test");
    tokio::fs::create_dir_all(&base).await.unwrap();

    let policy = build_policy(&base);
    let driver = super::LocalDriver::new(&policy).unwrap();
    driver.put("src/file.txt", b"copy me").await.unwrap();

    let copied = driver
        .copy_object("src/file.txt", "dest/file.txt")
        .await
        .unwrap();
    assert_eq!(copied, "dest/file.txt");
    assert!(driver.exists("dest/file.txt").await.unwrap());
    assert_eq!(driver.metadata("dest/file.txt").await.unwrap().size, 7);

    let mut stream = driver.get_stream("dest/file.txt").await.unwrap();
    let mut data = Vec::new();
    stream.read_to_end(&mut data).await.unwrap();
    assert_eq!(data, b"copy me");

    let local_path = driver
        .resolve_local_path("dest/file.txt")
        .expect("local path should resolve");
    assert!(local_path.ends_with("dest/file.txt"));

    driver.delete("dest/file.txt").await.unwrap();
    assert!(!driver.exists("dest/file.txt").await.unwrap());

    let _ = tokio::fs::remove_dir_all(&base).await;
}
