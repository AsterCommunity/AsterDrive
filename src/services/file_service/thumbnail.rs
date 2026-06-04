//! 文件服务子模块：`thumbnail`。

use crate::db::repository::file_repo;
use crate::errors::Result;
use crate::runtime::PrimaryAppState;
use crate::services::{
    media_processing_service, task_service, workspace_storage_service::WorkspaceStorageScope,
};
use bytes::Bytes;

use super::get_info_in_scope;

/// 缩略图查询结果：有数据直接返回，正在生成则标记 pending
pub struct ThumbnailResult {
    pub data: Bytes,
    pub blob_hash: String,
    pub thumbnail_processor: Option<String>,
    pub thumbnail_version: Option<String>,
}

pub struct ImagePreviewResult {
    pub data: Vec<u8>,
    pub blob_hash: String,
    pub image_preview_processor: String,
    pub image_preview_version: String,
}

pub(crate) async fn get_thumbnail_data_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    file_id: i64,
) -> Result<Option<ThumbnailResult>> {
    let f = get_info_in_scope(state, scope, file_id).await?;
    let blob = file_repo::find_blob_by_id(state.reader_db(), f.blob_id).await?;
    let thumbnail =
        media_processing_service::load_thumbnail_if_exists(state, &blob, &f.name, &f.mime_type)
            .await
            .map_err(media_processing_service::map_thumbnail_request_error)?;

    match thumbnail {
        Some(thumbnail) => Ok(Some(ThumbnailResult {
            data: thumbnail.data,
            blob_hash: blob.hash,
            thumbnail_processor: Some(thumbnail.thumbnail_processor),
            thumbnail_version: Some(thumbnail.thumbnail_version),
        })),
        None => {
            task_service::thumbnail::ensure_thumbnail_task(state, &blob, &f.name, &f.mime_type)
                .await
                .map_err(media_processing_service::map_thumbnail_request_error)?;
            Ok(None)
        }
    }
}

pub(crate) async fn get_image_preview_data_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    file_id: i64,
) -> Result<ImagePreviewResult> {
    let f = get_info_in_scope(state, scope, file_id).await?;
    image_preview_for_file(state, &f).await
}

pub(crate) async fn image_preview_for_file(
    state: &PrimaryAppState,
    f: &crate::entities::file::Model,
) -> Result<ImagePreviewResult> {
    let blob = file_repo::find_blob_by_id(state.reader_db(), f.blob_id).await?;
    let preview = media_processing_service::generate_and_store_image_preview(
        state,
        &blob,
        &f.name,
        &f.mime_type,
    )
    .await
    .map_err(media_processing_service::map_thumbnail_request_error)?;

    Ok(ImagePreviewResult {
        data: preview.data,
        blob_hash: blob.hash,
        image_preview_processor: preview.image_preview_processor,
        image_preview_version: preview.image_preview_version,
    })
}

/// 获取文件缩略图。返回 `Ok(Some(data))` 直接有图；`Ok(None)` 表示正在后台生成。
pub async fn get_thumbnail_data(
    state: &PrimaryAppState,
    file_id: i64,
    user_id: i64,
) -> Result<Option<ThumbnailResult>> {
    get_thumbnail_data_in_scope(state, WorkspaceStorageScope::Personal { user_id }, file_id).await
}
