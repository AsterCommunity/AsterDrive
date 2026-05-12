//! 管理员 API 路由：`users`。

use crate::api::dto::admin::{
    AdminUserListQuery, CreateUserReq, PatchUserReq, ResetUserPasswordReq,
};
use crate::api::dto::validate_request;
use crate::api::pagination::LimitOffsetQuery;
#[cfg(all(debug_assertions, feature = "openapi"))]
use crate::api::pagination::OffsetPage;
use crate::api::response::ApiResponse;
use crate::errors::Result;
use crate::runtime::PrimaryAppState;
use crate::services::{
    audit_service,
    auth_service::{self, Claims},
    profile_service, user_service,
};
use actix_web::{HttpRequest, HttpResponse, web};

#[api_docs_macros::path(
    post,
    path = "/api/v1/admin/users",
    tag = "admin",
    operation_id = "create_user",
    request_body = CreateUserReq,
    responses(
        (status = 201, description = "User created", body = inline(ApiResponse<crate::services::user_service::UserInfo>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 400, description = "Validation error"),
    ),
    security(("bearer" = [])),
)]
pub async fn create_user(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    body: web::Json<CreateUserReq>,
) -> Result<HttpResponse> {
    validate_request(&*body)?;
    let ctx = audit_service::AuditContext::from_request(&req, &claims);
    let user =
        user_service::create_with_audit(&state, &body.username, &body.email, &body.password, &ctx)
            .await?;
    Ok(HttpResponse::Created().json(ApiResponse::ok(user)))
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/admin/users",
    tag = "admin",
    operation_id = "list_users",
    params(LimitOffsetQuery, AdminUserListQuery),
    responses(
        (status = 200, description = "List users", body = inline(ApiResponse<OffsetPage<crate::services::user_service::UserInfo>>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer" = [])),
)]
pub async fn list_users(
    state: web::Data<PrimaryAppState>,
    page: web::Query<LimitOffsetQuery>,
    query: web::Query<AdminUserListQuery>,
) -> Result<HttpResponse> {
    let users = user_service::list_paginated(
        &state,
        page.limit_or(50, 100),
        page.offset(),
        query.keyword.as_deref(),
        query.role,
        query.status,
        query.sort_by(),
        query.sort_order(),
    )
    .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(users)))
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/admin/users/{id}",
    tag = "admin",
    operation_id = "get_user",
    params(("id" = i64, Path, description = "User ID")),
    responses(
        (status = 200, description = "User details", body = inline(ApiResponse<crate::services::user_service::UserInfo>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "User not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn get_user(
    state: web::Data<PrimaryAppState>,
    path: web::Path<i64>,
) -> Result<HttpResponse> {
    let user = user_service::get(&state, *path).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(user)))
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/admin/users/{id}/sessions/revoke",
    tag = "admin",
    operation_id = "revoke_user_sessions",
    params(("id" = i64, Path, description = "User ID")),
    responses(
        (status = 200, description = "User sessions revoked"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "User not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn revoke_user_sessions(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
) -> Result<HttpResponse> {
    let ctx = audit_service::AuditContext::from_request(&req, &claims);
    auth_service::revoke_user_sessions_with_audit(&state, *path, &ctx).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::<()>::ok_empty()))
}

#[api_docs_macros::path(
    patch,
    path = "/api/v1/admin/users/{id}",
    tag = "admin",
    operation_id = "update_user",
    params(("id" = i64, Path, description = "User ID")),
    request_body = PatchUserReq,
    responses(
        (status = 200, description = "User updated", body = inline(ApiResponse<crate::services::user_service::UserInfo>)),
        (status = 400, description = "Bad request, for example when policy_group_id cannot be null"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "User not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn update_user(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
    body: web::Json<PatchUserReq>,
) -> Result<HttpResponse> {
    let target_id = *path;
    validate_request(&*body)?;
    let body = body.into_inner();
    let ctx = audit_service::AuditContext::from_request(&req, &claims);
    let user = user_service::update_with_audit(
        &state,
        user_service::UpdateUserInput {
            id: target_id,
            email_verified: body.email_verified,
            role: body.role,
            status: body.status,
            storage_quota: body.storage_quota,
            policy_group_id: body.policy_group_id,
        },
        &ctx,
    )
    .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(user)))
}

#[api_docs_macros::path(
    put,
    path = "/api/v1/admin/users/{id}/password",
    tag = "admin",
    operation_id = "reset_user_password",
    params(("id" = i64, Path, description = "User ID")),
    request_body = ResetUserPasswordReq,
    responses(
        (status = 200, description = "User password reset"),
        (status = 400, description = "Validation error"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "User not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn reset_user_password(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
    body: web::Json<ResetUserPasswordReq>,
) -> Result<HttpResponse> {
    validate_request(&*body)?;
    let ctx = audit_service::AuditContext::from_request(&req, &claims);
    auth_service::set_password_with_audit(&state, *path, &body.password, &ctx).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::<()>::ok_empty()))
}

#[api_docs_macros::path(
    delete,
    path = "/api/v1/admin/users/{id}",
    tag = "admin",
    operation_id = "force_delete_user",
    params(("id" = i64, Path, description = "User ID")),
    responses(
        (status = 200, description = "User and all data permanently deleted"),
        (status = 400, description = "Cannot delete admin user"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Admin required"),
        (status = 404, description = "User not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn force_delete_user(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
) -> Result<HttpResponse> {
    let ctx = audit_service::AuditContext::from_request(&req, &claims);
    user_service::force_delete_with_audit(&state, *path, &ctx).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::<()>::ok_empty()))
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/admin/users/{id}/avatar/{size}",
    tag = "admin",
    operation_id = "get_user_avatar",
    params(
        ("id" = i64, Path, description = "User ID"),
        ("size" = u32, Path, description = "Avatar size (512 or 1024)")
    ),
    responses(
        (status = 200, description = "Avatar image (WebP)"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Avatar not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn get_user_avatar(
    state: web::Data<PrimaryAppState>,
    path: web::Path<(i64, u32)>,
) -> Result<HttpResponse> {
    let (user_id, size) = path.into_inner();
    let bytes = profile_service::get_avatar_bytes(&state, user_id, size).await?;
    Ok(profile_service::avatar_image_response(bytes))
}
