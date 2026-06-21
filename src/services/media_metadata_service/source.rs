use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::io::AsyncWriteExt;

use crate::entities::file_blob;
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::archive_service::range_reader::StorageRangeReader;
use crate::storage::StorageDriver;
use crate::utils::raii::TempFileGuard;

pub(super) async fn prepare_media_metadata_source(
    state: &PrimaryAppState,
    blob: &file_blob::Model,
    source_file_name: &str,
    source_mime_type: &str,
    allow_range: bool,
) -> Result<PreparedMediaMetadataSource> {
    let policy = state.policy_snapshot().get_policy_or_err(blob.policy_id)?;
    let driver = state.driver_registry().get_driver(&policy)?;

    if let Some(local_path_driver) = driver.as_local_path() {
        return Ok(PreparedMediaMetadataSource::Local(
            local_path_driver.resolve_local_path(&blob.storage_path)?,
        ));
    }

    if allow_range && driver.supports_efficient_range() {
        return Ok(PreparedMediaMetadataSource::Range(
            PreparedRangeMediaMetadataSource::new(
                driver,
                blob,
                source_file_name,
                source_mime_type,
                tokio::runtime::Handle::current(),
            )?,
        ));
    }

    let temp_source = stream_blob_to_temp_source(
        driver,
        blob,
        &state.config().server.temp_dir,
        source_file_name,
        source_mime_type,
    )
    .await?;
    Ok(PreparedMediaMetadataSource::Temp(temp_source))
}

pub(super) enum PreparedMediaMetadataSource {
    Local(PathBuf),
    Temp(TempFileGuard),
    Range(PreparedRangeMediaMetadataSource),
}

impl PreparedMediaMetadataSource {
    pub(super) fn path(&self) -> Option<&Path> {
        match self {
            Self::Local(path) => Some(path.as_path()),
            Self::Temp(guard) => Some(guard.path()),
            Self::Range(_) => None,
        }
    }
}

pub(super) struct PreparedRangeMediaMetadataSource {
    driver: Arc<dyn StorageDriver>,
    storage_path: String,
    size: u64,
    source_file_name: String,
    source_mime_type: String,
    handle: tokio::runtime::Handle,
}

impl PreparedRangeMediaMetadataSource {
    pub(super) fn new(
        driver: Arc<dyn StorageDriver>,
        blob: &file_blob::Model,
        source_file_name: &str,
        source_mime_type: &str,
        handle: tokio::runtime::Handle,
    ) -> Result<Self> {
        Ok(Self {
            driver,
            storage_path: blob.storage_path.clone(),
            size: crate::utils::numbers::i64_to_u64(blob.size, "media metadata source size")?,
            source_file_name: source_file_name.to_string(),
            source_mime_type: source_mime_type.to_string(),
            handle,
        })
    }

    pub(super) fn reader(&self) -> StorageRangeReader {
        StorageRangeReader::new(
            self.driver.clone(),
            self.storage_path.clone(),
            self.size,
            self.handle.clone(),
        )
    }

    pub(super) fn file_type(&self) -> Option<lofty::file::FileType> {
        media_metadata_source_file_type(&self.source_file_name, &self.source_mime_type)
    }
}

async fn stream_blob_to_temp_source(
    driver: Arc<dyn StorageDriver>,
    blob: &file_blob::Model,
    temp_root: &str,
    source_file_name: &str,
    source_mime_type: &str,
) -> Result<TempFileGuard> {
    let temp_dir = PathBuf::from(crate::utils::paths::runtime_temp_dir(temp_root));
    tokio::fs::create_dir_all(&temp_dir)
        .await
        .map_aster_err_ctx(
            "create media metadata temp dir",
            AsterError::storage_driver_error,
        )?;
    let extension = media_metadata_source_extension(source_file_name, source_mime_type);
    let temp_source = TempFileGuard::new(
        temp_dir.join(format!(
            "media-metadata-source-{}.{}",
            uuid::Uuid::new_v4(),
            extension
        )),
        "media metadata source temp file",
    );

    let mut stream = driver.get_stream(&blob.storage_path).await?;
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temp_source.path())
        .await
        .map_aster_err_ctx(
            "create media metadata source temp file",
            AsterError::storage_driver_error,
        )?;
    let copied = tokio::io::copy(&mut stream, &mut file)
        .await
        .map_aster_err_ctx(
            "copy media metadata source stream",
            AsterError::storage_driver_error,
        )?;
    file.flush().await.map_aster_err_ctx(
        "flush media metadata source temp file",
        AsterError::storage_driver_error,
    )?;
    drop(file);

    let expected_size = crate::utils::numbers::i64_to_u64(blob.size, "media metadata source size")?;
    if copied != expected_size {
        return Err(AsterError::storage_driver_error(format!(
            "media metadata source stream size mismatch: expected {expected_size} bytes, got {copied}"
        )));
    }

    Ok(temp_source)
}

fn media_metadata_source_file_type(
    source_file_name: &str,
    source_mime_type: &str,
) -> Option<lofty::file::FileType> {
    lofty::file::FileType::from_path(source_file_name).or_else(|| {
        lofty::file::FileType::from_ext(
            media_metadata_source_extension(source_file_name, source_mime_type).as_str(),
        )
    })
}

fn media_metadata_source_extension(source_file_name: &str, source_mime_type: &str) -> String {
    Path::new(source_file_name)
        .extension()
        .and_then(|value| value.to_str())
        .filter(|value| !value.trim().is_empty())
        .map(|value| value.to_ascii_lowercase())
        .or_else(|| {
            mime_guess::get_mime_extensions_str(source_mime_type)
                .and_then(|extensions| extensions.first().copied())
                .map(str::to_string)
        })
        .unwrap_or_else(|| "bin".to_string())
}
