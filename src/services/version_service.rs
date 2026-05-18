//! 服务模块：`version_service`。

use std::collections::BTreeMap;

use chrono::Utc;
use sea_orm::{ActiveModelTrait, Set};

use crate::db::repository::{file_repo, version_repo};
use crate::entities::file_version;
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::runtime::PrimaryAppState;
use crate::services::{
    audit_service::{self, AuditContext},
    storage_change_service,
    workspace_models::{FileInfo, FileVersion},
    workspace_storage_service::{self, WorkspaceResourceScope, WorkspaceStorageScope},
};

async fn load_version_for_file(
    db: &sea_orm::DatabaseConnection,
    file_id: i64,
    version_id: i64,
) -> Result<file_version::Model> {
    let version = version_repo::find_by_id(db, version_id)
        .await?
        .ok_or_else(|| AsterError::record_not_found("version not found"))?;

    if version.file_id != file_id {
        return Err(AsterError::record_not_found("version not found"));
    }

    Ok(version)
}

fn resource_scope_from_file(file: &crate::entities::file::Model) -> Result<WorkspaceResourceScope> {
    match file.team_id {
        Some(team_id) => Ok(WorkspaceResourceScope::Team { team_id }),
        None => Ok(WorkspaceResourceScope::Personal {
            user_id: file
                .owner_user_id
                .ok_or_else(|| AsterError::auth_forbidden("file has no personal owner"))?,
        }),
    }
}

fn add_reclaimed_bytes(total: &mut i64, bytes: i64, context: &str) -> Result<()> {
    *total = total.checked_add(bytes).ok_or_else(|| {
        AsterError::internal_error(format!("version storage accounting overflow: {context}"))
    })?;
    Ok(())
}

fn add_cleanup_count(counts: &mut BTreeMap<i64, i32>, blob_id: i64, context: &str) -> Result<()> {
    let entry = counts.entry(blob_id).or_default();
    *entry = entry.checked_add(1).ok_or_else(|| {
        AsterError::internal_error(format!(
            "version blob cleanup count overflow for blob {blob_id}: {context}"
        ))
    })?;
    Ok(())
}

async fn restore_version_inner(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    file: crate::entities::file::Model,
    version: file_version::Model,
) -> Result<crate::entities::file::Model> {
    let db = &state.db;
    if file.is_locked {
        return Err(AsterError::resource_locked("file is locked"));
    }

    let now = Utc::now();
    let current_blob = file_repo::find_blob_by_id(db, file.blob_id).await?;
    if let Err(e) =
        crate::services::media_processing_service::delete_thumbnail(state, &current_blob).await
    {
        tracing::warn!(
            "failed to delete thumbnail for blob {}: {e}",
            current_blob.id
        );
    }

    let txn = crate::db::transaction::begin(&state.db).await?;

    let previous_blob_id = current_blob.id;
    let target_blob_id = version.blob_id;

    let mut active: crate::entities::file::ActiveModel = file.into();
    active.blob_id = Set(target_blob_id);
    active.size = Set(version.size);
    active.updated_at = Set(now);
    let updated = active
        .update(&txn)
        .await
        .map_aster_err(AsterError::database_operation)?;

    let truncated_versions =
        version_repo::find_by_file_id_from_version(&txn, updated.id, version.version).await?;
    let truncated_blob_ids: Vec<i64> = truncated_versions.iter().map(|v| v.blob_id).collect();
    version_repo::delete_by_file_id_from_version(&txn, updated.id, version.version).await?;

    let mut reclaimed_bytes = 0i64;
    for truncated_version in &truncated_versions {
        if previous_blob_id != target_blob_id && truncated_version.id == version.id {
            continue;
        }
        add_reclaimed_bytes(
            &mut reclaimed_bytes,
            truncated_version.size,
            "restore truncated version bytes",
        )?;
    }
    if previous_blob_id != target_blob_id {
        add_reclaimed_bytes(
            &mut reclaimed_bytes,
            current_blob.size,
            "restore previous current blob bytes",
        )?;
    }
    if reclaimed_bytes != 0 {
        workspace_storage_service::update_storage_used(&txn, scope, -reclaimed_bytes).await?;
    }

    crate::db::transaction::commit(txn).await?;
    storage_change_service::publish(
        state,
        storage_change_service::StorageChangeEvent::new(
            storage_change_service::StorageChangeKind::FileVersionRestored,
            scope,
            vec![updated.id],
            vec![],
            vec![updated.folder_id],
        )
        .with_storage_delta(-reclaimed_bytes),
    );

    let mut cleanup_counts = BTreeMap::<i64, i32>::new();
    for blob_id in truncated_blob_ids {
        add_cleanup_count(
            &mut cleanup_counts,
            blob_id,
            "restore truncated version cleanup",
        )?;
    }

    if previous_blob_id != target_blob_id {
        add_cleanup_count(
            &mut cleanup_counts,
            previous_blob_id,
            "restore previous current blob cleanup",
        )?;
        if let Some(count) = cleanup_counts.get_mut(&target_blob_id) {
            *count = count.saturating_sub(1);
        }
    }

    let cleanup_counts: Vec<(i64, i32)> = cleanup_counts
        .into_iter()
        .filter(|(_, count)| *count > 0)
        .collect();
    cleanup_blobs_if_unused_by_counts(state, &cleanup_counts).await?;

    Ok(updated)
}

async fn delete_version_inner(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    parent_folder_id: Option<i64>,
    version: file_version::Model,
) -> Result<()> {
    let version_id = version.id;
    let file_id = version.file_id;
    let version_number = version.version;
    let blob_id = version.blob_id;
    let size = version.size;
    let txn = crate::db::transaction::begin(&state.db).await?;
    version_repo::delete_by_id(&txn, version_id).await?;
    version_repo::decrement_versions_after(&txn, file_id, version_number).await?;
    if size != 0 {
        workspace_storage_service::update_storage_used(&txn, scope, -size).await?;
    }
    crate::db::transaction::commit(txn).await?;
    storage_change_service::publish(
        state,
        storage_change_service::StorageChangeEvent::new(
            storage_change_service::StorageChangeKind::FileVersionDeleted,
            scope,
            vec![file_id],
            vec![],
            vec![parent_folder_id],
        )
        .with_storage_delta(-size),
    );
    cleanup_blob_if_unused(state, blob_id).await?;
    tracing::debug!(
        scope = ?scope,
        file_id,
        version_id,
        version = version_number,
        blob_id,
        reclaimed_bytes = size,
        "deleted file version"
    );
    Ok(())
}

async fn list_versions_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    file_id: i64,
) -> Result<Vec<file_version::Model>> {
    workspace_storage_service::verify_file_access(state, scope, file_id).await?;
    version_repo::find_by_file_id(&state.db, file_id).await
}

async fn restore_version_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    file_id: i64,
    version_id: i64,
) -> Result<crate::entities::file::Model> {
    let file = workspace_storage_service::verify_file_access(state, scope, file_id).await?;
    if let WorkspaceStorageScope::Team {
        team_id,
        actor_user_id,
    } = scope
    {
        workspace_storage_service::require_team_management_access(state, team_id, actor_user_id)
            .await?;
    }
    let version = load_version_for_file(&state.db, file_id, version_id).await?;
    restore_version_inner(state, scope, file, version).await
}

async fn delete_version_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    file_id: i64,
    version_id: i64,
) -> Result<()> {
    let file = workspace_storage_service::verify_file_access(state, scope, file_id).await?;
    if let WorkspaceStorageScope::Team {
        team_id,
        actor_user_id,
    } = scope
    {
        workspace_storage_service::require_team_management_access(state, team_id, actor_user_id)
            .await?;
    }
    let version = load_version_for_file(&state.db, file_id, version_id).await?;
    delete_version_inner(state, scope, file.folder_id, version).await
}

/// 列出文件的所有版本
pub async fn list_versions(
    state: &PrimaryAppState,
    file_id: i64,
    user_id: i64,
) -> Result<Vec<FileVersion>> {
    list_versions_in_scope(state, WorkspaceStorageScope::Personal { user_id }, file_id)
        .await
        .map(|versions| versions.into_iter().map(FileVersion::from).collect())
}

pub async fn list_versions_for_team(
    state: &PrimaryAppState,
    team_id: i64,
    file_id: i64,
    user_id: i64,
) -> Result<Vec<FileVersion>> {
    list_versions_in_scope(
        state,
        WorkspaceStorageScope::Team {
            team_id,
            actor_user_id: user_id,
        },
        file_id,
    )
    .await
    .map(|versions| versions.into_iter().map(FileVersion::from).collect())
}

/// 恢复到指定版本，并截断该版本及之后的历史版本
pub async fn restore_version(
    state: &PrimaryAppState,
    file_id: i64,
    version_id: i64,
    user_id: i64,
) -> Result<FileInfo> {
    restore_version_in_scope(
        state,
        WorkspaceStorageScope::Personal { user_id },
        file_id,
        version_id,
    )
    .await
    .map(FileInfo::from)
}

pub async fn restore_version_with_audit(
    state: &PrimaryAppState,
    file_id: i64,
    version_id: i64,
    user_id: i64,
    audit_ctx: &AuditContext,
) -> Result<FileInfo> {
    let file = restore_version(state, file_id, version_id, user_id).await?;
    audit_service::log(
        state,
        audit_ctx,
        audit_service::AuditAction::FileVersionRestore,
        crate::services::audit_service::AuditEntityType::File,
        Some(file.id),
        Some(&file.name),
        audit_service::details(audit_service::FileVersionAuditDetails { version_id }),
    )
    .await;
    Ok(file)
}

pub async fn restore_version_for_team(
    state: &PrimaryAppState,
    team_id: i64,
    file_id: i64,
    version_id: i64,
    user_id: i64,
) -> Result<FileInfo> {
    restore_version_in_scope(
        state,
        WorkspaceStorageScope::Team {
            team_id,
            actor_user_id: user_id,
        },
        file_id,
        version_id,
    )
    .await
    .map(FileInfo::from)
}

pub async fn restore_version_for_team_with_audit(
    state: &PrimaryAppState,
    team_id: i64,
    file_id: i64,
    version_id: i64,
    user_id: i64,
    audit_ctx: &AuditContext,
) -> Result<FileInfo> {
    let file = restore_version_for_team(state, team_id, file_id, version_id, user_id).await?;
    audit_service::log(
        state,
        audit_ctx,
        audit_service::AuditAction::FileVersionRestore,
        crate::services::audit_service::AuditEntityType::File,
        Some(file.id),
        Some(&file.name),
        audit_service::details(audit_service::FileVersionAuditDetails { version_id }),
    )
    .await;
    Ok(file)
}

/// 删除指定版本（减 blob ref_count）
pub async fn delete_version(
    state: &PrimaryAppState,
    file_id: i64,
    version_id: i64,
    user_id: i64,
) -> Result<()> {
    delete_version_in_scope(
        state,
        WorkspaceStorageScope::Personal { user_id },
        file_id,
        version_id,
    )
    .await
}

pub async fn delete_version_with_audit(
    state: &PrimaryAppState,
    file_id: i64,
    version_id: i64,
    user_id: i64,
    audit_ctx: &AuditContext,
) -> Result<()> {
    let file = workspace_storage_service::verify_file_access(
        state,
        WorkspaceStorageScope::Personal { user_id },
        file_id,
    )
    .await?;
    let _version = load_version_for_file(&state.db, file_id, version_id).await?;
    delete_version(state, file_id, version_id, user_id).await?;
    audit_service::log(
        state,
        audit_ctx,
        audit_service::AuditAction::FileVersionDelete,
        crate::services::audit_service::AuditEntityType::File,
        Some(file.id),
        Some(&file.name),
        audit_service::details(audit_service::FileVersionAuditDetails { version_id }),
    )
    .await;
    Ok(())
}

pub async fn delete_version_for_team(
    state: &PrimaryAppState,
    team_id: i64,
    file_id: i64,
    version_id: i64,
    user_id: i64,
) -> Result<()> {
    delete_version_in_scope(
        state,
        WorkspaceStorageScope::Team {
            team_id,
            actor_user_id: user_id,
        },
        file_id,
        version_id,
    )
    .await
}

pub async fn delete_version_for_team_with_audit(
    state: &PrimaryAppState,
    team_id: i64,
    file_id: i64,
    version_id: i64,
    user_id: i64,
    audit_ctx: &AuditContext,
) -> Result<()> {
    let file = workspace_storage_service::verify_file_access(
        state,
        WorkspaceStorageScope::Team {
            team_id,
            actor_user_id: user_id,
        },
        file_id,
    )
    .await?;
    let _version = load_version_for_file(&state.db, file_id, version_id).await?;
    delete_version_for_team(state, team_id, file_id, version_id, user_id).await?;
    audit_service::log(
        state,
        audit_ctx,
        audit_service::AuditAction::FileVersionDelete,
        crate::services::audit_service::AuditEntityType::File,
        Some(file.id),
        Some(&file.name),
        audit_service::details(audit_service::FileVersionAuditDetails { version_id }),
    )
    .await;
    Ok(())
}

/// 超出版本上限时清理最旧版本
pub async fn cleanup_excess(state: &PrimaryAppState, file_id: i64) -> Result<()> {
    let db = &state.db;
    let file = file_repo::find_by_id(db, file_id).await?;
    let scope = resource_scope_from_file(&file)?;
    let max_versions = get_max_versions(state).await;
    let mut deleted_count = 0u64;
    let mut reclaimed_bytes = 0i64;

    loop {
        let count = version_repo::count_by_file_id(db, file_id).await?;
        if count <= max_versions {
            break;
        }
        let oldest = version_repo::find_oldest_by_file_id(db, file_id).await?;
        if let Some(oldest) = oldest {
            let txn = crate::db::transaction::begin(&state.db).await?;
            version_repo::delete_by_id(&txn, oldest.id).await?;
            version_repo::decrement_versions_after(&txn, file_id, oldest.version).await?;
            if oldest.size != 0 {
                workspace_storage_service::update_storage_used_for_resource_scope(
                    &txn,
                    scope,
                    -oldest.size,
                )
                .await?;
            }
            crate::db::transaction::commit(txn).await?;
            cleanup_blob_if_unused(state, oldest.blob_id).await?;
            deleted_count += 1;
            add_reclaimed_bytes(
                &mut reclaimed_bytes,
                oldest.size,
                "cleanup excess version bytes",
            )?;
        } else {
            break;
        }
    }

    if deleted_count > 0 {
        storage_change_service::publish(
            state,
            storage_change_service::StorageChangeEvent::new_for_resource_scope(
                storage_change_service::StorageChangeKind::FileVersionDeleted,
                scope,
                vec![file_id],
                vec![],
                vec![file.folder_id],
            )
            .with_storage_delta(-reclaimed_bytes),
        );
        tracing::info!(
            file_id,
            scope = ?scope,
            deleted_count,
            reclaimed_bytes,
            max_versions,
            "cleaned up excess file versions"
        );
    }
    Ok(())
}

/// 清理所有版本（文件永久删除时调用）
pub async fn purge_all_versions(state: &PrimaryAppState, file_id: i64) -> Result<()> {
    let db = &state.db;
    let file = file_repo::find_by_id(db, file_id).await?;
    let scope = resource_scope_from_file(&file)?;
    let versions = version_repo::find_by_file_id(db, file_id).await?;
    let mut reclaimed_bytes = 0i64;
    for version in &versions {
        add_reclaimed_bytes(
            &mut reclaimed_bytes,
            version.size,
            "purge all version bytes",
        )?;
    }

    let txn = crate::db::transaction::begin(&state.db).await?;
    let blob_ids = version_repo::delete_all_by_file_id(&txn, file_id).await?;
    if reclaimed_bytes != 0 {
        workspace_storage_service::update_storage_used_for_resource_scope(
            &txn,
            scope,
            -reclaimed_bytes,
        )
        .await?;
    }
    crate::db::transaction::commit(txn).await?;

    for blob_id in blob_ids {
        cleanup_blob_if_unused(state, blob_id).await?;
    }

    tracing::debug!(
        file_id,
        scope = ?scope,
        version_count = versions.len(),
        reclaimed_bytes,
        "purged all file versions"
    );
    Ok(())
}

/// 如果 blob 不再被任何文件或版本引用，减 ref_count 并可能删除物理文件
async fn cleanup_blob_if_unused(state: &PrimaryAppState, blob_id: i64) -> Result<()> {
    let db = &state.db;
    let blob = file_repo::find_blob_by_id(db, blob_id).await?;

    file_repo::decrement_blob_ref_count(db, blob.id).await?;
    if !crate::services::file_service::ensure_blob_cleanup_if_unreferenced(state, blob.id).await {
        tracing::warn!(
            blob_id = blob.id,
            "blob cleanup incomplete after version cleanup; blob row retained for retry"
        );
    }

    Ok(())
}

async fn cleanup_blobs_if_unused_by_counts(
    state: &PrimaryAppState,
    blob_counts: &[(i64, i32)],
) -> Result<()> {
    if blob_counts.is_empty() {
        return Ok(());
    }

    file_repo::decrement_blob_ref_counts_by(&state.db, blob_counts).await?;
    for &(blob_id, _) in blob_counts {
        if !crate::services::file_service::ensure_blob_cleanup_if_unreferenced(state, blob_id).await
        {
            tracing::warn!(
                blob_id,
                "blob cleanup incomplete after version cleanup; blob row retained for retry"
            );
        }
    }

    Ok(())
}

async fn get_max_versions(state: &PrimaryAppState) -> u64 {
    state
        .runtime_config
        .get_u64("max_versions_per_file")
        .unwrap_or_else(|| {
            if let Some(raw) = state.runtime_config.get("max_versions_per_file") {
                tracing::warn!("invalid max_versions_per_file value '{}', using 10", raw);
            }
            10
        })
}
