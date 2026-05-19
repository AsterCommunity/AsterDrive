//! 文件 API 路由聚合入口。

use crate::api::middleware::auth::JwtAuth;
use crate::api::middleware::rate_limit;
use crate::config::{NetworkTrustConfig, RateLimitConfig};
use actix_governor::Governor;
use actix_web::middleware::Condition;
use actix_web::web;

pub mod access;
pub mod mutations;
pub mod upload;
pub mod versions;

pub use self::access::{
    download, get_archive_preview, get_direct_link, get_file, get_image_preview, get_preview_link,
    get_thumbnail, open_wopi,
};
pub use self::mutations::{
    CopyFileReq, CreateEmptyRequest, ExtractArchiveRequest, PatchFileReq, SetLockReq, copy_file,
    create_empty, delete_file, extract_archive, patch_file, set_lock, update_content,
};
pub use self::upload::{
    ChunkPath, CompleteUploadReq, CompletedPartReq, FileQuery, InitUploadReq, PresignPartsReq,
    UploadIdPath, cancel_upload, complete_upload, get_upload_progress, init_chunked_upload,
    list_recoverable_upload_sessions, presign_parts, upload, upload_chunk,
};
pub use self::versions::{delete_version, list_versions, restore_version};
// DTO types that need explicit import (not re-exported from submodules)
pub use crate::api::dto::files::{OpenWopiRequest, VersionPath};

pub(crate) use self::access::{
    archive_preview_manifest_response, archive_preview_pending_response, image_preview_response,
    team_download, team_get_archive_preview, team_get_direct_link, team_get_file,
    team_get_image_preview, team_get_preview_link, team_get_thumbnail, team_open_wopi,
    thumbnail_response,
};
pub(crate) use self::mutations::{
    team_copy_file, team_create_empty, team_delete_file, team_extract_archive, team_patch_file,
    team_set_lock, team_update_content,
};
pub(crate) use self::upload::{
    team_cancel_upload, team_complete_upload, team_get_upload_progress, team_init_chunked_upload,
    team_list_recoverable_upload_sessions, team_presign_parts, team_upload, team_upload_chunk,
};
pub(crate) use self::versions::{team_delete_version, team_list_versions, team_restore_version};

pub fn routes(
    rl: &RateLimitConfig,
    network_trust: &NetworkTrustConfig,
) -> impl actix_web::dev::HttpServiceFactory + use<> {
    let limiter = rate_limit::build_governor(&rl.api, &network_trust.trusted_proxies);

    web::scope("/files")
        .wrap(JwtAuth)
        .wrap(Condition::new(rl.enabled, Governor::new(&limiter)))
        .route("/upload", web::post().to(upload))
        .route("/new", web::post().to(create_empty))
        // chunked upload routes (before /{id} to avoid conflicts)
        .route("/upload/init", web::post().to(init_chunked_upload))
        .route(
            "/upload/sessions",
            web::get().to(list_recoverable_upload_sessions),
        )
        .route(
            "/upload/{upload_id}/{chunk_number}",
            web::put().to(upload_chunk),
        )
        .route(
            "/upload/{upload_id}/complete",
            web::post().to(complete_upload),
        )
        .route(
            "/upload/{upload_id}/presign-parts",
            web::post().to(presign_parts),
        )
        .route("/upload/{upload_id}", web::get().to(get_upload_progress))
        .route("/upload/{upload_id}", web::delete().to(cancel_upload))
        .route("/{id}", web::get().to(get_file))
        .route("/{id}/archive-preview", web::get().to(get_archive_preview))
        .route("/{id}/direct-link", web::get().to(get_direct_link))
        .route("/{id}/preview-link", web::post().to(get_preview_link))
        .route("/{id}/wopi/open", web::post().to(open_wopi))
        .route("/{id}/download", web::get().to(download))
        .route("/{id}/thumbnail", web::get().to(get_thumbnail))
        .route("/{id}/image-preview", web::get().to(get_image_preview))
        .route("/{id}/content", web::put().to(update_content))
        .route("/{id}/extract", web::post().to(extract_archive))
        .route("/{id}/lock", web::post().to(set_lock))
        .route("/{id}/copy", web::post().to(copy_file))
        .route("/{id}/versions", web::get().to(list_versions))
        .route(
            "/{id}/versions/{version_id}/restore",
            web::post().to(restore_version),
        )
        .route(
            "/{id}/versions/{version_id}",
            web::delete().to(delete_version),
        )
        .route("/{id}", web::delete().to(delete_file))
        .route("/{id}", web::patch().to(patch_file))
}

pub fn team_routes() -> actix_web::Scope {
    web::scope("/files")
        .route("/upload", web::post().to(team_upload))
        .route("/upload/init", web::post().to(team_init_chunked_upload))
        .route(
            "/upload/sessions",
            web::get().to(team_list_recoverable_upload_sessions),
        )
        .route(
            "/upload/{upload_id}/{chunk_number}",
            web::put().to(team_upload_chunk),
        )
        .route(
            "/upload/{upload_id}/complete",
            web::post().to(team_complete_upload),
        )
        .route(
            "/upload/{upload_id}/presign-parts",
            web::post().to(team_presign_parts),
        )
        .route(
            "/upload/{upload_id}",
            web::get().to(team_get_upload_progress),
        )
        .route("/upload/{upload_id}", web::delete().to(team_cancel_upload))
        .route("/new", web::post().to(team_create_empty))
        .route("/{id}", web::get().to(team_get_file))
        .route(
            "/{id}/archive-preview",
            web::get().to(team_get_archive_preview),
        )
        .route("/{id}/direct-link", web::get().to(team_get_direct_link))
        .route("/{id}/preview-link", web::post().to(team_get_preview_link))
        .route("/{id}/wopi/open", web::post().to(team_open_wopi))
        .route("/{id}/thumbnail", web::get().to(team_get_thumbnail))
        .route("/{id}/image-preview", web::get().to(team_get_image_preview))
        .route("/{id}/content", web::put().to(team_update_content))
        .route("/{id}/extract", web::post().to(team_extract_archive))
        .route("/{id}/lock", web::post().to(team_set_lock))
        .route("/{id}", web::patch().to(team_patch_file))
        .route("/{id}", web::delete().to(team_delete_file))
        .route("/{id}/copy", web::post().to(team_copy_file))
        .route("/{id}/versions", web::get().to(team_list_versions))
        .route(
            "/{id}/versions/{version_id}/restore",
            web::post().to(team_restore_version),
        )
        .route(
            "/{id}/versions/{version_id}",
            web::delete().to(team_delete_version),
        )
        .route("/{id}/download", web::get().to(team_download))
}
