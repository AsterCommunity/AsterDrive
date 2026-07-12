//! API 路由：`webdav_accounts`。

use crate::api::dto::validate_request;
use crate::api::dto::webdav::{CreateWebdavAccountReq, TestConnectionReq, WebdavSettingsInfo};
use crate::api::middleware::auth::JwtAuth;
use crate::api::middleware::rate_limit;
use crate::api::response::ApiResponse;
use crate::config::site_url;
use crate::config::{NetworkTrustConfig, RateLimitConfig};
use crate::db::repository::webdav_account_repo;
use crate::errors::Result;
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::{auth::local::Claims, ops::audit, webdav::account};
use actix_governor::Governor;
use actix_web::middleware::Condition;
use actix_web::{HttpRequest, HttpResponse, web};
use aster_forge_api::LimitOffsetQuery;
#[cfg(all(debug_assertions, feature = "openapi"))]
use aster_forge_api::OffsetPage;
use serde_json::json;

pub fn routes(
    rl: &RateLimitConfig,
    network_trust: &NetworkTrustConfig,
) -> impl actix_web::dev::HttpServiceFactory + use<> {
    let limiter = rate_limit::build_governor(&rl.api, &network_trust.trusted_proxies);

    web::scope("/webdav-accounts")
        .wrap(JwtAuth)
        .wrap(Condition::new(rl.enabled, Governor::new(&limiter)))
        .route("", web::get().to(list_accounts))
        .route("", web::post().to(create_account))
        .route("/{id}", web::delete().to(delete_account))
        .route("/{id}/toggle", web::post().to(toggle_account))
        .route("/settings", web::get().to(get_settings))
        .route("/test", web::post().to(test_connection))
}

#[aster_forge_api_docs_macros::path(
    get,
    path = "/api/v1/webdav-accounts/settings",
    tag = "webdav",
    operation_id = "get_webdav_settings",
    responses(
        (status = 200, description = "Current WebDAV settings for the signed-in user", body = inline(ApiResponse<WebdavSettingsInfo>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
    ),
    security(("bearer" = [])),
)]
pub async fn get_settings(
    state: web::Data<PrimaryAppState>,
    req: HttpRequest,
) -> Result<HttpResponse> {
    let endpoint_path = if state.config().webdav.prefix == "/" {
        "/".to_string()
    } else {
        format!("{}/", state.config().webdav.prefix.trim_end_matches('/'))
    };
    let conn = req.connection_info();

    Ok(HttpResponse::Ok().json(ApiResponse::ok(WebdavSettingsInfo {
        prefix: state.config().webdav.prefix.clone(),
        endpoint: site_url::public_app_url_or_path_for_request(
            state.get_ref().runtime_config(),
            &endpoint_path,
            conn.scheme(),
            conn.host(),
        ),
    })))
}

#[aster_forge_api_docs_macros::path(
    get,
    path = "/api/v1/webdav-accounts",
    tag = "webdav",
    operation_id = "list_webdav_accounts",
    params(LimitOffsetQuery),
    responses(
        (status = 200, description = "WebDAV accounts", body = inline(ApiResponse<OffsetPage<account::WebdavAccountInfo>>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
    ),
    security(("bearer" = [])),
)]
pub async fn list_accounts(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    query: web::Query<LimitOffsetQuery>,
) -> Result<HttpResponse> {
    let accounts = account::list_paginated(
        state.get_ref(),
        claims.user_id,
        query.limit_or(50, 100),
        query.offset(),
    )
    .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(accounts)))
}

#[aster_forge_api_docs_macros::path(
    post,
    path = "/api/v1/webdav-accounts",
    tag = "webdav",
    operation_id = "create_webdav_account",
    request_body = CreateWebdavAccountReq,
    responses(
        (status = 201, description = "Account created (password shown once)", body = inline(ApiResponse<account::WebdavAccountCreated>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
    ),
    security(("bearer" = [])),
)]
pub async fn create_account(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    body: web::Json<CreateWebdavAccountReq>,
) -> Result<HttpResponse> {
    validate_request(&*body)?;
    let result = account::create(
        state.get_ref(),
        claims.user_id,
        &body.username,
        body.password.as_deref(),
        body.root_folder_id,
    )
    .await?;
    let ctx = audit::AuditContext::from_request(&req, &claims);
    let details = webdav_created_audit_details(&result, body.root_folder_id);
    audit::log_with_details(
        state.get_ref(),
        &ctx,
        audit::AuditAction::WebdavAccountCreate,
        crate::services::ops::audit::AuditEntityType::WebdavAccount,
        Some(result.id),
        Some(&result.username),
        || Some(details.clone()),
    )
    .await;
    Ok(HttpResponse::Created().json(ApiResponse::ok(result)))
}

#[derive(Debug, serde::Deserialize)]
pub struct TeamWebdavAccountPath {
    pub team_id: i64,
}

#[derive(Debug, serde::Deserialize)]
pub struct TeamWebdavAccountIdPath {
    pub team_id: i64,
    pub account_id: i64,
}

#[aster_forge_api_docs_macros::path(
    get,
    path = "/api/v1/teams/{team_id}/webdav-accounts",
    tag = "webdav",
    operation_id = "list_team_webdav_accounts",
    params(("team_id" = i64, Path, description = "Team ID"), LimitOffsetQuery),
    responses(
        (status = 200, description = "Team WebDAV accounts", body = inline(ApiResponse<OffsetPage<account::WebdavAccountInfo>>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
    ),
    security(("bearer" = [])),
)]
pub async fn list_team_accounts(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    path: web::Path<TeamWebdavAccountPath>,
    query: web::Query<LimitOffsetQuery>,
) -> Result<HttpResponse> {
    let accounts = account::list_team_paginated(
        state.get_ref(),
        claims.user_id,
        path.team_id,
        query.limit_or(50, 100),
        query.offset(),
    )
    .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(accounts)))
}

#[aster_forge_api_docs_macros::path(
    post,
    path = "/api/v1/teams/{team_id}/webdav-accounts",
    tag = "webdav",
    operation_id = "create_team_webdav_account",
    params(("team_id" = i64, Path, description = "Team ID")),
    request_body = CreateWebdavAccountReq,
    responses(
        (status = 201, description = "Team WebDAV account created", body = inline(ApiResponse<account::WebdavAccountCreated>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
    ),
    security(("bearer" = [])),
)]
pub async fn create_team_account(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<TeamWebdavAccountPath>,
    body: web::Json<CreateWebdavAccountReq>,
) -> Result<HttpResponse> {
    validate_request(&*body)?;
    let result = account::create_for_team(
        state.get_ref(),
        claims.user_id,
        path.team_id,
        &body.username,
        body.password.as_deref(),
        body.root_folder_id,
    )
    .await?;
    let ctx = audit::AuditContext::from_request(&req, &claims);
    let details = webdav_created_audit_details(&result, body.root_folder_id);
    audit::log_with_details(
        state.get_ref(),
        &ctx,
        audit::AuditAction::TeamWebdavAccountCreate,
        crate::services::ops::audit::AuditEntityType::WebdavAccount,
        Some(result.id),
        Some(&result.username),
        || Some(details.clone()),
    )
    .await;
    Ok(HttpResponse::Created().json(ApiResponse::ok(result)))
}

#[aster_forge_api_docs_macros::path(
    delete,
    path = "/api/v1/teams/{team_id}/webdav-accounts/{account_id}",
    tag = "webdav",
    operation_id = "delete_team_webdav_account",
    params(
        ("team_id" = i64, Path, description = "Team ID"),
        ("account_id" = i64, Path, description = "Account ID"),
    ),
    responses(
        (status = 200, description = "Team WebDAV account deleted"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 404, description = crate::api::constants::OPENAPI_NOT_FOUND),
    ),
    security(("bearer" = [])),
)]
pub async fn delete_team_account(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<TeamWebdavAccountIdPath>,
) -> Result<HttpResponse> {
    let account = webdav_account_repo::find_by_id(state.writer_db(), path.account_id).await?;
    let details = webdav_account_audit_details(&account);
    account::delete_for_team(
        state.get_ref(),
        path.account_id,
        claims.user_id,
        path.team_id,
    )
    .await?;
    let ctx = audit::AuditContext::from_request(&req, &claims);
    audit::log_with_details(
        state.get_ref(),
        &ctx,
        audit::AuditAction::TeamWebdavAccountDelete,
        crate::services::ops::audit::AuditEntityType::WebdavAccount,
        Some(path.account_id),
        Some(&account.username),
        || Some(details.clone()),
    )
    .await;
    Ok(HttpResponse::Ok().json(ApiResponse::<()>::ok_empty()))
}

#[aster_forge_api_docs_macros::path(
    post,
    path = "/api/v1/teams/{team_id}/webdav-accounts/{account_id}/toggle",
    tag = "webdav",
    operation_id = "toggle_team_webdav_account",
    params(
        ("team_id" = i64, Path, description = "Team ID"),
        ("account_id" = i64, Path, description = "Account ID"),
    ),
    responses(
        (status = 200, description = "Team WebDAV account toggled", body = inline(ApiResponse<account::WebdavAccount>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 404, description = crate::api::constants::OPENAPI_NOT_FOUND),
    ),
    security(("bearer" = [])),
)]
pub async fn toggle_team_account(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<TeamWebdavAccountIdPath>,
) -> Result<HttpResponse> {
    let account = account::toggle_team_active(
        state.get_ref(),
        path.account_id,
        claims.user_id,
        path.team_id,
    )
    .await?;
    let ctx = audit::AuditContext::from_request(&req, &claims);
    audit::log_with_details(
        state.get_ref(),
        &ctx,
        audit::AuditAction::TeamWebdavAccountToggle,
        crate::services::ops::audit::AuditEntityType::WebdavAccount,
        Some(account.id),
        Some(&account.username),
        || {
            audit::details(serde_json::json!({
                "team_id": path.team_id,
                "is_active": account.is_active,
            }))
        },
    )
    .await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(account)))
}

#[aster_forge_api_docs_macros::path(
    delete,
    path = "/api/v1/webdav-accounts/{id}",
    tag = "webdav",
    operation_id = "delete_webdav_account",
    params(("id" = i64, Path, description = "Account ID")),
    responses(
        (status = 200, description = "Account deleted"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 404, description = crate::api::constants::OPENAPI_NOT_FOUND),
    ),
    security(("bearer" = [])),
)]
pub async fn delete_account(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
) -> Result<HttpResponse> {
    let account = webdav_account_repo::find_by_id(state.writer_db(), *path).await?;
    let details = webdav_account_audit_details(&account);
    account::delete(state.get_ref(), *path, claims.user_id).await?;
    let ctx = audit::AuditContext::from_request(&req, &claims);
    audit::log_with_details(
        state.get_ref(),
        &ctx,
        audit::AuditAction::WebdavAccountDelete,
        crate::services::ops::audit::AuditEntityType::WebdavAccount,
        Some(*path),
        Some(&account.username),
        || Some(details.clone()),
    )
    .await;
    Ok(HttpResponse::Ok().json(ApiResponse::<()>::ok_empty()))
}

#[aster_forge_api_docs_macros::path(
    post,
    path = "/api/v1/webdav-accounts/{id}/toggle",
    tag = "webdav",
    operation_id = "toggle_webdav_account",
    params(("id" = i64, Path, description = "Account ID")),
    responses(
        (status = 200, description = "Account toggled", body = inline(ApiResponse<account::WebdavAccount>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 404, description = crate::api::constants::OPENAPI_NOT_FOUND),
    ),
    security(("bearer" = [])),
)]
pub async fn toggle_account(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
) -> Result<HttpResponse> {
    let account = account::toggle_active(state.get_ref(), *path, claims.user_id).await?;
    let ctx = audit::AuditContext::from_request(&req, &claims);
    audit::log_with_details(
        state.get_ref(),
        &ctx,
        audit::AuditAction::WebdavAccountToggle,
        crate::services::ops::audit::AuditEntityType::WebdavAccount,
        Some(account.id),
        Some(&account.username),
        || {
            audit::details(serde_json::json!({
                "is_active": account.is_active,
            }))
        },
    )
    .await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(account)))
}

#[aster_forge_api_docs_macros::path(
    post,
    path = "/api/v1/webdav-accounts/test",
    tag = "webdav",
    operation_id = "test_webdav_connection",
    request_body = TestConnectionReq,
    responses(
        (status = 200, description = "Connection successful"),
        (status = 401, description = "Invalid credentials"),
    ),
    security(("bearer" = [])),
)]
pub async fn test_connection(
    state: web::Data<PrimaryAppState>,
    body: web::Json<TestConnectionReq>,
) -> Result<HttpResponse> {
    validate_request(&*body)?;
    account::test_credentials(state.get_ref(), &body.username, &body.password).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::<()>::ok_empty()))
}

fn webdav_created_audit_details(
    account: &account::WebdavAccountCreated,
    root_folder_id: Option<i64>,
) -> serde_json::Value {
    json!({
        "username": &account.username,
        "team_id": account.team_id,
        "root_folder_id": root_folder_id,
        "root_folder_path": account.root_folder_path.as_deref(),
        "is_active": true,
    })
}

fn webdav_account_audit_details(
    account: &crate::entities::webdav_account::Model,
) -> serde_json::Value {
    json!({
        "username": &account.username,
        "team_id": account.team_id,
        "root_folder_id": account.root_folder_id,
        "is_active": account.is_active,
    })
}
