//! 回收站服务子模块：`restore`。

use sea_orm::{ActiveModelTrait, Set};

use crate::db::repository::{file_repo, folder_repo};
use crate::entities::{file, folder};
use crate::errors::Result;
use crate::runtime::StorageChangeRuntimeState;
use crate::services::{
    files::folder as folder_ops, storage_change_service,
    workspace_storage_service::WorkspaceStorageScope,
};

use super::common::{
    parent_restore_target_unavailable, verify_file_in_trash_in_scope,
    verify_folder_in_trash_in_scope,
};

async fn restore_file_in_scope(
    state: &impl StorageChangeRuntimeState,
    scope: WorkspaceStorageScope,
    id: i64,
) -> Result<()> {
    tracing::debug!(scope = ?scope, file_id = id, "restoring file from trash");
    let file = verify_file_in_trash_in_scope(state, scope, id).await?;
    let mut restored_parent_id = file.folder_id;
    let mut restore_to_root = false;

    if let Some(folder_id) = file.folder_id {
        let parent = folder_repo::find_by_id(state.writer_db(), folder_id).await;
        if parent_restore_target_unavailable(&parent, scope)? {
            restored_parent_id = None;
            restore_to_root = true;
        }
    }

    if restore_to_root {
        let txn = crate::db::transaction::begin(state.writer_db()).await?;
        let file_name = file.name.clone();
        let mut active: file::ActiveModel = file.into();
        active.folder_id = Set(None);
        active.deleted_at = Set(None);
        active
            .update(&txn)
            .await
            .map_err(|err| file_repo::map_name_db_err(err, &file_name))?;
        crate::db::transaction::commit(txn).await?;
    } else {
        file_repo::restore(state.writer_db(), id).await?;
    }
    storage_change_service::publish(
        state,
        storage_change_service::StorageChangeEvent::new(
            storage_change_service::StorageChangeKind::FileRestoredFromTrash,
            scope,
            vec![id],
            vec![],
            vec![restored_parent_id],
        ),
    );
    tracing::debug!(
        scope = ?scope,
        file_id = id,
        restored_parent_id,
        restored_to_root = restored_parent_id.is_none(),
        "restored file from trash"
    );
    Ok(())
}

async fn restore_folder_in_scope(
    state: &impl StorageChangeRuntimeState,
    scope: WorkspaceStorageScope,
    id: i64,
) -> Result<()> {
    tracing::debug!(scope = ?scope, folder_id = id, "restoring folder from trash");
    let folder = verify_folder_in_trash_in_scope(state, scope, id).await?;
    let mut restored_parent_id = folder.parent_id;
    let mut restore_to_root = false;
    let (files, folder_ids) =
        folder_ops::collect_folder_tree_in_scope(state.writer_db(), scope, id, true).await?;
    let child_folder_ids: Vec<i64> = folder_ids.into_iter().filter(|&fid| fid != id).collect();

    if let Some(parent_id) = folder.parent_id {
        let parent = folder_repo::find_by_id(state.writer_db(), parent_id).await;
        if parent_restore_target_unavailable(&parent, scope)? {
            restore_to_root = true;
            restored_parent_id = None;
        }
    }

    let txn = crate::db::transaction::begin(state.writer_db()).await?;
    let mut active: folder::ActiveModel = folder.into();
    let folder_name = active.name.clone().take().unwrap_or_default();
    if restore_to_root {
        active.parent_id = Set(None);
    }
    active.deleted_at = Set(None);
    active
        .update(&txn)
        .await
        .map_err(|err| folder_repo::map_name_db_err(err, &folder_name))?;
    let file_ids: Vec<i64> = files.iter().map(|file| file.id).collect();
    file_repo::restore_many(&txn, &file_ids).await?;
    folder_repo::restore_many(&txn, &child_folder_ids).await?;
    crate::db::transaction::commit(txn).await?;
    storage_change_service::publish(
        state,
        storage_change_service::StorageChangeEvent::new(
            storage_change_service::StorageChangeKind::FolderRestoredFromTrash,
            scope,
            vec![],
            vec![id],
            vec![restored_parent_id],
        ),
    );
    tracing::debug!(
        scope = ?scope,
        folder_id = id,
        restored_parent_id,
        restored_to_root = restored_parent_id.is_none(),
        "restored folder from trash"
    );
    Ok(())
}

/// 恢复文件
pub async fn restore_file(
    state: &impl StorageChangeRuntimeState,
    id: i64,
    user_id: i64,
) -> Result<()> {
    restore_file_in_scope(state, WorkspaceStorageScope::Personal { user_id }, id).await
}

pub async fn restore_team_file(
    state: &impl StorageChangeRuntimeState,
    team_id: i64,
    id: i64,
    user_id: i64,
) -> Result<()> {
    restore_file_in_scope(
        state,
        WorkspaceStorageScope::Team {
            team_id,
            actor_user_id: user_id,
        },
        id,
    )
    .await
}

/// 恢复文件夹（递归恢复子项）
pub async fn restore_folder(
    state: &impl StorageChangeRuntimeState,
    id: i64,
    user_id: i64,
) -> Result<()> {
    restore_folder_in_scope(state, WorkspaceStorageScope::Personal { user_id }, id).await
}

pub async fn restore_team_folder(
    state: &impl StorageChangeRuntimeState,
    team_id: i64,
    id: i64,
    user_id: i64,
) -> Result<()> {
    restore_folder_in_scope(
        state,
        WorkspaceStorageScope::Team {
            team_id,
            actor_user_id: user_id,
        },
        id,
    )
    .await
}
