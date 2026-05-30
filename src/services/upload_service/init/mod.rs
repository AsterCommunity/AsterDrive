//! 上传初始化阶段。
//!
//! 这里不真正写入文件内容，只负责：
//! - 解析目标路径和目录自动创建
//! - 解析存储策略与大小限制
//! - 协商最终上传模式
//! - 在需要 session 的模式下预先写入 upload_sessions

mod context;
mod remote;
mod s3;

use chrono::{Duration, Utc};

use crate::api::subcode::ApiSubcode;
use crate::errors::{MapAsterErr, Result, chunk_upload_error_with_subcode};
use crate::runtime::PrimaryAppState;
use crate::services::upload_service::responses::InitUploadResponse;
use crate::services::upload_service::scope::{personal_scope, team_scope};
use crate::services::upload_service::shared::{
    UniqueUuidAttempt, delete_upload_session_record_after_init_error, with_unique_upload_id,
};
use crate::services::workspace_storage_service::{
    WorkspaceStorageScope, resolve_policy_upload_transport,
};
use crate::types::{UploadMode, UploadSessionStatus};
use crate::utils::{numbers, paths};

use self::context::{
    InitUploadContext, UploadSessionRecordParams, direct_upload_response,
    resolve_init_upload_context, try_persist_upload_session,
};

#[derive(Clone, Copy)]
pub struct InitUploadParams<'a> {
    pub filename: &'a str,
    pub total_size: i64,
    pub folder_id: Option<i64>,
    pub relative_path: Option<&'a str>,
    pub frontend_client_id: Option<&'a str>,
}

impl<'a> InitUploadParams<'a> {
    pub fn new(
        filename: &'a str,
        total_size: i64,
        folder_id: Option<i64>,
        relative_path: Option<&'a str>,
    ) -> Self {
        Self {
            filename,
            total_size,
            folder_id,
            relative_path,
            frontend_client_id: None,
        }
    }

    pub fn with_frontend_client(mut self, frontend_client_id: Option<&'a str>) -> Self {
        self.frontend_client_id = frontend_client_id;
        self
    }
}

async fn init_upload_for_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    params: InitUploadParams<'_>,
) -> Result<InitUploadResponse> {
    tracing::debug!(
        scope = ?scope,
        folder_id = params.folder_id,
        filename = %params.filename,
        total_size = params.total_size,
        relative_path = params.relative_path.unwrap_or(""),
        "initializing upload session"
    );

    let ctx = resolve_init_upload_context(
        state,
        scope,
        params.filename,
        params.total_size,
        params.folder_id,
        params.relative_path,
        params.frontend_client_id,
    )
    .await?;
    let transport = resolve_policy_upload_transport(&ctx.policy);

    if ctx.total_size == 0 {
        tracing::debug!(
            scope = ?ctx.scope,
            policy_id = ctx.policy.id,
            mode = ?UploadMode::Direct,
            folder_id = ctx.target.folder_id,
            "selected direct upload mode for empty file"
        );
        return Ok(direct_upload_response());
    }

    if let Some(response) = s3::init_s3_upload(state, &ctx).await? {
        record_upload_session_if_created(state, &response);
        return Ok(response);
    }

    if let Some(response) = remote::init_remote_upload(state, &ctx).await? {
        record_upload_session_if_created(state, &response);
        return Ok(response);
    }

    if transport.resolve_init_mode(&ctx.policy, ctx.total_size) == UploadMode::Direct {
        tracing::debug!(
            scope = ?ctx.scope,
            policy_id = ctx.policy.id,
            mode = ?UploadMode::Direct,
            folder_id = ctx.target.folder_id,
            "selected direct upload mode"
        );
        return Ok(direct_upload_response());
    }

    let response = init_chunked_upload_session(state, &ctx).await?;
    record_upload_session_if_created(state, &response);
    Ok(response)
}

fn record_upload_session_if_created(state: &PrimaryAppState, response: &InitUploadResponse) {
    if response.upload_id.is_some() {
        state.metrics.record_upload_session(response.mode.as_str());
    }
}

async fn init_chunked_upload_session(
    state: &PrimaryAppState,
    ctx: &InitUploadContext,
) -> Result<InitUploadResponse> {
    // 本地 / 其他非 direct 场景：服务端维护分片目录与 upload session，
    // complete 阶段会把这些 chunk 组装成最终文件。
    let chunk_size = ctx.policy.chunk_size;
    let total_chunks = numbers::calc_total_chunks(ctx.total_size, chunk_size, "chunked upload")?;
    let expires_at = Utc::now() + Duration::hours(24);

    let upload_id = with_unique_upload_id(|upload_id| async {
        let inserted = try_persist_upload_session(
            state.writer_db(),
            UploadSessionRecordParams {
                upload_id: &upload_id,
                scope: ctx.scope,
                filename: &ctx.target.filename,
                total_size: ctx.total_size,
                chunk_size,
                total_chunks,
                folder_id: ctx.target.folder_id,
                policy_id: ctx.policy.id,
                frontend_client_id: ctx.frontend_client_id.as_deref(),
                status: UploadSessionStatus::Uploading,
                s3_temp_key: None,
                s3_multipart_id: None,
                expires_at,
            },
        )
        .await?;
        if !inserted {
            return Ok(UniqueUuidAttempt::Collision);
        }
        Ok(UniqueUuidAttempt::Accepted(upload_id))
    })
    .await?;

    if let Err(error) = prepare_chunked_upload_temp_dir(state, &upload_id).await {
        delete_upload_session_record_after_init_error(
            state.writer_db(),
            &upload_id,
            "chunked temp dir initialization error",
        )
        .await;
        return Err(error);
    }

    tracing::debug!(
        scope = ?ctx.scope,
        upload_id = %upload_id,
        policy_id = ctx.policy.id,
        mode = ?UploadMode::Chunked,
        chunk_size,
        total_chunks,
        folder_id = ctx.target.folder_id,
        "initialized chunked upload session"
    );

    Ok(context::chunked_upload_response(
        UploadMode::Chunked,
        upload_id,
        chunk_size,
        total_chunks,
    ))
}

async fn prepare_chunked_upload_temp_dir(state: &PrimaryAppState, upload_id: &str) -> Result<()> {
    let temp_dir = paths::upload_temp_dir(&state.config.server.upload_temp_dir, upload_id);
    tokio::fs::create_dir_all(&temp_dir)
        .await
        .map_aster_err_ctx("create temp dir", |message| {
            chunk_upload_error_with_subcode(ApiSubcode::UploadTempDirCreateFailed, message)
        })?;
    Ok(())
}

/// 上传协商：服务端根据存储策略决定上传模式
pub async fn init_upload(
    state: &PrimaryAppState,
    user_id: i64,
    filename: &str,
    total_size: i64,
    folder_id: Option<i64>,
    relative_path: Option<&str>,
) -> Result<InitUploadResponse> {
    init_upload_with_frontend_client(
        state,
        user_id,
        InitUploadParams::new(filename, total_size, folder_id, relative_path),
    )
    .await
}

pub async fn init_upload_with_frontend_client(
    state: &PrimaryAppState,
    user_id: i64,
    params: InitUploadParams<'_>,
) -> Result<InitUploadResponse> {
    init_upload_for_scope(state, personal_scope(user_id), params).await
}

/// 团队空间上传协商：规则和个人空间一致，但路径归属与配额都落在团队 scope。
pub async fn init_upload_for_team(
    state: &PrimaryAppState,
    team_id: i64,
    user_id: i64,
    filename: &str,
    total_size: i64,
    folder_id: Option<i64>,
    relative_path: Option<&str>,
) -> Result<InitUploadResponse> {
    init_upload_for_team_with_frontend_client(
        state,
        team_id,
        user_id,
        InitUploadParams::new(filename, total_size, folder_id, relative_path),
    )
    .await
}

pub async fn init_upload_for_team_with_frontend_client(
    state: &PrimaryAppState,
    team_id: i64,
    user_id: i64,
    params: InitUploadParams<'_>,
) -> Result<InitUploadResponse> {
    init_upload_for_scope(state, team_scope(team_id, user_id), params).await
}
