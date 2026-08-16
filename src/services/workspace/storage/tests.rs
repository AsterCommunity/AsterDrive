//! 工作空间存储服务测试。

use crate::api::api_error_code::ApiErrorCode;
use crate::config::{Config, DatabaseConfig, RuntimeConfig};
use crate::db::repository::{file_create_idempotency_repo, file_repo, folder_repo};
use crate::runtime::PrimaryAppState;
use crate::services::mail::sender;
use crate::storage::{DriverRegistry, PolicySnapshot};
use crate::test_support::snapshot_dir_tree;
use aster_drive_model::entities::{file, file_blob, folder, storage_policy, team, user};
use aster_drive_model::types::{
    ObjectStorageDownloadStrategy, ObjectStorageUploadStrategy, ProviderDownloadFilenameMode,
    ProviderDownloadStrategy, UserRole, UserStatus,
};
use aster_drive_storage::{
    BlobMetadata, ListStorageDriver, LocalPathStorageDriver, StorageDriver, StoragePathVisitor,
    StreamUploadDriver,
};
use aster_forge_cache as cache;
use aster_forge_cache::CacheConfig;
use async_trait::async_trait;
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, IntoActiveModel, PaginatorTrait,
    QueryFilter, Set,
};
use std::collections::BTreeMap;
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
    EmptyFileNameMode, FileWritePrecondition, StorageCancellationCheck, StorageOperationContext,
    StoreFromTempHints, StoreFromTempParams, StorePreuploadedNondedupParams, WorkspaceStorageScope,
    create_empty, create_empty_from_relative_path_with_idempotency, create_empty_with_idempotency,
    parse_relative_upload_path, persist_preuploaded_blob, prepare_non_dedup_blob_upload,
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
            inner: crate::storage::drivers::local::LocalDriver::new(
                &crate::storage::connectors::test_support::local_base_path(policy),
            )
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
    async fn put(&self, path: &str, data: &[u8]) -> aster_drive_storage::Result<String> {
        self.inner.put(path, data).await
    }

    async fn get(&self, path: &str) -> aster_drive_storage::Result<Vec<u8>> {
        self.inner.get(path).await
    }

    async fn get_stream(
        &self,
        path: &str,
    ) -> aster_drive_storage::Result<Box<dyn AsyncRead + Unpin + Send>> {
        self.inner.get_stream(path).await
    }

    async fn delete(&self, path: &str) -> aster_drive_storage::Result<()> {
        self.inner.delete(path).await
    }

    async fn exists(&self, path: &str) -> aster_drive_storage::Result<bool> {
        self.inner.exists(path).await
    }

    async fn metadata(&self, path: &str) -> aster_drive_storage::Result<BlobMetadata> {
        self.inner.metadata(path).await
    }

    fn extensions(&self) -> aster_drive_storage::traits::StorageDriverExtensions<'_> {
        let mut extensions = self.inner.extensions();
        extensions.list = Some(self);
        extensions.stream_upload = Some(self);
        extensions.local_path = self.inner.extensions().local_path;
        extensions
    }
}

#[async_trait]
impl ListStorageDriver for CountingUploadDriver {
    async fn list_paths(&self, prefix: Option<&str>) -> aster_drive_storage::Result<Vec<String>> {
        self.inner.list_paths(prefix).await
    }

    async fn scan_paths(
        &self,
        prefix: Option<&str>,
        visitor: &mut dyn StoragePathVisitor,
    ) -> aster_drive_storage::Result<()> {
        self.inner.scan_paths(prefix, visitor).await
    }
}

#[async_trait]
impl StreamUploadDriver for CountingUploadDriver {
    async fn put_file(
        &self,
        storage_path: &str,
        local_path: &str,
    ) -> aster_drive_storage::Result<String> {
        self.put_file_count.fetch_add(1, Ordering::SeqCst);
        self.inner.put_file(storage_path, local_path).await
    }

    async fn put_reader(
        &self,
        storage_path: &str,
        reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
        size: i64,
    ) -> aster_drive_storage::Result<String> {
        self.put_reader_count.fetch_add(1, Ordering::SeqCst);
        self.inner.put_reader(storage_path, reader, size).await
    }
}

fn enable_content_dedup(policy: &storage_policy::Model) -> storage_policy::Model {
    crate::storage::connectors::test_support::with_local_content_dedup(policy, true)
}

#[derive(Default)]
struct RecordingEmptyDriver {
    objects: Mutex<BTreeMap<String, Vec<u8>>>,
    put_paths: Mutex<Vec<String>>,
    get_paths: Mutex<Vec<String>>,
    delete_paths: Mutex<Vec<String>>,
    exists_paths: Mutex<Vec<String>>,
}

impl RecordingEmptyDriver {
    fn object_paths(&self) -> Vec<String> {
        self.objects
            .lock()
            .expect("recording empty driver object lock should succeed")
            .keys()
            .cloned()
            .collect()
    }

    fn put_paths(&self) -> Vec<String> {
        self.put_paths
            .lock()
            .expect("recording empty driver put lock should succeed")
            .clone()
    }

    fn delete_paths(&self) -> Vec<String> {
        self.delete_paths
            .lock()
            .expect("recording empty driver delete lock should succeed")
            .clone()
    }

    fn assert_no_object_api_calls(&self) {
        assert!(self.put_paths().is_empty(), "unexpected put call");
        assert!(
            self.get_paths
                .lock()
                .expect("recording empty driver get lock should succeed")
                .is_empty(),
            "unexpected get call"
        );
        assert!(self.delete_paths().is_empty(), "unexpected delete call");
        assert!(
            self.exists_paths
                .lock()
                .expect("recording empty driver exists lock should succeed")
                .is_empty(),
            "unexpected exists call"
        );
    }
}

#[async_trait]
impl StorageDriver for RecordingEmptyDriver {
    async fn put(&self, path: &str, data: &[u8]) -> aster_drive_storage::Result<String> {
        self.put_paths
            .lock()
            .expect("recording empty driver put lock should succeed")
            .push(path.to_string());
        self.objects
            .lock()
            .expect("recording empty driver object lock should succeed")
            .insert(path.to_string(), data.to_vec());
        Ok(path.to_string())
    }

    async fn get(&self, path: &str) -> aster_drive_storage::Result<Vec<u8>> {
        self.get_paths
            .lock()
            .expect("recording empty driver get lock should succeed")
            .push(path.to_string());
        self.objects
            .lock()
            .expect("recording empty driver object lock should succeed")
            .get(path)
            .cloned()
            .ok_or_else(|| {
                aster_drive_storage::StorageError::new(
                    aster_drive_storage::StorageErrorKind::NotFound,
                    format!("missing test object {path}"),
                )
            })
    }

    async fn get_stream(
        &self,
        path: &str,
    ) -> aster_drive_storage::Result<Box<dyn AsyncRead + Unpin + Send>> {
        Ok(Box::new(std::io::Cursor::new(self.get(path).await?)))
    }

    async fn delete(&self, path: &str) -> aster_drive_storage::Result<()> {
        self.delete_paths
            .lock()
            .expect("recording empty driver delete lock should succeed")
            .push(path.to_string());
        self.objects
            .lock()
            .expect("recording empty driver object lock should succeed")
            .remove(path);
        Ok(())
    }

    async fn exists(&self, path: &str) -> aster_drive_storage::Result<bool> {
        self.exists_paths
            .lock()
            .expect("recording empty driver exists lock should succeed")
            .push(path.to_string());
        Ok(self
            .objects
            .lock()
            .expect("recording empty driver object lock should succeed")
            .contains_key(path))
    }

    async fn metadata(&self, path: &str) -> aster_drive_storage::Result<BlobMetadata> {
        let size = self
            .objects
            .lock()
            .expect("recording empty driver object lock should succeed")
            .get(path)
            .map(Vec::len)
            .ok_or_else(|| {
                aster_drive_storage::StorageError::new(
                    aster_drive_storage::StorageErrorKind::NotFound,
                    format!("missing test object {path}"),
                )
            })?;
        Ok(BlobMetadata {
            size: u64::try_from(size).expect("test object size should fit u64"),
            content_type: None,
        })
    }
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
                inner: crate::storage::drivers::local::LocalDriver::new(
                    &crate::storage::connectors::test_support::local_base_path(policy),
                )
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
    async fn put(&self, path: &str, data: &[u8]) -> aster_drive_storage::Result<String> {
        self.inner.put(path, data).await
    }

    async fn get(&self, path: &str) -> aster_drive_storage::Result<Vec<u8>> {
        self.inner.get(path).await
    }

    async fn get_stream(
        &self,
        path: &str,
    ) -> aster_drive_storage::Result<Box<dyn AsyncRead + Unpin + Send>> {
        self.inner.get_stream(path).await
    }

    async fn delete(&self, path: &str) -> aster_drive_storage::Result<()> {
        self.inner.delete(path).await
    }

    async fn exists(&self, path: &str) -> aster_drive_storage::Result<bool> {
        self.inner.exists(path).await
    }

    async fn metadata(&self, path: &str) -> aster_drive_storage::Result<BlobMetadata> {
        self.inner.metadata(path).await
    }

    fn extensions(&self) -> aster_drive_storage::traits::StorageDriverExtensions<'_> {
        let mut extensions = self.inner.extensions();
        extensions.list = Some(self);
        extensions.stream_upload = Some(self);
        extensions.local_path = self.inner.extensions().local_path;
        extensions
    }
}

#[async_trait]
impl ListStorageDriver for BlockingPutFileDriver {
    async fn list_paths(&self, prefix: Option<&str>) -> aster_drive_storage::Result<Vec<String>> {
        self.inner.list_paths(prefix).await
    }

    async fn scan_paths(
        &self,
        prefix: Option<&str>,
        visitor: &mut dyn StoragePathVisitor,
    ) -> aster_drive_storage::Result<()> {
        self.inner.scan_paths(prefix, visitor).await
    }
}

#[async_trait]
impl StreamUploadDriver for BlockingPutFileDriver {
    async fn put_file(
        &self,
        storage_path: &str,
        local_path: &str,
    ) -> aster_drive_storage::Result<String> {
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
    ) -> aster_drive_storage::Result<String> {
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
                inner: crate::storage::drivers::local::LocalDriver::new(
                    &crate::storage::connectors::test_support::local_base_path(policy),
                )
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
    async fn put(&self, path: &str, data: &[u8]) -> aster_drive_storage::Result<String> {
        self.inner.put(path, data).await
    }

    async fn get(&self, path: &str) -> aster_drive_storage::Result<Vec<u8>> {
        self.inner.get(path).await
    }

    async fn get_stream(
        &self,
        path: &str,
    ) -> aster_drive_storage::Result<Box<dyn AsyncRead + Unpin + Send>> {
        self.inner.get_stream(path).await
    }

    async fn delete(&self, path: &str) -> aster_drive_storage::Result<()> {
        self.inner.delete(path).await
    }

    async fn exists(&self, path: &str) -> aster_drive_storage::Result<bool> {
        self.inner.exists(path).await
    }

    async fn metadata(&self, path: &str) -> aster_drive_storage::Result<BlobMetadata> {
        self.inner.metadata(path).await
    }

    fn extensions(&self) -> aster_drive_storage::traits::StorageDriverExtensions<'_> {
        aster_drive_storage::traits::StorageDriverExtensions {
            list: Some(self),
            stream_upload: Some(self),
            local_path: Some(self),
            ..Default::default()
        }
    }
}

#[async_trait]
impl ListStorageDriver for BlockingLocalPathDriver {
    async fn list_paths(&self, prefix: Option<&str>) -> aster_drive_storage::Result<Vec<String>> {
        self.inner.list_paths(prefix).await
    }

    async fn scan_paths(
        &self,
        prefix: Option<&str>,
        visitor: &mut dyn StoragePathVisitor,
    ) -> aster_drive_storage::Result<()> {
        self.inner.scan_paths(prefix, visitor).await
    }
}

#[async_trait]
impl StreamUploadDriver for BlockingLocalPathDriver {
    async fn put_file(
        &self,
        storage_path: &str,
        local_path: &str,
    ) -> aster_drive_storage::Result<String> {
        self.inner.put_file(storage_path, local_path).await
    }

    async fn put_reader(
        &self,
        storage_path: &str,
        reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
        size: i64,
    ) -> aster_drive_storage::Result<String> {
        self.inner.put_reader(storage_path, reader, size).await
    }
}

impl LocalPathStorageDriver for BlockingLocalPathDriver {
    fn resolve_local_path(&self, path: &str) -> aster_drive_storage::Result<PathBuf> {
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
        self.inner
            .extensions()
            .local_path
            .unwrap()
            .resolve_local_path(path)
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
    async fn put(&self, path: &str, data: &[u8]) -> aster_drive_storage::Result<String> {
        self.objects
            .lock()
            .expect("recoverable stream driver lock should succeed")
            .insert(path.to_string(), data.to_vec());
        Ok(path.to_string())
    }

    async fn get(&self, path: &str) -> aster_drive_storage::Result<Vec<u8>> {
        self.objects
            .lock()
            .expect("recoverable stream driver lock should succeed")
            .get(path)
            .cloned()
            .ok_or_else(|| {
                aster_drive_storage::StorageError::new(
                    aster_drive_storage::StorageErrorKind::NotFound,
                    "object not found",
                )
            })
    }

    async fn get_stream(
        &self,
        _path: &str,
    ) -> aster_drive_storage::Result<Box<dyn AsyncRead + Unpin + Send>> {
        unreachable!()
    }

    async fn delete(&self, path: &str) -> aster_drive_storage::Result<()> {
        self.delete_count.fetch_add(1, Ordering::SeqCst);
        self.objects
            .lock()
            .expect("recoverable stream driver lock should succeed")
            .remove(path);
        Ok(())
    }

    async fn exists(&self, path: &str) -> aster_drive_storage::Result<bool> {
        Ok(self
            .objects
            .lock()
            .expect("recoverable stream driver lock should succeed")
            .contains_key(path))
    }

    async fn metadata(&self, path: &str) -> aster_drive_storage::Result<BlobMetadata> {
        let size = self.get(path).await?.len();
        Ok(BlobMetadata {
            size: u64::try_from(size).expect("test object size should fit u64"),
            content_type: None,
        })
    }

    fn extensions(&self) -> aster_drive_storage::traits::StorageDriverExtensions<'_> {
        aster_drive_storage::traits::StorageDriverExtensions {
            stream_upload: Some(self),
            ..Default::default()
        }
    }
}

#[async_trait]
impl StreamUploadDriver for RecoverableStreamDriver {
    async fn put_file(
        &self,
        storage_path: &str,
        local_path: &str,
    ) -> aster_drive_storage::Result<String> {
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
    ) -> aster_drive_storage::Result<String> {
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
                Ok(_) => Err(aster_drive_storage::StorageError::new(
                    aster_drive_storage::StorageErrorKind::Precondition,
                    "reader continued after cancellation",
                )),
                Err(error) => Err(aster_drive_storage::StorageError::new(
                    aster_drive_storage::StorageErrorKind::Precondition,
                    format!("reader stopped after cancellation: {error}"),
                )),
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
    async fn put(&self, _path: &str, _data: &[u8]) -> aster_drive_storage::Result<String> {
        unreachable!("temp import should use put_reader")
    }

    async fn get(&self, _path: &str) -> aster_drive_storage::Result<Vec<u8>> {
        unreachable!()
    }

    async fn get_stream(
        &self,
        _path: &str,
    ) -> aster_drive_storage::Result<Box<dyn AsyncRead + Unpin + Send>> {
        unreachable!()
    }

    async fn delete(&self, _path: &str) -> aster_drive_storage::Result<()> {
        self.delete_count.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn exists(&self, _path: &str) -> aster_drive_storage::Result<bool> {
        Ok(false)
    }

    async fn metadata(&self, _path: &str) -> aster_drive_storage::Result<BlobMetadata> {
        unreachable!()
    }

    fn extensions(&self) -> aster_drive_storage::traits::StorageDriverExtensions<'_> {
        aster_drive_storage::traits::StorageDriverExtensions {
            stream_upload: Some(self),
            ..Default::default()
        }
    }
}

#[async_trait]
impl StreamUploadDriver for CancelAfterFirstReadDriver {
    async fn put_file(
        &self,
        _storage_path: &str,
        _local_path: &str,
    ) -> aster_drive_storage::Result<String> {
        unreachable!("cancellable temp import should not use put_file")
    }

    async fn put_reader(
        &self,
        _storage_path: &str,
        mut reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
        _size: i64,
    ) -> aster_drive_storage::Result<String> {
        let mut buf = [0_u8; 4];
        let first = reader
            .read(&mut buf)
            .await
            .map_err(|error| crate::errors::AsterError::storage_driver_error(error.to_string()))?;
        assert!(first > 0, "first reader chunk should contain payload bytes");
        self.cancelled.store(true, Ordering::SeqCst);
        match reader.read(&mut buf).await {
            Ok(_) => Err(aster_drive_storage::StorageError::new(
                aster_drive_storage::StorageErrorKind::Precondition,
                "reader continued after cancellation",
            )),
            Err(error) => Err(aster_drive_storage::StorageError::new(
                aster_drive_storage::StorageErrorKind::Precondition,
                format!("reader stopped after cancellation: {error}"),
            )),
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
            inner: crate::storage::drivers::local::LocalDriver::new(
                &crate::storage::connectors::test_support::local_base_path(policy),
            )
            .expect("cancel after local path test driver should initialize"),
            cancelled,
        }
    }
}

#[async_trait]
impl StorageDriver for CancelAfterLocalPathDriver {
    async fn put(&self, path: &str, data: &[u8]) -> aster_drive_storage::Result<String> {
        self.inner.put(path, data).await
    }

    async fn get(&self, path: &str) -> aster_drive_storage::Result<Vec<u8>> {
        self.inner.get(path).await
    }

    async fn get_stream(
        &self,
        path: &str,
    ) -> aster_drive_storage::Result<Box<dyn AsyncRead + Unpin + Send>> {
        self.inner.get_stream(path).await
    }

    async fn delete(&self, path: &str) -> aster_drive_storage::Result<()> {
        self.inner.delete(path).await
    }

    async fn exists(&self, path: &str) -> aster_drive_storage::Result<bool> {
        self.inner.exists(path).await
    }

    async fn metadata(&self, path: &str) -> aster_drive_storage::Result<BlobMetadata> {
        self.inner.metadata(path).await
    }

    fn extensions(&self) -> aster_drive_storage::traits::StorageDriverExtensions<'_> {
        aster_drive_storage::traits::StorageDriverExtensions {
            local_path: Some(self),
            ..Default::default()
        }
    }
}

impl LocalPathStorageDriver for CancelAfterLocalPathDriver {
    fn resolve_local_path(&self, path: &str) -> aster_drive_storage::Result<PathBuf> {
        let resolved = self
            .inner
            .extensions()
            .local_path
            .unwrap()
            .resolve_local_path(path)?;
        self.cancelled.store(true, Ordering::SeqCst);
        Ok(resolved)
    }
}

struct StorageTestState {
    // Field order drops runtime/database handles before directory cleanup.
    app_state: PrimaryAppState,
    _temp_dir_guard: aster_forge_utils::raii::TempDirGuard,
}

impl std::ops::Deref for StorageTestState {
    type Target = PrimaryAppState;

    fn deref(&self) -> &Self::Target {
        &self.app_state
    }
}

async fn build_test_state() -> (
    StorageTestState,
    PathBuf,
    storage_policy::Model,
    user::Model,
) {
    let temp_root = std::env::temp_dir().join(format!(
        "asterdrive-workspace-storage-service-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&temp_root).expect("temp root should be created");
    let temp_dir_guard = aster_forge_utils::raii::TempDirGuard::new(
        temp_root.clone(),
        "workspace storage service test temporary directory",
    );
    let uploads_root = temp_root.join("uploads");
    std::fs::create_dir_all(&uploads_root).expect("uploads root should be created");

    let db = crate::db::connect_with_metrics(
        &DatabaseConfig {
            url: "sqlite::memory:".into(),
            pool_size: 1,
            retry_count: 0,
        },
        aster_drive_metrics::NoopMetrics::arc(),
    )
    .await
    .unwrap();
    crate::storage::connectors::test_support::migrate_current_storage_test_schema(&db).await;

    let now = Utc::now();
    let mut policy = crate::storage::connectors::test_support::local_policy(
        uploads_root.to_string_lossy().into_owned(),
    );
    policy.name = "Test Local Policy".to_string();
    policy.is_default = true;
    policy.chunk_size = 5_242_880;
    let policy = crate::storage::connectors::test_support::insertable_policy(policy)
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

    let storage_change_bus = crate::services::events::storage_change::StorageChangeBus::new(
        crate::services::events::storage_change::STORAGE_CHANGE_CHANNEL_CAPACITY,
    );
    let share_download_rollback =
        crate::services::share::spawn_detached_share_download_rollback_queue(
            db.clone(),
            crate::config::operations::share_download_rollback_queue_capacity(&runtime_config),
        );

    let state = PrimaryAppState {
        db_handles: aster_forge_db::DbHandles::single(db),
        driver_registry: Arc::new(
            DriverRegistry::noop().expect("built-in storage connector registry"),
        ),
        runtime_config: runtime_config.clone(),
        policy_snapshot: Arc::new(PolicySnapshot::new()),
        config: Arc::new(config),
        cache,
        config_sync: aster_forge_config::ConfigSyncRuntime::disabled_for_test("aster_drive"),
        metrics: aster_drive_metrics::NoopMetrics::arc(),
        mail_sender: sender::runtime_sender(runtime_config),
        storage_change_bus,
        share_download_rollback,
        background_task_dispatch_wakeup:
            crate::runtime::PrimaryAppState::new_background_task_dispatch_wakeup(),
        remote_protocol: crate::runtime::PrimaryAppState::new_remote_protocol(),
    };

    (
        StorageTestState {
            app_state: state,
            _temp_dir_guard: temp_dir_guard,
        },
        temp_root,
        policy,
        user,
    )
}

async fn replace_test_policy(
    state: &PrimaryAppState,
    current: &storage_policy::Model,
    replacement: storage_policy::Model,
) -> storage_policy::Model {
    crate::services::storage_policy::policy::ensure_policy_groups_seeded(state.writer_db())
        .await
        .unwrap();
    let mut active: storage_policy::ActiveModel = current.clone().into();
    active.name = Set(replacement.name);
    active.connector_id = Set(replacement.connector_id);
    active.storage_config = Set(replacement.storage_config);
    active.chunk_size = Set(replacement.chunk_size);
    active.updated_at = Set(Utc::now());
    let updated = active.update(state.writer_db()).await.unwrap();
    state.driver_registry.invalidate(updated.id);
    state
        .driver_registry
        .reload_policy_snapshot(&state.policy_snapshot, state.writer_db())
        .await
        .unwrap();
    updated
}

fn empty_file_connector_policy_cases(
    temp_root: &Path,
) -> Vec<(&'static str, storage_policy::Model)> {
    let behavior = aster_drive_storage::StoragePolicyBehaviorConfig::default();
    vec![
        (
            "local",
            crate::storage::connectors::test_support::local_policy(
                temp_root.join("local").to_string_lossy(),
            ),
        ),
        (
            "s3",
            crate::storage::connectors::test_support::s3_policy(
                "https://s3.example.test",
                "bucket",
                "",
                ObjectStorageUploadStrategy::Presigned,
                ObjectStorageDownloadStrategy::RelayStream,
            ),
        ),
        (
            "azure_blob",
            crate::storage::connectors::test_support::policy(
                "asterdrive.storage.azure_blob",
                1,
                serde_json::json!({
                    "endpoint": "https://account.blob.core.windows.net",
                    "bucket": "container",
                    "base_path": "",
                    "object_storage_upload_strategy": "presigned",
                    "object_storage_download_strategy": "relay_stream"
                }),
                behavior.clone(),
            ),
        ),
        (
            "tencent_cos",
            crate::storage::connectors::test_support::policy(
                "asterdrive.storage.tencent_cos",
                1,
                serde_json::json!({
                    "endpoint": "https://bucket-123.cos.ap-guangzhou.myqcloud.com",
                    "bucket": "bucket-123",
                    "base_path": "",
                    "object_storage_upload_strategy": "relay_stream",
                    "object_storage_download_strategy": "relay_stream"
                }),
                behavior.clone(),
            ),
        ),
        (
            "qiniu",
            crate::storage::connectors::test_support::policy(
                "asterdrive.storage.qiniu",
                1,
                serde_json::json!({
                    "endpoint": "https://s3.cn-east-1.qiniucs.com",
                    "bucket": "bucket-name",
                    "base_path": "",
                    "s3_region": "cn-east-1",
                    "object_storage_upload_strategy": "presigned",
                    "object_storage_download_strategy": "relay_stream"
                }),
                behavior.clone(),
            ),
        ),
        (
            "onedrive",
            crate::storage::connectors::test_support::onedrive_policy_with_download(
                crate::storage::connectors::OneDriveAccountMode::Personal,
                None,
                None,
                None,
                ProviderDownloadStrategy::ServerRelay,
                ProviderDownloadFilenameMode::ProviderNative,
                behavior,
            ),
        ),
        (
            "sftp",
            crate::storage::connectors::test_support::policy(
                "asterdrive.storage.sftp",
                1,
                serde_json::json!({
                    "endpoint": "sftp://storage.example.test:22",
                    "base_path": "",
                    "sftp_host_key_fingerprint": null
                }),
                aster_drive_storage::StoragePolicyBehaviorConfig::default(),
            ),
        ),
    ]
}

#[tokio::test]
async fn canonical_empty_file_use_case_is_connector_independent() {
    for connector in [
        "local",
        "s3",
        "azure_blob",
        "tencent_cos",
        "qiniu",
        "onedrive",
        "sftp",
    ] {
        let (state, temp_root, current_policy, user) = build_test_state().await;
        let (_, replacement) = empty_file_connector_policy_cases(&temp_root)
            .into_iter()
            .find(|(name, _)| *name == connector)
            .expect("connector policy fixture should exist");
        let policy = replace_test_policy(&state, &current_policy, replacement).await;
        let driver = Arc::new(RecordingEmptyDriver::default());
        state
            .driver_registry
            .insert_for_test(policy.id, driver.clone());

        let first = create_empty(
            &state,
            WorkspaceStorageScope::Personal { user_id: user.id },
            None,
            "first-empty.txt",
            EmptyFileNameMode::Exact,
        )
        .await
        .unwrap_or_else(|error| panic!("{connector} first empty file failed: {error}"));
        let second = create_empty(
            &state,
            WorkspaceStorageScope::Personal { user_id: user.id },
            None,
            "second-empty.txt",
            EmptyFileNameMode::Exact,
        )
        .await
        .unwrap_or_else(|error| panic!("{connector} second empty file failed: {error}"));

        assert_eq!(first.blob_id, second.blob_id, "{connector} canonical blob");
        let blob = file_repo::find_blob_by_id(state.writer_db(), first.blob_id)
            .await
            .expect("virtual-empty blob should exist");
        assert!(blob.is_virtual_empty(), "{connector} backing");
        assert_eq!(blob.size, 0, "{connector} size");
        assert_eq!(blob.storage_path, None, "{connector} storage path");
        assert_eq!(blob.ref_count, 2, "{connector} shared ref count");
        assert!(driver.put_paths().is_empty(), "{connector} object writes");
        assert!(
            driver.delete_paths().is_empty(),
            "{connector} object deletes"
        );
        assert!(
            driver.object_paths().is_empty(),
            "{connector} object namespace"
        );
        driver.assert_no_object_api_calls();
    }
}

#[tokio::test]
async fn empty_file_idempotency_replays_same_result_and_rejects_request_drift() {
    let (state, _temp_root, policy, user) = build_test_state().await;
    let policy = replace_test_policy(&state, &policy, policy.clone()).await;
    let driver = Arc::new(RecordingEmptyDriver::default());
    state
        .driver_registry
        .insert_for_test(policy.id, driver.clone());
    let scope = WorkspaceStorageScope::Personal { user_id: user.id };

    let first = create_empty_with_idempotency(
        &state,
        scope,
        None,
        "idempotent.txt",
        EmptyFileNameMode::Exact,
        Some(("hashed-key", "request-fingerprint")),
    )
    .await
    .expect("first idempotent create should succeed");
    let replay = create_empty_with_idempotency(
        &state,
        scope,
        None,
        "idempotent.txt",
        EmptyFileNameMode::Exact,
        Some(("hashed-key", "request-fingerprint")),
    )
    .await
    .expect("idempotent replay should succeed");

    assert!(!first.replayed);
    assert!(replay.replayed);
    assert_eq!(first.file.id, replay.file.id);
    assert_eq!(first.file.blob_id, replay.file.blob_id);
    assert!(driver.put_paths().is_empty());

    let error = create_empty_with_idempotency(
        &state,
        scope,
        None,
        "different.txt",
        EmptyFileNameMode::Exact,
        Some(("hashed-key", "different-fingerprint")),
    )
    .await
    .expect_err("request drift must conflict");
    assert_eq!(error.api_error_code(), ApiErrorCode::Conflict);
}

#[tokio::test]
async fn empty_file_idempotency_replays_before_revalidating_changed_folder_state() {
    let (state, _temp_root, policy, user) = build_test_state().await;
    replace_test_policy(&state, &policy, policy.clone()).await;
    let now = Utc::now();
    let folder = folder::ActiveModel {
        name: Set("idempotent-parent".to_string()),
        owner_user_id: Set(Some(user.id)),
        created_by_user_id: Set(Some(user.id)),
        created_by_username: Set(user.username.clone()),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .expect("idempotent parent should insert");
    let scope = WorkspaceStorageScope::Personal { user_id: user.id };
    let mut storage_events = state.storage_change_bus.subscribe();

    let first = create_empty_with_idempotency(
        &state,
        scope,
        Some(folder.id),
        "folder-replay.txt",
        EmptyFileNameMode::Exact,
        Some(("folder-replay-key", "folder-replay-fingerprint")),
    )
    .await
    .expect("initial create should succeed");
    let first_event = tokio::time::timeout(Duration::from_secs(1), storage_events.recv())
        .await
        .expect("initial create should publish a storage event")
        .expect("storage change channel should stay open");
    assert_eq!(first_event.file_ids, vec![first.file.id]);

    folder_repo::soft_delete(state.writer_db(), folder.id)
        .await
        .expect("folder state should change after the committed create");
    let blob_before = file_repo::find_blob_by_id(state.writer_db(), first.file.blob_id)
        .await
        .expect("virtual-empty blob should exist");

    let replay = create_empty_with_idempotency(
        &state,
        scope,
        Some(folder.id),
        "folder-replay.txt",
        EmptyFileNameMode::Exact,
        Some(("folder-replay-key", "folder-replay-fingerprint")),
    )
    .await
    .expect("replay should return the committed result before folder revalidation");

    assert!(replay.replayed);
    assert_eq!(replay.file.id, first.file.id);
    assert_eq!(replay.file.blob_id, first.file.blob_id);
    let blob_after = file_repo::find_blob_by_id(state.writer_db(), first.file.blob_id)
        .await
        .expect("replayed virtual-empty blob should still exist");
    assert_eq!(blob_after.ref_count, blob_before.ref_count);
    assert!(
        tokio::time::timeout(Duration::from_millis(50), storage_events.recv())
            .await
            .is_err(),
        "idempotent replay should not publish a second storage event"
    );
}

#[tokio::test]
async fn empty_file_idempotency_serializes_concurrent_replays() {
    let (state, _temp_root, policy, user) = build_test_state().await;
    replace_test_policy(&state, &policy, policy.clone()).await;
    let scope = WorkspaceStorageScope::Personal { user_id: user.id };

    let results = futures::future::join_all((0..8).map(|_| {
        create_empty_with_idempotency(
            &state,
            scope,
            None,
            "concurrent-empty.txt",
            EmptyFileNameMode::Exact,
            Some(("concurrent-key", "concurrent-fingerprint")),
        )
    }))
    .await
    .into_iter()
    .collect::<Result<Vec<_>, _>>()
    .expect("concurrent replays should all succeed");

    assert_eq!(results.iter().filter(|result| !result.replayed).count(), 1);
    assert!(
        results
            .iter()
            .all(|result| result.file.id == results[0].file.id)
    );
    assert!(
        results
            .iter()
            .all(|result| result.file.blob_id == results[0].file.blob_id)
    );
}

#[tokio::test]
async fn empty_file_idempotency_conflicts_after_result_is_purged() {
    let (state, _temp_root, policy, user) = build_test_state().await;
    replace_test_policy(&state, &policy, policy.clone()).await;
    let scope = WorkspaceStorageScope::Personal { user_id: user.id };
    let created = create_empty_with_idempotency(
        &state,
        scope,
        None,
        "purged-empty.txt",
        EmptyFileNameMode::Exact,
        Some(("purged-key", "purged-fingerprint")),
    )
    .await
    .expect("initial create should succeed");
    crate::services::files::file::batch_purge_in_scope(&state, scope, vec![created.file.clone()])
        .await
        .expect("result file should be purged through the revision-aware lifecycle");

    let error = create_empty_with_idempotency(
        &state,
        scope,
        None,
        "purged-empty.txt",
        EmptyFileNameMode::Exact,
        Some(("purged-key", "purged-fingerprint")),
    )
    .await
    .expect_err("replay after result purge must conflict");
    assert_eq!(error.api_error_code(), ApiErrorCode::Conflict);
}

#[tokio::test]
async fn expired_empty_file_idempotency_key_can_be_reused() {
    let (state, _temp_root, policy, user) = build_test_state().await;
    replace_test_policy(&state, &policy, policy.clone()).await;
    let scope = WorkspaceStorageScope::Personal { user_id: user.id };
    let first = create_empty_with_idempotency(
        &state,
        scope,
        None,
        "first-expiring.txt",
        EmptyFileNameMode::Exact,
        Some(("expiring-key", "first-fingerprint")),
    )
    .await
    .expect("initial create should succeed");
    let idempotency_scope = file_create_idempotency_repo::FileCreateIdempotencyScope {
        actor_user_id: user.id,
        workspace_kind: "personal",
        workspace_id: user.id,
    };
    let record =
        file_create_idempotency_repo::find(state.writer_db(), idempotency_scope, "expiring-key")
            .await
            .expect("idempotency lookup should succeed")
            .expect("idempotency record should exist");
    let mut active = record.into_active_model();
    active.expires_at = Set(Utc::now() - chrono::Duration::seconds(1));
    active
        .update(state.writer_db())
        .await
        .expect("idempotency record should expire");

    let second = create_empty_with_idempotency(
        &state,
        scope,
        None,
        "second-expiring.txt",
        EmptyFileNameMode::Exact,
        Some(("expiring-key", "second-fingerprint")),
    )
    .await
    .expect("expired key should be reusable");
    assert_ne!(first.file.id, second.file.id);
    assert!(!second.replayed);
}

#[tokio::test]
async fn relative_empty_create_rolls_back_parent_blob_and_claim_on_file_insert_failure() {
    let (state, _temp_root, policy, user) = build_test_state().await;
    replace_test_policy(&state, &policy, policy.clone()).await;
    state
        .writer_db()
        .execute_unprepared(
            "CREATE TRIGGER fail_relative_empty_insert BEFORE INSERT ON files \
             WHEN NEW.name = 'rollback-empty.txt' BEGIN SELECT RAISE(ABORT, 'forced insert failure'); END",
        )
        .await
        .expect("failure trigger should install");
    let scope = WorkspaceStorageScope::Personal { user_id: user.id };
    let parsed = parse_relative_upload_path(&state, scope, None, "new-parent/rollback-empty.txt")
        .await
        .expect("relative path should parse");

    create_empty_from_relative_path_with_idempotency(
        &state,
        scope,
        parsed,
        Some(user.username.clone()),
        Some(("rollback-key", "rollback-fingerprint")),
    )
    .await
    .expect_err("forced file insert failure should roll back the transaction");

    assert_eq!(
        folder_repo::find_by_name_in_parent(state.writer_db(), user.id, None, "new-parent")
            .await
            .expect("folder lookup should succeed"),
        None
    );
    assert_eq!(
        file_repo::find_virtual_empty_blob_by_policy(state.writer_db(), policy.id)
            .await
            .expect("blob lookup should succeed"),
        None
    );
    let idempotency_scope = file_create_idempotency_repo::FileCreateIdempotencyScope {
        actor_user_id: user.id,
        workspace_kind: "personal",
        workspace_id: user.id,
    };
    assert_eq!(
        file_create_idempotency_repo::find(state.writer_db(), idempotency_scope, "rollback-key",)
            .await
            .expect("idempotency lookup should succeed"),
        None
    );
}

#[tokio::test]
async fn relative_empty_create_uses_the_final_parent_policy_override() {
    let (state, temp_root, default_policy, user) = build_test_state().await;
    crate::services::storage_policy::policy::ensure_policy_groups_seeded(state.writer_db())
        .await
        .expect("policy groups should seed");
    let mut override_policy = crate::storage::connectors::test_support::local_policy(
        temp_root.join("override-policy").to_string_lossy(),
    );
    override_policy.name = "Relative path override policy".to_string();
    override_policy.is_default = false;
    let override_policy =
        crate::storage::connectors::test_support::insertable_policy(override_policy)
            .insert(state.writer_db())
            .await
            .expect("override policy should insert");
    state
        .driver_registry
        .reload_policy_snapshot(&state.policy_snapshot, state.writer_db())
        .await
        .expect("policy snapshot should reload");

    let now = Utc::now();
    folder::ActiveModel {
        name: Set("bound-parent".to_string()),
        owner_user_id: Set(Some(user.id)),
        created_by_user_id: Set(Some(user.id)),
        created_by_username: Set(user.username.clone()),
        policy_id: Set(Some(override_policy.id)),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .expect("bound parent should insert");

    let scope = WorkspaceStorageScope::Personal { user_id: user.id };
    let parsed = parse_relative_upload_path(
        &state,
        scope,
        None,
        "bound-parent/generated-child/empty.txt",
    )
    .await
    .expect("relative path should parse");
    let created = create_empty_from_relative_path_with_idempotency(
        &state,
        scope,
        parsed,
        Some(user.username.clone()),
        Some(("override-key", "override-fingerprint")),
    )
    .await
    .expect("relative empty file should be created");
    let blob = file_repo::find_blob_by_id(state.writer_db(), created.file.blob_id)
        .await
        .expect("virtual empty blob should exist");

    assert_eq!(blob.policy_id, override_policy.id);
    assert_ne!(blob.policy_id, default_policy.id);
    assert!(blob.is_virtual_empty());
}

#[tokio::test]
async fn direct_empty_create_resolves_folder_policy_inside_the_writer_transaction() {
    let (state, temp_root, default_policy, user) = build_test_state().await;
    crate::services::storage_policy::policy::ensure_policy_groups_seeded(state.writer_db())
        .await
        .expect("policy groups should seed");
    let mut override_policy = crate::storage::connectors::test_support::local_policy(
        temp_root.join("direct-override-policy").to_string_lossy(),
    );
    override_policy.name = "Direct folder override policy".to_string();
    override_policy.is_default = false;
    let override_policy =
        crate::storage::connectors::test_support::insertable_policy(override_policy)
            .insert(state.writer_db())
            .await
            .expect("override policy should insert");
    state
        .driver_registry
        .reload_policy_snapshot(&state.policy_snapshot, state.writer_db())
        .await
        .expect("policy snapshot should reload");

    let now = Utc::now();
    let folder = folder::ActiveModel {
        name: Set("direct-bound-parent".to_string()),
        owner_user_id: Set(Some(user.id)),
        created_by_user_id: Set(Some(user.id)),
        created_by_username: Set(user.username.clone()),
        policy_id: Set(Some(override_policy.id)),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .expect("bound folder should insert");

    let created = create_empty_with_idempotency(
        &state,
        WorkspaceStorageScope::Personal { user_id: user.id },
        Some(folder.id),
        "direct-empty.txt",
        EmptyFileNameMode::Exact,
        Some(("direct-override-key", "direct-override-fingerprint")),
    )
    .await
    .expect("direct empty file should be created");
    let blob = file_repo::find_blob_by_id(state.writer_db(), created.file.blob_id)
        .await
        .expect("virtual empty blob should exist");

    assert_eq!(blob.policy_id, override_policy.id);
    assert_ne!(blob.policy_id, default_policy.id);
    assert!(blob.is_virtual_empty());
}

#[tokio::test]
async fn exact_name_conflict_never_creates_or_cleans_connector_objects() {
    for connector in ["s3", "onedrive"] {
        let (state, temp_root, current_policy, user) = build_test_state().await;
        let (_, replacement) = empty_file_connector_policy_cases(&temp_root)
            .into_iter()
            .find(|(name, _)| *name == connector)
            .expect("connector policy fixture should exist");
        let policy = replace_test_policy(&state, &current_policy, replacement).await;
        let driver = Arc::new(RecordingEmptyDriver::default());
        state
            .driver_registry
            .insert_for_test(policy.id, driver.clone());
        let scope = WorkspaceStorageScope::Personal { user_id: user.id };

        create_empty(
            &state,
            scope,
            None,
            "conflict.txt",
            EmptyFileNameMode::Exact,
        )
        .await
        .unwrap_or_else(|error| panic!("{connector} first empty file failed: {error}"));
        create_empty(
            &state,
            scope,
            None,
            "conflict.txt",
            EmptyFileNameMode::Exact,
        )
        .await
        .expect_err("exact-name conflict should fail without connector mutation");

        assert!(driver.put_paths().is_empty(), "{connector} object writes");
        assert!(driver.delete_paths().is_empty(), "{connector} cleanup");
        assert!(
            driver.object_paths().is_empty(),
            "{connector} object namespace"
        );
        driver.assert_no_object_api_calls();
    }
}

#[tokio::test]
async fn exact_name_conflict_keeps_virtual_empty_metadata_only() {
    let (state, _temp_root, current_policy, user) = build_test_state().await;
    let dedup_policy = enable_content_dedup(&current_policy);
    let policy = replace_test_policy(&state, &current_policy, dedup_policy).await;
    let driver = Arc::new(RecordingEmptyDriver::default());
    state
        .driver_registry
        .insert_for_test(policy.id, driver.clone());
    let scope = WorkspaceStorageScope::Personal { user_id: user.id };

    create_empty(
        &state,
        scope,
        None,
        "shared-conflict.txt",
        EmptyFileNameMode::Exact,
    )
    .await
    .unwrap();
    create_empty(
        &state,
        scope,
        None,
        "shared-conflict.txt",
        EmptyFileNameMode::Exact,
    )
    .await
    .expect_err("exact-name conflict should fail without owning the shared object");

    assert!(driver.put_paths().is_empty());
    assert!(driver.delete_paths().is_empty());
    assert!(driver.object_paths().is_empty());
    driver.assert_no_object_api_calls();
}

#[tokio::test]
async fn build_test_state_cleans_temp_root_on_drop() {
    let (state, temp_root, _, _) = build_test_state().await;
    std::fs::write(temp_root.join("cleanup-marker"), b"marker")
        .expect("cleanup marker should be written");

    drop(state);

    assert!(!temp_root.exists());
}

#[tokio::test]
async fn build_test_state_cleans_temp_root_during_panic_unwind() {
    let (state, temp_root, _, _) = build_test_state().await;
    std::fs::write(temp_root.join("panic-cleanup-marker"), b"marker")
        .expect("panic cleanup marker should be written");

    let panic_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        let _state = state;
        panic!("exercise storage test fixture cleanup during unwind");
    }));

    assert!(panic_result.is_err());
    assert!(!temp_root.exists());
}

#[tokio::test]
async fn persist_preuploaded_blob_keeps_prepared_named_storage_path() {
    let (state, _temp_root, policy, _) = build_test_state().await;
    let prepared = crate::services::workspace::storage::PreparedNonDedupBlobUpload::Opaque {
        upload_id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
        hash_prefix: "onedrive",
        storage_path: "files/550e8400-e29b-41d4-a716-446655440000/report.txt".to_string(),
        size: 7,
        policy_id: policy.id,
    };

    let blob = persist_preuploaded_blob(state.writer_db(), &prepared)
        .await
        .expect("prepared blob should persist");

    assert_eq!(blob.hash, "onedrive-550e8400-e29b-41d4-a716-446655440000");
    assert_eq!(
        blob.storage_path.as_deref(),
        Some("files/550e8400-e29b-41d4-a716-446655440000/report.txt")
    );
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
}

#[tokio::test]
async fn temp_store_silent_exact_name_updates_storage_without_storage_event() {
    let (state, temp_root, policy, user) = build_test_state().await;
    let scope = WorkspaceStorageScope::Personal { user_id: user.id };
    let mut storage_events = state.storage_change_bus.subscribe();

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

    let prepared = prepare_non_dedup_blob_upload(
        state.driver_registry.connectors(),
        &policy,
        payload.len() as i64,
        None,
    )
    .unwrap();
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
            lock_credentials: crate::services::files::lock::LockMutationCredentials::None,
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
        crate::services::task::list_tasks_paginated_in_scope(&*state, scope, 20, 0),
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
}

#[tokio::test]
async fn conditional_overwrite_rejects_file_changed_while_body_is_staged() {
    let (state, temp_root, policy, user) = build_test_state().await;
    let scope = WorkspaceStorageScope::Personal { user_id: user.id };

    let initial_temp = temp_root.join("conditional-existing-initial.bin");
    let initial_payload = b"initial payload";
    tokio::fs::write(&initial_temp, initial_payload)
        .await
        .unwrap();
    let initial = store_from_temp_exact_name_with_hints(
        &state,
        StoreFromTempParams::new(
            scope,
            None,
            "conditional-existing.txt",
            &initial_temp.to_string_lossy(),
            initial_payload.len() as i64,
        ),
        StoreFromTempHints {
            resolved_policy: Some(policy.clone()),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let (blocking_driver, entered_rx, release_put_file) = BlockingPutFileDriver::new(&policy);
    state
        .driver_registry
        .insert_for_test(policy.id, Arc::new(blocking_driver));
    let replacement_temp = temp_root.join("conditional-existing-replacement.bin");
    let replacement_payload = b"replacement payload";
    tokio::fs::write(&replacement_temp, replacement_payload)
        .await
        .unwrap();

    let state_for_store = state.clone();
    let policy_for_store = policy.clone();
    let replacement_path = replacement_temp.to_string_lossy().into_owned();
    let expected = initial.clone();
    let store_task = tokio::spawn(async move {
        let mut params = StoreFromTempParams::new(
            scope,
            None,
            "conditional-existing.txt",
            &replacement_path,
            replacement_payload.len() as i64,
        )
        .overwrite(expected.id);
        params.file_precondition = Some(FileWritePrecondition::existing(&expected));
        store_from_temp_exact_name_with_hints(
            &state_for_store,
            params,
            StoreFromTempHints {
                resolved_policy: Some(policy_for_store),
                ..Default::default()
            },
        )
        .await
    });

    tokio::time::timeout(Duration::from_secs(1), entered_rx)
        .await
        .expect("conditional overwrite should stage its blob")
        .expect("staging entry signal should be sent");

    let concurrent_updated_at = initial.updated_at + chrono::Duration::seconds(1);
    let mut concurrent_update: file::ActiveModel = initial.clone().into();
    concurrent_update.updated_at = Set(concurrent_updated_at);
    concurrent_update.update(state.writer_db()).await.unwrap();
    release_put_file.notify_one();

    let error = tokio::time::timeout(Duration::from_secs(1), store_task)
        .await
        .expect("conditional overwrite should finish")
        .expect("conditional overwrite task should join")
        .expect_err("a resource changed after preflight must reject the final write");
    assert_eq!(
        error.api_error_code(),
        ApiErrorCode::FileModifiedDuringWrite
    );

    let persisted = file::Entity::find_by_id(initial.id)
        .one(state.writer_db())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(persisted.blob_id, initial.blob_id);
    assert_eq!(persisted.size, initial.size);
    assert_eq!(persisted.updated_at, concurrent_updated_at);
}

#[tokio::test]
async fn conditional_create_rejects_file_appearing_while_body_is_staged() {
    let (state, temp_root, policy, user) = build_test_state().await;
    let scope = WorkspaceStorageScope::Personal { user_id: user.id };
    let (blocking_driver, entered_rx, release_put_file) = BlockingPutFileDriver::new(&policy);
    state
        .driver_registry
        .insert_for_test(policy.id, Arc::new(blocking_driver));

    let conditional_temp = temp_root.join("conditional-missing.bin");
    let conditional_payload = b"conditional payload";
    tokio::fs::write(&conditional_temp, conditional_payload)
        .await
        .unwrap();
    let state_for_store = state.clone();
    let policy_for_store = policy.clone();
    let conditional_path = conditional_temp.to_string_lossy().into_owned();
    let store_task = tokio::spawn(async move {
        let mut params = StoreFromTempParams::new(
            scope,
            None,
            "conditional-missing.txt",
            &conditional_path,
            conditional_payload.len() as i64,
        );
        params.file_precondition = Some(FileWritePrecondition::Missing);
        store_from_temp_exact_name_with_hints(
            &state_for_store,
            params,
            StoreFromTempHints {
                resolved_policy: Some(policy_for_store),
                ..Default::default()
            },
        )
        .await
    });

    tokio::time::timeout(Duration::from_secs(1), entered_rx)
        .await
        .expect("conditional create should stage its blob")
        .expect("staging entry signal should be sent");

    let now = Utc::now();
    let concurrent_blob = file_blob::ActiveModel {
        hash: Set(format!("conditional-race-{}", uuid::Uuid::new_v4())),
        size: Set(1),
        policy_id: Set(policy.id),
        storage_path: Set(Some(format!("files/{}", uuid::Uuid::new_v4()))),
        thumbnail_path: Set(None),
        thumbnail_processor: Set(None),
        thumbnail_version: Set(None),
        ref_count: Set(1),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .unwrap();
    let concurrent_file = file::ActiveModel {
        name: Set("conditional-missing.txt".to_string()),
        folder_id: Set(None),
        team_id: Set(None),
        blob_id: Set(concurrent_blob.id),
        size: Set(1),
        owner_user_id: Set(Some(user.id)),
        created_by_user_id: Set(Some(user.id)),
        created_by_username: Set(user.username.clone()),
        mime_type: Set("text/plain".to_string()),
        created_at: Set(now),
        updated_at: Set(now),
        deleted_at: Set(None),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .unwrap();
    release_put_file.notify_one();

    let error = tokio::time::timeout(Duration::from_secs(1), store_task)
        .await
        .expect("conditional create should finish")
        .expect("conditional create task should join")
        .expect_err("a resource appearing after preflight must reject the final write");
    assert_eq!(
        error.api_error_code(),
        ApiErrorCode::FileModifiedDuringWrite
    );

    let persisted = file::Entity::find_by_id(concurrent_file.id)
        .one(state.writer_db())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(persisted.blob_id, concurrent_blob.id);
    assert_eq!(persisted.size, 1);
    assert_eq!(
        file::Entity::find()
            .filter(file::Column::Name.eq("conditional-missing.txt"))
            .count(state.writer_db())
            .await
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn conditional_team_create_rejects_file_appearing_while_body_is_staged() {
    let (state, temp_root, policy, user) = build_test_state().await;
    let now = Utc::now();
    let team = team::ActiveModel {
        name: Set("Conditional Storage Team".to_string()),
        description: Set("Team conditional write test".to_string()),
        created_by: Set(user.id),
        storage_used: Set(0),
        storage_quota: Set(0),
        policy_group_id: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
        archived_at: Set(None),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .unwrap();
    let scope = WorkspaceStorageScope::Team {
        team_id: team.id,
        actor_user_id: user.id,
    };
    let local = crate::storage::connectors::resolve_local_filesystem_projection(
        state.driver_registry.connectors(),
        &policy,
    )
    .unwrap()
    .expect("conditional team policy should use the local connector");
    let uploads_root = PathBuf::from(local.base_path);
    let before = snapshot_dir_tree(&uploads_root).unwrap();
    let (blocking_driver, entered_rx, release_put_file) = BlockingPutFileDriver::new(&policy);
    state
        .driver_registry
        .insert_for_test(policy.id, Arc::new(blocking_driver));

    let conditional_temp = temp_root.join("conditional-team-missing.bin");
    let conditional_payload = b"conditional team payload";
    tokio::fs::write(&conditional_temp, conditional_payload)
        .await
        .unwrap();
    let state_for_store = state.clone();
    let policy_for_store = policy.clone();
    let conditional_path = conditional_temp.to_string_lossy().into_owned();
    let store_task = tokio::spawn(async move {
        let mut params = StoreFromTempParams::new(
            scope,
            None,
            "conditional-team-missing.txt",
            &conditional_path,
            conditional_payload.len() as i64,
        );
        params.file_precondition = Some(FileWritePrecondition::Missing);
        store_from_temp_exact_name_with_hints(
            &state_for_store,
            params,
            StoreFromTempHints {
                resolved_policy: Some(policy_for_store),
                ..Default::default()
            },
        )
        .await
    });

    tokio::time::timeout(Duration::from_secs(1), entered_rx)
        .await
        .expect("conditional team create should stage its blob")
        .expect("staging entry signal should be sent");

    let concurrent_blob = file_blob::ActiveModel {
        hash: Set(format!("conditional-team-race-{}", uuid::Uuid::new_v4())),
        size: Set(1),
        policy_id: Set(policy.id),
        storage_path: Set(Some(format!("files/{}", uuid::Uuid::new_v4()))),
        thumbnail_path: Set(None),
        thumbnail_processor: Set(None),
        thumbnail_version: Set(None),
        ref_count: Set(1),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .unwrap();
    let concurrent_file = file::ActiveModel {
        name: Set("conditional-team-missing.txt".to_string()),
        folder_id: Set(None),
        team_id: Set(Some(team.id)),
        blob_id: Set(concurrent_blob.id),
        size: Set(1),
        owner_user_id: Set(None),
        created_by_user_id: Set(Some(user.id)),
        created_by_username: Set(user.username.clone()),
        mime_type: Set("text/plain".to_string()),
        created_at: Set(now),
        updated_at: Set(now),
        deleted_at: Set(None),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .unwrap();
    release_put_file.notify_one();

    let error = tokio::time::timeout(Duration::from_secs(1), store_task)
        .await
        .expect("conditional team create should finish")
        .expect("conditional team create task should join")
        .expect_err("a team resource appearing after preflight must reject the final write");
    assert_eq!(
        error.api_error_code(),
        ApiErrorCode::FileModifiedDuringWrite
    );

    let persisted = file::Entity::find_by_id(concurrent_file.id)
        .one(state.writer_db())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(persisted.blob_id, concurrent_blob.id);
    assert_eq!(persisted.size, 1);
    assert_eq!(
        file::Entity::find()
            .filter(file::Column::TeamId.eq(team.id))
            .filter(file::Column::Name.eq("conditional-team-missing.txt"))
            .count(state.writer_db())
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        file_blob::Entity::find()
            .count(state.writer_db())
            .await
            .unwrap(),
        1,
        "failed staged upload must not leave a blob row"
    );
    assert_eq!(
        snapshot_dir_tree(&uploads_root).unwrap(),
        before,
        "failed staged upload must clean its storage object"
    );
}

#[tokio::test]
async fn missing_precondition_rejects_overwrite_context() {
    let (state, temp_root, policy, user) = build_test_state().await;
    let scope = WorkspaceStorageScope::Personal { user_id: user.id };
    let initial_temp = temp_root.join("missing-overwrite-initial.bin");
    let payload = b"initial missing overwrite payload";
    tokio::fs::write(&initial_temp, payload).await.unwrap();
    let initial = store_from_temp_exact_name_with_hints(
        &state,
        StoreFromTempParams::new(
            scope,
            None,
            "missing-overwrite.txt",
            &initial_temp.to_string_lossy(),
            payload.len() as i64,
        ),
        StoreFromTempHints {
            resolved_policy: Some(policy.clone()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let local = crate::storage::connectors::resolve_local_filesystem_projection(
        state.driver_registry.connectors(),
        &policy,
    )
    .unwrap()
    .expect("missing-precondition policy should use the local connector");
    let uploads_root = PathBuf::from(local.base_path);
    let before = snapshot_dir_tree(&uploads_root).unwrap();
    let replacement_temp = temp_root.join("missing-overwrite-replacement.bin");
    let replacement_payload = b"replacement must not be persisted";
    tokio::fs::write(&replacement_temp, replacement_payload)
        .await
        .unwrap();
    let replacement_path = replacement_temp.to_string_lossy().into_owned();
    let mut params = StoreFromTempParams::new(
        scope,
        None,
        "missing-overwrite-new-name.txt",
        &replacement_path,
        replacement_payload.len() as i64,
    )
    .overwrite(initial.id);
    params.file_precondition = Some(FileWritePrecondition::Missing);
    let error = store_from_temp_exact_name_with_hints(
        &state,
        params,
        StoreFromTempHints {
            resolved_policy: Some(policy),
            ..Default::default()
        },
    )
    .await
    .expect_err("Missing precondition must reject an overwrite target");
    assert_eq!(
        error.api_error_code(),
        ApiErrorCode::FileModifiedDuringWrite
    );

    let persisted = file::Entity::find_by_id(initial.id)
        .one(state.writer_db())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(persisted.blob_id, initial.blob_id);
    assert_eq!(persisted.size, initial.size);
    assert_eq!(persisted.updated_at, initial.updated_at);
    assert!(
        file::Entity::find()
            .filter(file::Column::Name.eq("missing-overwrite-new-name.txt"))
            .one(state.writer_db())
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        file_blob::Entity::find()
            .count(state.writer_db())
            .await
            .unwrap(),
        1,
        "failed contradictory write must not leave a blob row"
    );
    assert_eq!(
        snapshot_dir_tree(&uploads_root).unwrap(),
        before,
        "failed contradictory write must clean its staged object"
    );
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
        crate::services::task::list_tasks_paginated_in_scope(&*state, scope, 20, 0),
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
}

#[tokio::test]
async fn temp_store_cancellation_during_stream_upload_cleans_preuploaded_blob() {
    let (state, temp_root, local_policy, user) = build_test_state().await;
    let mut policy = crate::storage::connectors::test_support::s3_policy(
        "https://s3.example.test",
        "test-bucket",
        "",
        ObjectStorageUploadStrategy::RelayStream,
        ObjectStorageDownloadStrategy::RelayStream,
    );
    policy.id = local_policy.id;
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
}

#[tokio::test]
async fn cancelled_during_stream_upload_can_resume_from_same_temp_file() {
    let (state, temp_root, local_policy, user) = build_test_state().await;
    let mut policy = crate::storage::connectors::test_support::s3_policy(
        "https://s3.example.test",
        "test-bucket",
        "",
        ObjectStorageUploadStrategy::RelayStream,
        ObjectStorageDownloadStrategy::RelayStream,
    );
    policy.id = local_policy.id;
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
    assert_eq!(
        driver
            .get(blob.storage_path.as_deref().expect("stored blob path"))
            .await
            .unwrap(),
        payload
    );
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
        driver
            .get(blob.storage_path.as_deref().expect("stored blob path"))
            .await
            .unwrap(),
        payload,
        "stored local blob should contain original payload"
    );
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
    assert_eq!(
        driver
            .get(blob.storage_path.as_deref().expect("stored blob path"))
            .await
            .unwrap(),
        payload
    );
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
    assert_eq!(
        driver
            .get(blob.storage_path.as_deref().expect("stored blob path"))
            .await
            .unwrap(),
        payload
    );
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
}
