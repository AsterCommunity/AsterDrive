//! 基于 `StorageDriver::get_range()` 的同步 `Read + Seek` 适配器。

use std::io::{self, Read, Seek, SeekFrom};
use std::sync::Arc;

use tokio::io::AsyncReadExt;

use crate::storage::StorageDriver;

pub(crate) const DEFAULT_RANGE_READER_BLOCK_SIZE: u64 = 256 * 1024;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct RangeReaderStats {
    pub(crate) range_request_count: u64,
    pub(crate) range_bytes_read: u64,
}

pub(crate) struct StorageRangeReader {
    driver: Arc<dyn StorageDriver>,
    storage_path: String,
    size: u64,
    block_size: u64,
    position: u64,
    buffer_start: u64,
    buffer: Vec<u8>,
    stats: RangeReaderStats,
    handle: tokio::runtime::Handle,
}

impl StorageRangeReader {
    pub(crate) fn new(
        driver: Arc<dyn StorageDriver>,
        storage_path: impl Into<String>,
        size: u64,
        handle: tokio::runtime::Handle,
    ) -> Self {
        Self::with_block_size(
            driver,
            storage_path,
            size,
            DEFAULT_RANGE_READER_BLOCK_SIZE,
            handle,
        )
    }

    pub(crate) fn with_block_size(
        driver: Arc<dyn StorageDriver>,
        storage_path: impl Into<String>,
        size: u64,
        block_size: u64,
        handle: tokio::runtime::Handle,
    ) -> Self {
        Self {
            driver,
            storage_path: storage_path.into(),
            size,
            block_size: block_size.max(1),
            position: 0,
            buffer_start: 0,
            buffer: Vec::new(),
            stats: RangeReaderStats::default(),
            handle,
        }
    }

    #[cfg(test)]
    fn stats(&self) -> RangeReaderStats {
        self.stats
    }

    fn refill(&mut self) -> io::Result<()> {
        if self.position >= self.size {
            self.buffer_start = self.size;
            self.buffer.clear();
            return Ok(());
        }

        let block_start = (self.position / self.block_size) * self.block_size;
        let remaining = self.size.saturating_sub(block_start);
        let length = remaining.min(self.block_size);
        let driver = self.driver.clone();
        let storage_path = self.storage_path.clone();
        let bytes = self.handle.block_on(async move {
            let mut stream = driver
                .get_range(&storage_path, block_start, Some(length))
                .await
                .map_err(storage_error_to_io)?;
            let mut bytes = Vec::new();
            stream.read_to_end(&mut bytes).await.map_err(|error| {
                io::Error::other(crate::errors::AsterError::storage_driver_error(format!(
                    "read archive range stream: {error}"
                )))
            })?;
            Ok::<_, io::Error>(bytes)
        })?;

        let bytes_len =
            crate::utils::numbers::usize_to_u64(bytes.len(), "archive range response length")
                .map_err(io::Error::other)?;
        if bytes_len > length {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "archive range read returned {} bytes for requested length {length}",
                    bytes.len()
                ),
            ));
        }
        if bytes_len < length {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                format!(
                    "archive range read returned {} bytes for requested length {length}",
                    bytes.len()
                ),
            ));
        }

        self.stats.range_request_count = self
            .stats
            .range_request_count
            .checked_add(1)
            .ok_or_else(|| io::Error::other("archive range request counter overflow"))?;
        self.stats.range_bytes_read = self
            .stats
            .range_bytes_read
            .checked_add(bytes_len)
            .ok_or_else(|| io::Error::other("archive range byte counter overflow"))?;
        self.buffer_start = block_start;
        self.buffer = bytes;
        Ok(())
    }

    fn position_in_buffer(&self) -> Option<usize> {
        if self.position < self.buffer_start {
            return None;
        }
        let offset = self.position - self.buffer_start;
        let Ok(offset) = crate::utils::numbers::u64_to_usize(offset, "archive range buffer offset")
        else {
            return None;
        };
        if offset < self.buffer.len() {
            Some(offset)
        } else {
            None
        }
    }
}

impl Read for StorageRangeReader {
    fn read(&mut self, mut output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() || self.position >= self.size {
            return Ok(0);
        }

        let mut total_read = 0_usize;
        while !output.is_empty() && self.position < self.size {
            if self.position_in_buffer().is_none() {
                self.refill()?;
                if self.buffer.is_empty() {
                    break;
                }
            }

            let Some(offset) = self.position_in_buffer() else {
                break;
            };
            let available = &self.buffer[offset..];
            if available.is_empty() {
                break;
            }
            let copy_len = available.len().min(output.len());
            output[..copy_len].copy_from_slice(&available[..copy_len]);
            let copy_len_u64 =
                crate::utils::numbers::usize_to_u64(copy_len, "archive range reader copy length")
                    .map_err(io::Error::other)?;
            self.position = self
                .position
                .checked_add(copy_len_u64)
                .ok_or_else(|| io::Error::other("archive range reader position overflow"))?;
            total_read += copy_len;
            let (_, remaining) = output.split_at_mut(copy_len);
            output = remaining;
        }

        Ok(total_read)
    }
}

impl Seek for StorageRangeReader {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let next = match pos {
            SeekFrom::Start(offset) => offset,
            SeekFrom::End(offset) => seek_from_base(self.size, offset)?,
            SeekFrom::Current(offset) => seek_from_base(self.position, offset)?,
        };
        self.position = next;
        Ok(self.position)
    }
}

fn seek_from_base(base: u64, offset: i64) -> io::Result<u64> {
    if offset >= 0 {
        let offset = crate::utils::numbers::i64_to_u64(offset, "archive range seek offset")
            .map_err(io::Error::other)?;
        base.checked_add(offset)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "seek target overflow"))
    } else {
        base.checked_sub(offset.unsigned_abs()).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "seek before start of archive")
        })
    }
}

fn storage_error_to_io(error: crate::errors::AsterError) -> io::Error {
    io::Error::other(error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::errors::Result;
    use crate::storage::BlobMetadata;
    use async_trait::async_trait;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct MemoryRangeDriver {
        data: Vec<u8>,
        range_calls: AtomicUsize,
        stream_calls: AtomicUsize,
        ranges: Mutex<Vec<(u64, Option<u64>)>>,
    }

    impl MemoryRangeDriver {
        fn new(data: &[u8]) -> Self {
            Self {
                data: data.to_vec(),
                range_calls: AtomicUsize::new(0),
                stream_calls: AtomicUsize::new(0),
                ranges: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl StorageDriver for MemoryRangeDriver {
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
            self.ranges
                .lock()
                .expect("range lock should not be poisoned")
                .push((offset, length));
            let start = crate::utils::numbers::u64_to_usize(offset, "memory range start offset")?;
            let end = length
                .map(|len| {
                    offset.checked_add(len).ok_or_else(|| {
                        crate::errors::AsterError::internal_error("memory range end overflow")
                    })
                })
                .transpose()?
                .map(|end| crate::utils::numbers::u64_to_usize(end, "memory range end offset"))
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
                    "memory driver data length",
                )?,
                content_type: None,
            })
        }
    }

    #[test]
    fn range_reader_seeks_and_reads_across_cached_blocks() {
        let runtime = tokio::runtime::Runtime::new().expect("runtime should start");
        let driver = Arc::new(MemoryRangeDriver::new(b"0123456789abcdef"));
        let mut reader = StorageRangeReader::with_block_size(
            driver.clone(),
            "blob",
            16,
            4,
            runtime.handle().clone(),
        );

        let mut bytes = [0_u8; 6];
        reader.seek(SeekFrom::Start(2)).expect("seek should work");
        let read = reader.read(&mut bytes).expect("read should work");

        assert_eq!(read, 6);
        assert_eq!(&bytes, b"234567");
        assert_eq!(reader.stats().range_request_count, 2);
        assert_eq!(driver.range_calls.load(Ordering::SeqCst), 2);
        assert_eq!(driver.stream_calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn range_reader_handles_end_current_and_eof() {
        let runtime = tokio::runtime::Runtime::new().expect("runtime should start");
        let driver = Arc::new(MemoryRangeDriver::new(b"0123456789"));
        let mut reader =
            StorageRangeReader::with_block_size(driver, "blob", 10, 4, runtime.handle().clone());
        let mut bytes = [0_u8; 3];

        assert_eq!(reader.seek(SeekFrom::End(-3)).unwrap(), 7);
        assert_eq!(reader.read(&mut bytes).unwrap(), 3);
        assert_eq!(&bytes, b"789");
        assert_eq!(reader.read(&mut bytes).unwrap(), 0);
        assert_eq!(reader.seek(SeekFrom::Current(-2)).unwrap(), 8);

        let error = reader
            .seek(SeekFrom::Current(-20))
            .expect_err("negative seek should fail");
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    }
}
