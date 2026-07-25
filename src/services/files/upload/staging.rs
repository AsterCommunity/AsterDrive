//! Local staging-file contract for server-managed chunked uploads.
//!
//! Server-managed sessions use a format-specific `.offset-staging-v1` file. Init preallocates it
//! to `total_size`; each Chunk PUT writes its range at
//! `chunk_number * chunk_size`. The database receipt table is the durable completion index, while
//! the staging file may still contain unwritten sparse ranges until every receipt exists.
//!
//! The persisted `session_kind` is the session-format discriminator used by Chunk PUT and Complete;
//! temporary directory contents are never used to infer it.

use std::io::SeekFrom;
#[cfg(unix)]
use std::path::Path;

use tokio::io::AsyncSeekExt;

use crate::api::api_error_code::ApiErrorCode;
use crate::entities::upload_session;
use crate::errors::{MapAsterErr, Result, chunk_upload_error_with_code};
use crate::runtime::SharedRuntimeState;
use aster_forge_utils::numbers::i64_to_u64;
use aster_forge_utils::paths;

pub(crate) const CHUNK_RECEIPT_ETAG: &str = "aster-drive-offset-staging-receipt-v1";
const OFFSET_STAGING_FILE_NAME: &str = ".offset-staging-v1";

pub(crate) fn file_path(state: &impl SharedRuntimeState, upload_id: &str) -> String {
    file_path_in_upload_temp_dir(&state.config().server.upload_temp_dir, upload_id)
}

pub(crate) fn file_path_in_upload_temp_dir(upload_temp_dir: &str, upload_id: &str) -> String {
    let session_temp_dir = paths::upload_temp_dir(upload_temp_dir, upload_id);
    paths::temp_file_path(&session_temp_dir, OFFSET_STAGING_FILE_NAME)
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
    file.sync_all()
        .await
        .map_aster_err_ctx("sync chunk staging file", |message| {
            chunk_upload_error_with_code(ApiErrorCode::UploadTempFileWriteFailed, message)
        })?;
    sync_parent_directory(&path).await?;
    Ok(())
}

#[cfg(unix)]
async fn sync_parent_directory(path: &str) -> Result<()> {
    let parent = Path::new(path).parent().ok_or_else(|| {
        chunk_upload_error_with_code(
            ApiErrorCode::UploadTempFileWriteFailed,
            "chunk staging file has no parent directory",
        )
    })?;
    let directory = tokio::fs::File::open(parent)
        .await
        .map_aster_err_ctx("open chunk staging directory", |message| {
            chunk_upload_error_with_code(ApiErrorCode::UploadTempFileWriteFailed, message)
        })?;
    directory
        .sync_all()
        .await
        .map_aster_err_ctx("sync chunk staging directory", |message| {
            chunk_upload_error_with_code(ApiErrorCode::UploadTempFileWriteFailed, message)
        })
}

#[cfg(not(unix))]
async fn sync_parent_directory(_path: &str) -> Result<()> {
    Ok(())
}

pub(crate) fn chunk_receipt_etag() -> &'static str {
    CHUNK_RECEIPT_ETAG
}

pub(crate) fn chunk_receipt_matches(
    receipt: &crate::entities::upload_session_part::Model,
    expected_part_number: i32,
    expected_size: i64,
) -> bool {
    receipt.part_number == expected_part_number
        && receipt.etag == CHUNK_RECEIPT_ETAG
        && receipt.size == expected_size
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
