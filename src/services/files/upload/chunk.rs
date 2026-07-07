//! 分片上传阶段。
//!
//! 这里处理两类“已经进入分片模式”的 session：
//! - 服务端本地暂存 chunk 文件
//! - 服务端 relay 到 object-storage multipart，并把 ETag 记入 upload_session_parts

use bytes::Bytes;
use chrono::Utc;
use futures::StreamExt;
use tokio::io::AsyncWriteExt;

use crate::api::api_error_code::ApiErrorCode;
use crate::db::repository::{upload_session_part_repo, upload_session_repo};
use crate::entities::upload_session;
use crate::errors::{
    AsterError, MapAsterErr, Result, chunk_upload_error_with_code, payload_too_large_with_code,
    validation_error_with_code,
};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::files::upload::responses::ChunkUploadResponse;
use crate::services::files::upload::scope::{load_upload_session, personal_scope, team_scope};
use crate::services::files::upload::shared::{
    expected_chunk_size_for_upload, upload_session_chunk_unavailable_error,
};
use crate::types::UploadSessionStatus;
use crate::utils::numbers::{i64_to_u64, usize_to_i64};
use crate::utils::paths;

const RELAY_STREAM_PIPE_BUFFER_SIZE: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExistingLocalChunk {
    Missing,
    Complete,
    RemovedCorrupt,
}

async fn increment_session_received_count<C: sea_orm::ConnectionTrait>(
    db: &C,
    upload_id: &str,
) -> Result<()> {
    if upload_session_repo::increment_received_count_if_uploading(db, upload_id).await? {
        return Ok(());
    }

    // 计数自增失败不代表数据库坏了，更常见的是 session 状态已经不再允许继续上传。
    // 回读最新 session 后返回更准确的业务错误，避免客户端只看到模糊的 DB 失败。
    match upload_session_repo::find_by_id(db, upload_id).await {
        Ok(session) => Err(upload_session_chunk_unavailable_error(&session)),
        Err(error) => Err(error),
    }
}

async fn remove_local_chunk_file(path: &str, upload_id: &str, chunk_number: i32, reason: &str) {
    match tokio::fs::remove_file(path).await {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            tracing::warn!(
                upload_id,
                chunk_number,
                path,
                "failed to remove local chunk file after {reason}: {error}"
            );
        }
    }
}

async fn inspect_existing_local_chunk(
    chunk_path: &str,
    expected_size: i64,
    upload_id: &str,
    chunk_number: i32,
) -> Result<ExistingLocalChunk> {
    let metadata = match tokio::fs::metadata(chunk_path).await {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ExistingLocalChunk::Missing);
        }
        Err(error) => {
            return Err(chunk_upload_error_with_code(
                ApiErrorCode::UploadChunkPersistFailed,
                format!("stat existing chunk file: {error}"),
            ));
        }
    };

    let expected_size = i64_to_u64(expected_size, "expected chunk size")?;
    if metadata.is_file() && metadata.len() == expected_size {
        return Ok(ExistingLocalChunk::Complete);
    }

    tracing::warn!(
        upload_id,
        chunk_number,
        chunk_path,
        actual_size = metadata.len(),
        expected_size,
        is_file = metadata.is_file(),
        "removing corrupt local upload chunk"
    );
    remove_local_chunk_file(chunk_path, upload_id, chunk_number, "corrupt local chunk").await;
    Ok(ExistingLocalChunk::RemovedCorrupt)
}

async fn write_local_chunk_temp(
    temp_path: &str,
    data: &[u8],
    upload_id: &str,
    chunk_number: i32,
) -> Result<()> {
    use tokio::fs::OpenOptions;
    use tokio::io::AsyncWriteExt;

    let write_result = async {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(temp_path)
            .await
            .map_err(|error| {
                chunk_upload_error_with_code(
                    ApiErrorCode::UploadChunkPersistFailed,
                    format!("create temp chunk file: {error}"),
                )
            })?;

        file.write_all(data)
            .await
            .map_aster_err_ctx("write chunk", |message| {
                chunk_upload_error_with_code(ApiErrorCode::UploadChunkPersistFailed, message)
            })?;
        file.flush()
            .await
            .map_aster_err_ctx("flush chunk", |message| {
                chunk_upload_error_with_code(ApiErrorCode::UploadChunkPersistFailed, message)
            })?;
        Ok::<(), AsterError>(())
    }
    .await;

    if write_result.is_err() {
        remove_local_chunk_file(temp_path, upload_id, chunk_number, "temp chunk write error").await;
    }

    write_result
}

fn chunk_body_read_failed() -> AsterError {
    validation_error_with_code(
        ApiErrorCode::UploadRequestBodyReadFailed,
        "failed to read request body",
    )
}

fn chunk_body_size_mismatch(chunk_number: i32, expected_size: i64, actual_size: i64) -> AsterError {
    chunk_upload_error_with_code(
        ApiErrorCode::UploadChunkSizeMismatch,
        format!("chunk {chunk_number} size mismatch: expected {expected_size}, got {actual_size}"),
    )
}

fn chunk_body_too_large(chunk_number: i32, expected_size: i64) -> AsterError {
    payload_too_large_with_code(
        ApiErrorCode::UploadChunkTooLarge,
        format!("chunk {chunk_number} exceeds expected size {expected_size}"),
    )
}

fn chunk_body_size_overflow() -> AsterError {
    payload_too_large_with_code(
        ApiErrorCode::UploadChunkSizeOverflow,
        "chunk body size exceeds supported range",
    )
}

fn add_chunk_body_len(current: i64, chunk_len: usize) -> Result<i64> {
    current
        .checked_add(usize_to_i64(chunk_len, "chunk body part length")?)
        .ok_or_else(chunk_body_size_overflow)
}

fn ensure_chunk_body_not_too_large(
    actual_size: i64,
    expected_size: i64,
    chunk_number: i32,
) -> Result<()> {
    if actual_size > expected_size {
        return Err(chunk_body_too_large(chunk_number, expected_size));
    }
    Ok(())
}

fn ensure_chunk_body_exact_size(
    actual_size: i64,
    expected_size: i64,
    chunk_number: i32,
) -> Result<()> {
    if actual_size != expected_size {
        return Err(chunk_body_size_mismatch(
            chunk_number,
            expected_size,
            actual_size,
        ));
    }
    Ok(())
}

async fn drain_chunk_payload_exact_size(
    payload: &mut actix_web::web::Payload,
    expected_size: i64,
    chunk_number: i32,
) -> Result<()> {
    let mut size = 0i64;
    while let Some(chunk) = payload.next().await {
        let chunk = chunk.map_aster_err_with(chunk_body_read_failed)?;
        size = add_chunk_body_len(size, chunk.len())?;
        ensure_chunk_body_not_too_large(size, expected_size, chunk_number)?;
    }
    ensure_chunk_body_exact_size(size, expected_size, chunk_number)
}

async fn write_local_chunk_temp_stream(
    temp_path: &str,
    payload: &mut actix_web::web::Payload,
    expected_size: i64,
    upload_id: &str,
    chunk_number: i32,
) -> Result<()> {
    use tokio::fs::OpenOptions;
    use tokio::io::BufWriter;

    let write_result = async {
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(temp_path)
            .await
            .map_err(|error| {
                chunk_upload_error_with_code(
                    ApiErrorCode::UploadChunkPersistFailed,
                    format!("create temp chunk file: {error}"),
                )
            })?;
        let mut file = BufWriter::new(file);
        let mut size = 0i64;

        while let Some(chunk) = payload.next().await {
            let chunk = chunk.map_aster_err_with(chunk_body_read_failed)?;
            size = add_chunk_body_len(size, chunk.len())?;
            ensure_chunk_body_not_too_large(size, expected_size, chunk_number)?;
            file.write_all(&chunk)
                .await
                .map_aster_err_ctx("write chunk", |message| {
                    chunk_upload_error_with_code(ApiErrorCode::UploadChunkPersistFailed, message)
                })?;
        }

        ensure_chunk_body_exact_size(size, expected_size, chunk_number)?;
        file.flush()
            .await
            .map_aster_err_ctx("flush chunk", |message| {
                chunk_upload_error_with_code(ApiErrorCode::UploadChunkPersistFailed, message)
            })?;
        Ok::<(), AsterError>(())
    }
    .await;

    if write_result.is_err() {
        remove_local_chunk_file(temp_path, upload_id, chunk_number, "temp chunk write error").await;
    }

    write_result
}

async fn pipe_payload_to_writer(
    mut payload: actix_web::web::Payload,
    mut writer: tokio::io::DuplexStream,
    expected_size: i64,
    chunk_number: i32,
) -> Result<()> {
    let write_result = async {
        let mut size = 0i64;
        while let Some(chunk) = payload.next().await {
            let chunk = chunk.map_aster_err_with(chunk_body_read_failed)?;
            size = add_chunk_body_len(size, chunk.len())?;
            ensure_chunk_body_not_too_large(size, expected_size, chunk_number)?;
            writer
                .write_all(&chunk)
                .await
                .map_aster_err_ctx("stream relay chunk", |message| {
                    chunk_upload_error_with_code(ApiErrorCode::UploadChunkRelayFailed, message)
                })?;
        }
        ensure_chunk_body_exact_size(size, expected_size, chunk_number)?;
        writer
            .shutdown()
            .await
            .map_aster_err_ctx("finish relay chunk stream", |message| {
                chunk_upload_error_with_code(ApiErrorCode::UploadChunkRelayFailed, message)
            })?;
        Ok::<(), AsterError>(())
    }
    .await;

    drop(writer);
    write_result
}

async fn upload_multipart_part_payload(
    multipart: &(dyn crate::storage::MultipartStorageDriver + Send + Sync),
    temp_key: &str,
    multipart_id: &str,
    object_part_number: i32,
    payload: actix_web::web::Payload,
    expected_size: i64,
    chunk_number: i32,
) -> Result<String> {
    let (reader, writer) = tokio::io::duplex(RELAY_STREAM_PIPE_BUFFER_SIZE);
    let writer_future = pipe_payload_to_writer(payload, writer, expected_size, chunk_number);
    let upload_future = multipart.upload_multipart_part_reader(
        temp_key,
        multipart_id,
        object_part_number,
        Box::new(reader),
        expected_size,
    );
    tokio::pin!(upload_future);
    tokio::pin!(writer_future);

    tokio::select! {
        upload_result = &mut upload_future => {
            let writer_result = writer_future.await;
            prioritize_multipart_part_results(upload_result, writer_result)
        }
        writer_result = &mut writer_future => {
            if let Err(writer_error) = writer_result {
                if is_chunk_payload_error(&writer_error) {
                    // Payload validation/read failures are authoritative. Dropping upload_future
                    // cancels the provider request instead of waiting for it to observe EOF.
                    return Err(writer_error);
                }

                let upload_result = upload_future.await;
                return prioritize_multipart_part_results(upload_result, Err(writer_error));
            }

            let upload_result = upload_future.await;
            prioritize_multipart_part_results(upload_result, Ok(()))
        }
    }
}

fn is_chunk_payload_error(error: &AsterError) -> bool {
    let api_code = error.api_error_code_override();
    matches!(
        api_code,
        Some(
            ApiErrorCode::UploadChunkTooLarge
                | ApiErrorCode::UploadChunkSizeMismatch
                | ApiErrorCode::UploadChunkSizeOverflow
                | ApiErrorCode::UploadRequestBodyReadFailed
        )
    )
}

fn prioritize_multipart_part_results(
    upload_result: Result<String>,
    writer_result: Result<()>,
) -> Result<String> {
    match (upload_result, writer_result) {
        (Ok(etag), Ok(())) => Ok(etag),
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(error)) => Err(error),
        (Err(upload_error), Err(writer_error)) => {
            if is_chunk_payload_error(&writer_error) {
                Err(writer_error)
            } else {
                Err(upload_error)
            }
        }
    }
}

async fn publish_local_chunk_temp(
    temp_path: &str,
    chunk_path: &str,
    expected_size: i64,
    upload_id: &str,
    chunk_number: i32,
) -> Result<bool> {
    for _ in 0..2 {
        match tokio::fs::hard_link(temp_path, chunk_path).await {
            Ok(()) => {
                remove_local_chunk_file(temp_path, upload_id, chunk_number, "chunk publish").await;
                return Ok(true);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                match inspect_existing_local_chunk(
                    chunk_path,
                    expected_size,
                    upload_id,
                    chunk_number,
                )
                .await?
                {
                    ExistingLocalChunk::Complete => {
                        remove_local_chunk_file(
                            temp_path,
                            upload_id,
                            chunk_number,
                            "duplicate chunk publish",
                        )
                        .await;
                        return Ok(false);
                    }
                    ExistingLocalChunk::Missing | ExistingLocalChunk::RemovedCorrupt => continue,
                }
            }
            Err(error) => {
                remove_local_chunk_file(temp_path, upload_id, chunk_number, "chunk publish error")
                    .await;
                return Err(chunk_upload_error_with_code(
                    ApiErrorCode::UploadChunkPersistFailed,
                    format!("publish chunk file: {error}"),
                ));
            }
        }
    }

    remove_local_chunk_file(
        temp_path,
        upload_id,
        chunk_number,
        "chunk publish retry exhausted",
    )
    .await;
    Err(chunk_upload_error_with_code(
        ApiErrorCode::UploadChunkPersistFailed,
        "publish chunk file: existing chunk stayed unavailable",
    ))
}

async fn upload_chunk_impl(
    state: &PrimaryAppState,
    session: upload_session::Model,
    chunk_number: i32,
    data: Bytes,
) -> Result<ChunkUploadResponse> {
    let db = state.writer_db();
    let upload_id = session.id.as_str();
    tracing::debug!(
        upload_id,
        chunk_number,
        chunk_size = data.len(),
        status = ?session.status,
        total_chunks = session.total_chunks,
        "handling upload chunk"
    );
    if session.status != UploadSessionStatus::Uploading {
        return Err(upload_session_chunk_unavailable_error(&session));
    }
    if session.expires_at < Utc::now() {
        return Err(AsterError::upload_session_expired("session expired"));
    }
    if chunk_number < 0 || chunk_number >= session.total_chunks {
        return Err(validation_error_with_code(
            ApiErrorCode::UploadChunkNumberOutOfRange,
            format!(
                "chunk_number {} out of range [0, {})",
                chunk_number, session.total_chunks
            ),
        ));
    }

    let expected_size = expected_chunk_size_for_upload(&session, chunk_number)?;
    let data_len = usize_to_i64(data.len(), "chunk data length")?;
    if data_len != expected_size {
        return Err(chunk_upload_error_with_code(
            ApiErrorCode::UploadChunkSizeMismatch,
            format!("chunk {chunk_number} size mismatch: expected {expected_size}, got {data_len}"),
        ));
    }

    if let (Some(temp_key), Some(multipart_id)) = (
        session.object_temp_key.as_deref(),
        session.object_multipart_id.as_deref(),
    ) {
        let object_part_number = chunk_number + 1;

        // relay multipart 下，先 claim part 再上传到对象存储。
        // 否则并发重试会把同一个 part 号重复上传，最后谁的 ETag 留库就会变得不确定。
        if !upload_session_part_repo::try_claim_part(db, upload_id, object_part_number).await? {
            let updated = upload_session_repo::find_by_id(db, upload_id).await?;
            tracing::debug!(
                upload_id,
                chunk_number,
                part_number = object_part_number,
                received_count = updated.received_count,
                total_chunks = updated.total_chunks,
                "skipping already claimed relay multipart part"
            );
            return Ok(ChunkUploadResponse {
                received_count: updated.received_count,
                total_chunks: updated.total_chunks,
            });
        }

        let policy = state
            .policy_snapshot()
            .get_policy_or_err(session.policy_id)?;
        let multipart = state.driver_registry().get_multipart_driver(&policy)?;
        let etag = match multipart
            .upload_multipart_part_bytes(temp_key, multipart_id, object_part_number, data)
            .await
        {
            Ok(etag) => etag,
            Err(err) => {
                if let Err(cleanup_err) = upload_session_part_repo::delete_by_upload_and_part(
                    db,
                    upload_id,
                    object_part_number,
                )
                .await
                {
                    tracing::warn!(
                        upload_id,
                        part_number = object_part_number,
                        "failed to release relay multipart part claim after upload error: {cleanup_err}"
                    );
                }
                return Err(err);
            }
        };

        let txn = crate::db::transaction::begin(db).await?;
        let finalize_result = async {
            // 对象存储 part 上传成功以后，必须把 part 元数据和 received_count 放在同一事务里提交；
            // 否则 complete 阶段会看到“不完整的 part 清单”。
            upload_session_part_repo::upsert_part(
                &txn,
                upload_id,
                object_part_number,
                &etag,
                data_len,
            )
            .await?;
            increment_session_received_count(&txn, upload_id).await?;
            crate::db::transaction::commit(txn).await?;
            Ok::<(), AsterError>(())
        }
        .await;

        if let Err(err) = finalize_result {
            if let Err(cleanup_err) = upload_session_part_repo::delete_by_upload_and_part(
                db,
                upload_id,
                object_part_number,
            )
            .await
            {
                tracing::warn!(
                    upload_id,
                    part_number = object_part_number,
                    "failed to release relay multipart part claim after DB finalize error: {cleanup_err}"
                );
            }
            return Err(err);
        }

        let updated = upload_session_repo::find_by_id(db, upload_id).await?;
        tracing::debug!(
            upload_id,
            chunk_number,
            part_number = object_part_number,
            received_count = updated.received_count,
            total_chunks = updated.total_chunks,
            "stored relay multipart chunk"
        );
        return Ok(ChunkUploadResponse {
            received_count: updated.received_count,
            total_chunks: updated.total_chunks,
        });
    }

    let chunk_path = paths::upload_chunk_path(
        &state.config().server.upload_temp_dir,
        upload_id,
        chunk_number,
    );

    if inspect_existing_local_chunk(&chunk_path, expected_size, upload_id, chunk_number).await?
        == ExistingLocalChunk::Complete
    {
        let updated = upload_session_repo::find_by_id(db, upload_id).await?;
        tracing::debug!(
            upload_id,
            chunk_number,
            received_count = updated.received_count,
            total_chunks = updated.total_chunks,
            "skipping already uploaded chunk"
        );
        return Ok(ChunkUploadResponse {
            received_count: updated.received_count,
            total_chunks: updated.total_chunks,
        });
    }

    let chunk_dir = paths::upload_temp_dir(&state.config().server.upload_temp_dir, upload_id);
    let temp_chunk_path = paths::temp_file_path(
        &chunk_dir,
        &format!(
            ".chunk_{chunk_number}.{}.partial",
            crate::utils::id::new_uuid()
        ),
    );

    write_local_chunk_temp(&temp_chunk_path, data.as_ref(), upload_id, chunk_number).await?;

    if !publish_local_chunk_temp(
        &temp_chunk_path,
        &chunk_path,
        expected_size,
        upload_id,
        chunk_number,
    )
    .await?
    {
        let updated = upload_session_repo::find_by_id(db, upload_id).await?;
        tracing::debug!(
            upload_id,
            chunk_number,
            received_count = updated.received_count,
            total_chunks = updated.total_chunks,
            "skipping already uploaded chunk"
        );
        return Ok(ChunkUploadResponse {
            received_count: updated.received_count,
            total_chunks: updated.total_chunks,
        });
    }

    // 本地 chunk 模式的幂等语义靠最终 chunk 路径的无覆盖发布保证：
    // 同一块重复上传不会覆盖旧文件，而是直接回读 session 进度返回给客户端。
    increment_session_received_count(db, upload_id).await?;

    let updated = upload_session_repo::find_by_id(db, upload_id).await?;
    tracing::debug!(
        upload_id,
        chunk_number,
        received_count = updated.received_count,
        total_chunks = updated.total_chunks,
        "stored upload chunk"
    );
    Ok(ChunkUploadResponse {
        received_count: updated.received_count,
        total_chunks: updated.total_chunks,
    })
}

async fn upload_chunk_payload_impl(
    state: &PrimaryAppState,
    session: upload_session::Model,
    chunk_number: i32,
    mut payload: actix_web::web::Payload,
) -> Result<ChunkUploadResponse> {
    let db = state.writer_db();
    let upload_id = session.id.as_str();
    tracing::debug!(
        upload_id,
        chunk_number,
        status = ?session.status,
        total_chunks = session.total_chunks,
        "handling upload chunk stream"
    );
    if session.status != UploadSessionStatus::Uploading {
        return Err(upload_session_chunk_unavailable_error(&session));
    }
    if session.expires_at < Utc::now() {
        return Err(AsterError::upload_session_expired("session expired"));
    }
    if chunk_number < 0 || chunk_number >= session.total_chunks {
        return Err(validation_error_with_code(
            ApiErrorCode::UploadChunkNumberOutOfRange,
            format!(
                "chunk_number {} out of range [0, {})",
                chunk_number, session.total_chunks
            ),
        ));
    }

    let expected_size = expected_chunk_size_for_upload(&session, chunk_number)?;

    if let (Some(temp_key), Some(multipart_id)) = (
        session.object_temp_key.as_deref(),
        session.object_multipart_id.as_deref(),
    ) {
        let object_part_number = chunk_number + 1;

        if !upload_session_part_repo::try_claim_part(db, upload_id, object_part_number).await? {
            drain_chunk_payload_exact_size(&mut payload, expected_size, chunk_number).await?;
            let updated = upload_session_repo::find_by_id(db, upload_id).await?;
            tracing::debug!(
                upload_id,
                chunk_number,
                part_number = object_part_number,
                received_count = updated.received_count,
                total_chunks = updated.total_chunks,
                "skipping already claimed relay multipart part"
            );
            return Ok(ChunkUploadResponse {
                received_count: updated.received_count,
                total_chunks: updated.total_chunks,
            });
        }

        let policy = state
            .policy_snapshot()
            .get_policy_or_err(session.policy_id)?;
        let multipart = state.driver_registry().get_multipart_driver(&policy)?;
        let etag = match upload_multipart_part_payload(
            multipart.as_ref(),
            temp_key,
            multipart_id,
            object_part_number,
            payload,
            expected_size,
            chunk_number,
        )
        .await
        {
            Ok(etag) => etag,
            Err(err) => {
                if let Err(cleanup_err) = upload_session_part_repo::delete_by_upload_and_part(
                    db,
                    upload_id,
                    object_part_number,
                )
                .await
                {
                    tracing::warn!(
                        upload_id,
                        part_number = object_part_number,
                        "failed to release relay multipart part claim after upload error: {cleanup_err}"
                    );
                }
                return Err(err);
            }
        };

        let txn = crate::db::transaction::begin(db).await?;
        let finalize_result = async {
            upload_session_part_repo::upsert_part(
                &txn,
                upload_id,
                object_part_number,
                &etag,
                expected_size,
            )
            .await?;
            increment_session_received_count(&txn, upload_id).await?;
            crate::db::transaction::commit(txn).await?;
            Ok::<(), AsterError>(())
        }
        .await;

        if let Err(err) = finalize_result {
            if let Err(cleanup_err) = upload_session_part_repo::delete_by_upload_and_part(
                db,
                upload_id,
                object_part_number,
            )
            .await
            {
                tracing::warn!(
                    upload_id,
                    part_number = object_part_number,
                    "failed to release relay multipart part claim after DB finalize error: {cleanup_err}"
                );
            }
            return Err(err);
        }

        let updated = upload_session_repo::find_by_id(db, upload_id).await?;
        tracing::debug!(
            upload_id,
            chunk_number,
            part_number = object_part_number,
            received_count = updated.received_count,
            total_chunks = updated.total_chunks,
            "stored relay multipart chunk"
        );
        return Ok(ChunkUploadResponse {
            received_count: updated.received_count,
            total_chunks: updated.total_chunks,
        });
    }

    let chunk_path = paths::upload_chunk_path(
        &state.config().server.upload_temp_dir,
        upload_id,
        chunk_number,
    );

    if inspect_existing_local_chunk(&chunk_path, expected_size, upload_id, chunk_number).await?
        == ExistingLocalChunk::Complete
    {
        drain_chunk_payload_exact_size(&mut payload, expected_size, chunk_number).await?;
        let updated = upload_session_repo::find_by_id(db, upload_id).await?;
        tracing::debug!(
            upload_id,
            chunk_number,
            received_count = updated.received_count,
            total_chunks = updated.total_chunks,
            "skipping already uploaded chunk"
        );
        return Ok(ChunkUploadResponse {
            received_count: updated.received_count,
            total_chunks: updated.total_chunks,
        });
    }

    let chunk_dir = paths::upload_temp_dir(&state.config().server.upload_temp_dir, upload_id);
    let temp_chunk_path = paths::temp_file_path(
        &chunk_dir,
        &format!(
            ".chunk_{chunk_number}.{}.partial",
            crate::utils::id::new_uuid()
        ),
    );

    write_local_chunk_temp_stream(
        &temp_chunk_path,
        &mut payload,
        expected_size,
        upload_id,
        chunk_number,
    )
    .await?;

    if !publish_local_chunk_temp(
        &temp_chunk_path,
        &chunk_path,
        expected_size,
        upload_id,
        chunk_number,
    )
    .await?
    {
        let updated = upload_session_repo::find_by_id(db, upload_id).await?;
        tracing::debug!(
            upload_id,
            chunk_number,
            received_count = updated.received_count,
            total_chunks = updated.total_chunks,
            "skipping already uploaded chunk"
        );
        return Ok(ChunkUploadResponse {
            received_count: updated.received_count,
            total_chunks: updated.total_chunks,
        });
    }

    increment_session_received_count(db, upload_id).await?;

    let updated = upload_session_repo::find_by_id(db, upload_id).await?;
    tracing::debug!(
        upload_id,
        chunk_number,
        received_count = updated.received_count,
        total_chunks = updated.total_chunks,
        "stored upload chunk"
    );
    Ok(ChunkUploadResponse {
        received_count: updated.received_count,
        total_chunks: updated.total_chunks,
    })
}

/// 上传单个分片
pub async fn upload_chunk(
    state: &PrimaryAppState,
    upload_id: &str,
    chunk_number: i32,
    user_id: i64,
    data: &[u8],
) -> Result<ChunkUploadResponse> {
    let session = load_upload_session(state, personal_scope(user_id), upload_id).await?;
    upload_chunk_impl(state, session, chunk_number, Bytes::copy_from_slice(data)).await
}

/// 上传单个分片，接收 HTTP body 已持有的 `Bytes`，避免 relay multipart 再复制一份大块数据。
pub async fn upload_chunk_bytes(
    state: &PrimaryAppState,
    upload_id: &str,
    chunk_number: i32,
    user_id: i64,
    data: Bytes,
) -> Result<ChunkUploadResponse> {
    let session = load_upload_session(state, personal_scope(user_id), upload_id).await?;
    upload_chunk_impl(state, session, chunk_number, data).await
}

pub async fn upload_chunk_payload(
    state: &PrimaryAppState,
    upload_id: &str,
    chunk_number: i32,
    user_id: i64,
    payload: actix_web::web::Payload,
) -> Result<ChunkUploadResponse> {
    let session = load_upload_session(state, personal_scope(user_id), upload_id).await?;
    upload_chunk_payload_impl(state, session, chunk_number, payload).await
}

pub async fn upload_chunk_for_team(
    state: &PrimaryAppState,
    team_id: i64,
    upload_id: &str,
    chunk_number: i32,
    user_id: i64,
    data: &[u8],
) -> Result<ChunkUploadResponse> {
    let session = load_upload_session(state, team_scope(team_id, user_id), upload_id).await?;
    upload_chunk_impl(state, session, chunk_number, Bytes::copy_from_slice(data)).await
}

pub async fn upload_chunk_bytes_for_team(
    state: &PrimaryAppState,
    team_id: i64,
    upload_id: &str,
    chunk_number: i32,
    user_id: i64,
    data: Bytes,
) -> Result<ChunkUploadResponse> {
    let session = load_upload_session(state, team_scope(team_id, user_id), upload_id).await?;
    upload_chunk_impl(state, session, chunk_number, data).await
}

pub async fn upload_chunk_payload_for_team(
    state: &PrimaryAppState,
    team_id: i64,
    upload_id: &str,
    chunk_number: i32,
    user_id: i64,
    payload: actix_web::web::Payload,
) -> Result<ChunkUploadResponse> {
    let session = load_upload_session(state, team_scope(team_id, user_id), upload_id).await?;
    upload_chunk_payload_impl(state, session, chunk_number, payload).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::FromRequest;
    use std::time::Duration;

    struct PendingMultipart;

    #[async_trait::async_trait]
    impl crate::storage::MultipartStorageDriver for PendingMultipart {
        async fn create_multipart_upload(&self, _path: &str) -> Result<String> {
            panic!("not used")
        }

        async fn presigned_upload_part_url(
            &self,
            _path: &str,
            _upload_id: &str,
            _part_number: i32,
            _expires: Duration,
        ) -> Result<String> {
            panic!("not used")
        }

        async fn complete_multipart_upload(
            &self,
            _path: &str,
            _upload_id: &str,
            _parts: Vec<(i32, String)>,
        ) -> Result<()> {
            panic!("not used")
        }

        async fn upload_multipart_part(
            &self,
            _path: &str,
            _upload_id: &str,
            _part_number: i32,
            _data: &[u8],
        ) -> Result<String> {
            panic!("not used")
        }

        async fn upload_multipart_part_reader(
            &self,
            _path: &str,
            _upload_id: &str,
            _part_number: i32,
            _reader: Box<dyn tokio::io::AsyncRead + Unpin + Send + Sync>,
            _size: i64,
        ) -> Result<String> {
            futures::future::pending().await
        }

        async fn abort_multipart_upload(&self, _path: &str, _upload_id: &str) -> Result<()> {
            panic!("not used")
        }

        async fn list_uploaded_part_details(
            &self,
            _path: &str,
            _upload_id: &str,
        ) -> Result<Vec<crate::storage::traits::UploadedMultipartPart>> {
            panic!("not used")
        }
    }

    async fn payload_from_bytes(data: &'static [u8]) -> actix_web::web::Payload {
        let (req, mut dev_payload) = actix_web::test::TestRequest::default()
            .set_payload(bytes::Bytes::from_static(data))
            .to_http_parts();
        actix_web::web::Payload::from_request(&req, &mut dev_payload)
            .await
            .expect("test payload should extract")
    }

    #[tokio::test]
    async fn multipart_payload_error_returns_without_waiting_for_upload_future() {
        let multipart = PendingMultipart;
        let payload = payload_from_bytes(b"too-large").await;

        let result = tokio::time::timeout(
            Duration::from_millis(100),
            upload_multipart_part_payload(&multipart, "tmp", "multipart-id", 1, payload, 4, 0),
        )
        .await
        .expect("payload validation error should not wait for the upload future");

        let err = result.expect_err("oversized payload should fail");
        assert_eq!(
            err.api_error_code_override(),
            Some(ApiErrorCode::UploadChunkTooLarge)
        );
    }

    #[test]
    fn multipart_result_priority_prefers_payload_validation_errors() {
        let upload_error = validation_error_with_code(
            ApiErrorCode::UploadStatusConflict,
            "provider upload failed",
        );
        let writer_error = chunk_body_size_mismatch(0, 4, 3);

        let err = prioritize_multipart_part_results(Err(upload_error), Err(writer_error))
            .expect_err("combined errors should fail");

        assert_eq!(
            err.api_error_code_override(),
            Some(ApiErrorCode::UploadChunkSizeMismatch)
        );
    }

    #[test]
    fn multipart_result_priority_prefers_upload_error_for_relay_failures() {
        let upload_error = validation_error_with_code(
            ApiErrorCode::UploadStatusConflict,
            "provider upload failed",
        );
        let writer_error = chunk_upload_error_with_code(
            ApiErrorCode::UploadChunkRelayFailed,
            "duplex relay failed",
        );

        let err = prioritize_multipart_part_results(Err(upload_error), Err(writer_error))
            .expect_err("combined errors should fail");

        assert_eq!(
            err.api_error_code_override(),
            Some(ApiErrorCode::UploadStatusConflict)
        );
    }
}
