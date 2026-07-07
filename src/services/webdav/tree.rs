//! 服务模块：`webdav::tree`。

use chrono::Utc;

use crate::db::repository::{file_repo, folder_repo, share_repo};
use crate::entities::folder;
use crate::errors::Result;
use crate::runtime::{PrimaryAppState, SharedRuntimeState, StorageChangeRuntimeState};
use crate::services::{
    files::{file, folder as folder_ops},
    storage_change_service,
    workspace::models::FileInfo,
    workspace::storage::WorkspaceStorageScope,
};

/// 递归收集文件夹树内的所有文件和子文件夹 ID
///
/// - `include_deleted = true`：收集全部（含已软删除），用于 purge
/// - `include_deleted = false`：只收集未删除项，用于 soft_delete
async fn collect_folder_tree_models(
    db: &sea_orm::DatabaseConnection,
    user_id: i64,
    folder_id: i64,
    include_deleted: bool,
) -> Result<(Vec<crate::entities::file::Model>, Vec<i64>)> {
    folder_ops::collect_folder_tree_in_scope(
        db,
        WorkspaceStorageScope::Personal { user_id },
        folder_id,
        include_deleted,
    )
    .await
}

async fn collect_folder_tree_models_in_scope(
    db: &sea_orm::DatabaseConnection,
    scope: WorkspaceStorageScope,
    folder_id: i64,
    include_deleted: bool,
) -> Result<(Vec<crate::entities::file::Model>, Vec<i64>)> {
    folder_ops::collect_folder_tree_in_scope(db, scope, folder_id, include_deleted).await
}

pub async fn collect_folder_tree(
    state: &impl SharedRuntimeState,
    user_id: i64,
    folder_id: i64,
    include_deleted: bool,
) -> Result<(Vec<FileInfo>, Vec<i64>)> {
    collect_folder_tree_models(state.writer_db(), user_id, folder_id, include_deleted)
        .await
        .map(|(files, folder_ids)| (files.into_iter().map(FileInfo::from).collect(), folder_ids))
}

/// 递归软删除文件夹及其所有内容（→ 回收站）
///
/// 先收集所有未删除的文件和文件夹 ID，再一次事务内批量 soft_delete。
pub async fn recursive_soft_delete(
    state: &impl StorageChangeRuntimeState,
    user_id: i64,
    folder_id: i64,
) -> Result<()> {
    recursive_soft_delete_in_scope(
        state,
        WorkspaceStorageScope::Personal { user_id },
        folder_id,
    )
    .await
}

pub(crate) async fn recursive_soft_delete_in_scope(
    state: &impl StorageChangeRuntimeState,
    scope: WorkspaceStorageScope,
    folder_id: i64,
) -> Result<()> {
    tracing::debug!(?scope, folder_id, "webdav soft deleting folder tree");
    let folder = folder_repo::find_by_id(state.writer_db(), folder_id).await?;
    let (files, folder_ids) =
        collect_folder_tree_models_in_scope(state.writer_db(), scope, folder_id, false).await?;

    let file_ids: Vec<i64> = files.into_iter().map(|f| f.id).collect();
    let file_count = file_ids.len();
    let folder_count = folder_ids.len();
    let now = Utc::now();

    let txn = crate::db::transaction::begin(state.writer_db()).await?;
    file_repo::soft_delete_many(&txn, &file_ids, now).await?;
    folder_repo::soft_delete_many(&txn, &folder_ids, now).await?;
    crate::db::transaction::commit(txn).await?;
    storage_change_service::publish(
        state,
        storage_change_service::StorageChangeEvent::new(
            storage_change_service::StorageChangeKind::FolderTrashed,
            scope,
            vec![],
            vec![folder.id],
            vec![folder.parent_id],
        ),
    );
    tracing::debug!(
        ?scope,
        folder_id,
        file_count,
        folder_count,
        "webdav soft deleted folder tree"
    );

    Ok(())
}

/// 永久删除文件夹树及其所有内容（批量优化版）
///
/// 先收集所有文件和文件夹 ID（含已删除），然后一次 batch_purge 处理所有文件，
/// 再批量删除文件夹记录和属性。比逐个 purge 快得多。
pub async fn purge_folder_tree(
    state: &PrimaryAppState,
    user_id: i64,
    folder_id: i64,
) -> Result<()> {
    tracing::debug!(user_id, folder_id, "webdav purging folder tree");
    let (all_files, all_folder_ids) =
        collect_folder_tree_models(state.writer_db(), user_id, folder_id, true).await?;
    let file_count = all_files.len();
    let folder_count = all_folder_ids.len();

    file::batch_purge_in_scope(
        state,
        WorkspaceStorageScope::Personal { user_id },
        all_files,
    )
    .await?;

    crate::db::repository::property_repo::delete_all_for_entities(
        state.writer_db(),
        crate::types::EntityType::Folder,
        &all_folder_ids,
    )
    .await?;

    let deleted_shares =
        share_repo::delete_by_folder_ids(state.writer_db(), &all_folder_ids).await?;
    if deleted_shares > 0 {
        crate::services::share::invalidate_active_share_target_cache_for_scope(
            state,
            WorkspaceStorageScope::Personal { user_id },
        )
        .await;
        crate::services::share::invalidate_all_share_token_record_cache(state).await;
    }
    crate::services::files::folder::invalidate_folder_path_cache_for_ids(state, &all_folder_ids)
        .await;
    folder_repo::delete_many(state.writer_db(), &all_folder_ids).await?;
    tracing::debug!(
        user_id,
        folder_id,
        file_count,
        folder_count,
        deleted_shares,
        "webdav purged folder tree"
    );

    Ok(())
}

/// 复制文件夹树及其所有内容到新位置
///
/// 利用 blob 去重：只增加 ref_count，不复制物理数据
pub async fn copy_folder_tree(
    state: &PrimaryAppState,
    user_id: i64,
    src_folder_id: i64,
    dest_parent_id: Option<i64>,
    dest_name: &str,
) -> Result<folder::Model> {
    copy_folder_tree_in_scope(
        state,
        crate::services::workspace::storage::WorkspaceStorageScope::Personal { user_id },
        src_folder_id,
        dest_parent_id,
        dest_name,
    )
    .await
}

pub(crate) async fn copy_folder_tree_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    src_folder_id: i64,
    dest_parent_id: Option<i64>,
    dest_name: &str,
) -> Result<folder::Model> {
    let (copied, storage_delta) = crate::services::files::folder::copy_folder_tree_in_scope(
        state,
        scope,
        src_folder_id,
        dest_parent_id,
        dest_name,
    )
    .await?;
    storage_change_service::publish(
        state,
        storage_change_service::StorageChangeEvent::new(
            storage_change_service::StorageChangeKind::FolderCreated,
            scope,
            vec![],
            vec![copied.id],
            vec![copied.parent_id],
        )
        .with_storage_delta(storage_delta),
    );
    Ok(copied)
}
