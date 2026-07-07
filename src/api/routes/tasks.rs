//! API 路由：`tasks`。

use crate::api::middleware::auth::JwtAuth;
use crate::api::middleware::rate_limit;
use crate::api::pagination::LimitOffsetQuery;
#[cfg(all(debug_assertions, feature = "openapi"))]
use crate::api::pagination::OffsetPage;
use crate::api::response::ApiResponse;
use crate::api::routes::team_scope;
use crate::config::{NetworkTrustConfig, RateLimitConfig};
use crate::errors::Result;
use crate::runtime::PrimaryAppState;
use crate::services::{
    audit_service::{self, AuditContext},
    auth::local::Claims,
    task_service,
    workspace_storage_service::WorkspaceStorageScope,
};
use actix_governor::Governor;
use actix_web::middleware::Condition;
use actix_web::{HttpRequest, HttpResponse, web};

pub fn routes(
    rl: &RateLimitConfig,
    network_trust: &NetworkTrustConfig,
) -> impl actix_web::dev::HttpServiceFactory + use<> {
    let limiter = rate_limit::build_governor(&rl.api, &network_trust.trusted_proxies);

    web::scope("/tasks")
        .wrap(JwtAuth)
        .wrap(Condition::new(rl.enabled, Governor::new(&limiter)))
        .route("", web::get().to(list_tasks))
        .route("/offline-download", web::post().to(create_offline_download))
        .route("/{id}", web::get().to(get_task))
        .route("/{id}/retry", web::post().to(retry_task))
}

pub fn team_routes() -> actix_web::Scope {
    web::scope("/{team_id}/tasks")
        .route("", web::get().to(team_list_tasks))
        .route(
            "/offline-download",
            web::post().to(team_create_offline_download),
        )
        .route("/{id}", web::get().to(team_get_task))
        .route("/{id}/retry", web::post().to(team_retry_task))
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/tasks",
    tag = "tasks",
    operation_id = "list_tasks",
    params(LimitOffsetQuery),
    responses(
        (status = 200, description = "Background tasks", body = inline(ApiResponse<OffsetPage<task_service::types::TaskInfo>>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
    ),
    security(("bearer" = [])),
)]
pub async fn list_tasks(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    query: web::Query<LimitOffsetQuery>,
) -> Result<HttpResponse> {
    list_tasks_response(
        state.get_ref(),
        WorkspaceStorageScope::Personal {
            user_id: claims.user_id,
        },
        &query,
    )
    .await
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/tasks/{id}",
    tag = "tasks",
    operation_id = "get_task",
    params(("id" = i64, Path, description = "Task ID")),
    responses(
        (status = 200, description = "Task details", body = inline(ApiResponse<task_service::types::TaskInfo>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 404, description = "Task not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn get_task(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    path: web::Path<i64>,
) -> Result<HttpResponse> {
    get_task_response(
        state.get_ref(),
        WorkspaceStorageScope::Personal {
            user_id: claims.user_id,
        },
        *path,
    )
    .await
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/tasks/{id}/retry",
    tag = "tasks",
    operation_id = "retry_task",
    params(("id" = i64, Path, description = "Task ID")),
    responses(
        (status = 200, description = "Task reset for retry", body = inline(ApiResponse<task_service::types::TaskInfo>)),
        (status = 400, description = "Task is not retryable"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 404, description = "Task not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn retry_task(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
) -> Result<HttpResponse> {
    let ctx = AuditContext::from_request(&req, &claims);
    retry_task_response(
        state.get_ref(),
        WorkspaceStorageScope::Personal {
            user_id: claims.user_id,
        },
        *path,
        &ctx,
    )
    .await
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/tasks/offline-download",
    tag = "tasks",
    operation_id = "create_offline_download_task",
    request_body = task_service::types::CreateOfflineDownloadTaskParams,
    responses(
        (status = 200, description = "Offline download task created", body = inline(ApiResponse<task_service::types::TaskInfo>)),
        (status = 400, description = "Invalid offline download request"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
    ),
    security(("bearer" = [])),
)]
pub async fn create_offline_download(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    body: web::Json<task_service::types::CreateOfflineDownloadTaskParams>,
) -> Result<HttpResponse> {
    create_offline_download_response(
        state.get_ref(),
        &claims,
        &req,
        WorkspaceStorageScope::Personal {
            user_id: claims.user_id,
        },
        body.into_inner(),
    )
    .await
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/teams/{team_id}/tasks",
    tag = "teams",
    operation_id = "list_team_tasks",
    params(
        ("team_id" = i64, Path, description = "Team ID"),
        LimitOffsetQuery
    ),
    responses(
        (status = 200, description = "Team tasks", body = inline(ApiResponse<OffsetPage<task_service::types::TaskInfo>>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer" = [])),
)]
pub(crate) async fn team_list_tasks(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    path: web::Path<i64>,
    query: web::Query<LimitOffsetQuery>,
) -> Result<HttpResponse> {
    list_tasks_response(state.get_ref(), team_scope(*path, claims.user_id), &query).await
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/teams/{team_id}/tasks/{id}",
    tag = "teams",
    operation_id = "get_team_task",
    params(
        ("team_id" = i64, Path, description = "Team ID"),
        ("id" = i64, Path, description = "Task ID")
    ),
    responses(
        (status = 200, description = "Team task details", body = inline(ApiResponse<task_service::types::TaskInfo>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Task not found"),
    ),
    security(("bearer" = [])),
)]
pub(crate) async fn team_get_task(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    path: web::Path<(i64, i64)>,
) -> Result<HttpResponse> {
    let (team_id, task_id) = path.into_inner();
    get_task_response(
        state.get_ref(),
        team_scope(team_id, claims.user_id),
        task_id,
    )
    .await
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/teams/{team_id}/tasks/{id}/retry",
    tag = "teams",
    operation_id = "retry_team_task",
    params(
        ("team_id" = i64, Path, description = "Team ID"),
        ("id" = i64, Path, description = "Task ID")
    ),
    responses(
        (status = 200, description = "Team task reset for retry", body = inline(ApiResponse<task_service::types::TaskInfo>)),
        (status = 400, description = "Task is not retryable"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Task not found"),
    ),
    security(("bearer" = [])),
)]
pub(crate) async fn team_retry_task(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<(i64, i64)>,
) -> Result<HttpResponse> {
    let (team_id, task_id) = path.into_inner();
    let ctx = AuditContext::from_request(&req, &claims);
    retry_task_response(
        state.get_ref(),
        team_scope(team_id, claims.user_id),
        task_id,
        &ctx,
    )
    .await
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/teams/{team_id}/tasks/offline-download",
    tag = "teams",
    operation_id = "create_team_offline_download_task",
    params(("team_id" = i64, Path, description = "Team ID")),
    request_body = task_service::types::CreateOfflineDownloadTaskParams,
    responses(
        (status = 200, description = "Team offline download task created", body = inline(ApiResponse<task_service::types::TaskInfo>)),
        (status = 400, description = "Invalid offline download request"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer" = [])),
)]
pub(crate) async fn team_create_offline_download(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
    body: web::Json<task_service::types::CreateOfflineDownloadTaskParams>,
) -> Result<HttpResponse> {
    create_offline_download_response(
        state.get_ref(),
        &claims,
        &req,
        team_scope(*path, claims.user_id),
        body.into_inner(),
    )
    .await
}

pub(crate) async fn list_tasks_response(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    query: &LimitOffsetQuery,
) -> Result<HttpResponse> {
    let page = task_service::list_tasks_paginated_in_scope(
        state,
        scope,
        query.limit_or(20, 100),
        query.offset(),
    )
    .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(page)))
}

pub(crate) async fn get_task_response(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    task_id: i64,
) -> Result<HttpResponse> {
    let task = task_service::get_task_in_scope(state, scope, task_id).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(task)))
}

pub(crate) async fn retry_task_response(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    task_id: i64,
    audit_ctx: &AuditContext,
) -> Result<HttpResponse> {
    let task =
        task_service::retry_task_in_scope_with_audit(state, scope, task_id, audit_ctx).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(task)))
}

pub(crate) async fn create_offline_download_response(
    state: &PrimaryAppState,
    claims: &Claims,
    req: &HttpRequest,
    scope: WorkspaceStorageScope,
    body: task_service::types::CreateOfflineDownloadTaskParams,
) -> Result<HttpResponse> {
    let ctx = AuditContext::from_request(req, claims);
    let task =
        task_service::offline_download::create_offline_download_task_in_scope(state, scope, body)
            .await?;
    audit_service::log_with_details(
        state,
        &ctx,
        audit_service::AuditAction::OfflineDownload,
        crate::services::audit_service::AuditEntityType::Task,
        Some(task.id),
        Some(&task.display_name),
        || {
            audit_service::details(serde_json::json!({
                "task_id": task.id,
                "target_folder_id": match &task.payload {
                    task_service::types::TaskPayload::OfflineDownload(payload) => payload.target_folder_id,
                    _ => None,
                },
                "source": match &task.payload {
                    task_service::types::TaskPayload::OfflineDownload(payload) => payload.source_display_url.clone(),
                    _ => "external link".to_string(),
                },
            }))
        },
    )
    .await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(task)))
}
