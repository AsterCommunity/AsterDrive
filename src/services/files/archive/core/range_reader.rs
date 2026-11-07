//! 基于 `StorageDriver::get_range()` 的同步 `Read + Seek` 适配器。

use std::collections::VecDeque;
use std::io::{self, Read, Seek, SeekFrom};
use std::sync::{Arc, Mutex};

use tokio::io::AsyncReadExt;

use aster_drive_storage::StorageDriver;

pub(crate) const DEFAULT_RANGE_READER_BLOCK_SIZE: u64 = 256 * 1024;
const MAX_RANGE_REQUESTS: u64 = 4096;
const MAX_RANGE_BYTES: u64 = 256 * 1024 * 1024;
const CACHED_BLOCKS: usize = 8;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct RangeReaderStats {
    pub(crate) range_request_count: u64,
    pub(crate) range_bytes_read: u64,
}

/// Shared by parser passes over one source so reader recreation cannot renew I/O credit.
#[derive(Clone, Default)]
pub(crate) struct RangeReadBudget(Arc<Mutex<RangeReaderStats>>);

pub(crate) struct StorageRangeReader {
    driver: Arc<dyn StorageDriver>,
    storage_path: String,
    size: u64,
    block_size: u64,
    position: u64,
    buffer_start: u64,
    buffer: Vec<u8>,
    cached: VecDeque<(u64, Vec<u8>)>,
    stats: RangeReaderStats,
    budget: RangeReadBudget,
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
            block_size: block_size.clamp(1, DEFAULT_RANGE_READER_BLOCK_SIZE),
            position: 0,
            buffer_start: 0,
            buffer: Vec::new(),
            cached: VecDeque::new(),
            stats: RangeReaderStats::default(),
            budget: RangeReadBudget::default(),
            handle,
        }
    }

    pub(crate) fn with_budget(mut self, budget: RangeReadBudget) -> Self {
        self.budget = budget;
        self
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
        if let Some(index) = self
            .cached
            .iter()
            .position(|(start, _)| *start == block_start)
        {
            let (start, bytes) = self
                .cached
                .remove(index)
                .ok_or_else(|| io::Error::other("cached block disappeared"))?;
            let previous = std::mem::replace(&mut self.buffer, bytes);
            if !previous.is_empty() {
                self.cached.push_back((self.buffer_start, previous));
            }
            self.buffer_start = start;
            return Ok(());
        }
        let remaining = self.size.saturating_sub(block_start);
        let length = remaining.min(self.block_size);
        // This reader only scans archive metadata. Its budget is independent of
        // archive size, so seeking between distant headers cannot multiply I/O
        // indefinitely. Charge before the request, including failed attempts.
        {
            let mut budget = self
                .budget
                .0
                .lock()
                .map_err(|_| io::Error::other("range budget lock poisoned"))?;
            if budget.range_request_count >= MAX_RANGE_REQUESTS
                || length > MAX_RANGE_BYTES - budget.range_bytes_read
            {
                return Err(io::Error::other("archive metadata range budget exceeded"));
            }
            budget.range_request_count += 1;
            budget.range_bytes_read += length;
        }
        self.stats.range_request_count += 1;
        self.stats.range_bytes_read += length;
        let driver = self.driver.clone();
        let storage_path = self.storage_path.clone();
        let bytes = self.handle.block_on(async move {
            let stream = driver
                .get_range(&storage_path, block_start, Some(length))
                .await
                .map_err(storage_error_to_io)?;
            let mut bytes = Vec::new();
            stream
                .take(length + 1)
                .read_to_end(&mut bytes)
                .await
                .map_err(|error| {
                    io::Error::other(crate::errors::AsterError::storage_driver_error(format!(
                        "read archive range stream: {error}"
                    )))
                })?;
            Ok::<_, io::Error>(bytes)
        })?;

        let bytes_len =
            aster_forge_utils::numbers::usize_to_u64(bytes.len(), "archive range response length")
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

        if !self.buffer.is_empty() {
            self.cached
                .push_back((self.buffer_start, std::mem::take(&mut self.buffer)));
            while self.cached.len() >= CACHED_BLOCKS {
                self.cached.pop_front();
            }
        }
        self.buffer_start = block_start;
        self.buffer = bytes;
        Ok(())
    }

    fn position_in_buffer(&self) -> Option<usize> {
        if self.position < self.buffer_start {
            return None;
        }
        let offset = self.position - self.buffer_start;
        let Ok(offset) =
            aster_forge_utils::numbers::u64_to_usize(offset, "archive range buffer offset")
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
            let copy_len_u64 = aster_forge_utils::numbers::usize_to_u64(
                copy_len,
                "archive range reader copy length",
            )
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
        let offset = aster_forge_utils::numbers::i64_to_u64(offset, "archive range seek offset")
            .map_err(io::Error::other)?;
        base.checked_add(offset)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "seek target overflow"))
    } else {
        base.checked_sub(offset.unsigned_abs()).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "seek before start of archive")
        })
    }
}

fn storage_error_to_io(error: aster_drive_storage::StorageError) -> io::Error {
    io::Error::other(error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aster_drive_storage::BlobMetadata;
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
        async fn put(&self, _path: &str, _data: &[u8]) -> aster_drive_storage::Result<String> {
            Ok("memory".to_string())
        }

        async fn get(&self, _path: &str) -> aster_drive_storage::Result<Vec<u8>> {
            Ok(self.data.clone())
        }

        async fn get_stream(
            &self,
            _path: &str,
        ) -> aster_drive_storage::Result<Box<dyn tokio::io::AsyncRead + Unpin + Send>> {
            self.stream_calls.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(std::io::Cursor::new(self.data.clone())))
        }

        async fn get_range(
            &self,
            _path: &str,
            offset: u64,
            length: Option<u64>,
        ) -> aster_drive_storage::Result<Box<dyn tokio::io::AsyncRead + Unpin + Send>> {
            self.range_calls.fetch_add(1, Ordering::SeqCst);
            self.ranges
                .lock()
                .expect("range lock should not be poisoned")
                .push((offset, length));
            let start =
                aster_forge_utils::numbers::u64_to_usize(offset, "memory range start offset")
                    .expect("test range offset should fit usize");
            let end = length
                .map(|len| {
                    offset.checked_add(len).ok_or_else(|| {
                        aster_drive_storage::StorageError::new(
                            aster_drive_storage::StorageErrorKind::Misconfigured,
                            "memory range end overflow",
                        )
                    })
                })
                .transpose()?
                .map(|end| {
                    aster_forge_utils::numbers::u64_to_usize(end, "memory range end offset")
                        .expect("test range end should fit usize")
                })
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

        async fn delete(&self, _path: &str) -> aster_drive_storage::Result<()> {
            Ok(())
        }

        async fn exists(&self, _path: &str) -> aster_drive_storage::Result<bool> {
            Ok(true)
        }

        async fn metadata(&self, _path: &str) -> aster_drive_storage::Result<BlobMetadata> {
            Ok(BlobMetadata {
                size: aster_forge_utils::numbers::usize_to_u64(
                    self.data.len(),
                    "memory driver data length",
                )
                .expect("test data length should fit u64"),
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

    #[test]
    fn repeated_seeks_reuse_multiple_blocks_without_more_requests() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let driver = Arc::new(MemoryRangeDriver::new(b"0123456789abcdef"));
        let mut reader = StorageRangeReader::with_block_size(
            driver.clone(),
            "blob",
            16,
            4,
            runtime.handle().clone(),
        );
        for _ in 0..20 {
            for offset in [0, 8, 4, 12] {
                reader.seek(SeekFrom::Start(offset)).unwrap();
                reader.read_exact(&mut [0]).unwrap();
            }
        }
        assert_eq!(driver.range_calls.load(Ordering::SeqCst), 4);
    }

    #[test]
    fn recreated_readers_share_byte_and_request_budgets() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        for byte_budget in [true, false] {
            let driver = Arc::new(MemoryRangeDriver::new(b"01234567"));
            let budget = super::RangeReadBudget::default();
            {
                let mut counters = budget.0.lock().unwrap();
                if byte_budget {
                    counters.range_bytes_read = super::MAX_RANGE_BYTES - 4;
                } else {
                    counters.range_request_count = super::MAX_RANGE_REQUESTS - 1;
                }
            }
            let make_reader = || {
                StorageRangeReader::with_block_size(
                    driver.clone(),
                    "blob",
                    8,
                    4,
                    runtime.handle().clone(),
                )
                .with_budget(budget.clone())
            };
            make_reader().read_exact(&mut [0]).unwrap();
            assert!(make_reader().read_exact(&mut [0]).is_err());
            assert_eq!(driver.range_calls.load(Ordering::SeqCst), 1);
        }
    }
}
