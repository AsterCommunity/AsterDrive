use async_trait::async_trait;
use bytes::Bytes;
use chrono::{DateTime, Utc};
use futures::TryStreamExt;
use reqwest::header::{CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE, LOCATION, RANGE};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt, sync::Arc};
use tokio::io::AsyncRead;
use tokio_util::io::StreamReader;

use crate::config::OUTBOUND_HTTP_USER_AGENT;
use aster_drive_storage::{MapStorageErr, Result, StorageErrorKind, storage_driver_error};

#[async_trait]
pub trait GoogleDriveAccessTokenProvider: Send + Sync + fmt::Debug {
    async fn access_token(&self) -> Result<String>;
    async fn refresh_access_token(&self) -> Result<String>;
}

#[derive(Clone)]
struct StaticTokenProvider {
    token: SecretString,
}

impl fmt::Debug for StaticTokenProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StaticTokenProvider")
            .field("token", &"***REDACTED***")
            .finish()
    }
}

#[async_trait]
impl GoogleDriveAccessTokenProvider for StaticTokenProvider {
    async fn access_token(&self) -> Result<String> {
        Ok(self.token.expose_secret().to_string())
    }
    async fn refresh_access_token(&self) -> Result<String> {
        Err(storage_driver_error(
            StorageErrorKind::Auth,
            "Google Drive static token cannot be refreshed",
        ))
    }
}

#[derive(Clone)]
pub struct GoogleDriveClientConfig {
    pub token_provider: Arc<dyn GoogleDriveAccessTokenProvider>,
}

impl GoogleDriveClientConfig {
    pub fn new(access_token: impl Into<String>) -> Self {
        Self {
            token_provider: Arc::new(StaticTokenProvider {
                token: SecretString::from(access_token.into()),
            }),
        }
    }
    pub fn with_token_provider(token_provider: Arc<dyn GoogleDriveAccessTokenProvider>) -> Self {
        Self { token_provider }
    }
}

#[derive(Clone)]
pub struct GoogleDriveClient {
    config: GoogleDriveClientConfig,
    http: reqwest::Client,
}

impl fmt::Debug for GoogleDriveClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GoogleDriveClient")
            .field("config", &"***REDACTED***")
            .finish()
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct GoogleDriveFile {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub size: Option<i64>,
    #[serde(rename = "mimeType", default)]
    pub mime_type: Option<String>,
    #[serde(default)]
    pub trashed: bool,
    #[serde(rename = "appProperties", default)]
    pub app_properties: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct ResumableSession {
    pub url: String,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct ResumableFragmentOutcome {
    pub completed: bool,
    pub next_offset: Option<u64>,
    pub next_expected_ranges: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct About {
    #[allow(dead_code)]
    storage_quota: Option<StorageQuota>,
}
#[derive(Debug, Deserialize)]
struct StorageQuota {
    #[allow(dead_code)]
    limit: Option<i64>,
    #[allow(dead_code)]
    usage: Option<i64>,
    #[allow(dead_code)]
    usage_in_drive: Option<i64>,
}
#[derive(Debug, Deserialize)]
struct FileList {
    #[serde(default)]
    files: Vec<GoogleDriveFile>,
}

impl GoogleDriveClient {
    pub fn new(config: GoogleDriveClientConfig) -> Result<Self> {
        let http = reqwest::ClientBuilder::new()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(90))
            .user_agent(OUTBOUND_HTTP_USER_AGENT)
            .build()
            .map_storage_err_ctx(
                StorageErrorKind::Misconfigured,
                "failed to build Google Drive HTTP client",
            )?;
        Ok(Self { config, http })
    }

    async fn token(&self) -> Result<String> {
        self.config.token_provider.access_token().await
    }

    async fn request(
        &self,
        request: reqwest::RequestBuilder,
        context: &str,
    ) -> Result<reqwest::Response> {
        let token = self.token().await?;
        let retry = request.try_clone();
        let mut response = request.bearer_auth(token).send().await.map_err(|error| {
            storage_driver_error(StorageErrorKind::Transient, format!("{context}: {error}"))
        })?;
        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            if let Some(retry) = retry {
                let token = self.config.token_provider.refresh_access_token().await?;
                response = retry.bearer_auth(token).send().await.map_err(|error| {
                    storage_driver_error(
                        StorageErrorKind::Transient,
                        format!("{context} after token refresh: {error}"),
                    )
                })?;
            }
        }
        if response.status().is_success() {
            return Ok(response);
        }
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        let kind = match status.as_u16() {
            401 => StorageErrorKind::Auth,
            403 => StorageErrorKind::Permission,
            404 => StorageErrorKind::NotFound,
            409 => StorageErrorKind::Precondition,
            429 => StorageErrorKind::RateLimited,
            500..=599 => StorageErrorKind::Transient,
            _ => StorageErrorKind::Unknown,
        };
        Err(storage_driver_error(
            kind,
            format!(
                "{context}: HTTP {status}: {}",
                sanitize_provider_error(&body)
            ),
        ))
    }

    fn api_url(path: &str) -> String {
        format!("https://www.googleapis.com/drive/v3{path}")
    }

    fn file_url(file_id: &str) -> String {
        let mut url = url::Url::parse(&Self::api_url("/files"))
            .expect("Google Drive API URL is static and valid");
        url.path_segments_mut()
            .expect("Google Drive API URL supports path segments")
            .push(file_id);
        url.into()
    }

    pub async fn about_storage(&self) -> Result<()> {
        self.request(
            self.http.get(with_query(
                Self::api_url("/about"),
                &[("fields", "storageQuota")],
            )),
            "Google Drive readiness check",
        )
        .await?
        .json::<About>()
        .await
        .map(|_| ())
        .map_err(|error| {
            storage_driver_error(
                StorageErrorKind::Transient,
                format!("decode Google Drive readiness response: {error}"),
            )
        })
    }

    pub async fn get_folder(
        &self,
        folder_id: &str,
        shared_drive_id: Option<&str>,
    ) -> Result<GoogleDriveFile> {
        let mut params = vec![("fields", "id,name,mimeType,trashed")];
        if shared_drive_id.is_some() {
            params.extend_from_slice(&[
                ("supportsAllDrives", "true"),
                ("includeItemsFromAllDrives", "true"),
            ]);
        }
        let file = self
            .request(
                self.http
                    .get(with_query(Self::file_url(folder_id), &params)),
                "get Google Drive root folder",
            )
            .await?
            .json::<GoogleDriveFile>()
            .await
            .map_err(|error| {
                storage_driver_error(
                    StorageErrorKind::Transient,
                    format!("decode Google Drive folder response: {error}"),
                )
            })?;
        if file.mime_type.as_deref() != Some("application/vnd.google-apps.folder") || file.trashed {
            return Err(storage_driver_error(
                StorageErrorKind::Misconfigured,
                "Google Drive root_folder_id is not an active folder",
            ));
        }
        Ok(file)
    }

    pub async fn find_managed_file(
        &self,
        parent_id: Option<&str>,
        shared_drive_id: Option<&str>,
        key: &str,
        policy_id: i64,
    ) -> Result<Option<GoogleDriveFile>> {
        let mut query = format!(
            "trashed = false and appProperties has {{ key = 'asterdrive_managed' and value = 'true' }} and appProperties has {{ key = 'asterdrive_policy_id' and value = '{policy_id}' }} and appProperties has {{ key = 'asterdrive_object_key' and value = '{}' }}",
            escape_query_value(key)
        );
        if let Some(parent) = parent_id {
            query.push_str(&format!(" and '{}' in parents", escape_query_value(parent)));
        }
        let mut params = vec![
            ("q", query.as_str()),
            (
                "spaces",
                if parent_id == Some("appDataFolder") {
                    "appDataFolder"
                } else {
                    "drive"
                },
            ),
            ("pageSize", "2"),
            ("fields", "files(id,name,size,mimeType,appProperties)"),
        ];
        if shared_drive_id.is_some() {
            params.extend_from_slice(&[
                ("supportsAllDrives", "true"),
                ("includeItemsFromAllDrives", "true"),
                ("corpora", "drive"),
                ("driveId", shared_drive_id.unwrap_or_default()),
            ]);
        }
        let request = self.http.get(with_query(Self::api_url("/files"), &params));
        let list = self
            .request(request, "list Google Drive managed object")
            .await?
            .json::<FileList>()
            .await
            .map_err(|error| {
                storage_driver_error(
                    StorageErrorKind::Transient,
                    format!("decode Google Drive file list: {error}"),
                )
            })?;
        if list.files.len() > 1 {
            return Err(storage_driver_error(
                StorageErrorKind::Precondition,
                "multiple Google Drive objects match one AsterDrive object key",
            ));
        }
        Ok(list.files.into_iter().next())
    }

    pub async fn create_resumable_upload(
        &self,
        parent_id: Option<&str>,
        shared_drive_id: Option<&str>,
        key: &str,
        properties: BTreeMap<String, String>,
        size: u64,
    ) -> Result<ResumableSession> {
        #[derive(Serialize)]
        struct Metadata<'a> {
            name: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            parents: Option<Vec<&'a str>>,
            #[serde(rename = "appProperties")]
            app_properties: BTreeMap<String, String>,
        }
        let metadata = Metadata {
            name: key,
            parents: parent_id.map(|id| vec![id]),
            app_properties: properties,
        };
        let mut params = vec![
            ("uploadType", "resumable"),
            ("fields", "id,name,size,mimeType,appProperties"),
        ];
        if shared_drive_id.is_some() {
            params.push(("supportsAllDrives", "true"));
        }
        let request = self
            .http
            .post(with_query(
                "https://www.googleapis.com/upload/drive/v3/files",
                &params,
            ))
            .header(CONTENT_TYPE, "application/json")
            .header("X-Upload-Content-Type", "application/octet-stream")
            .header("X-Upload-Content-Length", size.to_string())
            .json(&metadata);
        let response = self
            .request(request, "create Google Drive resumable upload")
            .await?;
        let url = response
            .headers()
            .get(LOCATION)
            .and_then(|value| value.to_str().ok())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                storage_driver_error(
                    StorageErrorKind::Transient,
                    "Google Drive resumable upload response missing Location",
                )
            })?;
        Ok(ResumableSession {
            url: url.to_string(),
            expires_at: None,
        })
    }

    pub async fn upload_resumable_fragment(
        &self,
        session: &ResumableSession,
        start: u64,
        total: u64,
        data: Bytes,
    ) -> Result<ResumableFragmentOutcome> {
        self.upload_resumable_fragment_url(&session.url, start, total, data)
            .await
    }

    pub async fn upload_resumable_fragment_url(
        &self,
        url: &str,
        start: u64,
        total: u64,
        data: Bytes,
    ) -> Result<ResumableFragmentOutcome> {
        if data.is_empty() && total != 0 {
            return Err(storage_driver_error(
                StorageErrorKind::Precondition,
                "Google Drive resumable fragments cannot be empty",
            ));
        }
        let end = start
            .checked_add(data.len() as u64)
            .and_then(|value| value.checked_sub(1))
            .unwrap_or(0);
        let token = self.token().await?;
        let response = self
            .http
            .put(url)
            .bearer_auth(token)
            .header(CONTENT_LENGTH, data.len())
            .header(
                CONTENT_RANGE,
                if total == 0 {
                    "bytes */0".to_string()
                } else {
                    format!("bytes {start}-{end}/{total}")
                },
            )
            .body(data)
            .send()
            .await
            .map_err(|error| {
                storage_driver_error(
                    StorageErrorKind::Transient,
                    format!("upload Google Drive resumable fragment: {error}"),
                )
            })?;
        if response.status().is_success() {
            return Ok(ResumableFragmentOutcome {
                completed: true,
                next_offset: None,
                next_expected_ranges: Vec::new(),
            });
        }
        if response.status() == reqwest::StatusCode::PERMANENT_REDIRECT {
            let range = response
                .headers()
                .get(RANGE)
                .and_then(|value| value.to_str().ok())
                .map(ToOwned::to_owned);
            return Ok(ResumableFragmentOutcome {
                completed: false,
                next_offset: parse_range_end(range.as_deref()).map(|value| value + 1),
                next_expected_ranges: range.into_iter().collect(),
            });
        }
        Err(map_response_error(response, "upload Google Drive resumable fragment").await)
    }

    pub async fn query_resumable_upload(
        &self,
        upload_url: &str,
    ) -> Result<ResumableFragmentOutcome> {
        let token = self.token().await?;
        let response = self
            .http
            .put(upload_url)
            .bearer_auth(token)
            .header(CONTENT_LENGTH, 0)
            .header(CONTENT_RANGE, "bytes */*")
            .body(Vec::new())
            .send()
            .await
            .map_err(|error| {
                storage_driver_error(
                    StorageErrorKind::Transient,
                    format!("query Google Drive resumable upload: {error}"),
                )
            })?;
        if response.status() != reqwest::StatusCode::PERMANENT_REDIRECT
            && !response.status().is_success()
        {
            return Err(map_response_error(response, "query Google Drive resumable upload").await);
        }
        let range = response
            .headers()
            .get(RANGE)
            .and_then(|value| value.to_str().ok())
            .map(ToOwned::to_owned);
        Ok(ResumableFragmentOutcome {
            completed: response.status().is_success(),
            next_offset: parse_range_end(range.as_deref()).map(|value| value + 1),
            next_expected_ranges: range.into_iter().collect(),
        })
    }

    pub async fn abort_resumable_upload(&self, upload_url: &str) -> Result<()> {
        let token = self.token().await?;
        let response = self
            .http
            .delete(upload_url)
            .bearer_auth(token)
            .send()
            .await
            .map_err(|error| {
                storage_driver_error(
                    StorageErrorKind::Transient,
                    format!("abort Google Drive resumable upload: {error}"),
                )
            })?;
        if response.status().is_success() || response.status() == reqwest::StatusCode::NOT_FOUND {
            Ok(())
        } else {
            Err(map_response_error(response, "abort Google Drive resumable upload").await)
        }
    }

    pub async fn download_bytes(&self, file_id: &str) -> Result<Vec<u8>> {
        self.request(
            self.http
                .get(with_query(Self::file_url(file_id), &[("alt", "media")])),
            "download Google Drive object",
        )
        .await?
        .bytes()
        .await
        .map(|value| value.to_vec())
        .map_err(|error| {
            storage_driver_error(
                StorageErrorKind::Transient,
                format!("read Google Drive object: {error}"),
            )
        })
    }

    pub async fn download_stream(
        &self,
        file_id: &str,
        offset: Option<u64>,
        length: Option<u64>,
    ) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
        let mut request = self
            .http
            .get(with_query(Self::file_url(file_id), &[("alt", "media")]));
        if let Some(offset) = offset {
            if length == Some(0) {
                return Err(storage_driver_error(
                    StorageErrorKind::Precondition,
                    "Google Drive range length cannot be zero",
                ));
            }
            let end = length
                .and_then(|len| offset.checked_add(len)?.checked_sub(1))
                .unwrap_or(u64::MAX);
            request = request.header(RANGE, format!("bytes={offset}-{end}"));
        }
        let response = self.request(request, "stream Google Drive object").await?;
        let stream = response.bytes_stream().map_err(std::io::Error::other);
        Ok(Box::new(StreamReader::new(stream)))
    }

    pub async fn delete_file(&self, file_id: &str, shared_drive_id: Option<&str>) -> Result<()> {
        let mut request = self.http.delete(Self::file_url(file_id));
        if shared_drive_id.is_some() {
            request = self.http.delete(with_query(
                Self::file_url(file_id),
                &[("supportsAllDrives", "true")],
            ));
        }
        self.request(request, "delete Google Drive managed object")
            .await
            .map(|_| ())
    }
}

async fn map_response_error(
    response: reqwest::Response,
    context: &str,
) -> aster_drive_storage::StorageError {
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    let kind = match status.as_u16() {
        401 => StorageErrorKind::Auth,
        403 => StorageErrorKind::Permission,
        404 => StorageErrorKind::NotFound,
        409 => StorageErrorKind::Precondition,
        429 => StorageErrorKind::RateLimited,
        500..=599 => StorageErrorKind::Transient,
        _ => StorageErrorKind::Unknown,
    };
    storage_driver_error(
        kind,
        format!(
            "{context}: HTTP {status}: {}",
            sanitize_provider_error(&body)
        ),
    )
}

fn escape_query_value(value: &str) -> String {
    value.replace('\\', "\\\\").replace('\'', "\\'")
}
fn with_query(raw: impl AsRef<str>, pairs: &[(&str, &str)]) -> String {
    let mut url = url::Url::parse(raw.as_ref()).expect("Google Drive URL is static and valid");
    {
        let mut query = url.query_pairs_mut();
        for (key, value) in pairs {
            query.append_pair(key, value);
        }
    }
    url.into()
}
fn parse_range_end(value: Option<&str>) -> Option<u64> {
    value?.rsplit_once('-')?.1.parse().ok()
}
fn sanitize_provider_error(body: &str) -> String {
    body.chars().filter(|c| !c.is_control()).take(512).collect()
}
