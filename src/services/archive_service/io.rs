//! Shared archive I/O helpers.

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::errors::{AsterError, MapAsterErr, Result};

pub(crate) async fn copy_async_reader_to_writer_with_expected_size<R, W, E>(
    reader: &mut R,
    writer: &mut W,
    expected_bytes: u64,
    context: &str,
    size_mismatch_error: E,
) -> Result<u64>
where
    R: AsyncRead + Unpin + ?Sized,
    W: AsyncWrite + Unpin,
    E: Fn(String) -> AsterError,
{
    let mut copied = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];

    loop {
        let read = reader.read(&mut buffer).await.map_aster_err_ctx(
            "read bounded archive stream chunk",
            AsterError::storage_driver_error,
        )?;
        if read == 0 {
            break;
        }

        let read_u64 = crate::utils::numbers::usize_to_u64(read, "archive stream chunk size")?;
        let next_copied = copied
            .checked_add(read_u64)
            .ok_or_else(|| AsterError::internal_error("archive stream byte counter overflow"))?;
        if next_copied > expected_bytes {
            return Err(size_mismatch_error(format!(
                "{context} expands beyond declared size: declared {expected_bytes} bytes"
            )));
        }

        writer.write_all(&buffer[..read]).await.map_aster_err_ctx(
            "write bounded archive stream chunk",
            AsterError::storage_driver_error,
        )?;
        copied = next_copied;
    }

    if copied != expected_bytes {
        return Err(size_mismatch_error(format!(
            "{context} size mismatch: declared {expected_bytes} bytes, downloaded {copied} bytes"
        )));
    }

    Ok(copied)
}
