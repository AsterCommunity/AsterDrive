//! 回收站服务子模块：`restore`。

use aster_forge_db::transaction;
use sea_orm::{ActiveModelTrait, Set};

use crate::db::repository::{file_repo, folder_repo};
use crate::errors::Result;
use crate::runtime::PrimaryAppState;
use crate::runtime::StorageChangeRuntimeState;
use crate::services::files::folder::FolderTreeMutationDispatch;
use crate::services::task::types::FolderTreeMutationOperation;
use crate::services::{
    events::storage_change,
    files::folder as folder_ops,
    workspace::storage::{self, WorkspaceStorageScope},
};
use aster_drive_model::entities::{file, folder};

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

    let txn = transaction::begin(state.writer_db()).await?;
    let current = file_repo::lock_by_id(&txn, id).await?;
    storage::ensure_file_scope(&current, scope)?;
    if current.deleted_at.is_none() {
        transaction::commit(txn).await?;
        return Ok(());
    }
    let before = current.clone();
    let file_name = current.name.clone();
    let mut active: file::ActiveModel = current.into();
    if restore_to_root {
        active.folder_id = Set(None);
    }
    active.deleted_at = Set(None);
    let mut after = active
        .update(&txn)
        .await
        .map_err(|e| file_repo::map_name_db_err(e, &file_name))?;
    let ctx = crate::services::workspace::activity::ActivityContext::actor(
        &txn,
        scope.actor_user_id(),
        crate::services::workspace::activity::ActivitySource::Rest,
    )
    .await?;
    crate::services::workspace::activity::file_changed(
        &txn,
        &ctx,
        &before,
        &mut after,
        aster_drive_model::types::AuditAction::FileRestore,
    )
    .await?;
    transaction::commit(txn).await?;
    storage_change::publish(
        state,
        storage_change::StorageChangeEvent::new(
            storage_change::StorageChangeKind::FileRestoredFromTrash,
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
    traversal_limits: Option<folder_ops::FolderTreeTraversalLimits>,
) -> Result<()> {
    tracing::debug!(scope = ?scope, folder_id = id, "restoring folder from trash");
    let folder = verify_folder_in_trash_in_scope(state, scope, id).await?;
    let mut restored_parent_id = folder.parent_id;
    let mut restore_to_root = false;
    let (files, folder_ids) = folder_ops::collect_folder_tree_in_scope(
        state.writer_db(),
        scope,
        id,
        true,
        traversal_limits,
    )
    .await?;
    let child_folder_ids: Vec<i64> = folder_ids.into_iter().filter(|&fid| fid != id).collect();

    if let Some(parent_id) = folder.parent_id {
        let parent = folder_repo::find_by_id(state.writer_db(), parent_id).await;
        if parent_restore_target_unavailable(&parent, scope)? {
            restore_to_root = true;
            restored_parent_id = None;
        }
    }

    let txn = transaction::begin(state.writer_db()).await?;
    let root_before = folder.clone();
    let mut active: folder::ActiveModel = folder.into();
    let folder_name = active.name.clone().take().unwrap_or_default();
    if restore_to_root {
        active.parent_id = Set(None);
    }
    active.deleted_at = Set(None);
    let mut restored_root = active
        .update(&txn)
        .await
        .map_err(|err| folder_repo::map_name_db_err(err, &folder_name))?;
    let ctx = crate::services::workspace::activity::ActivityContext::actor(
        &txn,
        scope.actor_user_id(),
        crate::services::workspace::activity::ActivitySource::Rest,
    )
    .await?;
    crate::services::workspace::activity::folder_changed(
        &txn,
        &ctx,
        &root_before,
        &mut restored_root,
        aster_drive_model::types::AuditAction::FolderRestore,
    )
    .await?;
    for before in &files {
        if before.deleted_at.is_some() {
            let mut after = before.clone();
            after.deleted_at = None;
            crate::services::workspace::activity::file_changed(
                &txn,
                &ctx,
                before,
                &mut after,
                aster_drive_model::types::AuditAction::FileRestore,
            )
            .await?;
        }
    }
    for id in &child_folder_ids {
        let before = folder_repo::find_by_id(&txn, *id).await?;
        if before.deleted_at.is_some() {
            let mut after = before.clone();
            after.deleted_at = None;
            crate::services::workspace::activity::folder_changed(
                &txn,
                &ctx,
                &before,
                &mut after,
                aster_drive_model::types::AuditAction::FolderRestore,
            )
            .await?;
        }
    }
    let file_ids: Vec<i64> = files.iter().map(|file| file.id).collect();
    file_repo::restore_many(&txn, &file_ids).await?;
    folder_repo::restore_many(&txn, &child_folder_ids).await?;
    transaction::commit(txn).await?;
    storage_change::publish(
        state,
        storage_change::StorageChangeEvent::new(
            storage_change::StorageChangeKind::FolderRestoredFromTrash,
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

pub(crate) async fn restore_folder_in_scope_with_dispatch(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    id: i64,
) -> Result<FolderTreeMutationDispatch> {
    match restore_folder_in_scope(state, scope, id, Some(folder_ops::REST_FOLDER_TREE_LIMITS)).await
    {
        Ok(()) => Ok(FolderTreeMutationDispatch::Completed),
        Err(crate::errors::AsterError::OperationResourceLimitExceeded(_)) => {
            Ok(FolderTreeMutationDispatch::Queued(Box::new(
                crate::services::task::folder_tree::create_folder_tree_mutation_task_in_scope(
                    state,
                    scope,
                    id,
                    FolderTreeMutationOperation::Restore,
                )
                .await?,
            )))
        }
        Err(error) => Err(error),
    }
}
