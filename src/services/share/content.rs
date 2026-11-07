//! 分享服务子模块：`content`。

use actix_web::http::header::HeaderValue;

use crate::api::api_error_code::ApiErrorCode;
use crate::db::repository::{file_repo, folder_repo, share_repo};
use crate::errors::{AsterError, Result, auth_forbidden_with_code};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::{
    files::{file as file_ops, folder},
    media::metadata,
    media::processing,
    task,
};
use aster_drive_model::entities::{file, share};

use super::cache::{
    invalidate_active_share_target_cache_for_share, invalidate_share_token_record_cache_for_share,
};
use super::shared::{
    load_share_file_resource, load_shared_folder_file_target,
    load_shared_folder_file_target_ignoring_download_limit, load_shared_subfolder_target,
    load_usable_share_ignoring_download_limit, load_valid_folder_share_root, load_valid_share,
};

pub(crate) async fn initialize_share_download(
    state: &impl SharedRuntimeState,
    share: &share::Model,
) -> Result<()> {
    match share_repo::increment_download_count(state.writer_db(), share.id).await {
        Ok(true) => {
            invalidate_share_token_record_cache_for_share(state, share).await;
            if share.max_downloads > 0
                && share.download_count.saturating_add(1) >= share.max_downloads
            {
                invalidate_active_share_target_cache_for_share(state, share).await;
            }
            Ok(())
        }
        Ok(false) => Err(AsterError::share_download_limit("download limit reached")),
        Err(error) => {
            tracing::warn!(
                share_id = share.id,
                "failed to increment download count: {error}"
            );
            Err(error)
        }
    }
}

pub async fn download_shared_file_with_range_header(
    state: &PrimaryAppState,
    token: &str,
    if_none_match: Option<&str>,
    range_header: Option<&HeaderValue>,
) -> Result<file_ops::DownloadOutcome> {
    download_shared_file_with_disposition_and_range_header(
        state,
        token,
        file_ops::DownloadDisposition::Attachment,
        if_none_match,
        range_header,
    )
    .await
}

pub(crate) async fn download_shared_file_with_disposition_and_range_header(
    state: &PrimaryAppState,
    token: &str,
    disposition: file_ops::DownloadDisposition,
    if_none_match: Option<&str>,
    range_header: Option<&HeaderValue>,
) -> Result<file_ops::DownloadOutcome> {
    let share = load_usable_share_ignoring_download_limit(state, token).await?;
    let file = load_share_file_resource(state, &share).await?;
    download_share_resource_with_disposition(
        state,
        &share,
        &file,
        disposition,
        if_none_match,
        range_header,
    )
    .await
}

pub async fn download_shared_folder_file_with_range_header(
    state: &PrimaryAppState,
    token: &str,
    file_id: i64,
    if_none_match: Option<&str>,
    range_header: Option<&HeaderValue>,
) -> Result<file_ops::DownloadOutcome> {
    download_shared_folder_file_with_disposition_and_range_header(
        state,
        token,
        file_id,
        file_ops::DownloadDisposition::Attachment,
        if_none_match,
        range_header,
    )
    .await
}

pub(crate) async fn download_shared_folder_file_with_disposition_and_range_header(
    state: &PrimaryAppState,
    token: &str,
    file_id: i64,
    disposition: file_ops::DownloadDisposition,
    if_none_match: Option<&str>,
    range_header: Option<&HeaderValue>,
) -> Result<file_ops::DownloadOutcome> {
    let (share, file) =
        load_shared_folder_file_target_ignoring_download_limit(state, token, file_id).await?;
    download_share_resource_with_disposition(
        state,
        &share,
        &file,
        disposition,
        if_none_match,
        range_header,
    )
    .await
}

pub async fn list_shared_folder(
    state: &impl SharedRuntimeState,
    token: &str,
    params: &folder::FolderListParams,
) -> Result<folder::FolderContents> {
    let (_, folder_id) = load_valid_folder_share_root(state, token).await?;
    tracing::debug!(
        folder_id,
        folder_limit = params.folder_limit,
        folder_offset = params.folder_offset,
        file_limit = params.file_limit,
        has_file_cursor = params.file_cursor.is_some(),
        sort_by = ?params.sort_by,
        sort_order = ?params.sort_order,
        "listing shared folder root"
    );

    let contents = folder::list_shared(state, folder_id, params).await?;
    tracing::debug!(
        folder_id,
        folders_total = contents.folders_total,
        files_total = contents.files_total,
        returned_folders = contents.folders.len(),
        returned_files = contents.files.len(),
        "listed shared folder root"
    );
    Ok(contents)
}

async fn load_or_enqueue_thumbnail(
    state: &PrimaryAppState,
    file: &file::Model,
) -> Result<Option<file_ops::ThumbnailResult>> {
    let blob = file_repo::find_blob_by_id(state.writer_db(), file.blob_id).await?;
    let thumbnail = processing::load_thumbnail_if_exists(state, &blob, &file.name, &file.mime_type)
        .await
        .map_err(processing::map_thumbnail_request_error)?;

    match thumbnail {
        Some(thumbnail) => Ok(Some(file_ops::ThumbnailResult {
            data: thumbnail.data,
            blob_hash: blob.hash,
            thumbnail_processor: Some(thumbnail.thumbnail_processor),
            thumbnail_version: Some(thumbnail.thumbnail_version),
        })),
        None => {
            task::thumbnail::ensure_thumbnail_task(state, &blob, &file.name, &file.mime_type)
                .await
                .map_err(processing::map_thumbnail_request_error)?;
            Ok(None)
        }
    }
}

pub async fn get_shared_thumbnail(
    state: &PrimaryAppState,
    token: &str,
) -> Result<Option<file_ops::ThumbnailResult>> {
    let share = load_valid_share(state, token).await?;
    tracing::debug!(share_id = share.id, "loading shared thumbnail");
    let file = load_share_file_resource(state, &share).await?;
    let thumbnail = load_or_enqueue_thumbnail(state, &file).await?;
    tracing::debug!(
        share_id = share.id,
        file_id = file.id,
        ready = thumbnail.is_some(),
        "loaded shared thumbnail state"
    );
    Ok(thumbnail)
}

pub async fn get_shared_image_preview(
    state: &PrimaryAppState,
    token: &str,
) -> Result<Option<file_ops::ImagePreviewResult>> {
    let share = load_valid_share(state, token).await?;
    tracing::debug!(share_id = share.id, "loading shared image preview");
    let file = load_share_file_resource(state, &share).await?;
    file_ops::image_preview_for_file(state, &file).await
}

pub async fn get_shared_media_metadata(
    state: &PrimaryAppState,
    token: &str,
) -> Result<metadata::MediaMetadataLookup> {
    let share = load_valid_share(state, token).await?;
    tracing::debug!(share_id = share.id, "loading shared media metadata");
    let file = load_share_file_resource(state, &share).await?;
    let metadata = metadata::get_for_file(state, &file).await?;
    tracing::debug!(
        share_id = share.id,
        file_id = file.id,
        pending = matches!(metadata, metadata::MediaMetadataLookup::Pending),
        "loaded shared media metadata state"
    );
    Ok(metadata)
}

pub async fn get_shared_folder_file_thumbnail(
    state: &PrimaryAppState,
    token: &str,
    file_id: i64,
) -> Result<Option<file_ops::ThumbnailResult>> {
    let (_, file) = load_shared_folder_file_target(state, token, file_id).await?;
    tracing::debug!(file_id = file.id, "loading shared folder file thumbnail");

    let thumbnail = load_or_enqueue_thumbnail(state, &file).await?;
    tracing::debug!(
        file_id = file.id,
        ready = thumbnail.is_some(),
        "loaded shared folder file thumbnail state"
    );
    Ok(thumbnail)
}

pub async fn get_shared_folder_file_image_preview(
    state: &PrimaryAppState,
    token: &str,
    file_id: i64,
) -> Result<Option<file_ops::ImagePreviewResult>> {
    let (_, file) = load_shared_folder_file_target(state, token, file_id).await?;
    tracing::debug!(
        file_id = file.id,
        "loading shared folder file image preview"
    );
    file_ops::image_preview_for_file(state, &file).await
}

pub async fn get_shared_folder_file_media_metadata(
    state: &PrimaryAppState,
    token: &str,
    file_id: i64,
) -> Result<metadata::MediaMetadataLookup> {
    let (_, file) = load_shared_folder_file_target(state, token, file_id).await?;
    tracing::debug!(
        file_id = file.id,
        "loading shared folder file media metadata"
    );
    let metadata = metadata::get_for_file(state, &file).await?;
    tracing::debug!(
        file_id = file.id,
        pending = matches!(metadata, metadata::MediaMetadataLookup::Pending),
        "loaded shared folder file media metadata state"
    );
    Ok(metadata)
}

pub(crate) async fn load_preview_shared_file(
    state: &impl SharedRuntimeState,
    token: &str,
) -> Result<(share::Model, aster_drive_model::entities::file::Model)> {
    let share = load_valid_share(state, token).await?;
    let file = load_share_file_resource(state, &share).await?;
    Ok((share, file))
}

pub(crate) async fn load_preview_shared_folder_file(
    state: &impl SharedRuntimeState,
    token: &str,
    file_id: i64,
) -> Result<(share::Model, aster_drive_model::entities::file::Model)> {
    load_shared_folder_file_target(state, token, file_id).await
}

pub(crate) async fn load_shared_file_ignoring_download_limit(
    state: &impl SharedRuntimeState,
    token: &str,
) -> Result<(share::Model, aster_drive_model::entities::file::Model)> {
    let share = load_usable_share_ignoring_download_limit(state, token).await?;
    let file = load_share_file_resource(state, &share).await?;
    Ok((share, file))
}

pub(crate) async fn load_shared_folder_file_ignoring_download_limit(
    state: &impl SharedRuntimeState,
    token: &str,
    file_id: i64,
) -> Result<(share::Model, aster_drive_model::entities::file::Model)> {
    load_shared_folder_file_target_ignoring_download_limit(state, token, file_id).await
}

pub async fn list_shared_subfolder(
    state: &impl SharedRuntimeState,
    token: &str,
    folder_id: i64,
    params: &folder::FolderListParams,
) -> Result<folder::FolderContents> {
    let (_, target) = load_shared_subfolder_target(state, token, folder_id).await?;
    tracing::debug!(
        folder_id = target.id,
        folder_limit = params.folder_limit,
        folder_offset = params.folder_offset,
        file_limit = params.file_limit,
        has_file_cursor = params.file_cursor.is_some(),
        sort_by = ?params.sort_by,
        sort_order = ?params.sort_order,
        "listing shared subfolder"
    );

    let contents = folder::list_shared(state, target.id, params).await?;
    tracing::debug!(
        folder_id = target.id,
        folders_total = contents.folders_total,
        files_total = contents.files_total,
        returned_folders = contents.folders.len(),
        returned_files = contents.files.len(),
        "listed shared subfolder"
    );
    Ok(contents)
}

/// 返回目标目录相对于分享根目录的路径链。
///
/// 公开分享不能直接复用登录态 ancestors 响应：目标目录在所属 workspace 中
/// 可能还有分享根以上的私人父目录。这里先通过 `load_shared_subfolder_target`
/// 验证目标位于分享子树内，再把分享根及其上方全部裁掉，只返回分享根下面的
/// 子目录链（包含目标目录本身）。
pub async fn get_shared_subfolder_ancestors(
    state: &impl SharedRuntimeState,
    token: &str,
    folder_id: i64,
) -> Result<Vec<folder::FolderAncestorItem>> {
    let (share, target) = load_shared_subfolder_target(state, token, folder_id).await?;
    let root_folder_id = share
        .folder_id
        .ok_or_else(|| AsterError::share_not_found("folder share root is missing"))?;
    let ancestors = if let Some(team_id) = share.team_id {
        folder_repo::find_team_ancestor_models(state.reader_db(), team_id, target.id).await?
    } else {
        folder_repo::find_ancestor_models(state.reader_db(), share.user_id, target.id).await?
    };
    let root_index = ancestors
        .iter()
        .position(|ancestor| ancestor.id == root_folder_id)
        .ok_or_else(|| {
            auth_forbidden_with_code(
                ApiErrorCode::ShareScopeDenied,
                "folder ancestor chain is outside shared folder scope",
            )
        })?;

    Ok(ancestors
        .into_iter()
        .skip(root_index.saturating_add(1))
        .map(|ancestor| folder::FolderAncestorItem {
            id: ancestor.id,
            name: ancestor.name.clone(),
            icon: folder::FolderIcon::from_model(&ancestor),
        })
        .collect())
}

async fn download_share_resource_with_disposition(
    state: &PrimaryAppState,
    share: &share::Model,
    file: &aster_drive_model::entities::file::Model,
    disposition: file_ops::DownloadDisposition,
    if_none_match: Option<&str>,
    range_header: Option<&HeaderValue>,
) -> Result<file_ops::DownloadOutcome> {
    tracing::debug!(
        share_id = share.id,
        file_id = file.id,
        disposition = ?disposition,
        has_if_none_match = if_none_match.is_some(),
        "starting shared file download"
    );
    let (current_file, blob, revision) =
        file_ops::load_current_download_snapshot(state, file.id).await?;
    if current_file.deleted_at.is_some()
        || current_file.owner_user_id != file.owner_user_id
        || current_file.team_id != file.team_id
        || current_file.folder_id != file.folder_id
    {
        return Err(AsterError::share_not_found(
            "shared file changed location before download",
        ));
    }
    let revision_etag = revision.etag;
    if let Some(if_none_match) = if_none_match
        && file_ops::if_none_match_matches(if_none_match, &revision_etag)
    {
        tracing::debug!(
            share_id = share.id,
            file_id = file.id,
            "shared file download satisfied by ETag"
        );
        return file_ops::build_download_outcome_with_disposition_and_range(
            state,
            &current_file,
            &blob,
            disposition,
            Some(if_none_match),
            None,
            &revision_etag,
        )
        .await;
    }

    let range = file_ops::resolve_range_for_download_snapshot(&current_file, range_header)?;

    initialize_share_download(state, share).await?;

    file_ops::build_download_outcome_with_disposition_and_range(
        state,
        &current_file,
        &blob,
        disposition,
        None,
        range,
        &revision_etag,
    )
    .await
}
