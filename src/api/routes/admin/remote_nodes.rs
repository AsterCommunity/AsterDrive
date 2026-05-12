//! 管理员 API 路由：`remote_nodes`。

use crate::api::dto::admin::{
    AdminRemoteNodeListQuery, CreateRemoteNodeReq, PatchRemoteNodeReq, TestRemoteNodeParamsReq,
};
use crate::api::dto::validate_request;
use crate::api::pagination::LimitOffsetQuery;
#[cfg(all(debug_assertions, feature = "openapi"))]
use crate::api::pagination::OffsetPage;
use crate::api::response::ApiResponse;
use crate::errors::Result;
use crate::runtime::PrimaryAppState;
use crate::services::{
    audit_service, auth_service::Claims, managed_follower_enrollment_service,
    managed_follower_service, managed_ingress_profile_service,
};
use crate::storage::remote_protocol::{
    RemoteCreateIngressProfileRequest, RemoteUpdateIngressProfileRequest,
};
use crate::types::DriverType;
use actix_web::{HttpRequest, HttpResponse, web};

fn driver_type_audit_name(driver_type: DriverType) -> &'static str {
    match driver_type {
        DriverType::Local => "local",
        DriverType::S3 => "s3",
        DriverType::Remote => "remote",
    }
}

fn enrollment_status_audit_name(
    status: managed_follower_service::RemoteNodeEnrollmentStatus,
) -> &'static str {
    match status {
        managed_follower_service::RemoteNodeEnrollmentStatus::NotStarted => "not_started",
        managed_follower_service::RemoteNodeEnrollmentStatus::Pending => "pending",
        managed_follower_service::RemoteNodeEnrollmentStatus::Redeemed => "redeemed",
        managed_follower_service::RemoteNodeEnrollmentStatus::Completed => "completed",
        managed_follower_service::RemoteNodeEnrollmentStatus::Expired => "expired",
    }
}

fn remote_node_audit_details(
    node: &managed_follower_service::RemoteNodeInfo,
) -> Option<serde_json::Value> {
    audit_service::details(audit_service::RemoteNodeAuditDetails {
        base_url: &node.base_url,
        is_enabled: node.is_enabled,
        enrollment_status: enrollment_status_audit_name(node.enrollment_status),
    })
}

impl From<CreateRemoteNodeReq> for managed_follower_service::CreateRemoteNodeInput {
    fn from(value: CreateRemoteNodeReq) -> Self {
        Self {
            name: value.name,
            base_url: value.base_url.unwrap_or_default(),
            is_enabled: value.is_enabled,
        }
    }
}

impl From<PatchRemoteNodeReq> for managed_follower_service::UpdateRemoteNodeInput {
    fn from(value: PatchRemoteNodeReq) -> Self {
        Self {
            name: value.name,
            base_url: value.base_url,
            is_enabled: value.is_enabled,
        }
    }
}

impl From<TestRemoteNodeParamsReq> for managed_follower_service::TestRemoteNodeInput {
    fn from(value: TestRemoteNodeParamsReq) -> Self {
        Self {
            base_url: value.base_url,
            access_key: value.access_key,
            secret_key: value.secret_key,
        }
    }
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/admin/remote-nodes",
    tag = "admin",
    operation_id = "list_remote_nodes",
    params(LimitOffsetQuery, AdminRemoteNodeListQuery),
    responses(
        (status = 200, description = "List remote nodes", body = inline(ApiResponse<OffsetPage<managed_follower_service::RemoteNodeInfo>>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer" = [])),
)]
pub async fn list_remote_nodes(
    state: web::Data<PrimaryAppState>,
    page: web::Query<LimitOffsetQuery>,
    query: web::Query<AdminRemoteNodeListQuery>,
) -> Result<HttpResponse> {
    let nodes = managed_follower_service::list_paginated(
        &state,
        page.limit_or(50, 100),
        page.offset(),
        query.sort_by(),
        query.sort_order(),
    )
    .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(nodes)))
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/admin/remote-nodes",
    tag = "admin",
    operation_id = "create_remote_node",
    request_body = CreateRemoteNodeReq,
    responses(
        (status = 201, description = "Remote node created", body = inline(ApiResponse<managed_follower_service::RemoteNodeInfo>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer" = [])),
)]
pub async fn create_remote_node(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    body: web::Json<CreateRemoteNodeReq>,
) -> Result<HttpResponse> {
    validate_request(&*body)?;
    let node = managed_follower_service::create(&state, body.into_inner().into()).await?;
    let ctx = audit_service::AuditContext::from_request(&req, &claims);
    audit_service::log(
        &state,
        &ctx,
        audit_service::AuditAction::AdminCreateRemoteNode,
        Some("remote_node"),
        Some(node.id),
        Some(&node.name),
        remote_node_audit_details(&node),
    )
    .await;
    Ok(HttpResponse::Created().json(ApiResponse::ok(node)))
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/admin/remote-nodes/{id}",
    tag = "admin",
    operation_id = "get_remote_node",
    params(("id" = i64, Path, description = "Remote node ID")),
    responses(
        (status = 200, description = "Remote node details", body = inline(ApiResponse<managed_follower_service::RemoteNodeInfo>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Remote node not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn get_remote_node(
    state: web::Data<PrimaryAppState>,
    path: web::Path<i64>,
) -> Result<HttpResponse> {
    let node = managed_follower_service::get(&state, *path).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(node)))
}

#[api_docs_macros::path(
    patch,
    path = "/api/v1/admin/remote-nodes/{id}",
    tag = "admin",
    operation_id = "update_remote_node",
    params(("id" = i64, Path, description = "Remote node ID")),
    request_body = PatchRemoteNodeReq,
    responses(
        (status = 200, description = "Remote node updated", body = inline(ApiResponse<managed_follower_service::RemoteNodeInfo>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Remote node not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn update_remote_node(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
    body: web::Json<PatchRemoteNodeReq>,
) -> Result<HttpResponse> {
    validate_request(&*body)?;
    let node = managed_follower_service::update(&state, *path, body.into_inner().into()).await?;
    let ctx = audit_service::AuditContext::from_request(&req, &claims);
    audit_service::log(
        &state,
        &ctx,
        audit_service::AuditAction::AdminUpdateRemoteNode,
        Some("remote_node"),
        Some(node.id),
        Some(&node.name),
        remote_node_audit_details(&node),
    )
    .await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(node)))
}

#[api_docs_macros::path(
    delete,
    path = "/api/v1/admin/remote-nodes/{id}",
    tag = "admin",
    operation_id = "delete_remote_node",
    params(("id" = i64, Path, description = "Remote node ID")),
    responses(
        (status = 200, description = "Remote node deleted"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Remote node not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn delete_remote_node(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
) -> Result<HttpResponse> {
    let node = managed_follower_service::get(&state, *path).await?;
    managed_follower_service::delete(&state, *path).await?;
    let ctx = audit_service::AuditContext::from_request(&req, &claims);
    audit_service::log(
        &state,
        &ctx,
        audit_service::AuditAction::AdminDeleteRemoteNode,
        Some("remote_node"),
        Some(node.id),
        Some(&node.name),
        remote_node_audit_details(&node),
    )
    .await;
    Ok(HttpResponse::Ok().json(ApiResponse::<()>::ok_empty()))
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/admin/remote-nodes/{id}/test",
    tag = "admin",
    operation_id = "test_remote_node_connection",
    params(("id" = i64, Path, description = "Remote node ID")),
    responses(
        (status = 200, description = "Remote node connection tested", body = inline(ApiResponse<managed_follower_service::RemoteNodeInfo>)),
        (status = 400, description = "Connection failed"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 412, description = "Remote node is disabled or not ready"),
        (status = 404, description = "Remote node not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn test_remote_node(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
) -> Result<HttpResponse> {
    let node = managed_follower_service::test_connection(&state, *path).await?;
    let ctx = audit_service::AuditContext::from_request(&req, &claims);
    audit_service::log(
        &state,
        &ctx,
        audit_service::AuditAction::AdminTestRemoteNode,
        Some("remote_node"),
        Some(node.id),
        Some(&node.name),
        remote_node_audit_details(&node),
    )
    .await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(node)))
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/admin/remote-nodes/test",
    tag = "admin",
    operation_id = "test_remote_node_params",
    request_body = TestRemoteNodeParamsReq,
    responses(
        (status = 200, description = "Remote node connection successful", body = inline(ApiResponse<crate::storage::remote_protocol::RemoteStorageCapabilities>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 400, description = "Connection failed"),
    ),
    security(("bearer" = [])),
)]
pub async fn test_remote_node_params(
    body: web::Json<TestRemoteNodeParamsReq>,
) -> Result<HttpResponse> {
    validate_request(&*body)?;
    let capabilities =
        managed_follower_service::test_connection_params(body.into_inner().into()).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(capabilities)))
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/admin/remote-nodes/{id}/enrollment-token",
    tag = "admin",
    operation_id = "create_remote_node_enrollment_token",
    params(("id" = i64, Path, description = "Remote node ID")),
    responses(
        (status = 201, description = "Enrollment command created", body = ApiResponse<managed_follower_enrollment_service::RemoteEnrollmentCommandInfo>),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Remote node not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn create_remote_node_enrollment_token(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
) -> Result<HttpResponse> {
    let command =
        managed_follower_enrollment_service::create_enrollment_command(&state, *path).await?;
    let ctx = audit_service::AuditContext::from_request(&req, &claims);
    audit_service::log(
        &state,
        &ctx,
        audit_service::AuditAction::AdminCreateRemoteNodeEnrollmentToken,
        Some("remote_node"),
        Some(command.remote_node_id),
        Some(&command.remote_node_name),
        audit_service::details(serde_json::json!({
            "expires_at": command.expires_at,
        })),
    )
    .await;
    Ok(HttpResponse::Created().json(ApiResponse::ok(command)))
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/admin/remote-nodes/{id}/ingress-profiles",
    tag = "admin",
    operation_id = "list_remote_node_ingress_profiles",
    params(("id" = i64, Path, description = "Remote node ID")),
    responses(
        (status = 200, description = "List remote node ingress profiles", body = inline(ApiResponse<Vec<crate::storage::remote_protocol::RemoteIngressProfileInfo>>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Remote node not found"),
        (status = 412, description = "Managed ingress profiles require a single primary binding"),
    ),
    security(("bearer" = [])),
)]
pub async fn list_remote_node_ingress_profiles(
    state: web::Data<PrimaryAppState>,
    path: web::Path<i64>,
) -> Result<HttpResponse> {
    let profiles = managed_ingress_profile_service::list_remote(&state, *path).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(profiles)))
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/admin/remote-nodes/{id}/ingress-profiles",
    tag = "admin",
    operation_id = "create_remote_node_ingress_profile",
    params(("id" = i64, Path, description = "Remote node ID")),
    request_body = RemoteCreateIngressProfileRequest,
    responses(
        (status = 201, description = "Remote node ingress profile created", body = inline(ApiResponse<crate::storage::remote_protocol::RemoteIngressProfileInfo>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Remote node not found"),
        (status = 412, description = "Managed ingress profiles require a single primary binding"),
    ),
    security(("bearer" = [])),
)]
pub async fn create_remote_node_ingress_profile(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
    body: web::Json<RemoteCreateIngressProfileRequest>,
) -> Result<HttpResponse> {
    let profile =
        managed_ingress_profile_service::create_remote(&state, *path, body.into_inner()).await?;
    let ctx = audit_service::AuditContext::from_request(&req, &claims);
    audit_service::log(
        &state,
        &ctx,
        audit_service::AuditAction::AdminCreateRemoteIngressProfile,
        Some("remote_ingress_profile"),
        Some(*path),
        Some(&profile.profile_key),
        audit_service::details(audit_service::RemoteIngressProfileAuditDetails {
            profile_key: &profile.profile_key,
            driver_type: driver_type_audit_name(profile.driver_type),
            is_default: profile.is_default,
        }),
    )
    .await;
    Ok(HttpResponse::Created().json(ApiResponse::ok(profile)))
}

#[api_docs_macros::path(
    patch,
    path = "/api/v1/admin/remote-nodes/{id}/ingress-profiles/{profile_key}",
    tag = "admin",
    operation_id = "update_remote_node_ingress_profile",
    params(
        ("id" = i64, Path, description = "Remote node ID"),
        ("profile_key" = String, Path, description = "Remote ingress profile key")
    ),
    request_body = RemoteUpdateIngressProfileRequest,
    responses(
        (status = 200, description = "Remote node ingress profile updated", body = inline(ApiResponse<crate::storage::remote_protocol::RemoteIngressProfileInfo>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Remote node or ingress profile not found"),
        (status = 412, description = "Managed ingress profiles require a single primary binding"),
    ),
    security(("bearer" = [])),
)]
pub async fn update_remote_node_ingress_profile(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<(i64, String)>,
    body: web::Json<RemoteUpdateIngressProfileRequest>,
) -> Result<HttpResponse> {
    let (id, profile_key) = path.into_inner();
    let profile =
        managed_ingress_profile_service::update_remote(&state, id, &profile_key, body.into_inner())
            .await?;
    let ctx = audit_service::AuditContext::from_request(&req, &claims);
    audit_service::log(
        &state,
        &ctx,
        audit_service::AuditAction::AdminUpdateRemoteIngressProfile,
        Some("remote_ingress_profile"),
        Some(id),
        Some(&profile.profile_key),
        audit_service::details(audit_service::RemoteIngressProfileAuditDetails {
            profile_key: &profile.profile_key,
            driver_type: driver_type_audit_name(profile.driver_type),
            is_default: profile.is_default,
        }),
    )
    .await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(profile)))
}

#[api_docs_macros::path(
    delete,
    path = "/api/v1/admin/remote-nodes/{id}/ingress-profiles/{profile_key}",
    tag = "admin",
    operation_id = "delete_remote_node_ingress_profile",
    params(
        ("id" = i64, Path, description = "Remote node ID"),
        ("profile_key" = String, Path, description = "Remote ingress profile key")
    ),
    responses(
        (status = 200, description = "Remote node ingress profile deleted"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Remote node or ingress profile not found"),
        (status = 412, description = "Managed ingress profiles require a single primary binding"),
    ),
    security(("bearer" = [])),
)]
pub async fn delete_remote_node_ingress_profile(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<(i64, String)>,
) -> Result<HttpResponse> {
    let (id, profile_key) = path.into_inner();
    managed_ingress_profile_service::delete_remote(&state, id, &profile_key).await?;
    let ctx = audit_service::AuditContext::from_request(&req, &claims);
    audit_service::log(
        &state,
        &ctx,
        audit_service::AuditAction::AdminDeleteRemoteIngressProfile,
        Some("remote_ingress_profile"),
        Some(id),
        Some(&profile_key),
        audit_service::details(serde_json::json!({
            "profile_key": &profile_key,
        })),
    )
    .await;
    Ok(HttpResponse::Ok().json(ApiResponse::<()>::ok_empty()))
}
