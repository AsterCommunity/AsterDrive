use std::collections::HashSet;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

use async_trait::async_trait;
use chrono::Utc;
use migration::Migrator;
use sea_orm::{ActiveModelTrait, EntityTrait, Set};
use tokio::io::{AsyncRead, empty};

use super::*;
use crate::config::{Config, DatabaseConfig, RuntimeConfig};
use crate::db::repository::file_repo;
use crate::entities::{file, file_blob, storage_policy, user};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::mail::sender;
use crate::services::workspace::storage::WorkspaceStorageScope;
use crate::storage::BlobMetadata;
use crate::storage::{DriverRegistry, PolicySnapshot, StorageDriver};
use crate::types::{
    DriverType, StoredStoragePolicyAllowedTypes, StoredStoragePolicyOptions, UserRole, UserStatus,
};
use aster_forge_cache as cache;
use aster_forge_cache::CacheConfig;

#[derive(Clone, Default)]
struct TrackingDeleteDriver {
    objects: Arc<Mutex<HashSet<String>>>,
    fail_delete_paths: Arc<Mutex<HashSet<String>>>,
    delete_attempts: Arc<Mutex<Vec<String>>>,
    delete_calls: Arc<AtomicUsize>,
}

impl TrackingDeleteDriver {
    fn insert_object(&self, path: &str) {
        self.objects
            .lock()
            .expect("tracking delete driver lock should succeed")
            .insert(path.to_string());
    }

    fn fail_delete_for(&self, path: &str) {
        self.fail_delete_paths
            .lock()
            .expect("tracking delete driver lock should succeed")
            .insert(path.to_string());
    }

    fn delete_calls(&self) -> usize {
        self.delete_calls.load(Ordering::SeqCst)
    }

    fn delete_attempts(&self) -> Vec<String> {
        self.delete_attempts
            .lock()
            .expect("tracking delete driver lock should succeed")
            .clone()
    }

    fn contains(&self, path: &str) -> bool {
        self.objects
            .lock()
            .expect("tracking delete driver lock should succeed")
            .contains(path)
    }
}

#[async_trait]
impl StorageDriver for TrackingDeleteDriver {
    async fn put(&self, path: &str, _data: &[u8]) -> crate::errors::Result<String> {
        self.insert_object(path);
        Ok(path.to_string())
    }

    async fn get(&self, _path: &str) -> crate::errors::Result<Vec<u8>> {
        Ok(Vec::new())
    }

    async fn get_stream(
        &self,
        _path: &str,
    ) -> crate::errors::Result<Box<dyn AsyncRead + Unpin + Send>> {
        Ok(Box::new(empty()))
    }

    async fn delete(&self, path: &str) -> crate::errors::Result<()> {
        self.delete_calls.fetch_add(1, Ordering::SeqCst);
        self.delete_attempts
            .lock()
            .expect("tracking delete driver lock should succeed")
            .push(path.to_string());
        if self
            .fail_delete_paths
            .lock()
            .expect("tracking delete driver lock should succeed")
            .contains(path)
        {
            return Err(crate::errors::AsterError::storage_driver_error(format!(
                "forced delete failure for {path}"
            )));
        }
        self.objects
            .lock()
            .expect("tracking delete driver lock should succeed")
            .remove(path);
        Ok(())
    }

    async fn exists(&self, path: &str) -> crate::errors::Result<bool> {
        Ok(self.contains(path))
    }

    async fn metadata(&self, path: &str) -> crate::errors::Result<BlobMetadata> {
        Ok(BlobMetadata {
            size: if self.contains(path) { 1 } else { 0 },
            content_type: Some("application/octet-stream".to_string()),
        })
    }
}

async fn build_deletion_test_state() -> (
    PrimaryAppState,
    user::Model,
    storage_policy::Model,
    TrackingDeleteDriver,
) {
    let temp_root = std::env::temp_dir().join(format!(
        "asterdrive-deletion-service-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&temp_root).expect("deletion test temp root should exist");

    let db = crate::db::connect_with_metrics(
        &DatabaseConfig {
            url: "sqlite::memory:".to_string(),
            pool_size: 1,
            retry_count: 0,
        },
        crate::metrics::NoopMetrics::arc(),
    )
    .await
    .expect("deletion test DB should connect");
    Migrator::up(&db, None)
        .await
        .expect("deletion test migrations should succeed");

    let now = Utc::now();
    let policy = storage_policy::ActiveModel {
        name: Set("Deletion Test Policy".to_string()),
        driver_type: Set(DriverType::Local),
        endpoint: Set(String::new()),
        bucket: Set(String::new()),
        access_key: Set(String::new()),
        secret_key: Set(String::new()),
        base_path: Set(temp_root.join("uploads").to_string_lossy().into_owned()),
        max_file_size: Set(0),
        allowed_types: Set(StoredStoragePolicyAllowedTypes::empty()),
        options: Set(StoredStoragePolicyOptions::empty()),
        is_default: Set(true),
        chunk_size: Set(0),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("deletion test policy should insert");

    let user = user::ActiveModel {
        username: Set(format!("deletion-user-{}", uuid::Uuid::new_v4())),
        email: Set(format!("deletion-{}@example.com", uuid::Uuid::new_v4())),
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
    .expect("deletion test user should insert");

    let runtime_config = Arc::new(RuntimeConfig::new());
    let cache = cache::create_cache(&CacheConfig {
        ..Default::default()
    })
    .await;
    let mut config = Config::default();
    config.server.temp_dir = temp_root.join(".tmp").to_string_lossy().into_owned();
    config.server.upload_temp_dir = temp_root.join(".uploads").to_string_lossy().into_owned();

    let driver = TrackingDeleteDriver::default();
    let driver_registry = Arc::new(DriverRegistry::noop());
    driver_registry.insert_for_test(policy.id, Arc::new(driver.clone()));
    let policy_snapshot = Arc::new(PolicySnapshot::new());
    policy_snapshot
        .reload(&db)
        .await
        .expect("policy snapshot should reload");

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
        upload_runtime: crate::runtime::PrimaryAppState::new_upload_runtime(),
    };

    (state, user, policy, driver)
}

async fn create_blob(
    db: &sea_orm::DatabaseConnection,
    policy_id: i64,
    storage_path: &str,
    size: i64,
    ref_count: i32,
) -> file_blob::Model {
    let now = Utc::now();
    file_blob::ActiveModel {
        hash: Set(format!("blob-{}", uuid::Uuid::new_v4())),
        size: Set(size),
        policy_id: Set(policy_id),
        storage_path: Set(storage_path.to_string()),
        thumbnail_path: Set(None),
        thumbnail_processor: Set(None),
        thumbnail_version: Set(None),
        ref_count: Set(ref_count),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("test blob should insert")
}

async fn create_file(
    db: &sea_orm::DatabaseConnection,
    user_id: i64,
    blob_id: i64,
    size: i64,
    name: &str,
) -> file::Model {
    let now = Utc::now();
    file::ActiveModel {
        name: Set(name.to_string()),
        folder_id: Set(None),
        team_id: Set(None),
        blob_id: Set(blob_id),
        size: Set(size),
        owner_user_id: Set(Some(user_id)),
        created_by_user_id: Set(Some(user_id)),
        created_by_username: Set("tester".to_string()),
        mime_type: Set("application/octet-stream".to_string()),
        created_at: Set(now),
        updated_at: Set(now),
        deleted_at: Set(None),
        is_locked: Set(false),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("test file should insert")
}

async fn set_user_storage_used(
    db: &sea_orm::DatabaseConnection,
    user_model: &user::Model,
    storage_used: i64,
) {
    let mut active: user::ActiveModel = user_model.clone().into();
    active.storage_used = Set(storage_used);
    active.updated_at = Set(Utc::now());
    active
        .update(db)
        .await
        .expect("test user storage should update");
}

#[tokio::test]
async fn ensure_blob_cleanup_if_unreferenced_deletes_zero_ref_blob() {
    let (state, _user, policy, driver) = build_deletion_test_state().await;
    let blob = create_blob(state.writer_db(), policy.id, "files/orphan.bin", 7, 0).await;
    driver.insert_object(&blob.storage_path);

    let cleaned = ensure_blob_cleanup_if_unreferenced(&state, blob.id).await;

    assert!(cleaned, "zero-ref blob should be cleaned");
    assert_eq!(driver.delete_calls(), 1, "object delete should run once");
    assert!(
        !driver.contains(&blob.storage_path),
        "blob object should be removed from the mock driver"
    );
    assert!(
        file_blob::Entity::find_by_id(blob.id)
            .one(state.writer_db())
            .await
            .expect("blob lookup should succeed")
            .is_none(),
        "blob row should be deleted after cleanup"
    );
}

#[tokio::test]
async fn ensure_blob_cleanup_if_unreferenced_skips_referenced_blob() {
    let (state, _user, policy, driver) = build_deletion_test_state().await;
    let blob = create_blob(state.writer_db(), policy.id, "files/in-use.bin", 9, 2).await;
    driver.insert_object(&blob.storage_path);

    let cleaned = ensure_blob_cleanup_if_unreferenced(&state, blob.id).await;

    assert!(
        cleaned,
        "positive ref_count should be treated as no cleanup needed"
    );
    assert_eq!(
        driver.delete_calls(),
        0,
        "referenced blob must not be deleted"
    );
    assert!(
        file_blob::Entity::find_by_id(blob.id)
            .one(state.writer_db())
            .await
            .expect("blob lookup should succeed")
            .is_some(),
        "referenced blob row must remain"
    );
}

#[tokio::test]
async fn ensure_blob_cleanup_if_unreferenced_skips_cleanup_claimed_blob() {
    let (state, _user, policy, driver) = build_deletion_test_state().await;
    let blob = create_blob(
        state.writer_db(),
        policy.id,
        "files/claimed.bin",
        9,
        file_repo::BLOB_CLEANUP_CLAIMED_REF_COUNT,
    )
    .await;
    driver.insert_object(&blob.storage_path);

    let cleaned = ensure_blob_cleanup_if_unreferenced(&state, blob.id).await;

    assert!(
        cleaned,
        "cleanup-claimed blob should be treated as already handled by another worker"
    );
    assert_eq!(
        driver.delete_calls(),
        0,
        "cleanup-claimed blob must not be deleted by a competing cleanup path"
    );
    let reloaded_blob = file_blob::Entity::find_by_id(blob.id)
        .one(state.writer_db())
        .await
        .expect("blob lookup should succeed")
        .expect("cleanup-claimed blob row should remain");
    assert_eq!(
        reloaded_blob.ref_count,
        file_repo::BLOB_CLEANUP_CLAIMED_REF_COUNT
    );
}

#[tokio::test]
async fn cleanup_unreferenced_blob_skips_cleanup_claimed_blob() {
    let (state, _user, policy, driver) = build_deletion_test_state().await;
    let blob = create_blob(
        state.writer_db(),
        policy.id,
        "files/reconcile-claimed.bin",
        9,
        file_repo::BLOB_CLEANUP_CLAIMED_REF_COUNT,
    )
    .await;
    driver.insert_object(&blob.storage_path);

    let cleaned = cleanup_unreferenced_blob(&state, &blob).await;

    assert!(
        !cleaned,
        "maintenance cleanup should not count a cleanup-claimed blob as deleted"
    );
    assert_eq!(
        driver.delete_calls(),
        0,
        "cleanup-claimed blob must not be deleted by a competing cleanup path"
    );
    let reloaded_blob = file_blob::Entity::find_by_id(blob.id)
        .one(state.writer_db())
        .await
        .expect("blob lookup should succeed")
        .expect("cleanup-claimed blob row should remain");
    assert_eq!(
        reloaded_blob.ref_count,
        file_repo::BLOB_CLEANUP_CLAIMED_REF_COUNT
    );
}

#[tokio::test]
async fn cleanup_unreferenced_blob_keeps_row_and_primary_object_when_thumbnail_delete_fails() {
    let (state, _user, policy, driver) = build_deletion_test_state().await;
    let blob = create_blob(
        state.writer_db(),
        policy.id,
        "files/thumbnail-delete-failed.bin",
        9,
        0,
    )
    .await;
    let thumbnail_path = "thumbs/thumbnail-delete-failed.webp";
    let mut active_blob: file_blob::ActiveModel = blob.clone().into();
    active_blob.thumbnail_path = Set(Some(thumbnail_path.to_string()));
    let blob = active_blob
        .update(state.writer_db())
        .await
        .expect("test blob thumbnail metadata should update");
    driver.insert_object(&blob.storage_path);
    driver.insert_object(thumbnail_path);
    driver.fail_delete_for(thumbnail_path);

    let cleaned = cleanup_unreferenced_blob(&state, &blob).await;

    assert!(
        !cleaned,
        "cleanup must report failure when a derived thumbnail object cannot be deleted"
    );
    let reloaded_blob = file_blob::Entity::find_by_id(blob.id)
        .one(state.writer_db())
        .await
        .expect("blob lookup should succeed")
        .expect("blob row should remain for a later cleanup retry");
    assert_eq!(
        reloaded_blob.ref_count, 0,
        "failed cleanup should release the cleanup claim instead of leaving the blob stuck"
    );
    assert_eq!(
        reloaded_blob.thumbnail_path.as_deref(),
        Some(thumbnail_path),
        "thumbnail metadata should remain so the next cleanup retry still knows the derived object"
    );
    assert!(
        driver.contains(&blob.storage_path),
        "primary blob object must remain when thumbnail cleanup fails"
    );
    assert!(
        driver.contains(thumbnail_path),
        "failed thumbnail object should remain tracked for retry"
    );
    let attempts = driver.delete_attempts();
    assert!(
        attempts.iter().any(|path| path == thumbnail_path),
        "cleanup should attempt to delete the thumbnail object first"
    );
    assert!(
        !attempts.iter().any(|path| path == &blob.storage_path),
        "primary object delete must not be attempted after thumbnail cleanup fails"
    );
}

#[tokio::test]
async fn batch_purge_in_scope_deletes_last_blob_reference() {
    let (state, user, policy, driver) = build_deletion_test_state().await;
    let blob = create_blob(state.writer_db(), policy.id, "files/last-ref.bin", 11, 1).await;
    driver.insert_object(&blob.storage_path);
    let file = create_file(state.writer_db(), user.id, blob.id, 11, "last-ref.bin").await;
    set_user_storage_used(state.writer_db(), &user, 11).await;

    let purged = batch_purge_in_scope(
        &state,
        WorkspaceStorageScope::Personal { user_id: user.id },
        vec![file.clone()],
    )
    .await
    .expect("batch purge should succeed");

    assert_eq!(purged, 1);
    assert_eq!(
        driver.delete_calls(),
        1,
        "last blob reference should delete object"
    );
    assert!(
        file::Entity::find_by_id(file.id)
            .one(state.writer_db())
            .await
            .expect("file lookup should succeed")
            .is_none(),
        "file row should be deleted"
    );
    assert!(
        file_blob::Entity::find_by_id(blob.id)
            .one(state.writer_db())
            .await
            .expect("blob lookup should succeed")
            .is_none(),
        "blob row should be deleted when the last reference is purged"
    );
    let reloaded_user = user::Entity::find_by_id(user.id)
        .one(state.writer_db())
        .await
        .expect("user lookup should succeed")
        .expect("user should remain");
    assert_eq!(
        reloaded_user.storage_used, 0,
        "purge should reclaim user storage"
    );
}

#[tokio::test]
async fn batch_purge_in_scope_keeps_blob_when_other_file_still_references_it() {
    let (state, user, policy, driver) = build_deletion_test_state().await;
    let blob = create_blob(state.writer_db(), policy.id, "files/shared.bin", 13, 2).await;
    driver.insert_object(&blob.storage_path);
    let file_a = create_file(state.writer_db(), user.id, blob.id, 13, "shared-a.bin").await;
    let _file_b = create_file(state.writer_db(), user.id, blob.id, 13, "shared-b.bin").await;
    set_user_storage_used(state.writer_db(), &user, 26).await;

    let purged = batch_purge_in_scope(
        &state,
        WorkspaceStorageScope::Personal { user_id: user.id },
        vec![file_a.clone()],
    )
    .await
    .expect("batch purge should succeed");

    assert_eq!(purged, 1);
    assert_eq!(
        driver.delete_calls(),
        0,
        "shared blob must not be deleted while another file still references it"
    );
    let reloaded_blob = file_blob::Entity::find_by_id(blob.id)
        .one(state.writer_db())
        .await
        .expect("blob lookup should succeed")
        .expect("shared blob should remain");
    assert_eq!(
        reloaded_blob.ref_count, 1,
        "shared blob ref_count should decrement to 1"
    );
    let reloaded_user = user::Entity::find_by_id(user.id)
        .one(state.writer_db())
        .await
        .expect("user lookup should succeed")
        .expect("user should remain");
    assert_eq!(
        reloaded_user.storage_used, 13,
        "only one file's bytes should be reclaimed"
    );
}
