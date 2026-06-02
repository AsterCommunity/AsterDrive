use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, ReadBuf};

use crate::errors::Result;

const CANCELLATION_CHECK_EAGER_READS: u32 = 8;
const CANCELLATION_CHECK_READ_INTERVAL: u32 = 64;

pub(crate) trait StorageCancellationCheck: Send + Sync {
    fn checkpoint(&self) -> Result<()>;
}

impl<F> StorageCancellationCheck for F
where
    F: Fn() -> Result<()> + Send + Sync,
{
    fn checkpoint(&self) -> Result<()> {
        self()
    }
}

#[derive(Clone, Default)]
pub(crate) struct StorageOperationContext {
    cancellation: Option<Arc<dyn StorageCancellationCheck>>,
}

impl StorageOperationContext {
    pub(crate) fn new<C>(cancellation: C) -> Self
    where
        C: StorageCancellationCheck + 'static,
    {
        Self {
            cancellation: Some(Arc::new(cancellation)),
        }
    }

    pub(crate) fn checkpoint(&self) -> Result<()> {
        if let Some(cancellation) = &self.cancellation {
            cancellation.checkpoint()?;
        }
        Ok(())
    }

    pub(crate) fn is_cancellable(&self) -> bool {
        self.cancellation.is_some()
    }

    pub(crate) fn wrap_reader(
        &self,
        reader: Box<dyn AsyncRead + Unpin + Send>,
    ) -> Box<dyn AsyncRead + Unpin + Send + Sync> {
        let reader: Box<dyn AsyncRead + Unpin + Send + Sync> =
            Box::new(SendToSyncReader::new(reader));
        match &self.cancellation {
            Some(cancellation) => Box::new(CancellationAwareReader {
                inner: reader,
                cancellation: cancellation.clone(),
                read_count: 0,
            }),
            None => reader,
        }
    }
}

struct SendToSyncReader {
    inner: Mutex<Box<dyn AsyncRead + Unpin + Send>>,
}

impl SendToSyncReader {
    fn new(inner: Box<dyn AsyncRead + Unpin + Send>) -> Self {
        Self {
            inner: Mutex::new(inner),
        }
    }
}

impl AsyncRead for SendToSyncReader {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        match self.inner.lock() {
            Ok(mut inner) => Pin::new(&mut *inner).poll_read(cx, buf),
            Err(_) => Poll::Ready(Err(std::io::Error::other(
                "send-to-sync reader mutex poisoned",
            ))),
        }
    }
}

impl Unpin for SendToSyncReader {}

struct CancellationAwareReader {
    inner: Box<dyn AsyncRead + Unpin + Send + Sync>,
    cancellation: Arc<dyn StorageCancellationCheck>,
    read_count: u32,
}

impl AsyncRead for CancellationAwareReader {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();
        if should_check_cancellation(this.read_count)
            && let Err(error) = this.cancellation.checkpoint()
        {
            return Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                error.to_string(),
            )));
        }

        let before = buf.filled().len();
        let poll = Pin::new(&mut this.inner).poll_read(cx, buf);
        if let Poll::Ready(Ok(())) = &poll
            && buf.filled().len() > before
        {
            this.read_count = this.read_count.wrapping_add(1);
        }
        poll
    }
}

impl Unpin for CancellationAwareReader {}

fn should_check_cancellation(read_count: u32) -> bool {
    read_count < CANCELLATION_CHECK_EAGER_READS
        || read_count.is_multiple_of(CANCELLATION_CHECK_READ_INTERVAL)
}
