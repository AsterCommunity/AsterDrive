//! 管理员 API 路由：`policies`。

use crate::api::dto::admin::{
    AdminPolicyGroupListQuery, AdminPolicyListQuery, CreatePolicyGroupReq, CreatePolicyReq,
    DeletePolicyQuery, ExecuteDraftStoragePolicyActionReq, ExecuteSavedStoragePolicyActionReq,
    MigratePolicyGroupAssignmentsReq, PatchPolicyGroupReq, PatchPolicyReq, PolicyGroupItemReq,
    PromoteS3CompatiblePolicyDriverReq, StartStorageAuthorizationReq, TestPolicyParamsReq,
};
use crate::api::dto::validate_request;
use crate::api::pagination::LimitOffsetQuery;
#[cfg(all(debug_assertions, feature = "openapi"))]
use crate::api::pagination::OffsetPage;
use crate::api::response::{ApiEmptyData, ApiResponse};
use crate::config::site_url;
use crate::errors::Result;
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::storage_credential_service;
use crate::services::{audit_service, auth::local::Claims, policy_service};
use crate::types::DriverType;
use actix_web::{HttpRequest, HttpResponse, http::header, web};

// ── Conversion helpers (must stay here because they use policy_service types) ──────────

struct PolicyConnectionInputParts {
    driver_type: DriverType,
    endpoint: Option<String>,
    bucket: Option<String>,
    access_key: Option<String>,
    secret_key: Option<String>,
    base_path: Option<String>,
    remote_node_id: Option<i64>,
    remote_storage_target_key: Option<String>,
    options: crate::types::StoragePolicyOptions,
}

impl From<PolicyConnectionInputParts> for policy_service::StoragePolicyConnectionInput {
    fn from(value: PolicyConnectionInputParts) -> Self {
        Self {
            driver_type: value.driver_type,
            endpoint: value.endpoint.unwrap_or_default(),
            bucket: value.bucket.unwrap_or_default(),
            access_key: value.access_key.unwrap_or_default(),
            secret_key: value.secret_key.unwrap_or_default(),
            base_path: value.base_path.unwrap_or_default(),
            remote_node_id: value.remote_node_id,
            remote_storage_target_key: value.remote_storage_target_key,
            options: value.options,
        }
    }
}

impl From<CreatePolicyReq> for policy_service::CreateStoragePolicyInput {
    fn from(value: CreatePolicyReq) -> Self {
        Self {
            name: value.name,
            connection: PolicyConnectionInputParts {
                driver_type: value.driver_type,
                endpoint: value.endpoint,
                bucket: value.bucket,
                access_key: value.access_key,
                secret_key: value.secret_key,
                base_path: value.base_path,
                remote_node_id: value.remote_node_id,
                remote_storage_target_key: value.remote_storage_target_key.clone(),
                options: crate::types::StoragePolicyOptions::default(),
            }
            .into(),
            max_file_size: value.max_file_size.unwrap_or(0),
            chunk_size: value.chunk_size,
            is_default: value.is_default.unwrap_or(false),
            allowed_types: value.allowed_types,
            options: value.options,
            remote_storage_target_key: value.remote_storage_target_key,
            application_config: value.application_config.unwrap_or_default(),
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
            remote_storage_target_key: value.remote_storage_target_key,
            max_file_size: value.max_file_size,
            chunk_size: value.chunk_size,
            is_default: value.is_default,
            allowed_types: value.allowed_types,
            options: value.options,
            application_config: value.application_config.unwrap_or_default(),
        }
    }
}

impl From<TestPolicyParamsReq> for policy_service::TestDraftStoragePolicyConnectionInput {
    fn from(value: TestPolicyParamsReq) -> Self {
        Self {
            policy_id: value.policy_id,
            connection: PolicyConnectionInputParts {
                driver_type: value.driver_type,
                endpoint: value.endpoint,
                bucket: value.bucket,
                access_key: value.access_key,
                secret_key: value.secret_key,
                base_path: value.base_path,
                remote_node_id: value.remote_node_id,
                remote_storage_target_key: value.remote_storage_target_key,
                options: value.options.unwrap_or_default(),
            }
            .into(),
        }
    }
}

impl From<ExecuteDraftStoragePolicyActionReq>
    for policy_service::ExecuteDraftStoragePolicyActionInput
{
    fn from(value: ExecuteDraftStoragePolicyActionReq) -> Self {
        Self {
            action: value.action,
            policy_id: value.policy_id,
            connection: PolicyConnectionInputParts {
                driver_type: value.driver_type,
                endpoint: value.endpoint,
                bucket: value.bucket,
                access_key: value.access_key,
                secret_key: value.secret_key,
                base_path: value.base_path,
                remote_node_id: value.remote_node_id,
                remote_storage_target_key: value.remote_storage_target_key,
                options: value.options.unwrap_or_default(),
            }
            .into(),
        }
    }
}

impl From<ExecuteSavedStoragePolicyActionReq>
    for policy_service::ExecuteSavedStoragePolicyActionInput
{
    fn from(value: ExecuteSavedStoragePolicyActionReq) -> Self {
        Self {
            action: value.action,
        }
    }
}

impl From<PromoteS3CompatiblePolicyDriverReq>
    for policy_service::PromoteS3CompatiblePolicyDriverInput
{
    fn from(value: PromoteS3CompatiblePolicyDriverReq) -> Self {
        Self {
            target_driver_type: value.target_driver_type,
            endpoint: value.endpoint,
            bucket: value.bucket,
        }
    }
}

impl From<StartStorageAuthorizationReq>
    for storage_credential_service::StorageAuthorizationStartInput
{
    fn from(value: StartStorageAuthorizationReq) -> Self {
        Self {
            provider: value.provider,
            microsoft_graph: value.microsoft_graph,
        }
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
        state.get_ref(),
        page.limit_or(50, 100),
        page.offset(),
        query.sort_by(),
        query.sort_order(),
    )
    .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(policies)))
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/admin/policies/storage-drivers",
    tag = "admin",
    operation_id = "list_storage_driver_descriptors",
    responses(
        (status = 200, description = "List storage driver capability descriptors", body = inline(ApiResponse<Vec<crate::storage::StorageConnectorDescriptor>>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer" = [])),
)]
pub async fn list_storage_driver_descriptors() -> Result<HttpResponse> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(
        crate::storage::connectors::list_storage_driver_descriptors(),
    )))
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
    let policy =
        policy_service::create_with_audit(state.get_ref(), body.into_inner().into(), &ctx).await?;
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
    let policy = policy_service::get(state.get_ref(), *path).await?;
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
    let capacity = policy_service::capacity_info(state.get_ref(), *path).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(capacity)))
}

fn storage_policy_action_response(
    result: policy_service::StoragePolicyActionResult,
) -> HttpResponse {
    HttpResponse::Ok().json(ApiResponse::ok(result))
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
        policy_service::update_with_audit(state.get_ref(), *path, body.into_inner().into(), &ctx)
            .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(policy)))
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/admin/policies/{id}/promote-s3-driver",
    tag = "admin",
    operation_id = "promote_s3_compatible_policy_driver",
    params(("id" = i64, Path, description = "Policy ID")),
    request_body = PromoteS3CompatiblePolicyDriverReq,
    responses(
        (status = 200, description = "Policy driver promoted", body = inline(ApiResponse<policy_service::StoragePolicy>)),
        (status = 400, description = "Promotion rejected"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Policy not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn promote_s3_compatible_policy_driver(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
    body: web::Json<PromoteS3CompatiblePolicyDriverReq>,
) -> Result<HttpResponse> {
    validate_request(&*body)?;
    let ctx = audit_service::AuditContext::from_request(&req, &claims);
    let policy = policy_service::promote_s3_compatible_driver_with_audit(
        state.get_ref(),
        *path,
        body.into_inner().into(),
        &ctx,
    )
    .await?;
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
    policy_service::delete_with_audit(state.get_ref(), *path, query.force, &ctx).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::<()>::ok_empty()))
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/admin/policies/{id}/test",
    tag = "admin",
    operation_id = "test_policy_connection",
    params(("id" = i64, Path, description = "Policy ID")),
    responses(
        (status = 200, description = "Connection successful", body = inline(ApiResponse<ApiEmptyData>)),
        (status = 400, description = "Connection request rejected"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
    ),
    security(("bearer" = [])),
)]
pub async fn test_policy_connection(
    state: web::Data<PrimaryAppState>,
    path: web::Path<i64>,
) -> Result<HttpResponse> {
    policy_service::test_connection(state.get_ref(), *path).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::<ApiEmptyData>::ok_empty_data()))
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/admin/policies/test",
    tag = "admin",
    operation_id = "test_policy_params",
    request_body = TestPolicyParamsReq,
    responses(
        (status = 200, description = "Connection successful", body = inline(ApiResponse<ApiEmptyData>)),
        (status = 400, description = "Connection request rejected"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
    ),
    security(("bearer" = [])),
)]
pub async fn test_policy_params(
    state: web::Data<PrimaryAppState>,
    body: web::Json<TestPolicyParamsReq>,
) -> Result<HttpResponse> {
    validate_request(&*body)?;
    policy_service::test_connection_params(state.get_ref(), body.into_inner().into()).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::<ApiEmptyData>::ok_empty_data()))
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/admin/policies/{id}/action",
    tag = "admin",
    operation_id = "execute_saved_storage_policy_action",
    params(("id" = i64, Path, description = "Policy ID")),
    request_body = ExecuteSavedStoragePolicyActionReq,
    responses(
        (status = 200, description = "Storage policy action executed", body = inline(ApiResponse<policy_service::StoragePolicyActionResult>)),
        (status = 400, description = "Action rejected"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Policy not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn execute_saved_storage_policy_action(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    path: web::Path<i64>,
    req: HttpRequest,
    body: web::Json<ExecuteSavedStoragePolicyActionReq>,
) -> Result<HttpResponse> {
    validate_request(&*body)?;
    let ctx = audit_service::AuditContext::from_request(&req, &claims);
    let result = policy_service::execute_saved_action_with_audit(
        state.get_ref(),
        *path,
        body.into_inner().into(),
        &ctx,
    )
    .await?;
    Ok(storage_policy_action_response(result))
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/admin/policies/action",
    tag = "admin",
    operation_id = "execute_draft_storage_policy_action",
    request_body = ExecuteDraftStoragePolicyActionReq,
    responses(
        (status = 200, description = "Storage policy action executed", body = inline(ApiResponse<policy_service::StoragePolicyActionResult>)),
        (status = 400, description = "Action rejected"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer" = [])),
)]
pub async fn execute_draft_storage_policy_action(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    body: web::Json<ExecuteDraftStoragePolicyActionReq>,
) -> Result<HttpResponse> {
    validate_request(&*body)?;
    let ctx = audit_service::AuditContext::from_request(&req, &claims);
    let result = policy_service::execute_draft_action_with_audit(
        state.get_ref(),
        body.into_inner().into(),
        &ctx,
    )
    .await?;
    Ok(storage_policy_action_response(result))
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/admin/policies/storage-credential-providers",
    tag = "admin",
    operation_id = "list_storage_credential_providers",
    responses(
        (status = 200, description = "Supported storage credential providers", body = inline(ApiResponse<Vec<storage_credential_service::StorageCredentialProviderInfo>>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer" = [])),
)]
pub async fn list_storage_credential_providers() -> Result<HttpResponse> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(
        storage_credential_service::list_supported_providers(),
    )))
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/admin/policies/{id}/storage-authorization/start",
    tag = "admin",
    operation_id = "start_storage_authorization",
    params(("id" = i64, Path, description = "Policy ID")),
    request_body = StartStorageAuthorizationReq,
    responses(
        (status = 200, description = "Storage credential authorization URL", body = inline(ApiResponse<storage_credential_service::StorageAuthorizationStartResponse>)),
        (status = 400, description = "Invalid authorization configuration"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Policy not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn start_storage_authorization(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
    body: web::Json<StartStorageAuthorizationReq>,
) -> Result<HttpResponse> {
    validate_request(&*body)?;
    let response = storage_credential_service::start_authorization(
        state.get_ref(),
        &req,
        *path,
        claims.user_id,
        body.into_inner().into(),
    )
    .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(response)))
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/admin/policies/{id}/storage-credentials",
    tag = "admin",
    operation_id = "list_storage_policy_credentials",
    params(("id" = i64, Path, description = "Policy ID")),
    responses(
        (status = 200, description = "Storage policy credentials", body = inline(ApiResponse<Vec<storage_credential_service::StoragePolicyCredentialInfo>>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Policy not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn list_storage_policy_credentials(
    state: web::Data<PrimaryAppState>,
    path: web::Path<i64>,
) -> Result<HttpResponse> {
    let credentials =
        storage_credential_service::list_policy_credentials(state.get_ref(), *path).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(credentials)))
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/admin/policies/{id}/storage-credentials/{provider}/validate",
    tag = "admin",
    operation_id = "validate_storage_policy_credential",
    params(
        ("id" = i64, Path, description = "Policy ID"),
        ("provider" = String, Path, description = "Storage credential provider"),
    ),
    responses(
        (status = 200, description = "Storage policy credential validation result", body = inline(ApiResponse<storage_credential_service::StoragePolicyCredentialValidationResult>)),
        (status = 400, description = "Invalid provider or credential state"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Policy or credential not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn validate_storage_policy_credential(
    state: web::Data<PrimaryAppState>,
    path: web::Path<(i64, String)>,
) -> Result<HttpResponse> {
    let (policy_id, provider) = path.into_inner();
    let provider = provider.parse().map_err(|()| {
        crate::errors::AsterError::validation_error("unsupported storage credential provider")
    })?;
    let result = storage_credential_service::validate_policy_credential(
        state.get_ref(),
        policy_id,
        provider,
    )
    .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(result)))
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/admin/policies/storage-authorization/callback",
    tag = "admin",
    operation_id = "finish_storage_authorization",
    params(storage_credential_service::StorageAuthorizationCallbackQuery),
    responses(
        (status = 302, description = "Storage credential authorization callback handled and redirected to the admin policies page with success or error state"),
    ),
)]
pub async fn finish_storage_authorization(
    state: web::Data<PrimaryAppState>,
    query: web::Query<storage_credential_service::StorageAuthorizationCallbackQuery>,
) -> Result<HttpResponse> {
    match storage_credential_service::finish_authorization_callback(state.get_ref(), &query).await {
        Ok(response) => Ok(storage_authorization_redirect_response(
            state.get_ref(),
            "success",
            Some(response.credential.policy_id),
            None,
        )),
        Err(error) => {
            let reason = error.reason().as_str();
            tracing::warn!(
                error = %error,
                reason,
                "storage authorization callback failed"
            );
            Ok(storage_authorization_redirect_response(
                state.get_ref(),
                "error",
                None,
                Some(reason),
            ))
        }
    }
}

fn storage_authorization_redirect_response(
    state: &PrimaryAppState,
    status: &str,
    policy_id: Option<i64>,
    reason: Option<&str>,
) -> HttpResponse {
    let path = storage_authorization_redirect_path(status, policy_id, reason);
    let redirect_url = site_url::public_app_url_or_path(state.runtime_config(), &path);
    HttpResponse::Found()
        .append_header((header::LOCATION, redirect_url))
        .finish()
}

fn storage_authorization_redirect_path(
    status: &str,
    policy_id: Option<i64>,
    reason: Option<&str>,
) -> String {
    let mut path = format!(
        "/admin/policies?storage_authorization={}",
        urlencoding::encode(status)
    );
    if let Some(policy_id) = policy_id {
        path.push_str("&policy_id=");
        path.push_str(&policy_id.to_string());
    }
    if let Some(reason) = reason {
        path.push_str("&reason=");
        path.push_str(&urlencoding::encode(reason));
    }
    path
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
        state.get_ref(),
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
        policy_service::create_group_with_audit(state.get_ref(), body.into_inner().into(), &ctx)
            .await?;
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
    let group = policy_service::get_group(state.get_ref(), *path).await?;
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
    let group = policy_service::update_group_with_audit(
        state.get_ref(),
        *path,
        body.into_inner().into(),
        &ctx,
    )
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
    policy_service::delete_group_with_audit(state.get_ref(), *path, &ctx).await?;
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
        state.get_ref(),
        *path,
        body.target_group_id,
        &ctx,
    )
    .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(result)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_authorization_redirect_path_includes_success_and_policy_id() {
        assert_eq!(
            storage_authorization_redirect_path("success", Some(42), None),
            "/admin/policies?storage_authorization=success&policy_id=42"
        );
    }

    #[test]
    fn storage_authorization_redirect_path_includes_stable_error_reason() {
        assert_eq!(
            storage_authorization_redirect_path("error", None, Some("invalid_state")),
            "/admin/policies?storage_authorization=error&reason=invalid_state"
        );
    }
}
