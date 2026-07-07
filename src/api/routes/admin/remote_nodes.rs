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
    auth::local::Claims, ops::audit, remote::enrollment, remote::remote_node,
    remote::storage_target,
};
use crate::storage::remote_protocol::{
    RemoteCreateStorageTargetRequest, RemoteUpdateStorageTargetRequest,
};
use actix_web::{HttpRequest, HttpResponse, web};

fn enrollment_status_audit_name(status: remote_node::RemoteNodeEnrollmentStatus) -> &'static str {
    match status {
        remote_node::RemoteNodeEnrollmentStatus::NotStarted => "not_started",
        remote_node::RemoteNodeEnrollmentStatus::Pending => "pending",
        remote_node::RemoteNodeEnrollmentStatus::Redeemed => "redeemed",
        remote_node::RemoteNodeEnrollmentStatus::Completed => "completed",
        remote_node::RemoteNodeEnrollmentStatus::Expired => "expired",
    }
}

fn remote_node_audit_details(node: &remote_node::RemoteNodeInfo) -> Option<serde_json::Value> {
    audit::details(audit::RemoteNodeAuditDetails {
        base_url: &node.base_url,
        is_enabled: node.is_enabled,
        enrollment_status: enrollment_status_audit_name(node.enrollment_status),
    })
}

fn remote_storage_target_audit_details(
    target: &crate::storage::remote_protocol::RemoteStorageTargetInfo,
) -> Option<serde_json::Value> {
    // TODO(remote-storage-target): audit action/detail names keep the old
    // remote ingress profile strings for stored audit compatibility.
    audit::details(audit::RemoteIngressProfileAuditDetails {
        target_key: &target.target_key,
        driver_type: target.driver_type.as_str(),
        is_default: target.is_default,
    })
}

impl From<CreateRemoteNodeReq> for remote_node::CreateRemoteNodeInput {
    fn from(value: CreateRemoteNodeReq) -> Self {
        Self {
            name: value.name,
            base_url: value.base_url.unwrap_or_default(),
            transport_mode: value.transport_mode,
            is_enabled: value.is_enabled,
        }
    }
}

impl From<PatchRemoteNodeReq> for remote_node::UpdateRemoteNodeInput {
    fn from(value: PatchRemoteNodeReq) -> Self {
        Self {
            name: value.name,
            base_url: value.base_url,
            transport_mode: value.transport_mode,
            is_enabled: value.is_enabled,
        }
    }
}

impl From<TestRemoteNodeParamsReq> for remote_node::TestRemoteNodeInput {
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
        (status = 200, description = "List remote nodes", body = inline(ApiResponse<OffsetPage<remote_node::RemoteNodeInfo>>)),
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
    let nodes = remote_node::list_paginated(
        state.get_ref(),
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
        (status = 201, description = "Remote node created", body = inline(ApiResponse<remote_node::RemoteNodeInfo>)),
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
    let node = remote_node::create(state.get_ref(), body.into_inner().into()).await?;
    let ctx = audit::AuditContext::from_request(&req, &claims);
    audit::log_with_details(
        state.get_ref(),
        &ctx,
        audit::AuditAction::AdminCreateRemoteNode,
        crate::services::ops::audit::AuditEntityType::RemoteNode,
        Some(node.id),
        Some(&node.name),
        || remote_node_audit_details(&node),
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
        (status = 200, description = "Remote node details", body = inline(ApiResponse<remote_node::RemoteNodeInfo>)),
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
    let node = remote_node::get(state.get_ref(), *path).await?;
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
        (status = 200, description = "Remote node updated", body = inline(ApiResponse<remote_node::RemoteNodeInfo>)),
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
    let node = remote_node::update(state.get_ref(), *path, body.into_inner().into()).await?;
    let ctx = audit::AuditContext::from_request(&req, &claims);
    audit::log_with_details(
        state.get_ref(),
        &ctx,
        audit::AuditAction::AdminUpdateRemoteNode,
        crate::services::ops::audit::AuditEntityType::RemoteNode,
        Some(node.id),
        Some(&node.name),
        || remote_node_audit_details(&node),
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
    let node = remote_node::get(state.get_ref(), *path).await?;
    remote_node::delete(state.get_ref(), *path).await?;
    let ctx = audit::AuditContext::from_request(&req, &claims);
    audit::log_with_details(
        state.get_ref(),
        &ctx,
        audit::AuditAction::AdminDeleteRemoteNode,
        crate::services::ops::audit::AuditEntityType::RemoteNode,
        Some(node.id),
        Some(&node.name),
        || remote_node_audit_details(&node),
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
        (status = 200, description = "Remote node connection tested", body = inline(ApiResponse<remote_node::RemoteNodeInfo>)),
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
    let node = remote_node::test_connection(state.get_ref(), *path).await?;
    let ctx = audit::AuditContext::from_request(&req, &claims);
    audit::log_with_details(
        state.get_ref(),
        &ctx,
        audit::AuditAction::AdminTestRemoteNode,
        crate::services::ops::audit::AuditEntityType::RemoteNode,
        Some(node.id),
        Some(&node.name),
        || remote_node_audit_details(&node),
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
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    body: web::Json<TestRemoteNodeParamsReq>,
) -> Result<HttpResponse> {
    validate_request(&*body)?;
    let body = body.into_inner();
    let base_url = body.base_url.clone();
    let capabilities = remote_node::test_connection_params(body.into()).await?;
    let ctx = audit::AuditContext::from_request(&req, &claims);
    audit::log_with_details(
        state.get_ref(),
        &ctx,
        audit::AuditAction::AdminTestRemoteNode,
        crate::services::ops::audit::AuditEntityType::RemoteNode,
        None,
        Some(&base_url),
        || {
            audit::details(audit::RemoteNodeParamTestAuditDetails {
                base_url: &base_url,
                success: true,
                protocol_version: &capabilities.protocol_version,
                server_version: capabilities.server_version.as_deref(),
                supports_list: capabilities.supports_list,
                supports_range_read: capabilities.supports_range_read,
                supports_stream_upload: capabilities.supports_stream_upload,
                supports_capacity: capabilities.supports_capacity,
            })
        },
    )
    .await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(capabilities)))
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/admin/remote-nodes/{id}/enrollment-token",
    tag = "admin",
    operation_id = "create_remote_node_enrollment_token",
    params(("id" = i64, Path, description = "Remote node ID")),
    responses(
        (status = 201, description = "Enrollment command created", body = ApiResponse<enrollment::RemoteEnrollmentCommandInfo>),
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
    let command = enrollment::create_enrollment_command(state.get_ref(), *path).await?;
    let ctx = audit::AuditContext::from_request(&req, &claims);
    audit::log_with_details(
        state.get_ref(),
        &ctx,
        audit::AuditAction::AdminCreateRemoteNodeEnrollmentToken,
        crate::services::ops::audit::AuditEntityType::RemoteNode,
        Some(command.remote_node_id),
        Some(&command.remote_node_name),
        || {
            audit::details(audit::RemoteNodeEnrollmentTokenAuditDetails {
                expires_at: command.expires_at,
            })
        },
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
        (status = 200, description = "Deprecated since 0.4.0; use /storage-targets. List remote node remote storage targets", body = inline(ApiResponse<Vec<crate::storage::remote_protocol::RemoteStorageTargetInfo>>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Remote node not found"),
        (status = 412, description = "Remote storage targets require a single primary binding"),
    ),
    security(("bearer" = [])),
)]
#[deprecated(since = "0.4.0", note = "use list_remote_node_storage_targets instead")]
pub async fn list_remote_node_ingress_profiles(
    state: web::Data<PrimaryAppState>,
    path: web::Path<i64>,
) -> Result<HttpResponse> {
    list_remote_node_storage_targets(state, path).await
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/admin/remote-nodes/{id}/storage-targets",
    tag = "admin",
    operation_id = "list_remote_node_storage_targets",
    params(("id" = i64, Path, description = "Remote node ID")),
    responses(
        (status = 200, description = "List remote node storage targets", body = inline(ApiResponse<Vec<crate::storage::remote_protocol::RemoteStorageTargetInfo>>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Remote node not found"),
        (status = 412, description = "Remote storage targets require a single primary binding"),
    ),
    security(("bearer" = [])),
)]
pub async fn list_remote_node_storage_targets(
    state: web::Data<PrimaryAppState>,
    path: web::Path<i64>,
) -> Result<HttpResponse> {
    let targets = storage_target::list_remote(state.get_ref(), *path).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(targets)))
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/admin/remote-nodes/{id}/ingress-profile-drivers",
    tag = "admin",
    operation_id = "list_remote_node_ingress_profile_drivers",
    params(("id" = i64, Path, description = "Remote node ID")),
    responses(
        (status = 200, description = "Deprecated since 0.4.0; use /storage-target-drivers. List remote node remote storage target driver descriptors", body = inline(ApiResponse<Vec<crate::services::remote::storage_target::RemoteStorageTargetDriverDescriptor>>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Remote node not found"),
    ),
    security(("bearer" = [])),
)]
#[deprecated(
    since = "0.4.0",
    note = "use list_remote_node_storage_target_drivers instead"
)]
pub async fn list_remote_node_ingress_profile_drivers(
    state: web::Data<PrimaryAppState>,
    path: web::Path<i64>,
) -> Result<HttpResponse> {
    list_remote_node_storage_target_drivers(state, path).await
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/admin/remote-nodes/{id}/storage-target-drivers",
    tag = "admin",
    operation_id = "list_remote_node_storage_target_drivers",
    params(("id" = i64, Path, description = "Remote node ID")),
    responses(
        (status = 200, description = "List remote node storage target driver descriptors", body = inline(ApiResponse<Vec<crate::services::remote::storage_target::RemoteStorageTargetDriverDescriptor>>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Remote node not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn list_remote_node_storage_target_drivers(
    state: web::Data<PrimaryAppState>,
    path: web::Path<i64>,
) -> Result<HttpResponse> {
    let descriptors =
        storage_target::list_remote_driver_descriptors(state.get_ref(), *path).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(descriptors)))
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/admin/remote-nodes/{id}/ingress-profiles",
    tag = "admin",
    operation_id = "create_remote_node_ingress_profile",
    params(("id" = i64, Path, description = "Remote node ID")),
    request_body = RemoteCreateStorageTargetRequest,
    responses(
        (status = 201, description = "Deprecated since 0.4.0; use /storage-targets. Remote node remote storage target created", body = inline(ApiResponse<crate::storage::remote_protocol::RemoteStorageTargetInfo>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Remote node not found"),
        (status = 412, description = "Remote storage targets require a single primary binding"),
    ),
    security(("bearer" = [])),
)]
#[deprecated(
    since = "0.4.0",
    note = "use create_remote_node_storage_target instead"
)]
pub async fn create_remote_node_ingress_profile(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
    body: web::Json<RemoteCreateStorageTargetRequest>,
) -> Result<HttpResponse> {
    create_remote_node_storage_target(state, claims, req, path, body).await
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/admin/remote-nodes/{id}/storage-targets",
    tag = "admin",
    operation_id = "create_remote_node_storage_target",
    params(("id" = i64, Path, description = "Remote node ID")),
    request_body = RemoteCreateStorageTargetRequest,
    responses(
        (status = 201, description = "Remote node storage target created", body = inline(ApiResponse<crate::storage::remote_protocol::RemoteStorageTargetInfo>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Remote node not found"),
        (status = 412, description = "Remote storage targets require a single primary binding"),
    ),
    security(("bearer" = [])),
)]
pub async fn create_remote_node_storage_target(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
    body: web::Json<RemoteCreateStorageTargetRequest>,
) -> Result<HttpResponse> {
    let target = storage_target::create_remote(state.get_ref(), *path, body.into_inner()).await?;
    let ctx = audit::AuditContext::from_request(&req, &claims);
    audit::log_with_details(
        state.get_ref(),
        &ctx,
        audit::AuditAction::AdminCreateRemoteIngressProfile,
        crate::services::ops::audit::AuditEntityType::RemoteIngressProfile,
        Some(*path),
        Some(&target.target_key),
        || remote_storage_target_audit_details(&target),
    )
    .await;
    Ok(HttpResponse::Created().json(ApiResponse::ok(target)))
}

#[api_docs_macros::path(
    patch,
    path = "/api/v1/admin/remote-nodes/{id}/ingress-profiles/{target_key}",
    tag = "admin",
    operation_id = "update_remote_node_ingress_profile",
    params(
        ("id" = i64, Path, description = "Remote node ID"),
        ("target_key" = String, Path, description = "Remote storage target key")
    ),
    request_body = RemoteUpdateStorageTargetRequest,
    responses(
        (status = 200, description = "Deprecated since 0.4.0; use /storage-targets/{target_key}. Remote node remote storage target updated", body = inline(ApiResponse<crate::storage::remote_protocol::RemoteStorageTargetInfo>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Remote node or remote storage target not found"),
        (status = 412, description = "Remote storage targets require a single primary binding"),
    ),
    security(("bearer" = [])),
)]
#[deprecated(
    since = "0.4.0",
    note = "use update_remote_node_storage_target instead"
)]
pub async fn update_remote_node_ingress_profile(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<(i64, String)>,
    body: web::Json<RemoteUpdateStorageTargetRequest>,
) -> Result<HttpResponse> {
    update_remote_node_storage_target(state, claims, req, path, body).await
}

#[api_docs_macros::path(
    patch,
    path = "/api/v1/admin/remote-nodes/{id}/storage-targets/{target_key}",
    tag = "admin",
    operation_id = "update_remote_node_storage_target",
    params(
        ("id" = i64, Path, description = "Remote node ID"),
        ("target_key" = String, Path, description = "Remote storage target key")
    ),
    request_body = RemoteUpdateStorageTargetRequest,
    responses(
        (status = 200, description = "Remote node storage target updated", body = inline(ApiResponse<crate::storage::remote_protocol::RemoteStorageTargetInfo>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Remote node or storage target not found"),
        (status = 412, description = "Remote storage targets require a single primary binding"),
    ),
    security(("bearer" = [])),
)]
pub async fn update_remote_node_storage_target(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<(i64, String)>,
    body: web::Json<RemoteUpdateStorageTargetRequest>,
) -> Result<HttpResponse> {
    let (id, target_key) = path.into_inner();
    let target =
        storage_target::update_remote(state.get_ref(), id, &target_key, body.into_inner()).await?;
    let ctx = audit::AuditContext::from_request(&req, &claims);
    audit::log_with_details(
        state.get_ref(),
        &ctx,
        audit::AuditAction::AdminUpdateRemoteIngressProfile,
        crate::services::ops::audit::AuditEntityType::RemoteIngressProfile,
        Some(id),
        Some(&target.target_key),
        || remote_storage_target_audit_details(&target),
    )
    .await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(target)))
}

#[api_docs_macros::path(
    delete,
    path = "/api/v1/admin/remote-nodes/{id}/ingress-profiles/{target_key}",
    tag = "admin",
    operation_id = "delete_remote_node_ingress_profile",
    params(
        ("id" = i64, Path, description = "Remote node ID"),
        ("target_key" = String, Path, description = "Remote storage target key")
    ),
    responses(
        (status = 200, description = "Deprecated since 0.4.0; use /storage-targets/{target_key}. Remote node remote storage target deleted"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Remote node or remote storage target not found"),
        (status = 412, description = "Remote storage targets require a single primary binding"),
    ),
    security(("bearer" = [])),
)]
#[deprecated(
    since = "0.4.0",
    note = "use delete_remote_node_storage_target instead"
)]
pub async fn delete_remote_node_ingress_profile(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<(i64, String)>,
) -> Result<HttpResponse> {
    delete_remote_node_storage_target(state, claims, req, path).await
}

#[api_docs_macros::path(
    delete,
    path = "/api/v1/admin/remote-nodes/{id}/storage-targets/{target_key}",
    tag = "admin",
    operation_id = "delete_remote_node_storage_target",
    params(
        ("id" = i64, Path, description = "Remote node ID"),
        ("target_key" = String, Path, description = "Remote storage target key")
    ),
    responses(
        (status = 200, description = "Remote node storage target deleted"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Remote node or storage target not found"),
        (status = 412, description = "Remote storage targets require a single primary binding"),
    ),
    security(("bearer" = [])),
)]
pub async fn delete_remote_node_storage_target(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<(i64, String)>,
) -> Result<HttpResponse> {
    let (id, target_key) = path.into_inner();
    storage_target::delete_remote(state.get_ref(), id, &target_key).await?;
    let ctx = audit::AuditContext::from_request(&req, &claims);
    audit::log_with_details(
        state.get_ref(),
        &ctx,
        audit::AuditAction::AdminDeleteRemoteIngressProfile,
        crate::services::ops::audit::AuditEntityType::RemoteIngressProfile,
        Some(id),
        Some(&target_key),
        || {
            audit::details(audit::RemoteIngressProfileDeleteAuditDetails {
                target_key: &target_key,
            })
        },
    )
    .await;
    Ok(HttpResponse::Ok().json(ApiResponse::<()>::ok_empty()))
}
