use chrono::Utc;
use sea_orm::{ActiveModelTrait, ConnectionTrait, Set};

use crate::db::repository::file_repo;
use crate::entities::{file, file_blob};
use crate::errors::{AsterError, Result};
use crate::services::workspace::scope::{WorkspaceStorageScope, load_scope_actor_username};

const MAX_AUTO_NAME_RETRIES: usize = 32;

#[derive(Clone, Copy)]
enum NewFileNameMode {
    ResolveUnique,
    Exact,
}

struct CreateFileFromBlobParams<'a> {
    scope: WorkspaceStorageScope,
    folder_id: Option<i64>,
    filename: &'a str,
    blob: &'a file_blob::Model,
    now: chrono::DateTime<Utc>,
    name_mode: NewFileNameMode,
    actor_username: Option<&'a str>,
}

async fn create_file_from_blob_with_name_mode<C: ConnectionTrait>(
    db: &C,
    params: CreateFileFromBlobParams<'_>,
) -> Result<file::Model> {
    let CreateFileFromBlobParams {
        scope,
        folder_id,
        filename,
        blob,
        now,
        name_mode,
        actor_username,
    } = params;
    let normalized_filename = aster_forge_validation::filename::normalize_validate_name(filename)?;
    let created_by_username = match actor_username {
        Some(username) => username.to_string(),
        None => load_scope_actor_username(db, scope).await?,
    };

    let (mut final_name, team_id) = match scope {
        WorkspaceStorageScope::Personal { user_id } => {
            let final_name = match name_mode {
                NewFileNameMode::ResolveUnique => {
                    file_repo::resolve_unique_filename(db, user_id, folder_id, &normalized_filename)
                        .await?
                }
                NewFileNameMode::Exact => normalized_filename.clone(),
            };
            (final_name, None)
        }
        WorkspaceStorageScope::Team { team_id, .. } => {
            let final_name = match name_mode {
                NewFileNameMode::ResolveUnique => {
                    file_repo::resolve_unique_team_filename(
                        db,
                        team_id,
                        folder_id,
                        &normalized_filename,
                    )
                    .await?
                }
                NewFileNameMode::Exact => normalized_filename.clone(),
            };
            (final_name, Some(team_id))
        }
    };
    let mime = mime_guess::from_path(&final_name)
        .first_or_octet_stream()
        .to_string();
    let max_attempts = match name_mode {
        NewFileNameMode::ResolveUnique => MAX_AUTO_NAME_RETRIES,
        NewFileNameMode::Exact => 1,
    };

    // `resolve_unique_*` 只能减少冲突，不能彻底消灭并发窗口。
    // 这里仍然依赖数据库唯一约束兜底，并在冲突时继续推进到下一个副本名。
    for attempt in 0..max_attempts {
        let classification = aster_forge_file_classification::classify_file(&final_name, &mime);
        let created = file::ActiveModel {
            name: Set(final_name.clone()),
            folder_id: Set(folder_id),
            team_id: Set(team_id),
            blob_id: Set(blob.id),
            size: Set(blob.size),
            owner_user_id: Set(scope.owner_user_id()),
            created_by_user_id: Set(Some(scope.actor_user_id())),
            created_by_username: Set(created_by_username.clone()),
            mime_type: Set(mime.clone()),
            extension: Set(classification.extension),
            compound_extension: Set(classification.compound_extension),
            file_category: Set(classification.category),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(db)
        .await;

        match created {
            Ok(created) => return Ok(created),
            Err(err) if file_repo::is_name_conflict_db_err(&err) => {
                if matches!(name_mode, NewFileNameMode::Exact) {
                    return Err(file_repo::map_name_db_err(err, &final_name));
                }
                if attempt + 1 == max_attempts {
                    return Err(AsterError::validation_error(format!(
                        "failed to allocate a unique file name for '{}'",
                        normalized_filename
                    )));
                }
                final_name = aster_forge_validation::filename::next_copy_name(&final_name);
            }
            Err(err) => return Err(AsterError::from(err)),
        }
    }

    Err(AsterError::validation_error(format!(
        "failed to allocate a unique file name for '{}'",
        normalized_filename
    )))
}

pub(crate) async fn create_new_file_from_blob<C: ConnectionTrait>(
    db: &C,
    scope: WorkspaceStorageScope,
    folder_id: Option<i64>,
    filename: &str,
    blob: &file_blob::Model,
    now: chrono::DateTime<Utc>,
) -> Result<file::Model> {
    create_file_from_blob_with_name_mode(
        db,
        CreateFileFromBlobParams {
            scope,
            folder_id,
            filename,
            blob,
            now,
            name_mode: NewFileNameMode::ResolveUnique,
            actor_username: None,
        },
    )
    .await
}

pub(crate) async fn create_new_file_from_blob_with_actor_username<C: ConnectionTrait>(
    db: &C,
    scope: WorkspaceStorageScope,
    folder_id: Option<i64>,
    filename: &str,
    blob: &file_blob::Model,
    now: chrono::DateTime<Utc>,
    actor_username: &str,
) -> Result<file::Model> {
    create_file_from_blob_with_name_mode(
        db,
        CreateFileFromBlobParams {
            scope,
            folder_id,
            filename,
            blob,
            now,
            name_mode: NewFileNameMode::ResolveUnique,
            actor_username: Some(actor_username),
        },
    )
    .await
}

pub(crate) async fn create_exact_file_from_blob<C: ConnectionTrait>(
    db: &C,
    scope: WorkspaceStorageScope,
    folder_id: Option<i64>,
    filename: &str,
    blob: &file_blob::Model,
    now: chrono::DateTime<Utc>,
) -> Result<file::Model> {
    create_file_from_blob_with_name_mode(
        db,
        CreateFileFromBlobParams {
            scope,
            folder_id,
            filename,
            blob,
            now,
            name_mode: NewFileNameMode::Exact,
            actor_username: None,
        },
    )
    .await
}

pub(crate) async fn create_exact_file_from_blob_with_actor_username<C: ConnectionTrait>(
    db: &C,
    scope: WorkspaceStorageScope,
    folder_id: Option<i64>,
    filename: &str,
    blob: &file_blob::Model,
    now: chrono::DateTime<Utc>,
    actor_username: &str,
) -> Result<file::Model> {
    create_file_from_blob_with_name_mode(
        db,
        CreateFileFromBlobParams {
            scope,
            folder_id,
            filename,
            blob,
            now,
            name_mode: NewFileNameMode::Exact,
            actor_username: Some(actor_username),
        },
    )
    .await
}
