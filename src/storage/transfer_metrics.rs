//! Data-plane observation without buffering or timers on the noop path.

use aster_drive_metrics::{SharedMetricsRecorder, TransferGuard, TransferLabels, TransferOutcome};
use bytes::Bytes;
use futures::Stream;
use std::{
    pin::Pin,
    task::{Context, Poll},
};
#[cfg(test)]
use tokio::io::{AsyncRead, ReadBuf};

pub(crate) fn payload(
    payload: actix_web::dev::Payload,
    metrics: SharedMetricsRecorder,
    labels: TransferLabels,
) -> actix_web::dev::Payload {
    if !metrics.enabled() {
        return payload;
    }
    actix_web::dev::Payload::from(Box::pin(ObservedPayload {
        inner: payload,
        guard: None,
        metrics,
        labels,
    })
        as Pin<Box<dyn Stream<Item = Result<Bytes, actix_web::error::PayloadError>>>>)
}
struct ObservedPayload {
    inner: actix_web::dev::Payload,
    // A rejected request that never polls the body is not a cancelled transfer.
    guard: Option<TransferGuard>,
    metrics: SharedMetricsRecorder,
    labels: TransferLabels,
}
impl Stream for ObservedPayload {
    type Item = Result<Bytes, actix_web::error::PayloadError>;
    fn poll_next(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        let guard = this
            .guard
            .get_or_insert_with(|| TransferGuard::new(this.metrics.clone(), this.labels));
        let result = Pin::new(&mut this.inner).poll_next(context);
        match &result {
            Poll::Ready(Some(Ok(bytes))) => guard.bytes(bytes.len() as u64),
            Poll::Ready(Some(Err(_))) => guard.finish(TransferOutcome::Failure),
            Poll::Ready(None) => guard.finish(TransferOutcome::Success),
            Poll::Pending => {}
        }
        result
    }
}

pub(crate) fn stream<S>(
    inner: S,
    metrics: SharedMetricsRecorder,
    labels: TransferLabels,
    expected: Option<u64>,
) -> ObservedStream<S> {
    ObservedStream {
        inner,
        guard: TransferGuard::new(metrics, labels),
        expected,
    }
}
pub(crate) struct ObservedStream<S> {
    inner: S,
    guard: TransferGuard,
    expected: Option<u64>,
}
impl<S, E> Stream for ObservedStream<S>
where
    S: Stream<Item = Result<Bytes, E>> + Unpin,
{
    type Item = Result<Bytes, E>;
    fn poll_next(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        let result = Pin::new(&mut this.inner).poll_next(context);
        match &result {
            Poll::Ready(Some(Ok(bytes))) => {
                this.guard.bytes(bytes.len() as u64);
                if let Some(remaining) = &mut this.expected {
                    *remaining = remaining.saturating_sub(bytes.len() as u64);
                }
            }
            Poll::Ready(Some(Err(_))) => this.guard.finish(TransferOutcome::Failure),
            Poll::Ready(None) => {
                this.guard
                    .finish(if this.expected.is_some_and(|remaining| remaining > 0) {
                        TransferOutcome::Failure
                    } else {
                        TransferOutcome::Success
                    })
            }
            Poll::Pending => {}
        }
        result
    }
}
impl<S> Drop for ObservedStream<S> {
    fn drop(&mut self) {
        if self.expected == Some(0) {
            self.guard.finish(TransferOutcome::Success);
        }
    }
}

#[cfg(test)]
fn reader(
    inner: Box<dyn AsyncRead + Unpin + Send>,
    metrics: SharedMetricsRecorder,
    labels: TransferLabels,
    expected: Option<u64>,
) -> Box<dyn AsyncRead + Unpin + Send> {
    Box::new(tokio_util::io::StreamReader::new(stream(
        tokio_util::io::ReaderStream::new(inner),
        metrics,
        labels,
        expected,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use aster_drive_metrics::{MetricsRecorder, TransferDirection, TransferMode, TransferSource};
    use std::sync::{Arc, Mutex};
    use tokio::io::AsyncReadExt;

    #[derive(Default)]
    struct Capture {
        bytes: Mutex<u64>,
        active: Mutex<i64>,
        outcomes: Mutex<Vec<TransferOutcome>>,
    }
    impl MetricsRecorder for Capture {
        fn enabled(&self) -> bool {
            true
        }
        fn transfer_bytes(&self, _: TransferLabels, bytes: u64) {
            *self.bytes.lock().unwrap() += bytes;
        }
        fn transfer_active(&self, _: TransferLabels, delta: i64) {
            *self.active.lock().unwrap() += delta;
        }
        fn transfer_finished(&self, _: TransferLabels, outcome: TransferOutcome, _: f64) {
            self.outcomes.lock().unwrap().push(outcome);
        }
    }
    fn labels() -> TransferLabels {
        TransferLabels {
            direction: TransferDirection::Egress,
            source: TransferSource::File,
            mode: TransferMode::Range,
        }
    }
    fn assert_capture(capture: &Capture, bytes: u64, outcome: TransferOutcome) {
        assert_eq!(*capture.bytes.lock().unwrap(), bytes);
        assert_eq!(*capture.active.lock().unwrap(), 0);
        assert_eq!(*capture.outcomes.lock().unwrap(), vec![outcome]);
    }

    #[tokio::test]
    async fn reader_tracks_actual_range_bytes_zero_length_and_repeated_eof() {
        for bytes in [b"".as_slice(), b"partial".as_slice()] {
            let capture = Arc::new(Capture::default());
            let mut observed = reader(
                Box::new(std::io::Cursor::new(bytes)),
                capture.clone(),
                labels(),
                Some(bytes.len() as u64),
            );
            assert_eq!(*capture.active.lock().unwrap(), 1);
            let mut body = Vec::new();
            observed.read_to_end(&mut body).await.unwrap();
            assert_eq!(observed.read(&mut [0; 1]).await.unwrap(), 0);
            drop(observed);
            assert_eq!(body, bytes);
            assert_capture(&capture, bytes.len() as u64, TransferOutcome::Success);
        }
    }

    #[tokio::test]
    async fn short_eof_partial_drop_and_complete_length_drop_are_distinct() {
        for (read_all, expected, outcome) in [
            (true, 9, TransferOutcome::Failure),
            (false, 9, TransferOutcome::Cancelled),
            (false, 3, TransferOutcome::Success),
        ] {
            let capture = Arc::new(Capture::default());
            let mut observed = reader(
                Box::new(std::io::Cursor::new(b"abc")),
                capture.clone(),
                labels(),
                Some(expected),
            );
            if read_all {
                observed.read_to_end(&mut Vec::new()).await.unwrap();
            } else {
                observed.read_exact(&mut [0; 3]).await.unwrap();
            }
            drop(observed);
            assert_capture(&capture, 3, outcome);
        }
    }

    struct FailingReader {
        sent: bool,
    }
    impl AsyncRead for FailingReader {
        fn poll_read(
            mut self: Pin<&mut Self>,
            _: &mut Context<'_>,
            buffer: &mut ReadBuf<'_>,
        ) -> Poll<std::io::Result<()>> {
            if self.sent {
                return Poll::Ready(Err(std::io::Error::other("injected failure")));
            }
            self.sent = true;
            buffer.put_slice(b"abc");
            Poll::Ready(Ok(()))
        }
    }

    #[tokio::test]
    async fn reader_preserves_error_bytes_and_never_counts_a_zero_capacity_read_as_eof() {
        let capture = Arc::new(Capture::default());
        let mut observed = reader(
            Box::new(FailingReader { sent: false }),
            capture.clone(),
            labels(),
            None,
        );
        assert_eq!(observed.read(&mut []).await.unwrap(), 0);
        assert_eq!(*capture.active.lock().unwrap(), 1);
        assert!(observed.read_to_end(&mut Vec::new()).await.is_err());
        drop(observed);
        assert_capture(&capture, 3, TransferOutcome::Failure);
    }

    #[actix_web::test]
    async fn payload_observes_failure_and_drop_without_buffering_or_double_counting() {
        use futures::StreamExt;
        for outcome in [
            TransferOutcome::Success,
            TransferOutcome::Failure,
            TransferOutcome::Cancelled,
        ] {
            let capture = Arc::new(Capture::default());
            let input: Vec<Result<Bytes, actix_web::error::PayloadError>> = match outcome {
                TransferOutcome::Failure => vec![
                    Ok(Bytes::from_static(b"abc")),
                    Err(actix_web::error::PayloadError::Incomplete(None)),
                ],
                _ => vec![Ok(Bytes::from_static(b"abc"))],
            };
            let input = actix_web::dev::Payload::from(
                Box::pin(futures::stream::iter(input)) as Pin<Box<dyn Stream<Item = _>>>
            );
            let mut observed = payload(input, capture.clone(), labels());
            assert!(observed.next().await.unwrap().is_ok());
            match outcome {
                TransferOutcome::Success => {
                    assert!(observed.next().await.is_none());
                    assert!(observed.next().await.is_none());
                }
                TransferOutcome::Failure => {
                    assert!(observed.next().await.unwrap().is_err());
                }
                TransferOutcome::Cancelled => {}
            }
            drop(observed);
            assert_capture(&capture, 3, outcome);
        }
    }

    #[tokio::test]
    async fn noop_reader_still_streams_and_propagates_io_errors() {
        let mut observed = reader(
            Box::new(FailingReader { sent: false }),
            aster_drive_metrics::NoopMetrics::arc(),
            labels(),
            None,
        );
        let mut body = Vec::new();
        assert!(observed.read_to_end(&mut body).await.is_err());
        assert_eq!(body, b"abc");
    }
    #[test]
    fn unconsumed_payload_does_not_create_or_cancel_a_transfer() {
        let capture = Arc::new(Capture::default());
        let observed = payload(actix_web::dev::Payload::None, capture.clone(), labels());
        assert_eq!(*capture.active.lock().unwrap(), 0);
        drop(observed);
        assert_eq!(*capture.bytes.lock().unwrap(), 0);
        assert!(capture.outcomes.lock().unwrap().is_empty());
    }
}
