use actix_web::web;
use futures::StreamExt;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncWriteExt, BufWriter};

use crate::api::api_error_code::ApiErrorCode;
use crate::errors::{AsterError, MapAsterErr, Result, file_upload_error_with_code};
use crate::runtime::PrimaryAppState;
use crate::services::workspace::storage::{
    StoreFromTempHints, StoreFromTempParams, WorkspaceStorageScope, store_from_temp_with_hints,
};
use aster_drive_model::entities::{file, storage_policy};
use aster_forge_utils::numbers::usize_to_i64;

pub(crate) struct LocalStreamIngestParams<'a> {
    pub scope: WorkspaceStorageScope,
    pub folder_id: Option<i64>,
    pub filename: &'a str,
    pub mime_type: &'a str,
    pub policy: &'a storage_policy::Model,
    pub declared_size: i64,
    pub actor_username: Option<&'a str>,
    pub upload_id: &'a str,
}

pub(crate) async fn ingest_local_stream(
    state: &PrimaryAppState,
    mut payload: web::Payload,
    params: LocalStreamIngestParams<'_>,
) -> Result<file::Model> {
    let local = crate::storage::connectors::resolve_local_filesystem_projection(
        state.driver_registry().connectors(),
        params.policy,
    )?
    .ok_or_else(|| AsterError::internal_error("local stream plan has no local projection"))?;
    let filename = aster_forge_validation::filename::normalize_validate_name(params.filename)?;
    let staging_token = format!("{}.upload", aster_forge_utils::id::new_uuid());
    let staging_path =
        crate::storage::drivers::local::upload_staging_path(&local.base_path, &staging_token)
            .map_aster_err_ctx(
                "resolve local staging path",
                upload_local_staging_path_failed,
            )?;
    if let Some(parent) = staging_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_aster_err_ctx("create local staging dir", upload_local_staging_dir_failed)?;
    }

    let staging_file = tokio::fs::File::create(&staging_path)
        .await
        .map_aster_err_ctx(
            "create local staging file",
            upload_local_staging_file_failed,
        )?;
    let mut staging_file = BufWriter::new(staging_file);
    let mut hasher = local.content_dedup.then(Sha256::new);
    let mut size = 0_i64;
    let staging_path = staging_path.to_string_lossy().into_owned();
    let write_result = async {
        while let Some(chunk) = payload.next().await {
            let chunk = chunk.map_aster_err(upload_body_read_failed)?;
            size = size
                .checked_add(usize_to_i64(chunk.len(), "stream body chunk length")?)
                .ok_or_else(|| {
                    file_upload_error_with_code(
                        ApiErrorCode::UploadBodySizeOverflow,
                        "stream body size overflows i64",
                    )
                })?;
            if size > params.declared_size {
                return Err(size_mismatch(params.declared_size, size));
            }
            if let Some(hasher) = hasher.as_mut() {
                hasher.update(&chunk);
            }
            staging_file.write_all(&chunk).await.map_aster_err_ctx(
                "write local staging file",
                upload_local_staging_write_failed,
            )?;
        }
        staging_file.flush().await.map_aster_err_ctx(
            "flush local staging file",
            upload_local_staging_flush_failed,
        )
    }
    .await;
    drop(staging_file);

    if let Err(error) = write_result {
        aster_forge_utils::fs::cleanup_temp_file(&staging_path).await;
        return Err(error);
    }
    if size != params.declared_size {
        aster_forge_utils::fs::cleanup_temp_file(&staging_path).await;
        return Err(size_mismatch(params.declared_size, size));
    }

    let precomputed_hash =
        hasher.map(|hasher| aster_forge_crypto::sha256_digest_to_hex(&hasher.finalize()));
    let result = store_from_temp_with_hints(
        state,
        StoreFromTempParams::new(
            params.scope,
            params.folder_id,
            &filename,
            &staging_path,
            size,
        ),
        StoreFromTempHints {
            resolved_policy: Some(params.policy.clone()),
            precomputed_hash: precomputed_hash.as_deref(),
            actor_username: params.actor_username,
            mime_type: Some(params.mime_type),
            complete_upload_id: Some(params.upload_id),
            ..Default::default()
        },
    )
    .await;
    aster_forge_utils::fs::cleanup_temp_file(&staging_path).await;
    result
}

fn upload_body_read_failed(message: String) -> AsterError {
    file_upload_error_with_code(ApiErrorCode::UploadFieldReadFailed, message)
}

fn upload_local_staging_path_failed(message: String) -> AsterError {
    file_upload_error_with_code(ApiErrorCode::UploadLocalStagingPathResolveFailed, message)
}

fn upload_local_staging_dir_failed(message: String) -> AsterError {
    file_upload_error_with_code(ApiErrorCode::UploadLocalStagingDirCreateFailed, message)
}

fn upload_local_staging_file_failed(message: String) -> AsterError {
    file_upload_error_with_code(ApiErrorCode::UploadLocalStagingFileCreateFailed, message)
}

fn upload_local_staging_write_failed(message: String) -> AsterError {
    file_upload_error_with_code(ApiErrorCode::UploadLocalStagingWriteFailed, message)
}

fn upload_local_staging_flush_failed(message: String) -> AsterError {
    file_upload_error_with_code(ApiErrorCode::UploadLocalStagingFlushFailed, message)
}

fn size_mismatch(expected: i64, actual: i64) -> AsterError {
    AsterError::validation_error(format!(
        "size mismatch: declared {expected} bytes, received {actual} bytes"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_stream_error_helpers_preserve_specific_api_codes() {
        for (error, expected) in [
            (
                upload_body_read_failed("read".to_string()),
                ApiErrorCode::UploadFieldReadFailed,
            ),
            (
                upload_local_staging_path_failed("path".to_string()),
                ApiErrorCode::UploadLocalStagingPathResolveFailed,
            ),
            (
                upload_local_staging_dir_failed("dir".to_string()),
                ApiErrorCode::UploadLocalStagingDirCreateFailed,
            ),
            (
                upload_local_staging_file_failed("file".to_string()),
                ApiErrorCode::UploadLocalStagingFileCreateFailed,
            ),
            (
                upload_local_staging_write_failed("write".to_string()),
                ApiErrorCode::UploadLocalStagingWriteFailed,
            ),
            (
                upload_local_staging_flush_failed("flush".to_string()),
                ApiErrorCode::UploadLocalStagingFlushFailed,
            ),
        ] {
            assert_eq!(error.api_error_code(), expected);
        }
    }

    #[test]
    fn local_stream_size_mismatch_reports_declared_and_actual_sizes() {
        let error = size_mismatch(8, 9);
        assert_eq!(error.code(), "E005");
        assert!(error.message().contains("declared 8"));
        assert!(error.message().contains("received 9"));
    }
}
