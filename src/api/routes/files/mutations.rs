//! 文件 API 路由：`mutations`。

pub use crate::api::dto::files::*;
use crate::api::dto::validate_request;
use crate::api::response::ApiResponse;
use crate::api::routes::team_scope;
use crate::errors::Result;
use crate::runtime::PrimaryAppState;
use crate::services::{
    audit_service::{self, AuditContext},
    auth::local::Claims,
    files::file,
    workspace::storage::WorkspaceStorageScope,
};
use actix_web::{HttpRequest, HttpResponse, web};

#[api_docs_macros::path(
    post,
    path = "/api/v1/files/new",
    tag = "files",
    operation_id = "create_empty_file",
    request_body(content = CreateEmptyRequest, content_type = "application/json"),
    responses(
        (status = 201, description = "Empty file created", body = inline(ApiResponse<crate::services::workspace::models::FileInfo>)),
        (status = 400, description = "Invalid name"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
    ),
    security(("bearer" = [])),
)]
pub async fn create_empty(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    body: web::Json<CreateEmptyRequest>,
) -> Result<HttpResponse> {
    create_empty_response(
        state.get_ref(),
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
    post,
    path = "/api/v1/files/{id}/extract",
    tag = "files",
    operation_id = "extract_file_archive",
    params(("id" = i64, Path, description = "File ID")),
    request_body = ExtractArchiveRequest,
    responses(
        (status = 200, description = "Archive extract task created", body = inline(ApiResponse<crate::services::task_service::types::TaskInfo>)),
        (status = 400, description = "Unsupported archive format"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 404, description = "File not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn extract_archive(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
    body: web::Json<ExtractArchiveRequest>,
) -> Result<HttpResponse> {
    extract_archive_response(
        state.get_ref(),
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
    delete,
    path = "/api/v1/files/{id}",
    tag = "files",
    operation_id = "delete_file",
    params(("id" = i64, Path, description = "File ID")),
    responses(
        (status = 200, description = "File deleted"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 404, description = "File not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn delete_file(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
) -> Result<HttpResponse> {
    delete_file_response(
        state.get_ref(),
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
    path = "/api/v1/files/{id}",
    tag = "files",
    operation_id = "patch_file",
    params(("id" = i64, Path, description = "File ID")),
    request_body = PatchFileReq,
    responses(
        (status = 200, description = "File updated", body = inline(ApiResponse<crate::services::workspace::models::FileInfo>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 404, description = "File not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn patch_file(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
    body: web::Json<PatchFileReq>,
) -> Result<HttpResponse> {
    patch_file_response(
        state.get_ref(),
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
    put,
    path = "/api/v1/files/{id}/content",
    tag = "files",
    operation_id = "update_file_content",
    params(("id" = i64, Path, description = "File ID")),
    request_body(content = Vec<u8>, content_type = "application/octet-stream"),
    responses(
        (status = 200, description = "Content updated", body = inline(ApiResponse<crate::services::workspace::models::FileInfo>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 404, description = "File not found"),
        (status = 412, description = "Precondition failed (ETag mismatch)"),
        (status = 423, description = "File is locked by another user"),
    ),
    security(("bearer" = [])),
)]
pub async fn update_content(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    path: web::Path<i64>,
    req: HttpRequest,
    mut payload: web::Payload,
) -> Result<HttpResponse> {
    update_content_response(
        state.get_ref(),
        &claims,
        &req,
        WorkspaceStorageScope::Personal {
            user_id: claims.user_id,
        },
        *path,
        &mut payload,
    )
    .await
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/files/{id}/lock",
    tag = "files",
    operation_id = "set_file_lock",
    params(("id" = i64, Path, description = "File ID")),
    request_body = SetLockReq,
    responses(
        (status = 200, description = "Lock state updated", body = inline(ApiResponse<crate::services::workspace::models::FileInfo>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 404, description = "File not found"),
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
        state.get_ref(),
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

#[api_docs_macros::path(
    post,
    path = "/api/v1/files/{id}/copy",
    tag = "files",
    operation_id = "copy_file",
    params(("id" = i64, Path, description = "Source file ID")),
    request_body = CopyFileReq,
    responses(
        (status = 201, description = "File copied", body = inline(ApiResponse<crate::services::workspace::models::FileInfo>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 404, description = "File not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn copy_file(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
    body: web::Json<CopyFileReq>,
) -> Result<HttpResponse> {
    copy_file_response(
        state.get_ref(),
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
    post,
    path = "/api/v1/teams/{team_id}/files/new",
    tag = "teams",
    operation_id = "create_empty_team_file",
    params(("team_id" = i64, Path, description = "Team ID")),
    request_body = CreateEmptyRequest,
    responses(
        (status = 201, description = "Empty team file created", body = inline(ApiResponse<crate::services::workspace::models::FileInfo>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer" = [])),
)]
pub(crate) async fn team_create_empty(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
    body: web::Json<CreateEmptyRequest>,
) -> Result<HttpResponse> {
    create_empty_response(
        state.get_ref(),
        &claims,
        &req,
        team_scope(*path, claims.user_id),
        &body,
    )
    .await
}

#[api_docs_macros::path(
    put,
    path = "/api/v1/teams/{team_id}/files/{id}/content",
    tag = "teams",
    operation_id = "update_team_file_content",
    params(
        ("team_id" = i64, Path, description = "Team ID"),
        ("id" = i64, Path, description = "File ID")
    ),
    request_body(content = Vec<u8>, content_type = "application/octet-stream"),
    responses(
        (status = 200, description = "Content updated", body = inline(ApiResponse<crate::services::workspace::models::FileInfo>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "File not found"),
        (status = 412, description = "Precondition failed (ETag mismatch)"),
        (status = 423, description = "File is locked by another user"),
    ),
    security(("bearer" = [])),
)]
pub(crate) async fn team_update_content(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    path: web::Path<(i64, i64)>,
    req: HttpRequest,
    mut payload: web::Payload,
) -> Result<HttpResponse> {
    let (team_id, file_id) = path.into_inner();
    update_content_response(
        state.get_ref(),
        &claims,
        &req,
        team_scope(team_id, claims.user_id),
        file_id,
        &mut payload,
    )
    .await
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/teams/{team_id}/files/{id}/extract",
    tag = "teams",
    operation_id = "extract_team_file_archive",
    params(
        ("team_id" = i64, Path, description = "Team ID"),
        ("id" = i64, Path, description = "File ID")
    ),
    request_body = ExtractArchiveRequest,
    responses(
        (status = 200, description = "Team archive extract task created", body = inline(ApiResponse<crate::services::task_service::types::TaskInfo>)),
        (status = 400, description = "Unsupported archive format"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "File not found"),
    ),
    security(("bearer" = [])),
)]
pub(crate) async fn team_extract_archive(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<(i64, i64)>,
    body: web::Json<ExtractArchiveRequest>,
) -> Result<HttpResponse> {
    let (team_id, file_id) = path.into_inner();
    extract_archive_response(
        state.get_ref(),
        &claims,
        &req,
        team_scope(team_id, claims.user_id),
        file_id,
        &body,
    )
    .await
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/teams/{team_id}/files/{id}/lock",
    tag = "teams",
    operation_id = "set_team_file_lock",
    params(
        ("team_id" = i64, Path, description = "Team ID"),
        ("id" = i64, Path, description = "File ID")
    ),
    request_body = SetLockReq,
    responses(
        (status = 200, description = "Lock state updated", body = inline(ApiResponse<crate::services::workspace::models::FileInfo>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "File not found"),
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
    let (team_id, file_id) = path.into_inner();
    set_lock_response(
        state.get_ref(),
        &claims,
        &req,
        team_scope(team_id, claims.user_id),
        file_id,
        body.locked,
    )
    .await
}

#[api_docs_macros::path(
    patch,
    path = "/api/v1/teams/{team_id}/files/{id}",
    tag = "teams",
    operation_id = "patch_team_file",
    params(
        ("team_id" = i64, Path, description = "Team ID"),
        ("id" = i64, Path, description = "File ID")
    ),
    request_body = PatchFileReq,
    responses(
        (status = 200, description = "Team file updated", body = inline(ApiResponse<crate::services::workspace::models::FileInfo>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "File not found"),
    ),
    security(("bearer" = [])),
)]
pub(crate) async fn team_patch_file(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<(i64, i64)>,
    body: web::Json<PatchFileReq>,
) -> Result<HttpResponse> {
    let (team_id, file_id) = path.into_inner();
    patch_file_response(
        state.get_ref(),
        &claims,
        &req,
        team_scope(team_id, claims.user_id),
        file_id,
        &body,
    )
    .await
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/teams/{team_id}/files/{id}/copy",
    tag = "teams",
    operation_id = "copy_team_file",
    params(
        ("team_id" = i64, Path, description = "Team ID"),
        ("id" = i64, Path, description = "Source file ID")
    ),
    request_body = CopyFileReq,
    responses(
        (status = 201, description = "Team file copied", body = inline(ApiResponse<crate::services::workspace::models::FileInfo>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "File not found"),
    ),
    security(("bearer" = [])),
)]
pub(crate) async fn team_copy_file(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<(i64, i64)>,
    body: web::Json<CopyFileReq>,
) -> Result<HttpResponse> {
    let (team_id, file_id) = path.into_inner();
    copy_file_response(
        state.get_ref(),
        &claims,
        &req,
        team_scope(team_id, claims.user_id),
        file_id,
        &body,
    )
    .await
}

#[api_docs_macros::path(
    delete,
    path = "/api/v1/teams/{team_id}/files/{id}",
    tag = "teams",
    operation_id = "delete_team_file",
    params(
        ("team_id" = i64, Path, description = "Team ID"),
        ("id" = i64, Path, description = "File ID")
    ),
    responses(
        (status = 200, description = "Team file deleted"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "File not found"),
    ),
    security(("bearer" = [])),
)]
pub(crate) async fn team_delete_file(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<(i64, i64)>,
) -> Result<HttpResponse> {
    let (team_id, file_id) = path.into_inner();
    delete_file_response(
        state.get_ref(),
        &claims,
        &req,
        team_scope(team_id, claims.user_id),
        file_id,
    )
    .await
}

pub(crate) async fn create_empty_response(
    state: &PrimaryAppState,
    claims: &Claims,
    req: &HttpRequest,
    scope: WorkspaceStorageScope,
    body: &CreateEmptyRequest,
) -> Result<HttpResponse> {
    validate_request(body)?;
    let ctx = AuditContext::from_request(req, claims);
    let file =
        file::create_empty_in_scope_with_audit(state, scope, body.folder_id, &body.name, &ctx)
            .await?;
    Ok(HttpResponse::Created().json(ApiResponse::ok(file)))
}

pub(crate) async fn extract_archive_response(
    state: &PrimaryAppState,
    claims: &Claims,
    req: &HttpRequest,
    scope: WorkspaceStorageScope,
    file_id: i64,
    body: &ExtractArchiveRequest,
) -> Result<HttpResponse> {
    let task = crate::services::task_service::archive::create_archive_extract_task_in_scope(
        state,
        scope,
        file_id,
        crate::services::task_service::types::CreateArchiveExtractTaskParams {
            target_folder_id: body.target_folder_id,
            output_folder_name: body.output_folder_name.clone(),
            filename_encoding: body.filename_encoding,
        },
    )
    .await?;
    let ctx = AuditContext::from_request(req, claims);
    let file_ids = [file_id];
    audit_service::log_with_details(
        state,
        &ctx,
        audit_service::AuditAction::ArchiveExtract,
        crate::services::audit_service::AuditEntityType::Task,
        Some(task.id),
        Some(&task.display_name),
        || {
            audit_service::details(audit_service::ArchiveSelectionAuditDetails {
                file_ids: &file_ids,
                folder_ids: &[],
                archive_name: body.output_folder_name.as_deref(),
                target_folder_id: body.target_folder_id,
            })
        },
    )
    .await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(task)))
}

pub(crate) async fn delete_file_response(
    state: &PrimaryAppState,
    claims: &Claims,
    req: &HttpRequest,
    scope: WorkspaceStorageScope,
    file_id: i64,
) -> Result<HttpResponse> {
    let ctx = AuditContext::from_request(req, claims);
    file::delete_in_scope_with_audit(state, scope, file_id, &ctx).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::<()>::ok_empty()))
}

pub(crate) async fn patch_file_response(
    state: &PrimaryAppState,
    claims: &Claims,
    req: &HttpRequest,
    scope: WorkspaceStorageScope,
    file_id: i64,
    body: &PatchFileReq,
) -> Result<HttpResponse> {
    validate_request(body)?;
    let ctx = AuditContext::from_request(req, claims);
    let file = file::update_in_scope_with_audit(
        state,
        scope,
        file_id,
        body.name.clone(),
        body.folder_id,
        &ctx,
    )
    .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(file)))
}

pub(crate) async fn update_content_response(
    state: &PrimaryAppState,
    claims: &Claims,
    req: &HttpRequest,
    scope: WorkspaceStorageScope,
    file_id: i64,
    payload: &mut web::Payload,
) -> Result<HttpResponse> {
    let if_match = req
        .headers()
        .get("If-Match")
        .and_then(|value| value.to_str().ok());
    let declared_size = request_content_length(req);
    let ctx = AuditContext::from_request(req, claims);
    let (file, new_hash) = file::update_content_stream_in_scope_with_audit(
        state,
        scope,
        file_id,
        payload,
        declared_size,
        if_match,
        &ctx,
    )
    .await?;

    Ok(HttpResponse::Ok()
        .insert_header(("ETag", format!("\"{new_hash}\"")))
        .json(ApiResponse::ok(file)))
}

fn request_content_length(req: &HttpRequest) -> Option<i64> {
    req.headers()
        .get(actix_web::http::header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .and_then(|value| crate::utils::numbers::u64_to_i64(value, "content length").ok())
}

pub(crate) async fn set_lock_response(
    state: &PrimaryAppState,
    claims: &Claims,
    req: &HttpRequest,
    scope: WorkspaceStorageScope,
    file_id: i64,
    locked: bool,
) -> Result<HttpResponse> {
    let ctx = AuditContext::from_request(req, claims);
    let file = file::set_lock_in_scope_with_audit(state, scope, file_id, locked, &ctx).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(file)))
}

pub(crate) async fn copy_file_response(
    state: &PrimaryAppState,
    claims: &Claims,
    req: &HttpRequest,
    scope: WorkspaceStorageScope,
    file_id: i64,
    body: &CopyFileReq,
) -> Result<HttpResponse> {
    let ctx = AuditContext::from_request(req, claims);
    let file =
        file::copy_file_in_scope_with_audit(state, scope, file_id, body.folder_id, &ctx).await?;
    Ok(HttpResponse::Created().json(ApiResponse::ok(file)))
}
