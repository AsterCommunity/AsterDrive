use chrono::Utc;
use std::time::Instant;

use crate::api::api_error_code::ApiErrorCode;
use crate::db::repository::file_repo;
use crate::entities::{file, storage_policy, upload_session};
use crate::errors::{AsterError, MapAsterErr, Result, upload_assembly_error_with_code};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::upload_service::shared::{
    cleanup_upload_temp_dir, run_upload_completion_stage,
};
use crate::services::workspace::storage::{self, PreparedNonDedupBlobUpload};
use crate::storage::StorageDriver;
use crate::storage::connectors::{
    StorageConnectorChunkedCompletion, resolve_policy_upload_transport,
};
use crate::types::UploadSessionStatus;
use crate::utils::numbers::usize_to_i64;
use crate::utils::paths;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;

struct AssembledTempFile {
    path: String,
    size: i64,
    file_hash: Option<String>,
}

enum AssembledBlobPlan {
    Dedup {
        file_hash: String,
        storage_path: String,
    },
    Preuploaded(PreparedNonDedupBlobUpload),
}

pub(super) async fn complete_chunked_upload_with_actor_username(
    state: &PrimaryAppState,
    session: upload_session::Model,
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
    actor_username: Option<&str>,
) -> Result<file::Model> {
    if resolve_policy_upload_transport(policy)?.chunked_completion()
        == StorageConnectorChunkedCompletion::RelayLocalChunksToStreamUpload
    {
        return finalize_stream_relay_chunked_upload_session(
            state,
            session,
            policy,
            driver,
            actor_username,
        )
        .await;
    }

    let assemble_started_at = Instant::now();
    let assembled = assemble_local_chunks_to_temp_file(
        state,
        session,
        storage::local_content_dedup_enabled(policy),
    )
    .await?;
    let assemble_elapsed_ms = assemble_started_at.elapsed().as_millis();

    let stage_started_at = Instant::now();
    let assembled_size = assembled.size;
    let blob_plan = stage_assembled_blob_upload(driver, policy, assembled).await?;
    let stage_elapsed_ms = stage_started_at.elapsed().as_millis();

    let persist_started_at = Instant::now();
    persist_assembled_upload(
        state,
        session,
        driver,
        policy.id,
        assembled_size,
        &blob_plan,
        actor_username,
    )
    .await
    .inspect(|file| {
        tracing::debug!(
            upload_id = %session.id,
            file_id = file.id,
            size = assembled_size,
            assemble_elapsed_ms,
            stage_elapsed_ms,
            persist_elapsed_ms = persist_started_at.elapsed().as_millis(),
            "local chunked upload finalized"
        );
    })
}

async fn finalize_stream_relay_chunked_upload_session(
    state: &PrimaryAppState,
    session: &upload_session::Model,
    policy: &storage_policy::Model,
    driver: &dyn StorageDriver,
    actor_username: Option<&str>,
) -> Result<file::Model> {
    const CHUNK_RELAY_BUFFER_SIZE: usize = 64 * 1024;

    let prepared =
        storage::prepare_non_dedup_blob_upload(policy, session.total_size)?;
    let (writer, reader) = tokio::io::duplex(CHUNK_RELAY_BUFFER_SIZE);
    let relay_task = tokio::spawn(stream_local_chunks_into_writer(
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
        storage::cleanup_preuploaded_blob_upload(
            driver,
            &prepared,
            "chunked upload relay error",
        )
        .await;
        return Err(error);
    }

    let persist_started_at = Instant::now();
    persist_preuploaded_chunked_upload(state, session, driver, &prepared, actor_username)
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

async fn stream_local_chunks_into_writer(
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

async fn assemble_local_chunks_to_temp_file(
    state: &PrimaryAppState,
    session: &upload_session::Model,
    should_dedup: bool,
) -> Result<AssembledTempFile> {
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

    Ok(AssembledTempFile {
        path: assembled_path,
        size,
        file_hash: hasher
            .map(|hasher| crate::utils::hash::sha256_digest_to_hex(&hasher.finalize())),
    })
}

async fn stage_assembled_blob_upload(
    driver: &dyn StorageDriver,
    policy: &storage_policy::Model,
    assembled: AssembledTempFile,
) -> Result<AssembledBlobPlan> {
    let AssembledTempFile {
        path,
        size,
        file_hash,
    } = assembled;
    if let Some(file_hash) = file_hash {
        let storage_path = crate::utils::storage_path_from_blob_key(&file_hash);
        crate::storage::drivers::local::promote_local_file_if_absent(
            driver,
            &storage_path,
            &path,
            size,
        )
        .await?;

        return Ok(AssembledBlobPlan::Dedup {
            file_hash,
            storage_path,
        });
    }

    // 不做 dedup 的情况下，先为 blob 预分配最终 key，再把 assembled 文件传上去。
    // 失败只会留下孤儿 storage 对象，由 blob GC 自然回收。
    let preuploaded = storage::prepare_non_dedup_blob_upload(policy, size)?;
    storage::upload_temp_file_to_prepared_blob(driver, &preuploaded, &path)
        .await?;
    Ok(AssembledBlobPlan::Preuploaded(preuploaded))
}

async fn persist_assembled_upload(
    state: &PrimaryAppState,
    session: &upload_session::Model,
    driver: &dyn StorageDriver,
    policy_id: i64,
    size: i64,
    blob_plan: &AssembledBlobPlan,
    actor_username: Option<&str>,
) -> Result<file::Model> {
    let now = Utc::now();
    let create_result = async {
        let txn = crate::db::transaction::begin(state.writer_db()).await?;

        let blob = match blob_plan {
            AssembledBlobPlan::Dedup {
                file_hash,
                storage_path,
            } => {
                let blob =
                    file_repo::find_or_create_blob(&txn, file_hash, size, policy_id, storage_path)
                        .await?;
                blob.model
            }
            AssembledBlobPlan::Preuploaded(preuploaded) => {
                storage::persist_preuploaded_blob(&txn, preuploaded).await?
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

        crate::db::transaction::commit(txn).await?;
        Ok::<file::Model, AsterError>(created)
    }
    .await;

    match create_result {
        Ok(created) => Ok(created),
        Err(error) => {
            if let AssembledBlobPlan::Preuploaded(preuploaded) = blob_plan {
                storage::cleanup_preuploaded_blob_upload(
                    driver,
                    preuploaded,
                    "chunked upload DB error after storing assembled blob",
                )
                .await;
            }
            // dedup 失败不主动删 storage 对象：另一路并发上传可能正在引用同内容的 blob，
            // 删除会造成 ref=1 的活 blob 丢数据；留给 orphan-blob GC 处理。
            Err(error)
        }
    }
}

async fn persist_preuploaded_chunked_upload(
    state: &PrimaryAppState,
    session: &upload_session::Model,
    driver: &dyn StorageDriver,
    prepared: &PreparedNonDedupBlobUpload,
    actor_username: Option<&str>,
) -> Result<file::Model> {
    let now = Utc::now();
    let create_result = async {
        let txn = crate::db::transaction::begin(state.writer_db()).await?;
        let blob = storage::persist_preuploaded_blob(&txn, prepared).await?;
        let created = storage::finalize_upload_session_blob_with_actor_username(
            &txn,
            session,
            &blob,
            now,
            actor_username,
        )
        .await?;
        crate::db::transaction::commit(txn).await?;
        Ok::<file::Model, AsterError>(created)
    }
    .await;

    match create_result {
        Ok(created) => Ok(created),
        Err(error) => {
            storage::cleanup_preuploaded_blob_upload(
                driver,
                prepared,
                "chunked upload DB error after streaming preuploaded blob",
            )
            .await;
            Err(error)
        }
    }
}
