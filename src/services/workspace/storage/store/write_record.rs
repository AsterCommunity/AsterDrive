use chrono::Utc;
use sea_orm::{ActiveModelTrait, ConnectionTrait, Set};

use crate::entities::{file, file_blob};
use crate::errors::{AsterError, Result};
use crate::services::workspace::storage::{
    WorkspaceStorageScope, create_exact_file_from_blob,
    create_exact_file_from_blob_with_actor_username, create_new_file_from_blob,
    create_new_file_from_blob_with_actor_username, update_storage_used,
};

use super::NewFileMode;
use super::prepare::OverwriteContext;

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
    } = params;
    let result = if let Some(OverwriteContext {
        old_file,
        old_blob,
        skip_lock_check,
    }) = overwrite_ctx
    {
        let current_file =
            super::revalidate_overwrite_target(txn, scope, &old_file, skip_lock_check).await?;
        let existing_id = current_file.id;
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

        let next_ver = crate::db::repository::version_repo::next_version(txn, existing_id).await?;
        crate::db::repository::version_repo::create(
            txn,
            crate::entities::file_version::ActiveModel {
                file_id: Set(existing_id),
                blob_id: Set(old_blob.id),
                version: Set(next_ver),
                size: Set(old_blob.size),
                created_at: Set(now),
                ..Default::default()
            },
        )
        .await?;
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
