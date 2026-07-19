//! 上传服务子模块：`progress`。

use std::collections::HashMap;

use crate::api::api_error_code::ApiErrorCode;
use crate::api::constants::HOUR_SECS;
use crate::db::repository::{upload_session_part_repo, upload_session_repo};
use crate::entities::upload_session;
use crate::errors::{
    Result, chunk_upload_error_with_code, upload_assembly_error_with_code,
    validation_error_with_code,
};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::files::upload::kind::{mode_for_kind, resolve_upload_session_kind};
use crate::services::files::upload::provider_session::decrypt_provider_session;
use crate::services::files::upload::responses::{
    ProviderResumableUploadResponse, RecoverableUploadPartResponse,
    RecoverableUploadSessionResponse, UploadProgressResponse,
};
use crate::services::files::upload::scope::{
    load_upload_session, load_upload_session_for_read, personal_scope, team_scope,
};
use crate::services::files::upload::shared::expected_chunk_size_for_upload;
use crate::services::files::upload::staging;
use crate::services::workspace::storage;
use crate::storage::StorageErrorKind;
use crate::types::{UploadSessionKind, UploadSessionStatus};
use aster_forge_utils::paths;
use futures::{StreamExt, stream};

const RECOVERABLE_UPLOAD_SESSIONS_LIMIT: u64 = 100;
const RECOVERABLE_UPLOAD_PROGRESS_CONCURRENCY: usize = 8;
const PRESIGNED_PARTS_MAX_BATCH: usize = 64;

/// 查询上传进度
async fn get_progress_impl(
    state: &PrimaryAppState,
    session: upload_session::Model,
) -> Result<UploadProgressResponse> {
    tracing::debug!(
        upload_id = %session.id,
        status = ?session.status,
        total_chunks = session.total_chunks,
        received_count = session.received_count,
        "loading upload progress"
    );

    let kind = resolve_upload_session_kind(state, &session).await?;
    let (chunks_on_disk, provider_resumable) = match kind {
        UploadSessionKind::ProviderPresignedMultipart
        | UploadSessionKind::RemotePresignedMultipart => {
            let (temp_key, multipart_id) = presigned_multipart_fields(&session)?;
            let policy = state
                .policy_snapshot()
                .get_policy_or_err(session.policy_id)?;
            let chunks = state
                .driver_registry
                .get_multipart_driver(&policy)?
                .list_uploaded_parts(temp_key, multipart_id)
                .await?;
            (chunks, None)
        }
        UploadSessionKind::ProviderRelayMultipart | UploadSessionKind::RemoteRelayMultipart => {
            let part_numbers =
                upload_session_part_repo::list_part_numbers(state.reader_db(), &session.id).await?;
            let mut chunks = Vec::with_capacity(part_numbers.len());
            for part_number in part_numbers {
                if part_number <= 0 || part_number > session.total_chunks {
                    return Err(chunk_upload_error_with_code(
                        ApiErrorCode::UploadChunkPersistFailed,
                        format!(
                            "relay multipart part number {part_number} is out of range [1, {}]",
                            session.total_chunks
                        ),
                    ));
                }
                chunks.push(part_number - 1);
            }
            (chunks, None)
        }
        UploadSessionKind::OffsetStaging | UploadSessionKind::StreamStaging => {
            (list_offset_staging_chunks(state, &session).await?, None)
        }
        UploadSessionKind::LegacyChunkFiles => {
            (scan_received_chunks(state, &session.id).await, None)
        }
        UploadSessionKind::ProviderPresignedSingle | UploadSessionKind::RemotePresignedSingle => {
            (scan_received_chunks(state, &session.id).await, None)
        }
        UploadSessionKind::ProviderDirectResumable => {
            let secret = decrypt_provider_session(state, &session)?;
            let policy = state
                .policy_snapshot()
                .get_policy_or_err(session.policy_id)?;
            let driver = state.driver_registry().get_driver(&policy)?;
            let provider = driver.as_provider_resumable_upload().ok_or_else(|| {
                upload_assembly_error_with_code(
                    ApiErrorCode::UploadSessionCorrupted,
                    "provider resumable driver is unavailable",
                )
            })?;
            match provider
                .query_frontend_upload_session(&secret.upload_url)
                .await
            {
                Ok(status) => {
                    let chunks = provider_completed_chunks(
                        &status.next_expected_ranges,
                        session.chunk_size,
                        session.total_chunks,
                    );
                    (
                        chunks,
                        Some(ProviderResumableUploadResponse {
                            upload_url: secret.upload_url,
                            expires_at: status.expires_at,
                            next_expected_ranges: status.next_expected_ranges,
                        }),
                    )
                }
                Err(error) if error.storage_error_kind() == Some(StorageErrorKind::NotFound) => {
                    let committed = match session.object_temp_key.as_deref() {
                        Some(temp_key) => driver.exists(temp_key).await?,
                        None => false,
                    };
                    if !committed {
                        return Err(error);
                    }
                    (
                        (0..session.total_chunks).collect(),
                        Some(ProviderResumableUploadResponse {
                            upload_url: secret.upload_url,
                            expires_at: Some(session.expires_at),
                            next_expected_ranges: Vec::new(),
                        }),
                    )
                }
                Err(error) => return Err(error),
            }
        }
    };

    let progress = UploadProgressResponse {
        upload_id: session.id,
        status: session.status,
        received_count: session.received_count,
        chunks_on_disk,
        chunk_size: session.chunk_size,
        total_chunks: session.total_chunks,
        filename: session.filename,
        provider_resumable,
    };
    tracing::debug!(
        upload_id = %progress.upload_id,
        status = ?progress.status,
        received_count = progress.received_count,
        total_chunks = progress.total_chunks,
        chunk_count = progress.chunks_on_disk.len(),
        "loaded upload progress"
    );
    Ok(progress)
}

fn presigned_multipart_fields(session: &upload_session::Model) -> Result<(&str, &str)> {
    let temp_key = session.object_temp_key.as_deref().ok_or_else(|| {
        upload_assembly_error_with_code(
            ApiErrorCode::UploadSessionCorrupted,
            "presigned multipart session is missing object_temp_key",
        )
    })?;
    let multipart_id = session.object_multipart_id.as_deref().ok_or_else(|| {
        upload_assembly_error_with_code(
            ApiErrorCode::UploadSessionCorrupted,
            "presigned multipart session is missing object_multipart_id",
        )
    })?;
    Ok((temp_key, multipart_id))
}

async fn list_offset_staging_chunks(
    state: &PrimaryAppState,
    session: &upload_session::Model,
) -> Result<Vec<i32>> {
    let receipts =
        upload_session_part_repo::list_all_by_upload(state.reader_db(), &session.id).await?;
    let mut chunks = Vec::with_capacity(receipts.len());
    for receipt in receipts {
        let chunk_number = receipt.part_number.checked_sub(1).ok_or_else(|| {
            chunk_upload_error_with_code(
                ApiErrorCode::UploadChunkPersistFailed,
                format!(
                    "local chunk receipt has invalid part number {}",
                    receipt.part_number
                ),
            )
        })?;
        if chunk_number >= session.total_chunks {
            return Err(chunk_upload_error_with_code(
                ApiErrorCode::UploadChunkPersistFailed,
                format!(
                    "local chunk receipt part {} is out of range",
                    receipt.part_number
                ),
            ));
        }
        let expected_size = expected_chunk_size_for_upload(session, chunk_number)?;
        if !staging::chunk_receipt_matches(&receipt, chunk_number + 1, expected_size) {
            return Err(chunk_upload_error_with_code(
                ApiErrorCode::UploadChunkPersistFailed,
                format!("local chunk receipt is corrupted for chunk {chunk_number}"),
            ));
        }
        chunks.push(chunk_number);
    }
    Ok(chunks)
}

async fn recoverable_session_response(
    state: &PrimaryAppState,
    session: upload_session::Model,
) -> Result<RecoverableUploadSessionResponse> {
    let mode = mode_for_kind(resolve_upload_session_kind(state, &session).await?);
    let progress = get_progress_impl(state, session.clone()).await?;
    let completed_parts = upload_session_part_repo::list_by_upload(state.reader_db(), &session.id)
        .await?
        .into_iter()
        .map(|part| RecoverableUploadPartResponse {
            part_number: part.part_number,
            etag: part.etag,
        })
        .collect();

    Ok(RecoverableUploadSessionResponse {
        upload_id: session.id,
        mode,
        status: progress.status,
        filename: progress.filename,
        total_size: session.total_size,
        chunk_size: progress.chunk_size,
        total_chunks: progress.total_chunks,
        received_count: progress.received_count,
        folder_id: session.folder_id,
        chunks_on_disk: progress.chunks_on_disk,
        completed_parts,
        provider_resumable: progress.provider_resumable.clone(),
        expires_at: session.expires_at,
        updated_at: session.updated_at,
    })
}

fn provider_completed_chunks(ranges: &[String], chunk_size: i64, total_chunks: i32) -> Vec<i32> {
    let Some(first) = ranges.first() else {
        return Vec::new();
    };
    let start = first
        .split('-')
        .next()
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(0);
    if start <= 0 || chunk_size <= 0 || total_chunks <= 0 {
        return Vec::new();
    }
    let count = start
        .checked_add(chunk_size - 1)
        .and_then(|value| value.checked_div(chunk_size))
        .unwrap_or(i64::MAX)
        .min(i64::from(total_chunks));
    (0..count)
        .filter_map(|value| i32::try_from(value).ok())
        .collect()
}

#[cfg(test)]
mod provider_progress_tests {
    use super::provider_completed_chunks;

    #[test]
    fn provider_ranges_translate_expected_offset_to_completed_chunks() {
        assert_eq!(
            provider_completed_chunks(&["20971520-".to_string()], 10 * 1024 * 1024, 4),
            vec![0, 1]
        );
        assert_eq!(
            provider_completed_chunks(&["10485760-".to_string()], 10 * 1024 * 1024, 4),
            vec![0]
        );
    }

    #[test]
    fn provider_ranges_reject_empty_invalid_and_non_positive_inputs() {
        assert!(provider_completed_chunks(&[], 10, 4).is_empty());
        assert!(provider_completed_chunks(&["0-".to_string()], 10, 4).is_empty());
        assert!(provider_completed_chunks(&["invalid".to_string()], 10, 4).is_empty());
        assert!(provider_completed_chunks(&["10-".to_string()], 0, 4).is_empty());
        assert!(provider_completed_chunks(&["10-".to_string()], 10, 0).is_empty());
    }

    #[test]
    fn provider_ranges_are_clamped_to_session_and_overflow_is_bounded() {
        assert_eq!(
            provider_completed_chunks(&["999999999-".to_string()], 10, 3),
            vec![0, 1, 2]
        );
        assert!(provider_completed_chunks(&[i64::MAX.to_string()], 10, 3).len() <= 3);
    }
}

async fn list_recoverable_sessions_impl(
    state: &PrimaryAppState,
    user_id: i64,
    team_id: Option<i64>,
    frontend_client_id: Option<&str>,
) -> Result<Vec<RecoverableUploadSessionResponse>> {
    let sessions = upload_session_repo::find_recoverable_by_owner(
        state.reader_db(),
        user_id,
        team_id,
        frontend_client_id,
        RECOVERABLE_UPLOAD_SESSIONS_LIMIT,
    )
    .await?;
    stream::iter(sessions)
        .map(|session| recoverable_session_response(state, session))
        .buffered(RECOVERABLE_UPLOAD_PROGRESS_CONCURRENCY)
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .collect()
}

pub async fn get_progress(
    state: &PrimaryAppState,
    upload_id: &str,
    user_id: i64,
) -> Result<UploadProgressResponse> {
    let session = load_upload_session_for_read(state, personal_scope(user_id), upload_id).await?;
    get_progress_impl(state, session).await
}

pub async fn list_recoverable_sessions(
    state: &PrimaryAppState,
    user_id: i64,
    frontend_client_id: Option<&str>,
) -> Result<Vec<RecoverableUploadSessionResponse>> {
    list_recoverable_sessions_impl(state, user_id, None, frontend_client_id).await
}

pub async fn get_progress_for_team(
    state: &PrimaryAppState,
    team_id: i64,
    upload_id: &str,
    user_id: i64,
) -> Result<UploadProgressResponse> {
    let session =
        load_upload_session_for_read(state, team_scope(team_id, user_id), upload_id).await?;
    get_progress_impl(state, session).await
}

pub async fn list_recoverable_sessions_for_team(
    state: &PrimaryAppState,
    team_id: i64,
    user_id: i64,
    frontend_client_id: Option<&str>,
) -> Result<Vec<RecoverableUploadSessionResponse>> {
    storage::require_team_access(state, team_id, user_id).await?;
    list_recoverable_sessions_impl(state, user_id, Some(team_id), frontend_client_id).await
}

/// 为 multipart presigned 上传批量生成 per-part presigned PUT URL
async fn presign_parts_impl(
    state: &PrimaryAppState,
    session: upload_session::Model,
    part_numbers: Vec<i32>,
) -> Result<HashMap<i32, String>> {
    tracing::debug!(
        upload_id = %session.id,
        status = ?session.status,
        requested_part_count = part_numbers.len(),
        "presigning multipart upload parts"
    );
    if session.status != UploadSessionStatus::Presigned {
        return Err(validation_error_with_code(
            ApiErrorCode::UploadStatusConflict,
            format!(
                "session status is '{:?}', expected 'presigned'",
                session.status
            ),
        ));
    }
    validate_presign_part_numbers(&session, &part_numbers)?;

    let multipart_id = session.object_multipart_id.as_deref().ok_or_else(|| {
        validation_error_with_code(
            ApiErrorCode::UploadChunkSessionInvalid,
            "not a multipart upload session",
        )
    })?;
    let temp_key = session.object_temp_key.as_deref().ok_or_else(|| {
        validation_error_with_code(
            ApiErrorCode::UploadSessionCorrupted,
            "missing object_temp_key",
        )
    })?;

    let policy = state
        .policy_snapshot()
        .get_policy_or_err(session.policy_id)?;
    let multipart = state.driver_registry().get_multipart_driver(&policy)?;

    let expires = std::time::Duration::from_secs(HOUR_SECS);
    let mut urls = HashMap::new();
    for part_num in part_numbers {
        let url = multipart
            .presigned_upload_part_url(temp_key, multipart_id, part_num, expires)
            .await?;
        urls.insert(part_num, url);
    }
    tracing::debug!(
        upload_id = %session.id,
        url_count = urls.len(),
        "presigned multipart upload parts"
    );
    Ok(urls)
}

fn validate_presign_part_numbers(
    session: &upload_session::Model,
    part_numbers: &[i32],
) -> Result<()> {
    if part_numbers.is_empty() {
        return Err(validation_error_with_code(
            ApiErrorCode::UploadPartNumbersEmpty,
            "part_numbers cannot be empty",
        ));
    }
    if part_numbers.len() > PRESIGNED_PARTS_MAX_BATCH {
        return Err(validation_error_with_code(
            ApiErrorCode::UploadPartNumbersTooMany,
            format!("part_numbers cannot contain more than {PRESIGNED_PARTS_MAX_BATCH} entries"),
        ));
    }

    for part_number in part_numbers {
        if *part_number < 1 || *part_number > session.total_chunks {
            return Err(validation_error_with_code(
                ApiErrorCode::UploadPartNumberOutOfRange,
                format!(
                    "part number {} is outside the valid range 1..={}",
                    part_number, session.total_chunks
                ),
            ));
        }
    }

    Ok(())
}

pub async fn presign_parts(
    state: &PrimaryAppState,
    upload_id: &str,
    user_id: i64,
    part_numbers: Vec<i32>,
) -> Result<HashMap<i32, String>> {
    let session = load_upload_session(state, personal_scope(user_id), upload_id).await?;
    presign_parts_impl(state, session, part_numbers).await
}

pub async fn presign_parts_for_team(
    state: &PrimaryAppState,
    team_id: i64,
    upload_id: &str,
    user_id: i64,
    part_numbers: Vec<i32>,
) -> Result<HashMap<i32, String>> {
    let session = load_upload_session(state, team_scope(team_id, user_id), upload_id).await?;
    presign_parts_impl(state, session, part_numbers).await
}

/// 扫描临时目录中实际存在的 chunk 文件，返回排序后的 chunk 编号列表
async fn scan_received_chunks(state: &PrimaryAppState, upload_id: &str) -> Vec<i32> {
    let dir = paths::upload_temp_dir(&state.config().server.upload_temp_dir, upload_id);
    let mut received = Vec::new();
    let Ok(mut entries) = tokio::fs::read_dir(&dir).await else {
        return received;
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if let Some(num_str) = name.strip_prefix("chunk_")
            && let Ok(n) = num_str.parse::<i32>()
        {
            received.push(n);
        }
    }
    received.sort();
    received
}

#[cfg(test)]
mod tests {
    use super::presigned_multipart_fields;
    use crate::entities::upload_session;
    use crate::types::UploadSessionStatus;

    fn session(
        object_temp_key: Option<&str>,
        object_multipart_id: Option<&str>,
    ) -> upload_session::Model {
        let now = chrono::Utc::now();
        upload_session::Model {
            id: "progress-test".to_string(),
            user_id: 1,
            team_id: None,
            frontend_client_id: None,
            filename: "progress-test.bin".to_string(),
            total_size: 10,
            chunk_size: 5,
            total_chunks: 2,
            received_count: 0,
            folder_id: None,
            policy_id: 1,
            status: UploadSessionStatus::Presigned,
            session_kind: None,
            object_temp_key: object_temp_key.map(str::to_string),
            object_multipart_id: object_multipart_id.map(str::to_string),
            provider_session_ciphertext: None,
            file_id: None,
            created_at: now,
            expires_at: now + chrono::Duration::hours(1),
            updated_at: now,
        }
    }

    #[test]
    fn presigned_multipart_fields_requires_both_object_identifiers() {
        assert_eq!(
            presigned_multipart_fields(&session(Some("files/temp"), Some("multipart"))).unwrap(),
            ("files/temp", "multipart")
        );
        assert!(presigned_multipart_fields(&session(None, Some("multipart"))).is_err());
        assert!(presigned_multipart_fields(&session(Some("files/temp"), None)).is_err());
    }
}
