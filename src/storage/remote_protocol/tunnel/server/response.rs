use bytes::Bytes;
use http::{HeaderMap, HeaderName, HeaderValue, StatusCode};
use serde::Serialize;

use crate::errors::Result;
use aster_drive_storage::StorageErrorKind;

use super::{REMOTE_TUNNEL_BODY_LIMIT, RemoteTunnelHttpResponse, RemoteTunnelResponse};

pub(crate) fn header_pairs_to_map(headers: Vec<(String, String)>) -> Result<HeaderMap> {
    let mut map = HeaderMap::new();
    for (name, value) in headers {
        let name = HeaderName::from_bytes(name.as_bytes()).map_err(|error| {
            crate::errors::storage_driver_error(
                StorageErrorKind::Misconfigured,
                format!("reverse tunnel returned invalid header name '{name}': {error}"),
            )
        })?;
        let value = HeaderValue::from_str(&value).map_err(|error| {
            crate::errors::storage_driver_error(
                StorageErrorKind::Misconfigured,
                format!("reverse tunnel returned invalid header value for '{name}': {error}"),
            )
        })?;
        map.append(name, value);
    }
    Ok(map)
}

pub(crate) fn tunnel_http_response(
    response: RemoteTunnelResponse,
) -> Result<RemoteTunnelHttpResponse> {
    let status = StatusCode::from_u16(response.status).map_err(|error| {
        crate::errors::storage_driver_error(
            StorageErrorKind::Misconfigured,
            format!("reverse tunnel returned invalid HTTP status: {error}"),
        )
    })?;
    let headers = header_pairs_to_map(response.headers)?;
    Ok(RemoteTunnelHttpResponse {
        status,
        headers,
        body: Bytes::from(response.body),
    })
}

pub fn response_headers_for_tunnel(headers: &HeaderMap) -> Vec<(String, String)> {
    headers
        .iter()
        .filter_map(|(name, value)| match value.to_str() {
            Ok(value) => Some((name.as_str().to_string(), value.to_string())),
            Err(error) => {
                tracing::debug!(
                    header = name.as_str(),
                    "dropping reverse tunnel response header with non-UTF-8 value: {error}"
                );
                None
            }
        })
        .collect()
}

pub async fn tunnel_response_from_reqwest(
    request_id: String,
    mut response: reqwest::Response,
) -> Result<RemoteTunnelResponse> {
    let status = response.status().as_u16();
    let headers = response_headers_for_reqwest(response.headers());
    let too_large = || {
        crate::errors::storage_driver_error(
            StorageErrorKind::Unsupported,
            format!(
                "reverse tunnel response body exceeds {} bytes; use direct transport or a streaming tunnel",
                REMOTE_TUNNEL_BODY_LIMIT
            ),
        )
    };
    // Content-Length is only an early rejection hint. Chunked and decoded bodies
    // must pass the same check before each append, rather than after buffering.
    if response
        .content_length()
        .is_some_and(|size| size > REMOTE_TUNNEL_BODY_LIMIT as u64)
        || response
            .headers()
            .get(reqwest::header::CONTENT_LENGTH)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok())
            .is_some_and(|size| size > REMOTE_TUNNEL_BODY_LIMIT as u64)
    {
        return Err(too_large());
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|error| {
        crate::errors::storage_driver_error(
            StorageErrorKind::Transient,
            format!("read reverse tunnel proxied response body: {error}"),
        )
    })? {
        if chunk.len() > REMOTE_TUNNEL_BODY_LIMIT - body.len() {
            return Err(too_large());
        }
        body.extend_from_slice(&chunk);
    }

    Ok(RemoteTunnelResponse {
        request_id,
        status,
        headers,
        body,
    })
}

fn response_headers_for_reqwest(headers: &reqwest::header::HeaderMap) -> Vec<(String, String)> {
    headers
        .iter()
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|value| (name.as_str().to_string(), value.to_string()))
        })
        .collect()
}

pub fn envelope_response<T: Serialize>(data: T) -> actix_web::HttpResponse {
    actix_web::HttpResponse::Ok().json(crate::api::response::success(data))
}

pub fn empty_envelope_response() -> actix_web::HttpResponse {
    actix_web::HttpResponse::Ok().json(crate::api::response::empty_success())
}

#[cfg(test)]
mod budget_tests {
    use super::*;
    use futures::StreamExt;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    fn streaming_response(
        chunks: usize,
        declared: Option<usize>,
        polls: Arc<AtomicUsize>,
    ) -> reqwest::Response {
        let bytes = Bytes::from(vec![7; 1024 * 1024]);
        let stream = futures::stream::iter(0..chunks).map(move |_| {
            polls.fetch_add(1, Ordering::SeqCst);
            Ok::<_, std::io::Error>(bytes.clone())
        });
        let mut response = http::Response::builder().status(200);
        if let Some(length) = declared {
            response = response.header("content-length", length);
        }
        response
            .body(reqwest::Body::wrap_stream(stream))
            .unwrap()
            .into()
    }

    #[tokio::test]
    async fn declared_oversize_is_rejected_before_body_polling() {
        let polls = Arc::new(AtomicUsize::new(0));
        let response = streaming_response(1, Some(REMOTE_TUNNEL_BODY_LIMIT + 1), polls.clone());
        assert!(
            tunnel_response_from_reqwest("request".into(), response)
                .await
                .is_err()
        );
        assert_eq!(polls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn chunked_oversize_stops_at_first_excess_chunk() {
        let polls = Arc::new(AtomicUsize::new(0));
        let response = streaming_response(100, None, polls.clone());
        let error = tunnel_response_from_reqwest("request".into(), response)
            .await
            .unwrap_err();
        assert!(error.message().contains("exceeds"));
        assert_eq!(polls.load(Ordering::SeqCst), 65);
    }

    #[tokio::test]
    async fn exact_limit_retains_response_bytes_and_status() {
        let polls = Arc::new(AtomicUsize::new(0));
        let response = streaming_response(64, None, polls);
        let converted = tunnel_response_from_reqwest("request".into(), response)
            .await
            .unwrap();
        assert_eq!(converted.status, 200);
        assert_eq!(converted.body.len(), REMOTE_TUNNEL_BODY_LIMIT);
        assert!(converted.body.iter().all(|byte| *byte == 7));
    }
}
