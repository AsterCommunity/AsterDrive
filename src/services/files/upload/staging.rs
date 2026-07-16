//! Local staging-file contract for server-managed chunked uploads.
//!
//! New chunked sessions preallocate one logical file and write each chunk directly to its
//! deterministic offset. Compact per-chunk marker files remain the durable completion/index contract;
//! the staging file itself may contain unwritten sparse ranges until all markers exist.

use std::io::SeekFrom;

use tokio::io::AsyncSeekExt;

use crate::api::api_error_code::ApiErrorCode;
use crate::entities::upload_session;
use crate::errors::{MapAsterErr, Result, chunk_upload_error_with_code};
use crate::runtime::SharedRuntimeState;
use aster_forge_utils::numbers::i64_to_u64;
use aster_forge_utils::paths;

const CHUNK_MARKER_PREFIX: &str = "aster-drive-staged-chunk-v1:";

pub(crate) fn file_path(state: &impl SharedRuntimeState, upload_id: &str) -> String {
    paths::upload_assembled_path(&state.config().server.upload_temp_dir, upload_id)
}

pub(crate) async fn prepare(
    state: &impl SharedRuntimeState,
    upload_id: &str,
    total_size: i64,
) -> Result<()> {
    let path = file_path(state, upload_id);
    let file = tokio::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&path)
        .await
        .map_aster_err_ctx("create chunk staging file", |message| {
            chunk_upload_error_with_code(ApiErrorCode::UploadTempFileCreateFailed, message)
        })?;
    file.set_len(i64_to_u64(total_size, "chunk staging total size")?)
        .await
        .map_aster_err_ctx("preallocate chunk staging file", |message| {
            chunk_upload_error_with_code(ApiErrorCode::UploadTempFileWriteFailed, message)
        })?;
    Ok(())
}

pub(crate) async fn exists(state: &impl SharedRuntimeState, upload_id: &str) -> Result<bool> {
    match tokio::fs::metadata(file_path(state, upload_id)).await {
        Ok(metadata) => Ok(metadata.is_file()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(chunk_upload_error_with_code(
            ApiErrorCode::UploadChunkPersistFailed,
            format!("stat chunk staging file: {error}"),
        )),
    }
}

pub(crate) fn chunk_marker_contents(expected_size: i64) -> String {
    format!("{CHUNK_MARKER_PREFIX}{expected_size}\n")
}

pub(crate) async fn chunk_marker_matches(
    chunk_path: &str,
    expected_size: i64,
) -> std::io::Result<bool> {
    let contents = tokio::fs::read_to_string(chunk_path).await?;
    Ok(contents == chunk_marker_contents(expected_size))
}

pub(crate) async fn open_for_chunk_write(
    state: &impl SharedRuntimeState,
    session: &upload_session::Model,
    chunk_number: i32,
) -> Result<tokio::fs::File> {
    let chunk_offset = i64::from(chunk_number)
        .checked_mul(session.chunk_size)
        .ok_or_else(|| {
            chunk_upload_error_with_code(
                ApiErrorCode::UploadChunkSizeOverflow,
                "chunk staging offset exceeds i64 range",
            )
        })?;
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .open(file_path(state, &session.id))
        .await
        .map_aster_err_ctx("open chunk staging file", |message| {
            chunk_upload_error_with_code(ApiErrorCode::UploadChunkPersistFailed, message)
        })?;
    file.seek(SeekFrom::Start(i64_to_u64(
        chunk_offset,
        "chunk staging offset",
    )?))
    .await
    .map_aster_err_ctx("seek chunk staging file", |message| {
        chunk_upload_error_with_code(ApiErrorCode::UploadChunkPersistFailed, message)
    })?;
    Ok(file)
}
