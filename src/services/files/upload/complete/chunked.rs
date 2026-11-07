//! Server-managed chunked-upload completion.
//!
//! `.offset-staging-v1` is the explicit format discriminator for current sessions.

use chrono::Utc;
use sea_orm::DbBackend;
use std::time::Instant;

use crate::api::api_error_code::ApiErrorCode;
use crate::db::repository::{file_repo, upload_session_part_repo};
use crate::errors::{AsterError, MapAsterErr, Result, upload_assembly_error_with_code};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::files::upload::ingest::staging;
use crate::services::files::upload::session::shared::{
    cleanup_upload_temp_dir, expected_chunk_size_for_upload, run_upload_completion_stage,
};
use crate::services::workspace::storage;
use aster_drive_model::entities::{file, storage_policy, upload_session};
use aster_drive_model::types::{UploadSessionKind, UploadSessionStatus};
use aster_drive_storage::StorageDriver;
use aster_forge_utils::numbers::{i32_to_usize, i64_to_u64};
use tokio::io::AsyncReadExt;

use super::contract::{
    VerifiedUploadSource, VerifiedUploadedBlob, cleanup_unpublished_upload_candidate,
};

struct ChunkedTempFile {
    path: String,
    size: i64,
    file_hash: Option<String>,
}

#[derive(Clone)]
enum ChunkedBlobPublication {
    ExistingDedup {
        blob_id: i64,
        hash: String,
        size: i64,
        policy_id: i64,
    },
    Staged(VerifiedUploadedBlob),
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
            staging::ensure_session_reservation(state, &session).await?;
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
    let should_dedup =
        storage::local_content_dedup_enabled(state.driver_registry().connectors(), policy)?;
    let chunked_temp = aster_drive_metrics::observe_upload_phase(
        state.metrics().clone(),
        session_kind.as_str(),
        aster_drive_metrics::UploadPhase::Assemble,
        load_offset_staging_file(state, session, should_dedup),
    )
    .await?;
    let prepare_elapsed_ms = prepare_started_at.elapsed().as_millis();

    if let Some(hash) = chunked_temp.file_hash.as_deref()
        && let Some(existing) =
            file_repo::find_active_blob_by_hash(state.writer_db(), hash, policy.id).await?
        && !existing.is_virtual_empty()
        && let Some(file) = publish_chunked_upload(
            state,
            session,
            driver,
            ChunkedBlobPublication::ExistingDedup {
                blob_id: existing.id,
                hash: hash.to_owned(),
                size: chunked_temp.size,
                policy_id: policy.id,
            },
            actor_username,
        )
        .await?
    {
        return Ok(file);
    }
    let stage_started_at = Instant::now();
    let staged_size = chunked_temp.size;
    let verified = stage_chunked_temp_file(
        state.driver_registry().connectors(),
        driver,
        policy,
        &session.filename,
        chunked_temp,
    )
    .await?;
    let stage_elapsed_ms = stage_started_at.elapsed().as_millis();

    let persist_started_at = Instant::now();
    aster_drive_metrics::observe_upload_phase(
        state.metrics().clone(),
        session_kind.as_str(),
        aster_drive_metrics::UploadPhase::Finalize,
        async {
            publish_chunked_upload(
                state,
                session,
                driver,
                ChunkedBlobPublication::Staged(verified),
                actor_username,
            )
            .await?
            .ok_or_else(|| AsterError::internal_error("staged chunked upload resolved no blob"))
        },
    )
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
        state.driver_registry().connectors(),
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
    publish_chunked_upload(
        state,
        session,
        driver,
        ChunkedBlobPublication::Staged(verified),
        actor_username,
    )
    .await?
    .ok_or_else(|| AsterError::internal_error("staged stream relay resolved no blob"))
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
    registry: &crate::storage::connectors::StorageConnectorRegistry,
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
        // The hash deduplicates metadata; physical publication belongs only to this attempt.
        let storage_path = aster_drive_storage::object_path::new_local_blob().1;
        let promoted = crate::storage::drivers::local::promote_local_file_if_absent(
            driver,
            &storage_path,
            &path,
            size,
            || Ok(()),
        )
        .await?;
        if !promoted.created() {
            return Err(AsterError::conflict(
                "upload candidate storage key already exists",
            ));
        }

        return VerifiedUploadedBlob::deduplicated_candidate(
            size,
            policy.id,
            storage_path,
            file_hash,
        );
    }

    // 不做 dedup 的情况下，先为 blob 预分配最终 key，再把 staging 文件传上去。
    // DB finalize 失败后的清理归属由 VerifiedUploadedBlob 的 cleanup plan 表达。
    let preuploaded =
        storage::prepare_non_dedup_blob_upload(registry, policy, size, Some(filename))?;
    storage::upload_temp_file_to_prepared_blob(driver, &preuploaded, &path).await?;
    VerifiedUploadedBlob::preuploaded_non_dedup(preuploaded)
}

async fn publish_chunked_upload(
    state: &PrimaryAppState,
    session: &upload_session::Model,
    driver: &dyn StorageDriver,
    publication_source: ChunkedBlobPublication,
    actor_username: Option<&str>,
) -> Result<Option<file::Model>> {
    let now = Utc::now();
    let retry_on_mysql_deadlock = state.writer_db().get_database_backend() == DbBackend::MySql;
    let transaction_session = session.clone();
    let transaction_source = publication_source.clone();
    let transaction_actor_username = actor_username.map(str::to_owned);
    let transaction_now = now;
    let create_result = aster_forge_db::transaction::with_transaction_retry(
        state.writer_db(),
        &aster_forge_db::retry::RetryConfig::deadlock(),
        move |txn| {
            let session = transaction_session.clone();
            let publication_source = transaction_source.clone();
            let actor_username = transaction_actor_username.clone();
            let now = transaction_now;
            Box::pin(async move {
                crate::services::workspace::scope::authorize_workspace_on(
                    txn,
                    workspace_scope_from_session(&session),
                    crate::services::workspace::scope::WorkspaceOperation::CreateFile,
                    &crate::services::workspace::scope::WorkspaceAccessConstraints::default(),
                )
                .await?;
                storage::lock_storage_usage(txn, workspace_scope_from_session(&session)).await?;

                let blob = match &publication_source {
                    ChunkedBlobPublication::ExistingDedup {
                        blob_id,
                        hash,
                        size,
                        policy_id,
                    } => {
                        let Some(blob) = storage::try_adopt_existing_dedup_blob_on(
                            txn, *blob_id, hash, *size, *policy_id,
                        )
                        .await?
                        else {
                            return Ok(None);
                        };
                        blob
                    }
                    ChunkedBlobPublication::Staged(verified) => match verified.source() {
                        VerifiedUploadSource::DeduplicatedContent { file_hash }
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
                    },
                };

                let created = storage::finalize_upload_session_blob_with_actor_username(
                    txn,
                    &session,
                    &blob,
                    now,
                    actor_username.as_deref(),
                )
                .await?;

                let retained_candidate = match &publication_source {
                    ChunkedBlobPublication::Staged(verified) => {
                        blob.storage_path.as_deref() == Some(verified.storage_path())
                    }
                    ChunkedBlobPublication::ExistingDedup { .. } => false,
                };
                Ok::<Option<(file::Model, bool)>, AsterError>(Some((created, retained_candidate)))
            })
        },
        move |error: &AsterError| {
            retry_on_mysql_deadlock
                && error.database_error_kind() == Some(aster_forge_db::DatabaseErrorKind::Deadlock)
        },
    )
    .await;

    match create_result {
        Ok(Some((created, retained_candidate))) => {
            match (&publication_source, retained_candidate) {
                (ChunkedBlobPublication::Staged(verified), false) => {
                    cleanup_unpublished_upload_candidate(
                        driver,
                        verified,
                        "dedup candidate was not selected for publication",
                    )
                    .await;
                }
                (ChunkedBlobPublication::Staged(_), true)
                | (ChunkedBlobPublication::ExistingDedup { .. }, _) => {}
            }
            Ok(Some(created))
        }
        Ok(None) => Ok(None),
        Err(error) => {
            if !error.database_commit_outcome_uncertain() {
                match &publication_source {
                    ChunkedBlobPublication::Staged(verified) => {
                        cleanup_unpublished_upload_candidate(
                            driver,
                            verified,
                            "chunked upload DB error after storing staged blob",
                        )
                        .await;
                    }
                    ChunkedBlobPublication::ExistingDedup { .. } => {}
                }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn chunked_dedup_candidates_are_private_and_preserve_source_for_retry() {
        let root = aster_forge_utils::raii::TempDirGuard::new(
            std::env::temp_dir().join(uuid::Uuid::new_v4().to_string()),
            "chunked private candidate test",
        );
        tokio::fs::create_dir_all(root.path()).await.unwrap();
        let base = root.path().join("objects");
        let policy = crate::storage::connectors::test_support::local_policy(base.to_string_lossy());
        let driver =
            crate::storage::drivers::local::LocalDriver::new(base.to_str().unwrap()).unwrap();
        let registry = crate::storage::DriverRegistry::noop().unwrap();
        let payload = b"two chunked uploads with identical logical content";
        let hash = aster_forge_crypto::sha256_hex(payload);
        let mut candidates = Vec::new();
        for name in ["first.bin", "second.bin"] {
            let source = root.path().join(name);
            tokio::fs::write(&source, payload).await.unwrap();
            let candidate = stage_chunked_temp_file(
                registry.connectors(),
                &driver,
                &policy,
                name,
                ChunkedTempFile {
                    path: source.to_string_lossy().into_owned(),
                    size: payload.len() as i64,
                    file_hash: Some(hash.clone()),
                },
            )
            .await
            .unwrap();
            assert!(
                source.exists(),
                "metadata failure must leave source bytes available to retry"
            );
            candidates.push(candidate);
        }
        assert_ne!(candidates[0].storage_path(), candidates[1].storage_path());
        cleanup_unpublished_upload_candidate(&driver, &candidates[0], "failed publisher").await;
        assert!(!driver.exists(candidates[0].storage_path()).await.unwrap());
        assert_eq!(
            driver.get(candidates[1].storage_path()).await.unwrap(),
            payload
        );
    }
}
