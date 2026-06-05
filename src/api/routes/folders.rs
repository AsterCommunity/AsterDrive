//! API 路由：`folders`。

pub use crate::api::dto::folders::*;
use crate::api::dto::validate_request;
use crate::api::middleware::auth::JwtAuth;
use crate::api::middleware::rate_limit;
use crate::api::pagination::FolderListQuery;
use crate::api::response::ApiResponse;
use crate::api::routes::{files, team_scope};
use crate::config::{NetworkTrustConfig, RateLimitConfig};
use crate::errors::Result;
use crate::runtime::PrimaryAppState;
use crate::services::{
    audit_service::AuditContext, auth_service::Claims, folder_service,
    workspace_storage_service::WorkspaceStorageScope,
};
use actix_governor::Governor;
use actix_web::middleware::Condition;
use actix_web::{HttpRequest, HttpResponse, web};

pub fn routes(
    rl: &RateLimitConfig,
    network_trust: &NetworkTrustConfig,
) -> impl actix_web::dev::HttpServiceFactory + use<> {
    let limiter = rate_limit::build_governor(&rl.api, &network_trust.trusted_proxies);

    web::scope("/folders")
        .wrap(JwtAuth)
        .wrap(Condition::new(rl.enabled, Governor::new(&limiter)))
        .route("", web::get().to(list_root))
        .route("", web::post().to(create_folder))
        .route("/{id}", web::get().to(list_folder))
        .route("/{id}/info", web::get().to(get_folder_info))
        .route("/{id}/ancestors", web::get().to(get_ancestors))
        .route("/{id}/lock", web::post().to(set_lock))
        .route("/{id}/copy", web::post().to(copy_folder))
        .route("/{id}", web::delete().to(delete_folder))
        .route("/{id}", web::patch().to(patch_folder))
}

pub fn team_routes() -> actix_web::Scope {
    web::scope("/{team_id}")
        .route("/folders", web::get().to(team_list_root))
        .route("/folders", web::post().to(team_create_folder))
        .route("/folders/{id}", web::get().to(team_list_folder))
        .route("/folders/{id}/info", web::get().to(team_get_folder_info))
        .route("/folders/{id}", web::patch().to(team_patch_folder))
        .route("/folders/{id}", web::delete().to(team_delete_folder))
        .route("/folders/{id}/lock", web::post().to(team_set_lock))
        .route("/folders/{id}/copy", web::post().to(team_copy_folder))
        .route("/folders/{id}/ancestors", web::get().to(team_get_ancestors))
        .service(files::team_routes())
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/folders",
    tag = "folders",
    operation_id = "create_folder",
    request_body = CreateFolderReq,
    responses(
        (status = 201, description = "Folder created", body = inline(ApiResponse<crate::services::workspace_models::FolderInfo>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
    ),
    security(("bearer" = [])),
)]
pub async fn create_folder(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    body: web::Json<CreateFolderReq>,
) -> Result<HttpResponse> {
    create_folder_response(
        &state,
        &claims,
        &req,
        WorkspaceStorageScope::Personal {
            user_id: claims.user_id,
        },
        &body,
    )
    .await
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/folders",
    tag = "folders",
    operation_id = "list_root",
    params(FolderListQuery),
    responses(
        (status = 200, description = "Root folder contents", body = inline(ApiResponse<folder_service::FolderContents>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
    ),
    security(("bearer" = [])),
)]
pub async fn list_root(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    query: web::Query<FolderListQuery>,
) -> Result<HttpResponse> {
    list_folder_response(
        &state,
        WorkspaceStorageScope::Personal {
            user_id: claims.user_id,
        },
        None,
        &query,
    )
    .await
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/folders/{id}",
    tag = "folders",
    operation_id = "list_folder",
    params(("id" = i64, Path, description = "Folder ID"), FolderListQuery),
    responses(
        (status = 200, description = "Folder contents", body = inline(ApiResponse<folder_service::FolderContents>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 404, description = "Folder not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn list_folder(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    path: web::Path<i64>,
    query: web::Query<FolderListQuery>,
) -> Result<HttpResponse> {
    list_folder_response(
        &state,
        WorkspaceStorageScope::Personal {
            user_id: claims.user_id,
        },
        Some(*path),
        &query,
    )
    .await
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/folders/{id}/info",
    tag = "folders",
    operation_id = "get_folder_info",
    params(("id" = i64, Path, description = "Folder ID")),
    responses(
        (status = 200, description = "Folder info", body = inline(ApiResponse<crate::services::workspace_models::FolderInfo>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 404, description = "Folder not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn get_folder_info(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    path: web::Path<i64>,
) -> Result<HttpResponse> {
    get_folder_info_response(
        &state,
        WorkspaceStorageScope::Personal {
            user_id: claims.user_id,
        },
        *path,
    )
    .await
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/folders/{id}/ancestors",
    tag = "folders",
    operation_id = "get_folder_ancestors",
    params(("id" = i64, Path, description = "Folder ID")),
    responses(
        (status = 200, description = "Folder ancestors", body = inline(ApiResponse<Vec<folder_service::FolderAncestorItem>>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 404, description = "Folder not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn get_ancestors(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    path: web::Path<i64>,
) -> Result<HttpResponse> {
    get_ancestors_response(
        &state,
        WorkspaceStorageScope::Personal {
            user_id: claims.user_id,
        },
        *path,
    )
    .await
}

#[api_docs_macros::path(
    delete,
    path = "/api/v1/folders/{id}",
    tag = "folders",
    operation_id = "delete_folder",
    params(("id" = i64, Path, description = "Folder ID")),
    responses(
        (status = 200, description = "Folder deleted"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 404, description = "Folder not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn delete_folder(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
) -> Result<HttpResponse> {
    delete_folder_response(
        &state,
        &claims,
        &req,
        WorkspaceStorageScope::Personal {
            user_id: claims.user_id,
        },
        *path,
    )
    .await
}

#[api_docs_macros::path(
    patch,
    path = "/api/v1/folders/{id}",
    tag = "folders",
    operation_id = "patch_folder",
    params(("id" = i64, Path, description = "Folder ID")),
    request_body = PatchFolderReq,
    responses(
        (status = 200, description = "Folder updated", body = inline(ApiResponse<crate::services::workspace_models::FolderInfo>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 404, description = "Folder not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn patch_folder(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
    body: web::Json<PatchFolderReq>,
) -> Result<HttpResponse> {
    patch_folder_response(
        &state,
        &claims,
        &req,
        WorkspaceStorageScope::Personal {
            user_id: claims.user_id,
        },
        *path,
        &body,
    )
    .await
}

// ── Lock ────────────────────────────────────────────────────────────

#[api_docs_macros::path(
    post,
    path = "/api/v1/folders/{id}/lock",
    tag = "folders",
    operation_id = "set_folder_lock",
    params(("id" = i64, Path, description = "Folder ID")),
    request_body = SetLockReq,
    responses(
        (status = 200, description = "Lock state updated", body = inline(ApiResponse<crate::services::workspace_models::FolderInfo>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 404, description = "Folder not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn set_lock(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
    body: web::Json<SetLockReq>,
) -> Result<HttpResponse> {
    set_lock_response(
        &state,
        &claims,
        &req,
        WorkspaceStorageScope::Personal {
            user_id: claims.user_id,
        },
        *path,
        body.locked,
    )
    .await
}

// ── Copy ───────────────────────────────────────────────────────────

#[api_docs_macros::path(
    post,
    path = "/api/v1/folders/{id}/copy",
    tag = "folders",
    operation_id = "copy_folder",
    params(("id" = i64, Path, description = "Source folder ID")),
    request_body = CopyFolderReq,
    responses(
        (status = 201, description = "Folder copied", body = inline(ApiResponse<crate::services::workspace_models::FolderInfo>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 404, description = "Folder not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn copy_folder(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
    body: web::Json<CopyFolderReq>,
) -> Result<HttpResponse> {
    copy_folder_response(
        &state,
        &claims,
        &req,
        WorkspaceStorageScope::Personal {
            user_id: claims.user_id,
        },
        *path,
        &body,
    )
    .await
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/teams/{team_id}/folders",
    tag = "teams",
    operation_id = "list_team_root",
    params(
        ("team_id" = i64, Path, description = "Team ID"),
        FolderListQuery
    ),
    responses(
        (status = 200, description = "Team root folder contents", body = inline(ApiResponse<folder_service::FolderContents>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer" = [])),
)]
pub(crate) async fn team_list_root(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    path: web::Path<i64>,
    query: web::Query<FolderListQuery>,
) -> Result<HttpResponse> {
    list_folder_response(&state, team_scope(*path, claims.user_id), None, &query).await
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/teams/{team_id}/folders",
    tag = "teams",
    operation_id = "create_team_folder",
    params(("team_id" = i64, Path, description = "Team ID")),
    request_body = CreateFolderReq,
    responses(
        (status = 201, description = "Team folder created", body = inline(ApiResponse<crate::services::workspace_models::FolderInfo>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer" = [])),
)]
pub(crate) async fn team_create_folder(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    path: web::Path<i64>,
    req: HttpRequest,
    body: web::Json<CreateFolderReq>,
) -> Result<HttpResponse> {
    create_folder_response(
        &state,
        &claims,
        &req,
        team_scope(*path, claims.user_id),
        &body,
    )
    .await
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/teams/{team_id}/folders/{id}",
    tag = "teams",
    operation_id = "list_team_folder",
    params(
        ("team_id" = i64, Path, description = "Team ID"),
        ("id" = i64, Path, description = "Folder ID"),
        FolderListQuery
    ),
    responses(
        (status = 200, description = "Team folder contents", body = inline(ApiResponse<folder_service::FolderContents>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Folder not found"),
    ),
    security(("bearer" = [])),
)]
pub(crate) async fn team_list_folder(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    path: web::Path<(i64, i64)>,
    query: web::Query<FolderListQuery>,
) -> Result<HttpResponse> {
    let (team_id, folder_id) = path.into_inner();
    list_folder_response(
        &state,
        team_scope(team_id, claims.user_id),
        Some(folder_id),
        &query,
    )
    .await
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/teams/{team_id}/folders/{id}/info",
    tag = "teams",
    operation_id = "get_team_folder_info",
    params(
        ("team_id" = i64, Path, description = "Team ID"),
        ("id" = i64, Path, description = "Folder ID")
    ),
    responses(
        (status = 200, description = "Team folder info", body = inline(ApiResponse<crate::services::workspace_models::FolderInfo>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Folder not found"),
    ),
    security(("bearer" = [])),
)]
pub(crate) async fn team_get_folder_info(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    path: web::Path<(i64, i64)>,
) -> Result<HttpResponse> {
    let (team_id, folder_id) = path.into_inner();
    get_folder_info_response(&state, team_scope(team_id, claims.user_id), folder_id).await
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/teams/{team_id}/folders/{id}/ancestors",
    tag = "teams",
    operation_id = "get_team_folder_ancestors",
    params(
        ("team_id" = i64, Path, description = "Team ID"),
        ("id" = i64, Path, description = "Folder ID")
    ),
    responses(
        (status = 200, description = "Team folder ancestors", body = inline(ApiResponse<Vec<folder_service::FolderAncestorItem>>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Folder not found"),
    ),
    security(("bearer" = [])),
)]
pub(crate) async fn team_get_ancestors(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    path: web::Path<(i64, i64)>,
) -> Result<HttpResponse> {
    let (team_id, folder_id) = path.into_inner();
    get_ancestors_response(&state, team_scope(team_id, claims.user_id), folder_id).await
}

#[api_docs_macros::path(
    delete,
    path = "/api/v1/teams/{team_id}/folders/{id}",
    tag = "teams",
    operation_id = "delete_team_folder",
    params(
        ("team_id" = i64, Path, description = "Team ID"),
        ("id" = i64, Path, description = "Folder ID")
    ),
    responses(
        (status = 200, description = "Team folder deleted"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Folder not found"),
    ),
    security(("bearer" = [])),
)]
pub(crate) async fn team_delete_folder(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<(i64, i64)>,
) -> Result<HttpResponse> {
    let (team_id, folder_id) = path.into_inner();
    delete_folder_response(
        &state,
        &claims,
        &req,
        team_scope(team_id, claims.user_id),
        folder_id,
    )
    .await
}

#[api_docs_macros::path(
    patch,
    path = "/api/v1/teams/{team_id}/folders/{id}",
    tag = "teams",
    operation_id = "patch_team_folder",
    params(
        ("team_id" = i64, Path, description = "Team ID"),
        ("id" = i64, Path, description = "Folder ID")
    ),
    request_body = PatchFolderReq,
    responses(
        (status = 200, description = "Team folder updated", body = inline(ApiResponse<crate::services::workspace_models::FolderInfo>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Folder not found"),
    ),
    security(("bearer" = [])),
)]
pub(crate) async fn team_patch_folder(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<(i64, i64)>,
    body: web::Json<PatchFolderReq>,
) -> Result<HttpResponse> {
    let (team_id, folder_id) = path.into_inner();
    patch_folder_response(
        &state,
        &claims,
        &req,
        team_scope(team_id, claims.user_id),
        folder_id,
        &body,
    )
    .await
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/teams/{team_id}/folders/{id}/copy",
    tag = "teams",
    operation_id = "copy_team_folder",
    params(
        ("team_id" = i64, Path, description = "Team ID"),
        ("id" = i64, Path, description = "Source folder ID")
    ),
    request_body = CopyFolderReq,
    responses(
        (status = 201, description = "Team folder copied", body = inline(ApiResponse<crate::services::workspace_models::FolderInfo>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Folder not found"),
    ),
    security(("bearer" = [])),
)]
pub(crate) async fn team_copy_folder(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<(i64, i64)>,
    body: web::Json<CopyFolderReq>,
) -> Result<HttpResponse> {
    let (team_id, folder_id) = path.into_inner();
    copy_folder_response(
        &state,
        &claims,
        &req,
        team_scope(team_id, claims.user_id),
        folder_id,
        &body,
    )
    .await
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/teams/{team_id}/folders/{id}/lock",
    tag = "teams",
    operation_id = "set_team_folder_lock",
    params(
        ("team_id" = i64, Path, description = "Team ID"),
        ("id" = i64, Path, description = "Folder ID")
    ),
    request_body = SetLockReq,
    responses(
        (status = 200, description = "Lock state updated", body = inline(ApiResponse<crate::services::workspace_models::FolderInfo>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Folder not found"),
    ),
    security(("bearer" = [])),
)]
pub(crate) async fn team_set_lock(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<(i64, i64)>,
    body: web::Json<SetLockReq>,
) -> Result<HttpResponse> {
    let (team_id, folder_id) = path.into_inner();
    set_lock_response(
        &state,
        &claims,
        &req,
        team_scope(team_id, claims.user_id),
        folder_id,
        body.locked,
    )
    .await
}

pub(crate) async fn create_folder_response(
    state: &PrimaryAppState,
    claims: &Claims,
    req: &HttpRequest,
    scope: WorkspaceStorageScope,
    body: &CreateFolderReq,
) -> Result<HttpResponse> {
    validate_request(body)?;
    let ctx = AuditContext::from_request(req, claims);
    let folder =
        folder_service::create_in_scope_with_audit(state, scope, &body.name, body.parent_id, &ctx)
            .await?;
    Ok(HttpResponse::Created().json(ApiResponse::ok(folder)))
}

pub(crate) async fn list_folder_response(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    parent_id: Option<i64>,
    query: &FolderListQuery,
) -> Result<HttpResponse> {
    let params = folder_service::FolderListParams::from(query);
    let contents = folder_service::list_in_scope(state, scope, parent_id, &params).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(contents)))
}

pub(crate) async fn get_ancestors_response(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    folder_id: i64,
) -> Result<HttpResponse> {
    let ancestors = folder_service::get_ancestors_in_scope(state, scope, folder_id).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(ancestors)))
}

pub(crate) async fn get_folder_info_response(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    folder_id: i64,
) -> Result<HttpResponse> {
    let folder =
        folder_service::get_info_with_storage_used_in_scope(state, scope, folder_id).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(folder)))
}

pub(crate) async fn delete_folder_response(
    state: &PrimaryAppState,
    claims: &Claims,
    req: &HttpRequest,
    scope: WorkspaceStorageScope,
    folder_id: i64,
) -> Result<HttpResponse> {
    let ctx = AuditContext::from_request(req, claims);
    folder_service::delete_in_scope_with_audit(state, scope, folder_id, &ctx).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::<()>::ok_empty()))
}

pub(crate) async fn patch_folder_response(
    state: &PrimaryAppState,
    claims: &Claims,
    req: &HttpRequest,
    scope: WorkspaceStorageScope,
    folder_id: i64,
    body: &PatchFolderReq,
) -> Result<HttpResponse> {
    validate_request(body)?;
    let ctx = AuditContext::from_request(req, claims);
    let folder = folder_service::update_in_scope_with_audit(
        state,
        scope,
        folder_id,
        body.name.clone(),
        body.parent_id,
        body.policy_id,
        &ctx,
    )
    .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(folder)))
}

pub(crate) async fn set_lock_response(
    state: &PrimaryAppState,
    claims: &Claims,
    req: &HttpRequest,
    scope: WorkspaceStorageScope,
    folder_id: i64,
    locked: bool,
) -> Result<HttpResponse> {
    let ctx = AuditContext::from_request(req, claims);
    let folder =
        folder_service::set_lock_in_scope_with_audit(state, scope, folder_id, locked, &ctx).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(folder)))
}

pub(crate) async fn copy_folder_response(
    state: &PrimaryAppState,
    claims: &Claims,
    req: &HttpRequest,
    scope: WorkspaceStorageScope,
    folder_id: i64,
    body: &CopyFolderReq,
) -> Result<HttpResponse> {
    let ctx = AuditContext::from_request(req, claims);
    let folder = folder_service::copy_folder_in_scope_with_audit(
        state,
        scope,
        folder_id,
        body.parent_id,
        &ctx,
    )
    .await?;
    Ok(HttpResponse::Created().json(ApiResponse::ok(folder)))
}
