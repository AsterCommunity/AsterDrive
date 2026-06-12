//! API 路由：`share_public`。

use crate::api::api_error_code::ApiErrorCode;
use crate::api::dto::batch::ArchiveDownloadReq;
use crate::api::dto::files::ArchivePreviewQuery;
use crate::api::dto::share_public::DirectLinkQuery;
pub use crate::api::dto::share_public::VerifyPasswordReq;
use crate::api::dto::validate_request;
use crate::api::middleware::rate_limit;
use crate::api::pagination::FolderListQuery;
use crate::api::response::ApiResponse;
use crate::api::routes::files;
use crate::config::auth_runtime::RuntimeAuthPolicy;
use crate::config::operations;
use crate::config::{NetworkTrustConfig, RateLimitConfig};
use crate::errors::{Result, auth_forbidden_with_code};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::audit_service::AuditRequestInfo;
use crate::services::file_service::ResolvedDownloadRange;
use crate::services::{
    archive_preview_service, direct_link_service, file_service, media_metadata_service,
    preview_link_service, profile_service, share_service, share_stream_service,
    stream_ticket_service, task_service,
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

fn media_metadata_response(lookup: media_metadata_service::MediaMetadataLookup) -> HttpResponse {
    match lookup {
        media_metadata_service::MediaMetadataLookup::Ready(info) => {
            HttpResponse::Ok().json(ApiResponse::ok(info))
        }
        media_metadata_service::MediaMetadataLookup::Pending => HttpResponse::Accepted()
            .insert_header((header::RETRY_AFTER, "2"))
            .json(ApiResponse::<()>::ok_empty()),
    }
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

fn share_cookie_binding(
    req: &actix_web::HttpRequest,
    state: &PrimaryAppState,
) -> share_service::ShareCookieBinding {
    let request_info = AuditRequestInfo::from_request_with_trusted_proxies(
        req,
        &state.config().network_trust.trusted_proxies,
    );
    share_service::ShareCookieBinding::from_request_parts(
        request_info.user_agent.as_deref(),
        request_info.ip_address.as_deref(),
    )
}

async fn check_share_cookie(
    state: &PrimaryAppState,
    req: &actix_web::HttpRequest,
    token: &str,
) -> Result<()> {
    let cookie_value = share_cookie_value(req, token);
    let binding = share_cookie_binding(req, state);
    share_service::check_share_password_cookie(state, token, cookie_value.as_deref(), &binding)
        .await
}

async fn check_share_cookie_ignoring_download_limit(
    state: &PrimaryAppState,
    req: &actix_web::HttpRequest,
    token: &str,
) -> Result<()> {
    let cookie_value = share_cookie_value(req, token);
    let binding = share_cookie_binding(req, state);
    share_service::check_share_password_cookie_ignoring_download_limit(
        state,
        token,
        cookie_value.as_deref(),
        &binding,
    )
    .await
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

pub fn routes(
    rl: &RateLimitConfig,
    network_trust: &NetworkTrustConfig,
) -> impl actix_web::dev::HttpServiceFactory + use<> {
    let limiter = rate_limit::build_governor(&rl.public, &network_trust.trusted_proxies);
    let verify_limiter = rate_limit::build_governor(&rl.auth, &network_trust.trusted_proxies);

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
        .route(
            "/{token}/archive-download",
            web::post().to(archive_download),
        )
        .route(
            "/{token}/archive-download/{ticket}",
            web::get().to(archive_download_stream),
        )
        .route("/{token}/download", web::get().to(download_shared))
        .route(
            "/{token}/files/{file_id}/download",
            web::get().to(download_shared_folder_file_handler),
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
            "/{token}/media-metadata",
            web::get().to(shared_media_metadata),
        )
        .route(
            "/{token}/image-preview",
            web::get().to(shared_image_preview),
        )
        .route(
            "/{token}/files/{file_id}/thumbnail",
            web::get().to(shared_folder_file_thumbnail),
        )
        .route(
            "/{token}/files/{file_id}/media-metadata",
            web::get().to(shared_folder_file_media_metadata),
        )
        .route(
            "/{token}/files/{file_id}/image-preview",
            web::get().to(shared_folder_file_image_preview),
        )
        .route("/{token}/avatar/{size}", web::get().to(shared_avatar))
}

pub fn direct_routes(
    rl: &RateLimitConfig,
    network_trust: &NetworkTrustConfig,
) -> impl actix_web::dev::HttpServiceFactory + use<> {
    let limiter = rate_limit::build_governor(&rl.public, &network_trust.trusted_proxies);

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
    let info = share_service::get_share_info(state.get_ref(), &path).await?;
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
    req: actix_web::HttpRequest,
) -> Result<HttpResponse> {
    let binding = share_cookie_binding(&req, state.get_ref());
    let result =
        share_service::verify_password_and_sign(state.get_ref(), &path, &body.password, &binding)
            .await?;
    let auth_policy = RuntimeAuthPolicy::from_runtime_config(state.get_ref().runtime_config());
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
    check_share_cookie(state.get_ref(), &req, &token).await?;

    let (scheme, host) = request_origin_parts(&req);
    let link = preview_link_service::create_token_for_shared_file_for_origin(
        state.get_ref(),
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
    params(("token" = String, Path, description = "Share token"), ArchivePreviewQuery),
    responses(
        (status = 200, description = "Archive preview manifest", body = inline(ApiResponse<archive_preview_service::ArchivePreviewManifest>)),
        (status = 202, description = "Archive preview generation has been queued"),
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
    query: web::Query<ArchivePreviewQuery>,
) -> Result<HttpResponse> {
    let token = path.into_inner();
    check_share_cookie(state.get_ref(), &req, &token).await?;

    match archive_preview_service::preview_shared_file(
        state.get_ref(),
        &token,
        query.filename_encoding,
    )
    .await?
    {
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
    path = "/api/v1/s/{token}/archive-download",
    tag = "shares",
    operation_id = "create_shared_archive_download",
    params(("token" = String, Path, description = "Share token")),
    request_body = ArchiveDownloadReq,
    responses(
        (status = 200, description = "Shared archive download ticket", body = inline(ApiResponse<stream_ticket_service::StreamTicketInfo>)),
        (status = 400, description = "Invalid archive selection"),
        (status = 403, description = "Password required, download limit, file outside shared folder, or archive downloads disabled"),
        (status = 404, description = "Share not found"),
    ),
)]
pub async fn archive_download(
    state: web::Data<PrimaryAppState>,
    path: web::Path<String>,
    req: actix_web::HttpRequest,
    body: web::Json<ArchiveDownloadReq>,
) -> Result<HttpResponse> {
    ensure_share_archive_download_enabled(state.get_ref())?;
    let token = path.into_inner();
    check_share_cookie(state.get_ref(), &req, &token).await?;

    let body = body.into_inner();
    validate_request(&body)?;
    let ticket = stream_ticket_service::create_shared_archive_download_ticket(
        state.get_ref(),
        &token,
        &task_service::types::CreateArchiveTaskParams {
            file_ids: body.file_ids,
            folder_ids: body.folder_ids,
            archive_name: body.archive_name,
        },
    )
    .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(ticket)))
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/s/{token}/archive-download/{ticket}",
    tag = "shares",
    operation_id = "download_shared_archive",
    params(
        ("token" = String, Path, description = "Share token"),
        ("ticket" = String, Path, description = "Shared archive download ticket")
    ),
    responses(
        (status = 200, description = "Shared archive stream download"),
        (status = 400, description = "Invalid ticket"),
        (status = 403, description = "Password required, download limit, file outside shared folder, or archive downloads disabled"),
        (status = 404, description = "Share not found"),
    ),
)]
pub async fn archive_download_stream(
    state: web::Data<PrimaryAppState>,
    path: web::Path<(String, String)>,
    req: actix_web::HttpRequest,
) -> Result<HttpResponse> {
    ensure_share_archive_download_enabled(state.get_ref())?;
    let (token, ticket) = path.into_inner();
    check_share_cookie(state.get_ref(), &req, &token).await?;
    let params = stream_ticket_service::resolve_shared_archive_download_ticket(
        state.get_ref(),
        &token,
        &ticket,
    )
    .await?;
    task_service::archive::stream_shared_archive_download(state.get_ref(), &token, params).await
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
    check_share_cookie(state.get_ref(), &req, &token).await?;

    let (scheme, host) = request_origin_parts(&req);
    let session = share_stream_service::create_session_for_shared_file_for_origin(
        state.get_ref(),
        &token,
        preview_link_service::RequestOrigin {
            scheme: &scheme,
            host: &host,
        },
    )
    .await?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(session)))
}

fn ensure_share_archive_download_enabled(state: &PrimaryAppState) -> Result<()> {
    if !operations::archive_download_share_enabled(state.runtime_config()) {
        return Err(auth_forbidden_with_code(
            ApiErrorCode::ArchiveDownloadShareDisabled,
            "archive downloads for shared files are disabled by the administrator",
        ));
    }
    Ok(())
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
    check_share_cookie(state.get_ref(), &req, path.as_str()).await?;
    let range = shared_file_range(state.get_ref(), path.as_str(), &req).await?;
    let has_range = range.is_some();

    let outcome = file_service::record_download_result(
        state.get_ref(),
        "share",
        has_range,
        share_service::download_shared_file_with_range(
            state.get_ref(),
            &path,
            req.headers()
                .get("If-None-Match")
                .and_then(|v| v.to_str().ok()),
            range,
        ),
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
    let file =
        direct_link_service::resolve_file_for_download(state.get_ref(), &token, &filename).await?;
    let range = file_service::parse_range_header(req.headers().get(header::RANGE), file.size)?;
    let has_range = range.is_some();
    let outcome = file_service::record_download_result(
        state.get_ref(),
        "direct_link",
        has_range,
        direct_link_service::download_file(
            state.get_ref(),
            &token,
            &filename,
            query.force_download(),
            req.headers()
                .get("If-None-Match")
                .and_then(|v| v.to_str().ok()),
            range,
        ),
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
    let file =
        preview_link_service::resolve_file_for_download(state.get_ref(), &token, &filename).await?;
    let range = file_service::parse_range_header(req.headers().get(header::RANGE), file.size)?;
    let has_range = range.is_some();
    let outcome = file_service::record_download_result(
        state.get_ref(),
        "preview_link",
        has_range,
        preview_link_service::download_file(
            state.get_ref(),
            &token,
            &filename,
            req.headers()
                .get("If-None-Match")
                .and_then(|v| v.to_str().ok()),
            range,
        ),
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
    check_share_cookie_ignoring_download_limit(state.get_ref(), &req, &token).await?;
    let file = share_stream_service::resolve_file_for_stream(
        state.get_ref(),
        &token,
        &session_token,
        &filename,
    )
    .await?;
    let range = file_service::parse_range_header(req.headers().get(header::RANGE), file.size)?;
    let has_range = range.is_some();
    let outcome = file_service::record_download_result(
        state.get_ref(),
        "share_stream",
        has_range,
        share_stream_service::stream_file(
            state.get_ref(),
            &token,
            &session_token,
            &filename,
            range,
        ),
    )
    .await?;
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
pub async fn download_shared_folder_file_handler(
    state: web::Data<PrimaryAppState>,
    path: web::Path<(String, i64)>,
    req: actix_web::HttpRequest,
) -> Result<HttpResponse> {
    let (token, file_id) = path.into_inner();
    check_share_cookie(state.get_ref(), &req, &token).await?;
    let range = shared_folder_file_range(state.get_ref(), &token, file_id, &req).await?;
    let has_range = range.is_some();

    let outcome = file_service::record_download_result(
        state.get_ref(),
        "share",
        has_range,
        share_service::download_shared_folder_file_with_range(
            state.get_ref(),
            &token,
            file_id,
            req.headers()
                .get("If-None-Match")
                .and_then(|v| v.to_str().ok()),
            range,
        ),
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
    check_share_cookie(state.get_ref(), &req, &token).await?;

    let (scheme, host) = request_origin_parts(&req);
    let link = preview_link_service::create_token_for_shared_folder_file_for_origin(
        state.get_ref(),
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
        ("file_id" = i64, Path, description = "File ID inside shared folder"),
        ArchivePreviewQuery
    ),
    responses(
        (status = 200, description = "Archive preview manifest", body = inline(ApiResponse<archive_preview_service::ArchivePreviewManifest>)),
        (status = 202, description = "Archive preview generation has been queued"),
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
    query: web::Query<ArchivePreviewQuery>,
) -> Result<HttpResponse> {
    let (token, file_id) = path.into_inner();
    check_share_cookie(state.get_ref(), &req, &token).await?;

    match archive_preview_service::preview_shared_folder_file(
        state.get_ref(),
        &token,
        file_id,
        query.filename_encoding,
    )
    .await?
    {
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
    check_share_cookie(state.get_ref(), &req, &token).await?;

    let (scheme, host) = request_origin_parts(&req);
    let session = share_stream_service::create_session_for_shared_folder_file_for_origin(
        state.get_ref(),
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
    check_share_cookie(state.get_ref(), &req, path.as_str()).await?;

    let params = crate::services::folder_service::FolderListParams::from(&query.0);
    let contents = share_service::list_shared_folder(state.get_ref(), &path, &params).await?;
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
    check_share_cookie(state.get_ref(), &req, &token).await?;

    let params = crate::services::folder_service::FolderListParams::from(&query.0);
    let contents =
        share_service::list_shared_subfolder(state.get_ref(), &token, folder_id, &params).await?;
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
    check_share_cookie(state.get_ref(), &req, &token).await?;

    let bytes = share_service::get_share_avatar_bytes(state.get_ref(), &token, size).await?;
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
    check_share_cookie(state.get_ref(), &req, path.as_str()).await?;

    let result = share_service::get_shared_thumbnail(state.get_ref(), &path).await?;
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
    path = "/api/v1/s/{token}/image-preview",
    tag = "shares",
    operation_id = "shared_image_preview",
    params(("token" = String, Path, description = "Share token")),
    responses(
        (status = 200, description = "Image preview (WebP)"),
        (status = 202, description = "Image preview is being generated"),
        (status = 304, description = "Image preview not modified"),
        (status = 400, description = "Image preview not supported for this file type"),
        (status = 403, description = "Password required"),
        (status = 412, description = "Storage backend is disabled or not ready"),
        (status = 404, description = "Share or file not found"),
        (status = 500, description = "Unexpected image preview generation failure"),
    ),
)]
pub async fn shared_image_preview(
    state: web::Data<PrimaryAppState>,
    path: web::Path<String>,
    req: actix_web::HttpRequest,
) -> Result<HttpResponse> {
    let token = path.into_inner();
    check_share_cookie(state.get_ref(), &req, &token).await?;

    let result = share_service::get_shared_image_preview(state.get_ref(), &token).await?;
    let if_none_match = req
        .headers()
        .get("If-None-Match")
        .and_then(|value| value.to_str().ok());

    match result {
        Some(result) => Ok(files::image_preview_response(
            result,
            if_none_match,
            "public, max-age=0, must-revalidate".to_string(),
        )),
        None => Ok(thumbnail_pending_response()),
    }
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/s/{token}/media-metadata",
    tag = "shares",
    operation_id = "shared_file_media_metadata",
    params(("token" = String, Path, description = "Share token")),
    responses(
        (status = 200, description = "Blob media metadata", body = inline(ApiResponse<media_metadata_service::MediaMetadataInfo>)),
        (status = 202, description = "Media metadata extraction in progress"),
        (status = 403, description = "Password required"),
        (status = 404, description = "Share or file not found"),
    ),
)]
pub async fn shared_media_metadata(
    state: web::Data<PrimaryAppState>,
    path: web::Path<String>,
    req: actix_web::HttpRequest,
) -> Result<HttpResponse> {
    let token = path.into_inner();
    check_share_cookie(state.get_ref(), &req, &token).await?;

    let lookup = share_service::get_shared_media_metadata(state.get_ref(), &token).await?;
    Ok(media_metadata_response(lookup))
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
    check_share_cookie(state.get_ref(), &req, &token).await?;

    let result =
        share_service::get_shared_folder_file_thumbnail(state.get_ref(), &token, file_id).await?;
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
    path = "/api/v1/s/{token}/files/{file_id}/media-metadata",
    tag = "shares",
    operation_id = "shared_folder_file_media_metadata",
    params(
        ("token" = String, Path, description = "Share token"),
        ("file_id" = i64, Path, description = "File ID inside shared folder")
    ),
    responses(
        (status = 200, description = "Blob media metadata", body = inline(ApiResponse<media_metadata_service::MediaMetadataInfo>)),
        (status = 202, description = "Media metadata extraction in progress"),
        (status = 403, description = "Password required or file outside shared scope"),
        (status = 404, description = "Share or file not found"),
    )
)]
pub async fn shared_folder_file_media_metadata(
    state: web::Data<PrimaryAppState>,
    path: web::Path<(String, i64)>,
    req: actix_web::HttpRequest,
) -> Result<HttpResponse> {
    let (token, file_id) = path.into_inner();
    check_share_cookie(state.get_ref(), &req, &token).await?;

    let lookup =
        share_service::get_shared_folder_file_media_metadata(state.get_ref(), &token, file_id)
            .await?;
    Ok(media_metadata_response(lookup))
}

#[api_docs_macros::path(
    get,
    path = "/api/v1/s/{token}/files/{file_id}/image-preview",
    tag = "shares",
    operation_id = "shared_folder_file_image_preview",
    params(
        ("token" = String, Path, description = "Share token"),
        ("file_id" = i64, Path, description = "File ID inside shared folder")
    ),
    responses(
        (status = 200, description = "Image preview (WebP)"),
        (status = 202, description = "Image preview is being generated"),
        (status = 304, description = "Image preview not modified"),
        (status = 400, description = "Image preview not supported for this file type"),
        (status = 403, description = "Password required or file outside shared scope"),
        (status = 412, description = "Storage backend is disabled or not ready"),
        (status = 404, description = "Share or file not found"),
        (status = 500, description = "Unexpected image preview generation failure"),
    )
)]
pub async fn shared_folder_file_image_preview(
    state: web::Data<PrimaryAppState>,
    path: web::Path<(String, i64)>,
    req: actix_web::HttpRequest,
) -> Result<HttpResponse> {
    let (token, file_id) = path.into_inner();
    check_share_cookie(state.get_ref(), &req, &token).await?;

    let result =
        share_service::get_shared_folder_file_image_preview(state.get_ref(), &token, file_id)
            .await?;
    let if_none_match = req
        .headers()
        .get("If-None-Match")
        .and_then(|value| value.to_str().ok());

    match result {
        Some(result) => Ok(files::image_preview_response(
            result,
            if_none_match,
            "public, max-age=0, must-revalidate".to_string(),
        )),
        None => Ok(thumbnail_pending_response()),
    }
}

#[cfg(test)]
mod tests {
    use super::{direct_routes, routes};
    use crate::cache;
    use crate::config::{CacheConfig, Config, DatabaseConfig, NetworkTrustConfig, RateLimitConfig};
    use crate::db::repository::{background_task_repo, file_repo, folder_repo};
    use crate::entities::{file, file_blob, folder, storage_policy, user};
    use crate::runtime::{PrimaryAppState, SharedRuntimeState};
    use crate::services::{mail_service, media_processing_service, share_service};
    use crate::storage::drivers::local::LocalDriver;
    use crate::storage::{DriverRegistry, PolicySnapshot, StorageDriver};
    use crate::types::{
        BackgroundTaskKind, BackgroundTaskStatus, DriverType, StoredStoragePolicyAllowedTypes,
        StoredStoragePolicyOptions, UserRole, UserStatus,
    };
    use actix_web::body;
    use actix_web::http::{StatusCode, header};
    use actix_web::{App, HttpResponse, test, web};
    use chrono::Utc;
    use image::codecs::png::PngEncoder;
    use image::{ColorType, ImageEncoder};
    use migration::Migrator;
    use sea_orm::{ActiveModelTrait, Set};
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

    async fn build_share_image_preview_route_state()
    -> (PrimaryAppState, user::Model, file::Model, folder::Model) {
        let temp_root = std::env::temp_dir().join(format!(
            "asterdrive-share-image-preview-route-{}",
            uuid::Uuid::new_v4()
        ));
        tokio::fs::create_dir_all(&temp_root)
            .await
            .expect("share image preview route temp root should exist");

        let db = crate::db::connect_with_metrics(
            &DatabaseConfig {
                url: "sqlite::memory:".to_string(),
                pool_size: 1,
                retry_count: 0,
            },
            crate::metrics_core::NoopMetrics::arc(),
        )
        .await
        .expect("share image preview route database should connect");
        Migrator::up(&db, None)
            .await
            .expect("share image preview route migrations should succeed");

        let now = Utc::now();
        let storage_root = temp_root.join("storage");
        tokio::fs::create_dir_all(&storage_root)
            .await
            .expect("share image preview route storage root should exist");
        let policy = storage_policy::ActiveModel {
            name: Set("Share Image Preview Route Policy".to_string()),
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
        .expect("share image preview route policy should insert");

        let user = user::ActiveModel {
            username: Set("share-preview-route-user".to_string()),
            email: Set("share-preview-route@example.com".to_string()),
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
        .expect("share image preview route user should insert");

        let source_bytes = tiny_png();
        let source_hash = crate::utils::hash::sha256_hex(&source_bytes);
        let driver = Arc::new(
            LocalDriver::new(&policy).expect("share image preview route local driver should build"),
        );
        let source_path = "objects/source.png";
        driver
            .put(source_path, &source_bytes)
            .await
            .expect("share image preview route source object should write");
        let blob = file_repo::create_blob(
            &db,
            file_blob::ActiveModel {
                hash: Set(source_hash),
                size: Set(crate::utils::numbers::usize_to_i64(
                    source_bytes.len(),
                    "share image preview route source size",
                )
                .expect("share image preview route source size should fit i64")),
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
        .expect("share image preview route blob should insert");

        let folder = folder_repo::create(
            &db,
            folder::ActiveModel {
                name: Set("shared-folder".to_string()),
                parent_id: Set(None),
                team_id: Set(None),
                owner_user_id: Set(Some(user.id)),
                created_by_user_id: Set(Some(user.id)),
                created_by_username: Set(user.username.clone()),
                policy_id: Set(None),
                created_at: Set(now),
                updated_at: Set(now),
                deleted_at: Set(None),
                is_locked: Set(false),
                ..Default::default()
            },
        )
        .await
        .expect("share image preview route folder should insert");

        let file = file_repo::create_with_blob(
            &db,
            file_repo::CreateFileWithBlobInput {
                name: "source.png",
                folder_id: Some(folder.id),
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
        .expect("share image preview route file should insert");

        let policy_snapshot = Arc::new(PolicySnapshot::new());
        policy_snapshot
            .reload(&db)
            .await
            .expect("share image preview route policy snapshot should reload");
        let driver_registry = Arc::new(DriverRegistry::noop());
        driver_registry.insert_for_test(policy.id, driver);

        let runtime_config = Arc::new(crate::config::RuntimeConfig::new());
        let cache = cache::create_cache(&CacheConfig {
            enabled: false,
            ..Default::default()
        })
        .await;
        let mut config = Config::default();
        config.server.temp_dir = temp_root.join(".tmp").to_string_lossy().into_owned();
        config.server.upload_temp_dir = temp_root.join(".uploads").to_string_lossy().into_owned();

        let (storage_change_tx, _) = tokio::sync::broadcast::channel(
            crate::services::storage_change_service::STORAGE_CHANGE_CHANNEL_CAPACITY,
        );
        let share_download_rollback =
            crate::services::share_service::spawn_detached_share_download_rollback_queue(
                db.clone(),
                crate::config::operations::share_download_rollback_queue_capacity(&runtime_config),
            );

        let state = PrimaryAppState {
            db_handles: crate::db::DbHandles::single(db),
            driver_registry,
            runtime_config: runtime_config.clone(),
            policy_snapshot,
            config: Arc::new(config),
            cache,
            metrics: crate::metrics_core::NoopMetrics::arc(),
            mail_sender: mail_service::runtime_sender(runtime_config),
            storage_change_tx,
            share_download_rollback,
            background_task_dispatch_wakeup:
                crate::runtime::PrimaryAppState::new_background_task_dispatch_wakeup(),
            remote_protocol: crate::runtime::PrimaryAppState::new_remote_protocol(),
        };

        (state, user, file, folder)
    }

    async fn init_share_app(
        state: PrimaryAppState,
    ) -> impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    > {
        test::init_service(App::new().app_data(web::Data::new(state)).service(
            web::scope("/api/v1").service(routes(
                &RateLimitConfig {
                    enabled: false,
                    ..Default::default()
                },
                &NetworkTrustConfig::default(),
            )),
        ))
        .await
    }

    #[actix_web::test]
    async fn direct_routes_do_not_shadow_later_root_services() {
        let app = test::init_service(
            App::new()
                .service(direct_routes(
                    &RateLimitConfig::default(),
                    &NetworkTrustConfig::default(),
                ))
                .route(
                    "/",
                    web::get().to(|| async { HttpResponse::Ok().body("root") }),
                )
                .route(
                    "/after",
                    web::get().to(|| async { HttpResponse::Ok().finish() }),
                ),
        )
        .await;

        let root_req = test::TestRequest::get().uri("/").to_request();
        let root_resp = test::call_service(&app, root_req).await;
        assert_eq!(root_resp.status(), StatusCode::OK);
        let root_body = body::to_bytes(root_resp.into_body())
            .await
            .expect("root response body should be readable");
        assert_eq!(root_body.as_ref(), b"root");

        let after_req = test::TestRequest::get().uri("/after").to_request();
        let after_resp = test::call_service(&app, after_req).await;
        assert_eq!(after_resp.status(), StatusCode::OK);
    }

    #[actix_web::test]
    async fn shared_image_preview_enqueues_background_task_on_cache_miss() {
        let (state, user, file, _) = build_share_image_preview_route_state().await;
        let share = share_service::create_share(
            &state,
            user.id,
            share_service::ShareTarget::file(file.id),
            None,
            None,
            0,
        )
        .await
        .expect("file share should create");
        let app = init_share_app(state.clone()).await;

        let response = test::call_service(
            &app,
            test::TestRequest::get()
                .uri(&format!("/api/v1/s/{}/image-preview", share.token))
                .to_request(),
        )
        .await;

        assert_eq!(response.status(), StatusCode::ACCEPTED);
        assert_eq!(response.headers().get(header::RETRY_AFTER).unwrap(), "2");
        let task = background_task_repo::find_latest_by_kind_and_display_name(
            state.writer_db(),
            BackgroundTaskKind::ImagePreviewGenerate,
            &format!(
                "Generate image preview for blob #{} via AsterDrive built-in",
                file.blob_id
            ),
        )
        .await
        .expect("image preview task lookup should succeed")
        .expect("image preview cache miss should enqueue a task");
        assert_eq!(task.status, BackgroundTaskStatus::Pending);
        let body = body::to_bytes(response.into_body())
            .await
            .expect("shared image preview 202 body should read");
        assert!(body.is_empty());
    }

    #[actix_web::test]
    async fn shared_image_preview_returns_cached_webp_and_honors_if_none_match() {
        let (state, user, file, _) = build_share_image_preview_route_state().await;
        let share = share_service::create_share(
            &state,
            user.id,
            share_service::ShareTarget::file(file.id),
            None,
            None,
            0,
        )
        .await
        .expect("file share should create");
        let blob = file_repo::find_blob_by_id(state.writer_db(), file.blob_id)
            .await
            .expect("share image preview route blob should load");
        media_processing_service::generate_and_store_image_preview(
            &state,
            &blob,
            &file.name,
            &file.mime_type,
        )
        .await
        .expect("share image preview route cache should pre-generate");
        let app = init_share_app(state.clone()).await;

        let response = test::call_service(
            &app,
            test::TestRequest::get()
                .uri(&format!("/api/v1/s/{}/image-preview", share.token))
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
            "public, max-age=0, must-revalidate"
        );
        let etag = response
            .headers()
            .get(header::ETAG)
            .expect("shared image preview response should include ETag")
            .to_str()
            .expect("shared image preview ETag should be valid header")
            .to_string();
        let expected_etag = format!(
            "\"{}\"",
            media_processing_service::image_preview_etag_value_for(
                &image_preview_blob_hash(),
                crate::services::thumbnail_service::IMAGES_THUMBNAIL_PROCESSOR_NAMESPACE,
                crate::services::thumbnail_service::CURRENT_IMAGE_PREVIEW_VERSION,
            )
        );
        assert_eq!(etag, expected_etag);
        let body = body::to_bytes(response.into_body())
            .await
            .expect("shared image preview route response body should read");
        assert!(!body.is_empty());

        let not_modified = test::call_service(
            &app,
            test::TestRequest::get()
                .uri(&format!("/api/v1/s/{}/image-preview", share.token))
                .insert_header((header::IF_NONE_MATCH, etag))
                .to_request(),
        )
        .await;

        assert_eq!(not_modified.status(), StatusCode::NOT_MODIFIED);
        assert_eq!(
            not_modified.headers().get(header::CACHE_CONTROL).unwrap(),
            "public, max-age=0, must-revalidate"
        );
        let body = body::to_bytes(not_modified.into_body())
            .await
            .expect("shared image preview route 304 response body should read");
        assert!(body.is_empty());
    }

    #[actix_web::test]
    async fn shared_folder_file_image_preview_enqueues_background_task_on_cache_miss() {
        let (state, user, file, folder) = build_share_image_preview_route_state().await;
        let share = share_service::create_share(
            &state,
            user.id,
            share_service::ShareTarget::folder(folder.id),
            None,
            None,
            0,
        )
        .await
        .expect("folder share should create");
        let app = init_share_app(state.clone()).await;

        let response = test::call_service(
            &app,
            test::TestRequest::get()
                .uri(&format!(
                    "/api/v1/s/{}/files/{}/image-preview",
                    share.token, file.id
                ))
                .to_request(),
        )
        .await;

        assert_eq!(response.status(), StatusCode::ACCEPTED);
        assert_eq!(response.headers().get(header::RETRY_AFTER).unwrap(), "2");
        let task = background_task_repo::find_latest_by_kind_and_display_name(
            state.writer_db(),
            BackgroundTaskKind::ImagePreviewGenerate,
            &format!(
                "Generate image preview for blob #{} via AsterDrive built-in",
                file.blob_id
            ),
        )
        .await
        .expect("folder image preview task lookup should succeed")
        .expect("folder image preview cache miss should enqueue a task");
        assert_eq!(task.status, BackgroundTaskStatus::Pending);
    }
}
