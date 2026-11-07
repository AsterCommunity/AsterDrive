//! Budgets applied before ZIP metadata allocations, including ZIP64 construction.
//!
//! ZipIterable avoids an eager per-entry index. ZIP 9.2.0 still allocates its
//! ZIP64 extensible sector immediately after reading the fixed 56-byte record.
//! The exact dependency pin and the constructor regression tests keep this
//! Read adapter's fixed-record boundary explicit when upgrading the parser.

use std::io::{self, Read, Seek, SeekFrom};
use std::time::Instant;

use crate::errors::AsterError;

use super::super::scan::ensure_archive_scan_deadline;

const MAX_ZIP_METADATA_READ_BYTES: u64 = 64 * 1024 * 1024;
const MAX_ZIP_METADATA_OPERATIONS: u64 = 1_000_000;
const MAX_ZIP64_END_RECORD_BYTES: u64 = 1024 * 1024;

pub(crate) fn open_zip_archive<R: Read + Seek>(
    reader: R,
    deadline: Option<Instant>,
) -> zip::result::ZipResult<zip::unstable::read::ZipIterable<ZipMetadataReader<R>>> {
    zip::unstable::read::ZipIterable::try_new(
        ZipMetadataReader {
            reader,
            deadline,
            metadata: true,
            after_seek: false,
            bytes_read: 0,
            operations: 0,
            failed: false,
        },
        Default::default(),
    )
}

pub(crate) struct ZipMetadataReader<R> {
    reader: R,
    deadline: Option<Instant>,
    metadata: bool,
    after_seek: bool,
    bytes_read: u64,
    operations: u64,
    failed: bool,
}

impl<R> ZipMetadataReader<R> {
    /// Only extraction after a successful complete metadata preflight may
    /// read payload bytes, which are bounded by the extraction contract.
    pub(crate) fn finish_metadata(&mut self) {
        self.metadata = false;
    }

    fn checkpoint(&mut self) -> io::Result<()> {
        if self.failed {
            return Err(budget_error());
        }
        ensure_archive_scan_deadline(self.deadline).map_err(io::Error::other)?;
        if self.metadata {
            self.operations = self.operations.saturating_add(1);
            if self.operations > MAX_ZIP_METADATA_OPERATIONS {
                self.failed = true;
                return Err(budget_error());
            }
        }
        Ok(())
    }

    fn charge_bytes(&mut self, bytes: usize) -> io::Result<()> {
        if self.metadata {
            self.bytes_read = self.bytes_read.saturating_add(bytes as u64);
            if self.bytes_read > MAX_ZIP_METADATA_READ_BYTES {
                self.failed = true;
                return Err(budget_error());
            }
        }
        Ok(())
    }
}

fn budget_error() -> io::Error {
    io::Error::other(AsterError::validation_error(
        "ZIP metadata exceeds the server processing budget",
    ))
}

impl<R: Read> Read for ZipMetadataReader<R> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        self.checkpoint()?;
        self.after_seek = false;
        let read = self.reader.read(output)?;
        self.charge_bytes(read)?;
        Ok(read)
    }

    fn read_exact(&mut self, output: &mut [u8]) -> io::Result<()> {
        self.checkpoint()?;
        self.charge_bytes(output.len())?;
        let after_seek = std::mem::take(&mut self.after_seek);
        self.reader.read_exact(output)?;
        // The parser seeks to every EOCD64 candidate before reading this
        // fixed block. Inspect it before returning to its allocation site.
        // Variable-length names/comments are not interpreted as records.
        if self.metadata && after_seek && output.len() == 56 && output[..4] == *b"PK\x06\x06" {
            let mut size = [0; 8];
            size.copy_from_slice(&output[4..12]);
            if u64::from_le_bytes(size) > MAX_ZIP64_END_RECORD_BYTES {
                // ZIP tries other candidates after parse errors. Latch the
                // failure so fallback cannot resume an exhausted budget.
                self.failed = true;
                return Err(budget_error());
            }
        }
        Ok(())
    }
}

impl<R: Seek> Seek for ZipMetadataReader<R> {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.checkpoint()?;
        let position = self.reader.seek(position)?;
        self.after_seek = true;
        Ok(position)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn reader() -> ZipMetadataReader<Cursor<Vec<u8>>> {
        ZipMetadataReader {
            reader: Cursor::new(vec![0; 64]),
            deadline: None,
            metadata: true,
            after_seek: false,
            bytes_read: 0,
            operations: 0,
            failed: false,
        }
    }

    #[test]
    fn exhausted_metadata_budget_prevents_reads_and_candidate_fallback() {
        let mut source = reader();
        source.bytes_read = MAX_ZIP_METADATA_READ_BYTES - 3;
        let mut bytes = [0; 4];
        assert!(source.read_exact(&mut bytes).is_err());
        assert_eq!(source.reader.position(), 0);
        assert!(source.seek(SeekFrom::Start(0)).is_err());
        assert!(source.read(&mut bytes).is_err());
    }

    #[test]
    fn expired_constructor_deadline_precedes_io() {
        let mut source = reader();
        source.deadline = Some(Instant::now() - std::time::Duration::from_secs(1));
        assert!(source.read_exact(&mut [0; 4]).is_err());
        assert_eq!(source.reader.position(), 0);
    }
}
