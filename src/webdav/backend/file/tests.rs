//! Tests for WebDAV file write handling.

use super::{AsterDavWriteHandle, DavWriteOpenContext, streaming_direct_eligibility_error};
use crate::config::{Config, DatabaseConfig, RuntimeConfig};
use crate::db::repository::file_repo;
use crate::runtime::PrimaryAppState;
use crate::services::mail::sender;
use crate::storage::{DriverRegistry, PolicySnapshot};
use crate::test_support::snapshot_dir_tree;
use aster_drive_model::entities::{storage_policy, user};
use aster_drive_model::types::{
    ObjectStorageDownloadStrategy, ObjectStorageUploadStrategy, UserRole, UserStatus,
};
use aster_drive_storage::{BlobMetadata, StorageDriver, StreamUploadDriver};
use aster_forge_cache as cache;
use aster_forge_cache::CacheConfig;
use aster_forge_webdav::DavWriteHandle;
use async_trait::async_trait;
use bytes::Bytes;
use chrono::Utc;
use sea_orm::{ActiveModelTrait, Set};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};
use tokio::io::{AsyncRead, AsyncReadExt};

#[derive(Clone, Default)]
struct MockDirectS3Driver {
    objects: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    put_file_calls: Arc<AtomicUsize>,
    put_reader_calls: Arc<AtomicUsize>,
}

impl MockDirectS3Driver {
    fn put_file_calls(&self) -> usize {
        self.put_file_calls.load(Ordering::SeqCst)
    }

    fn put_reader_calls(&self) -> usize {
        self.put_reader_calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl StorageDriver for MockDirectS3Driver {
    async fn put(&self, path: &str, data: &[u8]) -> aster_drive_storage::Result<String> {
        self.objects
            .lock()
            .expect("mock direct S3 driver lock should succeed")
            .insert(path.to_string(), data.to_vec());
        Ok(path.to_string())
    }

    async fn get(&self, path: &str) -> aster_drive_storage::Result<Vec<u8>> {
        Ok(self
            .objects
            .lock()
            .expect("mock direct S3 driver lock should succeed")
            .get(path)
            .cloned()
            .unwrap_or_default())
    }

    async fn get_stream(
        &self,
        _path: &str,
    ) -> aster_drive_storage::Result<Box<dyn AsyncRead + Unpin + Send>> {
        Ok(Box::new(tokio::io::empty()))
    }

    async fn delete(&self, path: &str) -> aster_drive_storage::Result<()> {
        self.objects
            .lock()
            .expect("mock direct S3 driver lock should succeed")
            .remove(path);
        Ok(())
    }

    async fn exists(&self, path: &str) -> aster_drive_storage::Result<bool> {
        Ok(self
            .objects
            .lock()
            .expect("mock direct S3 driver lock should succeed")
            .contains_key(path))
    }

    async fn metadata(&self, path: &str) -> aster_drive_storage::Result<BlobMetadata> {
        let size = self
            .objects
            .lock()
            .expect("mock direct S3 driver lock should succeed")
            .get(path)
            .map(|bytes| u64::try_from(bytes.len()).expect("mock object size should fit u64"))
            .unwrap_or(0);
        Ok(BlobMetadata {
            size,
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
impl StreamUploadDriver for MockDirectS3Driver {
    async fn put_file(
        &self,
        storage_path: &str,
        local_path: &str,
    ) -> aster_drive_storage::Result<String> {
        self.put_file_calls.fetch_add(1, Ordering::SeqCst);
        let data = tokio::fs::read(local_path).await.map_err(|error| {
            crate::errors::AsterError::storage_driver_error(format!(
                "mock direct S3 put_file failed: {error}"
            ))
        })?;
        self.objects
            .lock()
            .expect("mock direct S3 driver lock should succeed")
            .insert(storage_path.to_string(), data);
        Ok(storage_path.to_string())
    }

    async fn put_reader(
        &self,
        storage_path: &str,
        mut reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
        _size: i64,
    ) -> aster_drive_storage::Result<String> {
        self.put_reader_calls.fetch_add(1, Ordering::SeqCst);
        let mut data = Vec::new();
        reader.read_to_end(&mut data).await.map_err(|error| {
            crate::errors::AsterError::storage_driver_error(format!(
                "mock direct S3 reader failed: {error}"
            ))
        })?;
        self.objects
            .lock()
            .expect("mock direct S3 driver lock should succeed")
            .insert(storage_path.to_string(), data);
        Ok(storage_path.to_string())
    }
}

async fn build_s3_direct_test_state() -> (
    PrimaryAppState,
    user::Model,
    storage_policy::Model,
    MockDirectS3Driver,
) {
    let temp_root = std::env::temp_dir().join(format!(
        "asterdrive-webdav-file-direct-s3-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&temp_root).expect("temp root should be created");

    let db = crate::db::connect_with_metrics(
        &DatabaseConfig {
            url: "sqlite::memory:".into(),
            pool_size: 1,
            retry_count: 0,
        },
        aster_drive_metrics::NoopMetrics::arc(),
    )
    .await
    .expect("test database connection should succeed");
    crate::storage::connectors::test_support::migrate_current_storage_test_schema(&db).await;

    let now = Utc::now();
    let mut policy = crate::storage::connectors::test_support::s3_policy(
        "https://mock-s3.example",
        "mock-bucket",
        "",
        ObjectStorageUploadStrategy::RelayStream,
        ObjectStorageDownloadStrategy::RelayStream,
    );
    policy.name = "Direct S3 Policy".to_string();
    policy.is_default = true;
    policy.chunk_size = 5_242_880;
    let policy = crate::storage::connectors::test_support::insertable_policy(policy)
        .insert(&db)
        .await
        .expect("test S3 policy should be inserted");

    let user = user::ActiveModel {
        username: Set("webdavs3writer".to_string()),
        email: Set("webdavs3writer@example.com".to_string()),
        password_hash: Set("unused".to_string()),
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
    .expect("test user should be inserted");

    crate::services::storage_policy::policy::ensure_policy_groups_seeded(&db)
        .await
        .expect("policy groups should be seeded for direct S3 test");

    let runtime_config = Arc::new(RuntimeConfig::new());
    let cache = cache::create_cache(&CacheConfig {
        ..Default::default()
    })
    .await;

    let mut config = Config::default();
    config.server.temp_dir = temp_root.join(".tmp").to_string_lossy().into_owned();
    config.server.upload_temp_dir = temp_root.join(".uploads").to_string_lossy().into_owned();

    let driver_registry =
        Arc::new(DriverRegistry::noop().expect("built-in storage connector registry"));
    let mock_driver = MockDirectS3Driver::default();
    driver_registry.insert_for_test(policy.id, Arc::new(mock_driver.clone()));
    let policy_snapshot = Arc::new(PolicySnapshot::new());
    driver_registry
        .reload_policy_snapshot(&policy_snapshot, &db)
        .await
        .expect("policy snapshot should reload");

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
        driver_registry,
        runtime_config: runtime_config.clone(),
        policy_snapshot,
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

    (state, user, policy, mock_driver)
}

#[tokio::test]
async fn known_size_s3_write_avoids_runtime_temp_files() {
    let (state, user, _, driver) = build_s3_direct_test_state().await;
    let runtime_temp_dir =
        aster_forge_utils::paths::runtime_temp_dir(&state.config.server.temp_dir);
    let before = snapshot_dir_tree(Path::new(&runtime_temp_dir)).unwrap();
    let payload = b"stream direct to s3";

    let mut write_handle = AsterDavWriteHandle::for_write(
        state.clone(),
        user.id,
        None,
        "direct-s3.txt".to_string(),
        None,
        Some(u64::try_from(payload.len()).expect("payload length should fit u64")),
    )
    .await
    .expect("S3 direct WebDAV file should initialize");
    write_handle
        .write_bytes(Bytes::copy_from_slice(payload))
        .await
        .expect("S3 direct WebDAV write should succeed");
    write_handle
        .finish()
        .await
        .expect("S3 direct WebDAV finish should succeed");

    let after = snapshot_dir_tree(Path::new(&runtime_temp_dir)).unwrap();
    assert_eq!(
        after, before,
        "known-size S3 WebDAV write should not create runtime temp files"
    );

    let stored =
        file_repo::find_by_name_in_folder(state.writer_db(), user.id, None, "direct-s3.txt")
            .await
            .expect("stored file lookup should succeed")
            .expect("S3 direct WebDAV finish should create a file");
    assert_eq!(
        stored.size,
        i64::try_from(payload.len()).expect("payload length should fit i64")
    );

    let objects = driver
        .objects
        .lock()
        .expect("mock direct S3 driver lock should succeed");
    assert_eq!(
        objects.len(),
        1,
        "direct S3 path should upload exactly one object"
    );
    assert!(
        objects.values().any(|bytes| bytes.as_slice() == payload),
        "uploaded object should match the WebDAV payload"
    );
    assert_eq!(driver.put_reader_calls(), 1);
    assert_eq!(driver.put_file_calls(), 0);
}

#[tokio::test]
async fn known_size_s3_write_with_precondition_uses_transactional_temp_upload() {
    let (state, user, _, driver) = build_s3_direct_test_state().await;
    let payload = b"conditional s3 upload";
    let mut write_handle = AsterDavWriteHandle::for_write_with_audit(
        state.clone(),
        DavWriteOpenContext {
            scope: crate::services::workspace::storage::WorkspaceStorageScope::Personal {
                user_id: user.id,
            },
            folder_id: None,
            filename: "conditional-s3.txt".to_string(),
            existing_file_id: None,
            declared_size: Some(
                u64::try_from(payload.len()).expect("payload length should fit u64"),
            ),
            submitted_lock_tokens: vec![],
            audit_ctx: crate::services::ops::audit::AuditContext {
                user_id: user.id,
                ip_address: None,
                user_agent: None,
            },
            file_precondition: Some(
                crate::services::workspace::storage::FileWritePrecondition::Missing,
            ),
        },
    )
    .await
    .expect("conditional S3 WebDAV file should initialize");

    write_handle
        .write_bytes(Bytes::copy_from_slice(payload))
        .await
        .expect("conditional S3 WebDAV write should succeed");
    write_handle
        .finish()
        .await
        .expect("conditional S3 WebDAV finish should succeed");

    let stored =
        file_repo::find_by_name_in_folder(state.writer_db(), user.id, None, "conditional-s3.txt")
            .await
            .expect("stored file lookup should succeed")
            .expect("conditional S3 WebDAV finish should create a file");
    assert_eq!(
        stored.size,
        i64::try_from(payload.len()).expect("payload length should fit i64")
    );
    assert_eq!(driver.put_reader_calls(), 0);
    assert_eq!(driver.put_file_calls(), 1);
    assert!(
        driver
            .objects
            .lock()
            .expect("mock direct S3 driver lock should succeed")
            .values()
            .any(|bytes| bytes.as_slice() == payload)
    );
}

#[test]
fn streaming_direct_eligibility_failure_maps_to_general_failure() {
    let policy = crate::storage::connectors::test_support::s3_policy(
        "https://mock-s3.example",
        "mock-bucket",
        "",
        ObjectStorageUploadStrategy::RelayStream,
        ObjectStorageDownloadStrategy::RelayStream,
    );
    let error = crate::errors::AsterError::internal_error("test connector registry failure");

    assert!(matches!(
        streaming_direct_eligibility_error(&policy, &error),
        aster_forge_webdav::FsError::GeneralFailure
    ));
}
