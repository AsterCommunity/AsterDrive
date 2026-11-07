use crate::errors::{AsterError, MapAsterErr, Result};
use std::path::Path;

use aster_drive_storage::traits::driver::StorageDriver;
use aster_forge_utils::numbers;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromoteLocalFileOutcome {
    Created,
    AlreadyExists,
}

impl PromoteLocalFileOutcome {
    pub fn created(self) -> bool {
        matches!(self, Self::Created)
    }
}

/// Publish one private candidate without consuming the retry source.
pub async fn promote_local_file_if_absent(
    driver: &dyn StorageDriver,
    storage_path: &str,
    local_path: &str,
    expected_size: i64,
    checkpoint: impl Fn() -> Result<()>,
) -> Result<PromoteLocalFileOutcome> {
    checkpoint()?;
    let local_driver = driver.extensions().local_path.ok_or_else(|| {
        AsterError::storage_driver_error("local path storage driver not supported")
    })?;
    let target = local_driver.resolve_local_path(storage_path)?;
    checkpoint()?;
    if let Some(parent) = target.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_aster_err(AsterError::storage_driver_error)?;
    }
    checkpoint()?;

    let expected_size = numbers::i64_to_u64(expected_size, "local dedup blob size")?;
    match tokio::fs::hard_link(local_path, &target).await {
        Ok(()) => match validate_existing_local_blob_size(&target, expected_size).await {
            Ok(()) => Ok(PromoteLocalFileOutcome::Created),
            Err(error) => {
                if let Err(cleanup_error) = tokio::fs::remove_file(&target).await
                    && cleanup_error.kind() != std::io::ErrorKind::NotFound
                {
                    tracing::warn!(
                        target = %target.display(),
                        "failed to cleanup invalid promoted local blob: {cleanup_error}"
                    );
                }
                Err(error)
            }
        },
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            validate_existing_local_blob_size(&target, expected_size).await?;
            Ok(PromoteLocalFileOutcome::AlreadyExists)
        }
        Err(link_error) => {
            promote_local_file_via_temp_copy(
                local_path,
                &target,
                expected_size,
                link_error,
                checkpoint,
            )
            .await
        }
    }
}

async fn promote_local_file_via_temp_copy(
    local_path: &str,
    target: &Path,
    expected_size: u64,
    link_error: std::io::Error,
    checkpoint: impl Fn() -> Result<()>,
) -> Result<PromoteLocalFileOutcome> {
    let Some(parent) = target.parent() else {
        return Err(AsterError::storage_driver_error(format!(
            "local dedup target has no parent: {}",
            target.display()
        )));
    };
    let temp_name = aster_drive_storage::object_path::new_promotion_staging_name();
    let temp_path = aster_forge_utils::raii::TempFileGuard::new(
        parent.join(temp_name),
        "private local candidate copy",
    );
    copy_file_to_temp(local_path, temp_path.path(), expected_size, &checkpoint).await?;
    checkpoint()?;

    match tokio::fs::hard_link(temp_path.path(), target).await {
        Ok(()) => Ok(PromoteLocalFileOutcome::Created),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            validate_existing_local_blob_size(target, expected_size).await?;
            checkpoint()?;
            Ok(PromoteLocalFileOutcome::AlreadyExists)
        }
        Err(error) => Err(AsterError::storage_driver_error(format!(
            "promote local candidate with no-clobber link failed after initial link error ({link_error}): {error}"
        ))),
    }
}

async fn copy_file_to_temp(
    local_path: &str,
    temp_path: &Path,
    expected_size: u64,
    checkpoint: impl Fn() -> Result<()>,
) -> Result<()> {
    checkpoint()?;
    let copied = super::copy_file_with_checkpoint(
        Path::new(local_path),
        temp_path,
        checkpoint,
        "local dedup",
    )
    .await?;

    if copied != expected_size {
        return Err(AsterError::storage_driver_error(format!(
            "local dedup temp copy size mismatch: expected {expected_size}, copied {copied}"
        )));
    }
    Ok(())
}

async fn validate_existing_local_blob_size(target: &Path, expected_size: u64) -> Result<()> {
    let metadata = tokio::fs::metadata(target).await.map_aster_err_ctx(
        "inspect existing local dedup blob",
        AsterError::storage_driver_error,
    )?;
    if metadata.len() != expected_size {
        return Err(AsterError::storage_driver_error(format!(
            "existing local dedup blob size mismatch for {}: expected {}, actual {}",
            target.display(),
            expected_size,
            metadata.len()
        )));
    }
    Ok(())
}
