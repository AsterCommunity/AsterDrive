//! Tests for WebDAV file write handling.

use super::AsterDavFile;
use crate::config::{CacheConfig, Config, DatabaseConfig, RuntimeConfig};
use crate::db::repository::file_repo;
use crate::entities::{storage_policy, user};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::mail::sender;
use crate::storage::BlobMetadata;
use crate::storage::{DriverRegistry, PolicySnapshot, StorageDriver, StreamUploadDriver};
use crate::types::{DriverType, UserRole, UserStatus};
use crate::webdav::dav::DavFile;
use aster_forge_cache as cache;
use async_trait::async_trait;
use bytes::Bytes;
use chrono::Utc;
use migration::Migrator;
use sea_orm::{ActiveModelTrait, Set};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncRead, AsyncReadExt};

#[derive(Clone, Default)]
struct MockDirectS3Driver {
    objects: Arc<Mutex<HashMap<String, Vec<u8>>>>,
}

#[async_trait]
impl StorageDriver for MockDirectS3Driver {
    async fn put(&self, path: &str, data: &[u8]) -> crate::errors::Result<String> {
        self.objects
            .lock()
            .expect("mock direct S3 driver lock should succeed")
            .insert(path.to_string(), data.to_vec());
        Ok(path.to_string())
    }

    async fn get(&self, path: &str) -> crate::errors::Result<Vec<u8>> {
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
    ) -> crate::errors::Result<Box<dyn AsyncRead + Unpin + Send>> {
        Ok(Box::new(tokio::io::empty()))
    }

    async fn delete(&self, path: &str) -> crate::errors::Result<()> {
        self.objects
            .lock()
            .expect("mock direct S3 driver lock should succeed")
            .remove(path);
        Ok(())
    }

    async fn exists(&self, path: &str) -> crate::errors::Result<bool> {
        Ok(self
            .objects
            .lock()
            .expect("mock direct S3 driver lock should succeed")
            .contains_key(path))
    }

    async fn metadata(&self, path: &str) -> crate::errors::Result<BlobMetadata> {
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

    fn as_stream_upload(&self) -> Option<&dyn StreamUploadDriver> {
        Some(self)
    }
}

#[async_trait]
impl StreamUploadDriver for MockDirectS3Driver {
    async fn put_file(
        &self,
        storage_path: &str,
        local_path: &str,
    ) -> crate::errors::Result<String> {
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
    ) -> crate::errors::Result<String> {
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

fn snapshot_dir_tree(path: &Path) -> std::io::Result<BTreeMap<String, bool>> {
    fn walk(
        root: &Path,
        current: &Path,
        entries: &mut BTreeMap<String, bool>,
    ) -> std::io::Result<()> {
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
                entries.insert(format!("{relative}/"), true);
                walk(root, &path, entries)?;
            } else {
                entries.insert(relative, false);
            }
        }
        Ok(())
    }

    let mut entries = BTreeMap::new();
    if !path.exists() {
        return Ok(entries);
    }
    walk(path, path, &mut entries)?;
    Ok(entries)
}

async fn build_s3_direct_test_state() -> (PrimaryAppState, user::Model, MockDirectS3Driver) {
    let temp_root = std::env::temp_dir().join(format!(
        "asterdrive-webdav-file-direct-s3-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&temp_root).expect("temp root should be created");

    let db = crate::db::connect_with_metrics(
        &DatabaseConfig {
            url: "sqlite::memory:".to_string(),
            pool_size: 1,
            retry_count: 0,
        },
        crate::metrics::NoopMetrics::arc(),
    )
    .await
    .expect("test database connection should succeed");
    Migrator::up(&db, None)
        .await
        .expect("test migrations should succeed");

    let now = Utc::now();
    let policy = storage_policy::ActiveModel {
        name: Set("Direct S3 Policy".to_string()),
        driver_type: Set(DriverType::S3),
        endpoint: Set("https://mock-s3.example".to_string()),
        bucket: Set("mock-bucket".to_string()),
        access_key: Set("mock-access".to_string()),
        secret_key: Set("mock-secret".to_string()),
        base_path: Set(String::new()),
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

    let policy_snapshot = Arc::new(PolicySnapshot::new());
    policy_snapshot
        .reload(&db)
        .await
        .expect("policy snapshot should reload");

    let driver_registry = Arc::new(DriverRegistry::noop());
    let mock_driver = MockDirectS3Driver::default();
    driver_registry.insert_for_test(policy.id, Arc::new(mock_driver.clone()));

    let (storage_change_tx, _) = tokio::sync::broadcast::channel(
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
        metrics: crate::metrics::NoopMetrics::arc(),
        mail_sender: sender::runtime_sender(runtime_config),
        storage_change_tx,
        share_download_rollback,
        background_task_dispatch_wakeup:
            crate::runtime::PrimaryAppState::new_background_task_dispatch_wakeup(),
        remote_protocol: crate::runtime::PrimaryAppState::new_remote_protocol(),
    };

    (state, user, mock_driver)
}

#[tokio::test]
async fn known_size_s3_write_avoids_runtime_temp_files() {
    let (state, user, driver) = build_s3_direct_test_state().await;
    let runtime_temp_dir = crate::utils::paths::runtime_temp_dir(&state.config.server.temp_dir);
    let before = snapshot_dir_tree(Path::new(&runtime_temp_dir)).unwrap();
    let payload = b"stream direct to s3";

    let mut dav_file = AsterDavFile::for_write(
        state.clone(),
        user.id,
        None,
        "direct-s3.txt".to_string(),
        None,
        Some(u64::try_from(payload.len()).expect("payload length should fit u64")),
    )
    .await
    .expect("S3 direct WebDAV file should initialize");
    dav_file
        .write_bytes(Bytes::copy_from_slice(payload))
        .await
        .expect("S3 direct WebDAV write should succeed");
    dav_file
        .flush()
        .await
        .expect("S3 direct WebDAV flush should succeed");

    let after = snapshot_dir_tree(Path::new(&runtime_temp_dir)).unwrap();
    assert_eq!(
        after, before,
        "known-size S3 WebDAV write should not create runtime temp files"
    );

    let stored =
        file_repo::find_by_name_in_folder(state.writer_db(), user.id, None, "direct-s3.txt")
            .await
            .expect("stored file lookup should succeed")
            .expect("S3 direct WebDAV flush should create a file");
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
}
