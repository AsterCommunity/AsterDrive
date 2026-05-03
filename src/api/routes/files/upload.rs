//! 文件 API 路由：`upload`。

pub use crate::api::dto::files::*;
use crate::api::dto::validate_request;
use crate::api::response::ApiResponse;
use crate::api::routes::team_scope;
use crate::errors::Result;
use crate::runtime::PrimaryAppState;
use crate::services::{
    audit_service::AuditContext, auth_service::Claims, upload_service,
    workspace_storage_service::WorkspaceStorageScope,
};
use actix_web::{HttpRequest, HttpResponse, web};

#[derive(Clone, Copy)]
pub(crate) struct UploadResponseParams<'a> {
    pub scope: WorkspaceStorageScope,
    pub folder_id: Option<i64>,
    pub relative_path: Option<&'a str>,
    pub declared_size: Option<i64>,
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/files/upload",
    tag = "files",
    operation_id = "upload_file",
    params(FileQuery),
    request_body(content = String, content_type = "multipart/form-data", description = "File to upload"),
    responses(
        (status = 201, description = "File uploaded", body = inline(ApiResponse<crate::services::workspace_models::FileInfo>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
    ),
    security(("bearer" = [])),
)]
pub async fn upload(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    query: web::Query<FileQuery>,
    mut payload: actix_multipart::Multipart,
) -> Result<HttpResponse> {
    upload_response(
        &state,
        &claims,
        &req,
        &mut payload,
        UploadResponseParams {
            scope: WorkspaceStorageScope::Personal {
                user_id: claims.user_id,
            },
            folder_id: query.folder_id,
            relative_path: query.relative_path.as_deref(),
            declared_size: query.declared_size,
        },
    )
    .await
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/files/upload/init",
    tag = "files",
    operation_id = "init_chunked_upload",
    request_body = InitUploadReq,
    responses(
        (status = 201, description = "Upload session created", body = inline(ApiResponse<upload_service::InitUploadResponse>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
    ),
    security(("bearer" = [])),
)]
pub async fn init_chunked_upload(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    body: web::Json<InitUploadReq>,
) -> Result<HttpResponse> {
    validate_request(&*body)?;
    let resp = upload_service::init_upload(
        &state,
        claims.user_id,
        &body.filename,
        body.total_size,
        body.folder_id,
        body.relative_path.as_deref(),
    )
    .await?;
    Ok(HttpResponse::Created().json(ApiResponse::ok(resp)))
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/files/upload/sessions",
    tag = "files",
    operation_id = "list_recoverable_upload_sessions",
    responses(
        (status = 200, description = "Recoverable upload sessions", body = inline(ApiResponse<Vec<upload_service::RecoverableUploadSessionResponse>>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
    ),
    security(("bearer" = [])),
)]
pub async fn list_recoverable_upload_sessions(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
) -> Result<HttpResponse> {
    let resp = upload_service::list_recoverable_sessions(&state, claims.user_id).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(resp)))
}

#[api_docs_macros::path(
    put,
    path = "/api/v1/files/upload/{upload_id}/{chunk_number}",
    tag = "files",
    operation_id = "upload_chunk",
    params(
        ("upload_id" = String, Path, description = "Upload session ID"),
        ("chunk_number" = i32, Path, description = "Chunk number (0-indexed)"),
    ),
    request_body(content = Vec<u8>, content_type = "application/octet-stream"),
    responses(
        (status = 200, description = "Chunk uploaded", body = inline(ApiResponse<upload_service::ChunkUploadResponse>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 404, description = "Session not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn upload_chunk(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    path: web::Path<ChunkPath>,
    body: web::Bytes,
) -> Result<HttpResponse> {
    let resp = upload_service::upload_chunk(
        &state,
        &path.upload_id,
        path.chunk_number,
        claims.user_id,
        &body,
    )
    .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(resp)))
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/files/upload/{upload_id}/complete",
    tag = "files",
    operation_id = "complete_chunked_upload",
    params(("upload_id" = String, Path, description = "Upload session ID")),
    request_body(content = CompleteUploadReq, description = "Multipart completion data (optional, only for presigned_multipart mode)", content_type = "application/json"),
    responses(
        (status = 201, description = "File created", body = inline(ApiResponse<crate::services::workspace_models::FileInfo>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 404, description = "Session not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn complete_upload(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<UploadIdPath>,
    body: Option<web::Json<CompleteUploadReq>>,
) -> Result<HttpResponse> {
    let parts = body
        .and_then(|payload| payload.into_inner().parts)
        .map(|parts| {
            parts
                .into_iter()
                .map(|part| (part.part_number, part.etag))
                .collect()
        });
    let ctx = AuditContext::from_request(&req, &claims);
    let file = upload_service::complete_upload_with_audit(
        &state,
        &path.upload_id,
        claims.user_id,
        parts,
        &ctx,
    )
    .await?;
    Ok(HttpResponse::Created().json(ApiResponse::ok(file)))
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/files/upload/{upload_id}",
    tag = "files",
    operation_id = "get_upload_progress",
    params(("upload_id" = String, Path, description = "Upload session ID")),
    responses(
        (status = 200, description = "Upload progress", body = ApiResponse<upload_service::UploadProgressResponse>),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 404, description = "Session not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn get_upload_progress(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    path: web::Path<UploadIdPath>,
) -> Result<HttpResponse> {
    let resp = upload_service::get_progress(&state, &path.upload_id, claims.user_id).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(resp)))
}

#[api_docs_macros::path(
    delete,
    path = "/api/v1/files/upload/{upload_id}",
    tag = "files",
    operation_id = "cancel_upload",
    params(("upload_id" = String, Path, description = "Upload session ID")),
    responses(
        (status = 200, description = "Upload cancelled"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 404, description = "Session not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn cancel_upload(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    path: web::Path<UploadIdPath>,
) -> Result<HttpResponse> {
    upload_service::cancel_upload(&state, &path.upload_id, claims.user_id).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::<()>::ok_empty()))
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/files/upload/{upload_id}/presign-parts",
    tag = "files",
    operation_id = "presign_upload_parts",
    params(("upload_id" = String, Path, description = "Upload session ID")),
    request_body = PresignPartsReq,
    responses(
        (status = 200, description = "Presigned URLs for each part", body = inline(ApiResponse<std::collections::HashMap<i32, String>>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 404, description = "Session not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn presign_parts(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    path: web::Path<UploadIdPath>,
    body: web::Json<PresignPartsReq>,
) -> Result<HttpResponse> {
    let urls = upload_service::presign_parts(
        &state,
        &path.upload_id,
        claims.user_id,
        body.into_inner().part_numbers,
    )
    .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(urls)))
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/teams/{team_id}/files/upload",
    tag = "teams",
    operation_id = "upload_team_file",
    params(
        ("team_id" = i64, Path, description = "Team ID"),
        FileQuery
    ),
    request_body(content = String, content_type = "multipart/form-data", description = "File to upload"),
    responses(
        (status = 201, description = "Team file uploaded", body = inline(ApiResponse<crate::services::workspace_models::FileInfo>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer" = [])),
)]
pub(crate) async fn team_upload(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
    query: web::Query<FileQuery>,
    mut payload: actix_multipart::Multipart,
) -> Result<HttpResponse> {
    upload_response(
        &state,
        &claims,
        &req,
        &mut payload,
        UploadResponseParams {
            scope: team_scope(*path, claims.user_id),
            folder_id: query.folder_id,
            relative_path: query.relative_path.as_deref(),
            declared_size: query.declared_size,
        },
    )
    .await
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/teams/{team_id}/files/upload/init",
    tag = "teams",
    operation_id = "init_team_chunked_upload",
    params(("team_id" = i64, Path, description = "Team ID")),
    request_body = InitUploadReq,
    responses(
        (status = 201, description = "Team upload session created", body = inline(ApiResponse<upload_service::InitUploadResponse>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer" = [])),
)]
pub(crate) async fn team_init_chunked_upload(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    path: web::Path<i64>,
    body: web::Json<InitUploadReq>,
) -> Result<HttpResponse> {
    validate_request(&*body)?;
    let resp = upload_service::init_upload_for_team(
        &state,
        *path,
        claims.user_id,
        &body.filename,
        body.total_size,
        body.folder_id,
        body.relative_path.as_deref(),
    )
    .await?;
    Ok(HttpResponse::Created().json(ApiResponse::ok(resp)))
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/teams/{team_id}/files/upload/sessions",
    tag = "teams",
    operation_id = "list_team_recoverable_upload_sessions",
    params(("team_id" = i64, Path, description = "Team ID")),
    responses(
        (status = 200, description = "Team recoverable upload sessions", body = inline(ApiResponse<Vec<upload_service::RecoverableUploadSessionResponse>>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer" = [])),
)]
pub(crate) async fn team_list_recoverable_upload_sessions(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    path: web::Path<i64>,
) -> Result<HttpResponse> {
    let resp =
        upload_service::list_recoverable_sessions_for_team(&state, *path, claims.user_id).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(resp)))
}

#[api_docs_macros::path(
    put,
    path = "/api/v1/teams/{team_id}/files/upload/{upload_id}/{chunk_number}",
    tag = "teams",
    operation_id = "upload_team_chunk",
    params(
        ("team_id" = i64, Path, description = "Team ID"),
        ("upload_id" = String, Path, description = "Upload session ID"),
        ("chunk_number" = i32, Path, description = "Chunk number (0-indexed)")
    ),
    request_body(content = Vec<u8>, content_type = "application/octet-stream"),
    responses(
        (status = 200, description = "Chunk uploaded", body = inline(ApiResponse<upload_service::ChunkUploadResponse>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Session not found"),
    ),
    security(("bearer" = [])),
)]
pub(crate) async fn team_upload_chunk(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    path: web::Path<(i64, String, i32)>,
    body: web::Bytes,
) -> Result<HttpResponse> {
    let (team_id, upload_id, chunk_number) = path.into_inner();
    let resp = upload_service::upload_chunk_for_team(
        &state,
        team_id,
        &upload_id,
        chunk_number,
        claims.user_id,
        &body,
    )
    .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(resp)))
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/teams/{team_id}/files/upload/{upload_id}/complete",
    tag = "teams",
    operation_id = "complete_team_chunked_upload",
    params(
        ("team_id" = i64, Path, description = "Team ID"),
        ("upload_id" = String, Path, description = "Upload session ID")
    ),
    request_body(content = CompleteUploadReq, description = "Multipart completion data (optional, only for presigned_multipart mode)", content_type = "application/json"),
    responses(
        (status = 201, description = "Team file created", body = inline(ApiResponse<crate::services::workspace_models::FileInfo>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Session not found"),
    ),
    security(("bearer" = [])),
)]
pub(crate) async fn team_complete_upload(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<(i64, String)>,
    body: Option<web::Json<CompleteUploadReq>>,
) -> Result<HttpResponse> {
    let (team_id, upload_id) = path.into_inner();
    let parts = body
        .and_then(|payload| payload.into_inner().parts)
        .map(|parts| {
            parts
                .into_iter()
                .map(|part| (part.part_number, part.etag))
                .collect()
        });
    let ctx = AuditContext::from_request(&req, &claims);
    let file = upload_service::complete_upload_for_team_with_audit(
        &state,
        team_id,
        &upload_id,
        claims.user_id,
        parts,
        &ctx,
    )
    .await?;
    Ok(HttpResponse::Created().json(ApiResponse::ok(file)))
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/teams/{team_id}/files/upload/{upload_id}",
    tag = "teams",
    operation_id = "get_team_upload_progress",
    params(
        ("team_id" = i64, Path, description = "Team ID"),
        ("upload_id" = String, Path, description = "Upload session ID")
    ),
    responses(
        (status = 200, description = "Upload progress", body = inline(ApiResponse<upload_service::UploadProgressResponse>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Session not found"),
    ),
    security(("bearer" = [])),
)]
pub(crate) async fn team_get_upload_progress(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    path: web::Path<(i64, String)>,
) -> Result<HttpResponse> {
    let (team_id, upload_id) = path.into_inner();
    let resp =
        upload_service::get_progress_for_team(&state, team_id, &upload_id, claims.user_id).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(resp)))
}

#[api_docs_macros::path(
    delete,
    path = "/api/v1/teams/{team_id}/files/upload/{upload_id}",
    tag = "teams",
    operation_id = "cancel_team_upload",
    params(
        ("team_id" = i64, Path, description = "Team ID"),
        ("upload_id" = String, Path, description = "Upload session ID")
    ),
    responses(
        (status = 200, description = "Upload cancelled"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Session not found"),
    ),
    security(("bearer" = [])),
)]
pub(crate) async fn team_cancel_upload(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    path: web::Path<(i64, String)>,
) -> Result<HttpResponse> {
    let (team_id, upload_id) = path.into_inner();
    upload_service::cancel_upload_for_team(&state, team_id, &upload_id, claims.user_id).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::<()>::ok_empty()))
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/teams/{team_id}/files/upload/{upload_id}/presign-parts",
    tag = "teams",
    operation_id = "presign_team_upload_parts",
    params(
        ("team_id" = i64, Path, description = "Team ID"),
        ("upload_id" = String, Path, description = "Upload session ID")
    ),
    request_body = PresignPartsReq,
    responses(
        (status = 200, description = "Presigned URLs", body = inline(ApiResponse<std::collections::HashMap<i32, String>>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Session not found"),
    ),
    security(("bearer" = [])),
)]
pub(crate) async fn team_presign_parts(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    path: web::Path<(i64, String)>,
    body: web::Json<PresignPartsReq>,
) -> Result<HttpResponse> {
    let (team_id, upload_id) = path.into_inner();
    let urls = upload_service::presign_parts_for_team(
        &state,
        team_id,
        &upload_id,
        claims.user_id,
        body.into_inner().part_numbers,
    )
    .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(urls)))
}

pub(crate) async fn upload_response(
    state: &PrimaryAppState,
    claims: &Claims,
    req: &HttpRequest,
    payload: &mut actix_multipart::Multipart,
    params: UploadResponseParams<'_>,
) -> Result<HttpResponse> {
    let ctx = AuditContext::from_request(req, claims);
    let file = upload_service::upload_in_scope_with_audit(
        state,
        payload,
        upload_service::UploadInScopeParams {
            scope: params.scope,
            folder_id: params.folder_id,
            relative_path: params.relative_path,
            declared_size: params.declared_size,
        },
        &ctx,
    )
    .await?;
    Ok(HttpResponse::Created().json(ApiResponse::ok(file)))
}
