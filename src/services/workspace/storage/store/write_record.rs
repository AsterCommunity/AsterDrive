use chrono::Utc;
use sea_orm::{ActiveModelTrait, ConnectionTrait, Set};

use crate::errors::{AsterError, Result};
use crate::services::workspace::storage::{
    WorkspaceStorageScope, create_exact_file_from_blob,
    create_exact_file_from_blob_with_actor_username, create_new_file_from_blob,
    create_new_file_from_blob_with_actor_username, update_storage_used,
};
use aster_drive_model::entities::{file, file_blob};

use super::NewFileMode;
use super::prepare::OverwriteContext;

fn map_revision_append_error(
    error: crate::db::repository::revision_repo::RevisionAppendError,
) -> AsterError {
    use crate::api::api_error_code::ApiErrorCode;
    use crate::db::repository::revision_repo::RevisionAppendError;

    match error {
        RevisionAppendError::HeadChanged => crate::errors::precondition_failed_with_code(
            ApiErrorCode::FileModifiedDuringWrite,
            "file revision head changed while content was being committed",
        ),
        RevisionAppendError::EtagMismatch => crate::errors::precondition_failed_with_code(
            ApiErrorCode::FileEtagMismatch,
            "file has been modified (ETag mismatch)",
        ),
        RevisionAppendError::Repository(error) => error,
    }
}

pub(super) struct WriteFileRecordFromTempParams<'a> {
    pub scope: WorkspaceStorageScope,
    pub folder_id: Option<i64>,
    pub filename: &'a str,
    pub mime: &'a str,
    pub blob: &'a file_blob::Model,
    pub overwrite_ctx: Option<OverwriteContext>,
    pub now: chrono::DateTime<Utc>,
    pub storage_delta: i64,
    pub new_file_mode: NewFileMode,
    pub actor_username: Option<&'a str>,
    pub lock_credentials: &'a crate::services::files::lock::LockMutationCredentials,
    pub file_precondition: Option<&'a super::FileWritePrecondition>,
    pub expected_current_revision_id: Option<i64>,
    pub expected_current_revision_etag: Option<&'a str>,
    pub revision_etag: Option<&'a str>,
}

pub(super) async fn write_file_record_from_temp<C: ConnectionTrait>(
    txn: &C,
    params: WriteFileRecordFromTempParams<'_>,
) -> Result<file::Model> {
    let WriteFileRecordFromTempParams {
        scope,
        folder_id,
        filename,
        mime,
        blob,
        overwrite_ctx,
        now,
        storage_delta,
        new_file_mode,
        actor_username,
        lock_credentials,
        file_precondition,
        expected_current_revision_id,
        expected_current_revision_etag,
        revision_etag,
    } = params;
    if overwrite_ctx.is_none() {
        let workspace = match scope {
            WorkspaceStorageScope::Personal { user_id } => {
                crate::services::files::lock::LockWorkspace::Personal { user_id }
            }
            WorkspaceStorageScope::Team { team_id, .. } => {
                crate::services::files::lock::LockWorkspace::Team { team_id }
            }
        };
        let submitted = lock_credentials.submitted();
        crate::services::files::lock::enforce_collection_membership_mutation_on(
            txn, workspace, folder_id, &submitted,
        )
        .await?;
    }
    let result = if let Some(OverwriteContext { old_file }) = overwrite_ctx {
        let current_file = super::revalidate_overwrite_target(
            txn,
            scope,
            &old_file,
            lock_credentials,
            file_precondition,
        )
        .await?;
        let existing_id = current_file.id;
        let locked_revision_id =
            crate::db::repository::revision_repo::lock_history_by_file_id(txn, existing_id)
                .await?
                .current_revision_id;
        let current_name = current_file.name.clone();
        let mut active: file::ActiveModel = current_file.into();
        active.blob_id = Set(blob.id);
        active.size = Set(blob.size);
        let classification = aster_forge_file_classification::classify_file(&current_name, mime);
        active.mime_type = Set(mime.to_string());
        active.extension = Set(classification.extension);
        active.compound_extension = Set(classification.compound_extension);
        active.file_category = Set(classification.category);
        active.updated_at = Set(now);
        let updated = active.update(txn).await.map_err(AsterError::from)?;
        let actor_username = match actor_username {
            Some(username) => username.to_owned(),
            None => {
                crate::services::workspace::storage::load_scope_actor_username(txn, scope).await?
            }
        };

        let revision_input = crate::db::repository::revision_repo::NewRevision {
            blob_id: blob.id,
            logical_size: blob.size,
            mime_type: mime,
            content_sha256: None,
            creator_user_id: Some(scope.actor_user_id()),
            creator_display_name: &actor_username,
            comment: None,
            reason: crate::db::repository::revision_repo::RevisionReason::Overwrite,
            created_at: now,
            etag: revision_etag,
        };
        if expected_current_revision_etag.is_some() {
            crate::db::repository::revision_repo::append_for_expected_etag(
                txn,
                existing_id,
                expected_current_revision_etag,
                revision_input,
            )
            .await
            .map_err(map_revision_append_error)?;
        } else {
            crate::db::repository::revision_repo::append(
                txn,
                existing_id,
                expected_current_revision_id.or(locked_revision_id),
                revision_input,
            )
            .await
            .map_err(map_revision_append_error)?;
        }
        updated
    } else {
        match new_file_mode {
            NewFileMode::ResolveUnique => {
                create_new_file_record_from_blob(
                    txn,
                    scope,
                    folder_id,
                    filename,
                    blob,
                    now,
                    actor_username,
                )
                .await?
            }
            NewFileMode::Exact => {
                create_exact_file_record_from_blob(
                    txn,
                    scope,
                    folder_id,
                    filename,
                    blob,
                    now,
                    actor_username,
                )
                .await?
            }
        }
    };

    if storage_delta != 0 {
        update_storage_used(txn, scope, storage_delta).await?;
    }

    Ok(result)
}

async fn create_new_file_record_from_blob<C: ConnectionTrait>(
    txn: &C,
    scope: WorkspaceStorageScope,
    folder_id: Option<i64>,
    filename: &str,
    blob: &file_blob::Model,
    now: chrono::DateTime<Utc>,
    actor_username: Option<&str>,
) -> Result<file::Model> {
    match actor_username {
        Some(username) => {
            create_new_file_from_blob_with_actor_username(
                txn, scope, folder_id, filename, blob, now, username,
            )
            .await
        }
        None => create_new_file_from_blob(txn, scope, folder_id, filename, blob, now).await,
    }
}

async fn create_exact_file_record_from_blob<C: ConnectionTrait>(
    txn: &C,
    scope: WorkspaceStorageScope,
    folder_id: Option<i64>,
    filename: &str,
    blob: &file_blob::Model,
    now: chrono::DateTime<Utc>,
    actor_username: Option<&str>,
) -> Result<file::Model> {
    match actor_username {
        Some(username) => {
            create_exact_file_from_blob_with_actor_username(
                txn, scope, folder_id, filename, blob, now, username,
            )
            .await
        }
        None => create_exact_file_from_blob(txn, scope, folder_id, filename, blob, now).await,
    }
}
