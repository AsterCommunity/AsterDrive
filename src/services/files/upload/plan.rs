//! 上传初始化阶段。
//!
//! 这里不真正写入文件内容，只负责：
//! - 解析目标路径和目录自动创建
//! - 解析存储策略与大小限制
//! - 协商最终上传模式
//! - 在需要 session 的模式下预先写入 upload_sessions

mod context;
pub(crate) mod mime;
mod object_storage;
mod provider;
mod remote;

use chrono::{Duration, Utc};

use crate::api::api_error_code::ApiErrorCode;
use crate::errors::{MapAsterErr, Result, chunk_upload_error_with_code};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::files::upload::ingest::staging;
use crate::services::files::upload::session::responses::InitUploadResponse;
use crate::services::files::upload::session::scope::{personal_scope, team_scope};
use crate::services::files::upload::session::shared::{
    UniqueUuidAttempt, delete_upload_session_record_after_init_error, with_unique_upload_id,
};
use crate::services::ops::deployment;
use crate::services::workspace::storage::{
    WorkspaceStorageScope, resolve_policy_upload_transport_for_execution,
};
use aster_drive_model::types::{UploadSessionStatus, UploadTransport};
use aster_forge_utils::numbers;
use aster_forge_utils::paths;

use self::context::{
    InitUploadContext, UploadSessionRecordParams, init_stream_session, materialize_upload_target,
    resolve_init_upload_context, session_kind_for_transport, try_persist_upload_session,
    validate_storage_capacity,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UploadPlan {
    pub upload_id: String,
    pub filename: String,
    pub mime_type: String,
    pub total_size: i64,
    pub policy_id: i64,
    pub placement_profile_id: i64,
    pub placement_rule_id: Option<i64>,
    pub placement_revision: i64,
    pub transport: aster_drive_model::types::UploadTransport,
}

impl UploadPlan {
    pub(crate) fn try_from_session(
        session: &aster_drive_model::entities::upload_session::Model,
    ) -> Result<Self> {
        let placement_profile_id = session.placement_profile_id.ok_or_else(|| {
            crate::errors::AsterError::validation_error(
                "upload session is missing its placement profile binding",
            )
        })?;
        let placement_revision = session.placement_revision.ok_or_else(|| {
            crate::errors::AsterError::validation_error(
                "upload session is missing its placement revision binding",
            )
        })?;
        Ok(Self {
            upload_id: session.id.clone(),
            filename: session.filename.clone(),
            mime_type: session.mime_type.clone(),
            total_size: session.total_size,
            policy_id: session.policy_id,
            placement_profile_id,
            placement_rule_id: session.placement_rule_id,
            placement_revision,
            transport: crate::services::files::upload::session::kind::mode_for_kind(
                session.session_kind,
            ),
        })
    }
}

#[derive(Clone, Copy)]
pub struct InitUploadParams<'a> {
    pub filename: &'a str,
    pub total_size: i64,
    pub folder_id: Option<i64>,
    pub relative_path: Option<&'a str>,
    pub mime_type: Option<&'a str>,
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
            mime_type: None,
            frontend_client_id: None,
        }
    }

    pub fn with_frontend_client(mut self, frontend_client_id: Option<&'a str>) -> Self {
        self.frontend_client_id = frontend_client_id;
        self
    }

    pub fn with_mime_type(mut self, mime_type: Option<&'a str>) -> Self {
        self.mime_type = mime_type;
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

    let mut ctx = resolve_init_upload_context(state, scope, params).await?;
    let transport = resolve_policy_upload_transport_for_execution(
        state.driver_registry().connectors(),
        &ctx.policy,
        ctx.routing_decision.execution_preference,
    )?;

    if ctx.total_size == 0 {
        return Err(crate::errors::AsterError::validation_error(
            "zero-byte files must be created through /files/new",
        ));
    }

    validate_storage_capacity(state, &ctx.policy, ctx.total_size).await?;
    materialize_upload_target(state, &mut ctx).await?;

    if transport.resolve_init_mode(&ctx.policy, ctx.total_size) == UploadTransport::Stream
        && transport.supports_streaming_direct_upload(&ctx.policy, ctx.total_size)
    {
        let response = init_stream_session(state, &ctx).await?;
        record_upload_session_if_created(state, &response);
        return Ok(response);
    }

    if let Some(response) = provider::init_provider_resumable_upload(state, &ctx).await? {
        record_upload_session_if_created(state, &response);
        return Ok(response);
    }

    // Object-storage and remote transports have protocol-level upload session
    // setup. Generic stream-upload connectors fall through to direct/chunked
    // modes; any provider-native resumable session stays inside the concrete
    // driver instead of leaking into upload-service dispatch.
    if let Some(response) = object_storage::init_object_storage_upload(state, &ctx).await? {
        record_upload_session_if_created(state, &response);
        return Ok(response);
    }

    if let Some(response) = remote::init_remote_upload(state, &ctx).await? {
        record_upload_session_if_created(state, &response);
        return Ok(response);
    }

    let response = init_chunked_upload_session(state, &ctx).await?;
    record_upload_session_if_created(state, &response);
    Ok(response)
}

fn record_upload_session_if_created(
    state: &impl SharedRuntimeState,
    response: &InitUploadResponse,
) {
    if response.upload_id.is_some() {
        state
            .metrics()
            .record_upload_session(response.mode.as_str());
    }
}

async fn init_chunked_upload_session(
    state: &PrimaryAppState,
    ctx: &InitUploadContext,
) -> Result<InitUploadResponse> {
    // 本地 / 其他非 direct 场景：服务端维护 upload session，并预创建格式专用的
    // `.offset-staging-v1` 文件。每个 Chunk PUT 按 offset 写入并登记 DB receipt，Complete
    // 只校验 receipt 和 staging 内容后推进存储和元数据。
    let transport = resolve_policy_upload_transport_for_execution(
        state.driver_registry().connectors(),
        &ctx.policy,
        ctx.routing_decision.execution_preference,
    )?;
    let chunk_size = ctx.policy.chunk_size;
    let total_chunks = numbers::calc_total_chunks(ctx.total_size, chunk_size, "chunked upload")?;
    let expires_at = Utc::now() + Duration::hours(24);
    let session_kind = session_kind_for_transport(transport, UploadTransport::Chunked)?;
    deployment::validate_upload_session_kind(state.config(), session_kind)?;

    let upload_id = with_unique_upload_id(|upload_id| async {
        let inserted = try_persist_upload_session(
            state.writer_db(),
            UploadSessionRecordParams {
                upload_id: &upload_id,
                scope: ctx.scope,
                filename: &ctx.target.filename,
                mime_type: &ctx.mime_type,
                total_size: ctx.total_size,
                chunk_size,
                total_chunks,
                folder_id: ctx.target.folder_id,
                policy_id: ctx.policy.id,
                placement_profile_id: Some(ctx.routing_decision.profile_id),
                placement_rule_id: ctx.routing_decision.rule_id,
                placement_revision: Some(ctx.routing_decision.revision),
                placement_execution_preference: ctx.routing_decision.execution_preference.as_str(),
                frontend_client_id: ctx.frontend_client_id.as_deref(),
                status: UploadSessionStatus::Uploading,
                session_kind,
                object_temp_key: None,
                object_multipart_id: None,
                provider_session_ciphertext: None,
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

    if let Err(error) = prepare_chunked_upload_staging_file(state, &upload_id, ctx.total_size).await
    {
        let temp_dir = paths::upload_temp_dir(&state.config().server.upload_temp_dir, &upload_id);
        aster_forge_utils::fs::cleanup_temp_dir(&temp_dir).await;
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
        mode = ?UploadTransport::Chunked,
        chunk_size,
        total_chunks,
        folder_id = ctx.target.folder_id,
        "initialized chunked upload session"
    );

    Ok(context::chunked_upload_response(
        UploadTransport::Chunked,
        upload_id,
        chunk_size,
        total_chunks,
        session_kind,
    ))
}

async fn prepare_chunked_upload_staging_file(
    state: &PrimaryAppState,
    upload_id: &str,
    total_size: i64,
) -> Result<()> {
    let temp_dir = paths::upload_temp_dir(&state.config().server.upload_temp_dir, upload_id);
    tokio::fs::create_dir_all(&temp_dir)
        .await
        .map_aster_err_ctx("create temp dir", |message| {
            chunk_upload_error_with_code(ApiErrorCode::UploadTempDirCreateFailed, message)
        })?;
    staging::prepare(state, upload_id, total_size).await?;
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
