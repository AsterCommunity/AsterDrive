use actix_multipart::Multipart;
use futures::StreamExt;
use tokio::io::{AsyncWriteExt, BufWriter};

use crate::entities::file;
use crate::errors::{MapAsterErr, Result};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::workspace::storage::{
    StoreFromTempHints, StoreFromTempParams, VerifiedFolderPolicyHint, WorkspaceStorageScope,
    create_empty, resolve_policy_for_size_with_verified_folder, store_from_temp_with_hints,
};
use crate::utils::numbers::usize_to_i64;

use super::common::{
    upload_body_size_overflow_error, upload_empty_file_error, upload_field_read_failed,
    upload_size_mismatch_error, upload_temp_dir_create_failed, upload_temp_file_create_failed,
    upload_temp_file_flush_failed, upload_temp_file_write_failed,
};

pub(super) struct StagedUploadParams<'a> {
    pub scope: WorkspaceStorageScope,
    pub folder_id: Option<i64>,
    pub relative_path: Option<&'a str>,
    pub resolved_filename: &'a str,
    pub verified_folder: Option<VerifiedFolderPolicyHint>,
    pub declared_size: Option<i64>,
    pub actor_username: Option<&'a str>,
}

pub(super) async fn upload_staged(
    state: &PrimaryAppState,
    payload: &mut Multipart,
    params: StagedUploadParams<'_>,
) -> Result<file::Model> {
    let StagedUploadParams {
        scope,
        folder_id,
        relative_path,
        resolved_filename,
        verified_folder,
        declared_size,
        actor_username,
    } = params;

    let mut filename = String::from("unnamed");
    let mut saw_file_field = false;
    let temp_dir = &state.config().server.temp_dir;
    let runtime_temp_dir = crate::utils::paths::runtime_temp_dir(temp_dir);
    let temp_path =
        crate::utils::paths::runtime_temp_file_path(temp_dir, &uuid::Uuid::new_v4().to_string());
    tokio::fs::create_dir_all(&runtime_temp_dir)
        .await
        .map_aster_err_ctx("create temp dir", upload_temp_dir_create_failed)?;

    let temp_file = tokio::fs::File::create(&temp_path)
        .await
        .map_aster_err_ctx("create temp", upload_temp_file_create_failed)?;
    let mut temp_file = BufWriter::new(temp_file);
    let mut size: i64 = 0;

    while let Some(field) = payload.next().await {
        let mut field = field.map_aster_err(upload_field_read_failed)?;
        let is_file = field
            .content_disposition()
            .and_then(|content| content.get_filename().map(|name| name.to_string()));

        if let Some(name) = is_file {
            saw_file_field = true;
            filename = if relative_path.is_some() {
                resolved_filename.to_string()
            } else {
                name
            };

            while let Some(chunk) = field.next().await {
                let chunk = chunk.map_aster_err(upload_field_read_failed)?;
                temp_file
                    .write_all(&chunk)
                    .await
                    .map_aster_err_ctx("write temp", upload_temp_file_write_failed)?;
                size = size
                    .checked_add(usize_to_i64(chunk.len(), "chunk length")?)
                    .ok_or_else(upload_body_size_overflow_error)?;
            }
            break;
        }
    }

    temp_file
        .flush()
        .await
        .map_aster_err_ctx("flush temp", upload_temp_file_flush_failed)?;
    drop(temp_file);

    if !saw_file_field {
        crate::utils::cleanup_temp_file(&temp_path).await;
        return Err(upload_empty_file_error());
    }

    if let Some(declared_size) = declared_size
        && size != declared_size
    {
        crate::utils::cleanup_temp_file(&temp_path).await;
        return Err(upload_size_mismatch_error(declared_size, size));
    }

    if size == 0 {
        crate::utils::cleanup_temp_file(&temp_path).await;
        return create_empty(state, scope, folder_id, &filename).await;
    }

    let result = store_from_temp_with_hints(
        state,
        StoreFromTempParams::new(scope, folder_id, &filename, &temp_path, size),
        StoreFromTempHints {
            resolved_policy: Some(
                resolve_policy_for_size_with_verified_folder(state, scope, verified_folder, size)
                    .await?,
            ),
            actor_username,
            ..Default::default()
        },
    )
    .await;

    crate::utils::cleanup_temp_file(&temp_path).await;
    if let Ok(file) = &result {
        tracing::debug!(
            scope = ?scope,
            file_id = file.id,
            folder_id = file.folder_id,
            size = file.size,
            "completed staged multipart upload"
        );
    }
    result
}
