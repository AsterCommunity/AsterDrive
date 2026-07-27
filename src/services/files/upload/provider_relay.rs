use std::future::Future;
use std::time::Duration;

use aster_forge_db::transaction;
use bytes::Bytes;
use chrono::Utc;
use futures::StreamExt;
use tokio::io::AsyncWriteExt;

use crate::api::api_error_code::ApiErrorCode;
use crate::db::repository::{upload_session_part_repo, upload_session_repo};
use crate::entities::upload_session;
use crate::errors::{
    AsterError, MapAsterErr, Result, chunk_upload_error_with_code, upload_assembly_error_with_code,
    validation_error_with_code,
};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::files::upload::provider_session::decrypt_provider_session;
use crate::services::files::upload::responses::ChunkUploadResponse;
use crate::services::files::upload::shared::{
    expected_chunk_size_for_upload, upload_session_chunk_unavailable_error,
};
use crate::storage::{
    ProviderResumableUploadDriver, ProviderResumableUploadFragmentOutcome, StorageDriver,
    StorageErrorKind,
};
use crate::types::UploadSessionStatus;
use aster_forge_utils::numbers;

const RELAY_STREAM_PIPE_BUFFER_SIZE: usize = 64 * 1024;
const CLAIM_STALE_AFTER: Duration = Duration::from_secs(120);
const CLAIM_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);
const FRAGMENT_UPLOAD_TIMEOUT: Duration = Duration::from_secs(90);
const PROVIDER_RANGE_RECEIPT: &str = "provider-range-v1";

enum ClaimOutcome {
    Claimed,
    Completed,
    Pending,
}

#[derive(Clone)]
struct ProviderRelayContext {
    driver: std::sync::Arc<dyn StorageDriver>,
    upload_url: String,
    temp_key: String,
}

pub(super) async fn upload_bytes(
    state: &PrimaryAppState,
    session: upload_session::Model,
    chunk_number: i32,
    data: Bytes,
) -> Result<ChunkUploadResponse> {
    let context = load_context(state, &session).await?;
    upload_bytes_with_context(state, session, chunk_number, data, &context).await
}

async fn upload_bytes_with_context(
    state: &PrimaryAppState,
    session: upload_session::Model,
    chunk_number: i32,
    data: Bytes,
    context: &ProviderRelayContext,
) -> Result<ChunkUploadResponse> {
    validate_chunk_request(&session, chunk_number)?;
    let expected_size = expected_chunk_size_for_upload(&session, chunk_number)?;
    let actual_size = numbers::usize_to_i64(data.len(), "provider relay chunk size")?;
    if actual_size != expected_size {
        return Err(chunk_size_mismatch(
            chunk_number,
            expected_size,
            actual_size,
        ));
    }

    match claim_or_reconcile(state, &session, chunk_number, expected_size, context).await? {
        ClaimOutcome::Completed => return response(state, &session.id).await,
        ClaimOutcome::Pending => return Err(chunk_pending_error(chunk_number)),
        ClaimOutcome::Claimed => {}
    }

    let start = chunk_start(&session, chunk_number)?;
    let total_size = numbers::i64_to_u64(session.total_size, "provider relay total size")?;
    let result = upload_with_claim_heartbeat(
        state,
        &session.id,
        chunk_number,
        provider(context)?.upload_session_fragment_reader(
            &context.upload_url,
            start,
            total_size,
            Box::new(std::io::Cursor::new(data)),
            expected_size,
        ),
    )
    .await;
    finish_fragment_result(
        state,
        &session,
        chunk_number,
        expected_size,
        context,
        result,
    )
    .await
}

pub(super) async fn upload_payload(
    state: &PrimaryAppState,
    session: upload_session::Model,
    chunk_number: i32,
    payload: actix_web::web::Payload,
) -> Result<ChunkUploadResponse> {
    let context = load_context(state, &session).await?;
    upload_payload_with_context(state, session, chunk_number, payload, &context).await
}

async fn upload_payload_with_context(
    state: &PrimaryAppState,
    session: upload_session::Model,
    chunk_number: i32,
    payload: actix_web::web::Payload,
    context: &ProviderRelayContext,
) -> Result<ChunkUploadResponse> {
    validate_chunk_request(&session, chunk_number)?;
    let expected_size = expected_chunk_size_for_upload(&session, chunk_number)?;
    match claim_or_reconcile(state, &session, chunk_number, expected_size, context).await? {
        ClaimOutcome::Completed => {
            drain_payload_exact_size(payload, expected_size, chunk_number).await?;
            return response(state, &session.id).await;
        }
        ClaimOutcome::Pending => {
            drain_payload_exact_size(payload, expected_size, chunk_number).await?;
            return Err(chunk_pending_error(chunk_number));
        }
        ClaimOutcome::Claimed => {}
    }

    let start = chunk_start(&session, chunk_number)?;
    let total_size = numbers::i64_to_u64(session.total_size, "provider relay total size")?;
    let (reader, writer) = tokio::io::duplex(RELAY_STREAM_PIPE_BUFFER_SIZE);
    let writer_future = pipe_payload(payload, writer, expected_size, chunk_number);
    let upload_future = upload_with_claim_heartbeat(
        state,
        &session.id,
        chunk_number,
        provider(context)?.upload_session_fragment_reader(
            &context.upload_url,
            start,
            total_size,
            Box::new(reader),
            expected_size,
        ),
    );
    tokio::pin!(writer_future);
    tokio::pin!(upload_future);

    let result = tokio::select! {
        upload_result = &mut upload_future => {
            match writer_future.await {
                Ok(()) => upload_result,
                Err(writer_error) => {
                    reconcile_after_payload_error(
                        state,
                        &session,
                        chunk_number,
                        expected_size,
                        context,
                        upload_result,
                    )
                    .await;
                    return Err(writer_error);
                }
            }
        }
        writer_result = &mut writer_future => {
            if let Err(error) = writer_result {
                let upload_result = upload_future.await;
                reconcile_after_payload_error(
                    state,
                    &session,
                    chunk_number,
                    expected_size,
                    context,
                    upload_result,
                )
                .await;
                return Err(error);
            }
            upload_future.await
        }
    };

    finish_fragment_result(
        state,
        &session,
        chunk_number,
        expected_size,
        context,
        result,
    )
    .await
}

async fn upload_with_claim_heartbeat<F>(
    state: &PrimaryAppState,
    upload_id: &str,
    chunk_number: i32,
    upload_future: F,
) -> Result<ProviderResumableUploadFragmentOutcome>
where
    F: Future<Output = Result<ProviderResumableUploadFragmentOutcome>>,
{
    upload_with_claim_heartbeat_timeout(
        state,
        upload_id,
        chunk_number,
        FRAGMENT_UPLOAD_TIMEOUT,
        upload_future,
    )
    .await
}

async fn upload_with_claim_heartbeat_timeout<F>(
    state: &PrimaryAppState,
    upload_id: &str,
    chunk_number: i32,
    upload_timeout: Duration,
    upload_future: F,
) -> Result<ProviderResumableUploadFragmentOutcome>
where
    F: Future<Output = Result<ProviderResumableUploadFragmentOutcome>>,
{
    let mut heartbeat = tokio::time::interval(CLAIM_HEARTBEAT_INTERVAL);
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    heartbeat.tick().await;
    let deadline = tokio::time::sleep(upload_timeout);
    tokio::pin!(deadline);
    tokio::pin!(upload_future);

    loop {
        tokio::select! {
            result = &mut upload_future => return result,
            _ = &mut deadline => {
                return Err(crate::storage::error::storage_driver_error(
                    StorageErrorKind::Transient,
                    "provider relay fragment upload timed out",
                ));
            }
            _ = heartbeat.tick() => {
                if !upload_session_part_repo::touch_claimed_part(
                    state.writer_db(),
                    upload_id,
                    chunk_number + 1,
                )
                .await?
                {
                    return Err(corrupted(
                        "provider relay claim disappeared while fragment was uploading",
                    ));
                }
            }
        }
    }
}

async fn reconcile_after_payload_error(
    state: &PrimaryAppState,
    session: &upload_session::Model,
    chunk_number: i32,
    expected_size: i64,
    context: &ProviderRelayContext,
    upload_result: Result<ProviderResumableUploadFragmentOutcome>,
) {
    if let Err(error) = finish_fragment_result(
        state,
        session,
        chunk_number,
        expected_size,
        context,
        upload_result,
    )
    .await
    {
        tracing::warn!(
            upload_id = %session.id,
            chunk_number,
            "failed to reconcile provider relay state after request body error: {error}"
        );
    }
}

pub(super) async fn reconcile_progress(
    state: &PrimaryAppState,
    session: &upload_session::Model,
) -> Result<Vec<i32>> {
    let context = load_context(state, session).await?;
    reconcile_progress_with_context(state, session, &context).await
}

async fn reconcile_progress_with_context(
    state: &PrimaryAppState,
    session: &upload_session::Model,
    context: &ProviderRelayContext,
) -> Result<Vec<i32>> {
    let next_offset = provider_next_offset(context, session.total_size).await?;
    loop {
        let current = upload_session_repo::find_by_id(state.writer_db(), &session.id).await?;
        if current.received_count >= current.total_chunks {
            return Ok((0..current.total_chunks).collect());
        }
        let chunk_number = current.received_count;
        let expected_size = expected_chunk_size_for_upload(&current, chunk_number)?;
        let end = chunk_end_exclusive(chunk_start(&current, chunk_number)?, expected_size)?;
        if next_offset < end {
            return Ok((0..current.received_count).collect());
        }
        finalize_receipt(state, &current, chunk_number, expected_size).await?;
    }
}

fn validate_chunk_request(session: &upload_session::Model, chunk_number: i32) -> Result<()> {
    if session.status != UploadSessionStatus::Uploading {
        return Err(upload_session_chunk_unavailable_error(session));
    }
    if session.expires_at <= Utc::now() {
        return Err(AsterError::upload_session_expired("session expired"));
    }
    if chunk_number < 0 || chunk_number >= session.total_chunks {
        return Err(validation_error_with_code(
            ApiErrorCode::UploadChunkNumberOutOfRange,
            format!(
                "chunk_number {chunk_number} out of range [0, {})",
                session.total_chunks
            ),
        ));
    }
    Ok(())
}

async fn load_context(
    state: &PrimaryAppState,
    session: &upload_session::Model,
) -> Result<ProviderRelayContext> {
    let temp_key = session
        .object_temp_key
        .as_deref()
        .ok_or_else(|| corrupted("provider relay resumable session is missing object_temp_key"))?;
    let secret = decrypt_provider_session(state, session)?;
    let policy = state
        .policy_snapshot()
        .get_policy_or_err(session.policy_id)?;
    let driver = state.driver_registry().get_driver(&policy)?;
    let provider = driver
        .extensions()
        .provider_resumable
        .ok_or_else(|| corrupted("provider relay resumable driver is unavailable"))?;
    if provider.provider_resumable_upload_capabilities().provider != secret.provider {
        return Err(corrupted(
            "provider relay session metadata does not match the configured driver",
        ));
    }
    Ok(ProviderRelayContext {
        driver,
        upload_url: secret.upload_url,
        temp_key: temp_key.to_string(),
    })
}

fn provider(context: &ProviderRelayContext) -> Result<&dyn ProviderResumableUploadDriver> {
    context
        .driver
        .extensions()
        .provider_resumable
        .ok_or_else(|| corrupted("provider relay resumable driver is unavailable"))
}

async fn claim_or_reconcile(
    state: &PrimaryAppState,
    session: &upload_session::Model,
    chunk_number: i32,
    expected_size: i64,
    context: &ProviderRelayContext,
) -> Result<ClaimOutcome> {
    loop {
        let txn = transaction::begin(state.writer_db()).await?;
        let current = upload_session_repo::lock_by_id(&txn, &session.id).await?;
        if current.status != UploadSessionStatus::Uploading {
            return Err(upload_session_chunk_unavailable_error(&current));
        }
        if chunk_number < current.received_count {
            validate_completed_receipt(&txn, &current.id, chunk_number, expected_size).await?;
            transaction::commit(txn).await?;
            return Ok(ClaimOutcome::Completed);
        }
        if chunk_number > current.received_count {
            return Err(out_of_order_error(chunk_number, current.received_count));
        }
        let claimed =
            upload_session_part_repo::try_claim_part(&txn, &current.id, chunk_number + 1).await?;
        transaction::commit(txn).await?;
        if claimed {
            return Ok(ClaimOutcome::Claimed);
        }

        let next_offset = provider_next_offset(context, session.total_size).await?;
        let start = chunk_start(session, chunk_number)?;
        let end = chunk_end_exclusive(start, expected_size)?;
        if next_offset >= end {
            finalize_receipt(state, session, chunk_number, expected_size).await?;
            return Ok(ClaimOutcome::Completed);
        }
        if next_offset != start {
            return Err(corrupted(format!(
                "provider next offset {next_offset} does not match claimed range {start}-{end}"
            )));
        }

        let stale_before = Utc::now()
            - chrono::Duration::from_std(CLAIM_STALE_AFTER)
                .map_err(|error| AsterError::internal_error(error.to_string()))?;
        if upload_session_part_repo::delete_stale_claim(
            state.writer_db(),
            &session.id,
            chunk_number + 1,
            stale_before,
        )
        .await?
        {
            continue;
        }
        return Ok(ClaimOutcome::Pending);
    }
}

async fn finish_fragment_result(
    state: &PrimaryAppState,
    session: &upload_session::Model,
    chunk_number: i32,
    expected_size: i64,
    context: &ProviderRelayContext,
    result: Result<ProviderResumableUploadFragmentOutcome>,
) -> Result<ChunkUploadResponse> {
    let start = chunk_start(session, chunk_number)?;
    let end = chunk_end_exclusive(start, expected_size)?;
    match result {
        Ok(outcome) => {
            let next = if outcome.completed {
                numbers::i64_to_u64(session.total_size, "provider relay total size")?
            } else {
                next_expected_offset(&outcome.next_expected_ranges)?
            };
            if next != end {
                return Err(corrupted(format!(
                    "provider accepted range ended at {next}, expected {end}"
                )));
            }
        }
        Err(upload_error) => match provider_next_offset(context, session.total_size).await {
            Ok(next) if next >= end => {}
            Ok(next) if next == start => {
                release_claim(state, &session.id, chunk_number).await;
                return Err(upload_error);
            }
            Ok(next) => {
                return Err(corrupted(format!(
                    "provider returned inconsistent next offset {next} after fragment failure"
                )));
            }
            Err(query_error) => {
                tracing::warn!(
                    upload_id = %session.id,
                    chunk_number,
                    "failed to reconcile ambiguous provider fragment result: {query_error}"
                );
                return Err(upload_error);
            }
        },
    }
    finalize_receipt(state, session, chunk_number, expected_size).await?;
    response(state, &session.id).await
}

async fn provider_next_offset(context: &ProviderRelayContext, total_size: i64) -> Result<u64> {
    match provider(context)?
        .query_upload_session(&context.upload_url)
        .await
    {
        Ok(status) => next_expected_offset(&status.next_expected_ranges),
        Err(error) if error.storage_error_kind() == Some(StorageErrorKind::NotFound) => {
            if context.driver.exists(&context.temp_key).await? {
                return Ok(numbers::i64_to_u64(
                    total_size,
                    "provider relay total size",
                )?);
            }
            Err(error)
        }
        Err(error) => Err(error),
    }
}

pub(super) fn next_expected_offset(ranges: &[String]) -> Result<u64> {
    let first = ranges
        .first()
        .ok_or_else(|| corrupted("provider upload session returned no next expected range"))?;
    let start = first.split('-').next().unwrap_or_default();
    start.parse::<u64>().map_err(|error| {
        corrupted(format!(
            "provider upload session returned invalid next expected range: {error}"
        ))
    })
}

async fn finalize_receipt(
    state: &PrimaryAppState,
    session: &upload_session::Model,
    chunk_number: i32,
    expected_size: i64,
) -> Result<()> {
    let txn = transaction::begin(state.writer_db()).await?;
    let current = upload_session_repo::lock_by_id(&txn, &session.id).await?;
    if current.received_count > chunk_number {
        validate_completed_receipt(&txn, &session.id, chunk_number, expected_size).await?;
        transaction::commit(txn).await?;
        return Ok(());
    }
    if current.received_count != chunk_number {
        return Err(out_of_order_error(chunk_number, current.received_count));
    }
    if !upload_session_part_repo::finalize_claimed_part(
        &txn,
        &session.id,
        chunk_number + 1,
        PROVIDER_RANGE_RECEIPT,
        expected_size,
    )
    .await?
    {
        validate_completed_receipt(&txn, &session.id, chunk_number, expected_size).await?;
    }
    if !upload_session_repo::advance_provider_relay_received_count(&txn, &session.id, chunk_number)
        .await?
    {
        return Err(corrupted(
            "failed to advance provider relay upload progress",
        ));
    }
    transaction::commit(txn).await?;
    Ok(())
}

async fn validate_completed_receipt<C: sea_orm::ConnectionTrait>(
    db: &C,
    upload_id: &str,
    chunk_number: i32,
    expected_size: i64,
) -> Result<()> {
    let receipt =
        upload_session_part_repo::find_by_upload_and_part(db, upload_id, chunk_number + 1)
            .await?
            .ok_or_else(|| corrupted("provider relay receipt is missing"))?;
    if receipt.etag != PROVIDER_RANGE_RECEIPT || receipt.size != expected_size {
        return Err(corrupted("provider relay receipt is invalid"));
    }
    Ok(())
}

fn chunk_start(session: &upload_session::Model, chunk_number: i32) -> Result<u64> {
    let start = i64::from(chunk_number)
        .checked_mul(session.chunk_size)
        .ok_or_else(|| corrupted("provider relay chunk offset overflow"))?;
    Ok(numbers::i64_to_u64(start, "provider relay chunk offset")?)
}

fn chunk_end_exclusive(start: u64, size: i64) -> Result<u64> {
    start
        .checked_add(numbers::i64_to_u64(size, "provider relay fragment size")?)
        .ok_or_else(|| corrupted("provider relay chunk range overflow"))
}

async fn response(state: &PrimaryAppState, upload_id: &str) -> Result<ChunkUploadResponse> {
    let current = upload_session_repo::find_by_id(state.writer_db(), upload_id).await?;
    Ok(ChunkUploadResponse {
        received_count: current.received_count,
        total_chunks: current.total_chunks,
    })
}

async fn release_claim(state: &PrimaryAppState, upload_id: &str, chunk_number: i32) {
    if let Err(error) = upload_session_part_repo::delete_by_upload_and_part(
        state.writer_db(),
        upload_id,
        chunk_number + 1,
    )
    .await
    {
        tracing::warn!(
            upload_id,
            chunk_number,
            "failed to release provider relay claim: {error}"
        );
    }
}

async fn pipe_payload(
    mut payload: actix_web::web::Payload,
    mut writer: tokio::io::DuplexStream,
    expected_size: i64,
    chunk_number: i32,
) -> Result<()> {
    let mut size = 0_i64;
    while let Some(chunk) = payload.next().await {
        let chunk = chunk.map_aster_err_ctx("read provider relay request body", |_| {
            validation_error_with_code(
                ApiErrorCode::UploadRequestBodyReadFailed,
                "failed to read request body",
            )
        })?;
        size = size
            .checked_add(numbers::usize_to_i64(
                chunk.len(),
                "provider relay body part",
            )?)
            .ok_or_else(|| chunk_size_mismatch(chunk_number, expected_size, i64::MAX))?;
        if size > expected_size {
            return Err(chunk_size_mismatch(chunk_number, expected_size, size));
        }
        writer
            .write_all(&chunk)
            .await
            .map_aster_err_ctx("stream provider relay chunk", |message| {
                chunk_upload_error_with_code(ApiErrorCode::UploadChunkRelayFailed, message)
            })?;
    }
    if size != expected_size {
        return Err(chunk_size_mismatch(chunk_number, expected_size, size));
    }
    writer
        .shutdown()
        .await
        .map_aster_err_ctx("finish provider relay chunk stream", |message| {
            chunk_upload_error_with_code(ApiErrorCode::UploadChunkRelayFailed, message)
        })
}

async fn drain_payload_exact_size(
    mut payload: actix_web::web::Payload,
    expected_size: i64,
    chunk_number: i32,
) -> Result<()> {
    let mut size = 0_i64;
    while let Some(chunk) = payload.next().await {
        let chunk = chunk.map_err(|_| {
            validation_error_with_code(
                ApiErrorCode::UploadRequestBodyReadFailed,
                "failed to read request body",
            )
        })?;
        size = size
            .checked_add(numbers::usize_to_i64(
                chunk.len(),
                "provider relay retry body",
            )?)
            .ok_or_else(|| chunk_size_mismatch(chunk_number, expected_size, i64::MAX))?;
        if size > expected_size {
            return Err(chunk_size_mismatch(chunk_number, expected_size, size));
        }
    }
    if size != expected_size {
        return Err(chunk_size_mismatch(chunk_number, expected_size, size));
    }
    Ok(())
}

fn chunk_size_mismatch(chunk_number: i32, expected: i64, actual: i64) -> AsterError {
    chunk_upload_error_with_code(
        ApiErrorCode::UploadChunkSizeMismatch,
        format!("chunk {chunk_number} size mismatch: expected {expected}, got {actual}"),
    )
}

fn out_of_order_error(chunk_number: i32, expected: i32) -> AsterError {
    chunk_upload_error_with_code(
        ApiErrorCode::UploadChunkSessionInvalid,
        format!("provider relay chunk {chunk_number} is out of order; expected chunk {expected}"),
    )
}

fn chunk_pending_error(chunk_number: i32) -> AsterError {
    AsterError::upload_assembling(format!(
        "provider relay chunk {chunk_number} is already being uploaded"
    ))
    .with_api_error_code(ApiErrorCode::UploadChunkPending)
}

fn corrupted(message: impl Into<String>) -> AsterError {
    upload_assembly_error_with_code(ApiErrorCode::UploadSessionCorrupted, message)
}

#[cfg(test)]
mod tests;
