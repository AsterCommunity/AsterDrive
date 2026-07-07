//! 批量操作服务子模块：`copy`。

use std::{borrow::Cow, collections::HashSet};

use futures::{StreamExt, stream};

use crate::errors::{AsterError, Result};
use crate::runtime::PrimaryAppState;
use crate::services::{
    files::{file, folder},
    storage_change_service,
    workspace_storage_service::{self, WorkspaceStorageScope},
};

use super::shared::load_target_folder_in_scope;
use super::{
    BatchResult, NormalizedSelection, load_normalized_selection_in_scope, reserve_unique_name,
};

const BATCH_FOLDER_COPY_CONCURRENCY: usize = 4;

pub(crate) async fn batch_copy_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    file_ids: &[i64],
    folder_ids: &[i64],
    target_folder_id: Option<i64>,
) -> Result<BatchResult> {
    batch_copy_between_scopes(state, scope, scope, file_ids, folder_ids, target_folder_id).await
}

pub(crate) async fn batch_copy_between_scopes(
    state: &PrimaryAppState,
    source_scope: WorkspaceStorageScope,
    dest_scope: WorkspaceStorageScope,
    file_ids: &[i64],
    folder_ids: &[i64],
    target_folder_id: Option<i64>,
) -> Result<BatchResult> {
    let mut result = BatchResult::new();
    let NormalizedSelection {
        file_ids: normalized_file_ids,
        folder_ids: normalized_folder_ids,
        file_map,
        folder_map: _,
    } = load_normalized_selection_in_scope(state, source_scope, file_ids, folder_ids).await?;
    workspace_storage_service::require_scope_access(state, dest_scope).await?;
    let target_error = load_target_folder_in_scope(state, dest_scope, target_folder_id)
        .await
        .err();

    let mut reserved_file_names: HashSet<String> = if target_error.is_none() {
        workspace_storage_service::list_files_in_folder(state, dest_scope, target_folder_id)
            .await?
            .into_iter()
            .map(|file| file.name)
            .collect()
    } else {
        HashSet::new()
    };

    let (mut planned_storage_used, storage_quota) =
        workspace_storage_service::load_storage_limits(state, dest_scope).await?;
    let mut file_copy_specs = Vec::new();

    for &id in &normalized_file_ids {
        let Some(file) = file_map.get(&id) else {
            result.record_failure(
                "file",
                id,
                AsterError::file_not_found(format!("file #{id}")).to_string(),
            );
            continue;
        };
        if let Err(err) = workspace_storage_service::ensure_active_file_scope(file, source_scope) {
            result.record_failure("file", id, err.to_string());
            continue;
        }
        if let Some(error) = target_error.as_ref() {
            result.record_failure("file", id, error.clone());
            continue;
        }

        let projected_storage_used =
            planned_storage_used.checked_add(file.size).ok_or_else(|| {
                AsterError::internal_error("planned copied byte count overflow during batch copy")
            })?;
        if storage_quota > 0 && projected_storage_used > storage_quota {
            result.record_failure(
                "file",
                id,
                AsterError::storage_quota_exceeded(format!(
                    "quota {}, used {}, need {}",
                    storage_quota, planned_storage_used, file.size
                ))
                .to_string(),
            );
            continue;
        }

        let dest_name = reserve_unique_name(&mut reserved_file_names, &file.name);
        planned_storage_used = projected_storage_used;
        result.record_success();
        file_copy_specs.push(file::BatchDuplicateFileRecordSpec {
            src: file,
            dest_name: Cow::Owned(dest_name),
        });
    }

    if !file_copy_specs.is_empty() {
        let storage_delta = file_copy_specs.iter().try_fold(0i64, |acc, spec| {
            acc.checked_add(spec.src.size).ok_or_else(|| {
                AsterError::internal_error("copied byte count overflow during batch copy")
            })
        })?;
        let created_files = file::batch_duplicate_file_records_with_specs_in_scope(
            state,
            dest_scope,
            &file_copy_specs,
            target_folder_id,
        )
        .await?;
        storage_change_service::publish(
            state,
            storage_change_service::StorageChangeEvent::new(
                storage_change_service::StorageChangeKind::FileCreated,
                dest_scope,
                created_files.into_iter().map(|file| file.id).collect(),
                vec![],
                vec![target_folder_id],
            )
            .with_storage_delta(storage_delta),
        );
    }

    if let Some(error) = target_error.as_ref() {
        for &id in &normalized_folder_ids {
            result.record_failure("folder", id, error.clone());
        }
        return Ok(result);
    }

    let mut folder_copy_results =
        stream::iter(normalized_folder_ids.iter().copied().enumerate().map(
            |(index, id)| async move {
                let copy_result = folder::copy_folder_between_scopes(
                    state,
                    source_scope,
                    dest_scope,
                    id,
                    target_folder_id,
                )
                .await;
                (index, id, copy_result)
            },
        ))
        .buffer_unordered(BATCH_FOLDER_COPY_CONCURRENCY)
        .collect::<Vec<_>>()
        .await;
    folder_copy_results.sort_unstable_by_key(|(index, _, _)| *index);

    for (_, id, copy_result) in folder_copy_results {
        match copy_result {
            Ok(_) => result.record_success(),
            Err(e) => result.record_failure("folder", id, e.to_string()),
        }
    }

    Ok(result)
}

/// 批量复制（target_folder_id = None 表示复制到根目录）
///
/// 文件复制会先统一做权限/配额/命名预校验，再批量写入；
/// 文件夹复制仍复用高层递归 copy 流程以保持行为一致。
pub async fn batch_copy(
    state: &PrimaryAppState,
    user_id: i64,
    file_ids: &[i64],
    folder_ids: &[i64],
    target_folder_id: Option<i64>,
) -> Result<BatchResult> {
    batch_copy_in_scope(
        state,
        WorkspaceStorageScope::Personal { user_id },
        file_ids,
        folder_ids,
        target_folder_id,
    )
    .await
}

/// 团队空间批量复制（target_folder_id = None 表示复制到团队根目录）
pub async fn batch_copy_team(
    state: &PrimaryAppState,
    team_id: i64,
    user_id: i64,
    file_ids: &[i64],
    folder_ids: &[i64],
    target_folder_id: Option<i64>,
) -> Result<BatchResult> {
    batch_copy_in_scope(
        state,
        WorkspaceStorageScope::Team {
            team_id,
            actor_user_id: user_id,
        },
        file_ids,
        folder_ids,
        target_folder_id,
    )
    .await
}
