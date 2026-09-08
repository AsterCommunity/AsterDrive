use std::time::Duration;

use aster_drive_model::entities::{file, file_blob, storage_policy};
use aster_drive_storage::{DirectDownloadOptions, DirectDownloadRequest};

use crate::errors::{AsterError, Result};
use crate::runtime::PrimaryAppState;
use crate::services::files::file::{DownloadDisposition, requires_inline_sandbox};

pub(crate) const DIRECT_DOWNLOAD_TTL: Duration = Duration::from_secs(5 * 60);

/// Resolve one browser-addressable download request after authorization and a
/// writer-backed file snapshot have completed.
///
/// The connector decides whether direct delivery is enabled; the driver owns
/// the final host, object-path construction, and URL authentication. Returning
/// `None` is an intentional fallback to the same-origin streaming path.
pub(crate) async fn resolve_download_delivery(
    state: &PrimaryAppState,
    policy: &storage_policy::Model,
    file: &file::Model,
    blob: &file_blob::Model,
    disposition: DownloadDisposition,
) -> Result<Option<DirectDownloadRequest>> {
    if blob.is_virtual_empty()
        || (disposition == DownloadDisposition::Inline && requires_inline_sandbox(&file.mime_type))
    {
        return Ok(None);
    }
    if !crate::storage::connectors::direct_download_enabled(
        state.driver_registry().connectors(),
        policy,
    )? {
        return Ok(None);
    }
    let driver = state.driver_registry().get_driver(policy)?;
    let direct_download = driver.extensions().direct_download.ok_or_else(|| {
        AsterError::storage_driver_error("direct download is enabled but unsupported by driver")
    })?;
    let storage_path = blob.storage_path_for_connector().ok_or_else(|| {
        AsterError::internal_error(format!("stored blob #{} is missing storage_path", blob.id))
    })?;
    direct_download
        .resolve_download_url(
            storage_path,
            DIRECT_DOWNLOAD_TTL,
            DirectDownloadOptions {
                download_name: Some(file.name.clone()),
                require_download_name_match:
                    crate::storage::connectors::presigned_download_requires_filename_match(
                        state.driver_registry().connectors(),
                        policy,
                    )?,
                response_cache_control: Some("private, max-age=0, must-revalidate".to_string()),
                response_content_disposition: Some(disposition.header_value(&file.name)),
                response_content_type: Some(file.mime_type.clone()),
            },
        )
        .await
        .map_err(Into::into)
}
