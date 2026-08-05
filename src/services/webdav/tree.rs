//! 服务模块：`webdav::tree`。

use crate::db::repository::{folder_repo, share_repo};
use crate::errors::Result;
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::{
    files::{file, folder as folder_ops},
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
) -> Result<(Vec<aster_drive_model::entities::file::Model>, Vec<i64>)> {
    folder_ops::collect_folder_tree_in_scope(
        db,
        WorkspaceStorageScope::Personal { user_id },
        folder_id,
        include_deleted,
        None,
    )
    .await
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
        aster_drive_model::types::EntityType::Folder,
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
