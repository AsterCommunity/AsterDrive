//! 文件服务子模块：`lock`。

use crate::entities::file;
use crate::errors::Result;
use crate::runtime::StorageChangeRuntimeState;
use crate::services::{
    lock_service, storage_change_service, workspace::models::FileInfo,
    workspace::storage::WorkspaceStorageScope,
};
use crate::types::EntityType;

pub(crate) async fn set_lock_in_scope(
    state: &impl StorageChangeRuntimeState,
    scope: WorkspaceStorageScope,
    file_id: i64,
    locked: bool,
) -> Result<file::Model> {
    tracing::debug!(
        scope = ?scope,
        file_id,
        locked,
        "setting file lock state"
    );
    crate::services::workspace::storage::verify_file_access(state, scope, file_id).await?;

    if locked {
        lock_service::lock(
            state,
            EntityType::File,
            file_id,
            Some(scope.actor_user_id()),
            None,
            None,
        )
        .await?;
    } else {
        lock_service::unlock(state, EntityType::File, file_id, scope.actor_user_id()).await?;
    }

    let file =
        crate::services::workspace::storage::verify_file_access(state, scope, file_id)
            .await?;
    publish_file_lock_change(state, scope, &file, locked).await?;
    tracing::debug!(
        scope = ?scope,
        file_id = file.id,
        locked = file.is_locked,
        "updated file lock state"
    );
    Ok(file)
}

/// 设置/解除文件锁，返回更新后的文件信息
pub async fn set_lock(
    state: &impl StorageChangeRuntimeState,
    file_id: i64,
    user_id: i64,
    locked: bool,
) -> Result<FileInfo> {
    set_lock_in_scope(
        state,
        WorkspaceStorageScope::Personal { user_id },
        file_id,
        locked,
    )
    .await
    .map(Into::into)
}

async fn publish_file_lock_change(
    state: &impl StorageChangeRuntimeState,
    scope: WorkspaceStorageScope,
    file: &file::Model,
    locked: bool,
) -> Result<()> {
    storage_change_service::publish(
        state,
        storage_change_service::StorageChangeEvent::new(
            if locked {
                storage_change_service::StorageChangeKind::LockCreated
            } else {
                storage_change_service::StorageChangeKind::LockDeleted
            },
            scope,
            vec![file.id],
            vec![],
            vec![file.folder_id],
        ),
    );
    Ok(())
}
