//! Server-managed chunked-upload completion.
//!
//! `.offset-staging-v1` is the explicit format discriminator for current sessions.

use chrono::Utc;
use sea_orm::DbBackend;
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
use crate::types::{UploadSessionKind, UploadSessionStatus};
use aster_forge_utils::numbers::{i32_to_usize, i64_to_u64};
use tokio::io::AsyncReadExt;

use super::contract::{
    VerifiedUploadSource, VerifiedUploadedBlob, cleanup_verified_upload_after_db_failure,
};

struct ChunkedTempFile {
    path: String,
    size: i64,
    file_hash: Option<String>,
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
    if session_kind != UploadSessionKind::OffsetStaging {
        return Err(upload_assembly_error_with_code(
            ApiErrorCode::UploadSessionCorrupted,
            "chunked completion requires an offset or stream staging session",
        ));
    }

    let prepare_started_at = Instant::now();
    let should_dedup = storage::local_content_dedup_enabled(policy);
    let chunked_temp = load_offset_staging_file(state, session, should_dedup).await?;
    let prepare_elapsed_ms = prepare_started_at.elapsed().as_millis();

    let stage_started_at = Instant::now();
    let staged_size = chunked_temp.size;
    let verified = stage_chunked_temp_file(driver, policy, &session.filename, chunked_temp).await?;
    let stage_elapsed_ms = stage_started_at.elapsed().as_millis();

    let persist_started_at = Instant::now();
    persist_chunked_upload(state, session, driver, &verified, actor_username)
        .await
        .inspect(|file| {
            tracing::debug!(
                upload_id = %session.id,
                file_id = file.id,
                size = staged_size,
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
    should_dedup: bool,
) -> Result<ChunkedTempFile> {
    let path = validate_offset_staging_file(state, session).await?;
    let file_hash = if should_dedup {
        Some(hash_staging_file(&path).await?)
    } else {
        None
    };
    Ok(ChunkedTempFile {
        path,
        size: session.total_size,
        file_hash,
    })
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
    let prepared = storage::prepare_non_dedup_blob_upload(
        policy,
        session.total_size,
        Some(&session.filename),
    )?;
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

async fn stage_chunked_temp_file(
    driver: &dyn StorageDriver,
    policy: &storage_policy::Model,
    filename: &str,
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
    let preuploaded = storage::prepare_non_dedup_blob_upload(policy, size, Some(filename))?;
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
    let retry_on_mysql_deadlock = state.writer_db().get_database_backend() == DbBackend::MySql;
    let transaction_session = session.clone();
    let transaction_verified = verified.clone();
    let transaction_actor_username = actor_username.map(str::to_owned);
    let transaction_now = now;
    let create_result = aster_forge_db::transaction::with_transaction_retry(
        state.writer_db(),
        &aster_forge_db::retry::RetryConfig::deadlock(),
        move |txn| {
            let session = transaction_session.clone();
            let verified = transaction_verified.clone();
            let actor_username = transaction_actor_username.clone();
            let now = transaction_now;
            Box::pin(async move {
                storage::lock_storage_usage(txn, workspace_scope_from_session(&session)).await?;

                let blob = match verified.source() {
                    VerifiedUploadSource::ContentAddressed { file_hash }
                    | VerifiedUploadSource::OpaqueObject { file_hash } => {
                        file_repo::find_or_create_blob(
                            txn,
                            file_hash,
                            verified.size(),
                            verified.policy_id(),
                            verified.storage_path(),
                        )
                        .await?
                        .model
                    }
                    VerifiedUploadSource::PreuploadedNonDedup { prepared } => {
                        storage::persist_preuploaded_blob(txn, prepared).await?
                    }
                };

                let created = storage::finalize_upload_session_blob_with_actor_username(
                    txn,
                    &session,
                    &blob,
                    now,
                    actor_username.as_deref(),
                )
                .await?;

                Ok::<file::Model, AsterError>(created)
            })
        },
        move |error: &AsterError| {
            retry_on_mysql_deadlock
                && error.database_error_kind() == Some(aster_forge_db::DatabaseErrorKind::Deadlock)
        },
    )
    .await;

    match create_result {
        Ok(created) => Ok(created),
        Err(error) => {
            if !error.database_commit_outcome_uncertain() {
                cleanup_verified_upload_after_db_failure(
                    driver,
                    verified,
                    "chunked upload DB error after storing staged blob",
                )
                .await;
            }
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
    let retry_on_mysql_deadlock = state.writer_db().get_database_backend() == DbBackend::MySql;
    let transaction_session = session.clone();
    let transaction_verified = verified.clone();
    let transaction_actor_username = actor_username.map(str::to_owned);
    let transaction_now = now;
    let create_result = aster_forge_db::transaction::with_transaction_retry(
        state.writer_db(),
        &aster_forge_db::retry::RetryConfig::deadlock(),
        move |txn| {
            let session = transaction_session.clone();
            let verified = transaction_verified.clone();
            let actor_username = transaction_actor_username.clone();
            let now = transaction_now;
            Box::pin(async move {
                storage::lock_storage_usage(txn, workspace_scope_from_session(&session)).await?;
                let blob = match verified.source() {
                    VerifiedUploadSource::PreuploadedNonDedup { prepared } => {
                        storage::persist_preuploaded_blob(txn, prepared).await?
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
                    txn,
                    &session,
                    &blob,
                    now,
                    actor_username.as_deref(),
                )
                .await?;
                Ok::<file::Model, AsterError>(created)
            })
        },
        move |error: &AsterError| {
            retry_on_mysql_deadlock
                && error.database_error_kind() == Some(aster_forge_db::DatabaseErrorKind::Deadlock)
        },
    )
    .await;

    match create_result {
        Ok(created) => Ok(created),
        Err(error) => {
            if !error.database_commit_outcome_uncertain() {
                cleanup_verified_upload_after_db_failure(
                    driver,
                    verified,
                    "chunked upload DB error after streaming preuploaded blob",
                )
                .await;
            }
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
