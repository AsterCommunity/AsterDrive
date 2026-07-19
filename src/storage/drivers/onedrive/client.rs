use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::TryStreamExt;
use reqwest::StatusCode;
use reqwest::header::{AUTHORIZATION, CONTENT_LENGTH, CONTENT_TYPE, LOCATION, RANGE};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::sync::Arc;
use tokio::io::AsyncRead;
use tokio_util::io::StreamReader;

use crate::config::OUTBOUND_HTTP_USER_AGENT;
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::storage::error::{StorageErrorKind, storage_driver_error};
use crate::storage::traits::driver::BlobMetadata;
use crate::storage::traits::extensions::{StorageCapacityInfo, StorageCapacityStatus};

use super::error::{invalid_graph_url, map_graph_response_error, map_reqwest_error};

#[derive(Clone, Debug)]
pub struct MicrosoftGraphClient {
    config: MicrosoftGraphClientConfig,
    http: reqwest::Client,
}

#[derive(Clone, Debug)]
pub struct MicrosoftGraphClientConfig {
    pub graph_base_url: String,
    pub token_provider: Arc<dyn MicrosoftGraphAccessTokenProvider>,
}

#[async_trait]
pub trait MicrosoftGraphAccessTokenProvider: Send + Sync + std::fmt::Debug {
    fn is_configured(&self) -> bool {
        true
    }

    async fn access_token(&self) -> Result<String>;
    async fn refresh_access_token(&self) -> Result<String>;
}

#[derive(Clone)]
struct StaticMicrosoftGraphAccessTokenProvider {
    access_token: SecretString,
}

#[derive(Clone, Debug, Deserialize)]
pub struct MicrosoftGraphDriveItem {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub size: Option<i64>,
    #[serde(default)]
    #[serde(rename = "file")]
    pub file: Option<serde_json::Value>,
    #[serde(default)]
    #[serde(rename = "folder")]
    pub folder: Option<serde_json::Value>,
    #[serde(default)]
    #[serde(rename = "parentReference")]
    pub parent_reference: Option<MicrosoftGraphDriveItemParentReference>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct MicrosoftGraphDriveItemParentReference {
    #[serde(default)]
    #[serde(rename = "driveId")]
    pub drive_id: Option<String>,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct MicrosoftGraphDrive {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    quota: Option<MicrosoftGraphQuota>,
}

#[derive(Debug, Deserialize)]
pub struct MicrosoftGraphUploadSession {
    #[serde(rename = "uploadUrl")]
    pub upload_url: String,
    #[serde(default, rename = "expirationDateTime")]
    pub expires_at: Option<DateTime<Utc>>,
    #[serde(default, rename = "nextExpectedRanges")]
    pub next_expected_ranges: Vec<String>,
}

#[derive(Debug, Serialize)]
struct MicrosoftGraphUploadSessionRequest {
    item: MicrosoftGraphUploadSessionItem,
}

#[derive(Debug, Serialize)]
struct MicrosoftGraphUploadSessionItem {
    #[serde(rename = "@microsoft.graph.conflictBehavior")]
    conflict_behavior: &'static str,
}

#[derive(Debug, Deserialize)]
struct MicrosoftGraphQuota {
    #[serde(default)]
    remaining: Option<i64>,
    #[serde(default)]
    total: Option<i64>,
    #[serde(default)]
    used: Option<i64>,
}

impl MicrosoftGraphClientConfig {
    pub fn new(graph_base_url: impl Into<String>, access_token: impl Into<String>) -> Self {
        Self {
            graph_base_url: graph_base_url.into(),
            token_provider: Arc::new(StaticMicrosoftGraphAccessTokenProvider {
                access_token: SecretString::from(access_token.into()),
            }),
        }
    }

    pub fn with_token_provider(
        graph_base_url: impl Into<String>,
        token_provider: Arc<dyn MicrosoftGraphAccessTokenProvider>,
    ) -> Self {
        Self {
            graph_base_url: graph_base_url.into(),
            token_provider,
        }
    }
}

#[async_trait]
impl MicrosoftGraphAccessTokenProvider for StaticMicrosoftGraphAccessTokenProvider {
    fn is_configured(&self) -> bool {
        !self.access_token.expose_secret().trim().is_empty()
    }

    async fn access_token(&self) -> Result<String> {
        Ok(self.access_token.expose_secret().to_string())
    }

    async fn refresh_access_token(&self) -> Result<String> {
        Err(storage_driver_error(
            StorageErrorKind::Auth,
            "Microsoft Graph access token cannot be refreshed",
        ))
    }
}

impl fmt::Debug for StaticMicrosoftGraphAccessTokenProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StaticMicrosoftGraphAccessTokenProvider")
            .field("access_token", &"***REDACTED***")
            .finish()
    }
}

#[cfg(test)]
mod static_provider_tests {
    use super::*;

    #[test]
    fn debug_redacts_static_access_token() {
        let provider = StaticMicrosoftGraphAccessTokenProvider {
            access_token: SecretString::from("plain-access-token"),
        };

        let debug = format!("{provider:?}");
        assert!(debug.contains(r#"access_token: "***REDACTED***""#));
        assert!(!debug.contains("plain-access-token"));
    }
}

impl MicrosoftGraphClient {
    pub fn new(config: MicrosoftGraphClientConfig) -> Result<Self> {
        if config.graph_base_url.trim().is_empty() {
            return Err(storage_driver_error(
                StorageErrorKind::Misconfigured,
                "Microsoft Graph base URL cannot be empty",
            ));
        }
        if !config.token_provider.is_configured() {
            return Err(storage_driver_error(
                StorageErrorKind::Auth,
                "Microsoft Graph access token cannot be empty",
            ));
        }
        let http = reqwest::ClientBuilder::new()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(60))
            .user_agent(OUTBOUND_HTTP_USER_AGENT)
            .build()
            .map_aster_err_ctx(
                "failed to build Microsoft Graph HTTP client",
                AsterError::internal_error,
            )?;
        Ok(Self { config, http })
    }

    pub async fn get_drive_item_by_id(
        &self,
        drive_id: &str,
        item_id: &str,
    ) -> Result<MicrosoftGraphDriveItem> {
        self.get_json(
            &format!(
                "/drives/{}/items/{}",
                encode_path_segment(drive_id),
                encode_path_segment(item_id)
            ),
            "get OneDrive item metadata",
        )
        .await
    }

    pub async fn get_drive_root(&self, drive_id: &str) -> Result<MicrosoftGraphDriveItem> {
        self.get_json(
            &format!("/drives/{}/root", encode_path_segment(drive_id)),
            "get OneDrive drive root metadata",
        )
        .await
    }

    pub async fn get_me_drive(&self) -> Result<MicrosoftGraphDrive> {
        self.get_json("/me/drive", "get signed-in user's default OneDrive")
            .await
    }

    pub async fn get_site_drive(&self, site_id: &str) -> Result<MicrosoftGraphDrive> {
        self.get_json(
            &format!("/sites/{}/drive", encode_path_segment(site_id)),
            "get SharePoint site's default document library",
        )
        .await
    }

    pub async fn get_group_drive(&self, group_id: &str) -> Result<MicrosoftGraphDrive> {
        self.get_json(
            &format!("/groups/{}/drive", encode_path_segment(group_id)),
            "get Microsoft 365 group's default drive",
        )
        .await
    }

    pub async fn get_drive_item(&self, path: &str) -> Result<MicrosoftGraphDriveItem> {
        self.get_json(path, "get OneDrive item metadata").await
    }

    pub async fn metadata(&self, path: &str) -> Result<BlobMetadata> {
        let item = self.get_drive_item(path).await?;
        let size = item.size.unwrap_or(0);
        if size < 0 {
            return Err(storage_driver_error(
                StorageErrorKind::Unknown,
                "Microsoft Graph returned negative item size",
            ));
        }
        Ok(BlobMetadata {
            size: u64::try_from(size).map_err(|_| {
                storage_driver_error(StorageErrorKind::Unknown, "OneDrive item size overflow")
            })?,
            content_type: None,
        })
    }

    pub async fn put_small_content(&self, content_path: &str, data: &[u8]) -> Result<()> {
        let url = self.url(content_path)?;
        let body = data.to_vec();
        let response = self
            .send_with_auth("put OneDrive small content", |access_token| {
                self.http
                    .put(url.clone())
                    .header(AUTHORIZATION, authorization_header(&access_token))
                    .header(CONTENT_LENGTH, body.len().to_string())
                    .header(CONTENT_TYPE, "application/octet-stream")
                    .body(body.clone())
            })
            .await?;
        self.ensure_success(response, "put OneDrive small content")
            .await?;
        Ok(())
    }

    pub async fn create_upload_session(
        &self,
        upload_session_path: &str,
    ) -> Result<MicrosoftGraphUploadSession> {
        let url = self.url(upload_session_path)?;
        let response = self
            .send_with_auth("create OneDrive upload session", |access_token| {
                self.http
                    .post(url.clone())
                    .header(AUTHORIZATION, authorization_header(&access_token))
                    .json(&MicrosoftGraphUploadSessionRequest {
                        item: MicrosoftGraphUploadSessionItem {
                            conflict_behavior: "replace",
                        },
                    })
            })
            .await?;
        let response = self
            .ensure_success(response, "create OneDrive upload session")
            .await?;
        let session = response
            .json::<MicrosoftGraphUploadSession>()
            .await
            .map_aster_err_ctx(
                "create OneDrive upload session: invalid Microsoft Graph JSON",
                AsterError::storage_driver_error,
            )?;
        if session.upload_url.trim().is_empty() {
            return Err(storage_driver_error(
                StorageErrorKind::Unknown,
                "Microsoft Graph returned empty uploadUrl",
            ));
        }
        Ok(session)
    }

    pub async fn query_upload_session(
        &self,
        upload_url: &str,
    ) -> Result<MicrosoftGraphUploadSession> {
        let url = reqwest::Url::parse(upload_url).map_err(invalid_graph_url)?;
        let response = self
            .http
            .get(url)
            .send()
            .await
            .map_err(|err| map_reqwest_error("query OneDrive upload session", err))?;
        let response = self
            .ensure_success(response, "query OneDrive upload session")
            .await?;
        response
            .json::<MicrosoftGraphUploadSession>()
            .await
            .map_aster_err_ctx(
                "query OneDrive upload session: invalid Microsoft Graph JSON",
                AsterError::storage_driver_error,
            )
    }

    pub async fn abort_upload_session(&self, upload_url: &str) -> Result<()> {
        let url = reqwest::Url::parse(upload_url).map_err(invalid_graph_url)?;
        let response = self
            .http
            .delete(url)
            .send()
            .await
            .map_err(|err| map_reqwest_error("abort OneDrive upload session", err))?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(());
        }
        self.ensure_success(response, "abort OneDrive upload session")
            .await?;
        Ok(())
    }

    pub async fn upload_session_fragment(
        &self,
        upload_url: &str,
        start: u64,
        total_size: u64,
        data: Vec<u8>,
    ) -> Result<()> {
        if data.is_empty() {
            return Err(storage_driver_error(
                StorageErrorKind::Misconfigured,
                "OneDrive upload session fragment cannot be empty",
            ));
        }
        let len = u64::try_from(data.len()).map_err(|_| {
            storage_driver_error(
                StorageErrorKind::Misconfigured,
                "OneDrive upload session fragment length overflow",
            )
        })?;
        let end = start
            .checked_add(len)
            .and_then(|value| value.checked_sub(1))
            .ok_or_else(|| {
                storage_driver_error(
                    StorageErrorKind::Misconfigured,
                    "OneDrive upload session fragment range overflow",
                )
            })?;
        let content_range = format!("bytes {start}-{end}/{total_size}");
        let url = reqwest::Url::parse(upload_url).map_err(invalid_graph_url)?;
        let response = self
            .http
            .put(url)
            .header(CONTENT_LENGTH, len.to_string())
            .header(CONTENT_TYPE, "application/octet-stream")
            .header("Content-Range", content_range)
            .body(data)
            .send()
            .await
            .map_err(|err| map_reqwest_error("upload OneDrive session fragment", err))?;
        self.ensure_success(response, "upload OneDrive session fragment")
            .await?;
        Ok(())
    }

    pub async fn get_stream(
        &self,
        content_path: &str,
        offset: Option<u64>,
        length: Option<u64>,
    ) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
        self.get_content_response(content_path, offset, length)
            .await
            .map(|response| {
                let stream = response.bytes_stream().map_err(std::io::Error::other);
                Box::new(StreamReader::new(stream)) as Box<dyn AsyncRead + Unpin + Send>
            })
    }

    pub async fn get_bytes(&self, content_path: &str) -> Result<Vec<u8>> {
        let response = self.get_content_response(content_path, None, None).await?;
        response
            .bytes()
            .await
            .map(|bytes| bytes.to_vec())
            .map_err(|err| map_reqwest_error("read OneDrive content", err))
    }

    pub async fn delete(&self, path: &str) -> Result<()> {
        let url = self.url(path)?;
        let response = self
            .send_with_auth("delete OneDrive item", |access_token| {
                self.http
                    .delete(url.clone())
                    .header(AUTHORIZATION, authorization_header(&access_token))
            })
            .await?;
        self.ensure_success(response, "delete OneDrive item")
            .await?;
        Ok(())
    }

    pub async fn exists(&self, path: &str) -> Result<bool> {
        match self.get_drive_item(path).await {
            Ok(_) => Ok(true),
            Err(error) if error.storage_error_kind() == Some(StorageErrorKind::NotFound) => {
                Ok(false)
            }
            Err(error) => Err(error),
        }
    }

    pub async fn capacity_info(&self, drive_id: &str) -> Result<StorageCapacityInfo> {
        let drive: MicrosoftGraphDrive = self
            .get_json(
                &format!("/drives/{}", encode_path_segment(drive_id)),
                "get OneDrive drive quota",
            )
            .await?;
        let Some(quota) = drive.quota else {
            return Ok(StorageCapacityInfo {
                status: StorageCapacityStatus::Unavailable,
                total_bytes: None,
                available_bytes: None,
                used_bytes: None,
                source: "microsoft_graph".to_string(),
                observed_at: Utc::now(),
            });
        };
        Ok(StorageCapacityInfo {
            status: StorageCapacityStatus::Supported,
            total_bytes: quota.total,
            available_bytes: quota.remaining,
            used_bytes: quota.used,
            source: "microsoft_graph".to_string(),
            observed_at: Utc::now(),
        })
    }

    async fn get_json<T: serde::de::DeserializeOwned>(&self, path: &str, ctx: &str) -> Result<T> {
        let url = self.url(path)?;
        let response = self
            .send_with_auth(ctx, |access_token| {
                self.http
                    .get(url.clone())
                    .header(AUTHORIZATION, authorization_header(&access_token))
            })
            .await?;
        let response = self.ensure_success(response, ctx).await?;
        response.json::<T>().await.map_aster_err_ctx(
            &format!("{ctx}: invalid Microsoft Graph JSON"),
            AsterError::storage_driver_error,
        )
    }

    async fn ensure_success(
        &self,
        response: reqwest::Response,
        ctx: &str,
    ) -> Result<reqwest::Response> {
        if response.status().is_success() {
            return Ok(response);
        }
        Err(map_graph_response_error(ctx, response).await)
    }

    async fn get_content_response(
        &self,
        content_path: &str,
        offset: Option<u64>,
        length: Option<u64>,
    ) -> Result<reqwest::Response> {
        let url = self.url(content_path)?;
        let response = self
            .send_with_auth("get OneDrive content stream", |access_token| {
                self.http
                    .get(url.clone())
                    .header(AUTHORIZATION, authorization_header(&access_token))
            })
            .await?;
        if response.status().is_redirection() {
            let base_url = response.url().clone();
            let download_url = response.headers().get(LOCATION).ok_or_else(|| {
                storage_driver_error(
                    StorageErrorKind::Unknown,
                    "Microsoft Graph content response missing redirect location",
                )
            })?;
            let download_url = download_url.to_str().map_err(|_| {
                storage_driver_error(
                    StorageErrorKind::Unknown,
                    "Microsoft Graph content redirect location is invalid UTF-8",
                )
            })?;
            let download_url = base_url.join(download_url).map_err(invalid_graph_url)?;
            let mut request = self.http.get(download_url);
            // Microsoft Graph documents partial downloads as Range on the
            // actual downloadUrl/redirect target, not on the /content request.
            if let Some(range_header) = range_header(offset, length)? {
                request = request.header(RANGE, range_header);
            }
            let response = request
                .send()
                .await
                .map_err(|err| map_reqwest_error("follow OneDrive content redirect", err))?;
            return self
                .ensure_success(response, "get OneDrive content stream")
                .await;
        }

        self.ensure_success(response, "get OneDrive content stream")
            .await
    }

    async fn send_with_auth<F>(&self, ctx: &str, build_request: F) -> Result<reqwest::Response>
    where
        F: Fn(String) -> reqwest::RequestBuilder,
    {
        let access_token = self.config.token_provider.access_token().await?;
        if access_token.trim().is_empty() {
            return Err(storage_driver_error(
                StorageErrorKind::Auth,
                "Microsoft Graph access token cannot be empty",
            ));
        }
        let response = build_request(access_token)
            .send()
            .await
            .map_err(|err| map_reqwest_error(ctx, err))?;
        if response.status() != StatusCode::UNAUTHORIZED {
            return Ok(response);
        }

        let access_token = self.config.token_provider.refresh_access_token().await?;
        if access_token.trim().is_empty() {
            return Err(storage_driver_error(
                StorageErrorKind::Auth,
                "Microsoft Graph refreshed access token cannot be empty",
            ));
        }
        build_request(access_token)
            .send()
            .await
            .map_err(|err| map_reqwest_error(ctx, err))
    }

    fn url(&self, path: &str) -> Result<reqwest::Url> {
        let base = self.config.graph_base_url.trim().trim_end_matches('/');
        let path = path.trim();
        let path = if path.starts_with('/') {
            path.to_string()
        } else {
            format!("/{path}")
        };
        reqwest::Url::parse(&format!("{base}/v1.0{path}")).map_err(invalid_graph_url)
    }
}

fn authorization_header(access_token: &str) -> String {
    format!("Bearer {access_token}")
}

fn encode_path_segment(value: &str) -> String {
    percent_encoding::utf8_percent_encode(value.trim(), percent_encoding::NON_ALPHANUMERIC)
        .to_string()
}

fn range_header(offset: Option<u64>, length: Option<u64>) -> Result<Option<String>> {
    let Some(offset) = offset else {
        return Ok(None);
    };
    if let Some(length) = length {
        if length == 0 {
            return Err(storage_driver_error(
                StorageErrorKind::Misconfigured,
                "OneDrive range length cannot be zero",
            ));
        }
        let end = offset
            .checked_add(length)
            .and_then(|value| value.checked_sub(1))
            .ok_or_else(|| {
                storage_driver_error(StorageErrorKind::Misconfigured, "OneDrive range overflow")
            })?;
        Ok(Some(format!("bytes={offset}-{end}")))
    } else {
        Ok(Some(format!("bytes={offset}-")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{App, HttpRequest, HttpResponse, HttpServer, web};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct UploadProtocolState {
        methods: Vec<String>,
        paths: Vec<String>,
        auth_headers: Vec<Option<String>>,
        content_ranges: Vec<Option<String>>,
        content_lengths: Vec<Option<String>>,
        bodies: Vec<Vec<u8>>,
    }

    struct UploadProtocolTestServer {
        base_url: String,
        state: Arc<Mutex<UploadProtocolState>>,
        handle: actix_web::dev::ServerHandle,
        task: tokio::task::JoinHandle<std::io::Result<()>>,
    }

    impl UploadProtocolTestServer {
        async fn stop(self) {
            self.handle.stop(true).await;
            let _ = self.task.await;
        }
    }

    struct TestHttpServer {
        base_url: String,
        ranges: Arc<Mutex<Vec<Option<String>>>>,
        auth_headers: Arc<Mutex<Vec<Option<String>>>>,
        handle: actix_web::dev::ServerHandle,
        task: tokio::task::JoinHandle<std::io::Result<()>>,
    }

    impl TestHttpServer {
        async fn stop(self) {
            self.handle.stop(true).await;
            let _ = self.task.await;
        }
    }

    #[derive(Debug)]
    struct RefreshingTestTokenProvider {
        access_token_calls: Arc<Mutex<usize>>,
        refresh_calls: Arc<Mutex<usize>>,
        refreshed_token: String,
        fail_refresh: bool,
    }

    impl RefreshingTestTokenProvider {
        fn new(refreshed_token: impl Into<String>) -> Self {
            Self {
                access_token_calls: Arc::new(Mutex::new(0)),
                refresh_calls: Arc::new(Mutex::new(0)),
                refreshed_token: refreshed_token.into(),
                fail_refresh: false,
            }
        }

        fn failing() -> Self {
            Self {
                access_token_calls: Arc::new(Mutex::new(0)),
                refresh_calls: Arc::new(Mutex::new(0)),
                refreshed_token: String::new(),
                fail_refresh: true,
            }
        }

        fn access_token_calls(&self) -> usize {
            *self
                .access_token_calls
                .lock()
                .expect("access token call lock")
        }

        fn refresh_calls(&self) -> usize {
            *self.refresh_calls.lock().expect("refresh call lock")
        }
    }

    #[async_trait]
    impl MicrosoftGraphAccessTokenProvider for RefreshingTestTokenProvider {
        async fn access_token(&self) -> Result<String> {
            *self
                .access_token_calls
                .lock()
                .expect("access token call lock") += 1;
            Ok("expired-token".to_string())
        }

        async fn refresh_access_token(&self) -> Result<String> {
            *self.refresh_calls.lock().expect("refresh call lock") += 1;
            if self.fail_refresh {
                return Err(storage_driver_error(
                    StorageErrorKind::Auth,
                    "refresh failed",
                ));
            }
            Ok(self.refreshed_token.clone())
        }
    }

    async fn spawn_content_redirect_server() -> TestHttpServer {
        async fn content(_: HttpRequest) -> HttpResponse {
            HttpResponse::Found()
                .append_header(("Location", "/download"))
                .finish()
        }

        async fn download(
            req: HttpRequest,
            body: web::Data<Arc<Mutex<Vec<Option<String>>>>>,
        ) -> HttpResponse {
            let range = req
                .headers()
                .get("range")
                .and_then(|value| value.to_str().ok())
                .map(str::to_string);
            body.lock().expect("range log lock").push(range);
            HttpResponse::Ok().body("hello-world")
        }

        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("listener should bind");
        let base_url = format!(
            "http://{}",
            listener.local_addr().expect("listener addr should exist")
        );
        let ranges = Arc::new(Mutex::new(Vec::new()));
        let auth_headers = Arc::new(Mutex::new(Vec::new()));
        let ranges_data = web::Data::new(ranges.clone());
        let server = HttpServer::new(move || {
            App::new()
                .app_data(ranges_data.clone())
                .route("/v1.0/content", web::get().to(content))
                .route("/download", web::get().to(download))
        })
        .listen(listener)
        .expect("server should listen")
        .run();
        let handle = server.handle();
        let task = tokio::spawn(server);
        TestHttpServer {
            base_url,
            ranges,
            auth_headers,
            handle,
            task,
        }
    }

    async fn spawn_authorized_get_server() -> TestHttpServer {
        async fn drive(
            req: HttpRequest,
            auth_headers: web::Data<Arc<Mutex<Vec<Option<String>>>>>,
        ) -> HttpResponse {
            let authorization = req
                .headers()
                .get("authorization")
                .and_then(|value| value.to_str().ok())
                .map(str::to_string);
            auth_headers
                .lock()
                .expect("auth header log lock")
                .push(authorization.clone());
            match authorization.as_deref() {
                Some("Bearer refreshed-token") => HttpResponse::Ok().json(serde_json::json!({
                    "id": "drive-id",
                    "name": "Drive"
                })),
                _ => HttpResponse::Unauthorized().json(serde_json::json!({
                    "error": {
                        "code": "InvalidAuthenticationToken",
                        "message": "expired"
                    }
                })),
            }
        }

        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("listener should bind");
        let base_url = format!(
            "http://{}",
            listener.local_addr().expect("listener addr should exist")
        );
        let auth_headers = Arc::new(Mutex::new(Vec::new()));
        let auth_headers_data = web::Data::new(auth_headers.clone());
        let server = HttpServer::new(move || {
            App::new()
                .app_data(auth_headers_data.clone())
                .route("/v1.0/me/drive", web::get().to(drive))
        })
        .listen(listener)
        .expect("server should listen")
        .run();
        let handle = server.handle();
        let task = tokio::spawn(server);
        TestHttpServer {
            base_url,
            ranges: Arc::new(Mutex::new(Vec::new())),
            auth_headers,
            handle,
            task,
        }
    }

    async fn spawn_forbidden_get_server() -> TestHttpServer {
        async fn drive(
            req: HttpRequest,
            auth_headers: web::Data<Arc<Mutex<Vec<Option<String>>>>>,
        ) -> HttpResponse {
            let authorization = req
                .headers()
                .get("authorization")
                .and_then(|value| value.to_str().ok())
                .map(str::to_string);
            auth_headers
                .lock()
                .expect("auth header log lock")
                .push(authorization);
            HttpResponse::Forbidden().json(serde_json::json!({
                "error": {
                    "code": "accessDenied",
                    "message": "denied"
                }
            }))
        }

        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("listener should bind");
        let base_url = format!(
            "http://{}",
            listener.local_addr().expect("listener addr should exist")
        );
        let auth_headers = Arc::new(Mutex::new(Vec::new()));
        let auth_headers_data = web::Data::new(auth_headers.clone());
        let server = HttpServer::new(move || {
            App::new()
                .app_data(auth_headers_data.clone())
                .route("/v1.0/me/drive", web::get().to(drive))
        })
        .listen(listener)
        .expect("server should listen")
        .run();
        let handle = server.handle();
        let task = tokio::spawn(server);
        TestHttpServer {
            base_url,
            ranges: Arc::new(Mutex::new(Vec::new())),
            auth_headers,
            handle,
            task,
        }
    }

    async fn spawn_upload_protocol_server() -> UploadProtocolTestServer {
        async fn protocol(
            req: HttpRequest,
            body: web::Bytes,
            state: web::Data<Arc<Mutex<UploadProtocolState>>>,
        ) -> HttpResponse {
            let method = req.method().to_string();
            let path = req.uri().path().to_string();
            let authorization = req
                .headers()
                .get("authorization")
                .and_then(|value| value.to_str().ok())
                .map(str::to_string);
            let content_range = req
                .headers()
                .get("content-range")
                .and_then(|value| value.to_str().ok())
                .map(str::to_string);
            let content_length = req
                .headers()
                .get("content-length")
                .and_then(|value| value.to_str().ok())
                .map(str::to_string);
            let host = req.connection_info().host().to_string();
            let mut state = state.lock().expect("upload protocol state lock");
            state.methods.push(method.clone());
            state.paths.push(path);
            state.auth_headers.push(authorization);
            state.content_ranges.push(content_range);
            state.content_lengths.push(content_length);
            state.bodies.push(body.to_vec());
            drop(state);

            match method.as_str() {
                "POST" | "GET" => HttpResponse::Ok().json(serde_json::json!({
                    "uploadUrl": format!("http://{host}/upload-session"),
                    "expirationDateTime": "2026-07-20T00:00:00Z",
                    "nextExpectedRanges": ["655360-"]
                })),
                "DELETE" => HttpResponse::NotFound().finish(),
                "PUT" => HttpResponse::Accepted().finish(),
                _ => HttpResponse::MethodNotAllowed().finish(),
            }
        }

        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("listener should bind");
        let base_url = format!(
            "http://{}",
            listener.local_addr().expect("listener addr should exist")
        );
        let state = Arc::new(Mutex::new(UploadProtocolState::default()));
        let state_data = web::Data::new(state.clone());
        let server = HttpServer::new(move || {
            App::new()
                .app_data(state_data.clone())
                .default_service(web::to(protocol))
        })
        .listen(listener)
        .expect("server should listen")
        .run();
        let handle = server.handle();
        let task = tokio::spawn(server);
        UploadProtocolTestServer {
            base_url,
            state,
            handle,
            task,
        }
    }

    #[tokio::test]
    async fn content_redirect_is_followed_before_reading_bytes() {
        let server = spawn_content_redirect_server().await;
        let client = MicrosoftGraphClient::new(MicrosoftGraphClientConfig::new(
            &server.base_url,
            "access-token",
        ))
        .expect("client should build");

        let bytes = client
            .get_bytes("/content")
            .await
            .expect("content should be read");

        assert_eq!(bytes, b"hello-world");
        server.stop().await;
    }

    #[tokio::test]
    async fn content_redirect_preserves_range_on_download_url() {
        let server = spawn_content_redirect_server().await;
        let client = MicrosoftGraphClient::new(MicrosoftGraphClientConfig::new(
            &server.base_url,
            "access-token",
        ))
        .expect("client should build");

        let mut stream = client
            .get_stream("/content", Some(10), Some(5))
            .await
            .expect("stream should be readable");
        let mut bytes = Vec::new();
        tokio::io::AsyncReadExt::read_to_end(&mut stream, &mut bytes)
            .await
            .expect("stream should read");

        assert_eq!(bytes, b"hello-world");
        let ranges = server.ranges.lock().expect("range log lock").clone();
        assert_eq!(ranges, vec![Some("bytes=10-14".to_string())]);
        server.stop().await;
    }

    #[tokio::test]
    async fn graph_unauthorized_refreshes_token_and_retries_once() {
        let server = spawn_authorized_get_server().await;
        let provider = Arc::new(RefreshingTestTokenProvider::new("refreshed-token"));
        let client = MicrosoftGraphClient::new(MicrosoftGraphClientConfig::with_token_provider(
            &server.base_url,
            provider.clone(),
        ))
        .expect("client should build");

        let drive = client.get_me_drive().await.expect("retry should succeed");

        assert_eq!(drive.id, "drive-id");
        assert_eq!(provider.access_token_calls(), 1);
        assert_eq!(provider.refresh_calls(), 1);
        let auth_headers = server
            .auth_headers
            .lock()
            .expect("auth header log lock")
            .clone();
        assert_eq!(
            auth_headers,
            vec![
                Some("Bearer expired-token".to_string()),
                Some("Bearer refreshed-token".to_string()),
            ]
        );
        server.stop().await;
    }

    #[tokio::test]
    async fn graph_unauthorized_returns_auth_error_when_refresh_fails() {
        let server = spawn_authorized_get_server().await;
        let provider = Arc::new(RefreshingTestTokenProvider::failing());
        let client = MicrosoftGraphClient::new(MicrosoftGraphClientConfig::with_token_provider(
            &server.base_url,
            provider.clone(),
        ))
        .expect("client should build");

        let error = client.get_me_drive().await.unwrap_err();

        assert_eq!(error.storage_error_kind(), Some(StorageErrorKind::Auth));
        assert_eq!(provider.access_token_calls(), 1);
        assert_eq!(provider.refresh_calls(), 1);
        let auth_headers = server
            .auth_headers
            .lock()
            .expect("auth header log lock")
            .clone();
        assert_eq!(auth_headers, vec![Some("Bearer expired-token".to_string())]);
        server.stop().await;
    }

    #[tokio::test]
    async fn graph_unauthorized_rejects_empty_refreshed_token() {
        let server = spawn_authorized_get_server().await;
        let provider = Arc::new(RefreshingTestTokenProvider::new(" "));
        let client = MicrosoftGraphClient::new(MicrosoftGraphClientConfig::with_token_provider(
            &server.base_url,
            provider.clone(),
        ))
        .expect("client should build");

        let error = client.get_me_drive().await.unwrap_err();

        assert_eq!(error.storage_error_kind(), Some(StorageErrorKind::Auth));
        assert_eq!(provider.access_token_calls(), 1);
        assert_eq!(provider.refresh_calls(), 1);
        let auth_headers = server
            .auth_headers
            .lock()
            .expect("auth header log lock")
            .clone();
        assert_eq!(auth_headers, vec![Some("Bearer expired-token".to_string())]);
        server.stop().await;
    }

    #[tokio::test]
    async fn graph_non_unauthorized_error_does_not_refresh_token() {
        let server = spawn_forbidden_get_server().await;
        let provider = Arc::new(RefreshingTestTokenProvider::new("refreshed-token"));
        let client = MicrosoftGraphClient::new(MicrosoftGraphClientConfig::with_token_provider(
            &server.base_url,
            provider.clone(),
        ))
        .expect("client should build");

        let error = client.get_me_drive().await.unwrap_err();

        assert_eq!(
            error.storage_error_kind(),
            Some(StorageErrorKind::Permission)
        );
        assert_eq!(provider.access_token_calls(), 1);
        assert_eq!(provider.refresh_calls(), 0);
        let auth_headers = server
            .auth_headers
            .lock()
            .expect("auth header log lock")
            .clone();
        assert_eq!(auth_headers, vec![Some("Bearer expired-token".to_string())]);
        server.stop().await;
    }

    #[tokio::test]
    async fn create_upload_session_sends_graph_auth_and_replace_conflict_behavior() {
        let server = spawn_upload_protocol_server().await;
        let client = MicrosoftGraphClient::new(MicrosoftGraphClientConfig::new(
            &server.base_url,
            "server-oauth-token",
        ))
        .expect("client should build");

        let session = client
            .create_upload_session(
                "/drives/drive-id/items/root-id:/files/upload-1:/createUploadSession",
            )
            .await
            .expect("upload session should be created");

        assert_eq!(
            session.upload_url,
            format!("{}/upload-session", server.base_url)
        );
        assert_eq!(session.next_expected_ranges, vec!["655360-".to_string()]);
        {
            let state = server.state.lock().expect("upload protocol state lock");
            assert_eq!(state.methods, vec!["POST"]);
            assert_eq!(
                state.paths,
                vec!["/v1.0/drives/drive-id/items/root-id:/files/upload-1:/createUploadSession"]
            );
            assert_eq!(
                state.auth_headers,
                vec![Some("Bearer server-oauth-token".to_string())]
            );
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(&state.bodies[0])
                    .expect("request body should be JSON"),
                serde_json::json!({
                    "item": {
                        "@microsoft.graph.conflictBehavior": "replace"
                    }
                })
            );
        }
        server.stop().await;
    }

    #[tokio::test]
    async fn provider_upload_fragment_uses_range_headers_without_oauth() {
        let server = spawn_upload_protocol_server().await;
        let client = MicrosoftGraphClient::new(MicrosoftGraphClientConfig::new(
            &server.base_url,
            "server-oauth-token",
        ))
        .expect("client should build");

        client
            .upload_session_fragment(
                &format!("{}/upload-session", server.base_url),
                320 * 1024,
                1024 * 1024,
                vec![1, 2, 3, 4],
            )
            .await
            .expect("provider fragment should upload");

        {
            let state = server.state.lock().expect("upload protocol state lock");
            assert_eq!(state.methods, vec!["PUT"]);
            assert_eq!(state.auth_headers, vec![None]);
            assert_eq!(
                state.content_ranges,
                vec![Some("bytes 327680-327683/1048576".to_string())]
            );
            assert_eq!(state.content_lengths, vec![Some("4".to_string())]);
            assert_eq!(state.bodies, vec![vec![1, 2, 3, 4]]);
        }
        server.stop().await;
    }

    #[tokio::test]
    async fn provider_upload_session_status_and_not_found_abort_are_supported() {
        let server = spawn_upload_protocol_server().await;
        let client = MicrosoftGraphClient::new(MicrosoftGraphClientConfig::new(
            &server.base_url,
            "server-oauth-token",
        ))
        .expect("client should build");
        let upload_url = format!("{}/upload-session", server.base_url);

        let status = client
            .query_upload_session(&upload_url)
            .await
            .expect("provider status should be queryable");
        assert_eq!(status.next_expected_ranges, vec!["655360-".to_string()]);

        client
            .abort_upload_session(&upload_url)
            .await
            .expect("a missing provider session is already canceled");

        {
            let state = server.state.lock().expect("upload protocol state lock");
            assert_eq!(state.methods, vec!["GET", "DELETE"]);
            assert_eq!(state.auth_headers, vec![None, None]);
        }
        server.stop().await;
    }

    #[test]
    fn range_header_formats_bounded_and_open_ranges() {
        assert_eq!(range_header(None, None).unwrap(), None);
        assert_eq!(
            range_header(Some(10), Some(20)).unwrap().as_deref(),
            Some("bytes=10-29")
        );
        assert_eq!(
            range_header(Some(10), None).unwrap().as_deref(),
            Some("bytes=10-")
        );
    }

    #[test]
    fn provider_fragment_rejects_empty_data_and_range_overflow() {
        let empty = futures::executor::block_on(async {
            let client = MicrosoftGraphClient::new(MicrosoftGraphClientConfig::new(
                "https://graph.microsoft.com",
                "token",
            ))
            .expect("client should build");
            client
                .upload_session_fragment("https://upload.example/session", 0, 1, Vec::new())
                .await
                .unwrap_err()
        });
        assert_eq!(
            empty.storage_error_kind(),
            Some(StorageErrorKind::Misconfigured)
        );

        let overflow = futures::executor::block_on(async {
            let client = MicrosoftGraphClient::new(MicrosoftGraphClientConfig::new(
                "https://graph.microsoft.com",
                "token",
            ))
            .expect("client should build");
            client
                .upload_session_fragment(
                    "https://upload.example/session",
                    u64::MAX,
                    u64::MAX,
                    vec![1],
                )
                .await
                .unwrap_err()
        });
        assert_eq!(
            overflow.storage_error_kind(),
            Some(StorageErrorKind::Misconfigured)
        );
    }

    #[test]
    fn client_rejects_missing_access_token() {
        let error = MicrosoftGraphClient::new(MicrosoftGraphClientConfig::new(
            "https://graph.microsoft.com",
            " ",
        ))
        .unwrap_err();

        assert_eq!(error.storage_error_kind(), Some(StorageErrorKind::Auth));
    }
}
