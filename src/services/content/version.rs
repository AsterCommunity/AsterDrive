//! Canonical immutable file revision lifecycle.

use aster_forge_db::transaction;
use chrono::Utc;
use sea_orm::{ActiveModelTrait, Set};

use crate::db::repository::{file_repo, revision_repo};
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::{
    events::storage_change,
    ops::audit::{self, AuditContext},
    workspace::models::{FileInfo, FileVersion, FileVersionListQuery},
    workspace::storage::{self, WorkspaceResourceScope, WorkspaceStorageScope},
};
use aster_drive_model::entities::file_revision;

async fn load_revision_for_file<C: sea_orm::ConnectionTrait>(
    db: &C,
    file_id: i64,
    revision_id: i64,
) -> Result<file_revision::Model> {
    revision_repo::find_by_id_for_file(db, file_id, revision_id)
        .await?
        .ok_or_else(|| AsterError::record_not_found("version not found"))
}

fn resource_scope_from_file(
    file: &aster_drive_model::entities::file::Model,
) -> Result<WorkspaceResourceScope> {
    match file.team_id {
        Some(team_id) => Ok(WorkspaceResourceScope::Team { team_id }),
        None => Ok(WorkspaceResourceScope::Personal {
            user_id: file
                .owner_user_id
                .ok_or_else(|| AsterError::auth_forbidden("file has no personal owner"))?,
        }),
    }
}

async fn list_versions_in_scope(
    state: &impl SharedRuntimeState,
    scope: WorkspaceStorageScope,
    file_id: i64,
    query: FileVersionListQuery,
) -> Result<Vec<FileVersion>> {
    storage::verify_file_access_for_read(state, scope, file_id).await?;
    // A successful restore must be visible to the immediate follow-up history refresh.
    // Version history includes the authoritative current-head projection, so replica lag is not
    // an acceptable source for this read.
    let history = revision_repo::find_history_by_file_id(state.writer_db(), file_id).await?;
    revision_repo::find_page_by_file_id(
        state.writer_db(),
        file_id,
        query.limit(),
        query.after_sequence,
    )
    .await
    .map(|revisions| {
        revisions
            .into_iter()
            .map(|revision| {
                FileVersion::from_revision(file_id, history.current_revision_id, revision)
            })
            .collect()
    })
}

async fn restore_version_inner(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    file: aster_drive_model::entities::file::Model,
    revision_id: i64,
) -> Result<aster_drive_model::entities::file::Model> {
    let current_blob = file_repo::find_blob_by_id(state.writer_db(), file.blob_id).await?;
    if let Err(error) =
        crate::services::media::processing::delete_thumbnail(state, &current_blob).await
    {
        tracing::warn!(
            blob_id = current_blob.id,
            %error,
            "failed to delete thumbnail before revision restore"
        );
    }

    let now = Utc::now();
    let txn = transaction::begin(state.writer_db()).await?;
    let actor_username = storage::load_scope_actor_username(&txn, scope).await?;
    let current = crate::services::files::lock::enforce_file_mutation_on(
        &txn,
        &file,
        &crate::services::files::lock::SubmittedLockCredentials::none(),
    )
    .await?;
    if current.blob_id != file.blob_id {
        return Err(crate::errors::precondition_failed_with_code(
            crate::api::api_error_code::ApiErrorCode::FileModifiedDuringWrite,
            "file changed while the version restore was being prepared",
        ));
    }
    storage::lock_storage_usage(&txn, scope).await?;
    let history = revision_repo::lock_history_by_file_id(&txn, file.id).await?;
    let target = load_revision_for_file(&txn, file.id, revision_id).await?;
    let target_blob_id = target
        .blob_id
        .ok_or_else(|| AsterError::record_not_found("version content has been purged"))?;
    let target_mime = target
        .mime_type
        .clone()
        .unwrap_or_else(|| current.mime_type.clone());

    storage::update_storage_used(&txn, scope, target.logical_size).await?;
    file_repo::increment_blob_ref_count(&txn, target_blob_id).await?;
    revision_repo::restore_user_properties(&txn, file.id, target.id).await?;
    revision_repo::append(
        &txn,
        file.id,
        history.current_revision_id,
        revision_repo::NewRevision {
            blob_id: target_blob_id,
            logical_size: target.logical_size,
            mime_type: &target_mime,
            content_sha256: target.content_sha256.as_deref(),
            creator_user_id: Some(scope.actor_user_id()),
            creator_display_name: &actor_username,
            comment: Some("restored historical revision"),
            reason: revision_repo::RevisionReason::Restore,
            created_at: now,
        },
    )
    .await?;

    let current_name = current.name.clone();
    let mut active: aster_drive_model::entities::file::ActiveModel = current.into();
    active.blob_id = Set(target_blob_id);
    active.size = Set(target.logical_size);
    active.mime_type = Set(target_mime.clone());
    let classification =
        aster_forge_file_classification::classify_file(&current_name, &target_mime);
    active.extension = Set(classification.extension);
    active.compound_extension = Set(classification.compound_extension);
    active.file_category = Set(classification.category);
    active.updated_at = Set(now);
    let updated = active
        .update(&txn)
        .await
        .map_aster_err(AsterError::database_operation)?;
    transaction::commit(txn).await?;

    storage_change::publish(
        state,
        storage_change::StorageChangeEvent::new(
            storage_change::StorageChangeKind::FileVersionRestored,
            scope,
            vec![updated.id],
            vec![],
            vec![updated.folder_id],
        )
        .with_storage_delta(target.logical_size),
    );
    Ok(updated)
}

async fn delete_version_inner(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    file: &aster_drive_model::entities::file::Model,
    revision_id: i64,
) -> Result<()> {
    let txn = transaction::begin(state.writer_db()).await?;
    storage::lock_storage_usage(&txn, scope).await?;
    let history = revision_repo::lock_history_by_file_id(&txn, file.id).await?;
    if history.current_revision_id == Some(revision_id) {
        return Err(AsterError::validation_error(
            "the current file revision cannot be deleted",
        ));
    }
    let revision = load_revision_for_file(&txn, file.id, revision_id).await?;
    let blob_id = revision
        .blob_id
        .ok_or_else(|| AsterError::record_not_found("version content has been purged"))?;
    let reclaimed_bytes = revision.logical_size;
    revision_repo::tombstone(&txn, revision).await?;
    file_repo::decrement_blob_ref_count(&txn, blob_id).await?;
    if reclaimed_bytes != 0 {
        storage::update_storage_used(&txn, scope, -reclaimed_bytes).await?;
    }
    transaction::commit(txn).await?;

    storage_change::publish(
        state,
        storage_change::StorageChangeEvent::new(
            storage_change::StorageChangeKind::FileVersionDeleted,
            scope,
            vec![file.id],
            vec![],
            vec![file.folder_id],
        )
        .with_storage_delta(-reclaimed_bytes),
    );
    cleanup_blob_if_unreferenced(state, blob_id).await;
    Ok(())
}

async fn restore_version_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    file_id: i64,
    version_id: i64,
) -> Result<aster_drive_model::entities::file::Model> {
    let file = storage::verify_file_access(state, scope, file_id).await?;
    if let WorkspaceStorageScope::Team {
        team_id,
        actor_user_id,
    } = scope
    {
        storage::require_team_management_access(state, team_id, actor_user_id).await?;
    }
    restore_version_inner(state, scope, file, version_id).await
}

async fn delete_version_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    file_id: i64,
    version_id: i64,
) -> Result<()> {
    let file = storage::verify_file_access(state, scope, file_id).await?;
    if let WorkspaceStorageScope::Team {
        team_id,
        actor_user_id,
    } = scope
    {
        storage::require_team_management_access(state, team_id, actor_user_id).await?;
    }
    delete_version_inner(state, scope, &file, version_id).await
}

pub async fn list_versions(
    state: &impl SharedRuntimeState,
    file_id: i64,
    user_id: i64,
    query: FileVersionListQuery,
) -> Result<Vec<FileVersion>> {
    list_versions_in_scope(
        state,
        WorkspaceStorageScope::Personal { user_id },
        file_id,
        query,
    )
    .await
}

pub async fn list_versions_for_team(
    state: &impl SharedRuntimeState,
    team_id: i64,
    file_id: i64,
    user_id: i64,
    query: FileVersionListQuery,
) -> Result<Vec<FileVersion>> {
    list_versions_in_scope(
        state,
        WorkspaceStorageScope::Team {
            team_id,
            actor_user_id: user_id,
        },
        file_id,
        query,
    )
    .await
}

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
    audit::log_with_details(
        state,
        audit_ctx,
        audit::AuditAction::FileVersionRestore,
        crate::services::ops::audit::AuditEntityType::File,
        Some(file.id),
        Some(&file.name),
        || audit::details(audit::FileVersionAuditDetails { version_id }),
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
    audit::log_with_details(
        state,
        audit_ctx,
        audit::AuditAction::FileVersionRestore,
        crate::services::ops::audit::AuditEntityType::File,
        Some(file.id),
        Some(&file.name),
        || audit::details(audit::FileVersionAuditDetails { version_id }),
    )
    .await;
    Ok(file)
}

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
    let file =
        storage::verify_file_access(state, WorkspaceStorageScope::Personal { user_id }, file_id)
            .await?;
    load_revision_for_file(state.writer_db(), file_id, version_id).await?;
    delete_version(state, file_id, version_id, user_id).await?;
    audit_version_delete(state, audit_ctx, &file, version_id).await;
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
    let scope = WorkspaceStorageScope::Team {
        team_id,
        actor_user_id: user_id,
    };
    let file = storage::verify_file_access(state, scope, file_id).await?;
    load_revision_for_file(state.writer_db(), file_id, version_id).await?;
    delete_version_for_team(state, team_id, file_id, version_id, user_id).await?;
    audit_version_delete(state, audit_ctx, &file, version_id).await;
    Ok(())
}

async fn audit_version_delete(
    state: &PrimaryAppState,
    audit_ctx: &AuditContext,
    file: &aster_drive_model::entities::file::Model,
    version_id: i64,
) {
    audit::log_with_details(
        state,
        audit_ctx,
        audit::AuditAction::FileVersionDelete,
        crate::services::ops::audit::AuditEntityType::File,
        Some(file.id),
        Some(&file.name),
        || audit::details(audit::FileVersionAuditDetails { version_id }),
    )
    .await;
}

pub async fn cleanup_excess(state: &PrimaryAppState, file_id: i64) -> Result<()> {
    let file = file_repo::find_by_id(state.writer_db(), file_id).await?;
    let scope = resource_scope_from_file(&file)?;
    let max_history = get_max_versions(state);
    let max_total = max_history.saturating_add(1);
    let mut deleted_count = 0_u64;
    let mut reclaimed_bytes = 0_i64;

    loop {
        let retired = transaction::with_transaction(state.writer_db(), async |txn| {
            storage::lock_storage_usage_for_resource_scope(txn, scope).await?;
            revision_repo::lock_history_by_file_id(txn, file_id).await?;
            if revision_repo::count_by_file_id(txn, file_id).await? <= max_total {
                return Ok::<Option<(i64, i64)>, AsterError>(None);
            }
            let Some(oldest) = revision_repo::find_oldest_non_current(txn, file_id).await? else {
                return Ok(None);
            };
            let blob_id = oldest.blob_id.ok_or_else(|| {
                AsterError::internal_error("active historical revision has no blob")
            })?;
            let size = oldest.logical_size;
            revision_repo::tombstone(txn, oldest).await?;
            file_repo::decrement_blob_ref_count(txn, blob_id).await?;
            if size != 0 {
                storage::update_storage_used_for_resource_scope(txn, scope, -size).await?;
            }
            Ok(Some((blob_id, size)))
        })
        .await?;
        let Some((blob_id, size)) = retired else {
            break;
        };
        cleanup_blob_if_unreferenced(state, blob_id).await;
        deleted_count += 1;
        reclaimed_bytes = reclaimed_bytes
            .checked_add(size)
            .ok_or_else(|| AsterError::internal_error("revision cleanup byte count overflow"))?;
    }

    if deleted_count > 0 {
        storage_change::publish(
            state,
            storage_change::StorageChangeEvent::new_for_resource_scope(
                storage_change::StorageChangeKind::FileVersionDeleted,
                scope,
                vec![file_id],
                vec![],
                vec![file.folder_id],
            )
            .with_storage_delta(-reclaimed_bytes),
        );
    }
    Ok(())
}

async fn cleanup_blob_if_unreferenced(state: &PrimaryAppState, blob_id: i64) {
    if !crate::services::files::file::ensure_blob_cleanup_if_unreferenced(state, blob_id).await {
        tracing::warn!(
            blob_id,
            "blob cleanup incomplete after revision retirement; blob row retained for retry"
        );
    }
}

fn get_max_versions(state: &PrimaryAppState) -> u64 {
    state
        .runtime_config
        .get_u64("max_versions_per_file")
        .unwrap_or_else(|| {
            if let Some(raw) = state.runtime_config().get("max_versions_per_file") {
                tracing::warn!("invalid max_versions_per_file value '{}', using 10", raw);
            }
            10
        })
}
