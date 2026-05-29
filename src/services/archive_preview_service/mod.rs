//! ZIP 文件只读预览服务。

use std::path::Path;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::api::subcode::ApiSubcode;
use crate::config::operations;
use crate::db::repository::file_repo;
use crate::entities::{file, file_blob};
use crate::errors::{
    AsterError, MapAsterErr, Result, auth_forbidden_with_subcode, validation_error_with_subcode,
};
use crate::runtime::PrimaryAppState;
use crate::services::archive_service::format::{ArchiveFormat, detect_archive_preview_format};
use crate::services::archive_service::scan::ArchiveScanLimits;
use crate::services::workspace_storage_service::WorkspaceStorageScope;
use crate::services::{share_service, task_service, workspace_storage_service};
use crate::types::ArchiveFilenameEncoding;

mod cache;
mod model;
mod scan;

use cache::load_cached_raw_manifest;
pub(crate) use cache::store_cached_manifest;
pub use model::{
    ArchivePreviewEntry, ArchivePreviewEntryKind, ArchivePreviewExtractCompatibility,
    ArchivePreviewExtractUnsupportedReason, ArchivePreviewManifest,
};
use scan::build_manifest_from_raw;
pub(crate) use scan::{scan_manifest_from_storage_range, scan_manifest_from_temp};

const CACHE_SCHEMA_VERSION: u32 = 3;
const RAW_CACHE_SCHEMA_VERSION: u32 = 1;
const CACHE_NAMESPACE: &str = "system.archive_preview";
#[cfg(test)]
const FORMAT_ZIP: &str = ArchiveFormat::Zip.as_str();
#[cfg(test)]
const ZIP_RAW_MANIFEST_CACHE_NAME: &str = ArchiveFormat::Zip.raw_manifest_cache_name();
#[cfg(test)]
const SEVEN_ZIP_RAW_MANIFEST_CACHE_NAME: &str = ArchiveFormat::SevenZip.raw_manifest_cache_name();
const ENTITY_PROPERTY_VALUE_MAX_BYTES: usize = 65_536;
const ARCHIVE_PREVIEW_CACHE_WRAPPER_RESERVED_BYTES: usize = 1024;
const ARCHIVE_PREVIEW_MAX_CACHEABLE_MANIFEST_BYTES: usize =
    ENTITY_PROPERTY_VALUE_MAX_BYTES - ARCHIVE_PREVIEW_CACHE_WRAPPER_RESERVED_BYTES;

pub(crate) fn manifest_etag_value(manifest: &ArchivePreviewManifest) -> Result<String> {
    let value = serde_json::json!({
        "schema_version": manifest.schema_version,
        "format": manifest.format,
        "source_blob_id": manifest.source_blob_id,
        "source_hash": manifest.source_hash,
        "entry_count": manifest.entry_count,
        "file_count": manifest.file_count,
        "directory_count": manifest.directory_count,
        "total_uncompressed_size": manifest.total_uncompressed_size,
        "truncated": manifest.truncated,
        "extract_compatibility": manifest.extract_compatibility,
        "entries": manifest.entries,
    });
    let bytes = serde_json::to_vec(&value).map_aster_err_ctx(
        "serialize archive preview manifest etag",
        AsterError::internal_error,
    )?;
    Ok(crate::utils::hash::sha256_hex(&bytes))
}

#[derive(Debug, Clone)]
pub(crate) struct ArchivePreviewLimits {
    pub(crate) archive_format: ArchiveFormat,
    pub(crate) max_source_bytes: i64,
    pub(crate) max_manifest_bytes: usize,
    pub(crate) max_duration_secs: u64,
    pub(crate) scan_limits: ArchiveScanLimits,
    pub(crate) raw_signature: String,
    pub(crate) task_signature: String,
    pub(crate) filename_encoding: ArchiveFilenameEncoding,
}

#[derive(Debug, Clone)]
pub(crate) enum ArchivePreviewManifestLookup {
    Ready(ArchivePreviewManifest),
    Pending,
}

pub(crate) async fn preview_file_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    file_id: i64,
    filename_encoding: ArchiveFilenameEncoding,
) -> Result<ArchivePreviewManifestLookup> {
    ensure_user_preview_enabled(state)?;
    workspace_storage_service::require_scope_access(state, scope).await?;
    let source_file =
        workspace_storage_service::verify_file_access_for_read(state, scope, file_id).await?;
    workspace_storage_service::ensure_active_file_scope(&source_file, scope)?;
    preview_verified_file(state, &source_file, filename_encoding).await
}

pub(crate) async fn preview_shared_file(
    state: &PrimaryAppState,
    token: &str,
    filename_encoding: ArchiveFilenameEncoding,
) -> Result<ArchivePreviewManifestLookup> {
    ensure_share_preview_enabled(state)?;
    let (_, source_file) = share_service::load_preview_shared_file(state, token).await?;
    preview_verified_file(state, &source_file, filename_encoding).await
}

pub(crate) async fn preview_shared_folder_file(
    state: &PrimaryAppState,
    token: &str,
    file_id: i64,
    filename_encoding: ArchiveFilenameEncoding,
) -> Result<ArchivePreviewManifestLookup> {
    ensure_share_preview_enabled(state)?;
    let (_, source_file) =
        share_service::load_preview_shared_folder_file(state, token, file_id).await?;
    preview_verified_file(state, &source_file, filename_encoding).await
}

fn ensure_user_preview_enabled(state: &PrimaryAppState) -> Result<()> {
    ensure_preview_master_enabled(state)?;
    if !operations::archive_preview_user_enabled(&state.runtime_config) {
        return Err(archive_preview_forbidden_error(
            ApiSubcode::ArchivePreviewUserDisabled,
            "archive preview for user files is disabled",
        ));
    }
    Ok(())
}

fn ensure_share_preview_enabled(state: &PrimaryAppState) -> Result<()> {
    ensure_preview_master_enabled(state)?;
    if !operations::archive_preview_share_enabled(&state.runtime_config) {
        return Err(archive_preview_forbidden_error(
            ApiSubcode::ArchivePreviewShareDisabled,
            "archive preview for shared files is disabled",
        ));
    }
    Ok(())
}

fn ensure_preview_master_enabled(state: &PrimaryAppState) -> Result<()> {
    if !operations::archive_preview_enabled(&state.runtime_config) {
        return Err(archive_preview_forbidden_error(
            ApiSubcode::ArchivePreviewDisabled,
            "archive preview is disabled",
        ));
    }
    Ok(())
}

async fn preview_verified_file(
    state: &PrimaryAppState,
    source_file: &file::Model,
    filename_encoding: ArchiveFilenameEncoding,
) -> Result<ArchivePreviewManifestLookup> {
    let archive_format = ensure_archive_preview_source_supported(source_file)?;
    let blob = file_repo::find_blob_by_id(state.reader_db(), source_file.blob_id).await?;
    let limits = ArchivePreviewLimits::from_runtime_config(
        &state.runtime_config,
        filename_encoding,
        archive_format,
    )?;
    if let Some(raw_manifest) = load_cached_raw_manifest(state, source_file, &blob, &limits).await?
    {
        let manifest = build_manifest_from_raw(source_file.id, &raw_manifest, &limits)?;
        return Ok(ArchivePreviewManifestLookup::Ready(manifest));
    }

    if source_file.size > limits.max_source_bytes {
        return Err(archive_preview_validation_error(
            ApiSubcode::ArchivePreviewSourceTooLarge,
            format!(
                "source archive size {} exceeds archive preview limit {}",
                source_file.size, limits.max_source_bytes
            ),
        ));
    }

    task_service::ensure_archive_preview_task(state, source_file, &blob, &limits.task_signature)
        .await?;
    Ok(ArchivePreviewManifestLookup::Pending)
}

impl ArchivePreviewLimits {
    pub(crate) fn from_runtime_config(
        runtime_config: &crate::config::RuntimeConfig,
        filename_encoding: ArchiveFilenameEncoding,
        archive_format: ArchiveFormat,
    ) -> Result<Self> {
        let preview_max_entries = operations::archive_preview_max_entries(runtime_config);
        let scan_limits = ArchiveScanLimits {
            max_uncompressed_bytes: operations::archive_extract_max_uncompressed_bytes(
                runtime_config,
            ),
            max_entries: preview_max_entries
                .min(operations::archive_extract_max_entries(runtime_config)),
            max_files: preview_max_entries
                .min(operations::archive_extract_max_files(runtime_config)),
            max_directories: preview_max_entries
                .min(operations::archive_extract_max_directories(runtime_config)),
            max_depth: operations::archive_extract_max_depth(runtime_config),
            max_path_bytes: operations::archive_extract_max_path_bytes(runtime_config),
            max_compression_ratio: operations::archive_extract_max_compression_ratio(
                runtime_config,
            ),
            max_entry_compression_ratio: operations::archive_extract_max_entry_compression_ratio(
                runtime_config,
            ),
        };
        let configured_max_manifest_bytes = crate::utils::numbers::u64_to_usize(
            operations::archive_preview_max_manifest_bytes(runtime_config),
            operations::ARCHIVE_PREVIEW_MAX_MANIFEST_BYTES_KEY,
        )?;
        let max_manifest_bytes =
            configured_max_manifest_bytes.min(ARCHIVE_PREVIEW_MAX_CACHEABLE_MANIFEST_BYTES);
        let max_source_bytes = operations::archive_preview_max_source_bytes(runtime_config);
        let raw_signature = format!(
            "source={};uncompressed={};ratio={};entry_ratio={};raw-manifest-v1",
            max_source_bytes,
            scan_limits.max_uncompressed_bytes,
            scan_limits.max_compression_ratio,
            scan_limits.max_entry_compression_ratio
        );
        let task_signature = format!(
            "{};format={};entries={};files={};dirs={}",
            raw_signature,
            archive_format.as_str(),
            scan_limits.max_entries,
            scan_limits.max_files,
            scan_limits.max_directories,
        );
        Ok(Self {
            archive_format,
            max_source_bytes,
            max_manifest_bytes,
            max_duration_secs: operations::archive_preview_max_duration_secs(runtime_config),
            scan_limits,
            raw_signature,
            task_signature,
            filename_encoding,
        })
    }
}

pub(crate) async fn download_blob_to_temp(
    state: &PrimaryAppState,
    source_file: &file::Model,
    blob: &file_blob::Model,
    temp_path: &Path,
) -> Result<()> {
    let policy = state.policy_snapshot.get_policy_or_err(blob.policy_id)?;
    let driver = state.driver_registry.get_driver(&policy)?;
    let mut stream = driver.get_stream(&blob.storage_path).await?;
    let mut output = tokio::fs::File::create(temp_path).await.map_aster_err_ctx(
        "create archive preview source temp file",
        AsterError::storage_driver_error,
    )?;
    copy_async_reader_to_writer_with_expected_size(
        &mut stream,
        &mut output,
        crate::utils::numbers::i64_to_u64(source_file.size, "source archive size")?,
        "source archive",
    )
    .await?;
    output.flush().await.map_aster_err_ctx(
        "flush archive preview source temp file",
        AsterError::storage_driver_error,
    )?;
    Ok(())
}

pub(crate) fn ensure_archive_preview_source_supported(
    source_file: &file::Model,
) -> Result<ArchiveFormat> {
    detect_archive_preview_format(source_file).ok_or_else(|| {
        archive_preview_validation_error(
            ApiSubcode::ArchivePreviewUnsupportedType,
            "archive preview currently supports .zip and .7z files only",
        )
    })
}

fn archive_preview_forbidden_error(subcode: ApiSubcode, message: impl Into<String>) -> AsterError {
    auth_forbidden_with_subcode(subcode, message)
}

pub(crate) fn archive_preview_validation_error(
    subcode: ApiSubcode,
    message: impl Into<String>,
) -> AsterError {
    validation_error_with_subcode(subcode, message)
}

pub(crate) fn map_failed_task_error(last_error: Option<&str>) -> AsterError {
    let message = last_error.unwrap_or("archive preview generation failed");
    match crate::errors::task_error_subcode_from_storage(message) {
        Some(ApiSubcode::ArchivePreviewUnsupportedType) => {
            return archive_preview_validation_error(
                ApiSubcode::ArchivePreviewUnsupportedType,
                "archive preview currently supports .zip and .7z files only",
            );
        }
        Some(ApiSubcode::ArchivePreviewSourceTooLarge) => {
            return archive_preview_validation_error(
                ApiSubcode::ArchivePreviewSourceTooLarge,
                crate::errors::task_error_display_message(message).to_string(),
            );
        }
        Some(ApiSubcode::ArchivePreviewInvalidZip) => {
            return archive_preview_validation_error(
                ApiSubcode::ArchivePreviewInvalidZip,
                "invalid archive",
            );
        }
        Some(ApiSubcode::ArchivePreviewManifestTooLarge) => {
            return archive_preview_validation_error(
                ApiSubcode::ArchivePreviewManifestTooLarge,
                crate::errors::task_error_display_message(message).to_string(),
            );
        }
        Some(ApiSubcode::ArchivePreviewSourceSizeMismatch) => {
            return archive_preview_validation_error(
                ApiSubcode::ArchivePreviewSourceSizeMismatch,
                crate::errors::task_error_display_message(message).to_string(),
            );
        }
        Some(ApiSubcode::ArchivePreviewRejected) => {
            return archive_preview_validation_error(
                ApiSubcode::ArchivePreviewRejected,
                crate::errors::task_error_display_message(message).to_string(),
            );
        }
        _ => {}
    }

    // Backward compatibility for tasks failed before subcodes were encoded in last_error.
    let lower = message.to_ascii_lowercase();
    if lower.contains("archive preview currently supports")
        || (lower.contains("supports .zip") && lower.contains("archive preview"))
    {
        return archive_preview_validation_error(
            ApiSubcode::ArchivePreviewUnsupportedType,
            "archive preview currently supports .zip and .7z files only",
        );
    }
    if lower.contains("source archive size") && lower.contains("archive preview limit") {
        return archive_preview_validation_error(
            ApiSubcode::ArchivePreviewSourceTooLarge,
            message.to_string(),
        );
    }
    if lower.contains("invalid zip archive") || lower.contains("invalid 7z archive") {
        return archive_preview_validation_error(
            ApiSubcode::ArchivePreviewInvalidZip,
            "invalid archive",
        );
    }
    if lower.contains("manifest") && lower.contains("exceeds") {
        return archive_preview_validation_error(
            ApiSubcode::ArchivePreviewManifestTooLarge,
            message.to_string(),
        );
    }
    if lower.contains("size mismatch") || lower.contains("expands beyond declared size") {
        return archive_preview_validation_error(
            ApiSubcode::ArchivePreviewSourceSizeMismatch,
            message.to_string(),
        );
    }
    if lower.contains("archive contains")
        || lower.contains("archive uncompressed size")
        || lower.contains("compression ratio")
        || lower.contains("unsafe path")
    {
        return archive_preview_validation_error(
            ApiSubcode::ArchivePreviewRejected,
            message.to_string(),
        );
    }

    AsterError::record_not_found("archive preview is unavailable for this file")
}

fn map_archive_preview_scan_error(error: AsterError) -> AsterError {
    if matches!(error, AsterError::ValidationError(_)) && error.api_error_subcode().is_none() {
        return archive_preview_validation_error(
            ApiSubcode::ArchivePreviewRejected,
            error.message().to_string(),
        );
    }
    error
}

fn map_archive_open_error(error: zip::result::ZipError) -> AsterError {
    if let zip::result::ZipError::Io(io_error) = error
        && let Some(source) = io_error
            .get_ref()
            .and_then(|source| source.downcast_ref::<AsterError>())
    {
        return source.clone();
    }

    archive_preview_validation_error(ApiSubcode::ArchivePreviewInvalidZip, "invalid zip archive")
}

async fn copy_async_reader_to_writer_with_expected_size<R, W>(
    reader: &mut R,
    writer: &mut W,
    expected_bytes: u64,
    context: &str,
) -> Result<u64>
where
    R: AsyncRead + Unpin + ?Sized,
    W: AsyncWrite + Unpin,
{
    let mut copied = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];

    loop {
        let read = reader.read(&mut buffer).await.map_aster_err_ctx(
            "read bounded archive preview stream chunk",
            AsterError::storage_driver_error,
        )?;
        if read == 0 {
            break;
        }

        let read_u64 =
            crate::utils::numbers::usize_to_u64(read, "archive preview stream chunk size")?;
        let next_copied = copied.checked_add(read_u64).ok_or_else(|| {
            AsterError::internal_error("archive preview stream byte counter overflow")
        })?;
        if next_copied > expected_bytes {
            return Err(archive_preview_validation_error(
                ApiSubcode::ArchivePreviewSourceSizeMismatch,
                format!("{context} expands beyond declared size: declared {expected_bytes} bytes"),
            ));
        }

        writer.write_all(&buffer[..read]).await.map_aster_err_ctx(
            "write bounded archive preview stream chunk",
            AsterError::storage_driver_error,
        )?;
        copied = next_copied;
    }

    if copied != expected_bytes {
        return Err(archive_preview_validation_error(
            ApiSubcode::ArchivePreviewSourceSizeMismatch,
            format!(
                "{context} size mismatch: declared {expected_bytes} bytes, downloaded {copied} bytes"
            ),
        ));
    }

    Ok(copied)
}

#[cfg(test)]
mod tests;
