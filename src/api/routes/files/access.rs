//! 文件 API 路由：`access`。

use crate::api::dto::files::{
    ArchivePreviewQuery, DownloadQuery, FileResourceHandleRequest, OpenWopiRequest,
};
use crate::api::response::ApiResponse;
use crate::api::routes::team_scope;
use crate::errors::Result;
use crate::runtime::PrimaryAppState;
use crate::services::{
    auth::local::Claims,
    files::archive::preview,
    files::{direct_link, file, preview_link},
    media::metadata,
    media::processing,
    ops::audit::{self, AuditContext},
    preview::wopi,
    workspace::storage::WorkspaceStorageScope,
};
use actix_web::http::header;
use actix_web::{HttpRequest, HttpResponse, web};

fn request_origin_parts(req: &HttpRequest) -> (String, String) {
    let conn = req.connection_info();
    (conn.scheme().to_string(), conn.host().to_string())
}

pub(crate) fn download_disposition_from_query(
    query: &DownloadQuery,
) -> Result<file::DownloadDisposition> {
    match query.disposition.as_deref() {
        None | Some("") | Some("attachment") => Ok(file::DownloadDisposition::Attachment),
        Some("inline") => Ok(file::DownloadDisposition::Inline),
        Some(value) => Err(crate::errors::AsterError::validation_error(format!(
            "unsupported download disposition '{value}', expected inline or attachment"
        ))),
    }
}

#[aster_forge_api_docs_macros::path(
    get,
    path = "/api/v1/files/{id}",
    tag = "files",
    operation_id = "get_file",
    params(("id" = i64, Path, description = "File ID")),
    responses(
        (status = 200, description = "File info", body = inline(ApiResponse<crate::services::workspace::models::FileInfo>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 404, description = "File not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn get_file(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    path: web::Path<i64>,
) -> Result<HttpResponse> {
    get_file_response(
        state.get_ref(),
        WorkspaceStorageScope::Personal {
            user_id: claims.user_id,
        },
        *path,
    )
    .await
}

#[aster_forge_api_docs_macros::path(
    get,
    path = "/api/v1/files/{id}/archive-preview",
    tag = "files",
    operation_id = "get_file_archive_preview",
    params(("id" = i64, Path, description = "File ID"), ArchivePreviewQuery),
    responses(
        (status = 200, description = "Archive preview manifest", body = inline(ApiResponse<preview::ArchivePreviewManifest>)),
        (status = 202, description = "Archive preview generation has been queued"),
        (status = 304, description = "Archive preview not modified"),
        (status = 400, description = "Not a supported archive or archive rejected by limits"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Archive preview disabled"),
        (status = 404, description = "File not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn get_archive_preview(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
    query: web::Query<ArchivePreviewQuery>,
) -> Result<HttpResponse> {
    archive_preview_response(
        state.get_ref(),
        &req,
        WorkspaceStorageScope::Personal {
            user_id: claims.user_id,
        },
        *path,
        query.filename_encoding,
    )
    .await
}

#[aster_forge_api_docs_macros::path(
    get,
    path = "/api/v1/files/{id}/direct-link",
    tag = "files",
    operation_id = "get_file_direct_link",
    params(("id" = i64, Path, description = "File ID")),
    responses(
        (status = 200, description = "Direct link token", body = inline(ApiResponse<crate::services::files::direct_link::DirectLinkTokenInfo>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 404, description = "File not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn get_direct_link(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
) -> Result<HttpResponse> {
    direct_link_response(
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

#[aster_forge_api_docs_macros::path(
    post,
    path = "/api/v1/files/{id}/preview-link",
    tag = "files",
    operation_id = "create_file_preview_link",
    params(("id" = i64, Path, description = "File ID")),
    responses(
        (status = 200, description = "Preview link", body = inline(ApiResponse<crate::services::files::preview_link::PreviewLinkInfo>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 404, description = "File not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn get_preview_link(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
) -> Result<HttpResponse> {
    preview_link_response(
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

#[aster_forge_api_docs_macros::path(
    post,
    path = "/api/v1/files/{id}/resource-handle",
    tag = "files",
    operation_id = "resolve_file_resource_handle",
    params(("id" = i64, Path, description = "File ID")),
    request_body = FileResourceHandleRequest,
    responses(
        (status = 200, description = "Resolved file resource handle", body = inline(ApiResponse<crate::api::dto::files::FileResourceHandle>)),
        (status = 400, description = "Invalid resource handle request"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 404, description = "File not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn resolve_resource_handle(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    path: web::Path<i64>,
    body: web::Json<FileResourceHandleRequest>,
) -> Result<HttpResponse> {
    resource_handle_response(
        state.get_ref(),
        WorkspaceStorageScope::Personal {
            user_id: claims.user_id,
        },
        *path,
        file::FileResourcePathSet {
            download: format!("/files/{}/download", *path),
            image_preview: format!("/files/{}/image-preview", *path),
            thumbnail: format!("/files/{}/thumbnail", *path),
        },
        &body,
    )
    .await
}

#[aster_forge_api_docs_macros::path(
    post,
    path = "/api/v1/files/{id}/wopi/open",
    tag = "files",
    operation_id = "open_file_with_wopi",
    params(("id" = i64, Path, description = "File ID")),
    request_body = OpenWopiRequest,
    responses(
        (status = 200, description = "WOPI launch session", body = inline(ApiResponse<wopi::WopiLaunchSession>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 404, description = "File not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn open_wopi(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
    body: web::Json<OpenWopiRequest>,
) -> Result<HttpResponse> {
    open_wopi_response(
        state.get_ref(),
        &claims,
        &req,
        WorkspaceStorageScope::Personal {
            user_id: claims.user_id,
        },
        *path,
        &body.app_key,
    )
    .await
}

#[aster_forge_api_docs_macros::path(
    get,
    path = "/api/v1/files/{id}/download",
    tag = "files",
    operation_id = "download_file",
    params(("id" = i64, Path, description = "File ID"), DownloadQuery),
    responses(
        (status = 200, description = "File content"),
        (status = 206, description = "Partial file content"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 404, description = "File not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn download(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
    query: web::Query<DownloadQuery>,
) -> Result<HttpResponse> {
    download_response(
        state.get_ref(),
        &claims,
        &req,
        WorkspaceStorageScope::Personal {
            user_id: claims.user_id,
        },
        *path,
        download_disposition_from_query(&query)?,
    )
    .await
}

#[aster_forge_api_docs_macros::path(
    get,
    path = "/api/v1/files/{id}/thumbnail",
    tag = "files",
    operation_id = "get_thumbnail",
    params(("id" = i64, Path, description = "File ID")),
    responses(
        (status = 200, description = "Thumbnail image (WebP)"),
        (status = 304, description = "Thumbnail not modified"),
        (status = 202, description = "Thumbnail generation in progress"),
        (status = 400, description = "Thumbnail not supported for this file type"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 412, description = "Storage backend is disabled or not ready"),
        (status = 404, description = "File not found or thumbnail unavailable"),
        (status = 500, description = "Unexpected thumbnail generation failure"),
    ),
    security(("bearer" = [])),
)]
pub async fn get_thumbnail(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
) -> Result<HttpResponse> {
    get_thumbnail_response(
        state.get_ref(),
        &req,
        WorkspaceStorageScope::Personal {
            user_id: claims.user_id,
        },
        *path,
    )
    .await
}

#[aster_forge_api_docs_macros::path(
    get,
    path = "/api/v1/files/{id}/media-metadata",
    tag = "files",
    operation_id = "get_file_media_metadata",
    params(("id" = i64, Path, description = "File ID")),
    responses(
        (status = 200, description = "Blob media metadata", body = inline(ApiResponse<metadata::MediaMetadataInfo>)),
        (status = 202, description = "Media metadata extraction in progress"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 404, description = "File not found"),
    ),
    security(("bearer" = [])),
)]
pub async fn get_media_metadata(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    path: web::Path<i64>,
) -> Result<HttpResponse> {
    get_media_metadata_response(
        state.get_ref(),
        WorkspaceStorageScope::Personal {
            user_id: claims.user_id,
        },
        *path,
    )
    .await
}

#[aster_forge_api_docs_macros::path(
    get,
    path = "/api/v1/files/{id}/image-preview",
    tag = "files",
    operation_id = "get_file_image_preview",
    params(("id" = i64, Path, description = "File ID")),
    responses(
        (status = 200, description = "Image preview (WebP)"),
        (status = 202, description = "Image preview is being generated"),
        (status = 304, description = "Image preview not modified"),
        (status = 400, description = "Image preview not supported for this file type"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 412, description = "Storage backend is disabled or not ready"),
        (status = 404, description = "File not found"),
        (status = 500, description = "Unexpected image preview generation failure"),
    ),
    security(("bearer" = [])),
)]
pub async fn get_image_preview(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<i64>,
) -> Result<HttpResponse> {
    get_image_preview_response(
        state.get_ref(),
        &req,
        WorkspaceStorageScope::Personal {
            user_id: claims.user_id,
        },
        *path,
    )
    .await
}

#[aster_forge_api_docs_macros::path(
    get,
    path = "/api/v1/teams/{team_id}/files/{id}",
    tag = "teams",
    operation_id = "get_team_file",
    params(
        ("team_id" = i64, Path, description = "Team ID"),
        ("id" = i64, Path, description = "File ID")
    ),
    responses(
        (status = 200, description = "Team file info", body = inline(ApiResponse<crate::services::workspace::models::FileInfo>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "File not found"),
    ),
    security(("bearer" = [])),
)]
pub(crate) async fn team_get_file(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    path: web::Path<(i64, i64)>,
) -> Result<HttpResponse> {
    let (team_id, file_id) = path.into_inner();
    get_file_response(
        state.get_ref(),
        team_scope(team_id, claims.user_id),
        file_id,
    )
    .await
}

#[aster_forge_api_docs_macros::path(
    get,
    path = "/api/v1/teams/{team_id}/files/{id}/archive-preview",
    tag = "teams",
    operation_id = "get_team_file_archive_preview",
    params(
        ("team_id" = i64, Path, description = "Team ID"),
        ("id" = i64, Path, description = "File ID"),
        ArchivePreviewQuery
    ),
    responses(
        (status = 200, description = "Team archive preview manifest", body = inline(ApiResponse<preview::ArchivePreviewManifest>)),
        (status = 202, description = "Archive preview generation has been queued"),
        (status = 304, description = "Archive preview not modified"),
        (status = 400, description = "Not a supported archive or archive rejected by limits"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden or archive preview disabled"),
        (status = 404, description = "File not found"),
    ),
    security(("bearer" = [])),
)]
pub(crate) async fn team_get_archive_preview(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<(i64, i64)>,
    query: web::Query<ArchivePreviewQuery>,
) -> Result<HttpResponse> {
    let (team_id, file_id) = path.into_inner();
    archive_preview_response(
        state.get_ref(),
        &req,
        team_scope(team_id, claims.user_id),
        file_id,
        query.filename_encoding,
    )
    .await
}

#[aster_forge_api_docs_macros::path(
    get,
    path = "/api/v1/teams/{team_id}/files/{id}/direct-link",
    tag = "teams",
    operation_id = "get_team_file_direct_link",
    params(
        ("team_id" = i64, Path, description = "Team ID"),
        ("id" = i64, Path, description = "File ID")
    ),
    responses(
        (status = 200, description = "Team file direct link token", body = inline(ApiResponse<crate::services::files::direct_link::DirectLinkTokenInfo>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "File not found"),
    ),
    security(("bearer" = [])),
)]
pub(crate) async fn team_get_direct_link(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<(i64, i64)>,
) -> Result<HttpResponse> {
    let (team_id, file_id) = path.into_inner();
    direct_link_response(
        state.get_ref(),
        &claims,
        &req,
        team_scope(team_id, claims.user_id),
        file_id,
    )
    .await
}

#[aster_forge_api_docs_macros::path(
    post,
    path = "/api/v1/teams/{team_id}/files/{id}/preview-link",
    tag = "teams",
    operation_id = "create_team_file_preview_link",
    params(
        ("team_id" = i64, Path, description = "Team ID"),
        ("id" = i64, Path, description = "File ID")
    ),
    responses(
        (status = 200, description = "Team file preview link", body = inline(ApiResponse<crate::services::files::preview_link::PreviewLinkInfo>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "File not found"),
    ),
    security(("bearer" = [])),
)]
pub(crate) async fn team_get_preview_link(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<(i64, i64)>,
) -> Result<HttpResponse> {
    let (team_id, file_id) = path.into_inner();
    preview_link_response(
        state.get_ref(),
        &claims,
        &req,
        team_scope(team_id, claims.user_id),
        file_id,
    )
    .await
}

#[aster_forge_api_docs_macros::path(
    post,
    path = "/api/v1/teams/{team_id}/files/{id}/resource-handle",
    tag = "teams",
    operation_id = "resolve_team_file_resource_handle",
    params(
        ("team_id" = i64, Path, description = "Team ID"),
        ("id" = i64, Path, description = "File ID")
    ),
    request_body = FileResourceHandleRequest,
    responses(
        (status = 200, description = "Resolved team file resource handle", body = inline(ApiResponse<crate::api::dto::files::FileResourceHandle>)),
        (status = 400, description = "Invalid resource handle request"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "File not found"),
    ),
    security(("bearer" = [])),
)]
pub(crate) async fn team_resolve_resource_handle(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    path: web::Path<(i64, i64)>,
    body: web::Json<FileResourceHandleRequest>,
) -> Result<HttpResponse> {
    let (team_id, file_id) = path.into_inner();
    resource_handle_response(
        state.get_ref(),
        team_scope(team_id, claims.user_id),
        file_id,
        file::FileResourcePathSet {
            download: format!("/teams/{team_id}/files/{file_id}/download"),
            image_preview: format!("/teams/{team_id}/files/{file_id}/image-preview"),
            thumbnail: format!("/teams/{team_id}/files/{file_id}/thumbnail"),
        },
        &body,
    )
    .await
}

#[aster_forge_api_docs_macros::path(
    post,
    path = "/api/v1/teams/{team_id}/files/{id}/wopi/open",
    tag = "teams",
    operation_id = "open_team_file_with_wopi",
    params(
        ("team_id" = i64, Path, description = "Team ID"),
        ("id" = i64, Path, description = "File ID")
    ),
    request_body = OpenWopiRequest,
    responses(
        (status = 200, description = "Team WOPI launch session", body = inline(ApiResponse<wopi::WopiLaunchSession>)),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "File not found"),
    ),
    security(("bearer" = [])),
)]
pub(crate) async fn team_open_wopi(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<(i64, i64)>,
    body: web::Json<OpenWopiRequest>,
) -> Result<HttpResponse> {
    let (team_id, file_id) = path.into_inner();
    open_wopi_response(
        state.get_ref(),
        &claims,
        &req,
        team_scope(team_id, claims.user_id),
        file_id,
        &body.app_key,
    )
    .await
}

#[aster_forge_api_docs_macros::path(
    get,
    path = "/api/v1/teams/{team_id}/files/{id}/thumbnail",
    tag = "teams",
    operation_id = "get_team_thumbnail",
    params(
        ("team_id" = i64, Path, description = "Team ID"),
        ("id" = i64, Path, description = "File ID")
    ),
    responses(
        (status = 200, description = "Thumbnail image (WebP)"),
        (status = 304, description = "Thumbnail not modified"),
        (status = 202, description = "Thumbnail generation in progress"),
        (status = 400, description = "Thumbnail not supported for this file type"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 412, description = "Storage backend is disabled or not ready"),
        (status = 404, description = "File not found or thumbnail unavailable"),
        (status = 500, description = "Unexpected thumbnail generation failure"),
    ),
    security(("bearer" = [])),
)]
pub(crate) async fn team_get_thumbnail(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<(i64, i64)>,
) -> Result<HttpResponse> {
    let (team_id, file_id) = path.into_inner();
    get_thumbnail_response(
        state.get_ref(),
        &req,
        team_scope(team_id, claims.user_id),
        file_id,
    )
    .await
}

#[aster_forge_api_docs_macros::path(
    get,
    path = "/api/v1/teams/{team_id}/files/{id}/image-preview",
    tag = "teams",
    operation_id = "get_team_file_image_preview",
    params(
        ("team_id" = i64, Path, description = "Team ID"),
        ("id" = i64, Path, description = "File ID")
    ),
    responses(
        (status = 200, description = "Team image preview (WebP)"),
        (status = 202, description = "Image preview is being generated"),
        (status = 304, description = "Image preview not modified"),
        (status = 400, description = "Image preview not supported for this file type"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 412, description = "Storage backend is disabled or not ready"),
        (status = 404, description = "File not found"),
        (status = 500, description = "Unexpected image preview generation failure"),
    ),
    security(("bearer" = [])),
)]
pub(crate) async fn team_get_image_preview(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<(i64, i64)>,
) -> Result<HttpResponse> {
    let (team_id, file_id) = path.into_inner();
    get_image_preview_response(
        state.get_ref(),
        &req,
        team_scope(team_id, claims.user_id),
        file_id,
    )
    .await
}

#[aster_forge_api_docs_macros::path(
    get,
    path = "/api/v1/teams/{team_id}/files/{id}/media-metadata",
    tag = "teams",
    operation_id = "get_team_file_media_metadata",
    params(
        ("team_id" = i64, Path, description = "Team ID"),
        ("id" = i64, Path, description = "File ID")
    ),
    responses(
        (status = 200, description = "Team file blob media metadata", body = inline(ApiResponse<metadata::MediaMetadataInfo>)),
        (status = 202, description = "Media metadata extraction in progress"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "File not found"),
    ),
    security(("bearer" = [])),
)]
pub(crate) async fn team_get_media_metadata(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    path: web::Path<(i64, i64)>,
) -> Result<HttpResponse> {
    let (team_id, file_id) = path.into_inner();
    get_media_metadata_response(
        state.get_ref(),
        team_scope(team_id, claims.user_id),
        file_id,
    )
    .await
}

#[aster_forge_api_docs_macros::path(
    get,
    path = "/api/v1/teams/{team_id}/files/{id}/download",
    tag = "teams",
    operation_id = "download_team_file",
    params(
        ("team_id" = i64, Path, description = "Team ID"),
        ("id" = i64, Path, description = "File ID"),
        DownloadQuery
    ),
    responses(
        (status = 200, description = "Team file content"),
        (status = 206, description = "Partial team file content"),
        (status = 401, description = crate::api::constants::OPENAPI_UNAUTHORIZED),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "File not found"),
    ),
    security(("bearer" = [])),
)]
pub(crate) async fn team_download(
    state: web::Data<PrimaryAppState>,
    claims: web::ReqData<Claims>,
    req: HttpRequest,
    path: web::Path<(i64, i64)>,
    query: web::Query<DownloadQuery>,
) -> Result<HttpResponse> {
    let (team_id, file_id) = path.into_inner();
    download_response(
        state.get_ref(),
        &claims,
        &req,
        team_scope(team_id, claims.user_id),
        file_id,
        download_disposition_from_query(&query)?,
    )
    .await
}

pub(crate) async fn get_file_response(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    file_id: i64,
) -> Result<HttpResponse> {
    let file = file::get_info_with_storage_used_in_scope(state, scope, file_id).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(file)))
}

pub(crate) async fn resource_handle_response(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    file_id: i64,
    paths: file::FileResourcePathSet,
    request: &FileResourceHandleRequest,
) -> Result<HttpResponse> {
    let handle = file::resolve_file_resource_handle(state, scope, file_id, paths, request).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(handle)))
}

pub(crate) async fn archive_preview_response(
    state: &PrimaryAppState,
    req: &HttpRequest,
    scope: WorkspaceStorageScope,
    file_id: i64,
    filename_encoding: crate::types::ArchiveFilenameEncoding,
) -> Result<HttpResponse> {
    let if_none_match = req
        .headers()
        .get(header::IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok());
    match preview::preview_file_in_scope(state, scope, file_id, filename_encoding).await? {
        preview::ArchivePreviewManifestLookup::Ready(manifest) => {
            archive_preview_manifest_response(
                manifest,
                if_none_match,
                "private, max-age=0, must-revalidate",
            )
        }
        preview::ArchivePreviewManifestLookup::Pending => Ok(archive_preview_pending_response()),
    }
}

pub(crate) fn archive_preview_pending_response() -> HttpResponse {
    HttpResponse::Accepted()
        .insert_header((header::RETRY_AFTER, "2"))
        .json(ApiResponse::<()>::ok_empty())
}

pub(crate) fn archive_preview_manifest_response(
    manifest: preview::ArchivePreviewManifest,
    if_none_match: Option<&str>,
    cache_control: &str,
) -> Result<HttpResponse> {
    let etag_value = format!(
        "archive-preview-{}",
        preview::manifest_etag_value(&manifest)?
    );
    let etag = format!("\"{etag_value}\"");

    if let Some(if_none_match) = if_none_match
        && file::if_none_match_matches_value(if_none_match, &etag_value)
    {
        return Ok(HttpResponse::NotModified()
            .insert_header((header::ETAG, etag))
            .insert_header((header::CACHE_CONTROL, cache_control))
            .finish());
    }

    Ok(HttpResponse::Ok()
        .insert_header((header::ETAG, etag))
        .insert_header((header::CACHE_CONTROL, cache_control))
        .json(ApiResponse::ok(manifest)))
}

pub(crate) async fn direct_link_response(
    state: &PrimaryAppState,
    claims: &Claims,
    req: &HttpRequest,
    scope: WorkspaceStorageScope,
    file_id: i64,
) -> Result<HttpResponse> {
    let file = file::get_info_in_scope(state, scope, file_id).await?;
    let token = direct_link::create_token_in_scope(state, scope, file_id).await?;
    let ctx = AuditContext::from_request(req, claims);
    audit::log_with_details(
        state,
        &ctx,
        audit::AuditAction::FileDirectLinkCreate,
        crate::services::ops::audit::AuditEntityType::File,
        Some(file.id),
        Some(&file.name),
        || {
            audit::details(audit::FileAccessTokenAuditDetails {
                source: "direct_link",
                app_key: None,
            })
        },
    )
    .await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(token)))
}

pub(crate) async fn preview_link_response(
    state: &PrimaryAppState,
    claims: &Claims,
    req: &HttpRequest,
    scope: WorkspaceStorageScope,
    file_id: i64,
) -> Result<HttpResponse> {
    let file = file::get_info_in_scope(state, scope, file_id).await?;
    let (scheme, host) = request_origin_parts(req);
    let link = preview_link::create_token_for_file_in_scope_for_origin(
        state,
        scope,
        file_id,
        preview_link::RequestOrigin {
            scheme: &scheme,
            host: &host,
        },
    )
    .await?;
    let ctx = AuditContext::from_request(req, claims);
    audit::log_with_details(
        state,
        &ctx,
        audit::AuditAction::FilePreviewLinkCreate,
        crate::services::ops::audit::AuditEntityType::File,
        Some(file.id),
        Some(&file.name),
        || {
            audit::details(audit::FileAccessTokenAuditDetails {
                source: "preview_link",
                app_key: None,
            })
        },
    )
    .await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(link)))
}

pub(crate) async fn open_wopi_response(
    state: &PrimaryAppState,
    claims: &Claims,
    req: &HttpRequest,
    scope: WorkspaceStorageScope,
    file_id: i64,
    app_key: &str,
) -> Result<HttpResponse> {
    let file = file::get_info_in_scope(state, scope, file_id).await?;
    let (scheme, host) = request_origin_parts(req);
    let session = wopi::create_launch_session_in_scope(
        state,
        scope,
        file_id,
        app_key,
        Some(wopi::RequestOrigin {
            scheme: &scheme,
            host: &host,
        }),
    )
    .await?;
    let ctx = AuditContext::from_request(req, claims);
    audit::log_with_details(
        state,
        &ctx,
        audit::AuditAction::FileWopiOpen,
        crate::services::ops::audit::AuditEntityType::File,
        Some(file.id),
        Some(&file.name),
        || {
            audit::details(audit::FileAccessTokenAuditDetails {
                source: "wopi",
                app_key: Some(app_key),
            })
        },
    )
    .await;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(session)))
}

pub(crate) async fn download_response(
    state: &PrimaryAppState,
    claims: &Claims,
    req: &HttpRequest,
    scope: WorkspaceStorageScope,
    file_id: i64,
    disposition: file::DownloadDisposition,
) -> Result<HttpResponse> {
    let if_none_match = req
        .headers()
        .get("If-None-Match")
        .and_then(|value| value.to_str().ok());
    let file = file::get_info_in_scope(state, scope, file_id).await?;
    let range = file::parse_range_header(req.headers().get(header::RANGE), file.size)?;
    let ctx = AuditContext::from_request(req, claims);
    let outcome = file::download_in_scope_with_file_and_audit(
        state,
        scope,
        file,
        disposition,
        if_none_match,
        range,
        &ctx,
    )
    .await?;
    Ok(file::outcome_to_response(outcome))
}

pub(crate) async fn get_thumbnail_response(
    state: &PrimaryAppState,
    req: &HttpRequest,
    scope: WorkspaceStorageScope,
    file_id: i64,
) -> Result<HttpResponse> {
    let if_none_match = req
        .headers()
        .get("If-None-Match")
        .and_then(|value| value.to_str().ok());

    match file::get_thumbnail_data_in_scope(state, scope, file_id).await? {
        Some(result) => Ok(thumbnail_response(
            result,
            if_none_match,
            "private, max-age=0, must-revalidate".to_string(),
        )),
        None => Ok(HttpResponse::Accepted()
            .insert_header(("Retry-After", "2"))
            .json(ApiResponse::<()>::ok_empty())),
    }
}

pub(crate) async fn get_image_preview_response(
    state: &PrimaryAppState,
    req: &HttpRequest,
    scope: WorkspaceStorageScope,
    file_id: i64,
) -> Result<HttpResponse> {
    let if_none_match = req
        .headers()
        .get("If-None-Match")
        .and_then(|value| value.to_str().ok());
    let result = file::get_image_preview_data_in_scope(state, scope, file_id).await?;
    match result {
        Some(result) => Ok(image_preview_response(
            result,
            if_none_match,
            "private, max-age=0, must-revalidate".to_string(),
        )),
        None => Ok(HttpResponse::Accepted()
            .insert_header((header::RETRY_AFTER, "2"))
            .json(ApiResponse::<()>::ok_empty())),
    }
}

pub(crate) async fn get_media_metadata_response(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    file_id: i64,
) -> Result<HttpResponse> {
    match metadata::get_for_file_in_scope(state, scope, file_id).await? {
        metadata::MediaMetadataLookup::Ready(info) => {
            Ok(HttpResponse::Ok().json(ApiResponse::ok(info)))
        }
        metadata::MediaMetadataLookup::Pending => Ok(HttpResponse::Accepted()
            .insert_header((header::RETRY_AFTER, "2"))
            .json(ApiResponse::<()>::ok_empty())),
    }
}

pub(crate) fn image_preview_response(
    result: file::ImagePreviewResult,
    if_none_match: Option<&str>,
    cache_control: String,
) -> HttpResponse {
    let etag_value = processing::image_preview_etag_value_for(
        &result.blob_hash,
        &result.image_preview_processor,
        &result.image_preview_version,
    );
    let etag = format!("\"{etag_value}\"");
    if let Some(if_none_match) = if_none_match
        && file::if_none_match_matches_value(if_none_match, &etag_value)
    {
        return HttpResponse::NotModified()
            .insert_header(("ETag", etag))
            .insert_header(("Cache-Control", cache_control))
            .finish();
    }

    HttpResponse::Ok()
        .content_type("image/webp")
        .insert_header(("ETag", etag))
        .insert_header(("Cache-Control", cache_control))
        .body(result.data)
}

pub(crate) fn thumbnail_response(
    result: file::ThumbnailResult,
    if_none_match: Option<&str>,
    cache_control: String,
) -> HttpResponse {
    let etag_value = processing::thumbnail_etag_value_for(
        &result.blob_hash,
        result.thumbnail_processor.as_deref(),
        result.thumbnail_version.as_deref(),
    );
    let etag = format!("\"{etag_value}\"");
    if let Some(if_none_match) = if_none_match
        && file::if_none_match_matches_value(if_none_match, &etag_value)
    {
        return HttpResponse::NotModified()
            .insert_header(("ETag", etag))
            .insert_header(("Cache-Control", cache_control))
            .finish();
    }

    HttpResponse::Ok()
        .content_type("image/webp")
        .insert_header(("ETag", etag))
        .insert_header(("Cache-Control", cache_control))
        .body(result.data)
}

#[cfg(test)]
mod tests {
    use super::{image_preview_response, thumbnail_response};
    use crate::api::dto::files::{
        FileResourceConditionalHeaders, FileResourceCredentials, FileResourceDeliveryMode,
        FileResourceHandle, FileResourcePurpose, FileResourceRedirectPolicy,
        FileResourceRepresentation,
    };
    use crate::config::{Config, DatabaseConfig, RateLimitConfig, RuntimeConfig};
    use crate::db::repository::{background_task_repo, file_repo};
    use crate::entities::{file, file_blob, storage_policy, team, team_member, user};
    use crate::runtime::{PrimaryAppState, SharedRuntimeState};
    use crate::services::files::file::{ImagePreviewResult, ThumbnailResult};
    use crate::services::{auth::local, mail::sender, media::processing};
    use crate::storage::StorageDriver;
    use crate::storage::drivers::local::LocalDriver;
    use crate::storage::{DriverRegistry, PolicySnapshot};
    use crate::types::{
        BackgroundTaskKind, BackgroundTaskStatus, DriverType, StoredStoragePolicyAllowedTypes,
        StoredStoragePolicyOptions, TeamMemberRole, UserRole, UserStatus,
    };
    use actix_web::body;
    use actix_web::http::{StatusCode, header};
    use actix_web::{App, test, web};
    use aster_forge_cache as cache;
    use aster_forge_cache::CacheConfig;
    use chrono::Utc;
    use image::codecs::png::PngEncoder;
    use image::{ColorType, ImageEncoder};
    use migration::Migrator;
    use sea_orm::{ActiveModelTrait, Set};
    use serde_json::Value;
    use std::io::Cursor;
    use std::sync::Arc;

    fn tiny_png() -> Vec<u8> {
        let mut buf = Cursor::new(Vec::new());
        let encoder = PngEncoder::new(&mut buf);
        encoder
            .write_image(&[255, 0, 0], 1, 1, ColorType::Rgb8.into())
            .expect("test png should encode");
        buf.into_inner()
    }

    fn image_preview_blob_hash() -> String {
        crate::utils::hash::sha256_hex(&tiny_png())
    }

    async fn build_image_preview_route_state() -> (PrimaryAppState, user::Model, file::Model) {
        let temp_root = std::env::temp_dir().join(format!(
            "asterdrive-image-preview-route-{}",
            uuid::Uuid::new_v4()
        ));
        tokio::fs::create_dir_all(&temp_root)
            .await
            .expect("image preview route temp root should exist");

        let db = crate::db::connect_with_metrics(
            &DatabaseConfig {
                url: "sqlite::memory:".to_string(),
                pool_size: 1,
                retry_count: 0,
            },
            crate::metrics::NoopMetrics::arc(),
        )
        .await
        .expect("image preview route database should connect");
        Migrator::up(&db, None)
            .await
            .expect("image preview route migrations should succeed");

        let now = Utc::now();
        let storage_root = temp_root.join("storage");
        tokio::fs::create_dir_all(&storage_root)
            .await
            .expect("image preview route storage root should exist");
        let policy = storage_policy::ActiveModel {
            name: Set("Image Preview Route Policy".to_string()),
            driver_type: Set(DriverType::Local),
            endpoint: Set(String::new()),
            bucket: Set(String::new()),
            access_key: Set(String::new()),
            secret_key: Set(String::new()),
            base_path: Set(storage_root.to_string_lossy().into_owned()),
            max_file_size: Set(0),
            allowed_types: Set(StoredStoragePolicyAllowedTypes::empty()),
            options: Set(StoredStoragePolicyOptions::empty()),
            is_default: Set(true),
            chunk_size: Set(5_242_880),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&db)
        .await
        .expect("image preview route policy should insert");

        let user = user::ActiveModel {
            username: Set("preview-route-user".to_string()),
            email: Set("preview-route@example.com".to_string()),
            password_hash: Set("unused".to_string()),
            role: Set(UserRole::User),
            status: Set(UserStatus::Active),
            session_version: Set(1),
            email_verified_at: Set(Some(now)),
            pending_email: Set(None),
            storage_used: Set(0),
            storage_quota: Set(0),
            policy_group_id: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
            config: Set(None),
            ..Default::default()
        }
        .insert(&db)
        .await
        .expect("image preview route user should insert");

        let source_bytes = tiny_png();
        let source_hash = crate::utils::hash::sha256_hex(&source_bytes);
        let driver = Arc::new(
            LocalDriver::new(&policy).expect("image preview route local driver should build"),
        );
        let source_path = "objects/source.png";
        driver
            .put(source_path, &source_bytes)
            .await
            .expect("image preview route source object should write");
        let blob = file_repo::create_blob(
            &db,
            file_blob::ActiveModel {
                hash: Set(source_hash),
                size: Set(crate::utils::numbers::usize_to_i64(
                    source_bytes.len(),
                    "image preview route source size",
                )
                .expect("image preview route source size should fit i64")),
                policy_id: Set(policy.id),
                storage_path: Set(source_path.to_string()),
                thumbnail_path: Set(None),
                thumbnail_processor: Set(None),
                thumbnail_version: Set(None),
                ref_count: Set(1),
                created_at: Set(now),
                updated_at: Set(now),
                ..Default::default()
            },
        )
        .await
        .expect("image preview route blob should insert");

        let file = file_repo::create_with_blob(
            &db,
            file_repo::CreateFileWithBlobInput {
                name: "source.png",
                folder_id: None,
                team_id: None,
                blob_id: blob.id,
                size: blob.size,
                owner_user_id: Some(user.id),
                created_by_user_id: Some(user.id),
                created_by_username: &user.username,
                mime_type: "image/png",
                now,
            },
        )
        .await
        .expect("image preview route file should insert");

        let policy_snapshot = Arc::new(PolicySnapshot::new());
        policy_snapshot
            .reload(&db)
            .await
            .expect("image preview route policy snapshot should reload");
        let driver_registry = Arc::new(DriverRegistry::noop());
        driver_registry.insert_for_test(policy.id, driver);

        let runtime_config = Arc::new(RuntimeConfig::new());
        let cache = cache::create_cache(&CacheConfig {
            ..Default::default()
        })
        .await;
        let mut config = Config::default();
        config.server.temp_dir = temp_root.join(".tmp").to_string_lossy().into_owned();
        config.server.upload_temp_dir = temp_root.join(".uploads").to_string_lossy().into_owned();

        let (storage_change_tx, _) = tokio::sync::broadcast::channel(
            crate::services::events::storage_change::STORAGE_CHANGE_CHANNEL_CAPACITY,
        );
        let share_download_rollback =
            crate::services::share::spawn_detached_share_download_rollback_queue(
                db.clone(),
                crate::config::operations::share_download_rollback_queue_capacity(&runtime_config),
            );

        let state = PrimaryAppState {
            db_handles: aster_forge_db::DbHandles::single(db),
            driver_registry,
            runtime_config: runtime_config.clone(),
            policy_snapshot,
            config: Arc::new(config),
            cache,
            config_sync: aster_forge_config::ConfigSyncRuntime::disabled_for_test("aster_drive"),
            metrics: crate::metrics::NoopMetrics::arc(),
            mail_sender: sender::runtime_sender(runtime_config),
            storage_change_tx,
            share_download_rollback,
            background_task_dispatch_wakeup:
                crate::runtime::PrimaryAppState::new_background_task_dispatch_wakeup(),
            remote_protocol: crate::runtime::PrimaryAppState::new_remote_protocol(),
        };

        (state, user, file)
    }

    async fn access_token_for(state: &PrimaryAppState, user: &user::Model) -> String {
        local::issue_tokens_for_user(state, user, None, None)
            .await
            .expect("image preview route access token should issue")
            .0
    }

    async fn create_team_scoped_file(
        state: &PrimaryAppState,
        owner: &user::Model,
        original_file: &file::Model,
    ) -> (team::Model, file::Model) {
        let now = Utc::now();
        let team = team::ActiveModel {
            name: Set("Resource Handle Route Team".to_string()),
            description: Set(String::new()),
            created_by: Set(owner.id),
            storage_used: Set(0),
            storage_quota: Set(0),
            policy_group_id: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
            archived_at: Set(None),
            ..Default::default()
        }
        .insert(state.writer_db())
        .await
        .expect("resource handle route team should insert");
        team_member::ActiveModel {
            team_id: Set(team.id),
            user_id: Set(owner.id),
            role: Set(TeamMemberRole::Owner),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(state.writer_db())
        .await
        .expect("resource handle route team membership should insert");
        let team_file = file_repo::create(
            state.writer_db(),
            file::ActiveModel {
                name: Set("team-source.png".to_string()),
                folder_id: Set(None),
                team_id: Set(Some(team.id)),
                blob_id: Set(original_file.blob_id),
                size: Set(original_file.size),
                owner_user_id: Set(None),
                created_by_user_id: Set(Some(owner.id)),
                created_by_username: Set(owner.username.clone()),
                mime_type: Set("image/png".to_string()),
                created_at: Set(now),
                updated_at: Set(now),
                deleted_at: Set(None),
                is_locked: Set(false),
                ..Default::default()
            },
        )
        .await
        .expect("resource handle route team file should insert");

        (team, team_file)
    }

    #[actix_web::test]
    async fn resolve_resource_handle_route_defaults_representation_to_auto() {
        let (state, user, file) = build_image_preview_route_state().await;
        let token = access_token_for(&state, &user).await;
        let app = test::init_service(App::new().app_data(web::Data::new(state.clone())).service(
            web::scope("/api/v1").service(crate::api::routes::files::routes(
                &RateLimitConfig {
                    enabled: false,
                    ..Default::default()
                },
                &crate::config::NetworkTrustConfig::default(),
            )),
        ))
        .await;

        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri(&format!("/api/v1/files/{}/resource-handle", file.id))
                .insert_header((header::AUTHORIZATION, format!("Bearer {token}")))
                .set_json(serde_json::json!({
                    "purpose": "preview",
                    "delivery_mode": "blob_url"
                }))
                .to_request(),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        let body: Value = test::read_body_json(response).await;
        assert_eq!(body["code"], "success");
        let handle: FileResourceHandle =
            serde_json::from_value(body["data"].clone()).expect("resource handle should decode");
        assert_eq!(
            handle.identity.cache_key,
            format!("/files/{}/download", file.id)
        );
        assert_eq!(handle.identity.scope.as_deref(), Some("personal"));
        assert_eq!(
            handle.request.url,
            format!("/files/{}/download?disposition=inline", file.id)
        );
        assert_eq!(handle.request.credentials, FileResourceCredentials::Include);
        assert_eq!(
            handle.request.conditional_headers,
            FileResourceConditionalHeaders::Allowed
        );
        assert_eq!(
            handle.request.redirect_policy,
            FileResourceRedirectPolicy::SameOriginOnly
        );
        assert_eq!(handle.delivery.mode, FileResourceDeliveryMode::BlobUrl);
        assert_eq!(handle.delivery.mime_type.as_deref(), Some("image/png"));

        let decoded: crate::api::dto::files::FileResourceHandleRequest =
            serde_json::from_value(serde_json::json!({
                "purpose": "preview",
                "delivery_mode": "blob_url"
            }))
            .expect("request without representation should deserialize");
        assert_eq!(decoded.purpose, FileResourcePurpose::Preview);
        assert_eq!(decoded.representation, FileResourceRepresentation::Auto);
    }

    #[actix_web::test]
    async fn team_resolve_resource_handle_route_uses_team_paths_and_scope() {
        let (state, user, source_file) = build_image_preview_route_state().await;
        let (team, team_file) = create_team_scoped_file(&state, &user, &source_file).await;
        let token = access_token_for(&state, &user).await;
        let app = test::init_service(App::new().app_data(web::Data::new(state.clone())).service(
            web::scope("/api/v1").service(crate::api::routes::teams::routes(
                &RateLimitConfig {
                    enabled: false,
                    ..Default::default()
                },
                &crate::config::NetworkTrustConfig::default(),
            )),
        ))
        .await;

        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri(&format!(
                    "/api/v1/teams/{}/files/{}/resource-handle",
                    team.id, team_file.id
                ))
                .insert_header((header::AUTHORIZATION, format!("Bearer {token}")))
                .set_json(serde_json::json!({
                    "purpose": "preview",
                    "delivery_mode": "blob_url",
                    "representation": "thumbnail"
                }))
                .to_request(),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        let body: Value = test::read_body_json(response).await;
        assert_eq!(body["code"], "success");
        let handle: FileResourceHandle = serde_json::from_value(body["data"].clone())
            .expect("team resource handle should decode");
        assert_eq!(
            handle.identity.cache_key,
            format!("/teams/{}/files/{}/thumbnail", team.id, team_file.id)
        );
        assert_eq!(handle.identity.scope.as_deref(), Some("team"));
        assert_eq!(
            handle.request.url,
            format!("/teams/{}/files/{}/thumbnail", team.id, team_file.id)
        );
        assert_eq!(handle.request.credentials, FileResourceCredentials::Include);
        assert_eq!(
            handle.request.conditional_headers,
            FileResourceConditionalHeaders::Allowed
        );
        assert_eq!(
            handle.request.redirect_policy,
            FileResourceRedirectPolicy::SameOriginOnly
        );
        assert_eq!(handle.delivery.mode, FileResourceDeliveryMode::BlobUrl);
        assert_eq!(handle.delivery.mime_type.as_deref(), Some("image/webp"));
    }

    #[actix_web::test]
    async fn image_preview_response_returns_ok_with_etag_and_cache_headers() {
        let result = ImagePreviewResult {
            data: bytes::Bytes::from_static(&[1, 2, 3]),
            blob_hash: "abc".repeat(21) + "a",
            image_preview_processor: "images".to_string(),
            image_preview_version: "1".to_string(),
        };

        let resp = image_preview_response(
            result,
            None,
            "private, max-age=0, must-revalidate".to_string(),
        );

        assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
        assert_eq!(
            resp.headers().get(header::CONTENT_TYPE).unwrap(),
            "image/webp"
        );
        assert_eq!(
            resp.headers().get(header::ETAG).unwrap(),
            "\"image-preview-images-1-abcabcabcabcabcabcabcabcabcabcabcabcabcabcabcabcabcabcabcabcabca\""
        );
        assert_eq!(
            resp.headers().get(header::CACHE_CONTROL).unwrap(),
            "private, max-age=0, must-revalidate"
        );
        let body = body::to_bytes(resp.into_body())
            .await
            .expect("image preview response body should read");
        assert_eq!(body, web::Bytes::from_static(&[1, 2, 3]));
    }

    #[actix_web::test]
    async fn image_preview_response_respects_if_none_match() {
        let result = ImagePreviewResult {
            data: bytes::Bytes::from_static(&[1, 2, 3]),
            blob_hash: "abc".repeat(21) + "a",
            image_preview_processor: "images".to_string(),
            image_preview_version: "1".to_string(),
        };

        let resp = image_preview_response(
            result,
            Some(
                "\"image-preview-images-1-abcabcabcabcabcabcabcabcabcabcabcabcabcabcabcabcabcabcabcabcabca\"",
            ),
            "private, max-age=0, must-revalidate".to_string(),
        );

        assert_eq!(resp.status(), actix_web::http::StatusCode::NOT_MODIFIED);
        let body = body::to_bytes(resp.into_body())
            .await
            .expect("image preview 304 response body should read");
        assert!(body.is_empty());
    }

    #[actix_web::test]
    async fn thumbnail_response_respects_if_none_match() {
        let result = ThumbnailResult {
            data: web::Bytes::from_static(&[1, 2, 3]),
            blob_hash: "abc".repeat(21) + "a",
            thumbnail_processor: Some("images".to_string()),
            thumbnail_version: Some("1".to_string()),
        };

        let resp = thumbnail_response(
            result,
            Some(
                "\"thumb-images-1-abcabcabcabcabcabcabcabcabcabcabcabcabcabcabcabcabcabcabcabcabca\"",
            ),
            "private, max-age=0, must-revalidate".to_string(),
        );

        assert_eq!(resp.status(), actix_web::http::StatusCode::NOT_MODIFIED);
        let body = body::to_bytes(resp.into_body())
            .await
            .expect("thumbnail 304 response body should read");
        assert!(body.is_empty());
    }

    #[actix_web::test]
    async fn get_image_preview_route_enqueues_background_task_on_cache_miss() {
        let (state, user, file) = build_image_preview_route_state().await;
        let token = access_token_for(&state, &user).await;
        let app = test::init_service(App::new().app_data(web::Data::new(state.clone())).service(
            web::scope("/api/v1").service(crate::api::routes::files::routes(
                &RateLimitConfig {
                    enabled: false,
                    ..Default::default()
                },
                &crate::config::NetworkTrustConfig::default(),
            )),
        ))
        .await;

        let response = test::call_service(
            &app,
            test::TestRequest::get()
                .uri(&format!("/api/v1/files/{}/image-preview", file.id))
                .insert_header((header::AUTHORIZATION, format!("Bearer {token}")))
                .to_request(),
        )
        .await;

        let info = crate::services::files::file::get_info(&state, file.id, user.id)
            .await
            .expect("image preview route file info should load");
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        assert_eq!(response.headers().get(header::RETRY_AFTER).unwrap(), "2");
        let task = background_task_repo::find_latest_by_kind_and_display_name(
            state.writer_db(),
            BackgroundTaskKind::ImagePreviewGenerate,
            &format!(
                "Generate image preview for blob #{} via AsterDrive built-in",
                info.blob_id
            ),
        )
        .await
        .expect("image preview task lookup should succeed")
        .expect("image preview cache miss should enqueue a task");
        assert_eq!(task.status, BackgroundTaskStatus::Pending);
        let body = body::to_bytes(response.into_body())
            .await
            .expect("image preview 202 response body should read");
        assert!(!body.is_empty());
    }

    #[actix_web::test]
    async fn get_image_preview_route_returns_cached_webp_and_honors_if_none_match() {
        let (state, user, file) = build_image_preview_route_state().await;
        let token = access_token_for(&state, &user).await;
        let info = crate::services::files::file::get_info(&state, file.id, user.id)
            .await
            .expect("image preview route file info should load");
        let blob = file_repo::find_blob_by_id(state.writer_db(), info.blob_id)
            .await
            .expect("image preview route blob should load");
        processing::generate_and_store_image_preview(&state, &blob, &info.name, &info.mime_type)
            .await
            .expect("image preview route cache should pre-generate");

        let app = test::init_service(App::new().app_data(web::Data::new(state.clone())).service(
            web::scope("/api/v1").service(crate::api::routes::files::routes(
                &RateLimitConfig {
                    enabled: false,
                    ..Default::default()
                },
                &crate::config::NetworkTrustConfig::default(),
            )),
        ))
        .await;

        let response = test::call_service(
            &app,
            test::TestRequest::get()
                .uri(&format!("/api/v1/files/{}/image-preview", file.id))
                .insert_header((header::AUTHORIZATION, format!("Bearer {token}")))
                .to_request(),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "image/webp"
        );
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL).unwrap(),
            "private, max-age=0, must-revalidate"
        );
        let etag = response
            .headers()
            .get(header::ETAG)
            .expect("image preview response should include ETag")
            .to_str()
            .expect("image preview ETag should be valid header")
            .to_string();
        let expected_etag = format!(
            "\"{}\"",
            processing::image_preview_etag_value_for(
                &image_preview_blob_hash(),
                crate::services::files::thumbnail::IMAGES_THUMBNAIL_PROCESSOR_NAMESPACE,
                crate::services::files::thumbnail::CURRENT_IMAGE_PREVIEW_VERSION,
            )
        );
        assert_eq!(etag, expected_etag);
        let body = body::to_bytes(response.into_body())
            .await
            .expect("image preview route response body should read");
        assert!(!body.is_empty());

        let not_modified = test::call_service(
            &app,
            test::TestRequest::get()
                .uri(&format!("/api/v1/files/{}/image-preview", file.id))
                .insert_header((header::AUTHORIZATION, format!("Bearer {token}")))
                .insert_header((header::IF_NONE_MATCH, etag))
                .to_request(),
        )
        .await;

        assert_eq!(not_modified.status(), StatusCode::NOT_MODIFIED);
        assert_eq!(
            not_modified.headers().get(header::CACHE_CONTROL).unwrap(),
            "private, max-age=0, must-revalidate"
        );
        let body = body::to_bytes(not_modified.into_body())
            .await
            .expect("image preview route 304 response body should read");
        assert!(body.is_empty());
    }
}
