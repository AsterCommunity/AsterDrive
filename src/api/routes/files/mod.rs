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
    download, get_archive_preview, get_direct_link, get_file, get_image_preview,
    get_media_metadata, get_preview_link, get_thumbnail, open_wopi, resolve_resource_handle,
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
pub use crate::api::dto::files::{ArchivePreviewQuery, OpenWopiRequest, VersionPath};

pub(crate) use self::access::{
    archive_preview_manifest_response, archive_preview_pending_response, image_preview_response,
    team_download, team_get_archive_preview, team_get_direct_link, team_get_file,
    team_get_image_preview, team_get_media_metadata, team_get_preview_link, team_get_thumbnail,
    team_open_wopi, team_resolve_resource_handle, thumbnail_response,
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
        // Web app upload drop-zone fallback: direct browser-to-server upload.
        .route("/upload", web::post().to(upload))
        // Web app "new file" action for empty text/document placeholders.
        .route("/new", web::post().to(create_empty))
        // Web app resumable upload bootstrap; placed before /{id} to avoid conflicts.
        .route("/upload/init", web::post().to(init_chunked_upload))
        // Web app upload recovery list, used after reload or tab restore.
        .route(
            "/upload/sessions",
            web::get().to(list_recoverable_upload_sessions),
        )
        // Chunked upload data plane: browser sends one persisted chunk.
        .route(
            "/upload/{upload_id}/{chunk_number}",
            web::put().to(upload_chunk),
        )
        // Chunked upload finalization called by the web upload queue.
        .route(
            "/upload/{upload_id}/complete",
            web::post().to(complete_upload),
        )
        // Multipart/presigned upload part negotiation for capable storage policies.
        .route(
            "/upload/{upload_id}/presign-parts",
            web::post().to(presign_parts),
        )
        // Web app upload progress polling and recovery.
        .route("/upload/{upload_id}", web::get().to(get_upload_progress))
        // Web app upload cancellation.
        .route("/upload/{upload_id}", web::delete().to(cancel_upload))
        // Web app file detail panel and store refresh.
        .route("/{id}", web::get().to(get_file))
        // Archive preview panel manifest; not for raw file bytes.
        .route("/{id}/archive-preview", web::get().to(get_archive_preview))
        // User-created public direct download link.
        .route("/{id}/direct-link", web::get().to(get_direct_link))
        // External viewers and URL-template preview apps that need a browser-addressable short link.
        .route("/{id}/preview-link", web::post().to(get_preview_link))
        // Web preview resolver: authoritative URL/credentials/cache identity for in-app previews.
        .route(
            "/{id}/resource-handle",
            web::post().to(resolve_resource_handle),
        )
        // Office/WOPI launch session creation.
        .route("/{id}/wopi/open", web::post().to(open_wopi))
        // Browser download and resolver-generated original-content preview entry.
        .route("/{id}/download", web::get().to(download))
        // File list/card thumbnails.
        .route("/{id}/thumbnail", web::get().to(get_thumbnail))
        // Converted medium image preview for HEIC/RAW/non-browser-renderable images.
        .route("/{id}/image-preview", web::get().to(get_image_preview))
        // Audio/video/image metadata panel and media preview preparation.
        .route("/{id}/media-metadata", web::get().to(get_media_metadata))
        // In-app editable text/code save path.
        .route("/{id}/content", web::put().to(update_content))
        // Archive extraction task creation.
        .route("/{id}/extract", web::post().to(extract_archive))
        // Manual file lock toggle from the web app.
        .route("/{id}/lock", web::post().to(set_lock))
        // Web app copy action.
        .route("/{id}/copy", web::post().to(copy_file))
        // Version history panel.
        .route("/{id}/versions", web::get().to(list_versions))
        // Version history restore action.
        .route(
            "/{id}/versions/{version_id}/restore",
            web::post().to(restore_version),
        )
        // Version history delete action.
        .route(
            "/{id}/versions/{version_id}",
            web::delete().to(delete_version),
        )
        // Web app delete-to-trash action.
        .route("/{id}", web::delete().to(delete_file))
        // Web app rename/move metadata patch.
        .route("/{id}", web::patch().to(patch_file))
}

pub fn team_routes() -> actix_web::Scope {
    web::scope("/files")
        // Team workspace variants mirror personal file routes for team-scoped callers.
        .route("/upload", web::post().to(team_upload))
        // Team resumable upload bootstrap.
        .route("/upload/init", web::post().to(team_init_chunked_upload))
        // Team upload recovery list.
        .route(
            "/upload/sessions",
            web::get().to(team_list_recoverable_upload_sessions),
        )
        // Team chunked upload data plane.
        .route(
            "/upload/{upload_id}/{chunk_number}",
            web::put().to(team_upload_chunk),
        )
        // Team chunked upload finalization.
        .route(
            "/upload/{upload_id}/complete",
            web::post().to(team_complete_upload),
        )
        // Team multipart/presigned upload part negotiation.
        .route(
            "/upload/{upload_id}/presign-parts",
            web::post().to(team_presign_parts),
        )
        // Team upload progress polling and recovery.
        .route(
            "/upload/{upload_id}",
            web::get().to(team_get_upload_progress),
        )
        // Team upload cancellation.
        .route("/upload/{upload_id}", web::delete().to(team_cancel_upload))
        // Team "new empty file" action.
        .route("/new", web::post().to(team_create_empty))
        // Team file detail panel and store refresh.
        .route("/{id}", web::get().to(team_get_file))
        // Team archive preview manifest.
        .route(
            "/{id}/archive-preview",
            web::get().to(team_get_archive_preview),
        )
        // Team user-created public direct download link.
        .route("/{id}/direct-link", web::get().to(team_get_direct_link))
        // Team external viewers and URL-template preview apps.
        .route("/{id}/preview-link", web::post().to(team_get_preview_link))
        // Team web preview resolver.
        .route(
            "/{id}/resource-handle",
            web::post().to(team_resolve_resource_handle),
        )
        // Team Office/WOPI launch session creation.
        .route("/{id}/wopi/open", web::post().to(team_open_wopi))
        // Team file list/card thumbnails.
        .route("/{id}/thumbnail", web::get().to(team_get_thumbnail))
        // Team converted medium image preview for HEIC/RAW/non-browser-renderable images.
        .route("/{id}/image-preview", web::get().to(team_get_image_preview))
        // Team audio/video/image metadata panel and media preview preparation.
        .route(
            "/{id}/media-metadata",
            web::get().to(team_get_media_metadata),
        )
        // Team in-app editable text/code save path.
        .route("/{id}/content", web::put().to(team_update_content))
        // Team archive extraction task creation.
        .route("/{id}/extract", web::post().to(team_extract_archive))
        // Team manual file lock toggle.
        .route("/{id}/lock", web::post().to(team_set_lock))
        // Team rename/move metadata patch.
        .route("/{id}", web::patch().to(team_patch_file))
        // Team delete-to-trash action.
        .route("/{id}", web::delete().to(team_delete_file))
        // Team copy action.
        .route("/{id}/copy", web::post().to(team_copy_file))
        // Team version history panel.
        .route("/{id}/versions", web::get().to(team_list_versions))
        // Team version history restore action.
        .route(
            "/{id}/versions/{version_id}/restore",
            web::post().to(team_restore_version),
        )
        // Team version history delete action.
        .route(
            "/{id}/versions/{version_id}",
            web::delete().to(team_delete_version),
        )
        // Team browser download and resolver-generated original-content preview entry.
        .route("/{id}/download", web::get().to(team_download))
}
