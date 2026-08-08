//! 管理员 API 路由：`policies`。

use crate::api::dto::admin::{
    AdminPolicyGroupListQuery, AdminPolicyListQuery, CreatePolicyGroupReq, CreatePolicyReq,
    DeletePolicyQuery, MigratePolicyGroupAssignmentsReq, PatchPolicyGroupReq, PatchPolicyReq,
    PolicyGroupItemReq, StorageConnectorCatalogQuery, StorageConnectorLocalizationCatalogQuery,
    TestPolicyParamsReq,
};
use crate::api::dto::validate_request;
use crate::api::response::{ApiEmptyData, ApiResponse};
use crate::config::site_url;
use crate::errors::Result;
use crate::runtime::PrimaryAppState;
use crate::services::storage_policy::credential;
use crate::services::{auth::local::Claims, ops::audit, storage_policy::policy};
use actix_web::{HttpRequest, HttpResponse, http::header, web};
use aster_forge_api::LimitOffsetQuery;
#[cfg(all(debug_assertions, feature = "openapi"))]
use aster_forge_api::OffsetPage;

// ── Conversion helpers (must stay here because they use storage_policy::policy types) ──────────

impl From<CreatePolicyReq> for policy::CreateStoragePolicyInput {
    fn from(value: CreatePolicyReq) -> Self {
        Self {
            name: value.name,
            connection: value.connection,
            max_file_size: value.max_file_size.unwrap_or(0),
            chunk_size: value.chunk_size,
            is_default: value.is_default.unwrap_or(false),
            allowed_types: value.allowed_types,
        }
    }
}

impl From<PatchPolicyReq> for policy::UpdateStoragePolicyInput {
    fn from(value: PatchPolicyReq) -> Self {
        Self {
            name: value.name,
            connector_config: value.connector_config,
            behavior: value.behavior,
            credential: value.credential,
            max_file_size: value.max_file_size,
            chunk_size: value.chunk_size,
            is_default: value.is_default,
            allowed_types: value.allowed_types,
        }
    }
}

impl From<TestPolicyParamsReq> for policy::TestDraftStorageConnectorConnectionInput {
    fn from(value: TestPolicyParamsReq) -> Self {
        Self {
            policy_id: value.policy_id,
            connection: value.connection,
        }
    }
}

fn map_group_items(items: Vec<PolicyGroupItemReq>) -> Vec<policy::StoragePolicyGroupItemInput> {
    items.into_iter().map(Into::into).collect()
}

impl From<PolicyGroupItemReq> for policy::StoragePolicyGroupItemInput {
    fn from(value: PolicyGroupItemReq) -> Self {
        Self {
            policy_id: value.policy_id,
            priority: value.priority,
            min_file_size: value.min_file_size,
            max_file_size: value.max_file_size,
        }
    }
}

impl From<CreatePolicyGroupReq> for policy::CreateStoragePolicyGroupInput {
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

impl From<PatchPolicyGroupReq> for policy::UpdateStoragePolicyGroupInput {
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

#[aster_forge_api_docs_macros::path(
    get,
    path = "/api/v1/admin/policies",
    tag = "admin",
    operation_id = "list_policies",
    params(LimitOffsetQuery, AdminPolicyListQuery),
    responses(
        (status = 200, description = "List storage policies", body = inline(ApiResponse<OffsetPage<policy::StoragePolicy>>)),
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
    let policies = policy::list_paginated(
        state.get_ref(),
        page.limit_or(50, 100),
        page.offset(),
        query.sort_by(),
        query.sort_order(),
    )
    .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(policies)))
}

#[aster_forge_api_docs_macros::path(
    get,
    path = "/api/v1/admin/policies/storage-drivers",
    tag = "admin",
    operation_id = "list_storage_driver_descriptors",
    params(StorageConnectorCatalogQuery),
    responses(
        (status = 200, description = "List storage driver capability descriptors", body = inline(ApiResponse<Vec<aster_drive_storage::StorageConnectorDescriptor>>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer" = [])),
)]
pub async fn list_storage_driver_descriptors(
    state: web::Data<PrimaryAppState>,
    query: web::Query<StorageConnectorCatalogQuery>,
) -> Result<HttpResponse> {
    Ok(HttpResponse::Ok().json(ApiResponse::ok(
        crate::services::storage_policy::connector_catalog::list_storage_connector_catalog(
            state.driver_registry().connectors(),
            state.config(),
            query.context.into(),
        ),
    )))
}

#[aster_forge_api_docs_macros::path(
    get,
    path = "/api/v1/admin/policies/storage-drivers/localizations",
    tag = "admin",
    operation_id = "list_storage_driver_localizations",
    params(StorageConnectorLocalizationCatalogQuery),
    responses(
        (status = 200, description = "List connector-owned localized UI resources", body = inline(ApiResponse<aster_drive_storage::StorageConnectorLocalizationCatalog>)),
        (status = 304, description = "Connector localization resources are unchanged"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer" = [])),
)]
pub async fn list_storage_driver_localizations(
    state: web::Data<PrimaryAppState>,
    req: HttpRequest,
    query: web::Query<StorageConnectorLocalizationCatalogQuery>,
) -> Result<HttpResponse> {
    let context = query.context.into();
    let catalog =
        crate::services::storage_policy::connector_catalog::list_storage_connector_localizations(
            state.driver_registry().connectors(),
            state.config(),
            context,
            &query.locale,
        )?;
    let etag =
        crate::services::storage_policy::connector_catalog::storage_connector_localization_etag(
            context, &catalog,
        );
    let cache_control = "private, no-cache";

    if if_none_match_matches(req.headers().get(header::IF_NONE_MATCH), &etag) {
        return Ok(HttpResponse::NotModified()
            .insert_header((header::ETAG, etag))
            .insert_header((header::CACHE_CONTROL, cache_control))
            .finish());
    }

    Ok(HttpResponse::Ok()
        .insert_header((header::ETAG, etag))
        .insert_header((header::CACHE_CONTROL, cache_control))
        .json(ApiResponse::ok(catalog)))
}

fn if_none_match_matches(value: Option<&header::HeaderValue>, current_etag: &str) -> bool {
    let Some(value) = value.and_then(|value| value.to_str().ok()) else {
        return false;
    };
    value.split(',').map(str::trim).any(|candidate| {
        candidate == "*"
            || candidate == current_etag
            || candidate
                .strip_prefix("W/")
                .is_some_and(|weak| weak == current_etag)
    })
}

#[aster_forge_api_docs_macros::path(
    post,
    path = "/api/v1/admin/policies",
    tag = "admin",
    operation_id = "create_policy",
    request_body = CreatePolicyReq,
    responses(
        (status = 201, description = "Policy created", body = inline(ApiResponse<policy::StoragePolicy>)),
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
    let ctx = audit::AuditContext::from_request(&req, &claims);
    let policy = policy::create_with_audit(state.get_ref(), body.into_inner().into(), &ctx).await?;
    Ok(HttpResponse::Created().json(ApiResponse::ok(policy)))
}

#[aster_forge_api_docs_macros::path(
    post,
    path = "/api/v1/admin/policies/connector-transitions/resolve",
    tag = "admin",
    operation_id = "resolve_storage_connector_transitions",
    request_body = policy::ResolveStorageConnectorTransitionsInput,
    responses(
        (status = 200, description = "Resolve compatible connector transitions", body = inline(ApiResponse<policy::StorageConnectorTransitionPreviewList>)),
        (status = 400, description = "Source connector configuration rejected"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer" = [])),
)]
pub async fn resolve_storage_connector_transitions(
    state: web::Data<PrimaryAppState>,
    body: web::Json<policy::ResolveStorageConnectorTransitionsInput>,
) -> Result<HttpResponse> {
    let transitions =
        policy::resolve_connector_transitions(state.get_ref(), body.into_inner()).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(transitions)))
}

#[aster_forge_api_docs_macros::path(
    get,
    path = "/api/v1/admin/policies/{id}",
    tag = "admin",
    operation_id = "get_policy",
    params(("id" = i64, Path, description = "Policy ID")),
    responses(
        (status = 200, description = "Policy details", body = inline(ApiResponse<policy::StoragePolicy>)),
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
    let policy = policy::get(state.get_ref(), *path).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(policy)))
}

#[aster_forge_api_docs_macros::path(
    get,
    path = "/api/v1/admin/policies/{id}/capacity",
    tag = "admin",
    operation_id = "get_policy_capacity",
    params(("id" = i64, Path, description = "Policy ID")),
    responses(
        (status = 200, description = "Storage policy capacity observability", body = inline(ApiResponse<policy::StoragePolicyCapacityInfo>)),
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
    let capacity = policy::capacity_info(state.get_ref(), *path).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(capacity)))
}

fn storage_policy_action_response(result: policy::StoragePolicyActionResult) -> HttpResponse {
    HttpResponse::Ok().json(ApiResponse::ok(result))
}

#[aster_forge_api_docs_macros::path(
    patch,
    path = "/api/v1/admin/policies/{id}",
    tag = "admin",
    operation_id = "update_policy",
    params(("id" = i64, Path, description = "Policy ID")),
    request_body = PatchPolicyReq,
    responses(
        (status = 200, description = "Policy updated", body = inline(ApiResponse<policy::StoragePolicy>)),
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
    let ctx = audit::AuditContext::from_request(&req, &claims);
    let policy =
        policy::update_with_audit(state.get_ref(), *path, body.into_inner().into(), &ctx).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(policy)))
}

#[aster_forge_api_docs_macros::path(
    post,
    path = "/api/v1/admin/policies/{id}/connector-transitions",
    tag = "admin",
    operation_id = "execute_storage_connector_transition",
    params(("id" = i64, Path, description = "Policy ID")),
    request_body = policy::ExecuteStorageConnectorTransitionInput,
    responses(
        (status = 200, description = "Storage policy connector transitioned", body = inline(ApiResponse<policy::StoragePolicy>)),
        (status = 400, description = "Connector transition rejected"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Policy not found"),
        (status = 412, description = "Policy or credential changed, or active uploads block the transition"),
    ),
    security(("bearer" = [])),
)]
pub async fn execute_storage_connector_transition(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
    body: web::Json<policy::ExecuteStorageConnectorTransitionInput>,
) -> Result<HttpResponse> {
    let ctx = audit::AuditContext::from_request(&req, &claims);
    let policy = policy::execute_connector_transition_with_audit(
        state.get_ref(),
        *path,
        body.into_inner(),
        &ctx,
    )
    .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(policy)))
}

#[aster_forge_api_docs_macros::path(
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
    let ctx = audit::AuditContext::from_request(&req, &claims);
    policy::delete_with_audit(state.get_ref(), *path, query.force, &ctx).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::<()>::ok_empty()))
}

#[aster_forge_api_docs_macros::path(
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
    policy::test_connection(state.get_ref(), *path).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::<ApiEmptyData>::ok_empty_data()))
}

#[aster_forge_api_docs_macros::path(
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
    policy::test_connection_params(state.get_ref(), body.into_inner().into()).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::<ApiEmptyData>::ok_empty_data()))
}

#[aster_forge_api_docs_macros::path(
    post,
    path = "/api/v1/admin/policies/{id}/action",
    tag = "admin",
    operation_id = "execute_saved_storage_policy_action",
    params(("id" = i64, Path, description = "Policy ID")),
    request_body = policy::ExecuteSavedStorageConnectorActionInput,
    responses(
        (status = 200, description = "Storage policy action executed", body = inline(ApiResponse<policy::StoragePolicyActionResult>)),
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
    body: web::Json<policy::ExecuteSavedStorageConnectorActionInput>,
) -> Result<HttpResponse> {
    validate_request(&*body)?;
    let ctx = audit::AuditContext::from_request(&req, &claims);
    let result =
        policy::execute_saved_action_with_audit(state.get_ref(), *path, body.into_inner(), &ctx)
            .await?;
    Ok(storage_policy_action_response(result))
}

#[aster_forge_api_docs_macros::path(
    post,
    path = "/api/v1/admin/policies/action",
    tag = "admin",
    operation_id = "execute_draft_storage_policy_action",
    request_body = policy::ExecuteDraftStorageConnectorActionInput,
    responses(
        (status = 200, description = "Storage policy action executed", body = inline(ApiResponse<policy::StoragePolicyActionResult>)),
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
    body: web::Json<policy::ExecuteDraftStorageConnectorActionInput>,
) -> Result<HttpResponse> {
    validate_request(&*body)?;
    let ctx = audit::AuditContext::from_request(&req, &claims);
    let result =
        policy::execute_draft_action_with_audit(state.get_ref(), body.into_inner(), &ctx).await?;
    Ok(storage_policy_action_response(result))
}

#[aster_forge_api_docs_macros::path(
    post,
    path = "/api/v1/admin/policies/{id}/storage-authorization/start",
    tag = "admin",
    operation_id = "start_storage_authorization",
    params(("id" = i64, Path, description = "Policy ID")),
    responses(
        (status = 200, description = "Storage credential authorization URL", body = inline(ApiResponse<credential::StorageAuthorizationStartResponse>)),
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
) -> Result<HttpResponse> {
    let response =
        credential::start_authorization(state.get_ref(), &req, *path, claims.user_id).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(response)))
}

#[aster_forge_api_docs_macros::path(
    get,
    path = "/api/v1/admin/policies/{id}/storage-credentials",
    tag = "admin",
    operation_id = "list_storage_policy_credentials",
    params(("id" = i64, Path, description = "Policy ID")),
    responses(
        (status = 200, description = "Storage policy credentials", body = inline(ApiResponse<Vec<crate::storage::StorageConnectorCredentialInfo>>)),
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
    let credentials = credential::list_policy_credentials(state.get_ref(), *path).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(credentials)))
}

#[aster_forge_api_docs_macros::path(
    post,
    path = "/api/v1/admin/policies/{id}/storage-credentials/validate",
    tag = "admin",
    operation_id = "validate_storage_policy_credential",
    params(
        ("id" = i64, Path, description = "Policy ID"),
    ),
    responses(
        (status = 200, description = "Storage policy credential validation result", body = inline(ApiResponse<credential::StoragePolicyCredentialValidationResult>)),
        (status = 400, description = "Invalid provider or credential state"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Policy or credential not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn validate_storage_policy_credential(
    state: web::Data<PrimaryAppState>,
    path: web::Path<i64>,
) -> Result<HttpResponse> {
    let result = credential::validate_policy_credential(state.get_ref(), *path).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(result)))
}

#[aster_forge_api_docs_macros::path(
    get,
    path = "/api/v1/admin/policies/storage-authorization/callback",
    tag = "admin",
    operation_id = "finish_storage_authorization",
    params(credential::StorageAuthorizationCallbackQuery),
    responses(
        (status = 302, description = "Storage credential authorization callback handled and redirected to the admin policies page with success or error state"),
    ),
)]
pub async fn finish_storage_authorization(
    state: web::Data<PrimaryAppState>,
    query: web::Query<credential::StorageAuthorizationCallbackQuery>,
) -> Result<HttpResponse> {
    match credential::finish_authorization_callback(state.get_ref(), &query).await {
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

#[aster_forge_api_docs_macros::path(
    get,
    path = "/api/v1/admin/policy-groups",
    tag = "admin",
    operation_id = "list_policy_groups",
    params(LimitOffsetQuery, AdminPolicyGroupListQuery),
    responses(
        (status = 200, description = "List storage policy groups", body = inline(ApiResponse<OffsetPage<crate::services::storage_policy::policy::StoragePolicyGroupInfo>>)),
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
    let groups = policy::list_groups_paginated(
        state.get_ref(),
        page.limit_or(50, 100),
        page.offset(),
        query.sort_by(),
        query.sort_order(),
    )
    .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(groups)))
}

#[aster_forge_api_docs_macros::path(
    post,
    path = "/api/v1/admin/policy-groups",
    tag = "admin",
    operation_id = "create_policy_group",
    request_body = CreatePolicyGroupReq,
    responses(
        (status = 201, description = "Policy group created", body = inline(ApiResponse<crate::services::storage_policy::policy::StoragePolicyGroupInfo>)),
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
    let ctx = audit::AuditContext::from_request(&req, &claims);
    let group =
        policy::create_group_with_audit(state.get_ref(), body.into_inner().into(), &ctx).await?;
    Ok(HttpResponse::Created().json(ApiResponse::ok(group)))
}

#[aster_forge_api_docs_macros::path(
    get,
    path = "/api/v1/admin/policy-groups/{id}",
    tag = "admin",
    operation_id = "get_policy_group",
    params(("id" = i64, Path, description = "Policy group ID")),
    responses(
        (status = 200, description = "Policy group details", body = inline(ApiResponse<crate::services::storage_policy::policy::StoragePolicyGroupInfo>)),
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
    let group = policy::get_group(state.get_ref(), *path).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(group)))
}

#[aster_forge_api_docs_macros::path(
    patch,
    path = "/api/v1/admin/policy-groups/{id}",
    tag = "admin",
    operation_id = "update_policy_group",
    params(("id" = i64, Path, description = "Policy group ID")),
    request_body = PatchPolicyGroupReq,
    responses(
        (status = 200, description = "Policy group updated", body = inline(ApiResponse<crate::services::storage_policy::policy::StoragePolicyGroupInfo>)),
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
    let ctx = audit::AuditContext::from_request(&req, &claims);
    let group =
        policy::update_group_with_audit(state.get_ref(), *path, body.into_inner().into(), &ctx)
            .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(group)))
}

#[aster_forge_api_docs_macros::path(
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
    let ctx = audit::AuditContext::from_request(&req, &claims);
    policy::delete_group_with_audit(state.get_ref(), *path, &ctx).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::<()>::ok_empty()))
}

#[aster_forge_api_docs_macros::path(
    post,
    path = "/api/v1/admin/policy-groups/{id}/migrate-assignments",
    tag = "admin",
    operation_id = "migrate_policy_group_assignments",
    params(("id" = i64, Path, description = "Source policy group ID")),
    request_body = MigratePolicyGroupAssignmentsReq,
    responses(
        (status = 200, description = "Policy group assignments migrated", body = inline(ApiResponse<crate::services::storage_policy::policy::PolicyGroupAssignmentMigrationResult>)),
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
    let ctx = audit::AuditContext::from_request(&req, &claims);
    let result = policy::migrate_group_assignments_with_audit(
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

    #[test]
    fn connector_localization_if_none_match_supports_lists_weak_tags_and_wildcard() {
        let current = "\"revision\"";
        assert!(if_none_match_matches(
            Some(&header::HeaderValue::from_static("\"old\", \"revision\"")),
            current,
        ));
        assert!(if_none_match_matches(
            Some(&header::HeaderValue::from_static("W/\"revision\"")),
            current,
        ));
        assert!(if_none_match_matches(
            Some(&header::HeaderValue::from_static("*")),
            current,
        ));
    }

    #[test]
    fn connector_localization_if_none_match_ignores_nonmatching_or_invalid_values() {
        let current = "\"revision\"";
        assert!(!if_none_match_matches(
            Some(&header::HeaderValue::from_static("\"old\"")),
            current,
        ));
        assert!(!if_none_match_matches(None, current));
        assert!(!if_none_match_matches(
            Some(&header::HeaderValue::from_bytes(&[0xff]).unwrap()),
            current,
        ));
    }
}
