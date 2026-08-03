use std::collections::BTreeSet;

use aster_forge_db::transaction;
use chrono::Utc;

use crate::db::repository::{lock_namespace_repo, lock_repo};
use crate::errors::Result;
use crate::runtime::SharedRuntimeState;
use crate::services::ops::audit::{self, AuditContext};

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
