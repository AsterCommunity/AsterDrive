use crate::db::repository::file_repo;
use crate::errors::{AsterError, Result};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::{events::storage_change, workspace::storage::WorkspaceStorageScope};
use aster_drive_model::entities::file;
use sea_orm::ConnectionTrait;

pub(crate) async fn delete_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    id: i64,
) -> Result<()> {
    tracing::debug!(scope = ?scope, file_id = id, "soft deleting file");
    let file = delete_in_scope_on(state.writer_db(), scope, id, false).await?;
    storage_change::publish(
        state,
        storage_change::StorageChangeEvent::new(
            storage_change::StorageChangeKind::FileTrashed,
            scope,
            vec![file.id],
            vec![],
            vec![file.folder_id],
        ),
    );
    tracing::debug!(
        scope = ?scope,
        file_id = file.id,
        folder_id = file.folder_id,
        "soft deleted file"
    );
    Ok(())
}

/// Soft-deletes one locked file row on the caller's transaction.
///
/// Protocol adapters may set `allow_locked` only after revalidating current lock rows and submitted
/// tokens on the same transaction.
pub(crate) async fn delete_in_scope_on<C: ConnectionTrait>(
    db: &C,
    scope: WorkspaceStorageScope,
    id: i64,
    allow_locked: bool,
) -> Result<file::Model> {
    let file = file_repo::lock_by_id(db, id).await?;
    crate::services::workspace::storage::ensure_active_file_scope(&file, scope)?;
    if file.is_locked && !allow_locked {
        return Err(AsterError::resource_locked("file is locked"));
    }
    file_repo::soft_delete(db, id).await?;
    Ok(file)
}

/// 删除文件（软删除 → 回收站）
pub async fn delete(state: &PrimaryAppState, id: i64, user_id: i64) -> Result<()> {
    delete_in_scope(state, WorkspaceStorageScope::Personal { user_id }, id).await
}
