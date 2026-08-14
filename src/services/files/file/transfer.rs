//! 文件服务子模块：`transfer`。

use aster_forge_db::transaction;
use std::{
    borrow::Cow,
    collections::{BTreeMap, HashMap},
};

use chrono::Utc;
use sea_orm::{ActiveModelTrait, ConnectionTrait, Set};

use crate::db::repository::{file_repo, property_repo};
use crate::errors::{AsterError, Result};
use crate::runtime::PrimaryAppState;
use crate::services::{
    events::storage_change,
    workspace::models::FileInfo,
    workspace::storage::{self, WorkspaceStorageScope, load_scope_actor_username},
};
use aster_drive_model::{entities::file, types::EntityType};

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
    let db = state.writer_db();
    tracing::debug!(
        scope = ?scope,
        src_file_id = src_id,
        dest_folder_id,
        "copying file"
    );
    let src = storage::verify_file_access(state, scope, src_id).await?;

    if let Some(folder_id) = dest_folder_id {
        storage::verify_folder_access(state, scope, folder_id).await?;
    }

    let blob = file_repo::find_blob_by_id(db, src.blob_id).await?;
    storage::check_quota(db, scope, blob.size).await?;

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
                candidate_name = aster_forge_validation::filename::next_copy_name(&candidate_name);
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
    storage_change::publish(
        state,
        storage_change::StorageChangeEvent::new(
            storage_change::StorageChangeKind::FileCreated,
            scope,
            vec![copied.id],
            vec![],
            vec![copied.folder_id],
        )
        .with_storage_delta(blob.size),
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
pub(crate) struct BatchDuplicateFileRecordSpec<'a> {
    pub src: &'a file::Model,
    pub dest_name: Cow<'a, str>,
}

#[derive(Clone)]
pub(crate) struct BatchDuplicateFileRecordTargetSpec<'a> {
    pub src: &'a file::Model,
    pub dest_name: Cow<'a, str>,
    // Recursive folder-copy frontiers always target a concrete newly-created folder, never root.
    pub dest_folder_id: i64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CopiedFilePropertyMode {
    None,
    CopyUserProperties,
}

pub(crate) async fn batch_duplicate_file_records_with_specs_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    copy_specs: &[BatchDuplicateFileRecordSpec<'_>],
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

    let txn = transaction::begin(state.writer_db()).await?;
    storage::lock_storage_usage(&txn, scope).await?;
    let created_by_username = load_scope_actor_username(&txn, scope).await?;

    // 原子性地增加配额（CAS 语义：如果 quota > 0 且 used + total_size > quota，则失败）
    // 这避免了并发场景下的 TOCTOU 问题
    storage::update_storage_used(&txn, scope, total_size).await?;

    let blob_counts = collect_blob_ref_count_increments(
        copy_specs.iter().map(|spec| spec.src.blob_id),
        "batch copy",
    )?;
    file_repo::increment_blob_ref_counts_by(&txn, &blob_counts).await?;

    let models: Vec<file::ActiveModel> = copy_specs
        .iter()
        .map(|spec| {
            let classification = aster_forge_file_classification::classify_file(
                &spec.dest_name,
                &spec.src.mime_type,
            );
            file::ActiveModel {
                name: Set(spec.dest_name.to_string()),
                folder_id: Set(dest_folder_id),
                team_id: Set(scope.team_id()),
                blob_id: Set(spec.src.blob_id),
                size: Set(spec.src.size),
                owner_user_id: Set(scope.owner_user_id()),
                created_by_user_id: Set(Some(scope.actor_user_id())),
                created_by_username: Set(created_by_username.clone()),
                mime_type: Set(spec.src.mime_type.clone()),
                extension: Set(classification.extension),
                compound_extension: Set(classification.compound_extension),
                file_category: Set(classification.category),
                created_at: Set(now),
                updated_at: Set(now),
                ..Default::default()
            }
        })
        .collect();
    file_repo::create_many(&txn, models).await?;

    let dest_names: Vec<String> = copy_specs
        .iter()
        .map(|spec| spec.dest_name.to_string())
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
    for created in &created_files {
        crate::db::repository::revision_repo::create_initial(
            &txn,
            created,
            crate::db::repository::revision_repo::RevisionReason::Copy,
        )
        .await?;
    }

    transaction::commit(txn).await?;
    Ok(created_files)
}

pub(crate) async fn duplicate_file_record_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    src: &file::Model,
    dest_folder_id: Option<i64>,
    dest_name: &str,
) -> Result<file::Model> {
    let txn = transaction::begin(state.writer_db()).await?;
    let new_file =
        duplicate_file_record_in_scope_on(&txn, scope, src, dest_folder_id, dest_name).await?;
    transaction::commit(txn).await?;
    Ok(new_file)
}

pub(crate) async fn duplicate_file_record_in_scope_on<C: ConnectionTrait>(
    db: &C,
    scope: WorkspaceStorageScope,
    src: &file::Model,
    dest_folder_id: Option<i64>,
    dest_name: &str,
) -> Result<file::Model> {
    let new_file = duplicate_file_record_without_initial_revision_in_scope_on(
        db,
        scope,
        src,
        dest_folder_id,
        dest_name,
    )
    .await?;

    crate::db::repository::revision_repo::create_initial(
        db,
        &new_file,
        crate::db::repository::revision_repo::RevisionReason::Copy,
    )
    .await?;

    Ok(new_file)
}

/// Creates a copied file projection without publishing its initial revision.
///
/// WebDAV uses this inside one transaction to copy dead properties first; the caller must then
/// create the initial revision so its property snapshot observes those copied values.
pub(crate) async fn duplicate_file_record_without_initial_revision_in_scope_on<
    C: ConnectionTrait,
>(
    db: &C,
    scope: WorkspaceStorageScope,
    src: &file::Model,
    dest_folder_id: Option<i64>,
    dest_name: &str,
) -> Result<file::Model> {
    let blob = file_repo::find_blob_by_id(db, src.blob_id).await?;
    let now = Utc::now();
    let blob_size = blob.size;

    storage::lock_storage_usage(db, scope).await?;
    let created_by_username = load_scope_actor_username(db, scope).await?;
    storage::check_quota(db, scope, blob_size).await?;

    file_repo::increment_blob_ref_count(db, blob.id).await?;
    let classification = aster_forge_file_classification::classify_file(dest_name, &src.mime_type);

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
        extension: Set(classification.extension),
        compound_extension: Set(classification.compound_extension),
        file_category: Set(classification.category),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(db)
    .await
    .map_err(|err| file_repo::map_name_db_err(err, dest_name))?;

    storage::update_storage_used(db, scope, blob_size).await?;

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
    storage_change::publish(
        state,
        storage_change::StorageChangeEvent::new(
            storage_change::StorageChangeKind::FileCreated,
            WorkspaceStorageScope::Personal {
                user_id: src.owner_user_id.ok_or_else(|| {
                    AsterError::auth_forbidden("source file has no personal owner")
                })?,
            },
            vec![copied.id],
            vec![],
            vec![copied.folder_id],
        )
        .with_storage_delta(copied.size),
    );
    Ok(copied.into())
}

pub(crate) async fn batch_duplicate_file_records_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    src_files: &[file::Model],
    dest_folder_id: Option<i64>,
) -> Result<Vec<file::Model>> {
    let copy_specs: Vec<BatchDuplicateFileRecordSpec<'_>> = src_files
        .iter()
        .map(|src| BatchDuplicateFileRecordSpec {
            dest_name: Cow::Borrowed(src.name.as_str()),
            src,
        })
        .collect();

    batch_duplicate_file_records_with_specs_in_scope(state, scope, &copy_specs, dest_folder_id)
        .await
}

pub(crate) async fn batch_duplicate_file_records_to_mixed_folders_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    copy_specs: &[BatchDuplicateFileRecordTargetSpec<'_>],
    property_mode: CopiedFilePropertyMode,
) -> Result<i64> {
    if copy_specs.is_empty() {
        return Ok(0);
    }

    let total_size = copy_specs.iter().try_fold(0i64, |acc, spec| {
        acc.checked_add(spec.src.size).ok_or_else(|| {
            AsterError::internal_error("total copied byte count overflow during folder copy")
        })
    })?;
    let now = chrono::Utc::now();

    storage::check_quota(state.writer_db(), scope, total_size).await?;

    let txn = transaction::begin(state.writer_db()).await?;
    storage::lock_storage_usage(&txn, scope).await?;
    let created_by_username = load_scope_actor_username(&txn, scope).await?;
    storage::check_quota(&txn, scope, total_size).await?;

    let blob_counts = collect_blob_ref_count_increments(
        copy_specs.iter().map(|spec| spec.src.blob_id),
        "folder copy",
    )?;
    file_repo::increment_blob_ref_counts_by(&txn, &blob_counts).await?;

    let models: Vec<file::ActiveModel> = copy_specs
        .iter()
        .map(|spec| {
            let classification = aster_forge_file_classification::classify_file(
                &spec.dest_name,
                &spec.src.mime_type,
            );
            file::ActiveModel {
                name: Set(spec.dest_name.to_string()),
                folder_id: Set(Some(spec.dest_folder_id)),
                team_id: Set(scope.team_id()),
                blob_id: Set(spec.src.blob_id),
                size: Set(spec.src.size),
                owner_user_id: Set(scope.owner_user_id()),
                created_by_user_id: Set(Some(scope.actor_user_id())),
                created_by_username: Set(created_by_username.clone()),
                mime_type: Set(spec.src.mime_type.clone()),
                extension: Set(classification.extension),
                compound_extension: Set(classification.compound_extension),
                file_category: Set(classification.category),
                created_at: Set(now),
                updated_at: Set(now),
                ..Default::default()
            }
        })
        .collect();
    file_repo::create_many(&txn, models).await?;

    let mut dest_folder_ids: Vec<i64> = copy_specs.iter().map(|spec| spec.dest_folder_id).collect();
    dest_folder_ids.sort_unstable();
    dest_folder_ids.dedup();
    let created_files = match scope {
        WorkspaceStorageScope::Personal { user_id } => {
            file_repo::find_by_folders(&txn, user_id, &dest_folder_ids).await?
        }
        WorkspaceStorageScope::Team { team_id, .. } => {
            file_repo::find_by_team_folders(&txn, team_id, &dest_folder_ids).await?
        }
    };
    let mut created_by_target: HashMap<(i64, String), file::Model> = created_files
        .into_iter()
        .map(|created| {
            let folder_id = created.folder_id.ok_or_else(|| {
                AsterError::internal_error(format!(
                    "folder copy reloaded root file #{} from destination folders",
                    created.id
                ))
            })?;
            Ok(((folder_id, created.name.clone()), created))
        })
        .collect::<Result<_>>()?;

    let mut properties_by_source = HashMap::new();
    if property_mode == CopiedFilePropertyMode::CopyUserProperties {
        let source_targets: Vec<_> = copy_specs
            .iter()
            .map(|spec| (EntityType::File, spec.src.id))
            .collect();
        for property in property_repo::find_by_entities(&txn, &source_targets).await? {
            if property_repo::is_protected_namespace(&property.namespace) {
                continue;
            }
            properties_by_source
                .entry(property.entity_id)
                .or_insert_with(Vec::new)
                .push(property);
        }
    }

    let mut copied_properties = Vec::new();
    let mut created_in_spec_order = Vec::with_capacity(copy_specs.len());
    for spec in copy_specs {
        let target = (spec.dest_folder_id, spec.dest_name.to_string());
        let created = created_by_target.remove(&target).ok_or_else(|| {
            AsterError::internal_error(format!(
                "failed to reload copied file '{}' in folder {:?}",
                target.1, target.0
            ))
        })?;
        if let Some(properties) = properties_by_source.get(&spec.src.id) {
            for property in properties {
                copied_properties.push(property_repo::NewEntityProperty {
                    entity_type: EntityType::File,
                    entity_id: created.id,
                    namespace: property.namespace.clone(),
                    name: property.name.clone(),
                    value: property.value.clone(),
                });
            }
        }
        created_in_spec_order.push(created);
    }
    if !created_by_target.is_empty() {
        return Err(AsterError::internal_error(
            "folder copy reloaded unexpected destination files",
        ));
    }
    // Initial revisions freeze current user properties, so copied properties must land first.
    property_repo::insert_many(&txn, copied_properties).await?;
    crate::db::repository::revision_repo::create_initial_many(
        &txn,
        &created_in_spec_order,
        crate::db::repository::revision_repo::RevisionReason::Copy,
    )
    .await?;

    storage::update_storage_used(&txn, scope, total_size).await?;

    transaction::commit(txn).await?;
    Ok(total_size)
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
