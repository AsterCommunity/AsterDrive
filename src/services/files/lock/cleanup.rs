use std::collections::BTreeSet;

use aster_forge_db::transaction;
use chrono::Utc;
use sea_orm::ConnectionTrait;

use crate::db::repository::{lock_namespace_repo, lock_repo};
use crate::errors::Result;
use crate::runtime::SharedRuntimeState;
use crate::services::ops::audit::{self, AuditContext};
use aster_drive_model::types::LockWorkspaceType;

/// Remove expired locks in short namespace-serialized transactions.
pub async fn cleanup_expired(state: &impl SharedRuntimeState) -> Result<u64> {
    let now = Utc::now();
    let namespace_ids = lock_repo::find_expired_before(state.writer_db(), now)
        .await?
        .into_iter()
        .map(|lock| lock.namespace_id)
        .collect::<BTreeSet<_>>();
    let mut removed = 0_u64;

    for namespace_id in namespace_ids {
        let namespace_removed = transaction::with_transaction(state.writer_db(), async |txn| {
            let namespace = lock_namespace_repo::lock_by_id(txn, namespace_id).await?;
            let affected =
                lock_repo::delete_expired_by_namespace_before(txn, namespace_id, now).await?;
            if affected != 0 {
                lock_namespace_repo::increment_generation(txn, namespace).await?;
            }
            Ok::<u64, crate::errors::AsterError>(affected)
        })
        .await?;
        removed = removed.saturating_add(namespace_removed);
    }

    Ok(removed)
}

pub async fn cleanup_expired_with_audit(
    state: &impl SharedRuntimeState,
    audit_ctx: &AuditContext,
) -> Result<u64> {
    let count = cleanup_expired(state).await?;
    audit::log_with_details(
        state,
        audit_ctx,
        audit::AuditAction::AdminCleanupExpiredLocks,
        crate::services::ops::audit::AuditEntityType::ResourceLock,
        None,
        None,
        || audit::details(audit::LockCleanupAuditDetails { removed: count }),
    )
    .await;
    Ok(count)
}

pub(crate) async fn delete_all_held_by_on<C: ConnectionTrait>(
    db: &C,
    holder_user_id: i64,
) -> Result<u64> {
    let namespace_ids = lock_repo::find_by_owner(db, holder_user_id)
        .await?
        .into_iter()
        .map(|lock| lock.namespace_id)
        .collect::<BTreeSet<_>>();
    let mut removed = 0_u64;
    for namespace_id in namespace_ids {
        let namespace = lock_namespace_repo::lock_by_id(db, namespace_id).await?;
        let affected =
            lock_repo::delete_by_owner_in_namespace(db, namespace_id, holder_user_id).await?;
        if affected != 0 {
            lock_namespace_repo::increment_generation(db, namespace).await?;
            removed = removed.saturating_add(affected);
        }
    }
    Ok(removed)
}

pub(crate) async fn delete_workspace_namespace_on<C: ConnectionTrait>(
    db: &C,
    workspace_type: LockWorkspaceType,
    workspace_id: i64,
) -> Result<u64> {
    lock_namespace_repo::delete_by_workspace(db, workspace_type, workspace_id).await
}
