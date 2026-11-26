//! API 路由：`public`。

use crate::api::dto::validate_request;
use crate::api::middleware::rate_limit;
use crate::api::request_auth::{access_cookie_token, bearer_token};
#[cfg(all(debug_assertions, feature = "openapi"))]
use crate::api::response::ApiResponse;
use crate::config::{NetworkTrustConfig, RateLimitConfig};
use crate::errors::Result;
use crate::runtime::PrimaryAppState;
use crate::services::{auth::local, ops::audit, ops::config, remote::enrollment};
use actix_web::middleware::Condition;
use actix_web::{HttpRequest, HttpResponse, http::header, web};
use aster_forge_middleware::actix::rate_limit::RateLimit;
use serde::Deserialize;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;
use validator::Validate;

/// Request body for redeeming a remote node enrollment token.
#[derive(Debug, Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct RedeemRemoteEnrollmentReq {
    /// Single-use enrollment token issued by an administrator.
    #[validate(custom(function = "crate::api::dto::validation::validate_non_blank"))]
    pub token: String,
}

/// Request body for acknowledging a redeemed remote enrollment session.
#[derive(Debug, Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct AckRemoteEnrollmentReq {
    /// One-time acknowledgment token returned by the redeem call.
    #[validate(custom(function = "crate::api::dto::validation::validate_non_blank"))]
    pub ack_token: String,
}

/// Builds the unauthenticated `/api/v1/public` route scope; the enrollment exchange
/// routes get an extra public-tier rate limit.
pub fn routes(
    rl: &RateLimitConfig,
    network_trust: &NetworkTrustConfig,
) -> impl actix_web::dev::HttpServiceFactory + use<> {
    // Enrollment exchange returns long-lived node credentials, so unlike the
    // read-only config endpoints it shares the public rate-limit tier even
    // though the enrollment token itself is single-use and high-entropy.
    let enrollment_limiter =
        rate_limit::build_rate_limit(&rl.public, &network_trust.trusted_proxies);

    web::scope("/public")
        .route("/frontend-config", web::get().to(get_frontend_config))
        .route("/preview-apps", web::get().to(get_preview_apps))
        .route("/custom-config", web::get().to(get_custom_config))
        .route("/thumbnail-support", web::get().to(get_thumbnail_support))
        .route("/media-data-support", web::get().to(get_media_data_support))
        .service(
            web::scope("")
                .wrap(Condition::new(
                    rl.enabled,
                    RateLimit::new(&enrollment_limiter),
                ))
                .route(
                    "/remote-enrollment/redeem",
                    web::post().to(redeem_remote_enrollment),
                )
                .route(
                    "/remote-enrollment/ack",
                    web::post().to(ack_remote_enrollment),
                ),
        )
}

/// Returns the public frontend bootstrap config (branding and feature switches).
#[aster_forge_api_docs_macros::path(
    get,
    path = "/api/v1/public/frontend-config",
    tag = "public",
    operation_id = "get_public_frontend_config",
    responses(
        (status = 200, description = "Public frontend bootstrap config", body = inline(ApiResponse<config::PublicFrontendConfig>)),
    ),
)]
pub async fn get_frontend_config(state: web::Data<PrimaryAppState>) -> Result<HttpResponse> {
    let config = config::get_public_frontend_config(state.get_ref());
    Ok(public_config_response(config))
}

/// Returns the public preview app registry (WOPI apps and URL-template viewers).
#[aster_forge_api_docs_macros::path(
    get,
    path = "/api/v1/public/preview-apps",
    tag = "public",
    operation_id = "get_public_preview_apps",
    responses(
        (status = 200, description = "Public preview app config", body = inline(ApiResponse<crate::services::preview::apps::PublicPreviewAppsConfig>)),
    ),
)]
pub async fn get_preview_apps(state: web::Data<PrimaryAppState>) -> Result<HttpResponse> {
    let preview_apps = config::get_public_preview_apps(state.get_ref());
    Ok(public_config_response(preview_apps))
}

/// Returns the custom config entries visible to the request identity; authenticated
/// requests without a pending password change also receive authenticated-only entries.
#[aster_forge_api_docs_macros::path(
    get,
    path = "/api/v1/public/custom-config",
    tag = "public",
    operation_id = "get_public_custom_config",
    responses(
        (status = 200, description = "Custom config visible to the current request identity", body = inline(ApiResponse<config::PublicCustomConfig>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
    ),
)]
pub async fn get_custom_config(
    state: web::Data<PrimaryAppState>,
    req: HttpRequest,
) -> Result<HttpResponse> {
    let include_authenticated =
        request_has_unrestricted_access_token(state.get_ref(), &req).await?;
    let custom_config =
        config::get_public_custom_config(state.get_ref(), include_authenticated).await?;
    if include_authenticated {
        return Ok(HttpResponse::Ok()
            .insert_header((header::CACHE_CONTROL, "private, max-age=60"))
            .insert_header((header::VARY, "Authorization, Cookie"))
            .json(crate::api::response::success(custom_config)));
    }
    Ok(public_config_response(custom_config))
}

/// Returns which file types the deployment can render thumbnails for.
#[aster_forge_api_docs_macros::path(
    get,
    path = "/api/v1/public/thumbnail-support",
    tag = "public",
    operation_id = "get_public_thumbnail_support",
    responses(
        (status = 200, description = "Public thumbnail support config", body = inline(ApiResponse<crate::config::media_processing::PublicThumbnailSupport>)),
    ),
)]
pub async fn get_thumbnail_support(state: web::Data<PrimaryAppState>) -> Result<HttpResponse> {
    let support = config::get_public_thumbnail_support(state.get_ref()).await?;
    Ok(public_config_response(support))
}

/// Returns which file types support media metadata extraction.
#[aster_forge_api_docs_macros::path(
    get,
    path = "/api/v1/public/media-data-support",
    tag = "public",
    operation_id = "get_public_media_data_support",
    responses(
        (status = 200, description = "Public media metadata support config", body = inline(ApiResponse<crate::config::media_processing::PublicMediaDataSupport>)),
    ),
)]
pub async fn get_media_data_support(state: web::Data<PrimaryAppState>) -> Result<HttpResponse> {
    let support = config::get_public_media_data_support(state.get_ref()).await?;
    Ok(public_config_response(support))
}

fn public_config_response<T: serde::Serialize>(data: T) -> HttpResponse {
    HttpResponse::Ok()
        .insert_header((header::CACHE_CONTROL, config::PUBLIC_CONFIG_CACHE_CONTROL))
        .insert_header((header::VARY, "Authorization, Cookie"))
        .json(crate::api::response::success(data))
}

async fn request_has_unrestricted_access_token(
    state: &PrimaryAppState,
    req: &HttpRequest,
) -> Result<bool> {
    let token = access_cookie_token(req).or_else(|| bearer_token(req));
    let Some(token) = token else {
        return Ok(false);
    };

    let (claims, _) = local::authenticate_access_token(state, &token).await?;
    Ok(!claims.password_change)
}

/// Exchanges a remote node enrollment token for node bootstrap credentials, and
/// records the redeem phase in the audit log.
#[aster_forge_api_docs_macros::path(
    post,
    path = "/api/v1/public/remote-enrollment/redeem",
    tag = "public",
    operation_id = "redeem_remote_enrollment",
    request_body = RedeemRemoteEnrollmentReq,
    responses(
        (status = 200, description = "Redeem a remote enrollment token", body = ApiResponse<enrollment::RemoteEnrollmentBootstrap>),
    ),
)]
pub async fn redeem_remote_enrollment(
    state: web::Data<PrimaryAppState>,
    req: HttpRequest,
    body: web::Json<RedeemRemoteEnrollmentReq>,
) -> Result<HttpResponse> {
    validate_request(&*body)?;
    let bootstrap = enrollment::redeem_enrollment_token(state.get_ref(), &body.token).await?;
    let audit_info = audit::AuditRequestInfo::from_request(&req);
    let ctx = audit_info.to_context(0);
    audit::log(
        state.get_ref(),
        &ctx,
        audit::AuditAction::RemoteEnrollmentRedeem,
        crate::services::ops::audit::AuditEntityType::RemoteNode,
        Some(bootstrap.remote_node_id),
        Some(&bootstrap.remote_node_name),
        audit::details(audit::RemoteEnrollmentAuditDetails {
            phase: "redeemed",
            remote_node_id: bootstrap.remote_node_id,
            remote_node_name: &bootstrap.remote_node_name,
            is_enabled: bootstrap.is_enabled,
        }),
    )
    .await;
    Ok(HttpResponse::Ok().json(crate::api::response::success(bootstrap)))
}

/// Confirms that a redeemed remote node enrollment session completed successfully,
/// finalizing the enrollment on the master side.
#[aster_forge_api_docs_macros::path(
    post,
    path = "/api/v1/public/remote-enrollment/ack",
    tag = "public",
    operation_id = "ack_remote_enrollment",
    request_body = AckRemoteEnrollmentReq,
    responses(
        (status = 200, description = "Acknowledge a redeemed remote enrollment session"),
    ),
)]
pub async fn ack_remote_enrollment(
    state: web::Data<PrimaryAppState>,
    req: HttpRequest,
    body: web::Json<AckRemoteEnrollmentReq>,
) -> Result<HttpResponse> {
    validate_request(&*body)?;
    let ack = enrollment::ack_enrollment_token(state.get_ref(), &body.ack_token).await?;
    let audit_info = audit::AuditRequestInfo::from_request(&req);
    let ctx = audit_info.to_context(0);
    audit::log(
        state.get_ref(),
        &ctx,
        audit::AuditAction::RemoteEnrollmentAck,
        crate::services::ops::audit::AuditEntityType::RemoteNode,
        Some(ack.remote_node_id),
        Some(&ack.remote_node_name),
        audit::details(audit::RemoteEnrollmentAuditDetails {
            phase: "acked",
            remote_node_id: ack.remote_node_id,
            remote_node_name: &ack.remote_node_name,
            is_enabled: ack.is_enabled,
        }),
    )
    .await;
    Ok(HttpResponse::Ok().json(crate::api::response::empty_success()))
}
