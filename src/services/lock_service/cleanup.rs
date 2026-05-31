use std::collections::BTreeSet;

use chrono::Utc;

use crate::db::repository::lock_repo;
use crate::errors::Result;
use crate::runtime::PrimaryAppState;
use crate::services::audit_service::{self, AuditContext};
use crate::types::EntityType;
use crate::utils::numbers::usize_to_u64;

/// 清理过期锁（后台任务用）
pub async fn cleanup_expired(state: &PrimaryAppState) -> Result<u64> {
    let db = state.writer_db();

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
    audit_service::log_with_details(
        state,
        audit_ctx,
        audit_service::AuditAction::AdminCleanupExpiredLocks,
        crate::services::audit_service::AuditEntityType::ResourceLock,
        None,
        None,
        || audit_service::details(audit_service::LockCleanupAuditDetails { removed: count }),
    )
    .await;
    Ok(count)
}
