use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU64, Ordering};
use std::task::{Context, Poll};
use std::time::Instant;

use actix_web::HttpResponse;
use actix_web::body::{BodySize, MessageBody};
use aster_forge_webdav::{
    DavEvent, DavEventSink, DavOperationObservations, DavProtocolFailureClass, DavRequestHead,
    DavStreamOutcome,
};

tokio::task_local! {
    static ACTIVE: Arc<DavObservation>;
}

const UNCOLLECTED: u64 = u64::MAX;

pub(crate) struct DavObservation {
    request_head: DavRequestHead,
    started_at: Instant,
    sink: Arc<dyn DavEventSink>,
    status: AtomicU16,
    published: AtomicBool,
    response_started: AtomicBool,
    bytes_received: AtomicU64,
    bytes_sent: AtomicU64,
    requested_ranges: AtomicU64,
    served_ranges: AtomicU64,
    resources: AtomicU64,
    backend_open_count: AtomicU64,
    backend_call_count: AtomicU64,
}

impl DavObservation {
    pub(crate) fn new(
        request_head: DavRequestHead,
        started_at: Instant,
        sink: Arc<dyn DavEventSink>,
    ) -> Arc<Self> {
        Arc::new(Self {
            request_head,
            started_at,
            sink,
            status: AtomicU16::new(500),
            published: AtomicBool::new(false),
            response_started: AtomicBool::new(false),
            bytes_received: AtomicU64::new(UNCOLLECTED),
            bytes_sent: AtomicU64::new(0),
            requested_ranges: AtomicU64::new(UNCOLLECTED),
            served_ranges: AtomicU64::new(UNCOLLECTED),
            resources: AtomicU64::new(UNCOLLECTED),
            backend_open_count: AtomicU64::new(UNCOLLECTED),
            backend_call_count: AtomicU64::new(UNCOLLECTED),
        })
    }

    pub(crate) fn add_bytes_received(&self, count: u64) {
        add_observation(&self.bytes_received, count);
    }

    pub(crate) fn set_ranges(&self, requested: u64, served: u64) {
        self.requested_ranges.store(requested, Ordering::Relaxed);
        self.served_ranges.store(served, Ordering::Relaxed);
    }

    pub(crate) fn add_resource(&self) {
        self.add_resources(1);
    }

    pub(crate) fn add_resources(&self, count: u64) {
        add_observation(&self.resources, count);
    }

    pub(crate) fn add_backend_open(&self) {
        add_observation(&self.backend_open_count, 1);
        add_observation(&self.backend_call_count, 1);
    }

    pub(crate) fn add_backend_call(&self) {
        add_observation(&self.backend_call_count, 1);
    }

    fn publish(&self, stream: DavStreamOutcome) {
        if self.published.swap(true, Ordering::AcqRel) {
            return;
        }
        let status = self.status.load(Ordering::Relaxed);
        let observations = DavOperationObservations {
            bytes_received: collected(&self.bytes_received),
            bytes_sent: collected(&self.bytes_sent),
            requested_ranges: collected(&self.requested_ranges),
            served_ranges: collected(&self.served_ranges),
            resources: collected(&self.resources),
            backend_open_count: collected(&self.backend_open_count),
            backend_call_count: collected(&self.backend_call_count),
            protocol_failure: protocol_failure(status, stream),
            stream: Some(stream),
        };
        aster_forge_webdav::publish_non_authoritative(
            Some(self.sink.as_ref()),
            &DavEvent::completed_with_observations(
                &self.request_head,
                status,
                self.started_at.elapsed(),
                None,
                observations,
            ),
        );
    }
}

pub(crate) async fn scope<F: std::future::Future>(
    observation: Arc<DavObservation>,
    future: F,
) -> F::Output {
    ACTIVE.scope(observation, future).await
}

pub(crate) fn current() -> Option<Arc<DavObservation>> {
    ACTIVE.try_with(Arc::clone).ok()
}

// Observation is non-authoritative fire-and-forget; direct handlers and background tasks without
// an ACTIVE scope intentionally skip these updates.
pub(crate) fn add_bytes_received(count: usize) {
    if let Ok(count) = u64::try_from(count) {
        let _ = ACTIVE.try_with(|observation| observation.add_bytes_received(count));
    }
}

pub(crate) fn set_ranges(requested: usize, served: usize) {
    if let (Ok(requested), Ok(served)) = (u64::try_from(requested), u64::try_from(served)) {
        let _ = ACTIVE.try_with(|observation| observation.set_ranges(requested, served));
    }
}

pub(crate) fn add_resource() {
    let _ = ACTIVE.try_with(|observation| observation.add_resource());
}

pub(crate) fn add_resources(count: usize) {
    if let Ok(count) = u64::try_from(count) {
        let _ = ACTIVE.try_with(|observation| observation.add_resources(count));
    }
}

pub(crate) fn add_backend_calls(count: usize) {
    if let Ok(count) = u64::try_from(count) {
        let _ =
            ACTIVE.try_with(|observation| add_observation(&observation.backend_call_count, count));
    }
}

pub(crate) fn add_backend_open() {
    let _ = ACTIVE.try_with(|observation| observation.add_backend_open());
}

pub(crate) fn add_backend_call() {
    let _ = ACTIVE.try_with(|observation| observation.add_backend_call());
}

fn add_observation(value: &AtomicU64, count: u64) {
    let _ = value.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
        Some(if current == UNCOLLECTED {
            count
        } else {
            current.saturating_add(count)
        })
    });
}

fn collected(value: &AtomicU64) -> Option<u64> {
    match value.load(Ordering::Relaxed) {
        UNCOLLECTED => None,
        value => Some(value),
    }
}

const fn protocol_failure(
    status: u16,
    stream: DavStreamOutcome,
) -> Option<DavProtocolFailureClass> {
    match stream {
        DavStreamOutcome::Cancelled { .. } | DavStreamOutcome::Failed { .. } => {
            Some(DavProtocolFailureClass::Transport)
        }
        DavStreamOutcome::Completed if status < 400 => None,
        DavStreamOutcome::Completed if status == 405 || status == 501 => {
            Some(DavProtocolFailureClass::Capability)
        }
        DavStreamOutcome::Completed
            if status == 409 || status == 412 || status == 423 || status == 424 =>
        {
            Some(DavProtocolFailureClass::Precondition)
        }
        DavStreamOutcome::Completed if status >= 500 => Some(DavProtocolFailureClass::Backend),
        DavStreamOutcome::Completed => Some(DavProtocolFailureClass::Request),
    }
}

pub(crate) fn observe_response(
    response: HttpResponse,
    observation: Arc<DavObservation>,
) -> HttpResponse {
    observation
        .status
        .store(response.status().as_u16(), Ordering::Relaxed);
    response
        .map_body(move |_, body| ObservedBody { body, observation })
        .map_into_boxed_body()
}

struct ObservedBody<B> {
    body: B,
    observation: Arc<DavObservation>,
}

impl<B> MessageBody for ObservedBody<B>
where
    B: MessageBody + Unpin,
{
    type Error = B::Error;

    fn size(&self) -> BodySize {
        self.body.size()
    }

    fn poll_next(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
    ) -> Poll<Option<Result<actix_web::web::Bytes, Self::Error>>> {
        let this = self.get_mut();
        match Pin::new(&mut this.body).poll_next(context) {
            Poll::Ready(Some(Ok(bytes))) => {
                this.observation
                    .response_started
                    .store(true, Ordering::Relaxed);
                add_observation(
                    &this.observation.bytes_sent,
                    u64::try_from(bytes.len()).unwrap_or(u64::MAX),
                );
                Poll::Ready(Some(Ok(bytes)))
            }
            Poll::Ready(Some(Err(error))) => {
                this.observation.publish(DavStreamOutcome::Failed {
                    response_started: this.observation.response_started.load(Ordering::Relaxed),
                });
                Poll::Ready(Some(Err(error)))
            }
            Poll::Ready(None) => {
                this.observation.publish(DavStreamOutcome::Completed);
                Poll::Ready(None)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

impl<B> Drop for ObservedBody<B> {
    fn drop(&mut self) {
        self.observation.publish(DavStreamOutcome::Cancelled {
            response_started: self.observation.response_started.load(Ordering::Relaxed),
        });
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::time::Duration;

    use actix_web::body::to_bytes;
    use aster_forge_webdav::{
        DavEvent, DavMethod, DavObservationError, DavOperationObservations, DavPath,
        DavRequestHead, DavRequestOrigin,
    };

    use super::*;

    #[derive(Default)]
    struct CapturingSink(Mutex<Vec<DavEvent>>);

    impl DavEventSink for CapturingSink {
        fn publish(&self, event: &DavEvent) -> Result<(), DavObservationError> {
            self.0.lock().unwrap().push(event.clone());
            Ok(())
        }
    }

    fn request_head() -> DavRequestHead {
        DavRequestHead {
            method: DavMethod::Get,
            target: DavPath::new("/observed.txt").unwrap(),
            origin: DavRequestOrigin {
                scheme: "http".to_string(),
                host: "localhost".to_string(),
            },
            depth: None,
            overwrite: None,
            destination: None,
            if_header: None,
        }
    }

    #[actix_web::test]
    async fn completed_response_publishes_exact_bytes_and_collected_zeroes() {
        let sink = Arc::new(CapturingSink::default());
        let observation = DavObservation::new(request_head(), Instant::now(), sink.clone());
        observation.set_ranges(0, 0);
        observation.add_resource();
        let response = observe_response(HttpResponse::Ok().body("hello"), observation);

        let body = to_bytes(response.into_body()).await.unwrap();
        assert_eq!(body, "hello");

        let events = sink.0.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0].observations,
            DavOperationObservations {
                bytes_sent: Some(5),
                requested_ranges: Some(0),
                served_ranges: Some(0),
                resources: Some(1),
                stream: Some(DavStreamOutcome::Completed),
                ..DavOperationObservations::default()
            }
        );
    }

    #[test]
    fn dropped_unpolled_body_publishes_cancellation_once() {
        let sink = Arc::new(CapturingSink::default());
        let observation = DavObservation::new(request_head(), Instant::now(), sink.clone());
        let response = observe_response(HttpResponse::Ok().body("hello"), observation);
        drop(response);

        let events = sink.0.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0].observations.stream,
            Some(DavStreamOutcome::Cancelled {
                response_started: false
            })
        );
        assert_eq!(events[0].observations.bytes_sent, Some(0));
        assert!(events[0].elapsed >= Duration::ZERO);
    }
}
