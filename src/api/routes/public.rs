//! API 路由：`public`。

use crate::api::dto::validate_request;
use crate::api::request_auth::{access_cookie_token, bearer_token};
use crate::api::response::ApiResponse;
use crate::errors::Result;
use crate::runtime::PrimaryAppState;
use crate::services::{
    audit_service, auth_service, config_service, managed_follower_enrollment_service,
};
use actix_web::{HttpRequest, HttpResponse, http::header, web};
use serde::Deserialize;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;
use validator::Validate;

#[derive(Debug, Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct RedeemRemoteEnrollmentReq {
    #[validate(custom(function = "crate::api::dto::validation::validate_non_blank"))]
    pub token: String,
}

#[derive(Debug, Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct AckRemoteEnrollmentReq {
    #[validate(custom(function = "crate::api::dto::validation::validate_non_blank"))]
    pub ack_token: String,
}

pub fn routes() -> impl actix_web::dev::HttpServiceFactory + use<> {
    web::scope("/public")
        .route("/branding", web::get().to(get_branding))
        .route("/frontend-config", web::get().to(get_frontend_config))
        .route("/preview-apps", web::get().to(get_preview_apps))
        .route("/custom-config", web::get().to(get_custom_config))
        .route("/thumbnail-support", web::get().to(get_thumbnail_support))
        .route("/media-data-support", web::get().to(get_media_data_support))
        .route(
            "/remote-enrollment/redeem",
            web::post().to(redeem_remote_enrollment),
        )
        .route(
            "/remote-enrollment/ack",
            web::post().to(ack_remote_enrollment),
        )
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/public/branding",
    tag = "public",
    operation_id = "get_public_branding",
    responses(
        (status = 200, description = "Public branding config", body = inline(ApiResponse<config_service::PublicBranding>)),
    ),
)]
// TODO(0.3.0): remove this legacy endpoint after clients migrate to /public/frontend-config.
pub async fn get_branding(state: web::Data<PrimaryAppState>) -> Result<HttpResponse> {
    let branding = config_service::get_public_branding(&state);
    Ok(public_config_response(branding))
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/public/frontend-config",
    tag = "public",
    operation_id = "get_public_frontend_config",
    responses(
        (status = 200, description = "Public frontend bootstrap config", body = inline(ApiResponse<config_service::PublicFrontendConfig>)),
    ),
)]
pub async fn get_frontend_config(state: web::Data<PrimaryAppState>) -> Result<HttpResponse> {
    let config = config_service::get_public_frontend_config(&state);
    Ok(public_config_response(config))
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/public/preview-apps",
    tag = "public",
    operation_id = "get_public_preview_apps",
    responses(
        (status = 200, description = "Public preview app config", body = inline(ApiResponse<crate::services::preview_app_service::PublicPreviewAppsConfig>)),
    ),
)]
pub async fn get_preview_apps(state: web::Data<PrimaryAppState>) -> Result<HttpResponse> {
    let preview_apps = config_service::get_public_preview_apps(&state);
    Ok(public_config_response(preview_apps))
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/public/custom-config",
    tag = "public",
    operation_id = "get_public_custom_config",
    responses(
        (status = 200, description = "Custom config visible to the current request identity", body = inline(ApiResponse<config_service::PublicCustomConfig>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
    ),
)]
pub async fn get_custom_config(
    state: web::Data<PrimaryAppState>,
    req: HttpRequest,
) -> Result<HttpResponse> {
    let include_authenticated = request_has_valid_access_token(&state, &req).await?;
    let custom_config =
        config_service::get_public_custom_config(&state, include_authenticated).await?;
    if include_authenticated {
        return Ok(HttpResponse::Ok()
            .insert_header((header::CACHE_CONTROL, "private, max-age=60"))
            .insert_header((header::VARY, "Authorization, Cookie"))
            .json(ApiResponse::ok(custom_config)));
    }
    Ok(public_config_response(custom_config))
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/public/thumbnail-support",
    tag = "public",
    operation_id = "get_public_thumbnail_support",
    responses(
        (status = 200, description = "Public thumbnail support config", body = inline(ApiResponse<crate::config::media_processing::PublicThumbnailSupport>)),
    ),
)]
pub async fn get_thumbnail_support(state: web::Data<PrimaryAppState>) -> Result<HttpResponse> {
    let support = config_service::get_public_thumbnail_support(&state).await;
    Ok(public_config_response(support))
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/public/media-data-support",
    tag = "public",
    operation_id = "get_public_media_data_support",
    responses(
        (status = 200, description = "Public media metadata support config", body = inline(ApiResponse<crate::config::media_processing::PublicMediaDataSupport>)),
    ),
)]
pub async fn get_media_data_support(state: web::Data<PrimaryAppState>) -> Result<HttpResponse> {
    let support = config_service::get_public_media_data_support(&state).await;
    Ok(public_config_response(support))
}

fn public_config_response<T: serde::Serialize>(data: T) -> HttpResponse {
    HttpResponse::Ok()
        .insert_header((
            header::CACHE_CONTROL,
            config_service::PUBLIC_CONFIG_CACHE_CONTROL,
        ))
        .insert_header((header::VARY, "Authorization, Cookie"))
        .json(ApiResponse::ok(data))
}

async fn request_has_valid_access_token(
    state: &PrimaryAppState,
    req: &HttpRequest,
) -> Result<bool> {
    let token = access_cookie_token(req).or_else(|| bearer_token(req));
    let Some(token) = token else {
        return Ok(false);
    };

    auth_service::authenticate_access_token(state, &token).await?;
    Ok(true)
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/public/remote-enrollment/redeem",
    tag = "public",
    operation_id = "redeem_remote_enrollment",
    request_body = RedeemRemoteEnrollmentReq,
    responses(
        (status = 200, description = "Redeem a remote enrollment token", body = ApiResponse<managed_follower_enrollment_service::RemoteEnrollmentBootstrap>),
    ),
)]
pub async fn redeem_remote_enrollment(
    state: web::Data<PrimaryAppState>,
    req: HttpRequest,
    body: web::Json<RedeemRemoteEnrollmentReq>,
) -> Result<HttpResponse> {
    validate_request(&*body)?;
    let bootstrap =
        managed_follower_enrollment_service::redeem_enrollment_token(&state, &body.token).await?;
    let audit_info = audit_service::AuditRequestInfo::from_request(&req);
    let ctx = audit_info.to_context(0);
    audit_service::log(
        &state,
        &ctx,
        audit_service::AuditAction::RemoteEnrollmentRedeem,
        crate::services::audit_service::AuditEntityType::RemoteNode,
        Some(bootstrap.remote_node_id),
        Some(&bootstrap.remote_node_name),
        None,
    )
    .await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(bootstrap)))
}

#[api_docs_macros::path(
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
    managed_follower_enrollment_service::ack_enrollment_token(&state, &body.ack_token).await?;
    let audit_info = audit_service::AuditRequestInfo::from_request(&req);
    let ctx = audit_info.to_context(0);
    audit_service::log(
        &state,
        &ctx,
        audit_service::AuditAction::RemoteEnrollmentAck,
        crate::services::audit_service::AuditEntityType::RemoteNode,
        None,
        None,
        None,
    )
    .await;
    Ok(HttpResponse::Ok().json(ApiResponse::<()>::ok_empty()))
}
