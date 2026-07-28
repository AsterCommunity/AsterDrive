use bytes::Bytes;
use http::{HeaderMap, HeaderName, HeaderValue, StatusCode};
use serde::Serialize;

use crate::api::response::ApiResponse;
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
    response: reqwest::Response,
) -> Result<RemoteTunnelResponse> {
    let status = response.status().as_u16();
    let headers = response_headers_for_reqwest(response.headers());
    let body = response.bytes().await.map_err(|error| {
        crate::errors::storage_driver_error(
            StorageErrorKind::Transient,
            format!("read reverse tunnel proxied response body: {error}"),
        )
    })?;
    if body.len() > REMOTE_TUNNEL_BODY_LIMIT {
        return Err(crate::errors::storage_driver_error(
            StorageErrorKind::Unsupported,
            format!(
                "reverse tunnel response body exceeds {} bytes; use direct transport or a streaming tunnel",
                REMOTE_TUNNEL_BODY_LIMIT
            ),
        ));
    }

    Ok(RemoteTunnelResponse {
        request_id,
        status,
        headers,
        body: body.to_vec(),
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
    actix_web::HttpResponse::Ok().json(ApiResponse::ok(data))
}

pub fn empty_envelope_response() -> actix_web::HttpResponse {
    actix_web::HttpResponse::Ok().json(ApiResponse::<()>::ok_empty())
}
