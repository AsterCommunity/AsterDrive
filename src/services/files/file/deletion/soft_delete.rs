use crate::db::repository::file_repo;
use crate::errors::{AsterError, Result};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::{storage_change_service, workspace::storage::WorkspaceStorageScope};

pub(crate) async fn delete_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    id: i64,
) -> Result<()> {
    tracing::debug!(scope = ?scope, file_id = id, "soft deleting file");
    let file =
        crate::services::workspace::storage::verify_file_access(state, scope, id).await?;
    if file.is_locked {
        return Err(AsterError::resource_locked("file is locked"));
    }
    file_repo::soft_delete(state.writer_db(), id).await?;
    storage_change_service::publish(
        state,
        storage_change_service::StorageChangeEvent::new(
            storage_change_service::StorageChangeKind::FileTrashed,
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

/// 删除文件（软删除 → 回收站）
pub async fn delete(state: &PrimaryAppState, id: i64, user_id: i64) -> Result<()> {
    delete_in_scope(state, WorkspaceStorageScope::Personal { user_id }, id).await
}
