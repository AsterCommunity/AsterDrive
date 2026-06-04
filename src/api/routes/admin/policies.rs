//! 管理员 API 路由：`policies`。

use crate::api::dto::admin::{
    AdminPolicyGroupListQuery, AdminPolicyListQuery, CreatePolicyGroupReq, CreatePolicyReq,
    DeletePolicyQuery, MigratePolicyGroupAssignmentsReq, PatchPolicyGroupReq, PatchPolicyReq,
    PolicyGroupItemReq, TestPolicyParamsReq,
};
use crate::api::dto::validate_request;
use crate::api::pagination::LimitOffsetQuery;
#[cfg(all(debug_assertions, feature = "openapi"))]
use crate::api::pagination::OffsetPage;
use crate::api::response::ApiResponse;
use crate::errors::Result;
use crate::runtime::PrimaryAppState;
use crate::services::{audit_service, auth_service::Claims, policy_service};
use crate::types::DriverType;
use actix_web::{HttpRequest, HttpResponse, web};

// ── Conversion helpers (must stay here because they use policy_service types) ──────────

fn build_policy_connection_input(
    driver_type: DriverType,
    endpoint: Option<String>,
    bucket: Option<String>,
    access_key: Option<String>,
    secret_key: Option<String>,
    base_path: Option<String>,
    remote_node_id: Option<i64>,
) -> policy_service::StoragePolicyConnectionInput {
    policy_service::StoragePolicyConnectionInput {
        driver_type,
        endpoint: endpoint.unwrap_or_default(),
        bucket: bucket.unwrap_or_default(),
        access_key: access_key.unwrap_or_default(),
        secret_key: secret_key.unwrap_or_default(),
        base_path: base_path.unwrap_or_default(),
        remote_node_id,
    }
}

impl From<CreatePolicyReq> for policy_service::CreateStoragePolicyInput {
    fn from(value: CreatePolicyReq) -> Self {
        Self {
            name: value.name,
            connection: build_policy_connection_input(
                value.driver_type,
                value.endpoint,
                value.bucket,
                value.access_key,
                value.secret_key,
                value.base_path,
                value.remote_node_id,
            ),
            max_file_size: value.max_file_size.unwrap_or(0),
            chunk_size: value.chunk_size,
            is_default: value.is_default.unwrap_or(false),
            allowed_types: value.allowed_types,
            options: value.options,
        }
    }
}

impl From<PatchPolicyReq> for policy_service::UpdateStoragePolicyInput {
    fn from(value: PatchPolicyReq) -> Self {
        Self {
            name: value.name,
            endpoint: value.endpoint,
            bucket: value.bucket,
            access_key: value.access_key,
            secret_key: value.secret_key,
            base_path: value.base_path,
            remote_node_id: value.remote_node_id,
            max_file_size: value.max_file_size,
            chunk_size: value.chunk_size,
            is_default: value.is_default,
            allowed_types: value.allowed_types,
            options: value.options,
        }
    }
}

impl From<TestPolicyParamsReq> for policy_service::StoragePolicyConnectionInput {
    fn from(value: TestPolicyParamsReq) -> Self {
        build_policy_connection_input(
            value.driver_type,
            value.endpoint,
            value.bucket,
            value.access_key,
            value.secret_key,
            value.base_path,
            value.remote_node_id,
        )
    }
}

fn map_group_items(
    items: Vec<PolicyGroupItemReq>,
) -> Vec<policy_service::StoragePolicyGroupItemInput> {
    items.into_iter().map(Into::into).collect()
}

impl From<PolicyGroupItemReq> for policy_service::StoragePolicyGroupItemInput {
    fn from(value: PolicyGroupItemReq) -> Self {
        Self {
            policy_id: value.policy_id,
            priority: value.priority,
            min_file_size: value.min_file_size,
            max_file_size: value.max_file_size,
        }
    }
}

impl From<CreatePolicyGroupReq> for policy_service::CreateStoragePolicyGroupInput {
    fn from(value: CreatePolicyGroupReq) -> Self {
        Self {
            name: value.name,
            description: value.description,
            is_enabled: value.is_enabled,
            is_default: value.is_default,
            items: map_group_items(value.items),
        }
    }
}

impl From<PatchPolicyGroupReq> for policy_service::UpdateStoragePolicyGroupInput {
    fn from(value: PatchPolicyGroupReq) -> Self {
        Self {
            name: value.name,
            description: value.description,
            is_enabled: value.is_enabled,
            is_default: value.is_default,
            items: value.items.map(map_group_items),
        }
    }
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/admin/policies",
    tag = "admin",
    operation_id = "list_policies",
    params(LimitOffsetQuery, AdminPolicyListQuery),
    responses(
        (status = 200, description = "List storage policies", body = inline(ApiResponse<OffsetPage<policy_service::StoragePolicy>>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer" = [])),
)]
pub async fn list_policies(
    state: web::Data<PrimaryAppState>,
    page: web::Query<LimitOffsetQuery>,
    query: web::Query<AdminPolicyListQuery>,
) -> Result<HttpResponse> {
    let policies = policy_service::list_paginated(
        &state,
        page.limit_or(50, 100),
        page.offset(),
        query.sort_by(),
        query.sort_order(),
    )
    .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(policies)))
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/admin/policies",
    tag = "admin",
    operation_id = "create_policy",
    request_body = CreatePolicyReq,
    responses(
        (status = 201, description = "Policy created", body = inline(ApiResponse<policy_service::StoragePolicy>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer" = [])),
)]
pub async fn create_policy(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    body: web::Json<CreatePolicyReq>,
) -> Result<HttpResponse> {
    validate_request(&*body)?;
    let ctx = audit_service::AuditContext::from_request(&req, &claims);
    let policy = policy_service::create_with_audit(&state, body.into_inner().into(), &ctx).await?;
    Ok(HttpResponse::Created().json(ApiResponse::ok(policy)))
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/admin/policies/{id}",
    tag = "admin",
    operation_id = "get_policy",
    params(("id" = i64, Path, description = "Policy ID")),
    responses(
        (status = 200, description = "Policy details", body = inline(ApiResponse<policy_service::StoragePolicy>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Policy not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn get_policy(
    state: web::Data<PrimaryAppState>,
    path: web::Path<i64>,
) -> Result<HttpResponse> {
    let policy = policy_service::get(&state, *path).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(policy)))
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/admin/policies/{id}/capacity",
    tag = "admin",
    operation_id = "get_policy_capacity",
    params(("id" = i64, Path, description = "Policy ID")),
    responses(
        (status = 200, description = "Storage policy capacity observability", body = inline(ApiResponse<policy_service::StoragePolicyCapacityInfo>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Policy not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn get_policy_capacity(
    state: web::Data<PrimaryAppState>,
    path: web::Path<i64>,
) -> Result<HttpResponse> {
    let capacity = policy_service::capacity_info(&state, *path).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(capacity)))
}

#[api_docs_macros::path(
    patch,
    path = "/api/v1/admin/policies/{id}",
    tag = "admin",
    operation_id = "update_policy",
    params(("id" = i64, Path, description = "Policy ID")),
    request_body = PatchPolicyReq,
    responses(
        (status = 200, description = "Policy updated", body = inline(ApiResponse<policy_service::StoragePolicy>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Policy not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn update_policy(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
    body: web::Json<PatchPolicyReq>,
) -> Result<HttpResponse> {
    validate_request(&*body)?;
    let ctx = audit_service::AuditContext::from_request(&req, &claims);
    let policy =
        policy_service::update_with_audit(&state, *path, body.into_inner().into(), &ctx).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(policy)))
}

#[api_docs_macros::path(
    delete,
    path = "/api/v1/admin/policies/{id}",
    tag = "admin",
    operation_id = "delete_policy",
    params(("id" = i64, Path, description = "Policy ID"), DeletePolicyQuery),
    responses(
        (status = 200, description = "Policy deleted"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Policy not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn delete_policy(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
    query: web::Query<DeletePolicyQuery>,
) -> Result<HttpResponse> {
    let ctx = audit_service::AuditContext::from_request(&req, &claims);
    policy_service::delete_with_audit(&state, *path, query.force, &ctx).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::<()>::ok_empty()))
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/admin/policies/{id}/test",
    tag = "admin",
    operation_id = "test_policy_connection",
    params(("id" = i64, Path, description = "Policy ID")),
    responses(
        (status = 200, description = "Connection successful"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 400, description = "Connection failed"),
    ),
    security(("bearer" = [])),
)]
pub async fn test_policy_connection(
    state: web::Data<PrimaryAppState>,
    path: web::Path<i64>,
) -> Result<HttpResponse> {
    policy_service::test_connection(&state, *path).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::<()>::ok_empty()))
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/admin/policies/test",
    tag = "admin",
    operation_id = "test_policy_params",
    request_body = TestPolicyParamsReq,
    responses(
        (status = 200, description = "Connection successful"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 400, description = "Connection failed"),
    ),
    security(("bearer" = [])),
)]
pub async fn test_policy_params(
    state: web::Data<PrimaryAppState>,
    body: web::Json<TestPolicyParamsReq>,
) -> Result<HttpResponse> {
    validate_request(&*body)?;
    policy_service::test_connection_params(&state, body.into_inner().into()).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::<()>::ok_empty()))
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/admin/policy-groups",
    tag = "admin",
    operation_id = "list_policy_groups",
    params(LimitOffsetQuery, AdminPolicyGroupListQuery),
    responses(
        (status = 200, description = "List storage policy groups", body = inline(ApiResponse<OffsetPage<crate::services::policy_service::StoragePolicyGroupInfo>>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer" = [])),
)]
pub async fn list_policy_groups(
    state: web::Data<PrimaryAppState>,
    page: web::Query<LimitOffsetQuery>,
    query: web::Query<AdminPolicyGroupListQuery>,
) -> Result<HttpResponse> {
    let groups = policy_service::list_groups_paginated(
        &state,
        page.limit_or(50, 100),
        page.offset(),
        query.sort_by(),
        query.sort_order(),
    )
    .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(groups)))
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/admin/policy-groups",
    tag = "admin",
    operation_id = "create_policy_group",
    request_body = CreatePolicyGroupReq,
    responses(
        (status = 201, description = "Policy group created", body = inline(ApiResponse<crate::services::policy_service::StoragePolicyGroupInfo>)),
        (status = 400, description = "Bad Request"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer" = [])),
)]
pub async fn create_policy_group(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    body: web::Json<CreatePolicyGroupReq>,
) -> Result<HttpResponse> {
    validate_request(&*body)?;
    let ctx = audit_service::AuditContext::from_request(&req, &claims);
    let group =
        policy_service::create_group_with_audit(&state, body.into_inner().into(), &ctx).await?;
    Ok(HttpResponse::Created().json(ApiResponse::ok(group)))
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/admin/policy-groups/{id}",
    tag = "admin",
    operation_id = "get_policy_group",
    params(("id" = i64, Path, description = "Policy group ID")),
    responses(
        (status = 200, description = "Policy group details", body = inline(ApiResponse<crate::services::policy_service::StoragePolicyGroupInfo>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Policy group not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn get_policy_group(
    state: web::Data<PrimaryAppState>,
    path: web::Path<i64>,
) -> Result<HttpResponse> {
    let group = policy_service::get_group(&state, *path).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(group)))
}

#[api_docs_macros::path(
    patch,
    path = "/api/v1/admin/policy-groups/{id}",
    tag = "admin",
    operation_id = "update_policy_group",
    params(("id" = i64, Path, description = "Policy group ID")),
    request_body = PatchPolicyGroupReq,
    responses(
        (status = 200, description = "Policy group updated", body = inline(ApiResponse<crate::services::policy_service::StoragePolicyGroupInfo>)),
        (status = 400, description = "Bad Request"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Policy group not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn update_policy_group(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
    body: web::Json<PatchPolicyGroupReq>,
) -> Result<HttpResponse> {
    validate_request(&*body)?;
    let ctx = audit_service::AuditContext::from_request(&req, &claims);
    let group =
        policy_service::update_group_with_audit(&state, *path, body.into_inner().into(), &ctx)
            .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(group)))
}

#[api_docs_macros::path(
    delete,
    path = "/api/v1/admin/policy-groups/{id}",
    tag = "admin",
    operation_id = "delete_policy_group",
    params(("id" = i64, Path, description = "Policy group ID")),
    responses(
        (status = 200, description = "Policy group removed"),
        (status = 400, description = "Bad Request"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Policy group not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn delete_policy_group(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
) -> Result<HttpResponse> {
    let ctx = audit_service::AuditContext::from_request(&req, &claims);
    policy_service::delete_group_with_audit(&state, *path, &ctx).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::<()>::ok_empty()))
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/admin/policy-groups/{id}/migrate-assignments",
    tag = "admin",
    operation_id = "migrate_policy_group_assignments",
    params(("id" = i64, Path, description = "Source policy group ID")),
    request_body = MigratePolicyGroupAssignmentsReq,
    responses(
        (status = 200, description = "Policy group assignments migrated", body = inline(ApiResponse<crate::services::policy_service::PolicyGroupAssignmentMigrationResult>)),
        (status = 400, description = "Bad Request"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Policy group not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn migrate_policy_group_assignments(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
    body: web::Json<MigratePolicyGroupAssignmentsReq>,
) -> Result<HttpResponse> {
    validate_request(&*body)?;
    let ctx = audit_service::AuditContext::from_request(&req, &claims);
    let result = policy_service::migrate_group_assignments_with_audit(
        &state,
        *path,
        body.target_group_id,
        &ctx,
    )
    .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(result)))
}
