//! API 路由：`batch`。

use crate::api::api_error_code::ApiErrorCode;
pub use crate::api::dto::batch::*;
use crate::api::dto::validate_request;
use crate::api::middleware::auth::JwtAuth;
use crate::api::middleware::rate_limit;
use crate::api::response::ApiResponse;
use crate::api::routes::team_scope;
use crate::config::operations;
use crate::config::{NetworkTrustConfig, RateLimitConfig};
use crate::errors::{Result, auth_forbidden_with_code};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::{
    ops::audit::{self, AuditContext},
    auth::local::Claims,
    files::batch,
    share::ticket,
    task_service,
    workspace::storage::WorkspaceStorageScope,
};
use actix_governor::Governor;
use actix_web::middleware::Condition;
use actix_web::{HttpRequest, HttpResponse, web};

pub fn routes(
    rl: &RateLimitConfig,
    network_trust: &NetworkTrustConfig,
) -> impl actix_web::dev::HttpServiceFactory + use<> {
    let limiter = rate_limit::build_governor(&rl.write, &network_trust.trusted_proxies);

    web::scope("/batch")
        .wrap(JwtAuth)
        .wrap(Condition::new(rl.enabled, Governor::new(&limiter)))
        .route("/delete", web::post().to(batch_delete))
        .route("/move", web::post().to(batch_move))
        .route("/copy", web::post().to(batch_copy))
        .route("/archive-compress", web::post().to(archive_compress))
        .route("/archive-download", web::post().to(archive_download))
        .route(
            "/archive-download/{token}",
            web::get().to(archive_download_stream),
        )
}

pub fn team_routes() -> actix_web::Scope {
    web::scope("/{team_id}/batch")
        .route("/delete", web::post().to(team_batch_delete))
        .route("/move", web::post().to(team_batch_move))
        .route("/copy", web::post().to(team_batch_copy))
        .route("/archive-compress", web::post().to(team_archive_compress))
        .route("/archive-download", web::post().to(team_archive_download))
        .route(
            "/archive-download/{token}",
            web::get().to(team_archive_download_stream),
        )
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/batch/delete",
    tag = "batch",
    operation_id = "batch_delete",
    request_body = BatchDeleteReq,
    responses(
        (status = 200, description = "Batch delete result", body = inline(ApiResponse<batch::BatchResult>)),
        (status = 400, description = "Invalid request"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
    ),
    security(("bearer" = [])),
)]
pub async fn batch_delete(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    body: web::Json<BatchDeleteReq>,
) -> Result<HttpResponse> {
    let body = body.into_inner();
    batch_delete_response(
        state.get_ref(),
        &claims,
        &req,
        WorkspaceStorageScope::Personal {
            user_id: claims.user_id,
        },
        &body,
    )
    .await
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/batch/move",
    tag = "batch",
    operation_id = "batch_move",
    request_body = BatchMoveReq,
    responses(
        (status = 200, description = "Batch move result", body = inline(ApiResponse<batch::BatchResult>)),
        (status = 400, description = "Invalid request"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
    ),
    security(("bearer" = [])),
)]
pub async fn batch_move(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    body: web::Json<BatchMoveReq>,
) -> Result<HttpResponse> {
    let body = body.into_inner();
    batch_move_response(
        state.get_ref(),
        &claims,
        &req,
        WorkspaceStorageScope::Personal {
            user_id: claims.user_id,
        },
        &body,
    )
    .await
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/batch/copy",
    tag = "batch",
    operation_id = "batch_copy",
    request_body = BatchCopyReq,
    responses(
        (status = 200, description = "Batch copy result", body = inline(ApiResponse<batch::BatchResult>)),
        (status = 400, description = "Invalid request"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
    ),
    security(("bearer" = [])),
)]
pub async fn batch_copy(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    body: web::Json<BatchCopyReq>,
) -> Result<HttpResponse> {
    let body = body.into_inner();
    batch_copy_response(
        state.get_ref(),
        &claims,
        &req,
        WorkspaceStorageScope::Personal {
            user_id: claims.user_id,
        },
        &body,
    )
    .await
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/batch/archive-download",
    tag = "batch",
    operation_id = "batch_archive_download",
    request_body = ArchiveDownloadReq,
    responses(
        (status = 200, description = "Archive download ticket", body = inline(ApiResponse<ticket::StreamTicketInfo>)),
        (status = 400, description = "Invalid request"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "ArchiveDownloadUserDisabled"),
    ),
    security(("bearer" = [])),
)]
pub async fn archive_download(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    body: web::Json<ArchiveDownloadReq>,
) -> Result<HttpResponse> {
    let body = body.into_inner();
    archive_download_ticket_response(
        state.get_ref(),
        &claims,
        &req,
        WorkspaceStorageScope::Personal {
            user_id: claims.user_id,
        },
        &body,
    )
    .await
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/batch/archive-compress",
    tag = "batch",
    operation_id = "batch_archive_compress",
    request_body = ArchiveCompressReq,
    responses(
        (status = 200, description = "Archive compress task created", body = inline(ApiResponse<task_service::types::TaskInfo>)),
        (status = 400, description = "Invalid request"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
    ),
    security(("bearer" = [])),
)]
pub async fn archive_compress(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    body: web::Json<ArchiveCompressReq>,
) -> Result<HttpResponse> {
    let body = body.into_inner();
    archive_compress_response(
        state.get_ref(),
        &claims,
        &req,
        WorkspaceStorageScope::Personal {
            user_id: claims.user_id,
        },
        &body,
    )
    .await
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/batch/archive-download/{token}",
    tag = "batch",
    operation_id = "batch_archive_download_stream",
    params(("token" = String, Path, description = "Archive download ticket")),
    responses(
        (status = 200, description = "Archive stream download"),
        (status = 400, description = "Invalid ticket"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "ArchiveDownloadUserDisabled"),
    ),
    security(("bearer" = [])),
)]
pub async fn archive_download_stream(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    let token = path.into_inner();
    archive_download_stream_response(
        state.get_ref(),
        WorkspaceStorageScope::Personal {
            user_id: claims.user_id,
        },
        &token,
    )
    .await
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/teams/{team_id}/batch/delete",
    tag = "teams",
    operation_id = "batch_delete_team",
    params(("team_id" = i64, Path, description = "Team ID")),
    request_body = BatchDeleteReq,
    responses(
        (status = 200, description = "Team batch delete result", body = inline(ApiResponse<batch::BatchResult>)),
        (status = 400, description = "Invalid request"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer" = [])),
)]
pub(crate) async fn team_batch_delete(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
    body: web::Json<BatchDeleteReq>,
) -> Result<HttpResponse> {
    let team_id = *path;
    let body = body.into_inner();
    batch_delete_response(
        state.get_ref(),
        &claims,
        &req,
        team_scope(team_id, claims.user_id),
        &body,
    )
    .await
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/teams/{team_id}/batch/move",
    tag = "teams",
    operation_id = "batch_move_team",
    params(("team_id" = i64, Path, description = "Team ID")),
    request_body = BatchMoveReq,
    responses(
        (status = 200, description = "Team batch move result", body = inline(ApiResponse<batch::BatchResult>)),
        (status = 400, description = "Invalid request"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer" = [])),
)]
pub(crate) async fn team_batch_move(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
    body: web::Json<BatchMoveReq>,
) -> Result<HttpResponse> {
    let team_id = *path;
    let body = body.into_inner();
    batch_move_response(
        state.get_ref(),
        &claims,
        &req,
        team_scope(team_id, claims.user_id),
        &body,
    )
    .await
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/teams/{team_id}/batch/copy",
    tag = "teams",
    operation_id = "batch_copy_team",
    params(("team_id" = i64, Path, description = "Team ID")),
    request_body = BatchCopyReq,
    responses(
        (status = 200, description = "Team batch copy result", body = inline(ApiResponse<batch::BatchResult>)),
        (status = 400, description = "Invalid request"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer" = [])),
)]
pub(crate) async fn team_batch_copy(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
    body: web::Json<BatchCopyReq>,
) -> Result<HttpResponse> {
    let team_id = *path;
    let body = body.into_inner();
    batch_copy_response(
        state.get_ref(),
        &claims,
        &req,
        team_scope(team_id, claims.user_id),
        &body,
    )
    .await
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/teams/{team_id}/batch/archive-download",
    tag = "teams",
    operation_id = "batch_archive_download_team",
    params(("team_id" = i64, Path, description = "Team ID")),
    request_body = ArchiveDownloadReq,
    responses(
        (status = 200, description = "Team archive download ticket", body = inline(ApiResponse<ticket::StreamTicketInfo>)),
        (status = 400, description = "Invalid request"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden or ArchiveDownloadUserDisabled"),
    ),
    security(("bearer" = [])),
)]
pub(crate) async fn team_archive_download(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
    body: web::Json<ArchiveDownloadReq>,
) -> Result<HttpResponse> {
    let team_id = *path;
    let body = body.into_inner();
    archive_download_ticket_response(
        state.get_ref(),
        &claims,
        &req,
        team_scope(team_id, claims.user_id),
        &body,
    )
    .await
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/teams/{team_id}/batch/archive-compress",
    tag = "teams",
    operation_id = "batch_archive_compress_team",
    params(("team_id" = i64, Path, description = "Team ID")),
    request_body = ArchiveCompressReq,
    responses(
        (status = 200, description = "Team archive compress task created", body = inline(ApiResponse<task_service::types::TaskInfo>)),
        (status = 400, description = "Invalid request"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer" = [])),
)]
pub(crate) async fn team_archive_compress(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
    body: web::Json<ArchiveCompressReq>,
) -> Result<HttpResponse> {
    let team_id = *path;
    let body = body.into_inner();
    archive_compress_response(
        state.get_ref(),
        &claims,
        &req,
        team_scope(team_id, claims.user_id),
        &body,
    )
    .await
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/teams/{team_id}/batch/archive-download/{token}",
    tag = "teams",
    operation_id = "batch_archive_download_stream_team",
    params(
        ("team_id" = i64, Path, description = "Team ID"),
        ("token" = String, Path, description = "Archive download ticket")
    ),
    responses(
        (status = 200, description = "Team archive stream download"),
        (status = 400, description = "Invalid ticket"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden or ArchiveDownloadUserDisabled"),
    ),
    security(("bearer" = [])),
)]
pub(crate) async fn team_archive_download_stream(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    path: web::Path<(i64, String)>,
) -> Result<HttpResponse> {
    let (team_id, token) = path.into_inner();
    archive_download_stream_response(state.get_ref(), team_scope(team_id, claims.user_id), &token)
        .await
}

pub(crate) async fn batch_delete_response(
    state: &PrimaryAppState,
    claims: &Claims,
    req: &HttpRequest,
    scope: WorkspaceStorageScope,
    body: &BatchDeleteReq,
) -> Result<HttpResponse> {
    validate_request(body)?;
    let ctx = AuditContext::from_request(req, claims);
    let result = batch::batch_delete_in_scope_with_audit(
        state,
        scope,
        &body.file_ids,
        &body.folder_ids,
        &ctx,
    )
    .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(result)))
}

pub(crate) async fn batch_move_response(
    state: &PrimaryAppState,
    claims: &Claims,
    req: &HttpRequest,
    scope: WorkspaceStorageScope,
    body: &BatchMoveReq,
) -> Result<HttpResponse> {
    validate_request(body)?;
    let ctx = AuditContext::from_request(req, claims);
    let result = batch::batch_move_in_scope_with_audit(
        state,
        scope,
        &body.file_ids,
        &body.folder_ids,
        body.target_folder_id,
        &ctx,
    )
    .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(result)))
}

pub(crate) async fn batch_copy_response(
    state: &PrimaryAppState,
    claims: &Claims,
    req: &HttpRequest,
    scope: WorkspaceStorageScope,
    body: &BatchCopyReq,
) -> Result<HttpResponse> {
    validate_request(body)?;
    let ctx = AuditContext::from_request(req, claims);
    let result = batch::batch_copy_in_scope_with_audit(
        state,
        scope,
        &body.file_ids,
        &body.folder_ids,
        body.target_folder_id,
        &ctx,
    )
    .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(result)))
}

pub(crate) async fn archive_download_ticket_response(
    state: &PrimaryAppState,
    claims: &Claims,
    req: &HttpRequest,
    scope: WorkspaceStorageScope,
    body: &ArchiveDownloadReq,
) -> Result<HttpResponse> {
    validate_request(body)?;
    ensure_user_archive_download_enabled(state)?;
    let ticket = ticket::create_archive_download_ticket_in_scope(
        state,
        scope,
        &task_service::types::CreateArchiveTaskParams {
            file_ids: body.file_ids.clone(),
            folder_ids: body.folder_ids.clone(),
            archive_name: body.archive_name.clone(),
        },
    )
    .await?;
    let ctx = AuditContext::from_request(req, claims);
    audit::log_with_details(
        state,
        &ctx,
        audit::AuditAction::ArchiveDownload,
        crate::services::ops::audit::AuditEntityType::StreamTicket,
        None,
        Some(&ticket.token),
        || {
            audit::details(audit::ArchiveSelectionAuditDetails {
                file_ids: &body.file_ids,
                folder_ids: &body.folder_ids,
                archive_name: body.archive_name.as_deref(),
                target_folder_id: None,
            })
        },
    )
    .await;

    Ok(HttpResponse::Ok().json(ApiResponse::ok(ticket)))
}

pub(crate) async fn archive_compress_response(
    state: &PrimaryAppState,
    claims: &Claims,
    req: &HttpRequest,
    scope: WorkspaceStorageScope,
    body: &ArchiveCompressReq,
) -> Result<HttpResponse> {
    validate_request(body)?;
    let task = task_service::archive::create_archive_compress_task_in_scope(
        state,
        scope,
        task_service::types::CreateArchiveCompressTaskParams {
            file_ids: body.file_ids.clone(),
            folder_ids: body.folder_ids.clone(),
            archive_name: body.archive_name.clone(),
            target_folder_id: body.target_folder_id,
        },
    )
    .await?;
    let ctx = AuditContext::from_request(req, claims);
    audit::log_with_details(
        state,
        &ctx,
        audit::AuditAction::ArchiveCompress,
        crate::services::ops::audit::AuditEntityType::Task,
        Some(task.id),
        Some(&task.display_name),
        || {
            audit::details(audit::ArchiveSelectionAuditDetails {
                file_ids: &body.file_ids,
                folder_ids: &body.folder_ids,
                archive_name: body.archive_name.as_deref(),
                target_folder_id: body.target_folder_id,
            })
        },
    )
    .await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(task)))
}

pub(crate) async fn archive_download_stream_response(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    token: &str,
) -> Result<HttpResponse> {
    ensure_user_archive_download_enabled(state)?;
    let params = ticket::resolve_archive_download_ticket_in_scope(state, scope, token).await?;
    task_service::archive::stream_archive_download_in_scope(state, scope, params).await
}

fn ensure_user_archive_download_enabled(state: &PrimaryAppState) -> Result<()> {
    if !operations::archive_download_user_enabled(state.runtime_config()) {
        return Err(auth_forbidden_with_code(
            ApiErrorCode::ArchiveDownloadUserDisabled,
            "archive downloads for personal and team files are disabled by the administrator",
        ));
    }
    Ok(())
}
