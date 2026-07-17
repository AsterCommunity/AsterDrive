//! Server-managed chunked-upload completion.
//!
//! `.offset-staging-v1` is the explicit format discriminator for current sessions. The generic
//! `assembled` path belongs only to the deprecated payload-per-chunk compatibility path and may
//! survive a retryable storage/DB failure; its presence must never select offset-staging validation.

use aster_forge_db::transaction;
use chrono::Utc;
use std::time::Instant;

use crate::api::api_error_code::ApiErrorCode;
use crate::db::repository::{file_repo, upload_session_part_repo};
use crate::entities::{file, storage_policy, upload_session};
use crate::errors::{AsterError, MapAsterErr, Result, upload_assembly_error_with_code};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::files::upload::shared::{
    cleanup_upload_temp_dir, expected_chunk_size_for_upload, run_upload_completion_stage,
};
use crate::services::files::upload::staging;
use crate::services::workspace::storage;
use crate::storage::StorageDriver;
use crate::storage::connectors::resolve_policy_upload_transport;
use crate::types::{UploadSessionKind, UploadSessionStatus};
use aster_forge_utils::numbers::{i32_to_usize, i64_to_u64, usize_to_i64};
use aster_forge_utils::paths;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;

use super::contract::{
    VerifiedUploadSource, VerifiedUploadedBlob, cleanup_verified_upload_after_db_failure,
};

struct ChunkedTempFile {
    path: String,
    size: i64,
    file_hash: Option<String>,
}

fn warn_legacy_chunk_file_completion(session: &upload_session::Model, completion_mode: &str) {
    tracing::warn!(
        upload_id = %session.id,
        completion_mode,
        removal_version = "0.5.0",
        "using deprecated legacy per-chunk file completion path"
    );
}

pub(super) async fn complete_chunked_upload_with_actor_username(
    state: &PrimaryAppState,
    session: upload_session::Model,
    session_kind: UploadSessionKind,
    actor_username: Option<&str>,
) -> Result<file::Model> {
    let db = state.writer_db();
    let created = run_upload_completion_stage(
        db,
        &session,
        UploadSessionStatus::Uploading,
        "completed upload session",
        async {
            let policy = state
                .policy_snapshot()
                .get_policy_or_err(session.policy_id)?;
            let driver = state.driver_registry().get_driver(&policy)?;
            finalize_chunked_upload_session(
                state,
                &session,
                &policy,
                driver.as_ref(),
                session_kind,
                actor_username,
            )
            .await
        },
    )
    .await?;
    cleanup_upload_temp_dir(state, &session.id).await;
    Ok(created)
}

async fn finalize_chunked_upload_session(
    state: &PrimaryAppState,
    session: &upload_session::Model,
    policy: &storage_policy::Model,
    driver: &dyn StorageDriver,
    session_kind: UploadSessionKind,
    actor_username: Option<&str>,
) -> Result<file::Model> {
    // Only the pre-migration legacy kind may consult the connector fallback. New sessions already
    // carry an explicit execution plan, so capability probing cannot redirect their completion.
    let legacy_stream_relay = session_kind == UploadSessionKind::LegacyChunkFiles
        && resolve_policy_upload_transport(policy)?.chunked_completion()
            == crate::storage::connectors::StorageConnectorChunkedCompletion::RelayLocalChunksToStreamUpload;
    if matches!(session_kind, UploadSessionKind::StreamStaging) || legacy_stream_relay {
        return finalize_stream_relay_chunked_upload_session(
            state,
            session,
            policy,
            driver,
            session_kind,
            actor_username,
        )
        .await;
    }

    let prepare_started_at = Instant::now();
    let should_dedup = storage::local_content_dedup_enabled(policy);
    let (chunked_temp, used_offset_staging, legacy_assembly_wait_elapsed_ms) = if let Some(staged) =
        load_offset_staging_file(state, session, session_kind, should_dedup).await?
    {
        (staged, true, 0)
    } else {
        warn_legacy_chunk_file_completion(session, "assemble_to_local_temp_file");
        let assembly_wait_started_at = Instant::now();
        let assembly_permit = state
            .upload_runtime
            .acquire_chunk_assembly_to_local_temp_file()
            .await?;
        let assembly_wait_elapsed_ms = assembly_wait_started_at.elapsed().as_millis();
        let assembled =
            assemble_legacy_local_chunks_to_temp_file(state, session, should_dedup).await?;
        drop(assembly_permit);
        (assembled, false, assembly_wait_elapsed_ms)
    };
    let prepare_elapsed_ms = prepare_started_at.elapsed().as_millis();

    let stage_started_at = Instant::now();
    let staged_size = chunked_temp.size;
    let verified = stage_chunked_temp_file(driver, policy, chunked_temp).await?;
    let stage_elapsed_ms = stage_started_at.elapsed().as_millis();

    let persist_started_at = Instant::now();
    persist_chunked_upload(state, session, driver, &verified, actor_username)
        .await
        .inspect(|file| {
            tracing::debug!(
                upload_id = %session.id,
                file_id = file.id,
                size = staged_size,
                used_offset_staging,
                legacy_assembly_wait_elapsed_ms,
                prepare_elapsed_ms,
                stage_elapsed_ms,
                persist_elapsed_ms = persist_started_at.elapsed().as_millis(),
                "local chunked upload finalized"
            );
        })
}

async fn validate_offset_staging_file(
    state: &PrimaryAppState,
    session: &upload_session::Model,
) -> Result<String> {
    let receipts =
        upload_session_part_repo::list_all_by_upload(state.writer_db(), &session.id).await?;
    let expected_receipt_count = i32_to_usize(session.total_chunks, "total chunk count")?;
    if receipts.len() != expected_receipt_count {
        return Err(upload_assembly_error_with_code(
            ApiErrorCode::UploadAssemblyIoFailed,
            format!(
                "offset staging receipt count mismatch: expected {}, got {}",
                session.total_chunks,
                receipts.len()
            ),
        ));
    }

    for (chunk_number, receipt) in (0..session.total_chunks).zip(&receipts) {
        let expected_size = expected_chunk_size_for_upload(session, chunk_number)?;
        if !staging::chunk_receipt_matches(receipt, chunk_number + 1, expected_size) {
            return Err(upload_assembly_error_with_code(
                ApiErrorCode::UploadAssemblyIoFailed,
                format!(
                    "offset staging receipt is invalid for chunk {chunk_number}: part_number={}, etag={}, size={}, expected_size={expected_size}",
                    receipt.part_number, receipt.etag, receipt.size
                ),
            ));
        }
    }

    let path = staging::file_path(state, &session.id);
    let metadata = tokio::fs::metadata(&path)
        .await
        .map_aster_err_ctx("stat chunk staging file", |message| {
            upload_assembly_error_with_code(ApiErrorCode::UploadAssemblyIoFailed, message)
        })?;
    let expected_size = i64_to_u64(session.total_size, "chunk staging total size")?;
    if !metadata.is_file() || metadata.len() != expected_size {
        return Err(upload_assembly_error_with_code(
            ApiErrorCode::UploadAssemblyIoFailed,
            format!(
                "chunk staging file size mismatch: expected {expected_size}, got {}",
                metadata.len()
            ),
        ));
    }
    Ok(path)
}

async fn load_offset_staging_file(
    state: &PrimaryAppState,
    session: &upload_session::Model,
    session_kind: UploadSessionKind,
    should_dedup: bool,
) -> Result<Option<ChunkedTempFile>> {
    if !matches!(
        session_kind,
        UploadSessionKind::OffsetStaging | UploadSessionKind::StreamStaging
    ) {
        return Ok(None);
    }

    // A staging kind is an explicit execution contract. Missing or malformed staging must fail
    // the session rather than silently switching to the legacy `chunk_N` assembly path.
    let path = validate_offset_staging_file(state, session).await?;
    let file_hash = if should_dedup {
        Some(hash_staging_file(&path).await?)
    } else {
        None
    };
    Ok(Some(ChunkedTempFile {
        path,
        size: session.total_size,
        file_hash,
    }))
}

async fn hash_staging_file(path: &str) -> Result<String> {
    use sha2::{Digest, Sha256};

    const HASH_BUFFER_SIZE: usize = 64 * 1024;

    let mut file = tokio::fs::File::open(path)
        .await
        .map_aster_err_ctx("open chunk staging file for hashing", |message| {
            upload_assembly_error_with_code(ApiErrorCode::UploadAssemblyIoFailed, message)
        })?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; HASH_BUFFER_SIZE];
    loop {
        let read = file.read(&mut buffer).await.map_aster_err_ctx(
            "read chunk staging file for hashing",
            |message| {
                upload_assembly_error_with_code(ApiErrorCode::UploadAssemblyIoFailed, message)
            },
        )?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(aster_forge_crypto::sha256_digest_to_hex(&hasher.finalize()))
}

async fn finalize_stream_relay_chunked_upload_session(
    state: &PrimaryAppState,
    session: &upload_session::Model,
    policy: &storage_policy::Model,
    driver: &dyn StorageDriver,
    session_kind: UploadSessionKind,
    actor_username: Option<&str>,
) -> Result<file::Model> {
    if session_kind == UploadSessionKind::StreamStaging {
        return finalize_offset_staging_stream_relay(
            state,
            session,
            policy,
            driver,
            actor_username,
        )
        .await;
    }

    warn_legacy_chunk_file_completion(session, "stream_local_chunk_files");

    const CHUNK_RELAY_BUFFER_SIZE: usize = 64 * 1024;

    let prepared = storage::prepare_non_dedup_blob_upload(policy, session.total_size)?;
    let (writer, reader) = tokio::io::duplex(CHUNK_RELAY_BUFFER_SIZE);
    let relay_task = tokio::spawn(stream_legacy_local_chunks_into_writer(
        state.config().server.upload_temp_dir.clone(),
        session.id.clone(),
        session.total_chunks,
        writer,
    ));

    let upload_started_at = Instant::now();
    let upload_result = storage::upload_reader_to_prepared_blob(
        driver,
        &prepared,
        Box::new(reader),
        session.total_size,
    )
    .await;
    let upload_elapsed_ms = upload_started_at.elapsed().as_millis();

    let relay_result = relay_task.await.map_err(|error| {
        upload_assembly_error_with_code(
            ApiErrorCode::UploadChunkRelayFailed,
            format!("stream chunk relay task failed: {error}"),
        )
    })?;

    if let Err(error) = upload_result {
        storage::cleanup_preuploaded_blob_upload(
            driver,
            &prepared,
            "chunked upload storage write error",
        )
        .await;
        return Err(error);
    }
    if let Err(error) = relay_result {
        storage::cleanup_preuploaded_blob_upload(driver, &prepared, "chunked upload relay error")
            .await;
        return Err(error);
    }

    let persist_started_at = Instant::now();
    let verified = VerifiedUploadedBlob::preuploaded_non_dedup(prepared)?;
    persist_verified_chunked_upload(state, session, driver, &verified, actor_username)
        .await
        .inspect(|file| {
            tracing::debug!(
                upload_id = %session.id,
                file_id = file.id,
                size = session.total_size,
                upload_elapsed_ms,
                persist_elapsed_ms = persist_started_at.elapsed().as_millis(),
                "stream relay chunked upload finalized"
            );
        })
}

async fn finalize_offset_staging_stream_relay(
    state: &PrimaryAppState,
    session: &upload_session::Model,
    policy: &storage_policy::Model,
    driver: &dyn StorageDriver,
    actor_username: Option<&str>,
) -> Result<file::Model> {
    let path = validate_offset_staging_file(state, session).await?;
    let reader = tokio::fs::File::open(&path)
        .await
        .map_aster_err_ctx("open chunk staging file for stream upload", |message| {
            upload_assembly_error_with_code(ApiErrorCode::UploadAssemblyIoFailed, message)
        })?;
    let prepared = storage::prepare_non_dedup_blob_upload(policy, session.total_size)?;
    let upload_started_at = Instant::now();
    if let Err(error) = storage::upload_reader_to_prepared_blob(
        driver,
        &prepared,
        Box::new(reader),
        session.total_size,
    )
    .await
    {
        storage::cleanup_preuploaded_blob_upload(
            driver,
            &prepared,
            "chunk staging stream upload storage write error",
        )
        .await;
        return Err(error);
    }
    let upload_elapsed_ms = upload_started_at.elapsed().as_millis();

    let persist_started_at = Instant::now();
    let verified = VerifiedUploadedBlob::preuploaded_non_dedup(prepared)?;
    persist_verified_chunked_upload(state, session, driver, &verified, actor_username)
        .await
        .inspect(|file| {
            tracing::debug!(
                upload_id = %session.id,
                file_id = file.id,
                size = session.total_size,
                upload_elapsed_ms,
                persist_elapsed_ms = persist_started_at.elapsed().as_millis(),
                "offset staging stream relay chunked upload finalized"
            );
        })
}

/// Compatibility path for sessions created before offset staging was introduced.
/// Scheduled for removal in 0.5.0 after all 24-hour upload sessions have expired.
async fn stream_legacy_local_chunks_into_writer(
    upload_temp_dir: String,
    upload_id: String,
    total_chunks: i32,
    mut writer: tokio::io::DuplexStream,
) -> Result<()> {
    const STREAM_BUFFER_SIZE: usize = 64 * 1024;

    let mut buffer = vec![0u8; STREAM_BUFFER_SIZE];
    for chunk_number in 0..total_chunks {
        let chunk_path = paths::upload_chunk_path(&upload_temp_dir, &upload_id, chunk_number);
        let mut chunk_file = tokio::fs::File::open(&chunk_path).await.map_aster_err_ctx(
            &format!("open chunk {chunk_number}"),
            |message| {
                upload_assembly_error_with_code(ApiErrorCode::UploadAssemblyIoFailed, message)
            },
        )?;

        loop {
            let read = chunk_file.read(&mut buffer).await.map_aster_err_ctx(
                &format!("read chunk {chunk_number}"),
                |message| {
                    upload_assembly_error_with_code(ApiErrorCode::UploadAssemblyIoFailed, message)
                },
            )?;
            if read == 0 {
                break;
            }
            writer.write_all(&buffer[..read]).await.map_aster_err_ctx(
                "relay upload chunk",
                |message| {
                    upload_assembly_error_with_code(ApiErrorCode::UploadChunkRelayFailed, message)
                },
            )?;
        }
    }

    writer
        .shutdown()
        .await
        .map_aster_err_ctx("shutdown stream chunk relay", |message| {
            upload_assembly_error_with_code(ApiErrorCode::UploadChunkRelayFailed, message)
        })?;
    Ok(())
}

/// Compatibility path for sessions created before offset staging was introduced.
/// Scheduled for removal in 0.5.0 after all 24-hour upload sessions have expired.
async fn assemble_legacy_local_chunks_to_temp_file(
    state: &PrimaryAppState,
    session: &upload_session::Model,
    should_dedup: bool,
) -> Result<ChunkedTempFile> {
    use sha2::{Digest, Sha256};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    const ASSEMBLY_BUFFER_SIZE: usize = 64 * 1024;

    let upload_id = session.id.as_str();
    let assembled_path =
        paths::upload_assembled_path(&state.config().server.upload_temp_dir, upload_id);
    let mut out_file = tokio::fs::File::create(&assembled_path)
        .await
        .map_aster_err_ctx("create assembled file", |message| {
            upload_assembly_error_with_code(ApiErrorCode::UploadAssemblyIoFailed, message)
        })?;
    let mut hasher = should_dedup.then(Sha256::new);
    let mut size: i64 = 0;
    let mut buffer = vec![0u8; ASSEMBLY_BUFFER_SIZE];

    // 本地 chunk 模式：先按顺序把所有 chunk 拼成 assembled 文件。
    // 如果 local 策略启用了 dedup，会在拼装过程中顺便流式计算 hash，
    // 避免第二遍再把 assembled 文件完整读一遍。
    for i in 0..session.total_chunks {
        let chunk_path =
            paths::upload_chunk_path(&state.config().server.upload_temp_dir, upload_id, i);
        let mut chunk_file = tokio::fs::File::open(&chunk_path).await.map_aster_err_ctx(
            &format!("open chunk {i}"),
            |message| {
                upload_assembly_error_with_code(ApiErrorCode::UploadAssemblyIoFailed, message)
            },
        )?;

        loop {
            let n = chunk_file.read(&mut buffer).await.map_aster_err_ctx(
                &format!("read chunk {i}"),
                |message| {
                    upload_assembly_error_with_code(ApiErrorCode::UploadAssemblyIoFailed, message)
                },
            )?;
            if n == 0 {
                break;
            }

            let data = &buffer[..n];
            if let Some(hasher) = hasher.as_mut() {
                hasher.update(data);
            }
            let chunk_len = usize_to_i64(n, "assembled chunk length")?;
            size = size.checked_add(chunk_len).ok_or_else(|| {
                upload_assembly_error_with_code(
                    ApiErrorCode::UploadAssemblySizeOverflow,
                    "assembled upload size exceeds i64 range",
                )
            })?;
            out_file
                .write_all(data)
                .await
                .map_aster_err_ctx("write assembled", |message| {
                    upload_assembly_error_with_code(ApiErrorCode::UploadAssemblyIoFailed, message)
                })?;
        }
    }
    out_file
        .flush()
        .await
        .map_aster_err_ctx("flush assembled", |message| {
            upload_assembly_error_with_code(ApiErrorCode::UploadAssemblyIoFailed, message)
        })?;
    drop(out_file);

    Ok(ChunkedTempFile {
        path: assembled_path,
        size,
        file_hash: hasher
            .map(|hasher| aster_forge_crypto::sha256_digest_to_hex(&hasher.finalize())),
    })
}

async fn stage_chunked_temp_file(
    driver: &dyn StorageDriver,
    policy: &storage_policy::Model,
    chunked_temp: ChunkedTempFile,
) -> Result<VerifiedUploadedBlob> {
    let ChunkedTempFile {
        path,
        size,
        file_hash,
    } = chunked_temp;
    if let Some(file_hash) = file_hash {
        let storage_path =
            aster_forge_validation::filename::storage_path_from_blob_key(&file_hash)?;
        crate::storage::drivers::local::promote_local_file_if_absent(
            driver,
            &storage_path,
            &path,
            size,
        )
        .await?;

        return VerifiedUploadedBlob::deduplicated_content(
            size,
            policy.id,
            storage_path,
            file_hash,
        );
    }

    // 不做 dedup 的情况下，先为 blob 预分配最终 key，再把 staging 文件传上去。
    // DB finalize 失败后的清理归属由 VerifiedUploadedBlob 的 cleanup plan 表达。
    let preuploaded = storage::prepare_non_dedup_blob_upload(policy, size)?;
    storage::upload_temp_file_to_prepared_blob(driver, &preuploaded, &path).await?;
    VerifiedUploadedBlob::preuploaded_non_dedup(preuploaded)
}

async fn persist_chunked_upload(
    state: &PrimaryAppState,
    session: &upload_session::Model,
    driver: &dyn StorageDriver,
    verified: &VerifiedUploadedBlob,
    actor_username: Option<&str>,
) -> Result<file::Model> {
    let now = Utc::now();
    let mut deadlock_attempt = 0;
    let create_result = loop {
        let txn = transaction::begin(state.writer_db()).await?;
        let result = async {
            storage::lock_storage_usage(&txn, workspace_scope_from_session(session)).await?;

            let blob = match verified.source() {
                VerifiedUploadSource::ContentAddressed { file_hash }
                | VerifiedUploadSource::OpaqueObject { file_hash } => {
                    file_repo::find_or_create_blob(
                        &txn,
                        file_hash,
                        verified.size(),
                        verified.policy_id(),
                        verified.storage_path(),
                    )
                    .await?
                    .model
                }
                VerifiedUploadSource::PreuploadedNonDedup { prepared } => {
                    storage::persist_preuploaded_blob(&txn, prepared).await?
                }
            };

            let created = storage::finalize_upload_session_blob_with_actor_username(
                &txn,
                session,
                &blob,
                now,
                actor_username,
            )
            .await?;

            Ok::<file::Model, AsterError>(created)
        }
        .await;
        match result {
            Ok(created) => {
                transaction::commit(txn).await?;
                break Ok(created);
            }
            Err(error) => {
                if let Err(rollback_error) = transaction::rollback(txn).await {
                    tracing::warn!(callback_error = %error, rollback_error = %rollback_error, "storage transaction rollback failed");
                }
                if storage::retry_mysql_deadlock(state.writer_db(), deadlock_attempt, &error).await
                {
                    deadlock_attempt += 1;
                    continue;
                }
                break Err(error);
            }
        }
    };

    match create_result {
        Ok(created) => Ok(created),
        Err(error) => {
            cleanup_verified_upload_after_db_failure(
                driver,
                verified,
                "chunked upload DB error after storing staged blob",
            )
            .await;
            // dedup 失败不主动删 storage 对象：另一路并发上传可能正在引用同内容的 blob，
            // 删除会造成 ref=1 的活 blob 丢数据；留给 orphan-blob GC 处理。
            Err(error)
        }
    }
}

async fn persist_verified_chunked_upload(
    state: &PrimaryAppState,
    session: &upload_session::Model,
    driver: &dyn StorageDriver,
    verified: &VerifiedUploadedBlob,
    actor_username: Option<&str>,
) -> Result<file::Model> {
    let now = Utc::now();
    let mut deadlock_attempt = 0;
    let create_result = loop {
        let txn = transaction::begin(state.writer_db()).await?;
        let result = async {
            storage::lock_storage_usage(&txn, workspace_scope_from_session(session)).await?;
            let blob = match verified.source() {
                VerifiedUploadSource::PreuploadedNonDedup { prepared } => {
                    storage::persist_preuploaded_blob(&txn, prepared).await?
                }
                VerifiedUploadSource::ContentAddressed { .. }
                | VerifiedUploadSource::OpaqueObject { .. } => {
                    return Err(upload_assembly_error_with_code(
                        ApiErrorCode::UploadSessionCorrupted,
                        "stream relay chunked upload expected preuploaded blob",
                    ));
                }
            };
            let created = storage::finalize_upload_session_blob_with_actor_username(
                &txn,
                session,
                &blob,
                now,
                actor_username,
            )
            .await?;
            Ok::<file::Model, AsterError>(created)
        }
        .await;
        match result {
            Ok(created) => {
                transaction::commit(txn).await?;
                break Ok(created);
            }
            Err(error) => {
                if let Err(rollback_error) = transaction::rollback(txn).await {
                    tracing::warn!(callback_error = %error, rollback_error = %rollback_error, "storage transaction rollback failed");
                }
                if storage::retry_mysql_deadlock(state.writer_db(), deadlock_attempt, &error).await
                {
                    deadlock_attempt += 1;
                    continue;
                }
                break Err(error);
            }
        }
    };

    match create_result {
        Ok(created) => Ok(created),
        Err(error) => {
            cleanup_verified_upload_after_db_failure(
                driver,
                verified,
                "chunked upload DB error after streaming preuploaded blob",
            )
            .await;
            Err(error)
        }
    }
}

fn workspace_scope_from_session(session: &upload_session::Model) -> storage::WorkspaceStorageScope {
    match session.team_id {
        Some(team_id) => storage::WorkspaceStorageScope::Team {
            team_id,
            actor_user_id: session.user_id,
        },
        None => storage::WorkspaceStorageScope::Personal {
            user_id: session.user_id,
        },
    }
}
