use super::state::clear_entity_locked_if_unlocked;
use super::*;

use std::sync::Arc;

use chrono::{Duration, Utc};
use migration::Migrator;
use sea_orm::{ActiveModelTrait, Set};

use crate::cache;
use crate::config::{CacheConfig, Config, DatabaseConfig, RuntimeConfig};
use crate::db::repository::{file_repo, lock_repo};
use crate::entities::{file, file_blob, resource_lock, storage_policy, user};
use crate::runtime::PrimaryAppState;
use crate::services::mail_service;
use crate::storage::{DriverRegistry, PolicySnapshot};
use crate::types::{
    DriverType, EntityType, StoredLockOwnerInfo, StoredStoragePolicyAllowedTypes,
    StoredStoragePolicyOptions, UserRole, UserStatus,
};

fn sample_lock(owner_info: Option<StoredLockOwnerInfo>) -> resource_lock::Model {
    resource_lock::Model {
        id: 42,
        token: "urn:uuid:test".to_string(),
        entity_type: EntityType::File,
        entity_id: 7,
        path: "/docs/report.txt".to_string(),
        owner_id: Some(9),
        owner_info,
        timeout_at: None,
        shared: false,
        deep: false,
        created_at: Utc::now(),
    }
}

async fn build_lock_test_state() -> (PrimaryAppState, user::Model, file::Model) {
    let temp_root =
        std::env::temp_dir().join(format!("asterdrive-lock-service-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&temp_root).expect("lock service temp root should exist");

    let db = crate::db::connect(&DatabaseConfig {
        url: "sqlite::memory:".to_string(),
        pool_size: 1,
        retry_count: 0,
    })
    .await
    .expect("lock service test DB should connect");
    Migrator::up(&db, None)
        .await
        .expect("lock service migrations should succeed");

    let now = Utc::now();
    let policy = storage_policy::ActiveModel {
        name: Set("Lock Test Policy".to_string()),
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
    .expect("lock test policy should insert");

    let user = user::ActiveModel {
        username: Set(format!("lock-user-{}", uuid::Uuid::new_v4())),
        email: Set(format!("lock-{}@example.com", uuid::Uuid::new_v4())),
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
    .expect("lock test user should insert");

    let blob = file_blob::ActiveModel {
        hash: Set(format!("lock-blob-{}", uuid::Uuid::new_v4())),
        size: Set(1),
        policy_id: Set(policy.id),
        storage_path: Set(format!("files/{}", uuid::Uuid::new_v4())),
        thumbnail_path: Set(None),
        thumbnail_processor: Set(None),
        thumbnail_version: Set(None),
        ref_count: Set(1),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("lock test blob should insert");

    let file = file::ActiveModel {
        name: Set("lock-target.txt".to_string()),
        folder_id: Set(None),
        team_id: Set(None),
        blob_id: Set(blob.id),
        size: Set(1),
        owner_user_id: Set(Some(user.id)),
        created_by_user_id: Set(Some(user.id)),
        created_by_username: Set(user.username.clone()),
        mime_type: Set("text/plain".to_string()),
        created_at: Set(now),
        updated_at: Set(now),
        deleted_at: Set(None),
        is_locked: Set(false),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("lock test file should insert");

    let runtime_config = Arc::new(RuntimeConfig::new());
    let cache = cache::create_cache(&CacheConfig {
        enabled: false,
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
        mail_sender: mail_service::runtime_sender(runtime_config),
        storage_change_tx,
        share_download_rollback,
        background_task_dispatch_wakeup:
            crate::runtime::PrimaryAppState::new_background_task_dispatch_wakeup(),
    };

    (state, user, file)
}

#[test]
fn serializes_and_deserializes_wopi_owner_payload() {
    let owner_info = ResourceLockOwnerInfo::Wopi(WopiLockOwnerInfo {
        app_key: "collabora".to_string(),
        lock: "lock-123".to_string(),
    });
    let stored = serialize_resource_lock_owner_info(Some(&owner_info))
        .expect("wopi payload should serialize")
        .expect("stored owner info should exist");
    let parsed = deserialize_resource_lock_owner_info(&sample_lock(Some(stored)))
        .expect("wopi payload should deserialize");

    assert_eq!(parsed, Some(owner_info));
}

#[test]
fn serializes_and_deserializes_webdav_owner_payload() {
    let owner_info = ResourceLockOwnerInfo::Webdav(WebdavLockOwnerInfo {
        xml: "<D:owner xmlns:D=\"DAV:\"><D:href>mailto:test@example.com</D:href></D:owner>"
            .to_string(),
    });
    let stored = serialize_resource_lock_owner_info(Some(&owner_info))
        .expect("webdav payload should serialize")
        .expect("stored owner info should exist");
    let parsed = deserialize_resource_lock_owner_info(&sample_lock(Some(stored)))
        .expect("webdav payload should deserialize");

    assert_eq!(parsed, Some(owner_info));
}

#[test]
fn serializes_and_deserializes_text_owner_payload() {
    let owner_info = ResourceLockOwnerInfo::Text(TextLockOwnerInfo {
        value: "user@example.com".to_string(),
    });
    let stored = serialize_resource_lock_owner_info(Some(&owner_info))
        .expect("text payload should serialize")
        .expect("stored owner info should exist");
    let parsed = deserialize_resource_lock_owner_info(&sample_lock(Some(stored)))
        .expect("text owner payload should deserialize");

    assert_eq!(parsed, Some(owner_info));
}

#[test]
fn rejects_legacy_webdav_xml_owner_payload() {
    let error = deserialize_resource_lock_owner_info(&sample_lock(Some(StoredLockOwnerInfo(
        "<D:owner xmlns:D=\"DAV:\"><D:href>mailto:test@example.com</D:href></D:owner>".to_string(),
    ))))
    .expect_err("legacy raw xml payload should be rejected");

    assert!(
        error
            .to_string()
            .contains("deserialize resource lock owner payload")
    );
}

#[test]
fn rejects_legacy_text_owner_payload() {
    let error = deserialize_resource_lock_owner_info(&sample_lock(Some(StoredLockOwnerInfo(
        "user@example.com".to_string(),
    ))))
    .expect_err("legacy raw text payload should be rejected");

    assert!(
        error
            .to_string()
            .contains("deserialize resource lock owner payload")
    );
}

#[test]
fn rejects_unknown_owner_payload_kind() {
    let error = deserialize_resource_lock_owner_info(&sample_lock(Some(StoredLockOwnerInfo(
        r#"{"kind":"legacy","value":"user@example.com"}"#.to_string(),
    ))))
    .expect_err("unknown owner payload kind should be rejected");

    assert!(
        error
            .to_string()
            .contains("deserialize resource lock owner payload")
    );
}

#[tokio::test]
async fn lock_replaces_expired_lock_and_keeps_single_row() {
    let (state, user, file) = build_lock_test_state().await;
    let now = Utc::now();
    lock_repo::create(
        state.writer_db(),
        resource_lock::ActiveModel {
            token: Set("expired-lock".to_string()),
            entity_type: Set(EntityType::File),
            entity_id: Set(file.id),
            path: Set("/lock-target.txt".to_string()),
            owner_id: Set(Some(user.id)),
            owner_info: Set(None),
            timeout_at: Set(Some(now - Duration::seconds(5))),
            shared: Set(false),
            deep: Set(false),
            created_at: Set(now - Duration::seconds(30)),
            ..Default::default()
        },
    )
    .await
    .expect("expired lock should insert");

    let replacement = lock(
        &state,
        EntityType::File,
        file.id,
        Some(user.id),
        None,
        Some(Duration::seconds(30)),
    )
    .await
    .expect("expired lock should be replaced");

    let locks = lock_repo::find_all(state.writer_db())
        .await
        .expect("locks should load");
    assert_eq!(locks.len(), 1, "only the replacement lock should remain");
    assert_eq!(locks[0].id, replacement.id);
    assert_ne!(locks[0].token, "expired-lock");

    let reloaded_file = file_repo::find_by_id(state.writer_db(), file.id)
        .await
        .expect("file should reload");
    assert!(
        reloaded_file.is_locked,
        "replacement lock should sync is_locked"
    );
}

#[tokio::test]
async fn clear_entity_locked_if_unlocked_keeps_flag_when_replacement_lock_exists() {
    let (state, user, file) = build_lock_test_state().await;
    set_entity_locked(state.writer_db(), EntityType::File, file.id, true)
        .await
        .expect("file should be marked locked");

    let now = Utc::now();
    lock_repo::create(
        state.writer_db(),
        resource_lock::ActiveModel {
            token: Set("active-lock".to_string()),
            entity_type: Set(EntityType::File),
            entity_id: Set(file.id),
            path: Set("/lock-target.txt".to_string()),
            owner_id: Set(Some(user.id)),
            owner_info: Set(None),
            timeout_at: Set(Some(now + Duration::seconds(30))),
            shared: Set(false),
            deep: Set(false),
            created_at: Set(now),
            ..Default::default()
        },
    )
    .await
    .expect("active lock should insert");

    clear_entity_locked_if_unlocked(state.writer_db(), EntityType::File, file.id)
        .await
        .expect("helper should succeed");

    let reloaded_file = file_repo::find_by_id(state.writer_db(), file.id)
        .await
        .expect("file should reload");
    assert!(
        reloaded_file.is_locked,
        "existing replacement lock must keep is_locked cache set"
    );
}

#[tokio::test]
async fn clear_entity_locked_if_unlocked_clears_flag_when_no_lock_remains() {
    let (state, _, file) = build_lock_test_state().await;
    set_entity_locked(state.writer_db(), EntityType::File, file.id, true)
        .await
        .expect("file should be marked locked");

    clear_entity_locked_if_unlocked(state.writer_db(), EntityType::File, file.id)
        .await
        .expect("helper should succeed");

    let reloaded_file = file_repo::find_by_id(state.writer_db(), file.id)
        .await
        .expect("file should reload");
    assert!(
        !reloaded_file.is_locked,
        "is_locked should be cleared once no lock row remains"
    );
}
