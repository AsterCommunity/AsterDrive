use std::collections::BTreeMap;

use aster_forge_db::transaction;
use futures::{StreamExt, stream};

use crate::db::repository::{file_repo, share_repo};
use crate::entities::file;
use crate::errors::{AsterError, Result};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::{
    events::storage_change,
    share,
    workspace::storage::{self, WorkspaceResourceScope, WorkspaceStorageScope},
};
use aster_forge_utils::numbers::{i64_to_i32, usize_to_u32};

use super::blob_cleanup::ensure_blob_cleanup_if_unreferenced;

const BLOB_CLEANUP_CONCURRENCY: usize = 8;

#[derive(Debug, Default)]
pub(crate) struct BatchPurgeSummary {
    pub purged: u32,
    pub file_ids: Vec<i64>,
    pub freed_bytes: i64,
}

pub(crate) async fn purge_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    id: i64,
) -> Result<()> {
    storage::require_scope_access_with_db(state, state.writer_db(), scope).await?;

    let file = file_repo::find_by_id(state.writer_db(), id).await?;
    storage::ensure_file_scope(&file, scope)?;

    batch_purge_in_scope(state, scope, vec![file]).await?;
    Ok(())
}

/// 永久删除文件，处理 blob ref_count、物理文件、缩略图和配额。
pub async fn purge(state: &PrimaryAppState, id: i64, user_id: i64) -> Result<()> {
    purge_in_scope(state, WorkspaceStorageScope::Personal { user_id }, id).await
}

/// 批量永久删除文件：一次事务处理所有 DB 操作，事务后并行清理物理文件
///
/// 比逐个调 `purge()` 快得多——N 个文件只需 ~10 次 DB 查询而非 ~12N 次。
pub(crate) async fn batch_purge_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    files: Vec<file::Model>,
) -> Result<u32> {
    batch_purge_in_resource_scope(state, scope.into(), files).await
}

pub(crate) async fn batch_purge_in_resource_scope(
    state: &PrimaryAppState,
    scope: WorkspaceResourceScope,
    files: Vec<file::Model>,
) -> Result<u32> {
    Ok(
        batch_purge_in_resource_scope_internal(state, scope, files, true)
            .await?
            .purged,
    )
}

pub(crate) async fn batch_purge_in_resource_scope_silent(
    state: &PrimaryAppState,
    scope: WorkspaceResourceScope,
    files: Vec<file::Model>,
) -> Result<BatchPurgeSummary> {
    batch_purge_in_resource_scope_internal(state, scope, files, false).await
}

async fn batch_purge_in_resource_scope_internal(
    state: &PrimaryAppState,
    scope: WorkspaceResourceScope,
    files: Vec<file::Model>,
    emit_storage_event: bool,
) -> Result<BatchPurgeSummary> {
    if files.is_empty() {
        return Ok(BatchPurgeSummary::default());
    }

    let input_count = files.len();
    tracing::debug!(
        scope = ?scope,
        file_count = input_count,
        "purging files permanently"
    );

    for file in &files {
        storage::ensure_file_resource_scope(file, scope)?;
    }

    let file_ids: Vec<i64> = files.iter().map(|file| file.id).collect();
    let parent_ids: Vec<Option<i64>> = files.iter().map(|file| file.folder_id).collect();
    let blob_ids: Vec<i64> = files.iter().map(|file| file.blob_id).collect();
    let count = usize_to_u32(files.len(), "purged file count")?;

    let txn = transaction::begin(state.writer_db()).await?;
    storage::lock_storage_usage_for_resource_scope(&txn, scope).await?;

    let version_blob_ids =
        crate::db::repository::version_repo::delete_all_by_file_ids(&txn, &file_ids).await?;
    let version_blob_count = version_blob_ids.len();

    crate::db::repository::property_repo::delete_all_for_entities(
        &txn,
        crate::types::EntityType::File,
        &file_ids,
    )
    .await?;

    let deleted_shares = share_repo::delete_by_file_ids(&txn, &file_ids).await?;
    file_repo::delete_many(&txn, &file_ids).await?;

    let mut blob_decrements = BTreeMap::<i64, i64>::new();
    for &blob_id in &blob_ids {
        *blob_decrements.entry(blob_id).or_default() += 1;
    }
    for &version_blob_id in &version_blob_ids {
        *blob_decrements.entry(version_blob_id).or_default() += 1;
    }

    let blob_ids: Vec<i64> = blob_decrements.keys().copied().collect();
    let blobs_by_id = file_repo::find_blobs_by_ids(&txn, &blob_ids).await?;
    let mut total_freed_bytes = 0i64;
    let mut ref_count_decrements = Vec::with_capacity(blob_decrements.len());

    for (&blob_id, &decrement) in &blob_decrements {
        if let Some(blob) = blobs_by_id.get(&blob_id) {
            let freed_bytes = blob.size.checked_mul(decrement).ok_or_else(|| {
                AsterError::internal_error(format!(
                    "freed byte count overflow for blob {blob_id} during batch purge"
                ))
            })?;
            total_freed_bytes = total_freed_bytes.checked_add(freed_bytes).ok_or_else(|| {
                AsterError::internal_error("total freed byte count overflow during batch purge")
            })?;
            ref_count_decrements.push((
                blob_id,
                i64_to_i32(decrement, "blob decrement during batch purge")?,
            ));
        }
    }
    file_repo::decrement_blob_ref_counts_by(&txn, &ref_count_decrements).await?;

    storage::update_storage_used_for_resource_scope(&txn, scope, -total_freed_bytes).await?;

    transaction::commit(txn).await?;
    if emit_storage_event {
        storage_change::publish(
            state,
            storage_change::StorageChangeEvent::new_for_resource_scope(
                storage_change::StorageChangeKind::FilePurged,
                scope,
                file_ids.clone(),
                vec![],
                parent_ids.clone(),
            )
            .with_storage_delta(-total_freed_bytes),
        );
    }
    if deleted_shares > 0 {
        share::invalidate_active_share_target_cache_for_resource_scope(state, scope).await;
        share::invalidate_all_share_token_record_cache(state).await;
    }

    stream::iter(blob_ids.iter().copied())
        .for_each_concurrent(BLOB_CLEANUP_CONCURRENCY, |blob_id| async move {
            if !ensure_blob_cleanup_if_unreferenced(state, blob_id).await {
                tracing::warn!(
                    blob_id,
                    "batch purge left blob row for retry because object cleanup was incomplete"
                );
            }
        })
        .await;

    tracing::debug!(
        scope = ?scope,
        file_count = input_count,
        freed_bytes = total_freed_bytes,
        version_blob_count,
        deleted_shares,
        cleanup_blob_count = blob_ids.len(),
        "purged files permanently"
    );
    Ok(BatchPurgeSummary {
        purged: count,
        file_ids,
        freed_bytes: total_freed_bytes,
    })
}

pub async fn batch_purge(
    state: &PrimaryAppState,
    files: Vec<file::Model>,
    user_id: i64,
) -> Result<u32> {
    batch_purge_in_scope(state, WorkspaceStorageScope::Personal { user_id }, files).await
}
