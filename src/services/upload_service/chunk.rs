//! 分片上传阶段。
//!
//! 这里处理两类“已经进入分片模式”的 session：
//! - 服务端本地暂存 chunk 文件
//! - 服务端 relay 到 S3 multipart，并把 ETag 记入 upload_session_parts

use bytes::Bytes;
use chrono::Utc;

use crate::db::repository::{upload_session_part_repo, upload_session_repo};
use crate::entities::upload_session;
use crate::errors::{
    AsterError, MapAsterErr, Result, chunk_upload_error_with_subcode, validation_error_with_subcode,
};
use crate::runtime::PrimaryAppState;
use crate::services::upload_service::responses::ChunkUploadResponse;
use crate::services::upload_service::scope::{load_upload_session, personal_scope, team_scope};
use crate::services::upload_service::shared::{
    expected_chunk_size_for_upload, upload_session_chunk_unavailable_error,
};
use crate::types::UploadSessionStatus;
use crate::utils::numbers::{i64_to_u64, usize_to_i64};
use crate::utils::paths;

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
            return Err(chunk_upload_error_with_subcode(
                "upload.chunk_persist_failed",
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
                chunk_upload_error_with_subcode(
                    "upload.chunk_persist_failed",
                    format!("create temp chunk file: {error}"),
                )
            })?;

        file.write_all(data)
            .await
            .map_aster_err_ctx("write chunk", |message| {
                chunk_upload_error_with_subcode("upload.chunk_persist_failed", message)
            })?;
        file.flush()
            .await
            .map_aster_err_ctx("flush chunk", |message| {
                chunk_upload_error_with_subcode("upload.chunk_persist_failed", message)
            })?;
        Ok::<(), AsterError>(())
    }
    .await;

    if write_result.is_err() {
        remove_local_chunk_file(temp_path, upload_id, chunk_number, "temp chunk write error").await;
    }

    write_result
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
                return Err(chunk_upload_error_with_subcode(
                    "upload.chunk_persist_failed",
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
    Err(chunk_upload_error_with_subcode(
        "upload.chunk_persist_failed",
        "publish chunk file: existing chunk stayed unavailable",
    ))
}

async fn upload_chunk_impl(
    state: &PrimaryAppState,
    session: upload_session::Model,
    chunk_number: i32,
    data: Bytes,
) -> Result<ChunkUploadResponse> {
    let db = &state.db;
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
        return Err(validation_error_with_subcode(
            "upload.chunk_number_out_of_range",
            format!(
                "chunk_number {} out of range [0, {})",
                chunk_number, session.total_chunks
            ),
        ));
    }

    let expected_size = expected_chunk_size_for_upload(&session, chunk_number)?;
    let data_len = usize_to_i64(data.len(), "chunk data length")?;
    if data_len != expected_size {
        return Err(chunk_upload_error_with_subcode(
            "upload.chunk_size_mismatch",
            format!("chunk {chunk_number} size mismatch: expected {expected_size}, got {data_len}"),
        ));
    }

    if let (Some(temp_key), Some(multipart_id)) = (
        session.s3_temp_key.as_deref(),
        session.s3_multipart_id.as_deref(),
    ) {
        let s3_part_number = chunk_number + 1;

        // relay multipart 下，先 claim part 再上传到 S3。
        // 否则并发重试会把同一个 part 号重复上传，最后谁的 ETag 留库就会变得不确定。
        if !upload_session_part_repo::try_claim_part(db, upload_id, s3_part_number).await? {
            let updated = upload_session_repo::find_by_id(db, upload_id).await?;
            tracing::debug!(
                upload_id,
                chunk_number,
                part_number = s3_part_number,
                received_count = updated.received_count,
                total_chunks = updated.total_chunks,
                "skipping already claimed relay multipart part"
            );
            return Ok(ChunkUploadResponse {
                received_count: updated.received_count,
                total_chunks: updated.total_chunks,
            });
        }

        let policy = state.policy_snapshot.get_policy_or_err(session.policy_id)?;
        let multipart = state.driver_registry.get_multipart_driver(&policy)?;
        let etag = match multipart
            .upload_multipart_part_bytes(temp_key, multipart_id, s3_part_number, data)
            .await
        {
            Ok(etag) => etag,
            Err(err) => {
                if let Err(cleanup_err) = upload_session_part_repo::delete_by_upload_and_part(
                    db,
                    upload_id,
                    s3_part_number,
                )
                .await
                {
                    tracing::warn!(
                        upload_id,
                        part_number = s3_part_number,
                        "failed to release relay multipart part claim after upload error: {cleanup_err}"
                    );
                }
                return Err(err);
            }
        };

        let txn = crate::db::transaction::begin(db).await?;
        let finalize_result = async {
            // S3 上传成功以后，必须把 part 元数据和 received_count 放在同一事务里提交；
            // 否则 complete 阶段会看到“不完整的 part 清单”。
            upload_session_part_repo::upsert_part(&txn, upload_id, s3_part_number, &etag, data_len)
                .await?;
            increment_session_received_count(&txn, upload_id).await?;
            crate::db::transaction::commit(txn).await?;
            Ok::<(), AsterError>(())
        }
        .await;

        if let Err(err) = finalize_result {
            if let Err(cleanup_err) =
                upload_session_part_repo::delete_by_upload_and_part(db, upload_id, s3_part_number)
                    .await
            {
                tracing::warn!(
                    upload_id,
                    part_number = s3_part_number,
                    "failed to release relay multipart part claim after DB finalize error: {cleanup_err}"
                );
            }
            return Err(err);
        }

        let updated = upload_session_repo::find_by_id(db, upload_id).await?;
        tracing::debug!(
            upload_id,
            chunk_number,
            part_number = s3_part_number,
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
        &state.config.server.upload_temp_dir,
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

    let chunk_dir = paths::upload_temp_dir(&state.config.server.upload_temp_dir, upload_id);
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
