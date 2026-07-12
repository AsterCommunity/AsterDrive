//! API 路由：`shares`。

pub use crate::api::dto::shares::*;
use crate::api::dto::validate_request;
use crate::api::middleware::auth::JwtAuth;
use crate::api::middleware::rate_limit;
use crate::api::response::ApiResponse;
use crate::api::routes::team_scope;
use crate::config::{NetworkTrustConfig, RateLimitConfig};
use crate::errors::Result;
use crate::runtime::PrimaryAppState;
#[cfg(all(debug_assertions, feature = "openapi"))]
use crate::services::files::batch;
use crate::services::{
    auth::local::Claims, ops::audit::AuditContext, share, workspace::storage::WorkspaceStorageScope,
};
use actix_governor::Governor;
use actix_web::middleware::Condition;
use actix_web::{HttpRequest, HttpResponse, web};
use aster_forge_api::LimitOffsetQuery;
#[cfg(all(debug_assertions, feature = "openapi"))]
use aster_forge_api::OffsetPage;

pub fn routes(
    rl: &RateLimitConfig,
    network_trust: &NetworkTrustConfig,
) -> impl actix_web::dev::HttpServiceFactory + use<> {
    let limiter = rate_limit::build_governor(&rl.api, &network_trust.trusted_proxies);

    web::scope("/shares")
        .wrap(JwtAuth)
        .wrap(Condition::new(rl.enabled, Governor::new(&limiter)))
        .route("", web::post().to(create_share))
        .route("", web::get().to(list_shares))
        .route("/batch-delete", web::post().to(batch_delete_shares))
        .route("/{id}", web::patch().to(update_share))
        .route("/{id}", web::delete().to(delete_share))
}

pub fn team_routes() -> actix_web::Scope {
    web::scope("/{team_id}/shares")
        .route("", web::post().to(team_create_share))
        .route("", web::get().to(team_list_shares))
        .route("/batch-delete", web::post().to(team_batch_delete_shares))
        .route("/{id}", web::patch().to(team_update_share))
        .route("/{id}", web::delete().to(team_delete_share))
}

#[aster_forge_api_docs_macros::path(
    post,
    path = "/api/v1/shares",
    tag = "shares",
    operation_id = "create_share",
    request_body = CreateShareReq,
    responses(
        (status = 201, description = "Share created", body = inline(ApiResponse<crate::services::share::ShareInfo>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
    ),
    security(("bearer" = [])),
)]
pub async fn create_share(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    body: web::Json<CreateShareReq>,
) -> Result<HttpResponse> {
    let body = body.into_inner();
    create_share_response(
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

#[aster_forge_api_docs_macros::path(
    get,
    path = "/api/v1/shares",
    tag = "shares",
    operation_id = "list_my_shares",
    params(LimitOffsetQuery),
    responses(
        (status = 200, description = "My shares", body = inline(ApiResponse<OffsetPage<crate::services::share::MyShareInfo>>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
    ),
    security(("bearer" = [])),
)]
pub async fn list_shares(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    query: web::Query<LimitOffsetQuery>,
) -> Result<HttpResponse> {
    list_shares_response(
        state.get_ref(),
        WorkspaceStorageScope::Personal {
            user_id: claims.user_id,
        },
        &query,
    )
    .await
}

#[aster_forge_api_docs_macros::path(
    patch,
    path = "/api/v1/shares/{id}",
    tag = "shares",
    operation_id = "update_share",
    params(("id" = i64, Path, description = "Share ID")),
    request_body = UpdateShareReq,
    responses(
        (status = 200, description = "Share updated", body = inline(ApiResponse<crate::services::share::ShareInfo>)),
        (status = 400, description = "Invalid request"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 404, description = "Share not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn update_share(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
    body: web::Json<UpdateShareReq>,
) -> Result<HttpResponse> {
    let body = body.into_inner();
    update_share_response(
        state.get_ref(),
        &claims,
        &req,
        WorkspaceStorageScope::Personal {
            user_id: claims.user_id,
        },
        *path,
        &body,
    )
    .await
}

#[aster_forge_api_docs_macros::path(
    delete,
    path = "/api/v1/shares/{id}",
    tag = "shares",
    operation_id = "delete_share",
    params(("id" = i64, Path, description = "Share ID")),
    responses(
        (status = 200, description = "Share deleted"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 404, description = "Share not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn delete_share(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
) -> Result<HttpResponse> {
    delete_share_response(
        state.get_ref(),
        &claims,
        &req,
        WorkspaceStorageScope::Personal {
            user_id: claims.user_id,
        },
        *path,
    )
    .await
}

#[aster_forge_api_docs_macros::path(
    post,
    path = "/api/v1/shares/batch-delete",
    tag = "shares",
    operation_id = "batch_delete_shares",
    request_body = BatchDeleteSharesReq,
    responses(
        (status = 200, description = "Batch delete result", body = inline(ApiResponse<batch::BatchResult>)),
        (status = 400, description = "Invalid request"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
    ),
    security(("bearer" = [])),
)]
pub async fn batch_delete_shares(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    body: web::Json<BatchDeleteSharesReq>,
) -> Result<HttpResponse> {
    let body = body.into_inner();
    batch_delete_shares_response(
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

#[aster_forge_api_docs_macros::path(
    post,
    path = "/api/v1/teams/{team_id}/shares",
    tag = "teams",
    operation_id = "create_team_share",
    params(("team_id" = i64, Path, description = "Team ID")),
    request_body = CreateShareReq,
    responses(
        (status = 201, description = "Team share created", body = inline(ApiResponse<share::ShareInfo>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer" = [])),
)]
pub(crate) async fn team_create_share(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
    body: web::Json<CreateShareReq>,
) -> Result<HttpResponse> {
    let team_id = *path;
    let body = body.into_inner();
    create_share_response(
        state.get_ref(),
        &claims,
        &req,
        team_scope(team_id, claims.user_id),
        &body,
    )
    .await
}

#[aster_forge_api_docs_macros::path(
    get,
    path = "/api/v1/teams/{team_id}/shares",
    tag = "teams",
    operation_id = "list_team_shares",
    params(
        ("team_id" = i64, Path, description = "Team ID"),
        LimitOffsetQuery
    ),
    responses(
        (status = 200, description = "Team shares", body = inline(ApiResponse<OffsetPage<share::MyShareInfo>>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer" = [])),
)]
pub(crate) async fn team_list_shares(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    path: web::Path<i64>,
    query: web::Query<LimitOffsetQuery>,
) -> Result<HttpResponse> {
    list_shares_response(state.get_ref(), team_scope(*path, claims.user_id), &query).await
}

#[aster_forge_api_docs_macros::path(
    patch,
    path = "/api/v1/teams/{team_id}/shares/{id}",
    tag = "teams",
    operation_id = "update_team_share",
    params(
        ("team_id" = i64, Path, description = "Team ID"),
        ("id" = i64, Path, description = "Share ID")
    ),
    request_body = UpdateShareReq,
    responses(
        (status = 200, description = "Team share updated", body = inline(ApiResponse<share::ShareInfo>)),
        (status = 400, description = "Invalid request"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Share not found"),
    ),
    security(("bearer" = [])),
)]
pub(crate) async fn team_update_share(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<(i64, i64)>,
    body: web::Json<UpdateShareReq>,
) -> Result<HttpResponse> {
    let (team_id, share_id) = path.into_inner();
    let body = body.into_inner();
    update_share_response(
        state.get_ref(),
        &claims,
        &req,
        team_scope(team_id, claims.user_id),
        share_id,
        &body,
    )
    .await
}

#[aster_forge_api_docs_macros::path(
    delete,
    path = "/api/v1/teams/{team_id}/shares/{id}",
    tag = "teams",
    operation_id = "delete_team_share",
    params(
        ("team_id" = i64, Path, description = "Team ID"),
        ("id" = i64, Path, description = "Share ID")
    ),
    responses(
        (status = 200, description = "Team share deleted"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Share not found"),
    ),
    security(("bearer" = [])),
)]
pub(crate) async fn team_delete_share(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<(i64, i64)>,
) -> Result<HttpResponse> {
    let (team_id, share_id) = path.into_inner();
    delete_share_response(
        state.get_ref(),
        &claims,
        &req,
        team_scope(team_id, claims.user_id),
        share_id,
    )
    .await
}

#[aster_forge_api_docs_macros::path(
    post,
    path = "/api/v1/teams/{team_id}/shares/batch-delete",
    tag = "teams",
    operation_id = "batch_delete_team_shares",
    params(("team_id" = i64, Path, description = "Team ID")),
    request_body = BatchDeleteSharesReq,
    responses(
        (status = 200, description = "Batch delete result", body = inline(ApiResponse<batch::BatchResult>)),
        (status = 400, description = "Invalid request"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer" = [])),
)]
pub(crate) async fn team_batch_delete_shares(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
    body: web::Json<BatchDeleteSharesReq>,
) -> Result<HttpResponse> {
    let team_id = *path;
    let body = body.into_inner();
    batch_delete_shares_response(
        state.get_ref(),
        &claims,
        &req,
        team_scope(team_id, claims.user_id),
        &body,
    )
    .await
}

pub(crate) async fn create_share_response(
    state: &PrimaryAppState,
    claims: &Claims,
    req: &HttpRequest,
    scope: WorkspaceStorageScope,
    body: &CreateShareReq,
) -> Result<HttpResponse> {
    validate_request(body)?;
    let ctx = AuditContext::from_request(req, claims);
    let share = share::create_share_in_scope_with_audit(
        state,
        scope,
        body.target,
        body.password.clone(),
        body.expires_at,
        body.max_downloads,
        &ctx,
    )
    .await?;
    Ok(HttpResponse::Created().json(ApiResponse::ok(share)))
}

pub(crate) async fn list_shares_response(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    query: &LimitOffsetQuery,
) -> Result<HttpResponse> {
    let shares = share::list_shares_paginated_in_scope(
        state,
        scope,
        query.limit_or(50, 100),
        query.offset(),
    )
    .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(shares)))
}

pub(crate) async fn update_share_response(
    state: &PrimaryAppState,
    claims: &Claims,
    req: &HttpRequest,
    scope: WorkspaceStorageScope,
    share_id: i64,
    body: &UpdateShareReq,
) -> Result<HttpResponse> {
    validate_request(body)?;
    let ctx = AuditContext::from_request(req, claims);
    let share = share::update_share_in_scope_with_audit(
        state,
        scope,
        share_id,
        body.password.clone(),
        body.expires_at,
        body.max_downloads,
        &ctx,
    )
    .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(share)))
}

pub(crate) async fn delete_share_response(
    state: &PrimaryAppState,
    claims: &Claims,
    req: &HttpRequest,
    scope: WorkspaceStorageScope,
    share_id: i64,
) -> Result<HttpResponse> {
    let ctx = AuditContext::from_request(req, claims);
    share::delete_share_in_scope_with_audit(state, scope, share_id, &ctx).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::<()>::ok_empty()))
}

pub(crate) async fn batch_delete_shares_response(
    state: &PrimaryAppState,
    claims: &Claims,
    req: &HttpRequest,
    scope: WorkspaceStorageScope,
    body: &BatchDeleteSharesReq,
) -> Result<HttpResponse> {
    validate_request(body)?;
    let ctx = AuditContext::from_request(req, claims);
    let result =
        share::batch_delete_shares_in_scope_with_audit(state, scope, &body.share_ids, &ctx).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(result)))
}
