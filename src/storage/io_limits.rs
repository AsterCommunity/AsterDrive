//! Shared storage streaming limits used by HTTP and WebDAV readers.

/// Reader buffer used for user-visible download streams.
pub(crate) const DOWNLOAD_READER_BUFFER_BYTES: usize = 64 * 1024;
