//! 认证 API 路由：`profile`。

use super::{
    ActionMessageResp, RequestEmailChangeReq, UpdateAvatarSourceReq, UpdatePreferencesReq,
    UpdateProfileReq, apply_auth_mail_response_floor,
};
use crate::api::response::ApiResponse;
use crate::errors::Result;
use crate::runtime::PrimaryAppState;
use crate::services::auth::local::Claims;
use crate::services::ops::audit::{self, AuditContext};
use crate::services::{auth::local, user::account, user::profile};
use actix_web::{HttpRequest, HttpResponse, web};

#[aster_forge_api_docs_macros::path(
    post,
    path = "/api/v1/auth/email/change",
    tag = "auth",
    operation_id = "request_email_change",
    request_body = RequestEmailChangeReq,
    responses(
        (status = 200, description = "Email change requested", body = inline(ApiResponse<crate::api::routes::auth::UserInfo>)),
        (status = 400, description = "Validation error"),
        (status = 403, description = "Account pending activation"),
    ),
    security(("bearer" = [])),
)]
pub async fn request_email_change(
    state: web::Data<PrimaryAppState>,
    req: HttpRequest,
    claims: web::ReqData<Claims>,
    body: web::Json<RequestEmailChangeReq>,
) -> Result<HttpResponse> {
    let ctx = AuditContext::from_request(&req, &claims);
    let user = local::request_email_change_with_audit(
        state.get_ref(),
        claims.user_id,
        &body.new_email,
        &ctx,
    )
    .await?;
    let user_info = account::get_self_info(state.get_ref(), user.id).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(user_info)))
}

#[aster_forge_api_docs_macros::path(
    post,
    path = "/api/v1/auth/email/change/resend",
    tag = "auth",
    operation_id = "resend_email_change",
    responses(
        (status = 200, description = "Email change confirmation resend request accepted", body = inline(ApiResponse<ActionMessageResp>)),
        (status = 400, description = "No pending email change"),
    ),
    security(("bearer" = [])),
)]
pub async fn resend_email_change(
    state: web::Data<PrimaryAppState>,
    req: HttpRequest,
    claims: web::ReqData<Claims>,
) -> Result<HttpResponse> {
    let started_at = tokio::time::Instant::now();
    let ctx = AuditContext::from_request(&req, &claims);
    let result = local::resend_email_change_with_audit(state.get_ref(), claims.user_id, &ctx).await;
    match result {
        Ok(_) => {}
        Err(error) => {
            apply_auth_mail_response_floor(started_at).await;
            return Err(error);
        }
    }
    apply_auth_mail_response_floor(started_at).await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(ActionMessageResp {
        message: "If an email change is pending, a confirmation email will be sent".to_string(),
    })))
}

/// Update the current user's preferences.
///
/// Only non-null fields in the request body are merged into the existing
/// preferences. Returns the full updated preferences object.
#[aster_forge_api_docs_macros::path(
    patch,
    path = "/api/v1/auth/preferences",
    tag = "auth",
    operation_id = "update_preferences",
    request_body = UpdatePreferencesReq,
    responses(
        (status = 200, description = "Preferences updated", body = inline(ApiResponse<crate::api::routes::auth::UserPreferences>)),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer" = [])),
)]
pub async fn patch_preferences(
    state: web::Data<PrimaryAppState>,
    req: HttpRequest,
    claims: web::ReqData<Claims>,
    body: web::Json<UpdatePreferencesReq>,
) -> Result<HttpResponse> {
    let body = body.into_inner();
    let mut changed_fields = Vec::new();
    if body.theme_mode.is_some() {
        changed_fields.push("theme_mode");
    }
    if body.color_preset.is_some() {
        changed_fields.push("color_preset");
    }
    if body.view_mode.is_some() {
        changed_fields.push("view_mode");
    }
    if body.browser_open_mode.is_some() {
        changed_fields.push("browser_open_mode");
    }
    if body.sort_by.is_some() {
        changed_fields.push("sort_by");
    }
    if body.sort_order.is_some() {
        changed_fields.push("sort_order");
    }
    if body.language.is_some() {
        changed_fields.push("language");
    }
    if body.display_time_zone.is_some() {
        changed_fields.push("display_time_zone");
    }
    if body.storage_event_stream_enabled.is_some() {
        changed_fields.push("storage_event_stream_enabled");
    }
    let custom_upsert_count = body.custom.len();
    let custom_remove_count = body.remove_custom_keys.len();
    let prefs = account::update_preferences(state.get_ref(), claims.user_id, body).await?;
    let ctx = AuditContext::from_request(&req, &claims);
    audit::log_with_details(
        state.get_ref(),
        &ctx,
        audit::AuditAction::UserUpdatePreferences,
        crate::services::ops::audit::AuditEntityType::User,
        Some(claims.user_id),
        None,
        || {
            audit::details(audit::UserPreferencesAuditDetails {
                changed_fields,
                custom_upsert_count,
                custom_remove_count,
            })
        },
    )
    .await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(prefs)))
}

#[aster_forge_api_docs_macros::path(
    patch,
    path = "/api/v1/auth/profile",
    tag = "auth",
    operation_id = "update_profile",
    request_body = UpdateProfileReq,
    responses(
        (status = 200, description = "Profile updated", body = inline(ApiResponse<crate::api::routes::auth::UserProfileInfo>)),
        (status = 400, description = "Invalid profile input"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer" = [])),
)]
pub async fn patch_profile(
    state: web::Data<PrimaryAppState>,
    req: HttpRequest,
    claims: web::ReqData<Claims>,
    body: web::Json<UpdateProfileReq>,
) -> Result<HttpResponse> {
    let profile =
        profile::update_profile(state.get_ref(), claims.user_id, body.display_name.clone()).await?;
    let ctx = AuditContext::from_request(&req, &claims);
    audit::log_with_details(
        state.get_ref(),
        &ctx,
        audit::AuditAction::UserUpdateProfile,
        crate::services::ops::audit::AuditEntityType::User,
        Some(claims.user_id),
        None,
        || {
            audit::details(audit::UserProfileAuditDetails {
                display_name: profile.display_name.as_deref(),
            })
        },
    )
    .await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(profile)))
}

#[aster_forge_api_docs_macros::path(
    post,
    path = "/api/v1/auth/profile/avatar/upload",
    tag = "auth",
    operation_id = "upload_avatar",
    request_body(content = String, content_type = "multipart/form-data", description = "Avatar image to upload"),
    responses(
        (status = 200, description = "Avatar uploaded", body = inline(ApiResponse<crate::api::routes::auth::UserProfileInfo>)),
        (status = 400, description = "Invalid image upload"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer" = [])),
)]
pub async fn upload_avatar(
    state: web::Data<PrimaryAppState>,
    req: HttpRequest,
    claims: web::ReqData<Claims>,
    mut payload: actix_multipart::Multipart,
) -> Result<HttpResponse> {
    let profile = profile::upload_avatar(state.get_ref(), claims.user_id, &mut payload).await?;
    let ctx = AuditContext::from_request(&req, &claims);
    audit::log_with_details(
        state.get_ref(),
        &ctx,
        audit::AuditAction::UserUploadAvatar,
        crate::services::ops::audit::AuditEntityType::User,
        Some(claims.user_id),
        None,
        || {
            audit::details(audit::UserAvatarUploadAuditDetails {
                source: profile.avatar.source,
                version: profile.avatar.version,
            })
        },
    )
    .await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(profile)))
}

#[aster_forge_api_docs_macros::path(
    put,
    path = "/api/v1/auth/profile/avatar/source",
    tag = "auth",
    operation_id = "set_avatar_source",
    request_body = UpdateAvatarSourceReq,
    responses(
        (status = 200, description = "Avatar source updated", body = inline(ApiResponse<crate::api::routes::auth::UserProfileInfo>)),
        (status = 400, description = "Invalid avatar source"),
        (status = 401, description = "Not authenticated"),
    ),
    security(("bearer" = [])),
)]
pub async fn put_avatar_source(
    state: web::Data<PrimaryAppState>,
    req: HttpRequest,
    claims: web::ReqData<Claims>,
    body: web::Json<UpdateAvatarSourceReq>,
) -> Result<HttpResponse> {
    let profile = profile::set_avatar_source(state.get_ref(), claims.user_id, body.source).await?;
    let ctx = AuditContext::from_request(&req, &claims);
    audit::log_with_details(
        state.get_ref(),
        &ctx,
        audit::AuditAction::UserSetAvatarSource,
        crate::services::ops::audit::AuditEntityType::User,
        Some(claims.user_id),
        None,
        || {
            audit::details(audit::UserAvatarSourceAuditDetails {
                source: match body.source {
                    crate::types::AvatarSource::None => "none",
                    crate::types::AvatarSource::Gravatar => "gravatar",
                    crate::types::AvatarSource::Upload => "upload",
                },
            })
        },
    )
    .await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(profile)))
}

#[aster_forge_api_docs_macros::path(
    get,
    path = "/api/v1/auth/profile/avatar/{size}",
    tag = "auth",
    operation_id = "get_self_avatar",
    params(("size" = u32, Path, description = "Avatar size (512 or 1024)")),
    responses(
        (status = 200, description = "Avatar image (WebP)"),
        (status = 401, description = "Not authenticated"),
        (status = 404, description = "Avatar not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn get_self_avatar(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    path: web::Path<u32>,
) -> Result<HttpResponse> {
    let bytes = profile::get_avatar_bytes(state.get_ref(), claims.user_id, *path).await?;
    Ok(profile::avatar_image_response(bytes))
}
