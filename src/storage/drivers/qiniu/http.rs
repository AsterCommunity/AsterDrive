use super::{Endpoints, QiniuDriver, invalid};
use aster_drive_storage::{Result, StorageError, StorageErrorKind, storage_driver_error};
use futures::StreamExt;
use reqwest::{Method, Response};
use serde::{Deserialize, de::DeserializeOwned};
use std::time::{Duration, Instant};
use url::Url;
#[derive(Deserialize)]
struct Service {
    domains: Vec<String>,
}
#[derive(Deserialize)]
struct Region {
    ttl: u64,
    up: Service,
    rs: Service,
    rsf: Service,
}
#[derive(Deserialize)]
struct RegionResponse {
    hosts: Vec<Region>,
}
pub(super) enum ServiceKind {
    Upload,
    Management,
    List,
}
impl QiniuDriver {
    pub(super) async fn endpoint(&self, service: ServiceKind) -> Result<Url> {
        let mut cache = self.endpoints.lock().await;
        if cache
            .as_ref()
            .is_none_or(|v| v.expires_at <= Instant::now())
        {
            let mut url = self.region_query.clone();
            url.query_pairs_mut()
                .append_pair("ak", &self.credentials.access_key)
                .append_pair("bucket", &self.config.bucket);
            let response: RegionResponse = decode(
                self.control_client
                    .get(url)
                    .send()
                    .await
                    .map_err(transport_error)?,
            )
            .await?;
            let region = response
                .hosts
                .into_iter()
                .next()
                .ok_or_else(|| invalid("Kodo discovery returned no region"))?;
            let endpoint = |s: Service| -> Result<Url> {
                let host = s
                    .domains
                    .first()
                    .ok_or_else(|| invalid("Kodo discovery returned no service domain"))?;
                super::download::validate_domain(&format!("https://{host}"))
            };
            *cache = Some(Endpoints {
                upload: endpoint(region.up)?,
                management: endpoint(region.rs)?,
                list: endpoint(region.rsf)?,
                expires_at: Instant::now() + Duration::from_secs(region.ttl.clamp(1, 86_400)),
            });
        }
        let endpoints = cache
            .as_ref()
            .ok_or_else(|| invalid("Kodo discovery failed"))?;
        Ok(match service {
            ServiceKind::Upload => endpoints.upload.clone(),
            ServiceKind::Management => endpoints.management.clone(),
            ServiceKind::List => endpoints.list.clone(),
        })
    }
    pub(super) async fn management(&self, method: Method, url: Url) -> Result<Response> {
        let mut request = self
            .control_client
            .request(method, url)
            .build()
            .map_err(transport_error)?;
        self.sign_management(&mut request)?;
        checked(
            self.control_client
                .execute(request)
                .await
                .map_err(transport_error)?,
        )
        .await
    }
}
impl QiniuDriver {
    /// Bound inactivity while sending a body, then bound the small provider reply.
    /// reqwest's read_timeout starts before upload completion and cannot express
    /// this policy: a continuously progressing upload may legitimately take hours.
    pub(super) async fn send_upload<T: DeserializeOwned>(
        &self,
        builder: aster_forge_http::HttpRequestBuilder,
    ) -> Result<T> {
        aster_drive_outbound::send_upload(
            &self.upload_client,
            builder,
            self.config.read_timeout,
            decode,
        )
        .await
        .map_err(|error| match error {
            aster_drive_outbound::UploadError::Transport(error) => transport_error(error),
            aster_drive_outbound::UploadError::IdleTimeout => {
                storage_driver_error(StorageErrorKind::Transient, "Kodo upload stalled")
            }
            aster_drive_outbound::UploadError::AcknowledgementTimeout => storage_driver_error(
                StorageErrorKind::Transient,
                "Kodo upload response timed out",
            ),
            aster_drive_outbound::UploadError::Response(error) => error,
        })
    }
}

pub(super) fn transport_error(error: reqwest::Error) -> StorageError {
    // Never retain reqwest's complete request URL, which may be a bearer credential.
    let mut source = std::error::Error::source(&error);
    while let Some(cause) = source {
        if let Some(io) = cause.downcast_ref::<std::io::Error>()
            && aster_drive_storage::exact_size_error_kind(io).is_some()
        {
            return storage_driver_error(
                StorageErrorKind::Precondition,
                "Kodo input stream did not match its declared size",
            );
        }
        source = cause.source();
    }
    storage_driver_error(
        StorageErrorKind::Transient,
        if error.is_timeout() {
            "Kodo request timed out"
        } else {
            "Kodo HTTP transport failed"
        },
    )
}
pub(super) async fn checked(response: Response) -> Result<Response> {
    if response.status().is_success() {
        return Ok(response);
    }
    let code = response.status().as_u16();
    let request_id = request_id(&response).unwrap_or_else(|| "unavailable".into());
    // Recognize only this exact, non-sensitive provider reason. Do not expose raw
    // bodies, which may echo object URLs or credentials.
    let expiry_window_error = if code == 403 {
        let mut stream = response.bytes_stream();
        let mut bytes = Vec::new();
        while let Some(Ok(chunk)) = stream.next().await {
            if bytes.len().saturating_add(chunk.len()) > 8192 {
                break;
            }
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice::<serde_json::Value>(&bytes).ok()
            .and_then(|value|value.get("error").and_then(serde_json::Value::as_str).map(str::to_string))
            .is_some_and(|reason|reason=="The difference between the request expires and the server's time is too large")
    } else {
        false
    };
    let kind = if expiry_window_error {
        StorageErrorKind::Misconfigured
    } else {
        match code {
            401 => StorageErrorKind::Auth,
            403 => StorageErrorKind::Permission,
            404 | 612 | 631 | 701 => StorageErrorKind::NotFound,
            409 | 412 | 614 | 406 => StorageErrorKind::Precondition,
            429 | 573 => StorageErrorKind::RateLimited,
            408 | 500..=599 => StorageErrorKind::Transient,
            400 | 405 | 413 => StorageErrorKind::Misconfigured,
            _ => StorageErrorKind::Unknown,
        }
    };
    let reason = if expiry_window_error {
        ", signed request expiry exceeds the provider time window"
    } else {
        ""
    };
    Err(storage_driver_error(
        kind,
        format!("Kodo provider status={code}, RequestId={request_id}{reason}"),
    ))
}

pub(super) fn request_id(response: &Response) -> Option<String> {
    response
        .headers()
        .get("x-reqid")
        .and_then(|v| v.to_str().ok())
        .filter(|v| {
            v.len() <= 128
                && v.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_=/.".contains(&b))
        })
        .map(str::to_owned)
}
pub(super) async fn decode<T: DeserializeOwned>(response: Response) -> Result<T> {
    let mut stream = checked(response).await?.bytes_stream();
    let mut body = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(transport_error)?;
        if body.len().saturating_add(chunk.len()) > 8 * 1024 * 1024 {
            return Err(invalid("Kodo control response exceeds size limit"));
        }
        body.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&body).map_err(|_| {
        storage_driver_error(StorageErrorKind::Precondition, "invalid Kodo JSON response")
    })
}
