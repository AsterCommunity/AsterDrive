use chrono::{Duration, Utc};
use sea_orm::Set;

use crate::api::api_error_code::ApiErrorCode;
use crate::db::repository::{file_repo, folder_repo, lock_repo};
use crate::entities::resource_lock;
use crate::errors::{AsterError, Result, auth_forbidden_with_code};
use crate::runtime::SharedRuntimeState;
use crate::services::audit_service::{self, AuditContext};
use crate::types::EntityType;

use super::models::ResourceLockOwnerInfo;
use super::owner_info::serialize_resource_lock_owner_info;
use super::ownership::check_entity_ownership;
use super::path::resolve_entity_path;
use super::state::{clear_entity_locked_if_unlocked, set_entity_locked};

/// 锁定资源（REST/WebDAV/Web Editor 统一入口）
pub async fn lock(
    state: &impl SharedRuntimeState,
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
    let txn = crate::db::transaction::begin(state.writer_db()).await?;

    let result = async {
        lock_target_entity(&txn, entity_type, entity_id).await?;

        for existing in lock_repo::find_all_by_entity(&txn, entity_type, entity_id).await? {
            if existing
                .timeout_at
                .is_some_and(|timeout_at| timeout_at < now)
            {
                tracing::debug!(
                    lock_id = existing.id,
                    entity_type = ?entity_type,
                    entity_id,
                    "removing expired lock before acquiring replacement"
                );
                lock_repo::delete_by_id(&txn, existing.id).await?;
                continue;
            }

            return Err(AsterError::resource_locked("resource is already locked"));
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

        let lock = lock_repo::create(&txn, model).await?;
        set_entity_locked(&txn, entity_type, entity_id, true).await?;
        Ok(lock)
    }
    .await;

    match result {
        Ok(lock) => {
            crate::db::transaction::commit(txn).await?;
            tracing::debug!(
                lock_id = lock.id,
                entity_type = ?lock.entity_type,
                entity_id = lock.entity_id,
                owner_id = lock.owner_id,
                timeout_at = ?lock.timeout_at,
                "locked resource"
            );
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
    state: &impl SharedRuntimeState,
    entity_type: EntityType,
    entity_id: i64,
    user_id: i64,
) -> Result<()> {
    let now = Utc::now();

    crate::db::transaction::with_transaction(state.writer_db(), async |txn| {
        lock_target_entity(txn, entity_type, entity_id).await?;
        lock_repo::delete_expired_by_entity_before(txn, entity_type, entity_id, now).await?;

        let locks = lock_repo::find_all_by_entity_for_update(txn, entity_type, entity_id).await?;
        if !locks.is_empty() {
            let is_entity_owner =
                check_entity_ownership(txn, entity_type, entity_id, user_id).await?;
            if is_entity_owner {
                lock_repo::delete_by_entity(txn, entity_type, entity_id).await?;
            } else {
                let has_foreign_active_lock = locks.iter().any(|lock| {
                    lock.timeout_at.is_none_or(|timeout_at| timeout_at >= now)
                        && lock.owner_id != Some(user_id)
                });
                if has_foreign_active_lock {
                    return Err(auth_forbidden_with_code(
                        ApiErrorCode::LockNotOwner,
                        "not the lock owner",
                    ));
                }

                lock_repo::delete_by_entity_and_owner(txn, entity_type, entity_id, user_id).await?;
            }
        }

        clear_entity_locked_if_unlocked(txn, entity_type, entity_id).await?;
        Ok(())
    })
    .await?;

    tracing::debug!(
        entity_type = ?entity_type,
        entity_id,
        user_id,
        "unlocked resource"
    );
    Ok(())
}

/// 按 token 解锁（WebDAV UNLOCK 用）
pub async fn unlock_by_token(state: &impl SharedRuntimeState, token: &str) -> Result<()> {
    let db = state.writer_db();
    let lock = lock_repo::find_by_token(db, token)
        .await?
        .ok_or_else(|| AsterError::record_not_found("lock not found"))?;

    lock_repo::delete_by_token(db, token).await?;
    clear_entity_locked_if_unlocked(db, lock.entity_type, lock.entity_id).await?;
    tracing::debug!(
        lock_id = lock.id,
        entity_type = ?lock.entity_type,
        entity_id = lock.entity_id,
        "unlocked resource by token"
    );
    Ok(())
}

/// 强制解锁（admin 用）
pub async fn force_unlock(state: &impl SharedRuntimeState, lock_id: i64) -> Result<()> {
    let db = state.writer_db();
    let lock = lock_repo::find_by_id(db, lock_id)
        .await?
        .ok_or_else(|| AsterError::record_not_found("lock not found"))?;

    lock_repo::delete_by_id(db, lock_id).await?;
    clear_entity_locked_if_unlocked(db, lock.entity_type, lock.entity_id).await?;
    tracing::debug!(
        lock_id,
        entity_type = ?lock.entity_type,
        entity_id = lock.entity_id,
        "force unlocked resource"
    );
    Ok(())
}

pub async fn force_unlock_with_audit(
    state: &impl SharedRuntimeState,
    lock_id: i64,
    audit_ctx: &AuditContext,
) -> Result<()> {
    let lock = lock_repo::find_by_id(state.writer_db(), lock_id)
        .await?
        .ok_or_else(|| AsterError::record_not_found("lock not found"))?;
    force_unlock(state, lock_id).await?;
    audit_service::log_with_details(
        state,
        audit_ctx,
        audit_service::AuditAction::AdminForceUnlock,
        crate::services::audit_service::AuditEntityType::ResourceLock,
        Some(lock_id),
        Some(&lock.path),
        || {
            audit_service::details(audit_service::LockAuditDetails {
                entity_type: lock.entity_type,
                entity_id: lock.entity_id,
            })
        },
    )
    .await;
    Ok(())
}

async fn lock_target_entity<C: sea_orm::ConnectionTrait>(
    db: &C,
    entity_type: EntityType,
    entity_id: i64,
) -> Result<()> {
    match entity_type {
        EntityType::File => {
            file_repo::lock_by_id(db, entity_id).await?;
        }
        EntityType::Folder => {
            folder_repo::lock_by_id(db, entity_id).await?;
        }
    }
    Ok(())
}
