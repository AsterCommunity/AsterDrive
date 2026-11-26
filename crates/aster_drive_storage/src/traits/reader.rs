//! Ownership adapter for stream-upload contracts requiring a Sync reader.

use std::pin::Pin;
use std::sync::Mutex;
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, ReadBuf};

/// Wraps an `AsyncRead + Unpin + Send` reader to make it `Sync`.
///
/// Needed by stream-upload contracts that require `Box<dyn AsyncRead + Unpin + Send + Sync>`:
/// `AsyncRead` polling only ever holds exclusive access, so the mutex is
/// uncontended in practice and never blocks waiting on another task.
pub struct SyncReader(Mutex<Box<dyn AsyncRead + Unpin + Send>>);

impl SyncReader {
    /// Takes ownership of the reader and guards it behind the mutex.
    pub fn new(reader: Box<dyn AsyncRead + Unpin + Send>) -> Self {
        Self(Mutex::new(reader))
    }
}

impl AsyncRead for SyncReader {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        output: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        // AsyncRead provides exclusive access: the mutex establishes Sync for
        // the stored Send reader; polling never waits on a blocking lock.
        match self.get_mut().0.get_mut() {
            Ok(reader) => Pin::new(reader).poll_read(cx, output),
            Err(_) => Poll::Ready(Err(std::io::Error::other("stream reader mutex poisoned"))),
        }
    }
}
