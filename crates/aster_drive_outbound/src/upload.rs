//! Shared upload progress/acknowledgement lifecycle; provider protocols stay with the caller.
use aster_forge_http::{HttpClient, HttpRequestBuilder};
use std::{future::Future, sync::Arc, time::Duration};

/// Transport and lifecycle failures are separate from the provider's own response errors.
#[derive(Debug, thiserror::Error)]
pub enum UploadError<E> {
    /// The prepared request could not be built or sent; the URL is removed from its source.
    #[error("outbound upload transport failed")]
    Transport(#[source] reqwest::Error),
    /// No body consumption progress or response headers arrived within the idle budget.
    #[error("outbound upload progress or response headers timed out")]
    IdleTimeout,
    /// Provider acknowledgement/error decoding exceeded its independent bounded deadline.
    #[error("outbound upload acknowledgement timed out")]
    AcknowledgementTimeout,
    /// The provider decoder rejected its response (status, body, signature or receipt).
    #[error(transparent)]
    Response(E),
}

/// Execute a prepared upload on a reusable ProviderUpload pool and bound its provider reply.
///
/// The supplied pool must have no whole-transfer or pre-header read timeout. Transport body
/// consumption refreshes the idle budget; once headers arrive, `decode` has one bounded budget
/// for the small acknowledgement/error body. No background task is started. Dropping this future
/// cancels the exchange and reader, preserving any caller-owned relay deadline/claim heartbeat.
/// Signing, authentication, request framing and provider-specific response validation stay with
/// callers. Response errors are returned unchanged; transport errors have their URL removed.
///
/// # Errors
/// Returns request/transport, upload-idle, acknowledgement-deadline or provider-decoder errors.
pub async fn send_upload<T, E, F, Fut>(
    client: &HttpClient,
    builder: HttpRequestBuilder,
    idle_timeout: Duration,
    decode: F,
) -> Result<T, UploadError<E>>
where
    F: FnOnce(reqwest::Response) -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    let mut request = builder
        .build()
        .map_err(|error| UploadError::Transport(error.without_url()))?;
    let progress = Arc::new(tokio::sync::Notify::new());
    if let Some(body) = request.body_mut().take() {
        *request.body_mut() = Some(reqwest::Body::wrap(ProgressBody {
            body,
            progress: progress.clone(),
        }));
    }
    let response = client.execute(request);
    tokio::pin!(response);
    let response = loop {
        tokio::select! {
            biased;
            response=&mut response=>break response.map_err(|error|UploadError::Transport(error.without_url()))?,
            ()=progress.notified()=>{},
            ()=tokio::time::sleep(idle_timeout)=>return Err(UploadError::IdleTimeout),
        }
    };
    tokio::time::timeout(idle_timeout, decode(response))
        .await
        .map_err(|_| UploadError::AcknowledgementTimeout)?
        .map_err(UploadError::Response)
}

// Observe actual HTTP body consumption, preserving byte-size hints, EOF and error chains.
struct ProgressBody {
    body: reqwest::Body,
    progress: Arc<tokio::sync::Notify>,
}

impl http_body::Body for ProgressBody {
    type Data = bytes::Bytes;
    type Error = reqwest::Error;

    fn poll_frame(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<std::result::Result<http_body::Frame<Self::Data>, Self::Error>>>
    {
        let frame = std::pin::Pin::new(&mut self.body).poll_frame(cx);
        if matches!(
            &frame,
            std::task::Poll::Ready(Some(Ok(_))) | std::task::Poll::Ready(None)
        ) {
            self.progress.notify_one();
        }
        frame
    }

    fn size_hint(&self) -> http_body::SizeHint {
        self.body.size_hint()
    }

    fn is_end_stream(&self) -> bool {
        self.body.is_end_stream()
    }
}
