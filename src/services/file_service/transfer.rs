//! 文件服务子模块：`transfer`。

use std::collections::BTreeMap;

use chrono::Utc;
use sea_orm::{ActiveModelTrait, Set};

use crate::db::repository::file_repo;
use crate::entities::file;
use crate::errors::{AsterError, Result};
use crate::runtime::PrimaryAppState;
use crate::services::{
    storage_change_service,
    workspace_models::FileInfo,
    workspace_storage_service::{self, WorkspaceStorageScope, load_scope_actor_username},
};

use super::get_info_in_scope;

const MAX_COPY_NAME_RETRIES: usize = 32;

fn collect_blob_ref_count_increments(
    blob_ids: impl IntoIterator<Item = i64>,
    context: &str,
) -> Result<Vec<(i64, i32)>> {
    let mut counts = BTreeMap::<i64, i32>::new();
    for blob_id in blob_ids {
        let entry = counts.entry(blob_id).or_default();
        *entry = entry.checked_add(1).ok_or_else(|| {
            AsterError::internal_error(format!(
                "blob copy count overflow for blob {blob_id} during {context}"
            ))
        })?;
    }
    Ok(counts.into_iter().collect())
}

pub(crate) async fn copy_file_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    src_id: i64,
    dest_folder_id: Option<i64>,
) -> Result<file::Model> {
    let db = &state.db;
    tracing::debug!(
        scope = ?scope,
        src_file_id = src_id,
        dest_folder_id,
        "copying file"
    );
    let src = get_info_in_scope(state, scope, src_id).await?;

    if let Some(folder_id) = dest_folder_id {
        workspace_storage_service::verify_folder_access(state, scope, folder_id).await?;
    }

    let blob = file_repo::find_blob_by_id(db, src.blob_id).await?;
    workspace_storage_service::check_quota(db, scope, blob.size).await?;

    let copy_name = match scope {
        WorkspaceStorageScope::Personal { user_id } => {
            file_repo::resolve_unique_filename(db, user_id, dest_folder_id, &src.name).await?
        }
        WorkspaceStorageScope::Team { team_id, .. } => {
            file_repo::resolve_unique_team_filename(db, team_id, dest_folder_id, &src.name).await?
        }
    };

    let mut copied = None;
    let mut candidate_name = copy_name;
    for _ in 0..MAX_COPY_NAME_RETRIES {
        match duplicate_file_record_in_scope(state, scope, &src, dest_folder_id, &candidate_name)
            .await
        {
            Ok(file) => {
                copied = Some(file);
                break;
            }
            Err(err) if file_repo::is_duplicate_name_error(&err, &candidate_name) => {
                candidate_name = crate::utils::next_copy_name(&candidate_name);
            }
            Err(err) => return Err(err),
        }
    }
    let copied = copied.ok_or_else(|| {
        AsterError::validation_error(format!(
            "failed to allocate a unique copy name for '{}'",
            src.name
        ))
    })?;
    storage_change_service::publish(
        state,
        storage_change_service::StorageChangeEvent::new(
            storage_change_service::StorageChangeKind::FileCreated,
            scope,
            vec![copied.id],
            vec![],
            vec![copied.folder_id],
        ),
    );
    tracing::debug!(
        scope = ?scope,
        src_file_id = src_id,
        copied_file_id = copied.id,
        dest_folder_id = copied.folder_id,
        "copied file"
    );
    Ok(copied)
}

/// 复制文件（REST API 入口，带权限检查 + 副本命名）
///
/// `dest_folder_id = None` 表示复制到根目录。
pub async fn copy_file(
    state: &PrimaryAppState,
    src_id: i64,
    user_id: i64,
    dest_folder_id: Option<i64>,
) -> Result<FileInfo> {
    copy_file_in_scope(
        state,
        WorkspaceStorageScope::Personal { user_id },
        src_id,
        dest_folder_id,
    )
    .await
    .map(Into::into)
}

#[derive(Clone)]
pub(crate) struct BatchDuplicateFileRecordSpec {
    pub src: file::Model,
    pub dest_name: String,
}

#[derive(Clone)]
pub(crate) struct BatchDuplicateFileRecordTargetSpec {
    pub src: file::Model,
    pub dest_name: String,
    pub dest_folder_id: Option<i64>,
}

async fn batch_duplicate_file_records_with_specs_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    copy_specs: &[BatchDuplicateFileRecordSpec],
    dest_folder_id: Option<i64>,
) -> Result<Vec<file::Model>> {
    if copy_specs.is_empty() {
        return Ok(vec![]);
    }

    let total_size = copy_specs.iter().try_fold(0i64, |acc, spec| {
        acc.checked_add(spec.src.size).ok_or_else(|| {
            AsterError::internal_error("total copied byte count overflow during batch copy")
        })
    })?;
    let now = chrono::Utc::now();

    let txn = crate::db::transaction::begin(&state.db).await?;
    let created_by_username = load_scope_actor_username(&txn, scope).await?;

    // 原子性地增加配额（CAS 语义：如果 quota > 0 且 used + total_size > quota，则失败）
    // 这避免了并发场景下的 TOCTOU 问题
    workspace_storage_service::update_storage_used(&txn, scope, total_size).await?;

    let blob_counts = collect_blob_ref_count_increments(
        copy_specs.iter().map(|spec| spec.src.blob_id),
        "batch copy",
    )?;
    file_repo::increment_blob_ref_counts_by(&txn, &blob_counts).await?;

    let models: Vec<file::ActiveModel> = copy_specs
        .iter()
        .map(|spec| file::ActiveModel {
            name: Set(spec.dest_name.clone()),
            folder_id: Set(dest_folder_id),
            team_id: Set(scope.team_id()),
            blob_id: Set(spec.src.blob_id),
            size: Set(spec.src.size),
            owner_user_id: Set(scope.owner_user_id()),
            created_by_user_id: Set(Some(scope.actor_user_id())),
            created_by_username: Set(created_by_username.clone()),
            mime_type: Set(spec.src.mime_type.clone()),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        })
        .collect();
    file_repo::create_many(&txn, models).await?;

    let dest_names: Vec<String> = copy_specs
        .iter()
        .map(|spec| spec.dest_name.clone())
        .collect();
    let created_files = match scope {
        WorkspaceStorageScope::Personal { user_id } => {
            file_repo::find_by_names_in_folder(&txn, user_id, dest_folder_id, &dest_names).await?
        }
        WorkspaceStorageScope::Team { team_id, .. } => {
            file_repo::find_by_names_in_team_folder(&txn, team_id, dest_folder_id, &dest_names)
                .await?
        }
    };
    if created_files.len() != copy_specs.len() {
        return Err(AsterError::internal_error(
            "failed to load all copied files after batch insert",
        ));
    }

    crate::db::transaction::commit(txn).await?;
    Ok(created_files)
}

pub(crate) async fn duplicate_file_record_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    src: &file::Model,
    dest_folder_id: Option<i64>,
    dest_name: &str,
) -> Result<file::Model> {
    let blob = file_repo::find_blob_by_id(&state.db, src.blob_id).await?;
    let now = Utc::now();
    let blob_size = blob.size;

    let txn = crate::db::transaction::begin(&state.db).await?;
    let created_by_username = load_scope_actor_username(&txn, scope).await?;
    workspace_storage_service::check_quota(&txn, scope, blob_size).await?;

    file_repo::increment_blob_ref_count(&txn, blob.id).await?;

    let new_file = file::ActiveModel {
        name: Set(dest_name.to_string()),
        folder_id: Set(dest_folder_id),
        team_id: Set(scope.team_id()),
        blob_id: Set(src.blob_id),
        size: Set(src.size),
        owner_user_id: Set(scope.owner_user_id()),
        created_by_user_id: Set(Some(scope.actor_user_id())),
        created_by_username: Set(created_by_username),
        mime_type: Set(src.mime_type.clone()),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&txn)
    .await
    .map_err(|err| file_repo::map_name_db_err(err, dest_name))?;

    workspace_storage_service::update_storage_used(&txn, scope, blob_size).await?;

    crate::db::transaction::commit(txn).await?;

    Ok(new_file)
}

/// 复制文件记录的核心逻辑（blob ref_count++ + 新文件记录 + 配额更新）
///
/// 无权限检查，供底层复制流程复用。
pub async fn duplicate_file_record(
    state: &PrimaryAppState,
    src: &file::Model,
    dest_folder_id: Option<i64>,
    dest_name: &str,
) -> Result<FileInfo> {
    let copied = duplicate_file_record_in_scope(
        state,
        WorkspaceStorageScope::Personal {
            user_id: src
                .owner_user_id
                .ok_or_else(|| AsterError::auth_forbidden("source file has no personal owner"))?,
        },
        src,
        dest_folder_id,
        dest_name,
    )
    .await?;
    storage_change_service::publish(
        state,
        storage_change_service::StorageChangeEvent::new(
            storage_change_service::StorageChangeKind::FileCreated,
            WorkspaceStorageScope::Personal {
                user_id: src.owner_user_id.ok_or_else(|| {
                    AsterError::auth_forbidden("source file has no personal owner")
                })?,
            },
            vec![copied.id],
            vec![],
            vec![copied.folder_id],
        ),
    );
    Ok(copied.into())
}

pub(crate) async fn batch_duplicate_file_records_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    src_files: &[file::Model],
    dest_folder_id: Option<i64>,
) -> Result<Vec<file::Model>> {
    let copy_specs: Vec<BatchDuplicateFileRecordSpec> = src_files
        .iter()
        .cloned()
        .map(|src| BatchDuplicateFileRecordSpec {
            dest_name: src.name.clone(),
            src,
        })
        .collect();

    batch_duplicate_file_records_with_specs_in_scope(state, scope, &copy_specs, dest_folder_id)
        .await
}

pub(crate) async fn batch_duplicate_file_records_with_names_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    copy_specs: &[BatchDuplicateFileRecordSpec],
    dest_folder_id: Option<i64>,
) -> Result<Vec<file::Model>> {
    batch_duplicate_file_records_with_specs_in_scope(state, scope, copy_specs, dest_folder_id).await
}

pub(crate) async fn batch_duplicate_file_records_to_mixed_folders_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    copy_specs: &[BatchDuplicateFileRecordTargetSpec],
) -> Result<()> {
    if copy_specs.is_empty() {
        return Ok(());
    }

    let total_size = copy_specs.iter().try_fold(0i64, |acc, spec| {
        acc.checked_add(spec.src.size).ok_or_else(|| {
            AsterError::internal_error("total copied byte count overflow during folder copy")
        })
    })?;
    let now = chrono::Utc::now();

    workspace_storage_service::check_quota(&state.db, scope, total_size).await?;

    let txn = crate::db::transaction::begin(&state.db).await?;
    let created_by_username = load_scope_actor_username(&txn, scope).await?;
    workspace_storage_service::check_quota(&txn, scope, total_size).await?;

    let blob_counts = collect_blob_ref_count_increments(
        copy_specs.iter().map(|spec| spec.src.blob_id),
        "folder copy",
    )?;
    file_repo::increment_blob_ref_counts_by(&txn, &blob_counts).await?;

    let models: Vec<file::ActiveModel> = copy_specs
        .iter()
        .map(|spec| file::ActiveModel {
            name: Set(spec.dest_name.clone()),
            folder_id: Set(spec.dest_folder_id),
            team_id: Set(scope.team_id()),
            blob_id: Set(spec.src.blob_id),
            size: Set(spec.src.size),
            owner_user_id: Set(scope.owner_user_id()),
            created_by_user_id: Set(Some(scope.actor_user_id())),
            created_by_username: Set(created_by_username.clone()),
            mime_type: Set(spec.src.mime_type.clone()),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        })
        .collect();
    file_repo::create_many(&txn, models).await?;

    workspace_storage_service::update_storage_used(&txn, scope, total_size).await?;

    crate::db::transaction::commit(txn).await?;
    Ok(())
}

/// 批量复制文件记录：一次事务处理 blob ref_count + 文件创建 + 配额
///
/// 与 `duplicate_file_record` 的区别：N 个文件只开 1 次事务，
/// blob ref_count 按 blob_id 合并递增，配额只更新一次。
/// 不返回创建的 Model（递归复制场景不需要）。
pub async fn batch_duplicate_file_records(
    state: &PrimaryAppState,
    src_files: &[file::Model],
    dest_folder_id: Option<i64>,
) -> Result<Vec<FileInfo>> {
    if src_files.is_empty() {
        return Ok(vec![]);
    }

    batch_duplicate_file_records_in_scope(
        state,
        WorkspaceStorageScope::Personal {
            user_id: src_files[0]
                .owner_user_id
                .ok_or_else(|| AsterError::auth_forbidden("source file has no personal owner"))?,
        },
        src_files,
        dest_folder_id,
    )
    .await
    .map(|files| files.into_iter().map(Into::into).collect())
}
