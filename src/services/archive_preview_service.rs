//! ZIP 文件只读预览服务。

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::api::subcode::ApiSubcode;
use crate::config::operations;
use crate::db::repository::{file_repo, property_repo};
use crate::entities::{file, file_blob};
use crate::errors::{
    AsterError, MapAsterErr, Result, auth_forbidden_with_subcode, validation_error_with_subcode,
};
use crate::runtime::PrimaryAppState;
use crate::services::archive_service::range_reader::StorageRangeReader;
use crate::services::archive_service::zip_scan::{
    ZipScanEntryKind, ZipScanLimits, scan_zip_archive,
};
use crate::services::workspace_storage_service::WorkspaceStorageScope;
use crate::services::{share_service, task_service, workspace_storage_service};
use crate::storage::StorageDriver;
use crate::types::EntityType;

const CACHE_SCHEMA_VERSION: u32 = 1;
const FORMAT_ZIP: &str = "zip";
const CACHE_NAMESPACE: &str = "system.archive_preview";
const ZIP_MANIFEST_CACHE_NAME: &str = "zip_manifest.v1";
const ENTITY_PROPERTY_VALUE_MAX_BYTES: usize = 65_536;
const ARCHIVE_PREVIEW_CACHE_WRAPPER_RESERVED_BYTES: usize = 1024;
const ARCHIVE_PREVIEW_MAX_CACHEABLE_MANIFEST_BYTES: usize =
    ENTITY_PROPERTY_VALUE_MAX_BYTES - ARCHIVE_PREVIEW_CACHE_WRAPPER_RESERVED_BYTES;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ArchivePreviewManifest {
    pub schema_version: u32,
    pub format: String,
    pub source_blob_id: i64,
    pub source_hash: String,
    pub generated_at: String,
    pub entry_count: i64,
    pub file_count: i64,
    pub directory_count: i64,
    pub total_uncompressed_size: i64,
    pub truncated: bool,
    pub entries: Vec<ArchivePreviewEntry>,
}

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
        "entries": manifest.entries,
    });
    let bytes = serde_json::to_vec(&value).map_aster_err_ctx(
        "serialize archive preview manifest etag",
        AsterError::internal_error,
    )?;
    Ok(crate::utils::hash::sha256_hex(&bytes))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct ArchivePreviewEntry {
    pub path: String,
    pub name: String,
    pub parent: Option<String>,
    pub kind: ArchivePreviewEntryKind,
    pub size: i64,
    pub compressed_size: i64,
    pub modified_at: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum ArchivePreviewEntryKind {
    File,
    Directory,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedArchivePreviewManifest {
    schema_version: u32,
    source_blob_id: i64,
    source_hash: String,
    limit_signature: String,
    manifest: ArchivePreviewManifest,
}

#[derive(Debug, Serialize)]
struct CachedArchivePreviewManifestRef<'a> {
    schema_version: u32,
    source_blob_id: i64,
    source_hash: &'a str,
    limit_signature: &'a str,
    manifest: &'a ArchivePreviewManifest,
}

#[derive(Debug, Clone)]
pub(crate) struct ArchivePreviewLimits {
    pub(crate) max_source_bytes: i64,
    pub(crate) max_manifest_bytes: usize,
    pub(crate) max_duration_secs: u64,
    pub(crate) scan_limits: ZipScanLimits,
    pub(crate) signature: String,
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
) -> Result<ArchivePreviewManifestLookup> {
    ensure_user_preview_enabled(state)?;
    workspace_storage_service::require_scope_access(state, scope).await?;
    let source_file = workspace_storage_service::verify_file_access(state, scope, file_id).await?;
    workspace_storage_service::ensure_active_file_scope(&source_file, scope)?;
    preview_verified_file(state, &source_file).await
}

pub(crate) async fn preview_shared_file(
    state: &PrimaryAppState,
    token: &str,
) -> Result<ArchivePreviewManifestLookup> {
    ensure_share_preview_enabled(state)?;
    let (_, source_file) = share_service::load_preview_shared_file(state, token).await?;
    preview_verified_file(state, &source_file).await
}

pub(crate) async fn preview_shared_folder_file(
    state: &PrimaryAppState,
    token: &str,
    file_id: i64,
) -> Result<ArchivePreviewManifestLookup> {
    ensure_share_preview_enabled(state)?;
    let (_, source_file) =
        share_service::load_preview_shared_folder_file(state, token, file_id).await?;
    preview_verified_file(state, &source_file).await
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
) -> Result<ArchivePreviewManifestLookup> {
    ensure_archive_preview_source_supported(source_file)?;
    let blob = file_repo::find_blob_by_id(&state.db, source_file.blob_id).await?;
    if let Some(cached) = load_cached_manifest(state, source_file, &blob).await? {
        return Ok(ArchivePreviewManifestLookup::Ready(cached));
    }

    let limits = ArchivePreviewLimits::from_runtime_config(&state.runtime_config)?;
    if source_file.size > limits.max_source_bytes {
        return Err(archive_preview_validation_error(
            ApiSubcode::ArchivePreviewSourceTooLarge,
            format!(
                "source archive size {} exceeds archive preview limit {}",
                source_file.size, limits.max_source_bytes
            ),
        ));
    }

    task_service::ensure_archive_preview_task(state, source_file, &blob, &limits.signature).await?;
    Ok(ArchivePreviewManifestLookup::Pending)
}

impl ArchivePreviewLimits {
    pub(crate) fn from_runtime_config(
        runtime_config: &crate::config::RuntimeConfig,
    ) -> Result<Self> {
        let preview_max_entries = operations::archive_preview_max_entries(runtime_config);
        let scan_limits = ZipScanLimits {
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
        let signature = format!(
            "source={};manifest={};entries={};files={};dirs={};uncompressed={};depth={};path={};ratio={};entry_ratio={}",
            max_source_bytes,
            max_manifest_bytes,
            scan_limits.max_entries,
            scan_limits.max_files,
            scan_limits.max_directories,
            scan_limits.max_uncompressed_bytes,
            scan_limits.max_depth,
            scan_limits.max_path_bytes,
            scan_limits.max_compression_ratio,
            scan_limits.max_entry_compression_ratio
        );

        Ok(Self {
            max_source_bytes,
            max_manifest_bytes,
            max_duration_secs: operations::archive_preview_max_duration_secs(runtime_config),
            scan_limits,
            signature,
        })
    }
}

async fn load_cached_manifest(
    state: &PrimaryAppState,
    source_file: &file::Model,
    blob: &file_blob::Model,
) -> Result<Option<ArchivePreviewManifest>> {
    let Some(prop) = property_repo::find_by_key(
        &state.db,
        EntityType::File,
        source_file.id,
        CACHE_NAMESPACE,
        ZIP_MANIFEST_CACHE_NAME,
    )
    .await?
    else {
        return Ok(None);
    };

    let Some(value) = prop.value else {
        return Ok(None);
    };
    let cached = match serde_json::from_str::<CachedArchivePreviewManifest>(&value) {
        Ok(cached) => cached,
        Err(error) => {
            tracing::warn!(
                file_id = source_file.id,
                property_id = prop.id,
                "failed to parse archive preview cache: {error}"
            );
            return Ok(None);
        }
    };

    if cached.schema_version == CACHE_SCHEMA_VERSION
        && cached.source_blob_id == blob.id
        && cached.source_hash == blob.hash
        && cached.manifest.schema_version == CACHE_SCHEMA_VERSION
        && cached.manifest.format == FORMAT_ZIP
    {
        // Existing successful manifests stay usable when administrators later lower preview
        // limits; stricter limits only apply to cache misses and newly generated manifests.
        return Ok(Some(cached.manifest));
    }

    Ok(None)
}

pub(crate) async fn store_cached_manifest(
    state: &PrimaryAppState,
    source_file: &file::Model,
    blob: &file_blob::Model,
    limits: &ArchivePreviewLimits,
    manifest: &ArchivePreviewManifest,
) -> Result<()> {
    let serialized = serialize_cached_manifest(blob.id, &blob.hash, &limits.signature, manifest)?;
    if serialized.len() > ENTITY_PROPERTY_VALUE_MAX_BYTES {
        return Err(archive_preview_validation_error(
            ApiSubcode::ArchivePreviewManifestTooLarge,
            format!(
                "archive preview manifest for file #{} exceeds entity property limit {} bytes",
                source_file.id, ENTITY_PROPERTY_VALUE_MAX_BYTES
            ),
        ));
    }

    property_repo::upsert(
        &state.db,
        EntityType::File,
        source_file.id,
        CACHE_NAMESPACE,
        ZIP_MANIFEST_CACHE_NAME,
        Some(&serialized),
    )
    .await?;
    Ok(())
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

pub(crate) async fn scan_manifest_from_temp(
    source_file: &file::Model,
    blob: &file_blob::Model,
    temp_path: &Path,
    limits: &ArchivePreviewLimits,
) -> Result<ArchivePreviewManifest> {
    let path = temp_path.to_path_buf();
    scan_manifest_with_reader(
        source_file.id,
        blob.id,
        blob.hash.clone(),
        limits,
        move || {
            let file = std::fs::File::open(&path).map_aster_err_ctx(
                "open archive preview temp file",
                AsterError::storage_driver_error,
            )?;
            Ok(file)
        },
    )
    .await
}

pub(crate) async fn scan_manifest_from_storage_range(
    source_file: &file::Model,
    blob: &file_blob::Model,
    driver: Arc<dyn StorageDriver>,
    limits: &ArchivePreviewLimits,
) -> Result<ArchivePreviewManifest> {
    let source_size = crate::utils::numbers::i64_to_u64(source_file.size, "source archive size")?;
    let storage_path = blob.storage_path.clone();
    let handle = tokio::runtime::Handle::current();
    scan_manifest_with_reader(
        source_file.id,
        blob.id,
        blob.hash.clone(),
        limits,
        move || {
            Ok(StorageRangeReader::new(
                driver,
                storage_path,
                source_size,
                handle,
            ))
        },
    )
    .await
}

async fn scan_manifest_with_reader<R, F>(
    source_file_id: i64,
    source_blob_id: i64,
    source_hash: String,
    limits: &ArchivePreviewLimits,
    make_reader: F,
) -> Result<ArchivePreviewManifest>
where
    R: std::io::Read + std::io::Seek + Send + 'static,
    F: FnOnce() -> Result<R> + Send + 'static,
{
    let scan_limits = limits.scan_limits;
    let deadline =
        Instant::now().checked_add(std::time::Duration::from_secs(limits.max_duration_secs));
    let generated_at = Utc::now().to_rfc3339();
    let manifest_source_hash = source_hash.clone();

    let manifest = tokio::task::spawn_blocking(move || {
        let reader = make_reader()?;
        let mut archive = zip::ZipArchive::new(reader).map_err(map_archive_open_error)?;
        let scanned = scan_zip_archive(&mut archive, scan_limits, deadline, |_| Ok(()))
            .map_err(map_archive_preview_scan_error)?;
        let entries = scanned
            .entries
            .into_iter()
            .map(|entry| ArchivePreviewEntry {
                path: entry.path,
                name: entry.name,
                parent: entry.parent,
                kind: match entry.kind {
                    ZipScanEntryKind::File => ArchivePreviewEntryKind::File,
                    ZipScanEntryKind::Directory => ArchivePreviewEntryKind::Directory,
                },
                size: entry.size,
                compressed_size: entry.compressed_size,
                modified_at: entry.modified_at,
            })
            .collect();

        Ok::<_, AsterError>(ArchivePreviewManifest {
            schema_version: CACHE_SCHEMA_VERSION,
            format: FORMAT_ZIP.to_string(),
            source_blob_id,
            source_hash: manifest_source_hash,
            generated_at,
            entry_count: crate::utils::numbers::u64_to_i64(
                scanned.entry_count,
                "archive preview entry count",
            )?,
            file_count: crate::utils::numbers::u64_to_i64(
                scanned.file_count,
                "archive preview file count",
            )?,
            directory_count: crate::utils::numbers::u64_to_i64(
                scanned.directory_count,
                "archive preview directory count",
            )?,
            total_uncompressed_size: scanned.total_uncompressed_bytes,
            truncated: false,
            entries,
        })
    })
    .await
    .map_err(|error| {
        AsterError::internal_error(format!("archive preview worker failed: {error}"))
    })??;

    fit_manifest_to_limit(
        source_file_id,
        source_blob_id,
        &source_hash,
        &limits.signature,
        manifest,
        limits.max_manifest_bytes,
    )
}

fn fit_manifest_to_limit(
    file_id: i64,
    source_blob_id: i64,
    source_hash: &str,
    limit_signature: &str,
    manifest: ArchivePreviewManifest,
    max_manifest_bytes: usize,
) -> Result<ArchivePreviewManifest> {
    if manifest_fits_limits(
        source_blob_id,
        source_hash,
        limit_signature,
        &manifest,
        max_manifest_bytes,
    )? {
        return Ok(manifest);
    }

    let mut base = manifest;
    let original_entries = std::mem::take(&mut base.entries);
    base.truncated = true;
    let mut low = 0_usize;
    let mut high = original_entries.len();
    let mut best_entry_count = None;

    while low <= high {
        let mid = low + (high - low) / 2;
        let mut candidate = base.clone();
        candidate.entries = original_entries[..mid].to_vec();

        if manifest_fits_limits(
            source_blob_id,
            source_hash,
            limit_signature,
            &candidate,
            max_manifest_bytes,
        )? {
            best_entry_count = Some(mid);
            low = mid.saturating_add(1);
        } else if mid == 0 {
            break;
        } else {
            high = mid - 1;
        }
    }

    if let Some(entry_count) = best_entry_count {
        base.entries = original_entries[..entry_count].to_vec();
        return Ok(base);
    }

    Err(archive_preview_validation_error(
        ApiSubcode::ArchivePreviewManifestTooLarge,
        format!(
            "archive preview manifest for file #{file_id} exceeds server limit {max_manifest_bytes} bytes or entity property limit {ENTITY_PROPERTY_VALUE_MAX_BYTES} bytes"
        ),
    ))
}

fn manifest_fits_limits(
    source_blob_id: i64,
    source_hash: &str,
    limit_signature: &str,
    manifest: &ArchivePreviewManifest,
    max_manifest_bytes: usize,
) -> Result<bool> {
    if serialized_manifest_len(manifest)? > max_manifest_bytes {
        return Ok(false);
    }
    Ok(
        serialized_cached_manifest_len(source_blob_id, source_hash, limit_signature, manifest)?
            <= ENTITY_PROPERTY_VALUE_MAX_BYTES,
    )
}

fn serialized_manifest_len(manifest: &ArchivePreviewManifest) -> Result<usize> {
    serde_json::to_vec(manifest)
        .map(|bytes| bytes.len())
        .map_aster_err_ctx(
            "serialize archive preview manifest",
            AsterError::internal_error,
        )
}

fn serialized_cached_manifest_len(
    source_blob_id: i64,
    source_hash: &str,
    limit_signature: &str,
    manifest: &ArchivePreviewManifest,
) -> Result<usize> {
    serde_json::to_vec(&CachedArchivePreviewManifestRef {
        schema_version: CACHE_SCHEMA_VERSION,
        source_blob_id,
        source_hash,
        limit_signature,
        manifest,
    })
    .map(|bytes| bytes.len())
    .map_aster_err_ctx(
        "serialize archive preview cache",
        AsterError::internal_error,
    )
}

fn serialize_cached_manifest(
    source_blob_id: i64,
    source_hash: &str,
    limit_signature: &str,
    manifest: &ArchivePreviewManifest,
) -> Result<String> {
    serde_json::to_string(&CachedArchivePreviewManifestRef {
        schema_version: CACHE_SCHEMA_VERSION,
        source_blob_id,
        source_hash,
        limit_signature,
        manifest,
    })
    .map_aster_err_ctx(
        "serialize archive preview cache",
        AsterError::internal_error,
    )
}

pub(crate) fn ensure_archive_preview_source_supported(source_file: &file::Model) -> Result<()> {
    let mime = source_file.mime_type.to_ascii_lowercase();
    if source_file.name.to_ascii_lowercase().ends_with(".zip")
        || matches!(
            mime.as_str(),
            "application/zip" | "application/x-zip-compressed"
        )
    {
        Ok(())
    } else {
        Err(archive_preview_validation_error(
            ApiSubcode::ArchivePreviewUnsupportedType,
            "archive preview currently supports .zip files only",
        ))
    }
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
                "archive preview currently supports .zip files only",
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
                "invalid zip archive",
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
            "archive preview currently supports .zip files only",
        );
    }
    if lower.contains("source archive size") && lower.contains("archive preview limit") {
        return archive_preview_validation_error(
            ApiSubcode::ArchivePreviewSourceTooLarge,
            message.to_string(),
        );
    }
    if lower.contains("invalid zip archive") {
        return archive_preview_validation_error(
            ApiSubcode::ArchivePreviewInvalidZip,
            "invalid zip archive",
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
mod tests {
    use async_trait::async_trait;
    use std::io::Write;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::io::AsyncWriteExt;

    use super::*;
    use crate::storage::BlobMetadata;

    struct PreviewMemoryRangeDriver {
        data: Vec<u8>,
        range_calls: AtomicUsize,
        stream_calls: AtomicUsize,
    }

    impl PreviewMemoryRangeDriver {
        fn new(data: Vec<u8>) -> Self {
            Self {
                data,
                range_calls: AtomicUsize::new(0),
                stream_calls: AtomicUsize::new(0),
            }
        }
    }

    #[async_trait]
    impl StorageDriver for PreviewMemoryRangeDriver {
        async fn put(&self, _path: &str, _data: &[u8]) -> Result<String> {
            Ok("memory".to_string())
        }

        async fn get(&self, _path: &str) -> Result<Vec<u8>> {
            Ok(self.data.clone())
        }

        async fn get_stream(
            &self,
            _path: &str,
        ) -> Result<Box<dyn tokio::io::AsyncRead + Unpin + Send>> {
            self.stream_calls.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(std::io::Cursor::new(self.data.clone())))
        }

        async fn get_range(
            &self,
            _path: &str,
            offset: u64,
            length: Option<u64>,
        ) -> Result<Box<dyn tokio::io::AsyncRead + Unpin + Send>> {
            self.range_calls.fetch_add(1, Ordering::SeqCst);
            let start =
                crate::utils::numbers::u64_to_usize(offset, "preview memory range start offset")?;
            let end = length
                .map(|len| {
                    offset.checked_add(len).ok_or_else(|| {
                        AsterError::internal_error("preview memory range end overflow")
                    })
                })
                .transpose()?
                .map(|end| {
                    crate::utils::numbers::u64_to_usize(end, "preview memory range end offset")
                })
                .transpose()?
                .unwrap_or(self.data.len())
                .min(self.data.len());
            let bytes = if start >= self.data.len() {
                Vec::new()
            } else {
                self.data[start..end].to_vec()
            };
            Ok(Box::new(std::io::Cursor::new(bytes)))
        }

        fn supports_efficient_range(&self) -> bool {
            true
        }

        async fn delete(&self, _path: &str) -> Result<()> {
            Ok(())
        }

        async fn exists(&self, _path: &str) -> Result<bool> {
            Ok(true)
        }

        async fn metadata(&self, _path: &str) -> Result<BlobMetadata> {
            Ok(BlobMetadata {
                size: crate::utils::numbers::usize_to_u64(
                    self.data.len(),
                    "preview memory driver data length",
                )?,
                content_type: None,
            })
        }
    }

    fn failed_task_subcode(message: &str) -> Option<ApiSubcode> {
        map_failed_task_error(Some(message)).api_error_subcode()
    }

    fn preview_test_limits() -> ArchivePreviewLimits {
        ArchivePreviewLimits {
            max_source_bytes: 1024 * 1024,
            max_manifest_bytes: 64 * 1024,
            max_duration_secs: 10,
            scan_limits: ZipScanLimits {
                max_uncompressed_bytes: 1024 * 1024,
                max_entries: 100,
                max_files: 100,
                max_directories: 100,
                max_depth: 16,
                max_path_bytes: 4096,
                max_compression_ratio: 100,
                max_entry_compression_ratio: 100,
            },
            signature: "test".to_string(),
        }
    }

    fn preview_test_file(size: i64) -> file::Model {
        let now = Utc::now();
        file::Model {
            id: 7,
            name: "bundle.zip".to_string(),
            folder_id: None,
            team_id: None,
            blob_id: 9,
            size,
            owner_user_id: Some(1),
            created_by_user_id: Some(1),
            created_by_username: "tester".to_string(),
            mime_type: "application/zip".to_string(),
            extension: "zip".to_string(),
            compound_extension: None,
            file_category: crate::types::FileCategory::Archive,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            is_locked: false,
        }
    }

    fn preview_test_blob(size: i64) -> file_blob::Model {
        let now = Utc::now();
        file_blob::Model {
            id: 9,
            hash: "hash".to_string(),
            size,
            policy_id: 1,
            storage_path: "blob.zip".to_string(),
            thumbnail_path: None,
            thumbnail_processor: None,
            thumbnail_version: None,
            ref_count: 1,
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn map_failed_task_error_reads_persisted_subcode_without_text_matching() {
        let stored = crate::errors::encode_api_error_subcode_message(
            ApiSubcode::ArchivePreviewInvalidZip,
            "worker changed this wording".to_string(),
        );

        let error = map_failed_task_error(Some(&stored));

        assert_eq!(
            error.api_error_subcode(),
            Some(ApiSubcode::ArchivePreviewInvalidZip)
        );
        assert_eq!(error.message(), "invalid zip archive");
    }

    #[test]
    fn map_failed_task_error_preserves_archive_preview_subcodes() {
        assert_eq!(
            failed_task_subcode("archive preview currently supports .zip files only"),
            Some(ApiSubcode::ArchivePreviewUnsupportedType)
        );
        assert_eq!(
            failed_task_subcode(
                "source archive size 135064658 exceeds archive preview limit 67108864"
            ),
            Some(ApiSubcode::ArchivePreviewSourceTooLarge)
        );
        assert_eq!(
            failed_task_subcode("invalid zip archive"),
            Some(ApiSubcode::ArchivePreviewInvalidZip)
        );
        assert_eq!(
            failed_task_subcode(
                "archive preview manifest for file #1 exceeds server limit 64 bytes"
            ),
            Some(ApiSubcode::ArchivePreviewManifestTooLarge)
        );
        assert_eq!(
            failed_task_subcode(
                "source archive size mismatch: declared 3 bytes, downloaded 2 bytes"
            ),
            Some(ApiSubcode::ArchivePreviewSourceSizeMismatch)
        );
        assert_eq!(
            failed_task_subcode("archive contains 2 entries, exceeds server limit 1"),
            Some(ApiSubcode::ArchivePreviewRejected)
        );
    }

    #[test]
    fn map_failed_task_error_falls_back_to_unavailable_when_unknown() {
        let error = map_failed_task_error(Some("worker disappeared"));

        assert_eq!(error.code(), "E006");
        assert_eq!(
            error.message(),
            "archive preview is unavailable for this file"
        );
    }

    #[tokio::test]
    async fn bounded_copy_accepts_exact_size_and_preserves_bytes() {
        let (mut writer, mut reader) = tokio::io::duplex(16);
        let producer = tokio::spawn(async move {
            writer
                .write_all(b"zip")
                .await
                .expect("write should succeed");
        });
        let mut output = Vec::new();

        let copied = copy_async_reader_to_writer_with_expected_size(
            &mut reader,
            &mut output,
            3,
            "source archive",
        )
        .await
        .expect("exact-size stream should copy");

        producer.await.expect("producer should finish");
        assert_eq!(copied, 3);
        assert_eq!(output, b"zip");
    }

    #[tokio::test]
    async fn bounded_copy_rejects_short_and_long_streams() {
        let mut short_reader = tokio::io::empty();
        let mut short_output = Vec::new();
        let short_error = copy_async_reader_to_writer_with_expected_size(
            &mut short_reader,
            &mut short_output,
            1,
            "source archive",
        )
        .await
        .expect_err("short stream should fail");
        assert_eq!(
            short_error.api_error_subcode(),
            Some(ApiSubcode::ArchivePreviewSourceSizeMismatch)
        );
        assert!(short_error.message().contains("downloaded 0 bytes"));

        let (mut writer, mut reader) = tokio::io::duplex(16);
        let producer = tokio::spawn(async move {
            writer
                .write_all(b"too-long")
                .await
                .expect("write should succeed");
        });
        let mut long_output = Vec::new();
        let long_error = copy_async_reader_to_writer_with_expected_size(
            &mut reader,
            &mut long_output,
            3,
            "source archive",
        )
        .await
        .expect_err("long stream should fail");

        producer.await.expect("producer should finish");
        assert_eq!(
            long_error.api_error_subcode(),
            Some(ApiSubcode::ArchivePreviewSourceSizeMismatch)
        );
        assert!(
            long_error
                .message()
                .contains("expands beyond declared size")
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn range_manifest_scan_uses_get_range_without_full_stream() {
        let bytes = {
            let cursor = std::io::Cursor::new(Vec::new());
            let mut zip = zip::ZipWriter::new(cursor);
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            zip.add_directory("docs/", options)
                .expect("directory should be added");
            zip.start_file("docs/readme.txt", options)
                .expect("file should start");
            zip.write_all(b"hello").expect("file should write");
            zip.finish().expect("zip should finish").into_inner()
        };
        let source_size =
            crate::utils::numbers::usize_to_i64(bytes.len(), "preview range test zip size")
                .expect("test zip size should fit i64");
        let source_file = preview_test_file(source_size);
        let blob = preview_test_blob(source_size);
        let driver = Arc::new(PreviewMemoryRangeDriver::new(bytes));

        let manifest = scan_manifest_from_storage_range(
            &source_file,
            &blob,
            driver.clone(),
            &preview_test_limits(),
        )
        .await
        .expect("range manifest scan should succeed");

        assert_eq!(manifest.entry_count, 2);
        assert_eq!(manifest.file_count, 1);
        assert_eq!(manifest.directory_count, 1);
        assert_eq!(driver.stream_calls.load(Ordering::SeqCst), 0);
        assert!(driver.range_calls.load(Ordering::SeqCst) > 0);
    }
}
