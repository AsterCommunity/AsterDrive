//! 上传服务子模块：`progress`。

use std::collections::HashMap;

use crate::api::api_error_code::ApiErrorCode;
use crate::api::constants::HOUR_SECS;
use crate::db::repository::{upload_session_part_repo, upload_session_repo};
use crate::entities::upload_session;
use crate::errors::{Result, validation_error_with_code};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::upload_service::responses::{
    RecoverableUploadPartResponse, RecoverableUploadSessionResponse, UploadProgressResponse,
};
use crate::services::upload_service::scope::{
    load_upload_session, load_upload_session_for_read, personal_scope, team_scope,
};
use crate::services::workspace::storage::{self, resolve_policy_upload_transport};
use crate::types::{UploadMode, UploadSessionStatus};
use crate::utils::paths;
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

    let chunks_on_disk = if session.status == UploadSessionStatus::Presigned {
        match (
            session.object_temp_key.as_deref(),
            session.object_multipart_id.as_deref(),
        ) {
            (Some(temp_key), Some(multipart_id)) => {
                let policy = state
                    .policy_snapshot()
                    .get_policy_or_err(session.policy_id)?;
                state
                    .driver_registry
                    .get_multipart_driver(&policy)?
                    .list_uploaded_parts(temp_key, multipart_id)
                    .await?
            }
            _ => scan_received_chunks(state, &session.id).await,
        }
    } else if session.object_multipart_id.is_some() {
        let policy = state
            .policy_snapshot()
            .get_policy_or_err(session.policy_id)?;
        if is_relay_multipart_policy(&policy)? {
            upload_session_part_repo::list_part_numbers(state.reader_db(), &session.id)
                .await?
                .into_iter()
                .map(|part_number| part_number - 1)
                .collect()
        } else {
            scan_received_chunks(state, &session.id).await
        }
    } else {
        scan_received_chunks(state, &session.id).await
    };

    let progress = UploadProgressResponse {
        upload_id: session.id,
        status: session.status,
        received_count: session.received_count,
        chunks_on_disk,
        chunk_size: session.chunk_size,
        total_chunks: session.total_chunks,
        filename: session.filename,
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

fn is_relay_multipart_policy(policy: &crate::entities::storage_policy::Model) -> Result<bool> {
    Ok(resolve_policy_upload_transport(policy)?.uses_relay_multipart_tracking())
}

fn recoverable_mode_for_session(session: &upload_session::Model) -> UploadMode {
    if session.status == UploadSessionStatus::Presigned {
        if session.object_multipart_id.is_some() {
            return UploadMode::PresignedMultipart;
        }
        return UploadMode::Presigned;
    }
    UploadMode::Chunked
}

async fn recoverable_session_response(
    state: &PrimaryAppState,
    session: upload_session::Model,
) -> Result<RecoverableUploadSessionResponse> {
    let mode = recoverable_mode_for_session(&session);
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
        expires_at: session.expires_at,
        updated_at: session.updated_at,
    })
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
