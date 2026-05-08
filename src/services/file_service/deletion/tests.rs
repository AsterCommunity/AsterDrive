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
use crate::cache;
use crate::config::{CacheConfig, Config, DatabaseConfig, RuntimeConfig};
use crate::entities::{file, file_blob, storage_policy, user};
use crate::runtime::PrimaryAppState;
use crate::services::mail_service;
use crate::services::workspace_storage_service::WorkspaceStorageScope;
use crate::storage::driver::BlobMetadata;
use crate::storage::{DriverRegistry, PolicySnapshot, StorageDriver};
use crate::types::{
    DriverType, StoredStoragePolicyAllowedTypes, StoredStoragePolicyOptions, UserRole, UserStatus,
};

#[derive(Clone, Default)]
struct TrackingDeleteDriver {
    objects: Arc<Mutex<HashSet<String>>>,
    delete_calls: Arc<AtomicUsize>,
}

impl TrackingDeleteDriver {
    fn insert_object(&self, path: &str) {
        self.objects
            .lock()
            .expect("tracking delete driver lock should succeed")
            .insert(path.to_string());
    }

    fn delete_calls(&self) -> usize {
        self.delete_calls.load(Ordering::SeqCst)
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

    let db = crate::db::connect(&DatabaseConfig {
        url: "sqlite::memory:".to_string(),
        pool_size: 1,
        retry_count: 0,
    })
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
        enabled: false,
        ..Default::default()
    })
    .await;
    let mut config = Config::default();
    config.server.temp_dir = temp_root.join(".tmp").to_string_lossy().into_owned();
    config.server.upload_temp_dir = temp_root.join(".uploads").to_string_lossy().into_owned();

    let driver = TrackingDeleteDriver::default();
    let driver_registry = Arc::new(DriverRegistry::new());
    driver_registry.insert_for_test(policy.id, Arc::new(driver.clone()));
    let policy_snapshot = Arc::new(PolicySnapshot::new());
    policy_snapshot
        .reload(&db)
        .await
        .expect("policy snapshot should reload");

    let (storage_change_tx, _) = tokio::sync::broadcast::channel(
        crate::services::storage_change_service::STORAGE_CHANGE_CHANNEL_CAPACITY,
    );
    let share_download_rollback =
        crate::services::share_service::spawn_detached_share_download_rollback_queue(
            db.clone(),
            crate::config::operations::share_download_rollback_queue_capacity(&runtime_config),
        );
    let state = PrimaryAppState {
        db,
        driver_registry,
        runtime_config: runtime_config.clone(),
        policy_snapshot,
        config: Arc::new(config),
        cache,
        mail_sender: mail_service::runtime_sender(runtime_config),
        storage_change_tx,
        share_download_rollback,
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
    let blob = create_blob(&state.db, policy.id, "files/orphan.bin", 7, 0).await;
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
            .one(&state.db)
            .await
            .expect("blob lookup should succeed")
            .is_none(),
        "blob row should be deleted after cleanup"
    );
}

#[tokio::test]
async fn ensure_blob_cleanup_if_unreferenced_skips_referenced_blob() {
    let (state, _user, policy, driver) = build_deletion_test_state().await;
    let blob = create_blob(&state.db, policy.id, "files/in-use.bin", 9, 2).await;
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
            .one(&state.db)
            .await
            .expect("blob lookup should succeed")
            .is_some(),
        "referenced blob row must remain"
    );
}

#[tokio::test]
async fn batch_purge_in_scope_deletes_last_blob_reference() {
    let (state, user, policy, driver) = build_deletion_test_state().await;
    let blob = create_blob(&state.db, policy.id, "files/last-ref.bin", 11, 1).await;
    driver.insert_object(&blob.storage_path);
    let file = create_file(&state.db, user.id, blob.id, 11, "last-ref.bin").await;
    set_user_storage_used(&state.db, &user, 11).await;

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
            .one(&state.db)
            .await
            .expect("file lookup should succeed")
            .is_none(),
        "file row should be deleted"
    );
    assert!(
        file_blob::Entity::find_by_id(blob.id)
            .one(&state.db)
            .await
            .expect("blob lookup should succeed")
            .is_none(),
        "blob row should be deleted when the last reference is purged"
    );
    let reloaded_user = user::Entity::find_by_id(user.id)
        .one(&state.db)
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
    let blob = create_blob(&state.db, policy.id, "files/shared.bin", 13, 2).await;
    driver.insert_object(&blob.storage_path);
    let file_a = create_file(&state.db, user.id, blob.id, 13, "shared-a.bin").await;
    let _file_b = create_file(&state.db, user.id, blob.id, 13, "shared-b.bin").await;
    set_user_storage_used(&state.db, &user, 26).await;

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
        .one(&state.db)
        .await
        .expect("blob lookup should succeed")
        .expect("shared blob should remain");
    assert_eq!(
        reloaded_blob.ref_count, 1,
        "shared blob ref_count should decrement to 1"
    );
    let reloaded_user = user::Entity::find_by_id(user.id)
        .one(&state.db)
        .await
        .expect("user lookup should succeed")
        .expect("user should remain");
    assert_eq!(
        reloaded_user.storage_used, 13,
        "only one file's bytes should be reclaimed"
    );
}
