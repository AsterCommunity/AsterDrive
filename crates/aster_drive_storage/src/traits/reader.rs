//! Ownership adapter for stream-upload contracts requiring a Sync reader.

use std::pin::Pin;
use std::sync::Mutex;
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, ReadBuf};

pub struct SyncReader(Mutex<Box<dyn AsyncRead + Unpin + Send>>);

impl SyncReader {
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
