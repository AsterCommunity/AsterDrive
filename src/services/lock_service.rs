//! 服务模块：`lock_service`。

use std::collections::BTreeSet;

use chrono::{Duration, Utc};
use sea_orm::{ConnectionTrait, Set};
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::api::pagination::{AdminLockSortBy, OffsetPage, SortOrder, load_offset_page};
use crate::db::repository::{file_repo, folder_repo, lock_repo};
use crate::entities::resource_lock;
use crate::errors::{AsterError, Result};
use crate::runtime::PrimaryAppState;
use crate::services::{
    audit_service::{self, AuditContext},
    folder_service, profile_service, user_service,
};
use crate::types::{EntityType, StoredLockOwnerInfo};
use crate::utils::numbers::usize_to_u64;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct WopiLockOwnerInfo {
    pub app_key: String,
    pub lock: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct WebdavLockOwnerInfo {
    pub xml: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct TextLockOwnerInfo {
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ResourceLockOwnerInfo {
    Wopi(WopiLockOwnerInfo),
    Webdav(WebdavLockOwnerInfo),
    Text(TextLockOwnerInfo),
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ResourceLock {
    pub id: i64,
    pub token: String,
    pub entity_type: EntityType,
    pub entity_id: i64,
    pub path: String,
    pub owner: Option<user_service::UserSummary>,
    pub owner_info: Option<ResourceLockOwnerInfo>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub timeout_at: Option<chrono::DateTime<chrono::Utc>>,
    pub shared: bool,
    pub deep: bool,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// 锁定资源（REST/WebDAV/Web Editor 统一入口）
pub async fn lock(
    state: &PrimaryAppState,
    entity_type: EntityType,
    entity_id: i64,
    owner_id: Option<i64>,
    owner_info: Option<ResourceLockOwnerInfo>,
    timeout: Option<Duration>,
) -> Result<resource_lock::Model> {
    let now = Utc::now();
    let token = format!("urn:uuid:{}", uuid::Uuid::new_v4());
    let timeout_at = timeout.map(|d| now + d);
    let serialized_owner_info = serialize_resource_lock_owner_info(owner_info.as_ref())?;
    let txn = crate::db::transaction::begin(&state.db).await?;

    let result = async {
        if let Some(existing) = lock_repo::find_by_entity(&txn, entity_type, entity_id).await? {
            match existing.timeout_at {
                Some(timeout_at) if timeout_at < now => {
                    lock_repo::delete_by_id(&txn, existing.id).await?;
                }
                _ => return Err(AsterError::resource_locked("resource is already locked")),
            }
        }

        let path = resolve_entity_path(&txn, entity_type, entity_id).await?;
        let model = resource_lock::ActiveModel {
            token: Set(token),
            entity_type: Set(entity_type),
            entity_id: Set(entity_id),
            path: Set(path),
            owner_id: Set(owner_id),
            owner_info: Set(serialized_owner_info),
            timeout_at: Set(timeout_at),
            shared: Set(false),
            deep: Set(false),
            created_at: Set(now),
            ..Default::default()
        };

        let lock = lock_repo::create_if_unlocked(&txn, model, entity_type, entity_id)
            .await?
            .ok_or_else(|| AsterError::resource_locked("resource is already locked"))?;
        set_entity_locked(&txn, entity_type, entity_id, true).await?;
        Ok(lock)
    }
    .await;

    match result {
        Ok(lock) => {
            crate::db::transaction::commit(txn).await?;
            Ok(lock)
        }
        Err(error) => {
            if let Err(rollback_error) = crate::db::transaction::rollback(txn).await {
                tracing::error!(
                    entity_type = ?entity_type,
                    entity_id,
                    original_error = %error,
                    rollback_error = %rollback_error,
                    "failed to rollback lock acquisition transaction"
                );
            }
            Err(error)
        }
    }
}

/// 解锁资源（用户主动解锁）
pub async fn unlock(
    state: &PrimaryAppState,
    entity_type: EntityType,
    entity_id: i64,
    user_id: i64,
) -> Result<()> {
    let db = &state.db;

    // 校验归属：只有锁持有者或文件所有者可以解锁
    if let Some(existing) = lock_repo::find_by_entity(db, entity_type, entity_id).await? {
        let is_owner = existing.owner_id == Some(user_id);
        let is_entity_owner = check_entity_ownership(db, entity_type, entity_id, user_id).await?;
        if !is_owner && !is_entity_owner {
            return Err(AsterError::auth_forbidden("not the lock owner"));
        }
    }

    do_unlock_by_entity(state, entity_type, entity_id).await
}

/// 按 token 解锁（WebDAV UNLOCK 用）
pub async fn unlock_by_token(state: &PrimaryAppState, token: &str) -> Result<()> {
    let db = &state.db;
    let lock = lock_repo::find_by_token(db, token)
        .await?
        .ok_or_else(|| AsterError::record_not_found("lock not found"))?;

    lock_repo::delete_by_token(db, token).await?;
    clear_entity_locked_if_unlocked(db, lock.entity_type, lock.entity_id).await?;
    Ok(())
}

pub async fn list_paginated(
    state: &PrimaryAppState,
    limit: u64,
    offset: u64,
    sort_by: AdminLockSortBy,
    sort_order: SortOrder,
) -> Result<OffsetPage<ResourceLock>> {
    load_offset_page(limit, offset, 100, |limit, offset| async move {
        let (items, total) = crate::db::repository::lock_repo::find_paginated(
            &state.db, limit, offset, sort_by, sort_order,
        )
        .await?;
        let items = build_resource_locks(state, items).await?;
        Ok((items, total))
    })
    .await
}

async fn build_resource_locks(
    state: &PrimaryAppState,
    locks: Vec<resource_lock::Model>,
) -> Result<Vec<ResourceLock>> {
    let owner_ids: Vec<i64> = locks.iter().filter_map(|lock| lock.owner_id).collect();
    let owners = user_service::user_summaries_by_ids(
        state,
        &owner_ids,
        profile_service::AvatarAudience::AdminUser,
    )
    .await?;

    locks
        .into_iter()
        .map(|model| {
            let owner_info = deserialize_resource_lock_owner_info(&model)?;
            Ok(ResourceLock {
                id: model.id,
                token: model.token,
                entity_type: model.entity_type,
                entity_id: model.entity_id,
                path: model.path,
                owner: model
                    .owner_id
                    .and_then(|owner_id| owners.get(&owner_id).cloned()),
                owner_info,
                timeout_at: model.timeout_at,
                shared: model.shared,
                deep: model.deep,
                created_at: model.created_at,
            })
        })
        .collect()
}

/// 强制解锁（admin 用）
pub async fn force_unlock(state: &PrimaryAppState, lock_id: i64) -> Result<()> {
    let db = &state.db;
    let lock = lock_repo::find_by_id(db, lock_id)
        .await?
        .ok_or_else(|| AsterError::record_not_found("lock not found"))?;

    lock_repo::delete_by_id(db, lock_id).await?;
    clear_entity_locked_if_unlocked(db, lock.entity_type, lock.entity_id).await?;
    Ok(())
}

pub async fn force_unlock_with_audit(
    state: &PrimaryAppState,
    lock_id: i64,
    audit_ctx: &AuditContext,
) -> Result<()> {
    let lock = lock_repo::find_by_id(&state.db, lock_id)
        .await?
        .ok_or_else(|| AsterError::record_not_found("lock not found"))?;
    force_unlock(state, lock_id).await?;
    audit_service::log(
        state,
        audit_ctx,
        audit_service::AuditAction::AdminForceUnlock,
        Some("resource_lock"),
        Some(lock_id),
        Some(&lock.path),
        audit_service::details(audit_service::LockAuditDetails {
            entity_type: lock.entity_type,
            entity_id: lock.entity_id,
        }),
    )
    .await;
    Ok(())
}

/// 清理过期锁（后台任务用）
pub async fn cleanup_expired(state: &PrimaryAppState) -> Result<u64> {
    let db = &state.db;

    // 先查出过期锁的 entity 信息（需要重置 is_locked）
    let now = Utc::now();
    let expired = lock_repo::find_expired_before(db, now).await?;
    if expired.is_empty() {
        return Ok(0);
    }

    let count = usize_to_u64(expired.len(), "expired lock count")?;
    let mut file_ids = BTreeSet::new();
    let mut folder_ids = BTreeSet::new();
    for lock in &expired {
        match lock.entity_type {
            EntityType::File => {
                file_ids.insert(lock.entity_id);
            }
            EntityType::Folder => {
                folder_ids.insert(lock.entity_id);
            }
        }
    }

    // 批量删除
    lock_repo::delete_expired_before(db, now).await?;

    // 只在确无替代锁时清理 is_locked，避免和并发续锁/重锁打架。
    let file_ids: Vec<i64> = file_ids.into_iter().collect();
    if let Err(e) = lock_repo::clear_file_locked_flags_without_locks(db, &file_ids).await {
        tracing::warn!(
            expired_file_lock_count = file_ids.len(),
            "failed to batch-clear expired file locks: {e}"
        );
    }
    let folder_ids: Vec<i64> = folder_ids.into_iter().collect();
    if let Err(e) = lock_repo::clear_folder_locked_flags_without_locks(db, &folder_ids).await {
        tracing::warn!(
            expired_folder_lock_count = folder_ids.len(),
            "failed to batch-clear expired folder locks: {e}"
        );
    }

    Ok(count)
}

pub async fn cleanup_expired_with_audit(
    state: &PrimaryAppState,
    audit_ctx: &AuditContext,
) -> Result<u64> {
    let count = cleanup_expired(state).await?;
    audit_service::log(
        state,
        audit_ctx,
        audit_service::AuditAction::AdminCleanupExpiredLocks,
        Some("resource_lock"),
        None,
        None,
        audit_service::details(audit_service::LockCleanupAuditDetails { removed: count }),
    )
    .await;
    Ok(count)
}

// ── Internal helpers ────────────────────────────────────────────────

pub(crate) fn serialize_resource_lock_owner_info(
    owner_info: Option<&ResourceLockOwnerInfo>,
) -> Result<Option<StoredLockOwnerInfo>> {
    let Some(owner_info) = owner_info else {
        return Ok(None);
    };

    let raw = serde_json::to_string(owner_info).map_err(|error| {
        AsterError::internal_error(format!("serialize resource lock owner payload: {error}"))
    })?;

    Ok(Some(StoredLockOwnerInfo(raw)))
}

pub(crate) fn deserialize_resource_lock_owner_info(
    lock: &resource_lock::Model,
) -> Result<Option<ResourceLockOwnerInfo>> {
    let Some(raw) = lock.owner_info.as_ref() else {
        return Ok(None);
    };
    serde_json::from_str(raw.as_ref())
        .map(Some)
        .map_err(|error| {
            AsterError::internal_error(format!(
                "deserialize resource lock owner payload for lock #{}: {error}",
                lock.id
            ))
        })
}

async fn do_unlock_by_entity(
    state: &PrimaryAppState,
    entity_type: EntityType,
    entity_id: i64,
) -> Result<()> {
    lock_repo::delete_by_entity(&state.db, entity_type, entity_id).await?;
    clear_entity_locked_if_unlocked(&state.db, entity_type, entity_id).await?;
    Ok(())
}

async fn clear_entity_locked_if_unlocked(
    db: &impl ConnectionTrait,
    entity_type: EntityType,
    entity_id: i64,
) -> Result<()> {
    match entity_type {
        EntityType::File => {
            lock_repo::clear_file_locked_flag_without_lock(db, entity_id).await?;
        }
        EntityType::Folder => {
            lock_repo::clear_folder_locked_flag_without_lock(db, entity_id).await?;
        }
    }
    Ok(())
}

/// 同步 is_locked boolean 缓存（pub 给 db_lock_system 调用）
pub async fn set_entity_locked(
    db: &impl ConnectionTrait,
    entity_type: EntityType,
    entity_id: i64,
    locked: bool,
) -> Result<()> {
    use sea_orm::ActiveModelTrait;
    let now = Utc::now();

    match entity_type {
        EntityType::File => {
            let f = file_repo::find_by_id(db, entity_id).await?;
            let mut active: crate::entities::file::ActiveModel = f.into();
            active.is_locked = Set(locked);
            active.updated_at = Set(now);
            active.update(db).await.map_err(|e| {
                tracing::error!("failed to sync is_locked for file #{entity_id}: {e}");
                AsterError::from(e)
            })?;
        }
        EntityType::Folder => {
            let f = folder_repo::find_by_id(db, entity_id).await?;
            let mut active: crate::entities::folder::ActiveModel = f.into();
            active.is_locked = Set(locked);
            active.updated_at = Set(now);
            active.update(db).await.map_err(|e| {
                tracing::error!("failed to sync is_locked for folder #{entity_id}: {e}");
                AsterError::from(e)
            })?;
        }
    }
    Ok(())
}

/// 校验资源归属
async fn check_entity_ownership(
    db: &sea_orm::DatabaseConnection,
    entity_type: EntityType,
    entity_id: i64,
    user_id: i64,
) -> Result<bool> {
    match entity_type {
        EntityType::File => {
            let f = file_repo::find_by_id(db, entity_id).await?;
            Ok(f.owner_user_id == Some(user_id))
        }
        EntityType::Folder => {
            let f = folder_repo::find_by_id(db, entity_id).await?;
            Ok(f.owner_user_id == Some(user_id))
        }
    }
}

/// 从 entity 反查 WebDAV 路径
pub async fn resolve_entity_path<C: ConnectionTrait>(
    db: &C,
    entity_type: EntityType,
    entity_id: i64,
) -> Result<String> {
    match entity_type {
        EntityType::File => {
            let f = file_repo::find_by_id(db, entity_id).await?;
            let folder_path = match f.folder_id {
                Some(folder_id) => {
                    let mut folder_paths =
                        folder_service::build_folder_paths(db, &[folder_id]).await?;
                    let path = folder_paths.remove(&folder_id).ok_or_else(|| {
                        AsterError::record_not_found(format!("folder #{folder_id}"))
                    })?;
                    format!("{path}/")
                }
                None => String::new(),
            };
            if let Some(team_id) = f.team_id {
                let prefix = if folder_path.is_empty() {
                    format!("/teams/{team_id}/")
                } else {
                    format!("/teams/{team_id}{folder_path}")
                };
                Ok(format!("{prefix}{}", f.name))
            } else {
                let prefix = if folder_path.is_empty() {
                    "/"
                } else {
                    &folder_path
                };
                Ok(format!("{}{}", prefix, f.name))
            }
        }
        EntityType::Folder => {
            let f = folder_repo::find_by_id(db, entity_id).await?;
            let path = folder_service::build_folder_paths(db, &[f.id])
                .await?
                .remove(&f.id)
                .ok_or_else(|| AsterError::record_not_found(format!("folder #{}", f.id)))?;
            if let Some(team_id) = f.team_id {
                Ok(format!("/teams/{team_id}{path}/"))
            } else {
                Ok(format!("{path}/"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use crate::cache;
    use crate::config::{CacheConfig, Config, DatabaseConfig, RuntimeConfig};
    use crate::entities::{file, file_blob, storage_policy, user};
    use crate::services::mail_service;
    use crate::storage::{DriverRegistry, PolicySnapshot};
    use crate::types::{
        DriverType, StoredStoragePolicyAllowedTypes, StoredStoragePolicyOptions, UserRole,
        UserStatus,
    };
    use migration::Migrator;
    use sea_orm::{ActiveModelTrait, Set};

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
            db,
            driver_registry: Arc::new(DriverRegistry::new()),
            runtime_config: runtime_config.clone(),
            policy_snapshot: Arc::new(PolicySnapshot::new()),
            config: Arc::new(config),
            cache,
            mail_sender: mail_service::runtime_sender(runtime_config),
            storage_change_tx,
            share_download_rollback,
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
            "<D:owner xmlns:D=\"DAV:\"><D:href>mailto:test@example.com</D:href></D:owner>"
                .to_string(),
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
            &state.db,
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

        let locks = lock_repo::find_all(&state.db)
            .await
            .expect("locks should load");
        assert_eq!(locks.len(), 1, "only the replacement lock should remain");
        assert_eq!(locks[0].id, replacement.id);
        assert_ne!(locks[0].token, "expired-lock");

        let reloaded_file = file_repo::find_by_id(&state.db, file.id)
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
        set_entity_locked(&state.db, EntityType::File, file.id, true)
            .await
            .expect("file should be marked locked");

        let now = Utc::now();
        lock_repo::create(
            &state.db,
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

        clear_entity_locked_if_unlocked(&state.db, EntityType::File, file.id)
            .await
            .expect("helper should succeed");

        let reloaded_file = file_repo::find_by_id(&state.db, file.id)
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
        set_entity_locked(&state.db, EntityType::File, file.id, true)
            .await
            .expect("file should be marked locked");

        clear_entity_locked_if_unlocked(&state.db, EntityType::File, file.id)
            .await
            .expect("helper should succeed");

        let reloaded_file = file_repo::find_by_id(&state.db, file.id)
            .await
            .expect("file should reload");
        assert!(
            !reloaded_file.is_locked,
            "is_locked should be cleared once no lock row remains"
        );
    }
}
