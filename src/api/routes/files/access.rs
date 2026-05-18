//! 文件 API 路由：`access`。

use crate::api::dto::files::OpenWopiRequest;
use crate::api::response::ApiResponse;
use crate::api::routes::team_scope;
use crate::errors::Result;
use crate::runtime::PrimaryAppState;
use crate::services::{
    archive_preview_service,
    audit_service::{self, AuditContext},
    auth_service::Claims,
    direct_link_service, file_service, media_processing_service, preview_link_service,
    wopi_service,
    workspace_models::FileInfo,
    workspace_storage_service::WorkspaceStorageScope,
};
use actix_web::http::header;
use actix_web::{HttpRequest, HttpResponse, web};

fn request_origin_parts(req: &HttpRequest) -> (String, String) {
    let conn = req.connection_info();
    (conn.scheme().to_string(), conn.host().to_string())
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/files/{id}",
    tag = "files",
    operation_id = "get_file",
    params(("id" = i64, Path, description = "File ID")),
    responses(
        (status = 200, description = "File info", body = inline(ApiResponse<crate::services::workspace_models::FileInfo>)),
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
    path = "/api/v1/files/{id}/archive-preview",
    tag = "files",
    operation_id = "get_file_archive_preview",
    params(("id" = i64, Path, description = "File ID")),
    responses(
        (status = 200, description = "ZIP archive preview manifest", body = inline(ApiResponse<archive_preview_service::ArchivePreviewManifest>)),
        (status = 202, description = "ZIP archive preview generation has been queued"),
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
) -> Result<HttpResponse> {
    archive_preview_response(
        &state,
        &req,
        WorkspaceStorageScope::Personal {
            user_id: claims.user_id,
        },
        *path,
    )
    .await
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/files/{id}/direct-link",
    tag = "files",
    operation_id = "get_file_direct_link",
    params(("id" = i64, Path, description = "File ID")),
    responses(
        (status = 200, description = "Direct link token", body = inline(ApiResponse<crate::services::direct_link_service::DirectLinkTokenInfo>)),
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
    post,
    path = "/api/v1/files/{id}/preview-link",
    tag = "files",
    operation_id = "create_file_preview_link",
    params(("id" = i64, Path, description = "File ID")),
    responses(
        (status = 200, description = "Preview link", body = inline(ApiResponse<crate::services::preview_link_service::PreviewLinkInfo>)),
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
    post,
    path = "/api/v1/files/{id}/wopi/open",
    tag = "files",
    operation_id = "open_file_with_wopi",
    params(("id" = i64, Path, description = "File ID")),
    request_body = OpenWopiRequest,
    responses(
        (status = 200, description = "WOPI launch session", body = inline(ApiResponse<wopi_service::WopiLaunchSession>)),
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
        &state,
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

#[api_docs_macros::path(
    get,
    path = "/api/v1/files/{id}/download",
    tag = "files",
    operation_id = "download_file",
    params(("id" = i64, Path, description = "File ID")),
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
) -> Result<HttpResponse> {
    download_response(
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
        &state,
        &req,
        WorkspaceStorageScope::Personal {
            user_id: claims.user_id,
        },
        *path,
    )
    .await
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/teams/{team_id}/files/{id}",
    tag = "teams",
    operation_id = "get_team_file",
    params(
        ("team_id" = i64, Path, description = "Team ID"),
        ("id" = i64, Path, description = "File ID")
    ),
    responses(
        (status = 200, description = "Team file info", body = inline(ApiResponse<FileInfo>)),
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
    get_file_response(&state, team_scope(team_id, claims.user_id), file_id).await
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/teams/{team_id}/files/{id}/archive-preview",
    tag = "teams",
    operation_id = "get_team_file_archive_preview",
    params(
        ("team_id" = i64, Path, description = "Team ID"),
        ("id" = i64, Path, description = "File ID")
    ),
    responses(
        (status = 200, description = "Team ZIP archive preview manifest", body = inline(ApiResponse<archive_preview_service::ArchivePreviewManifest>)),
        (status = 202, description = "ZIP archive preview generation has been queued"),
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
) -> Result<HttpResponse> {
    let (team_id, file_id) = path.into_inner();
    archive_preview_response(&state, &req, team_scope(team_id, claims.user_id), file_id).await
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/teams/{team_id}/files/{id}/direct-link",
    tag = "teams",
    operation_id = "get_team_file_direct_link",
    params(
        ("team_id" = i64, Path, description = "Team ID"),
        ("id" = i64, Path, description = "File ID")
    ),
    responses(
        (status = 200, description = "Team file direct link token", body = inline(ApiResponse<crate::services::direct_link_service::DirectLinkTokenInfo>)),
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
        &state,
        &claims,
        &req,
        team_scope(team_id, claims.user_id),
        file_id,
    )
    .await
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/teams/{team_id}/files/{id}/preview-link",
    tag = "teams",
    operation_id = "create_team_file_preview_link",
    params(
        ("team_id" = i64, Path, description = "Team ID"),
        ("id" = i64, Path, description = "File ID")
    ),
    responses(
        (status = 200, description = "Team file preview link", body = inline(ApiResponse<crate::services::preview_link_service::PreviewLinkInfo>)),
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
        &state,
        &claims,
        &req,
        team_scope(team_id, claims.user_id),
        file_id,
    )
    .await
}

#[api_docs_macros::path(
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
        (status = 200, description = "Team WOPI launch session", body = inline(ApiResponse<wopi_service::WopiLaunchSession>)),
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
        &state,
        &claims,
        &req,
        team_scope(team_id, claims.user_id),
        file_id,
        &body.app_key,
    )
    .await
}

#[api_docs_macros::path(
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
    get_thumbnail_response(&state, &req, team_scope(team_id, claims.user_id), file_id).await
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/teams/{team_id}/files/{id}/download",
    tag = "teams",
    operation_id = "download_team_file",
    params(
        ("team_id" = i64, Path, description = "Team ID"),
        ("id" = i64, Path, description = "File ID")
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
) -> Result<HttpResponse> {
    let (team_id, file_id) = path.into_inner();
    download_response(
        &state,
        &claims,
        &req,
        team_scope(team_id, claims.user_id),
        file_id,
    )
    .await
}

pub(crate) async fn get_file_response(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    file_id: i64,
) -> Result<HttpResponse> {
    let file = file_service::get_info_in_scope(state, scope, file_id).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(FileInfo::from(file))))
}

pub(crate) async fn archive_preview_response(
    state: &PrimaryAppState,
    req: &HttpRequest,
    scope: WorkspaceStorageScope,
    file_id: i64,
) -> Result<HttpResponse> {
    let if_none_match = req
        .headers()
        .get(header::IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok());
    match archive_preview_service::preview_file_in_scope(state, scope, file_id).await? {
        archive_preview_service::ArchivePreviewManifestLookup::Ready(manifest) => {
            archive_preview_manifest_response(
                manifest,
                if_none_match,
                "private, max-age=0, must-revalidate",
            )
        }
        archive_preview_service::ArchivePreviewManifestLookup::Pending => {
            Ok(archive_preview_pending_response())
        }
    }
}

pub(crate) fn archive_preview_pending_response() -> HttpResponse {
    HttpResponse::Accepted()
        .insert_header((header::RETRY_AFTER, "2"))
        .json(ApiResponse::<()>::ok_empty())
}

pub(crate) fn archive_preview_manifest_response(
    manifest: archive_preview_service::ArchivePreviewManifest,
    if_none_match: Option<&str>,
    cache_control: &str,
) -> Result<HttpResponse> {
    let etag_value = format!(
        "archive-preview-{}",
        archive_preview_service::manifest_etag_value(&manifest)?
    );
    let etag = format!("\"{etag_value}\"");

    if let Some(if_none_match) = if_none_match
        && file_service::if_none_match_matches_value(if_none_match, &etag_value)
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
    let file = file_service::get_info_in_scope(state, scope, file_id).await?;
    let token = direct_link_service::create_token_in_scope(state, scope, file_id).await?;
    let ctx = AuditContext::from_request(req, claims);
    audit_service::log(
        state,
        &ctx,
        audit_service::AuditAction::FileDirectLinkCreate,
        crate::services::audit_service::AuditEntityType::File,
        Some(file.id),
        Some(&file.name),
        audit_service::details(audit_service::FileAccessTokenAuditDetails {
            source: "direct_link",
            app_key: None,
        }),
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
    let file = file_service::get_info_in_scope(state, scope, file_id).await?;
    let (scheme, host) = request_origin_parts(req);
    let link = preview_link_service::create_token_for_file_in_scope_for_origin(
        state,
        scope,
        file_id,
        preview_link_service::RequestOrigin {
            scheme: &scheme,
            host: &host,
        },
    )
    .await?;
    let ctx = AuditContext::from_request(req, claims);
    audit_service::log(
        state,
        &ctx,
        audit_service::AuditAction::FilePreviewLinkCreate,
        crate::services::audit_service::AuditEntityType::File,
        Some(file.id),
        Some(&file.name),
        audit_service::details(audit_service::FileAccessTokenAuditDetails {
            source: "preview_link",
            app_key: None,
        }),
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
    let file = file_service::get_info_in_scope(state, scope, file_id).await?;
    let (scheme, host) = request_origin_parts(req);
    let session = wopi_service::create_launch_session_in_scope(
        state,
        scope,
        file_id,
        app_key,
        Some(wopi_service::RequestOrigin {
            scheme: &scheme,
            host: &host,
        }),
    )
    .await?;
    let ctx = AuditContext::from_request(req, claims);
    audit_service::log(
        state,
        &ctx,
        audit_service::AuditAction::FileWopiOpen,
        crate::services::audit_service::AuditEntityType::File,
        Some(file.id),
        Some(&file.name),
        audit_service::details(audit_service::FileAccessTokenAuditDetails {
            source: "wopi",
            app_key: Some(app_key),
        }),
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
) -> Result<HttpResponse> {
    let if_none_match = req
        .headers()
        .get("If-None-Match")
        .and_then(|value| value.to_str().ok());
    let file = file_service::get_info_in_scope(state, scope, file_id).await?;
    let range = file_service::parse_range_header(req.headers().get(header::RANGE), file.size)?;
    let ctx = AuditContext::from_request(req, claims);
    let outcome = file_service::download_in_scope_with_file_and_audit(
        state,
        scope,
        file,
        if_none_match,
        range,
        &ctx,
    )
    .await?;
    Ok(file_service::outcome_to_response(outcome))
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

    match file_service::get_thumbnail_data_in_scope(state, scope, file_id).await? {
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

pub(crate) fn thumbnail_response(
    result: file_service::ThumbnailResult,
    if_none_match: Option<&str>,
    cache_control: String,
) -> HttpResponse {
    let etag_value = media_processing_service::thumbnail_etag_value_for(
        &result.blob_hash,
        result.thumbnail_processor.as_deref(),
        result.thumbnail_version.as_deref(),
    );
    let etag = format!("\"{etag_value}\"");
    if let Some(if_none_match) = if_none_match
        && file_service::if_none_match_matches_value(if_none_match, &etag_value)
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
