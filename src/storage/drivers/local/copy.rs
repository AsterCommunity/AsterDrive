use std::path::Path;

use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::errors::{AsterError, MapAsterErr, Result};

const LOCAL_COPY_BUF_SIZE: usize = 1024 * 1024;

pub async fn copy_file_with_checkpoint(
    source_path: &Path,
    target_path: &Path,
    checkpoint: impl Fn() -> Result<()>,
    operation_name: &str,
) -> Result<u64> {
    checkpoint()?;
    let mut source = tokio::fs::File::open(source_path).await.map_err(|error| {
        AsterError::storage_driver_error(format!(
            "open {operation_name} source {}: {error}",
            source_path.display()
        ))
    })?;
    let mut target = tokio::fs::File::create(target_path)
        .await
        .map_err(|error| {
            AsterError::storage_driver_error(format!(
                "create {operation_name} target {}: {error}",
                target_path.display()
            ))
        })?;
    let mut buf = vec![0_u8; LOCAL_COPY_BUF_SIZE];
    let mut copied = 0_u64;

    loop {
        checkpoint()?;
        let read = source.read(&mut buf).await.map_err(|error| {
            AsterError::storage_driver_error(format!("read {operation_name} source: {error}"))
        })?;
        if read == 0 {
            break;
        }
        target.write_all(&buf[..read]).await.map_err(|error| {
            AsterError::storage_driver_error(format!("write {operation_name} target: {error}"))
        })?;
        let read = u64::try_from(read).map_err(|_| {
            AsterError::storage_driver_error(format!(
                "{operation_name} read size exceeds u64 range"
            ))
        })?;
        copied = copied.checked_add(read).ok_or_else(|| {
            AsterError::storage_driver_error(format!("{operation_name} copy size overflow"))
        })?;
    }
    target
        .sync_all()
        .await
        .map_aster_err_ctx("sync local copy target", AsterError::storage_driver_error)?;
    checkpoint()?;

    Ok(copied)
}
