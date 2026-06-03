//! API 路由：Google Drive 存储策略授权。

use crate::api::middleware::{admin::RequireAdmin, auth::JwtAuth, rate_limit};
use crate::api::response::ApiResponse;
use crate::config::site_url;
use crate::config::{NetworkTrustConfig, RateLimitConfig};
use crate::errors::{AsterError, Result};
use crate::runtime::PrimaryAppState;
use crate::services::google_drive_storage_service;
use actix_governor::Governor;
use actix_web::http::header;
use actix_web::middleware::Condition;
use actix_web::{HttpRequest, HttpResponse, web};

pub fn routes(
    rl: &RateLimitConfig,
    network_trust: &NetworkTrustConfig,
) -> impl actix_web::dev::HttpServiceFactory + use<> {
    let api_limiter = rate_limit::build_governor(&rl.api, &network_trust.trusted_proxies);
    let auth_limiter = rate_limit::build_governor(&rl.auth, &network_trust.trusted_proxies);

    web::scope("/admin/policies/google-drive")
        .service(
            web::resource("/oauth/callback")
                .wrap(Condition::new(rl.enabled, Governor::new(&auth_limiter)))
                .route(web::get().to(finish_policy_auth_callback)),
        )
        .service(
            web::scope("")
                .wrap(JwtAuth)
                .wrap(RequireAdmin)
                .wrap(Condition::new(rl.enabled, Governor::new(&api_limiter)))
                .route(
                    "/{policy_id}/auth/status",
                    web::get().to(get_policy_auth_status),
                )
                .route("/{policy_id}/auth/start", web::post().to(start_policy_auth)),
        )
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/admin/policies/google-drive/{policy_id}/auth/status",
    tag = "admin",
    operation_id = "get_google_drive_policy_auth_status",
    params(("policy_id" = i64, Path, description = "Storage policy ID")),
    responses(
        (status = 200, description = "Google Drive policy authorization status", body = inline(ApiResponse<google_drive_storage_service::GoogleDrivePolicyAuthStatus>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer" = [])),
)]
pub async fn get_policy_auth_status(
    state: web::Data<PrimaryAppState>,
    path: web::Path<i64>,
) -> Result<HttpResponse> {
    let status = google_drive_storage_service::get_policy_auth_status(&state, *path).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(status)))
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/admin/policies/google-drive/{policy_id}/auth/start",
    tag = "admin",
    operation_id = "start_google_drive_policy_auth",
    params(("policy_id" = i64, Path, description = "Storage policy ID")),
    request_body = google_drive_storage_service::GoogleDriveStartPolicyAuthRequest,
    responses(
        (status = 200, description = "Google Drive authorization URL", body = inline(ApiResponse<google_drive_storage_service::GoogleDriveStartPolicyAuthResponse>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer" = [])),
)]
pub async fn start_policy_auth(
    state: web::Data<PrimaryAppState>,
    req: HttpRequest,
    path: web::Path<i64>,
    body: web::Json<google_drive_storage_service::GoogleDriveStartPolicyAuthRequest>,
) -> Result<HttpResponse> {
    let response = google_drive_storage_service::start_policy_auth(
        &state,
        &req,
        *path,
        body.return_path.as_deref(),
    )
    .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(response)))
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/admin/policies/google-drive/oauth/callback",
    tag = "admin",
    operation_id = "finish_google_drive_policy_auth",
    params(google_drive_storage_service::GoogleDrivePolicyAuthCallbackQuery),
    responses(
        (status = 302, description = "Google Drive authorization completed and redirected"),
        (status = 302, description = "Invalid Google Drive callback redirected to admin policies"),
    ),
)]
pub async fn finish_policy_auth_callback(
    state: web::Data<PrimaryAppState>,
    query: web::Query<google_drive_storage_service::GoogleDrivePolicyAuthCallbackQuery>,
) -> Result<HttpResponse> {
    match google_drive_storage_service::finish_policy_auth_callback(&state, &query).await {
        Ok(result) => Ok(google_drive_status_redirect_response(
            &state,
            &result.return_path,
            "success",
            None,
        )),
        Err(error) => Ok(google_drive_error_redirect_response(&state, &error)),
    }
}

fn google_drive_error_redirect_response(
    state: &PrimaryAppState,
    error: &AsterError,
) -> HttpResponse {
    tracing::warn!(error = %error, "Google Drive storage policy callback failed");
    let code = error.api_error_code().as_str();
    google_drive_status_redirect_response(state, "/admin/policies", "error", Some(code))
}

fn google_drive_status_redirect_response(
    state: &PrimaryAppState,
    return_path: &str,
    status: &str,
    code: Option<&str>,
) -> HttpResponse {
    let mut path = append_query_pair(return_path, "google_drive", status);
    if let Some(code) = code {
        path = append_query_pair(&path, "code", code);
    }
    let location = if path.starts_with("http://") || path.starts_with("https://") {
        path
    } else {
        site_url::public_app_url(&state.runtime_config, &path).unwrap_or(path)
    };
    HttpResponse::Found()
        .insert_header((header::LOCATION, location))
        .finish()
}

fn append_query_pair(path: &str, key: &str, value: &str) -> String {
    let separator = if path.contains('?') { '&' } else { '?' };
    format!(
        "{}{}{}={}",
        path,
        separator,
        urlencoding::encode(key),
        urlencoding::encode(value)
    )
}
