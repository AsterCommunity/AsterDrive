//! 工作空间存储服务测试。

use crate::api::api_error_code::ApiErrorCode;
use crate::cache;
use crate::config::{CacheConfig, Config, DatabaseConfig, RuntimeConfig};
use crate::entities::{file, file_blob, storage_policy, user};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::mail::sender;
use crate::storage::{BlobMetadata, StoragePathVisitor};
use crate::storage::{
    DriverRegistry, ListStorageDriver, LocalPathStorageDriver, PolicySnapshot, StorageDriver,
    StreamUploadDriver,
};
use crate::types::{DriverType, UserRole, UserStatus};
use async_trait::async_trait;
use chrono::Utc;
use migration::Migrator;
use sea_orm::{ActiveModelTrait, EntityTrait, PaginatorTrait, Set};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
    mpsc,
};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::sync::{Notify, oneshot};

use super::{
    StorageCancellationCheck, StorageOperationContext, StoreFromTempHints, StoreFromTempParams,
    StorePreuploadedNondedupParams, WorkspaceStorageScope, prepare_non_dedup_blob_upload,
    store_from_temp_exact_name_silent_with_hints, store_from_temp_exact_name_with_hints,
    store_from_temp_with_hints, store_preuploaded_nondedup, upload_temp_file_to_prepared_blob,
};

#[derive(Clone)]
struct CancelFlagCheck {
    cancelled: Arc<AtomicBool>,
}

impl StorageCancellationCheck for CancelFlagCheck {
    fn checkpoint(&self) -> crate::errors::Result<()> {
        if self.cancelled.load(Ordering::SeqCst) {
            return Err(crate::errors::precondition_failed_with_code(
                ApiErrorCode::TaskWorkerShutdownRequested,
                "test storage operation cancelled",
            ));
        }
        Ok(())
    }
}

fn cancellation_context(cancelled: Arc<AtomicBool>) -> StorageOperationContext {
    StorageOperationContext::new(CancelFlagCheck { cancelled })
}

struct CancelWhenStorageFileExistsCheck {
    root: PathBuf,
}

impl StorageCancellationCheck for CancelWhenStorageFileExistsCheck {
    fn checkpoint(&self) -> crate::errors::Result<()> {
        if storage_root_has_file(&self.root) {
            return Err(crate::errors::precondition_failed_with_code(
                ApiErrorCode::TaskWorkerShutdownRequested,
                "test storage operation cancelled after staging",
            ));
        }
        Ok(())
    }
}

fn cancel_when_storage_file_exists_context(root: PathBuf) -> StorageOperationContext {
    StorageOperationContext::new(CancelWhenStorageFileExistsCheck { root })
}

fn storage_root_has_file(root: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(root) else {
        return false;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            return true;
        }
        if path.is_dir() && storage_root_has_file(&path) {
            return true;
        }
    }
    false
}

struct CountingUploadDriver {
    inner: crate::storage::drivers::local::LocalDriver,
    put_file_count: AtomicUsize,
    put_reader_count: AtomicUsize,
}

impl CountingUploadDriver {
    fn new(policy: &storage_policy::Model) -> Self {
        Self {
            inner: crate::storage::drivers::local::LocalDriver::new(policy)
                .expect("counting test driver should initialize"),
            put_file_count: AtomicUsize::new(0),
            put_reader_count: AtomicUsize::new(0),
        }
    }

    fn put_file_count(&self) -> usize {
        self.put_file_count.load(Ordering::SeqCst)
    }

    fn put_reader_count(&self) -> usize {
        self.put_reader_count.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl StorageDriver for CountingUploadDriver {
    async fn put(&self, path: &str, data: &[u8]) -> crate::errors::Result<String> {
        self.inner.put(path, data).await
    }

    async fn get(&self, path: &str) -> crate::errors::Result<Vec<u8>> {
        self.inner.get(path).await
    }

    async fn get_stream(
        &self,
        path: &str,
    ) -> crate::errors::Result<Box<dyn AsyncRead + Unpin + Send>> {
        self.inner.get_stream(path).await
    }

    async fn delete(&self, path: &str) -> crate::errors::Result<()> {
        self.inner.delete(path).await
    }

    async fn exists(&self, path: &str) -> crate::errors::Result<bool> {
        self.inner.exists(path).await
    }

    async fn metadata(&self, path: &str) -> crate::errors::Result<BlobMetadata> {
        self.inner.metadata(path).await
    }

    fn as_list(&self) -> Option<&dyn ListStorageDriver> {
        Some(self)
    }

    fn as_stream_upload(&self) -> Option<&dyn StreamUploadDriver> {
        Some(self)
    }

    fn as_local_path(&self) -> Option<&dyn LocalPathStorageDriver> {
        self.inner.as_local_path()
    }
}

#[async_trait]
impl ListStorageDriver for CountingUploadDriver {
    async fn list_paths(&self, prefix: Option<&str>) -> crate::errors::Result<Vec<String>> {
        self.inner.list_paths(prefix).await
    }

    async fn scan_paths(
        &self,
        prefix: Option<&str>,
        visitor: &mut dyn StoragePathVisitor,
    ) -> crate::errors::Result<()> {
        self.inner.scan_paths(prefix, visitor).await
    }
}

#[async_trait]
impl StreamUploadDriver for CountingUploadDriver {
    async fn put_file(
        &self,
        storage_path: &str,
        local_path: &str,
    ) -> crate::errors::Result<String> {
        self.put_file_count.fetch_add(1, Ordering::SeqCst);
        self.inner.put_file(storage_path, local_path).await
    }

    async fn put_reader(
        &self,
        storage_path: &str,
        reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
        size: i64,
    ) -> crate::errors::Result<String> {
        self.put_reader_count.fetch_add(1, Ordering::SeqCst);
        self.inner.put_reader(storage_path, reader, size).await
    }
}

fn enable_content_dedup(policy: &storage_policy::Model) -> storage_policy::Model {
    let mut policy = policy.clone();
    policy.options =
        crate::types::StoredStoragePolicyOptions(r#"{"content_dedup":true}"#.to_string());
    policy
}

struct BlockingPutFileDriver {
    inner: crate::storage::drivers::local::LocalDriver,
    put_file_entered: Mutex<Option<oneshot::Sender<()>>>,
    release_put_file: Arc<Notify>,
}

impl BlockingPutFileDriver {
    fn new(policy: &storage_policy::Model) -> (Self, oneshot::Receiver<()>, Arc<Notify>) {
        let (entered_tx, entered_rx) = oneshot::channel();
        let release_put_file = Arc::new(Notify::new());
        (
            Self {
                inner: crate::storage::drivers::local::LocalDriver::new(policy)
                    .expect("blocking test driver should initialize"),
                put_file_entered: Mutex::new(Some(entered_tx)),
                release_put_file: release_put_file.clone(),
            },
            entered_rx,
            release_put_file,
        )
    }
}

#[async_trait]
impl StorageDriver for BlockingPutFileDriver {
    async fn put(&self, path: &str, data: &[u8]) -> crate::errors::Result<String> {
        self.inner.put(path, data).await
    }

    async fn get(&self, path: &str) -> crate::errors::Result<Vec<u8>> {
        self.inner.get(path).await
    }

    async fn get_stream(
        &self,
        path: &str,
    ) -> crate::errors::Result<Box<dyn AsyncRead + Unpin + Send>> {
        self.inner.get_stream(path).await
    }

    async fn delete(&self, path: &str) -> crate::errors::Result<()> {
        self.inner.delete(path).await
    }

    async fn exists(&self, path: &str) -> crate::errors::Result<bool> {
        self.inner.exists(path).await
    }

    async fn metadata(&self, path: &str) -> crate::errors::Result<BlobMetadata> {
        self.inner.metadata(path).await
    }

    fn as_list(&self) -> Option<&dyn ListStorageDriver> {
        Some(self)
    }

    fn as_stream_upload(&self) -> Option<&dyn StreamUploadDriver> {
        Some(self)
    }

    fn as_local_path(&self) -> Option<&dyn LocalPathStorageDriver> {
        self.inner.as_local_path()
    }
}

#[async_trait]
impl ListStorageDriver for BlockingPutFileDriver {
    async fn list_paths(&self, prefix: Option<&str>) -> crate::errors::Result<Vec<String>> {
        self.inner.list_paths(prefix).await
    }

    async fn scan_paths(
        &self,
        prefix: Option<&str>,
        visitor: &mut dyn StoragePathVisitor,
    ) -> crate::errors::Result<()> {
        self.inner.scan_paths(prefix, visitor).await
    }
}

#[async_trait]
impl StreamUploadDriver for BlockingPutFileDriver {
    async fn put_file(
        &self,
        storage_path: &str,
        local_path: &str,
    ) -> crate::errors::Result<String> {
        if let Some(sender) = self
            .put_file_entered
            .lock()
            .expect("blocking test driver lock should succeed")
            .take()
        {
            let _ = sender.send(());
        }
        self.release_put_file.notified().await;
        self.inner.put_file(storage_path, local_path).await
    }

    async fn put_reader(
        &self,
        storage_path: &str,
        reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
        size: i64,
    ) -> crate::errors::Result<String> {
        if let Some(sender) = self
            .put_file_entered
            .lock()
            .expect("blocking test driver lock should succeed")
            .take()
        {
            let _ = sender.send(());
        }
        self.release_put_file.notified().await;
        self.inner.put_reader(storage_path, reader, size).await
    }
}

struct BlockingLocalPathDriver {
    inner: crate::storage::drivers::local::LocalDriver,
    target_entered: Mutex<Option<oneshot::Sender<()>>>,
    release_target: Mutex<Option<mpsc::Receiver<()>>>,
}

impl BlockingLocalPathDriver {
    fn new(policy: &storage_policy::Model) -> (Self, oneshot::Receiver<()>, mpsc::Sender<()>) {
        let (entered_tx, entered_rx) = oneshot::channel();
        let (release_tx, release_rx) = mpsc::channel();
        (
            Self {
                inner: crate::storage::drivers::local::LocalDriver::new(policy)
                    .expect("blocking local path test driver should initialize"),
                target_entered: Mutex::new(Some(entered_tx)),
                release_target: Mutex::new(Some(release_rx)),
            },
            entered_rx,
            release_tx,
        )
    }
}

#[async_trait]
impl StorageDriver for BlockingLocalPathDriver {
    async fn put(&self, path: &str, data: &[u8]) -> crate::errors::Result<String> {
        self.inner.put(path, data).await
    }

    async fn get(&self, path: &str) -> crate::errors::Result<Vec<u8>> {
        self.inner.get(path).await
    }

    async fn get_stream(
        &self,
        path: &str,
    ) -> crate::errors::Result<Box<dyn AsyncRead + Unpin + Send>> {
        self.inner.get_stream(path).await
    }

    async fn delete(&self, path: &str) -> crate::errors::Result<()> {
        self.inner.delete(path).await
    }

    async fn exists(&self, path: &str) -> crate::errors::Result<bool> {
        self.inner.exists(path).await
    }

    async fn metadata(&self, path: &str) -> crate::errors::Result<BlobMetadata> {
        self.inner.metadata(path).await
    }

    fn as_list(&self) -> Option<&dyn ListStorageDriver> {
        Some(self)
    }

    fn as_stream_upload(&self) -> Option<&dyn StreamUploadDriver> {
        Some(self)
    }

    fn as_local_path(&self) -> Option<&dyn LocalPathStorageDriver> {
        Some(self)
    }
}

#[async_trait]
impl ListStorageDriver for BlockingLocalPathDriver {
    async fn list_paths(&self, prefix: Option<&str>) -> crate::errors::Result<Vec<String>> {
        self.inner.list_paths(prefix).await
    }

    async fn scan_paths(
        &self,
        prefix: Option<&str>,
        visitor: &mut dyn StoragePathVisitor,
    ) -> crate::errors::Result<()> {
        self.inner.scan_paths(prefix, visitor).await
    }
}

#[async_trait]
impl StreamUploadDriver for BlockingLocalPathDriver {
    async fn put_file(
        &self,
        storage_path: &str,
        local_path: &str,
    ) -> crate::errors::Result<String> {
        self.inner.put_file(storage_path, local_path).await
    }

    async fn put_reader(
        &self,
        storage_path: &str,
        reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
        size: i64,
    ) -> crate::errors::Result<String> {
        self.inner.put_reader(storage_path, reader, size).await
    }
}

impl LocalPathStorageDriver for BlockingLocalPathDriver {
    fn resolve_local_path(&self, path: &str) -> crate::errors::Result<PathBuf> {
        if let Some(sender) = self
            .target_entered
            .lock()
            .expect("blocking local path test driver lock should succeed")
            .take()
        {
            let _ = sender.send(());
        }
        let release_rx = self
            .release_target
            .lock()
            .expect("blocking local path release lock should succeed")
            .take()
            .expect("blocking local path release receiver should exist");
        release_rx.recv().map_err(|error| {
            crate::errors::AsterError::storage_driver_error(format!(
                "blocking local path release channel closed: {error}"
            ))
        })?;
        self.inner.as_local_path().unwrap().resolve_local_path(path)
    }
}

struct CancelAfterFirstReadDriver {
    cancelled: Arc<AtomicBool>,
    delete_count: AtomicUsize,
}

impl CancelAfterFirstReadDriver {
    fn new(cancelled: Arc<AtomicBool>) -> Self {
        Self {
            cancelled,
            delete_count: AtomicUsize::new(0),
        }
    }

    fn delete_count(&self) -> usize {
        self.delete_count.load(Ordering::SeqCst)
    }
}

struct RecoverableStreamDriver {
    cancel_next_upload: AtomicBool,
    cancelled: Arc<AtomicBool>,
    objects: Mutex<BTreeMap<String, Vec<u8>>>,
    delete_count: AtomicUsize,
}

impl RecoverableStreamDriver {
    fn new(cancelled: Arc<AtomicBool>) -> Self {
        Self {
            cancel_next_upload: AtomicBool::new(true),
            cancelled,
            objects: Mutex::new(BTreeMap::new()),
            delete_count: AtomicUsize::new(0),
        }
    }

    fn delete_count(&self) -> usize {
        self.delete_count.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl StorageDriver for RecoverableStreamDriver {
    async fn put(&self, path: &str, data: &[u8]) -> crate::errors::Result<String> {
        self.objects
            .lock()
            .expect("recoverable stream driver lock should succeed")
            .insert(path.to_string(), data.to_vec());
        Ok(path.to_string())
    }

    async fn get(&self, path: &str) -> crate::errors::Result<Vec<u8>> {
        self.objects
            .lock()
            .expect("recoverable stream driver lock should succeed")
            .get(path)
            .cloned()
            .ok_or_else(|| crate::errors::AsterError::storage_driver_error("object not found"))
    }

    async fn get_stream(
        &self,
        _path: &str,
    ) -> crate::errors::Result<Box<dyn AsyncRead + Unpin + Send>> {
        unreachable!()
    }

    async fn delete(&self, path: &str) -> crate::errors::Result<()> {
        self.delete_count.fetch_add(1, Ordering::SeqCst);
        self.objects
            .lock()
            .expect("recoverable stream driver lock should succeed")
            .remove(path);
        Ok(())
    }

    async fn exists(&self, path: &str) -> crate::errors::Result<bool> {
        Ok(self
            .objects
            .lock()
            .expect("recoverable stream driver lock should succeed")
            .contains_key(path))
    }

    async fn metadata(&self, path: &str) -> crate::errors::Result<BlobMetadata> {
        let size = self.get(path).await?.len();
        Ok(BlobMetadata {
            size: u64::try_from(size).expect("test object size should fit u64"),
            content_type: None,
        })
    }

    fn as_stream_upload(&self) -> Option<&dyn StreamUploadDriver> {
        Some(self)
    }
}

#[async_trait]
impl StreamUploadDriver for RecoverableStreamDriver {
    async fn put_file(
        &self,
        storage_path: &str,
        local_path: &str,
    ) -> crate::errors::Result<String> {
        let data = tokio::fs::read(local_path).await.map_err(|error| {
            crate::errors::AsterError::storage_driver_error(format!(
                "read test local file: {error}"
            ))
        })?;
        self.put(storage_path, &data).await
    }

    async fn put_reader(
        &self,
        storage_path: &str,
        mut reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
        _size: i64,
    ) -> crate::errors::Result<String> {
        let mut data = Vec::new();
        if self.cancel_next_upload.swap(false, Ordering::SeqCst) {
            let mut buf = [0_u8; 4];
            let read = reader.read(&mut buf).await.map_err(|error| {
                crate::errors::AsterError::storage_driver_error(format!(
                    "read first test upload chunk: {error}"
                ))
            })?;
            data.extend_from_slice(&buf[..read]);
            self.cancelled.store(true, Ordering::SeqCst);
            let mut next = [0_u8; 4];
            return match reader.read(&mut next).await {
                Ok(_) => Err(crate::errors::AsterError::storage_driver_error(
                    "reader continued after cancellation",
                )),
                Err(error) => Err(crate::errors::AsterError::storage_driver_error(format!(
                    "reader stopped after cancellation: {error}"
                ))),
            };
        }

        reader.read_to_end(&mut data).await.map_err(|error| {
            crate::errors::AsterError::storage_driver_error(format!(
                "read retry test upload: {error}"
            ))
        })?;
        self.put(storage_path, &data).await
    }
}

#[async_trait]
impl StorageDriver for CancelAfterFirstReadDriver {
    async fn put(&self, _path: &str, _data: &[u8]) -> crate::errors::Result<String> {
        unreachable!("temp import should use put_reader")
    }

    async fn get(&self, _path: &str) -> crate::errors::Result<Vec<u8>> {
        unreachable!()
    }

    async fn get_stream(
        &self,
        _path: &str,
    ) -> crate::errors::Result<Box<dyn AsyncRead + Unpin + Send>> {
        unreachable!()
    }

    async fn delete(&self, _path: &str) -> crate::errors::Result<()> {
        self.delete_count.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn exists(&self, _path: &str) -> crate::errors::Result<bool> {
        Ok(false)
    }

    async fn metadata(&self, _path: &str) -> crate::errors::Result<BlobMetadata> {
        unreachable!()
    }

    fn as_stream_upload(&self) -> Option<&dyn StreamUploadDriver> {
        Some(self)
    }
}

#[async_trait]
impl StreamUploadDriver for CancelAfterFirstReadDriver {
    async fn put_file(
        &self,
        _storage_path: &str,
        _local_path: &str,
    ) -> crate::errors::Result<String> {
        unreachable!("cancellable temp import should not use put_file")
    }

    async fn put_reader(
        &self,
        _storage_path: &str,
        mut reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
        _size: i64,
    ) -> crate::errors::Result<String> {
        let mut buf = [0_u8; 4];
        let first = reader
            .read(&mut buf)
            .await
            .map_err(|error| crate::errors::AsterError::storage_driver_error(error.to_string()))?;
        assert!(first > 0, "first reader chunk should contain payload bytes");
        self.cancelled.store(true, Ordering::SeqCst);
        match reader.read(&mut buf).await {
            Ok(_) => Err(crate::errors::AsterError::storage_driver_error(
                "reader continued after cancellation",
            )),
            Err(error) => Err(crate::errors::AsterError::storage_driver_error(format!(
                "reader stopped after cancellation: {error}"
            ))),
        }
    }
}

struct CancelAfterLocalPathDriver {
    inner: crate::storage::drivers::local::LocalDriver,
    cancelled: Arc<AtomicBool>,
}

impl CancelAfterLocalPathDriver {
    fn new(policy: &storage_policy::Model, cancelled: Arc<AtomicBool>) -> Self {
        Self {
            inner: crate::storage::drivers::local::LocalDriver::new(policy)
                .expect("cancel after local path test driver should initialize"),
            cancelled,
        }
    }
}

#[async_trait]
impl StorageDriver for CancelAfterLocalPathDriver {
    async fn put(&self, path: &str, data: &[u8]) -> crate::errors::Result<String> {
        self.inner.put(path, data).await
    }

    async fn get(&self, path: &str) -> crate::errors::Result<Vec<u8>> {
        self.inner.get(path).await
    }

    async fn get_stream(
        &self,
        path: &str,
    ) -> crate::errors::Result<Box<dyn AsyncRead + Unpin + Send>> {
        self.inner.get_stream(path).await
    }

    async fn delete(&self, path: &str) -> crate::errors::Result<()> {
        self.inner.delete(path).await
    }

    async fn exists(&self, path: &str) -> crate::errors::Result<bool> {
        self.inner.exists(path).await
    }

    async fn metadata(&self, path: &str) -> crate::errors::Result<BlobMetadata> {
        self.inner.metadata(path).await
    }

    fn as_local_path(&self) -> Option<&dyn LocalPathStorageDriver> {
        Some(self)
    }
}

impl LocalPathStorageDriver for CancelAfterLocalPathDriver {
    fn resolve_local_path(&self, path: &str) -> crate::errors::Result<PathBuf> {
        let resolved = self
            .inner
            .as_local_path()
            .unwrap()
            .resolve_local_path(path)?;
        self.cancelled.store(true, Ordering::SeqCst);
        Ok(resolved)
    }
}

fn snapshot_dir_tree(path: &Path) -> std::io::Result<BTreeSet<String>> {
    fn walk(root: &Path, current: &Path, entries: &mut BTreeSet<String>) -> std::io::Result<()> {
        for entry in std::fs::read_dir(current)? {
            let entry = entry?;
            let path = entry.path();
            let relative = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                entries.insert(format!("{relative}/"));
                walk(root, &path, entries)?;
            } else {
                entries.insert(relative);
            }
        }
        Ok(())
    }

    let mut entries = BTreeSet::new();
    if !path.exists() {
        return Ok(entries);
    }
    walk(path, path, &mut entries)?;
    Ok(entries)
}

async fn build_test_state() -> (PrimaryAppState, PathBuf, storage_policy::Model, user::Model) {
    let temp_root = std::env::temp_dir().join(format!(
        "asterdrive-workspace-storage-service-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&temp_root).expect("temp root should be created");
    let uploads_root = temp_root.join("uploads");
    std::fs::create_dir_all(&uploads_root).expect("uploads root should be created");

    let db = crate::db::connect_with_metrics(
        &DatabaseConfig {
            url: "sqlite::memory:".to_string(),
            pool_size: 1,
            retry_count: 0,
        },
        crate::metrics_core::NoopMetrics::arc(),
    )
    .await
    .unwrap();
    Migrator::up(&db, None).await.unwrap();

    let now = Utc::now();
    let policy = storage_policy::ActiveModel {
        name: Set("Test Local Policy".to_string()),
        driver_type: Set(DriverType::Local),
        endpoint: Set(String::new()),
        bucket: Set(String::new()),
        access_key: Set(String::new()),
        secret_key: Set(String::new()),
        base_path: Set(uploads_root.to_string_lossy().into_owned()),
        max_file_size: Set(0),
        allowed_types: Set(crate::types::StoredStoragePolicyAllowedTypes::empty()),
        options: Set(crate::types::StoredStoragePolicyOptions::empty()),
        is_default: Set(true),
        chunk_size: Set(5_242_880),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    let user = user::ActiveModel {
        username: Set("storage-conflict-user".to_string()),
        email: Set("storage-conflict@example.com".to_string()),
        password_hash: Set("not-used".to_string()),
        role: Set(UserRole::User),
        status: Set(UserStatus::Active),
        session_version: Set(0),
        email_verified_at: Set(Some(now)),
        pending_email: Set(None),
        storage_used: Set(0),
        storage_quota: Set(0),
        policy_group_id: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
        config: Set(None),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    let runtime_config = Arc::new(RuntimeConfig::new());
    let cache = cache::create_cache(&CacheConfig {
        ..Default::default()
    })
    .await;
    let mut config = Config::default();
    config.server.temp_dir = temp_root.join(".tmp").to_string_lossy().into_owned();
    config.server.upload_temp_dir = temp_root.join(".uploads").to_string_lossy().into_owned();

    let (storage_change_tx, _) = tokio::sync::broadcast::channel(
        crate::services::storage_change_service::STORAGE_CHANGE_CHANNEL_CAPACITY,
    );
    let share_download_rollback =
        crate::services::share_service::spawn_detached_share_download_rollback_queue(
            db.clone(),
            crate::config::operations::share_download_rollback_queue_capacity(&runtime_config),
        );

    let state = PrimaryAppState {
        db_handles: crate::db::DbHandles::single(db),
        driver_registry: Arc::new(DriverRegistry::noop()),
        runtime_config: runtime_config.clone(),
        policy_snapshot: Arc::new(PolicySnapshot::new()),
        config: Arc::new(config),
        cache,
        metrics: crate::metrics_core::NoopMetrics::arc(),
        mail_sender: sender::runtime_sender(runtime_config),
        storage_change_tx,
        share_download_rollback,
        background_task_dispatch_wakeup:
            crate::runtime::PrimaryAppState::new_background_task_dispatch_wakeup(),
        remote_protocol: crate::runtime::PrimaryAppState::new_remote_protocol(),
    };

    (state, temp_root, policy, user)
}

#[tokio::test]
async fn exact_name_conflict_cleans_preuploaded_local_blob() {
    let (state, temp_root, policy, user) = build_test_state().await;
    let scope = WorkspaceStorageScope::Personal { user_id: user.id };
    let uploads_root = temp_root.join("uploads");

    let first_temp = temp_root.join("first.bin");
    let first_bytes = b"first payload";
    tokio::fs::write(&first_temp, first_bytes).await.unwrap();
    store_from_temp_with_hints(
        &state,
        StoreFromTempParams::new(
            scope,
            None,
            "dup.txt",
            &first_temp.to_string_lossy(),
            first_bytes.len() as i64,
        ),
        StoreFromTempHints {
            resolved_policy: Some(policy.clone()),
            precomputed_hash: None,
            actor_username: None,
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let blob_count_before = file_blob::Entity::find()
        .count(state.writer_db())
        .await
        .unwrap();
    let upload_tree_before = snapshot_dir_tree(&uploads_root).unwrap();

    let second_temp = temp_root.join("second.bin");
    let second_bytes = b"second payload should be cleaned";
    tokio::fs::write(&second_temp, second_bytes).await.unwrap();
    let err = store_from_temp_exact_name_with_hints(
        &state,
        StoreFromTempParams::new(
            scope,
            None,
            "dup.txt",
            &second_temp.to_string_lossy(),
            second_bytes.len() as i64,
        ),
        StoreFromTempHints {
            resolved_policy: Some(policy),
            precomputed_hash: None,
            actor_username: None,
            ..Default::default()
        },
    )
    .await
    .expect_err("exact-name conflict should fail");

    assert!(
        err.message().contains("already exists"),
        "unexpected error message: {}",
        err.message()
    );

    let blob_count_after = file_blob::Entity::find()
        .count(state.writer_db())
        .await
        .unwrap();
    let upload_tree_after = snapshot_dir_tree(&uploads_root).unwrap();
    assert_eq!(blob_count_after, blob_count_before);
    assert_eq!(upload_tree_after, upload_tree_before);

    drop(state);
    let _ = std::fs::remove_dir_all(&temp_root);
}

#[tokio::test]
async fn temp_store_silent_exact_name_updates_storage_without_storage_event() {
    let (state, temp_root, policy, user) = build_test_state().await;
    let scope = WorkspaceStorageScope::Personal { user_id: user.id };
    let mut storage_events = state.storage_change_tx.subscribe();

    let normal_temp = temp_root.join("normal.bin");
    let normal_bytes = b"normal event";
    tokio::fs::write(&normal_temp, normal_bytes).await.unwrap();
    let normal = store_from_temp_exact_name_with_hints(
        &state,
        StoreFromTempParams::new(
            scope,
            None,
            "normal.txt",
            &normal_temp.to_string_lossy(),
            normal_bytes.len() as i64,
        ),
        StoreFromTempHints {
            resolved_policy: Some(policy.clone()),
            precomputed_hash: None,
            actor_username: None,
            ..Default::default()
        },
    )
    .await
    .expect("normal temp store should succeed");

    let normal_event = tokio::time::timeout(Duration::from_secs(1), storage_events.recv())
        .await
        .expect("normal temp store should publish a storage event")
        .expect("storage change channel should stay open");
    assert_eq!(normal_event.file_ids, vec![normal.id]);
    assert_eq!(normal_event.storage_delta, Some(normal_bytes.len() as i64));

    let silent_temp = temp_root.join("silent.bin");
    let silent_bytes = b"silent storage";
    tokio::fs::write(&silent_temp, silent_bytes).await.unwrap();
    let silent = store_from_temp_exact_name_silent_with_hints(
        &state,
        StoreFromTempParams::new(
            scope,
            None,
            "silent.txt",
            &silent_temp.to_string_lossy(),
            silent_bytes.len() as i64,
        ),
        StoreFromTempHints {
            resolved_policy: Some(policy),
            precomputed_hash: None,
            actor_username: None,
            ..Default::default()
        },
    )
    .await
    .expect("silent temp store should succeed");
    assert_eq!(silent.name, "silent.txt");

    let owner = crate::db::repository::user_repo::find_by_id(state.writer_db(), user.id)
        .await
        .expect("owner should still exist");
    assert_eq!(
        owner.storage_used,
        normal_bytes.len() as i64 + silent_bytes.len() as i64
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(50), storage_events.recv())
            .await
            .is_err(),
        "silent temp store should not publish a storage event"
    );

    drop(state);
    let _ = std::fs::remove_dir_all(&temp_root);
}

#[tokio::test]
async fn temp_preupload_quota_failure_does_not_write_blob() {
    let (state, temp_root, policy, user) = build_test_state().await;
    let scope = WorkspaceStorageScope::Personal { user_id: user.id };
    let uploads_root = temp_root.join("uploads");
    let driver = Arc::new(CountingUploadDriver::new(&policy));
    state
        .driver_registry
        .insert_for_test(policy.id, driver.clone());

    let payload = b"payload larger than quota";
    let temp_file = temp_root.join("quota-fail-temp.bin");
    tokio::fs::write(&temp_file, payload).await.unwrap();

    let mut active: user::ActiveModel = user.clone().into();
    active.storage_quota = Set((payload.len() as i64) - 1);
    active.update(state.writer_db()).await.unwrap();

    let upload_tree_before = snapshot_dir_tree(&uploads_root).unwrap();
    let err = store_from_temp_with_hints(
        &state,
        StoreFromTempParams::new(
            scope,
            None,
            "quota-fail-temp.bin",
            &temp_file.to_string_lossy(),
            payload.len() as i64,
        ),
        StoreFromTempHints {
            resolved_policy: Some(policy),
            precomputed_hash: None,
            actor_username: None,
            ..Default::default()
        },
    )
    .await
    .expect_err("quota failure should stop temp preupload before blob write");

    assert_eq!(err.code(), "E032");
    assert_eq!(driver.put_file_count(), 0);
    assert_eq!(
        file_blob::Entity::find()
            .count(state.writer_db())
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        snapshot_dir_tree(&uploads_root).unwrap(),
        upload_tree_before
    );

    drop(state);
    let _ = std::fs::remove_dir_all(&temp_root);
}

#[tokio::test]
async fn preuploaded_quota_failure_cleans_local_blob() {
    let (state, temp_root, policy, user) = build_test_state().await;
    let scope = WorkspaceStorageScope::Personal { user_id: user.id };
    let uploads_root = temp_root.join("uploads");
    let driver = Arc::new(CountingUploadDriver::new(&policy));
    state
        .driver_registry
        .insert_for_test(policy.id, driver.clone());

    let payload = b"already uploaded but over quota";
    let temp_file = temp_root.join("quota-fail-preuploaded.bin");
    tokio::fs::write(&temp_file, payload).await.unwrap();

    let prepared = prepare_non_dedup_blob_upload(&policy, payload.len() as i64).unwrap();
    upload_temp_file_to_prepared_blob(driver.as_ref(), &prepared, &temp_file.to_string_lossy())
        .await
        .unwrap();
    assert_eq!(driver.put_file_count(), 1);
    assert!(
        !snapshot_dir_tree(&uploads_root).unwrap().is_empty(),
        "preuploaded blob should exist before quota failure"
    );

    let mut active: user::ActiveModel = user.clone().into();
    active.storage_quota = Set((payload.len() as i64) - 1);
    active.update(state.writer_db()).await.unwrap();

    let err = store_preuploaded_nondedup(
        &state,
        StorePreuploadedNondedupParams {
            scope,
            folder_id: None,
            filename: "quota-fail-preuploaded.bin",
            size: payload.len() as i64,
            existing_file_id: None,
            skip_lock_check: false,
            policy: &policy,
            preuploaded_blob: prepared,
            actor_username: None,
        },
    )
    .await
    .expect_err("quota failure should clean preuploaded blob");

    assert_eq!(err.code(), "E032");
    assert_eq!(
        file_blob::Entity::find()
            .count(state.writer_db())
            .await
            .unwrap(),
        0
    );
    assert!(snapshot_dir_tree(&uploads_root).unwrap().is_empty());

    drop(state);
    let _ = std::fs::remove_dir_all(&temp_root);
}

#[tokio::test]
async fn slow_nondedup_preupload_does_not_block_task_listing() {
    let (state, temp_root, policy, user) = build_test_state().await;
    let scope = WorkspaceStorageScope::Personal { user_id: user.id };
    let (blocking_driver, entered_rx, release_put_file) = BlockingPutFileDriver::new(&policy);
    state
        .driver_registry
        .insert_for_test(policy.id, Arc::new(blocking_driver));

    let temp_file = temp_root.join("slow-upload.bin");
    let payload = b"slow upload payload";
    tokio::fs::write(&temp_file, payload).await.unwrap();

    let state_for_store = state.clone();
    let policy_for_store = policy.clone();
    let temp_path = temp_file.to_string_lossy().into_owned();
    let store_task = tokio::spawn(async move {
        store_from_temp_with_hints(
            &state_for_store,
            StoreFromTempParams::new(
                scope,
                None,
                "slow-upload.bin",
                &temp_path,
                payload.len() as i64,
            ),
            StoreFromTempHints {
                resolved_policy: Some(policy_for_store),
                precomputed_hash: None,
                actor_username: None,
                ..Default::default()
            },
        )
        .await
    });

    tokio::time::timeout(Duration::from_secs(1), entered_rx)
        .await
        .expect("preupload should reach put_file")
        .expect("put_file entry signal should be sent");

    let page = tokio::time::timeout(
        Duration::from_millis(250),
        crate::services::task_service::list_tasks_paginated_in_scope(&state, scope, 20, 0),
    )
    .await
    .expect("task listing should not wait for blocked blob upload")
    .expect("task listing should succeed");
    assert_eq!(page.total, 0);
    assert!(page.items.is_empty());

    release_put_file.notify_one();

    let stored = tokio::time::timeout(Duration::from_secs(1), store_task)
        .await
        .expect("store task should finish after releasing upload")
        .expect("store task should join")
        .expect("store task should succeed");
    assert_eq!(stored.name, "slow-upload.bin");

    drop(state);
    let _ = std::fs::remove_dir_all(&temp_root);
}

#[tokio::test]
async fn slow_dedup_blob_publish_does_not_block_task_listing() {
    let (state, temp_root, policy, user) = build_test_state().await;
    let policy = enable_content_dedup(&policy);
    let scope = WorkspaceStorageScope::Personal { user_id: user.id };
    let (blocking_driver, entered_rx, release_target) = BlockingLocalPathDriver::new(&policy);
    state
        .driver_registry
        .insert_for_test(policy.id, Arc::new(blocking_driver));

    let temp_file = temp_root.join("slow-dedup-upload.bin");
    let payload = b"slow dedup upload payload";
    tokio::fs::write(&temp_file, payload).await.unwrap();

    let state_for_store = state.clone();
    let policy_for_store = policy.clone();
    let temp_path = temp_file.to_string_lossy().into_owned();
    let store_task = tokio::task::spawn_blocking(move || {
        tokio::runtime::Handle::current().block_on(store_from_temp_with_hints(
            &state_for_store,
            StoreFromTempParams::new(
                scope,
                None,
                "slow-dedup-upload.bin",
                &temp_path,
                payload.len() as i64,
            ),
            StoreFromTempHints {
                resolved_policy: Some(policy_for_store),
                precomputed_hash: None,
                actor_username: None,
                ..Default::default()
            },
        ))
    });

    tokio::time::timeout(Duration::from_secs(1), entered_rx)
        .await
        .expect("dedup blob publish should resolve target path")
        .expect("target path entry signal should be sent");

    let page = tokio::time::timeout(
        Duration::from_millis(250),
        crate::services::task_service::list_tasks_paginated_in_scope(&state, scope, 20, 0),
    )
    .await
    .expect("task listing should not wait for blocked dedup blob publish")
    .expect("task listing should succeed");
    assert_eq!(page.total, 0);
    assert!(page.items.is_empty());

    release_target.send(()).unwrap();

    let stored = tokio::time::timeout(Duration::from_secs(1), store_task)
        .await
        .expect("store task should finish after releasing target path")
        .expect("store task should join")
        .expect("store task should succeed");
    assert_eq!(stored.name, "slow-dedup-upload.bin");

    drop(state);
    let _ = std::fs::remove_dir_all(&temp_root);
}

#[tokio::test]
async fn temp_store_cancellation_before_hash_does_not_touch_temp_file() {
    let (state, temp_root, policy, user) = build_test_state().await;
    let policy = enable_content_dedup(&policy);
    let scope = WorkspaceStorageScope::Personal { user_id: user.id };
    let cancelled = Arc::new(AtomicBool::new(true));

    let err = store_from_temp_with_hints(
        &state,
        StoreFromTempParams::new(
            scope,
            None,
            "missing.bin",
            &temp_root.join("does-not-exist.bin").to_string_lossy(),
            1,
        ),
        StoreFromTempHints {
            resolved_policy: Some(policy),
            operation_context: cancellation_context(cancelled),
            ..Default::default()
        },
    )
    .await
    .expect_err("pre-cancelled temp import should stop before opening temp file");

    assert_eq!(
        err.api_error_code_override(),
        Some(ApiErrorCode::TaskWorkerShutdownRequested)
    );
    assert_eq!(
        file_blob::Entity::find()
            .count(state.writer_db())
            .await
            .unwrap(),
        0
    );

    drop(state);
    let _ = std::fs::remove_dir_all(&temp_root);
}

#[tokio::test]
async fn cancelled_before_hash_can_resume_from_same_temp_file() {
    let (state, temp_root, policy, user) = build_test_state().await;
    let policy = enable_content_dedup(&policy);
    let scope = WorkspaceStorageScope::Personal { user_id: user.id };
    let cancelled = Arc::new(AtomicBool::new(true));

    let payload = b"resume before hash payload";
    let temp_file = temp_root.join("resume-before-hash.bin");
    tokio::fs::write(&temp_file, payload).await.unwrap();
    let temp_path = temp_file.to_string_lossy().into_owned();

    let err = store_from_temp_with_hints(
        &state,
        StoreFromTempParams::new(
            scope,
            None,
            "resume-before-hash.bin",
            &temp_path,
            payload.len() as i64,
        ),
        StoreFromTempHints {
            resolved_policy: Some(policy.clone()),
            operation_context: cancellation_context(cancelled.clone()),
            ..Default::default()
        },
    )
    .await
    .expect_err("first import should be interrupted before hash");

    assert_eq!(
        err.api_error_code_override(),
        Some(ApiErrorCode::TaskWorkerShutdownRequested)
    );
    assert!(temp_file.exists());
    assert_eq!(
        file::Entity::find().count(state.writer_db()).await.unwrap(),
        0
    );
    assert_eq!(
        file_blob::Entity::find()
            .count(state.writer_db())
            .await
            .unwrap(),
        0
    );

    cancelled.store(false, Ordering::SeqCst);
    let stored = store_from_temp_with_hints(
        &state,
        StoreFromTempParams::new(
            scope,
            None,
            "resume-before-hash.bin",
            &temp_path,
            payload.len() as i64,
        ),
        StoreFromTempHints {
            resolved_policy: Some(policy),
            operation_context: cancellation_context(cancelled),
            ..Default::default()
        },
    )
    .await
    .expect("retry should import the same temp file");

    assert_eq!(stored.name, "resume-before-hash.bin");
    assert_eq!(
        file::Entity::find().count(state.writer_db()).await.unwrap(),
        1
    );
    assert_eq!(
        file_blob::Entity::find()
            .count(state.writer_db())
            .await
            .unwrap(),
        1
    );

    drop(state);
    let _ = std::fs::remove_dir_all(&temp_root);
}

#[tokio::test]
async fn temp_store_cancellation_during_stream_upload_cleans_preuploaded_blob() {
    let (state, temp_root, mut policy, user) = build_test_state().await;
    policy.driver_type = DriverType::S3;
    let scope = WorkspaceStorageScope::Personal { user_id: user.id };
    let cancelled = Arc::new(AtomicBool::new(false));
    let driver = Arc::new(CancelAfterFirstReadDriver::new(cancelled.clone()));
    state
        .driver_registry
        .insert_for_test(policy.id, driver.clone());

    let payload = b"streaming cancellation payload";
    let temp_file = temp_root.join("cancel-stream.bin");
    tokio::fs::write(&temp_file, payload).await.unwrap();

    let err = store_from_temp_with_hints(
        &state,
        StoreFromTempParams::new(
            scope,
            None,
            "cancel-stream.bin",
            &temp_file.to_string_lossy(),
            payload.len() as i64,
        ),
        StoreFromTempHints {
            resolved_policy: Some(policy),
            operation_context: cancellation_context(cancelled),
            ..Default::default()
        },
    )
    .await
    .expect_err("streaming temp import should surface cancellation");

    assert_eq!(
        err.api_error_code_override(),
        Some(ApiErrorCode::TaskWorkerShutdownRequested)
    );
    assert_eq!(driver.delete_count(), 1);
    assert_eq!(
        file_blob::Entity::find()
            .count(state.writer_db())
            .await
            .unwrap(),
        0
    );

    drop(state);
    let _ = std::fs::remove_dir_all(&temp_root);
}

#[tokio::test]
async fn cancelled_during_stream_upload_can_resume_from_same_temp_file() {
    let (state, temp_root, mut policy, user) = build_test_state().await;
    policy.driver_type = DriverType::S3;
    let scope = WorkspaceStorageScope::Personal { user_id: user.id };
    let cancelled = Arc::new(AtomicBool::new(false));
    let driver = Arc::new(RecoverableStreamDriver::new(cancelled.clone()));
    state
        .driver_registry
        .insert_for_test(policy.id, driver.clone());

    let payload = b"resume stream upload payload";
    let temp_file = temp_root.join("resume-stream.bin");
    tokio::fs::write(&temp_file, payload).await.unwrap();
    let temp_path = temp_file.to_string_lossy().into_owned();

    let err = store_from_temp_with_hints(
        &state,
        StoreFromTempParams::new(
            scope,
            None,
            "resume-stream.bin",
            &temp_path,
            payload.len() as i64,
        ),
        StoreFromTempHints {
            resolved_policy: Some(policy.clone()),
            operation_context: cancellation_context(cancelled.clone()),
            ..Default::default()
        },
    )
    .await
    .expect_err("first stream upload should be interrupted");

    assert_eq!(
        err.api_error_code_override(),
        Some(ApiErrorCode::TaskWorkerShutdownRequested)
    );
    assert_eq!(driver.delete_count(), 1);
    assert!(temp_file.exists());
    assert_eq!(
        file::Entity::find().count(state.writer_db()).await.unwrap(),
        0
    );
    assert_eq!(
        file_blob::Entity::find()
            .count(state.writer_db())
            .await
            .unwrap(),
        0
    );

    cancelled.store(false, Ordering::SeqCst);
    let stored = store_from_temp_with_hints(
        &state,
        StoreFromTempParams::new(
            scope,
            None,
            "resume-stream.bin",
            &temp_path,
            payload.len() as i64,
        ),
        StoreFromTempHints {
            resolved_policy: Some(policy),
            operation_context: cancellation_context(cancelled),
            ..Default::default()
        },
    )
    .await
    .expect("retry should stream the same temp file");

    let blob = crate::db::repository::file_repo::find_blob_by_id(state.writer_db(), stored.blob_id)
        .await
        .unwrap();
    assert_eq!(driver.get(&blob.storage_path).await.unwrap(), payload);
    assert_eq!(
        file::Entity::find().count(state.writer_db()).await.unwrap(),
        1
    );
    assert_eq!(
        file_blob::Entity::find()
            .count(state.writer_db())
            .await
            .unwrap(),
        1
    );

    drop(state);
    let _ = std::fs::remove_dir_all(&temp_root);
}

#[tokio::test]
async fn cancellable_local_temp_store_uses_local_fast_path_without_driver_stream_upload() {
    let (state, temp_root, policy, user) = build_test_state().await;
    let scope = WorkspaceStorageScope::Personal { user_id: user.id };
    let cancelled = Arc::new(AtomicBool::new(false));
    let driver = Arc::new(CountingUploadDriver::new(&policy));
    state
        .driver_registry
        .insert_for_test(policy.id, driver.clone());

    let payload = b"local fast path payload";
    let temp_file = temp_root.join("local-fast-path.bin");
    tokio::fs::write(&temp_file, payload).await.unwrap();
    let temp_path = temp_file.to_string_lossy().into_owned();

    let stored = store_from_temp_with_hints(
        &state,
        StoreFromTempParams::new(
            scope,
            None,
            "local-fast-path.bin",
            &temp_path,
            payload.len() as i64,
        ),
        StoreFromTempHints {
            resolved_policy: Some(policy),
            operation_context: cancellation_context(cancelled),
            ..Default::default()
        },
    )
    .await
    .expect("cancellable local temp store should succeed");

    assert_eq!(driver.put_file_count(), 0);
    assert_eq!(driver.put_reader_count(), 0);
    assert!(
        temp_file.exists(),
        "cancellable local fast path should preserve the source temp file for retry safety"
    );
    let blob = crate::db::repository::file_repo::find_blob_by_id(state.writer_db(), stored.blob_id)
        .await
        .unwrap();
    assert_eq!(
        driver.get(&blob.storage_path).await.unwrap(),
        payload,
        "stored local blob should contain original payload"
    );

    drop(state);
    let _ = std::fs::remove_dir_all(&temp_root);
}

#[tokio::test]
async fn cancelled_after_local_preupload_staging_can_resume_from_same_temp_file() {
    let (state, temp_root, policy, user) = build_test_state().await;
    let scope = WorkspaceStorageScope::Personal { user_id: user.id };
    let uploads_root = temp_root.join("uploads");

    let payload = b"resume local staging payload";
    let temp_file = temp_root.join("resume-local-stage.bin");
    tokio::fs::write(&temp_file, payload).await.unwrap();
    let temp_path = temp_file.to_string_lossy().into_owned();

    let err = store_from_temp_with_hints(
        &state,
        StoreFromTempParams::new(
            scope,
            None,
            "resume-local-stage.bin",
            &temp_path,
            payload.len() as i64,
        ),
        StoreFromTempHints {
            resolved_policy: Some(policy.clone()),
            operation_context: cancel_when_storage_file_exists_context(uploads_root.clone()),
            ..Default::default()
        },
    )
    .await
    .expect_err("first local preupload should be interrupted after staging");

    assert_eq!(
        err.api_error_code_override(),
        Some(ApiErrorCode::TaskWorkerShutdownRequested)
    );
    assert!(temp_file.exists());
    assert_eq!(
        file::Entity::find().count(state.writer_db()).await.unwrap(),
        0
    );
    assert_eq!(
        file_blob::Entity::find()
            .count(state.writer_db())
            .await
            .unwrap(),
        0
    );
    let remaining_upload_entries = snapshot_dir_tree(&uploads_root).unwrap();
    assert!(
        remaining_upload_entries
            .iter()
            .all(|entry| entry.ends_with('/')),
        "cancelled local preupload should cleanup staged files: {remaining_upload_entries:?}"
    );

    let stored = store_from_temp_with_hints(
        &state,
        StoreFromTempParams::new(
            scope,
            None,
            "resume-local-stage.bin",
            &temp_path,
            payload.len() as i64,
        ),
        StoreFromTempHints {
            resolved_policy: Some(policy.clone()),
            ..Default::default()
        },
    )
    .await
    .expect("retry should import the same local temp file");

    let driver = state.driver_registry.get_driver(&policy).unwrap();
    let blob = crate::db::repository::file_repo::find_blob_by_id(state.writer_db(), stored.blob_id)
        .await
        .unwrap();
    assert_eq!(driver.get(&blob.storage_path).await.unwrap(), payload);
    assert_eq!(
        file::Entity::find().count(state.writer_db()).await.unwrap(),
        1
    );
    assert_eq!(
        file_blob::Entity::find()
            .count(state.writer_db())
            .await
            .unwrap(),
        1
    );

    drop(state);
    let _ = std::fs::remove_dir_all(&temp_root);
}

#[tokio::test]
async fn temp_store_cancellation_after_dedup_staging_rolls_back_object() {
    let (state, temp_root, policy, user) = build_test_state().await;
    let policy = enable_content_dedup(&policy);
    let scope = WorkspaceStorageScope::Personal { user_id: user.id };
    let uploads_root = temp_root.join("uploads");
    let cancelled = Arc::new(AtomicBool::new(false));
    let driver = Arc::new(CancelAfterLocalPathDriver::new(&policy, cancelled.clone()));
    state
        .driver_registry
        .insert_for_test(policy.id, driver.clone());

    let payload = b"dedup staging cancellation payload";
    let temp_file = temp_root.join("cancel-dedup-stage.bin");
    tokio::fs::write(&temp_file, payload).await.unwrap();

    let err = store_from_temp_with_hints(
        &state,
        StoreFromTempParams::new(
            scope,
            None,
            "cancel-dedup-stage.bin",
            &temp_file.to_string_lossy(),
            payload.len() as i64,
        ),
        StoreFromTempHints {
            resolved_policy: Some(policy),
            operation_context: cancellation_context(cancelled),
            ..Default::default()
        },
    )
    .await
    .expect_err("dedup staging cancellation should stop before DB persist");

    assert_eq!(
        err.api_error_code_override(),
        Some(ApiErrorCode::TaskWorkerShutdownRequested)
    );
    assert_eq!(
        file_blob::Entity::find()
            .count(state.writer_db())
            .await
            .unwrap(),
        0
    );
    let remaining_upload_entries = snapshot_dir_tree(&uploads_root).unwrap();
    assert!(
        remaining_upload_entries
            .iter()
            .all(|entry| entry.ends_with('/')),
        "dedup rollback should not leave staged files: {remaining_upload_entries:?}"
    );

    drop(state);
    let _ = std::fs::remove_dir_all(&temp_root);
}

#[tokio::test]
async fn cancelled_after_dedup_staging_can_resume_from_same_temp_file() {
    let (state, temp_root, policy, user) = build_test_state().await;
    let policy = enable_content_dedup(&policy);
    let scope = WorkspaceStorageScope::Personal { user_id: user.id };
    let uploads_root = temp_root.join("uploads");

    let payload = b"resume dedup staging payload";
    let temp_file = temp_root.join("resume-dedup-stage.bin");
    tokio::fs::write(&temp_file, payload).await.unwrap();
    let temp_path = temp_file.to_string_lossy().into_owned();

    let err = store_from_temp_with_hints(
        &state,
        StoreFromTempParams::new(
            scope,
            None,
            "resume-dedup-stage.bin",
            &temp_path,
            payload.len() as i64,
        ),
        StoreFromTempHints {
            resolved_policy: Some(policy.clone()),
            operation_context: cancel_when_storage_file_exists_context(uploads_root.clone()),
            ..Default::default()
        },
    )
    .await
    .expect_err("first dedup import should be interrupted after staging");

    assert_eq!(
        err.api_error_code_override(),
        Some(ApiErrorCode::TaskWorkerShutdownRequested)
    );
    assert!(temp_file.exists());
    assert_eq!(
        file::Entity::find().count(state.writer_db()).await.unwrap(),
        0
    );
    assert_eq!(
        file_blob::Entity::find()
            .count(state.writer_db())
            .await
            .unwrap(),
        0
    );
    let remaining_upload_entries = snapshot_dir_tree(&uploads_root).unwrap();
    assert!(
        remaining_upload_entries
            .iter()
            .all(|entry| entry.ends_with('/')),
        "cancelled dedup staging should rollback staged files: {remaining_upload_entries:?}"
    );

    let stored = store_from_temp_with_hints(
        &state,
        StoreFromTempParams::new(
            scope,
            None,
            "resume-dedup-stage.bin",
            &temp_path,
            payload.len() as i64,
        ),
        StoreFromTempHints {
            resolved_policy: Some(policy.clone()),
            ..Default::default()
        },
    )
    .await
    .expect("retry should import the same dedup temp file");

    let driver = state.driver_registry.get_driver(&policy).unwrap();
    let blob = crate::db::repository::file_repo::find_blob_by_id(state.writer_db(), stored.blob_id)
        .await
        .unwrap();
    assert_eq!(driver.get(&blob.storage_path).await.unwrap(), payload);
    assert_eq!(
        file::Entity::find().count(state.writer_db()).await.unwrap(),
        1
    );
    assert_eq!(
        file_blob::Entity::find()
            .count(state.writer_db())
            .await
            .unwrap(),
        1
    );

    drop(state);
    let _ = std::fs::remove_dir_all(&temp_root);
}
