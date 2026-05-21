//! 统一媒体处理服务。
//!
//! 当前已接入 thumbnail 和 avatar 场景，把业务层和具体处理实现解耦。

mod avatar;
mod cli_input;
mod resolve;
mod shared;
#[cfg(test)]
mod tests;
mod thumbnail;

pub use avatar::{probe_vips_cli_command, process_avatar_upload};
pub(crate) use resolve::{map_thumbnail_request_error, resolve_thumbnail_processor_for_blob};
pub use shared::{
    ImagePreviewData, ProcessedAvatar, StoredThumbnail, ThumbnailData,
    image_preview_etag_value_for, thumbnail_etag_value_for,
};
pub(crate) use shared::{cli_output_detail, run_cli_command_with_timeout};
pub(crate) use shared::{known_image_preview_cache_paths, known_thumbnail_cache_paths};
pub(crate) use thumbnail::generate_and_store_thumbnail_with_processor;
pub use thumbnail::{
    delete_thumbnail, generate_and_store_image_preview, generate_and_store_thumbnail,
    get_or_generate_thumbnail, load_thumbnail_if_exists, probe_ffmpeg_cli_command,
};
