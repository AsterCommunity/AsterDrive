//! API 路由：`share_public`。

use crate::api::dto::share_public::DirectLinkQuery;
pub use crate::api::dto::share_public::VerifyPasswordReq;
use crate::api::middleware::rate_limit;
use crate::api::pagination::FolderListQuery;
use crate::api::response::ApiResponse;
use crate::api::routes::files;
use crate::config::RateLimitConfig;
use crate::config::auth_runtime::RuntimeAuthPolicy;
use crate::errors::Result;
use crate::runtime::PrimaryAppState;
use crate::services::file_service::ResolvedDownloadRange;
use crate::services::{
    archive_preview_service, direct_link_service, file_service, preview_link_service,
    profile_service, share_service, share_stream_service,
};
use actix_governor::Governor;
use actix_web::http::header;
use actix_web::middleware::Condition;
use actix_web::{HttpRequest, HttpResponse, web};

const SHARE_COOKIE_PREFIX: &str = "aster_share_";

fn thumbnail_pending_response() -> HttpResponse {
    HttpResponse::Accepted()
        .insert_header(("Retry-After", "2"))
        .finish()
}

fn request_origin_parts(req: &HttpRequest) -> (String, String) {
    let conn = req.connection_info();
    (conn.scheme().to_string(), conn.host().to_string())
}

fn share_cookie_name(token: &str) -> String {
    format!("{SHARE_COOKIE_PREFIX}{token}")
}

fn share_cookie_path(token: &str) -> String {
    format!("/api/v1/s/{token}")
}

fn build_share_cookie(
    token: &str,
    value: String,
    secure: bool,
) -> actix_web::cookie::Cookie<'static> {
    actix_web::cookie::Cookie::build(share_cookie_name(token), value)
        .path(share_cookie_path(token))
        .http_only(true)
        .same_site(actix_web::cookie::SameSite::Lax)
        .secure(secure)
        .max_age(actix_web::cookie::time::Duration::hours(1))
        .finish()
}

fn share_cookie_value(req: &actix_web::HttpRequest, token: &str) -> Option<String> {
    req.cookie(&share_cookie_name(token))
        .map(|cookie| cookie.value().to_string())
}

async fn shared_file_range(
    state: &PrimaryAppState,
    token: &str,
    req: &HttpRequest,
) -> Result<Option<ResolvedDownloadRange>> {
    if !req.headers().contains_key(header::RANGE) {
        return Ok(None);
    }
    let (_, file) = share_service::load_preview_shared_file(state, token).await?;
    file_service::parse_range_header(req.headers().get(header::RANGE), file.size)
}

async fn shared_folder_file_range(
    state: &PrimaryAppState,
    token: &str,
    file_id: i64,
    req: &HttpRequest,
) -> Result<Option<ResolvedDownloadRange>> {
    if !req.headers().contains_key(header::RANGE) {
        return Ok(None);
    }
    let (_, file) = share_service::load_preview_shared_folder_file(state, token, file_id).await?;
    file_service::parse_range_header(req.headers().get(header::RANGE), file.size)
}

/// Extension methods for `DirectLinkQuery`.
impl DirectLinkQuery {
    pub(crate) fn force_download(&self) -> bool {
        self.download
            .as_deref()
            .map(|value| matches!(value, "1" | "true" | "yes" | "on"))
            .unwrap_or(false)
    }
}

pub fn routes(rl: &RateLimitConfig) -> impl actix_web::dev::HttpServiceFactory + use<> {
    let limiter = rate_limit::build_governor(&rl.public, &rl.trusted_proxies);
    let verify_limiter = rate_limit::build_governor(&rl.auth, &rl.trusted_proxies);

    web::scope("/s")
        .wrap(Condition::new(rl.enabled, Governor::new(&limiter)))
        .route("/{token}", web::get().to(get_share_info))
        .service(
            web::resource("/{token}/verify")
                .wrap(Condition::new(rl.enabled, Governor::new(&verify_limiter)))
                .route(web::post().to(verify_password)),
        )
        .route("/{token}/preview-link", web::post().to(create_preview_link))
        .route("/{token}/archive-preview", web::get().to(archive_preview))
        .route("/{token}/download", web::get().to(download_shared))
        .route(
            "/{token}/files/{file_id}/download",
            web::get().to(download_shared_folder_file),
        )
        .route(
            "/{token}/files/{file_id}/preview-link",
            web::post().to(create_folder_file_preview_link),
        )
        .route(
            "/{token}/files/{file_id}/archive-preview",
            web::get().to(folder_file_archive_preview),
        )
        .route(
            "/{token}/stream-session",
            web::post().to(create_stream_session),
        )
        .route(
            "/{token}/files/{file_id}/stream-session",
            web::post().to(create_folder_file_stream_session),
        )
        .route(
            "/{token}/stream/{session_token}/{filename}",
            web::get().to(stream_shared_video),
        )
        .route("/{token}/content", web::get().to(list_shared_content))
        .route(
            "/{token}/folders/{folder_id}/content",
            web::get().to(list_shared_subfolder_content),
        )
        .route("/{token}/thumbnail", web::get().to(shared_thumbnail))
        .route(
            "/{token}/files/{file_id}/thumbnail",
            web::get().to(shared_folder_file_thumbnail),
        )
        .route("/{token}/avatar/{size}", web::get().to(shared_avatar))
}

pub fn direct_routes(rl: &RateLimitConfig) -> impl actix_web::dev::HttpServiceFactory + use<> {
    let limiter = rate_limit::build_governor(&rl.public, &rl.trusted_proxies);

    (
        web::resource("/d/{token}/{filename}")
            .wrap(Condition::new(rl.enabled, Governor::new(&limiter)))
            .route(web::get().to(download_direct)),
        web::resource("/pv/{token}/{filename}")
            .wrap(Condition::new(rl.enabled, Governor::new(&limiter)))
            .route(web::get().to(download_preview)),
    )
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/s/{token}",
    tag = "shares",
    operation_id = "get_share_info",
    params(("token" = String, Path, description = "Share token")),
    responses(
        (status = 200, description = "Share info", body = inline(ApiResponse<share_service::SharePublicInfo>)),
        (status = 404, description = "Share not found or expired"),
    ),
)]
pub async fn get_share_info(
    state: web::Data<PrimaryAppState>,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    let info = share_service::get_share_info(&state, &path).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(info)))
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/s/{token}/verify",
    tag = "shares",
    operation_id = "verify_share_password",
    params(("token" = String, Path, description = "Share token")),
    request_body = VerifyPasswordReq,
    responses(
        (status = 200, description = "Password verified"),
        (status = 401, description = "Wrong password"),
        (status = 404, description = "Share not found"),
    ),
)]
pub async fn verify_password(
    state: web::Data<PrimaryAppState>,
    path: web::Path<String>,
    body: web::Json<VerifyPasswordReq>,
) -> Result<HttpResponse> {
    let result = share_service::verify_password_and_sign(&state, &path, &body.password).await?;
    let auth_policy = RuntimeAuthPolicy::from_runtime_config(&state.runtime_config);
    let cookie = build_share_cookie(
        path.as_str(),
        result.cookie_signature,
        auth_policy.cookie_secure,
    );

    Ok(HttpResponse::Ok()
        .cookie(cookie)
        .json(ApiResponse::<()>::ok_empty()))
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/s/{token}/preview-link",
    tag = "shares",
    operation_id = "create_shared_file_preview_link",
    params(("token" = String, Path, description = "Share token")),
    responses(
        (status = 200, description = "Preview link", body = inline(ApiResponse<crate::services::preview_link_service::PreviewLinkInfo>)),
        (status = 403, description = "Password required or download limit"),
        (status = 404, description = "Share not found"),
    ),
)]
pub async fn create_preview_link(
    state: web::Data<PrimaryAppState>,
    path: web::Path<String>,
    req: actix_web::HttpRequest,
) -> Result<HttpResponse> {
    let token = path.into_inner();
    let cookie_value = share_cookie_value(&req, &token);
    share_service::check_share_password_cookie(&state, &token, cookie_value.as_deref()).await?;

    let (scheme, host) = request_origin_parts(&req);
    let link = preview_link_service::create_token_for_shared_file_for_origin(
        &state,
        &token,
        preview_link_service::RequestOrigin {
            scheme: &scheme,
            host: &host,
        },
    )
    .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(link)))
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/s/{token}/archive-preview",
    tag = "shares",
    operation_id = "get_shared_file_archive_preview",
    params(("token" = String, Path, description = "Share token")),
    responses(
        (status = 200, description = "ZIP archive preview manifest", body = inline(ApiResponse<archive_preview_service::ArchivePreviewManifest>)),
        (status = 202, description = "ZIP archive preview generation has been queued"),
        (status = 304, description = "Archive preview not modified"),
        (status = 400, description = "Not a supported archive or archive rejected by limits"),
        (status = 403, description = "Password required or archive preview disabled"),
        (status = 404, description = "Share not found"),
    ),
)]
pub async fn archive_preview(
    state: web::Data<PrimaryAppState>,
    path: web::Path<String>,
    req: actix_web::HttpRequest,
) -> Result<HttpResponse> {
    let token = path.into_inner();
    let cookie_value = share_cookie_value(&req, &token);
    share_service::check_share_password_cookie(&state, &token, cookie_value.as_deref()).await?;

    match archive_preview_service::preview_shared_file(&state, &token).await? {
        archive_preview_service::ArchivePreviewManifestLookup::Ready(manifest) => {
            files::archive_preview_manifest_response(
                manifest,
                req.headers()
                    .get(header::IF_NONE_MATCH)
                    .and_then(|value| value.to_str().ok()),
                "public, max-age=0, must-revalidate",
            )
        }
        archive_preview_service::ArchivePreviewManifestLookup::Pending => {
            Ok(files::archive_preview_pending_response())
        }
    }
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/s/{token}/stream-session",
    tag = "shares",
    operation_id = "create_shared_file_stream_session",
    params(("token" = String, Path, description = "Share token")),
    responses(
        (status = 200, description = "Stream session", body = inline(ApiResponse<crate::services::share_stream_service::ShareStreamSessionInfo>)),
        (status = 403, description = "Password required or download limit"),
        (status = 404, description = "Share not found"),
    ),
)]
pub async fn create_stream_session(
    state: web::Data<PrimaryAppState>,
    path: web::Path<String>,
    req: actix_web::HttpRequest,
) -> Result<HttpResponse> {
    let token = path.into_inner();
    let cookie_value = share_cookie_value(&req, &token);
    share_service::check_share_password_cookie(&state, &token, cookie_value.as_deref()).await?;

    let (scheme, host) = request_origin_parts(&req);
    let session = share_stream_service::create_session_for_shared_file_for_origin(
        &state,
        &token,
        preview_link_service::RequestOrigin {
            scheme: &scheme,
            host: &host,
        },
    )
    .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(session)))
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/s/{token}/download",
    tag = "shares",
    operation_id = "download_shared_file",
    params(("token" = String, Path, description = "Share token")),
    responses(
        (status = 200, description = "File content"),
        (status = 206, description = "Partial file content"),
        (status = 403, description = "Password required or download limit"),
        (status = 404, description = "Share not found"),
    ),
)]
pub async fn download_shared(
    state: web::Data<PrimaryAppState>,
    path: web::Path<String>,
    req: actix_web::HttpRequest,
) -> Result<HttpResponse> {
    let cookie_value = share_cookie_value(&req, path.as_str());
    share_service::check_share_password_cookie(&state, &path, cookie_value.as_deref()).await?;
    let range = shared_file_range(&state, path.as_str(), &req).await?;

    let outcome = share_service::download_shared_file_with_range(
        &state,
        &path,
        req.headers()
            .get("If-None-Match")
            .and_then(|v| v.to_str().ok()),
        range,
    )
    .await?;
    Ok(file_service::outcome_to_response(outcome))
}

pub async fn download_direct(
    state: web::Data<PrimaryAppState>,
    path: web::Path<(String, String)>,
    query: web::Query<DirectLinkQuery>,
    req: actix_web::HttpRequest,
) -> Result<HttpResponse> {
    let (token, filename) = path.into_inner();
    let file = direct_link_service::resolve_file_for_download(&state, &token, &filename).await?;
    let range = file_service::parse_range_header(req.headers().get(header::RANGE), file.size)?;
    let outcome = direct_link_service::download_file(
        &state,
        &token,
        &filename,
        query.force_download(),
        req.headers()
            .get("If-None-Match")
            .and_then(|v| v.to_str().ok()),
        range,
    )
    .await?;
    Ok(file_service::outcome_to_response(outcome))
}

pub async fn download_preview(
    state: web::Data<PrimaryAppState>,
    path: web::Path<(String, String)>,
    req: actix_web::HttpRequest,
) -> Result<HttpResponse> {
    let (token, filename) = path.into_inner();
    let file = preview_link_service::resolve_file_for_download(&state, &token, &filename).await?;
    let range = file_service::parse_range_header(req.headers().get(header::RANGE), file.size)?;
    let outcome = preview_link_service::download_file(
        &state,
        &token,
        &filename,
        req.headers()
            .get("If-None-Match")
            .and_then(|v| v.to_str().ok()),
        range,
    )
    .await?;
    Ok(file_service::outcome_to_response(outcome))
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/s/{token}/stream/{session_token}/{filename}",
    tag = "shares",
    operation_id = "stream_shared_video",
    params(
        ("token" = String, Path, description = "Share token"),
        ("session_token" = String, Path, description = "Stream session token"),
        ("filename" = String, Path, description = "File name")
    ),
    responses(
        (status = 200, description = "File content"),
        (status = 206, description = "Partial file content"),
        (status = 403, description = "Password required or download limit"),
        (status = 404, description = "Share or file not found"),
    )
)]
pub async fn stream_shared_video(
    state: web::Data<PrimaryAppState>,
    path: web::Path<(String, String, String)>,
    req: actix_web::HttpRequest,
) -> Result<HttpResponse> {
    let (token, session_token, filename) = path.into_inner();
    let cookie_value = share_cookie_value(&req, &token);
    share_service::check_share_password_cookie_ignoring_download_limit(
        &state,
        &token,
        cookie_value.as_deref(),
    )
    .await?;
    let file =
        share_stream_service::resolve_file_for_stream(&state, &token, &session_token, &filename)
            .await?;
    let range = file_service::parse_range_header(req.headers().get(header::RANGE), file.size)?;
    let outcome =
        share_stream_service::stream_file(&state, &token, &session_token, &filename, range).await?;
    Ok(file_service::outcome_to_response(outcome))
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/s/{token}/files/{file_id}/download",
    tag = "shares",
    operation_id = "download_shared_folder_file",
    params(
        ("token" = String, Path, description = "Share token"),
        ("file_id" = i64, Path, description = "File ID inside shared folder")
    ),
    responses(
        (status = 200, description = "File content"),
        (status = 206, description = "Partial file content"),
        (status = 403, description = "Password required or file outside shared folder"),
        (status = 404, description = "Share or file not found"),
    )
)]
pub async fn download_shared_folder_file(
    state: web::Data<PrimaryAppState>,
    path: web::Path<(String, i64)>,
    req: actix_web::HttpRequest,
) -> Result<HttpResponse> {
    let (token, file_id) = path.into_inner();
    let cookie_value = share_cookie_value(&req, &token);
    share_service::check_share_password_cookie(&state, &token, cookie_value.as_deref()).await?;
    let range = shared_folder_file_range(&state, &token, file_id, &req).await?;

    let outcome = share_service::download_shared_folder_file_with_range(
        &state,
        &token,
        file_id,
        req.headers()
            .get("If-None-Match")
            .and_then(|v| v.to_str().ok()),
        range,
    )
    .await?;
    Ok(file_service::outcome_to_response(outcome))
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/s/{token}/files/{file_id}/preview-link",
    tag = "shares",
    operation_id = "create_shared_folder_file_preview_link",
    params(
        ("token" = String, Path, description = "Share token"),
        ("file_id" = i64, Path, description = "File ID inside shared folder")
    ),
    responses(
        (status = 200, description = "Preview link", body = inline(ApiResponse<crate::services::preview_link_service::PreviewLinkInfo>)),
        (status = 403, description = "Password required or file outside shared folder"),
        (status = 404, description = "Share or file not found"),
    )
)]
pub async fn create_folder_file_preview_link(
    state: web::Data<PrimaryAppState>,
    path: web::Path<(String, i64)>,
    req: actix_web::HttpRequest,
) -> Result<HttpResponse> {
    let (token, file_id) = path.into_inner();
    let cookie_value = share_cookie_value(&req, &token);
    share_service::check_share_password_cookie(&state, &token, cookie_value.as_deref()).await?;

    let (scheme, host) = request_origin_parts(&req);
    let link = preview_link_service::create_token_for_shared_folder_file_for_origin(
        &state,
        &token,
        file_id,
        preview_link_service::RequestOrigin {
            scheme: &scheme,
            host: &host,
        },
    )
    .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(link)))
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/s/{token}/files/{file_id}/archive-preview",
    tag = "shares",
    operation_id = "get_shared_folder_file_archive_preview",
    params(
        ("token" = String, Path, description = "Share token"),
        ("file_id" = i64, Path, description = "File ID inside shared folder")
    ),
    responses(
        (status = 200, description = "ZIP archive preview manifest", body = inline(ApiResponse<archive_preview_service::ArchivePreviewManifest>)),
        (status = 202, description = "ZIP archive preview generation has been queued"),
        (status = 304, description = "Archive preview not modified"),
        (status = 400, description = "Not a supported archive or archive rejected by limits"),
        (status = 403, description = "Password required, file outside shared folder, or archive preview disabled"),
        (status = 404, description = "Share or file not found"),
    )
)]
pub async fn folder_file_archive_preview(
    state: web::Data<PrimaryAppState>,
    path: web::Path<(String, i64)>,
    req: actix_web::HttpRequest,
) -> Result<HttpResponse> {
    let (token, file_id) = path.into_inner();
    let cookie_value = share_cookie_value(&req, &token);
    share_service::check_share_password_cookie(&state, &token, cookie_value.as_deref()).await?;

    match archive_preview_service::preview_shared_folder_file(&state, &token, file_id).await? {
        archive_preview_service::ArchivePreviewManifestLookup::Ready(manifest) => {
            files::archive_preview_manifest_response(
                manifest,
                req.headers()
                    .get(header::IF_NONE_MATCH)
                    .and_then(|value| value.to_str().ok()),
                "public, max-age=0, must-revalidate",
            )
        }
        archive_preview_service::ArchivePreviewManifestLookup::Pending => {
            Ok(files::archive_preview_pending_response())
        }
    }
}

#[api_docs_macros::path(
    post,
    path = "/api/v1/s/{token}/files/{file_id}/stream-session",
    tag = "shares",
    operation_id = "create_shared_folder_file_stream_session",
    params(
        ("token" = String, Path, description = "Share token"),
        ("file_id" = i64, Path, description = "File ID inside shared folder")
    ),
    responses(
        (status = 200, description = "Stream session", body = inline(ApiResponse<crate::services::share_stream_service::ShareStreamSessionInfo>)),
        (status = 403, description = "Password required or file outside shared folder"),
        (status = 404, description = "Share or file not found"),
    )
)]
pub async fn create_folder_file_stream_session(
    state: web::Data<PrimaryAppState>,
    path: web::Path<(String, i64)>,
    req: actix_web::HttpRequest,
) -> Result<HttpResponse> {
    let (token, file_id) = path.into_inner();
    let cookie_value = share_cookie_value(&req, &token);
    share_service::check_share_password_cookie(&state, &token, cookie_value.as_deref()).await?;

    let (scheme, host) = request_origin_parts(&req);
    let session = share_stream_service::create_session_for_shared_folder_file_for_origin(
        &state,
        &token,
        file_id,
        preview_link_service::RequestOrigin {
            scheme: &scheme,
            host: &host,
        },
    )
    .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(session)))
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/s/{token}/content",
    tag = "shares",
    operation_id = "list_shared_content",
    params(("token" = String, Path, description = "Share token"), FolderListQuery),
    responses(
        (status = 200, description = "Folder contents", body = inline(ApiResponse<crate::services::folder_service::FolderContents>)),
        (status = 403, description = "Password required"),
        (status = 404, description = "Share not found"),
    ),
)]
pub async fn list_shared_content(
    state: web::Data<PrimaryAppState>,
    path: web::Path<String>,
    query: web::Query<FolderListQuery>,
    req: actix_web::HttpRequest,
) -> Result<HttpResponse> {
    let cookie_value = share_cookie_value(&req, path.as_str());
    share_service::check_share_password_cookie(&state, &path, cookie_value.as_deref()).await?;

    let params = crate::services::folder_service::FolderListParams::from(&query.0);
    let contents = share_service::list_shared_folder(&state, &path, &params).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(contents)))
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/s/{token}/folders/{folder_id}/content",
    tag = "shares",
    operation_id = "list_shared_subfolder_content",
    params(
        ("token" = String, Path, description = "Share token"),
        ("folder_id" = i64, Path, description = "Subfolder ID inside shared folder"),
        FolderListQuery,
    ),
    responses(
        (status = 200, description = "Subfolder contents", body = inline(ApiResponse<crate::services::folder_service::FolderContents>)),
        (status = 403, description = "Password required or folder outside shared scope"),
        (status = 404, description = "Share or folder not found"),
    )
)]
pub async fn list_shared_subfolder_content(
    state: web::Data<PrimaryAppState>,
    path: web::Path<(String, i64)>,
    query: web::Query<FolderListQuery>,
    req: actix_web::HttpRequest,
) -> Result<HttpResponse> {
    let (token, folder_id) = path.into_inner();
    let cookie_value = share_cookie_value(&req, &token);
    share_service::check_share_password_cookie(&state, &token, cookie_value.as_deref()).await?;

    let params = crate::services::folder_service::FolderListParams::from(&query.0);
    let contents = share_service::list_shared_subfolder(&state, &token, folder_id, &params).await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(contents)))
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/s/{token}/avatar/{size}",
    tag = "shares",
    operation_id = "shared_avatar",
    params(
        ("token" = String, Path, description = "Share token"),
        ("size" = u32, Path, description = "Avatar size (512 or 1024)"),
    ),
    responses(
        (status = 200, description = "Share owner avatar image (WebP)"),
        (status = 403, description = "Password required"),
        (status = 404, description = "Share or avatar not found"),
    )
)]
pub async fn shared_avatar(
    state: web::Data<PrimaryAppState>,
    path: web::Path<(String, u32)>,
    req: actix_web::HttpRequest,
) -> Result<HttpResponse> {
    let (token, size) = path.into_inner();
    let cookie_value = share_cookie_value(&req, &token);
    share_service::check_share_password_cookie(&state, &token, cookie_value.as_deref()).await?;

    let bytes = share_service::get_share_avatar_bytes(&state, &token, size).await?;
    Ok(profile_service::avatar_image_response(bytes))
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/s/{token}/thumbnail",
    tag = "shares",
    operation_id = "shared_thumbnail",
    params(("token" = String, Path, description = "Share token")),
    responses(
        (status = 200, description = "Thumbnail image (WebP)"),
        (status = 202, description = "Thumbnail generation accepted"),
        (status = 304, description = "Thumbnail not modified"),
        (status = 400, description = "Thumbnail not supported for this file type"),
        (status = 403, description = "Password required"),
        (status = 412, description = "Storage backend is disabled or not ready"),
        (status = 404, description = "Share or file not found, or thumbnail unavailable"),
        (status = 500, description = "Unexpected thumbnail generation failure"),
    ),
)]
pub async fn shared_thumbnail(
    state: web::Data<PrimaryAppState>,
    path: web::Path<String>,
    req: actix_web::HttpRequest,
) -> Result<HttpResponse> {
    let cookie_value = share_cookie_value(&req, path.as_str());
    share_service::check_share_password_cookie(&state, &path, cookie_value.as_deref()).await?;

    let result = share_service::get_shared_thumbnail(&state, &path).await?;
    let if_none_match = req
        .headers()
        .get("If-None-Match")
        .and_then(|value| value.to_str().ok());

    match result {
        Some(result) => Ok(files::thumbnail_response(
            result,
            if_none_match,
            "public, max-age=0, must-revalidate".to_string(),
        )),
        None => Ok(thumbnail_pending_response()),
    }
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/s/{token}/files/{file_id}/thumbnail",
    tag = "shares",
    operation_id = "shared_folder_file_thumbnail",
    params(
        ("token" = String, Path, description = "Share token"),
        ("file_id" = i64, Path, description = "File ID inside shared folder")
    ),
    responses(
        (status = 200, description = "Thumbnail image (WebP)"),
        (status = 202, description = "Thumbnail generation accepted"),
        (status = 304, description = "Thumbnail not modified"),
        (status = 400, description = "Thumbnail not supported for this file type"),
        (status = 403, description = "Password required or file outside shared scope"),
        (status = 412, description = "Storage backend is disabled or not ready"),
        (status = 404, description = "Share or file not found, or thumbnail unavailable"),
        (status = 500, description = "Unexpected thumbnail generation failure"),
    )
)]
pub async fn shared_folder_file_thumbnail(
    state: web::Data<PrimaryAppState>,
    path: web::Path<(String, i64)>,
    req: actix_web::HttpRequest,
) -> Result<HttpResponse> {
    let (token, file_id) = path.into_inner();
    let cookie_value = share_cookie_value(&req, &token);
    share_service::check_share_password_cookie(&state, &token, cookie_value.as_deref()).await?;

    let result = share_service::get_shared_folder_file_thumbnail(&state, &token, file_id).await?;
    let if_none_match = req
        .headers()
        .get("If-None-Match")
        .and_then(|value| value.to_str().ok());

    match result {
        Some(result) => Ok(files::thumbnail_response(
            result,
            if_none_match,
            "public, max-age=0, must-revalidate".to_string(),
        )),
        None => Ok(thumbnail_pending_response()),
    }
}

#[cfg(test)]
mod tests {
    use super::direct_routes;
    use crate::config::RateLimitConfig;
    use actix_web::{App, HttpResponse, http::StatusCode, test, web};

    #[actix_web::test]
    async fn direct_routes_do_not_shadow_later_root_services() {
        let app = test::init_service(
            App::new()
                .service(direct_routes(&RateLimitConfig::default()))
                .route(
                    "/after",
                    web::get().to(|| async { HttpResponse::Ok().finish() }),
                ),
        )
        .await;

        let req = test::TestRequest::get().uri("/after").to_request();
        let resp = test::call_service(&app, req).await;

        assert_eq!(resp.status(), StatusCode::OK);
    }
}
