//! Google Drive storage driver.

use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, AeadCore, KeyInit, OsRng},
};
use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use bytes::Bytes;
use futures::{Stream, TryStreamExt};
use hkdf::Hkdf;
use reqwest::header;
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::Sha256;
use std::collections::HashMap;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio_util::io::{ReaderStream, StreamReader};

use crate::api::subcode::ApiSubcode;
use crate::entities::storage_policy;
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::storage::error::{
    StorageErrorKind, storage_driver_error, storage_driver_error_with_subcode,
};
use crate::storage::traits::driver::{BlobMetadata, StorageDriver};
use crate::storage::traits::extensions::StreamUploadDriver;
use crate::types::{StoragePolicyOptions, parse_storage_policy_options};
use crate::utils::OUTBOUND_HTTP_USER_AGENT;
use crate::utils::numbers;

const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const DRIVE_FILES_URL: &str = "https://www.googleapis.com/drive/v3/files";
const DRIVE_UPLOAD_FILES_URL: &str = "https://www.googleapis.com/upload/drive/v3/files";
const APP_PROPERTY_MANAGED: &str = "asterdrive_managed";
const APP_PROPERTY_STORAGE_PATH: &str = "asterdrive_storage_path";
const APP_PROPERTY_POLICY_ID: &str = "asterdrive_policy_id";
const DEFAULT_MIME_TYPE: &str = "application/octet-stream";
const CONNECT_TIMEOUT_SECS: u64 = 10;
const STREAM_UPLOAD_BUFFER_SIZE: usize = 256 * 1024;

pub const REQUIRED_DRIVE_SCOPE: &str = "https://www.googleapis.com/auth/drive";
pub const REQUIRED_DRIVE_APPDATA_SCOPE: &str = "https://www.googleapis.com/auth/drive.appdata";
pub const REQUIRED_USERINFO_EMAIL_SCOPE: &str = "https://www.googleapis.com/auth/userinfo.email";
pub const REQUIRED_USERINFO_PROFILE_SCOPE: &str =
    "https://www.googleapis.com/auth/userinfo.profile";
const TOKEN_CIPHERTEXT_VERSION: &str = "v1";
const GOOGLE_DRIVE_TOKEN_INFO: &[u8] = b"asterdrive:google-drive-storage-token:v1";

pub struct GoogleDriveDriver {
    client: reqwest::Client,
    client_id: String,
    client_secret: String,
    refresh_token: String,
    parent_id: String,
    shared_drive_id: Option<String>,
    use_app_data_folder: bool,
    policy_id: i64,
}

#[derive(Debug, Deserialize)]
struct GoogleDriveTokenResponse {
    access_token: String,
    #[serde(default)]
    token_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GoogleDriveErrorResponse {
    #[serde(default)]
    error: Option<Value>,
    #[serde(default)]
    error_description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GoogleDriveFileList {
    #[serde(default)]
    files: Vec<GoogleDriveFile>,
}

#[derive(Debug, Deserialize)]
struct GoogleDriveFile {
    id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    size: Option<String>,
    #[serde(default, rename = "mimeType")]
    mime_type: Option<String>,
    #[serde(default, rename = "appProperties")]
    app_properties: HashMap<String, String>,
}

struct SizedReaderStream<R> {
    stream: ReaderStream<R>,
    remaining: u64,
    finished: bool,
}

impl<R> SizedReaderStream<R>
where
    R: AsyncRead + Unpin,
{
    fn new(reader: R, size: u64) -> Self {
        Self {
            stream: ReaderStream::with_capacity(reader, STREAM_UPLOAD_BUFFER_SIZE),
            remaining: size,
            finished: false,
        }
    }
}

impl<R> Stream for SizedReaderStream<R>
where
    R: AsyncRead + Unpin + Send + Sync + 'static,
{
    type Item = std::result::Result<Bytes, std::io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.finished {
            return Poll::Ready(None);
        }

        match Pin::new(&mut self.stream).poll_next(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Some(Ok(chunk))) => {
                let chunk_len = match numbers::usize_to_u64(
                    chunk.len(),
                    "Google Drive upload stream chunk size",
                ) {
                    Ok(value) => value,
                    Err(error) => {
                        self.finished = true;
                        return Poll::Ready(Some(Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            error.to_string(),
                        ))));
                    }
                };
                if chunk_len > self.remaining {
                    self.finished = true;
                    return Poll::Ready(Some(Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "upload stream exceeded declared size",
                    ))));
                }

                self.remaining -= chunk_len;
                Poll::Ready(Some(Ok(chunk)))
            }
            Poll::Ready(Some(Err(error))) => {
                self.finished = true;
                Poll::Ready(Some(Err(error)))
            }
            Poll::Ready(None) => {
                self.finished = true;
                if self.remaining == 0 {
                    Poll::Ready(None)
                } else {
                    Poll::Ready(Some(Err(std::io::Error::new(
                        std::io::ErrorKind::UnexpectedEof,
                        format!(
                            "upload stream ended before declared size: {} bytes missing",
                            self.remaining
                        ),
                    ))))
                }
            }
        }
    }
}

impl GoogleDriveDriver {
    pub fn validate_policy(policy: &storage_policy::Model) -> Result<()> {
        if policy.access_key.trim().is_empty() {
            return Err(google_drive_storage_error(
                StorageErrorKind::Auth,
                ApiSubcode::GoogleDriveMisconfigured,
                "Google Drive client id is required",
            ));
        }
        if policy.secret_key.trim().is_empty() {
            return Err(google_drive_storage_error(
                StorageErrorKind::Auth,
                ApiSubcode::GoogleDriveMisconfigured,
                "Google Drive client secret is required",
            ));
        }
        Ok(())
    }

    pub fn new(policy: &storage_policy::Model) -> Result<Self> {
        Self::validate_policy(policy)?;
        let options = parse_storage_policy_options(policy.options.as_ref());
        let refresh_token = google_drive_refresh_token(policy.id, &options)?;
        let client = reqwest::ClientBuilder::new()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(CONNECT_TIMEOUT_SECS))
            .user_agent(OUTBOUND_HTTP_USER_AGENT)
            .build()
            .map_aster_err_ctx(
                "failed to build Google Drive HTTP client",
                AsterError::internal_error,
            )?;
        Ok(Self {
            client,
            client_id: policy.access_key.trim().to_string(),
            client_secret: policy.secret_key.trim().to_string(),
            refresh_token,
            parent_id: google_drive_parent_id(&options),
            shared_drive_id: normalize_optional(&options.google_drive_shared_drive_id),
            use_app_data_folder: options.google_drive_use_app_data_folder.unwrap_or(false),
            policy_id: policy.id,
        })
    }

    async fn access_token(&self) -> Result<String> {
        let form = {
            let mut form = url::form_urlencoded::Serializer::new(String::new());
            form.append_pair("grant_type", "refresh_token");
            form.append_pair("refresh_token", &self.refresh_token);
            form.append_pair("client_id", &self.client_id);
            form.append_pair("client_secret", &self.client_secret);
            form.finish()
        };

        let response = self
            .client
            .post(TOKEN_URL)
            .header(header::ACCEPT, "application/json")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(form)
            .send()
            .await
            .map_err(|error| {
                google_drive_storage_error(
                    StorageErrorKind::Transient,
                    ApiSubcode::GoogleDriveTransient,
                    format!("Google Drive token refresh failed: {error}"),
                )
            })?;
        if !response.status().is_success() {
            return Err(map_google_drive_error(response, "Google Drive token refresh").await);
        }
        let token = response
            .json::<GoogleDriveTokenResponse>()
            .await
            .map_err(|error| {
                google_drive_storage_error(
                    StorageErrorKind::Transient,
                    ApiSubcode::GoogleDriveTransient,
                    format!("Google Drive token refresh response is invalid: {error}"),
                )
            })?;
        if token.access_token.trim().is_empty() {
            return Err(google_drive_storage_error(
                StorageErrorKind::Auth,
                ApiSubcode::GoogleDriveConnectionExpired,
                "Google Drive token refresh response missing access_token",
            ));
        }
        if let Some(token_type) = token.token_type.as_deref()
            && !token_type.eq_ignore_ascii_case("bearer")
        {
            return Err(google_drive_storage_error(
                StorageErrorKind::Auth,
                ApiSubcode::GoogleDriveConnectionExpired,
                "Google Drive token refresh response returned unsupported token_type",
            ));
        }
        Ok(token.access_token)
    }

    async fn find_managed_file(
        &self,
        path: &str,
        access_token: &str,
    ) -> Result<Option<GoogleDriveFile>> {
        let mut url = reqwest::Url::parse(DRIVE_FILES_URL)
            .map_aster_err_ctx("invalid Google Drive files URL", AsterError::internal_error)?;
        let query = google_drive_managed_file_query(&self.parent_id, path, self.policy_id);
        {
            let mut pairs = url.query_pairs_mut();
            pairs.append_pair("q", &query);
            pairs.append_pair("fields", "files(id,name,size,mimeType,appProperties)");
            if self.use_app_data_folder {
                pairs.append_pair("spaces", "appDataFolder");
            } else {
                pairs.append_pair("supportsAllDrives", "true");
                pairs.append_pair("includeItemsFromAllDrives", "true");
                if let Some(shared_drive_id) = self.shared_drive_id.as_deref() {
                    pairs.append_pair("corpora", "drive");
                    pairs.append_pair("driveId", shared_drive_id);
                }
            }
        }
        let response = self
            .client
            .get(url)
            .bearer_auth(access_token)
            .header(header::ACCEPT, "application/json")
            .send()
            .await
            .map_err(|error| {
                google_drive_storage_error(
                    StorageErrorKind::Transient,
                    ApiSubcode::GoogleDriveTransient,
                    format!("Google Drive file lookup failed: {error}"),
                )
            })?;
        if !response.status().is_success() {
            return Err(map_google_drive_error(response, "Google Drive file lookup").await);
        }
        let mut files = response
            .json::<GoogleDriveFileList>()
            .await
            .map_err(|error| {
                google_drive_storage_error(
                    StorageErrorKind::Transient,
                    ApiSubcode::GoogleDriveTransient,
                    format!("Google Drive file lookup response is invalid: {error}"),
                )
            })?
            .files;
        Ok(files
            .drain(..)
            .find(|file| self.file_is_managed_path(file, path)))
    }

    async fn find_managed_file_id(&self, path: &str, access_token: &str) -> Result<Option<String>> {
        Ok(self
            .find_managed_file(path, access_token)
            .await?
            .map(|file| file.id))
    }

    fn file_is_managed_path(&self, file: &GoogleDriveFile, path: &str) -> bool {
        file.app_properties
            .get(APP_PROPERTY_MANAGED)
            .is_some_and(|value| value == "true")
            && file
                .app_properties
                .get(APP_PROPERTY_STORAGE_PATH)
                .is_some_and(|value| value == path)
            && file
                .app_properties
                .get(APP_PROPERTY_POLICY_ID)
                .is_some_and(|value| value == &self.policy_id.to_string())
    }

    fn append_upload_query_params(&self, url: &mut reqwest::Url) {
        let mut pairs = url.query_pairs_mut();
        pairs.append_pair("uploadType", "resumable");
        pairs.append_pair("fields", "id,name,size,mimeType,appProperties");
        if !self.use_app_data_folder {
            pairs.append_pair("supportsAllDrives", "true");
        }
    }

    fn metadata_body(&self, path: &str, include_parent: bool) -> Value {
        let mut body = json!({
            "name": google_drive_object_name(path),
            "mimeType": DEFAULT_MIME_TYPE,
            "appProperties": {
                APP_PROPERTY_MANAGED: "true",
                APP_PROPERTY_STORAGE_PATH: path,
                APP_PROPERTY_POLICY_ID: self.policy_id.to_string(),
            },
        });
        if include_parent {
            body["parents"] = json!([self.parent_id]);
        }
        body
    }

    async fn start_resumable_upload(
        &self,
        path: &str,
        access_token: &str,
        existing_file_id: Option<&str>,
        size: i64,
    ) -> Result<String> {
        let mut url = if let Some(file_id) = existing_file_id {
            reqwest::Url::parse(&format!("{DRIVE_UPLOAD_FILES_URL}/{file_id}"))
        } else {
            reqwest::Url::parse(DRIVE_UPLOAD_FILES_URL)
        }
        .map_aster_err_ctx(
            "invalid Google Drive upload URL",
            AsterError::internal_error,
        )?;
        self.append_upload_query_params(&mut url);
        let method = if existing_file_id.is_some() {
            reqwest::Method::PATCH
        } else {
            reqwest::Method::POST
        };
        let response = self
            .client
            .request(method, url)
            .bearer_auth(access_token)
            .header(header::ACCEPT, "application/json")
            .header(header::CONTENT_TYPE, "application/json")
            .header("X-Upload-Content-Type", DEFAULT_MIME_TYPE)
            .header("X-Upload-Content-Length", size.to_string())
            .json(&self.metadata_body(path, existing_file_id.is_none()))
            .send()
            .await
            .map_err(|error| {
                google_drive_storage_error(
                    StorageErrorKind::Transient,
                    ApiSubcode::GoogleDriveTransient,
                    format!("Google Drive resumable upload session failed: {error}"),
                )
            })?;
        if !response.status().is_success() {
            return Err(
                map_google_drive_error(response, "Google Drive resumable upload session").await,
            );
        }
        response
            .headers()
            .get(header::LOCATION)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string)
            .ok_or_else(|| {
                google_drive_storage_error(
                    StorageErrorKind::Transient,
                    ApiSubcode::GoogleDriveTransient,
                    "Google Drive resumable upload session missing location",
                )
            })
    }

    async fn upload_reader_to_session(
        &self,
        session_uri: &str,
        reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
        size: i64,
    ) -> Result<()> {
        let content_length = numbers::i64_to_u64(size, "Google Drive upload content length")?;
        let body = reqwest::Body::wrap_stream(SizedReaderStream::new(reader, content_length));
        let mut request = self
            .client
            .put(session_uri)
            .header(header::CONTENT_TYPE, DEFAULT_MIME_TYPE)
            .header(header::CONTENT_LENGTH, content_length.to_string());
        if let Some(range) = google_drive_upload_content_range(content_length) {
            request = request.header("Content-Range", range);
        }
        let response = request.body(body).send().await.map_err(|error| {
            google_drive_storage_error(
                StorageErrorKind::Transient,
                ApiSubcode::GoogleDriveTransient,
                format!("Google Drive resumable upload failed: {error}"),
            )
        })?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(map_google_drive_error(response, "Google Drive resumable upload").await)
        }
    }

    async fn download_response(
        &self,
        path: &str,
        range: Option<String>,
    ) -> Result<reqwest::Response> {
        let access_token = self.access_token().await?;
        let file = self
            .find_managed_file(path, &access_token)
            .await?
            .ok_or_else(|| {
                google_drive_storage_error(
                    StorageErrorKind::NotFound,
                    ApiSubcode::GoogleDriveRemoteNotFound,
                    format!("Google Drive object not found: {path}"),
                )
            })?;
        let mut url = reqwest::Url::parse(&format!("{DRIVE_FILES_URL}/{}", file.id))
            .map_aster_err_ctx(
                "invalid Google Drive download URL",
                AsterError::internal_error,
            )?;
        {
            let mut pairs = url.query_pairs_mut();
            pairs.append_pair("alt", "media");
            if !self.use_app_data_folder {
                pairs.append_pair("supportsAllDrives", "true");
            }
        }
        let mut request = self
            .client
            .get(url)
            .bearer_auth(access_token)
            .header(header::ACCEPT, DEFAULT_MIME_TYPE);
        if let Some(range) = range {
            request = request.header(header::RANGE, range);
        }
        let response = request.send().await.map_err(|error| {
            google_drive_storage_error(
                StorageErrorKind::Transient,
                ApiSubcode::GoogleDriveTransient,
                format!("Google Drive download failed: {error}"),
            )
        })?;
        if response.status().is_success() {
            Ok(response)
        } else {
            Err(map_google_drive_error(response, "Google Drive download").await)
        }
    }
}

#[async_trait]
impl StorageDriver for GoogleDriveDriver {
    async fn put(&self, path: &str, data: &[u8]) -> Result<String> {
        let size = numbers::usize_to_i64(data.len(), "Google Drive put body size")?;
        let reader = Box::new(std::io::Cursor::new(data.to_vec()));
        self.put_reader(path, reader, size).await
    }

    async fn get(&self, path: &str) -> Result<Vec<u8>> {
        let mut stream = self.get_stream(path).await?;
        let mut bytes = Vec::new();
        stream
            .read_to_end(&mut bytes)
            .await
            .map_aster_err_ctx("Google Drive read body", AsterError::storage_driver_error)?;
        Ok(bytes)
    }

    async fn get_stream(&self, path: &str) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
        let response = self.download_response(path, None).await?;
        let stream = response
            .bytes_stream()
            .map_err(|error| std::io::Error::other(error.to_string()));
        Ok(Box::new(StreamReader::new(stream)))
    }

    async fn get_range(
        &self,
        path: &str,
        offset: u64,
        length: Option<u64>,
    ) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
        if length == Some(0) {
            return Ok(Box::new(tokio::io::empty()));
        }
        let range = match length {
            Some(len) => format!("bytes={}-{}", offset, offset + len - 1),
            None => format!("bytes={offset}-"),
        };
        let response = self.download_response(path, Some(range)).await?;
        let stream = response
            .bytes_stream()
            .map_err(|error| std::io::Error::other(error.to_string()));
        Ok(Box::new(StreamReader::new(stream)))
    }

    fn supports_efficient_range(&self) -> bool {
        true
    }

    async fn delete(&self, path: &str) -> Result<()> {
        let access_token = self.access_token().await?;
        let Some(file) = self.find_managed_file(path, &access_token).await? else {
            return Ok(());
        };
        if !self.file_is_managed_path(&file, path) {
            return Err(google_drive_storage_error(
                StorageErrorKind::Precondition,
                ApiSubcode::GoogleDrivePermissionDenied,
                format!(
                    "refusing to delete non-AsterDrive Google Drive object: {}",
                    file.name.unwrap_or(file.id)
                ),
            ));
        }
        let mut url = reqwest::Url::parse(&format!("{DRIVE_FILES_URL}/{}", file.id))
            .map_aster_err_ctx(
                "invalid Google Drive delete URL",
                AsterError::internal_error,
            )?;
        if !self.use_app_data_folder {
            url.query_pairs_mut()
                .append_pair("supportsAllDrives", "true");
        }
        let response = self
            .client
            .delete(url)
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(|error| {
                google_drive_storage_error(
                    StorageErrorKind::Transient,
                    ApiSubcode::GoogleDriveTransient,
                    format!("Google Drive delete failed: {error}"),
                )
            })?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(map_google_drive_error(response, "Google Drive delete").await)
        }
    }

    async fn exists(&self, path: &str) -> Result<bool> {
        let access_token = self.access_token().await?;
        Ok(self
            .find_managed_file_id(path, &access_token)
            .await?
            .is_some())
    }

    async fn metadata(&self, path: &str) -> Result<BlobMetadata> {
        let access_token = self.access_token().await?;
        let file = self
            .find_managed_file(path, &access_token)
            .await?
            .ok_or_else(|| {
                google_drive_storage_error(
                    StorageErrorKind::NotFound,
                    ApiSubcode::GoogleDriveRemoteNotFound,
                    format!("Google Drive object not found: {path}"),
                )
            })?;
        let size = file
            .size
            .as_deref()
            .map(|value| value.parse::<u64>())
            .transpose()
            .map_err(|error| {
                google_drive_storage_error(
                    StorageErrorKind::Transient,
                    ApiSubcode::GoogleDriveTransient,
                    format!("Google Drive object size is invalid: {error}"),
                )
            })?
            .unwrap_or(0);
        Ok(BlobMetadata {
            size,
            content_type: file.mime_type,
        })
    }

    fn as_stream_upload(&self) -> Option<&dyn StreamUploadDriver> {
        Some(self)
    }
}

#[async_trait]
impl StreamUploadDriver for GoogleDriveDriver {
    async fn put_reader(
        &self,
        storage_path: &str,
        reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
        size: i64,
    ) -> Result<String> {
        if size < 0 {
            return Err(storage_driver_error(
                StorageErrorKind::Precondition,
                "Google Drive upload size must be non-negative",
            ));
        }
        let access_token = self.access_token().await?;
        let existing_file_id = self
            .find_managed_file_id(storage_path, &access_token)
            .await?;
        let session_uri = self
            .start_resumable_upload(
                storage_path,
                &access_token,
                existing_file_id.as_deref(),
                size,
            )
            .await?;
        self.upload_reader_to_session(&session_uri, reader, size)
            .await?;
        Ok(storage_path.to_string())
    }

    async fn put_file(&self, storage_path: &str, local_path: &str) -> Result<String> {
        let file = tokio::fs::File::open(local_path).await.map_aster_err_ctx(
            "Google Drive open upload file",
            AsterError::storage_driver_error,
        )?;
        let metadata = file.metadata().await.map_aster_err_ctx(
            "Google Drive inspect upload file",
            AsterError::storage_driver_error,
        )?;
        let size = numbers::u64_to_i64(metadata.len(), "Google Drive upload file size")?;
        self.put_reader(storage_path, Box::new(file), size).await
    }
}

pub fn google_drive_scopes(options: &StoragePolicyOptions) -> String {
    let drive_scope = if options.google_drive_use_app_data_folder.unwrap_or(false) {
        REQUIRED_DRIVE_APPDATA_SCOPE
    } else {
        REQUIRED_DRIVE_SCOPE
    };
    [
        drive_scope,
        REQUIRED_USERINFO_EMAIL_SCOPE,
        REQUIRED_USERINFO_PROFILE_SCOPE,
    ]
    .join(" ")
}

pub fn google_drive_parent_id(options: &StoragePolicyOptions) -> String {
    if options.google_drive_use_app_data_folder.unwrap_or(false) {
        return "appDataFolder".to_string();
    }
    normalize_optional(&options.google_drive_root_folder_id)
        .or_else(|| normalize_optional(&options.google_drive_shared_drive_id))
        .unwrap_or_else(|| "root".to_string())
}

fn google_drive_refresh_token(policy_id: i64, options: &StoragePolicyOptions) -> Result<String> {
    let encrypted = options
        .google_drive_refresh_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            google_drive_storage_error(
                StorageErrorKind::Auth,
                ApiSubcode::GoogleDriveConnectionExpired,
                "Google Drive storage policy is not authorized",
            )
        })?;
    decrypt_refresh_token(policy_id, encrypted)
}

pub fn encrypt_refresh_token(policy_id: i64, refresh_token: &str) -> Result<String> {
    let config = crate::config::get_config();
    let aad = google_drive_token_aad(policy_id);
    encrypt_token(&config.auth.mfa_secret_key, aad.as_bytes(), refresh_token)
}

pub fn decrypt_refresh_token(policy_id: i64, ciphertext: &str) -> Result<String> {
    let config = crate::config::get_config();
    let aad = google_drive_token_aad(policy_id);
    decrypt_token(&config.auth.mfa_secret_key, aad.as_bytes(), ciphertext)
}

fn google_drive_token_aad(policy_id: i64) -> String {
    format!("google_drive_storage_policy:{policy_id}:refresh_token")
}

fn cipher(master_key: &str) -> Result<Aes256Gcm> {
    let hk = Hkdf::<Sha256>::new(None, master_key.as_bytes());
    let mut key = [0_u8; 32];
    hk.expand(GOOGLE_DRIVE_TOKEN_INFO, &mut key)
        .map_aster_err_ctx(
            "failed to derive Google Drive token encryption key",
            AsterError::config_error,
        )?;
    Aes256Gcm::new_from_slice(&key).map_aster_err_ctx(
        "invalid Google Drive token encryption key",
        AsterError::config_error,
    )
}

fn encrypt_token(master_key: &str, aad: &[u8], plaintext: &str) -> Result<String> {
    let cipher = cipher(master_key)?;
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ciphertext = cipher
        .encrypt(
            &nonce,
            aes_gcm::aead::Payload {
                msg: plaintext.as_bytes(),
                aad,
            },
        )
        .map_aster_err_ctx(
            "failed to encrypt Google Drive token",
            AsterError::internal_error,
        )?;
    Ok(format!(
        "{}:{}:{}",
        TOKEN_CIPHERTEXT_VERSION,
        URL_SAFE_NO_PAD.encode(nonce),
        URL_SAFE_NO_PAD.encode(ciphertext)
    ))
}

fn decrypt_token(master_key: &str, aad: &[u8], ciphertext: &str) -> Result<String> {
    let mut parts = ciphertext.split(':');
    let version = parts.next();
    let nonce = parts.next();
    let encrypted = parts.next();
    if version != Some(TOKEN_CIPHERTEXT_VERSION)
        || nonce.is_none()
        || encrypted.is_none()
        || parts.next().is_some()
    {
        return Err(AsterError::database_operation(
            "invalid Google Drive token ciphertext format",
        ));
    }

    let nonce = URL_SAFE_NO_PAD
        .decode(nonce.expect("checked nonce"))
        .map_aster_err_ctx(
            "invalid Google Drive token nonce",
            AsterError::database_operation,
        )?;
    let nonce: [u8; 12] = nonce
        .try_into()
        .map_err(|_| AsterError::database_operation("invalid Google Drive token nonce length"))?;
    let encrypted = URL_SAFE_NO_PAD
        .decode(encrypted.expect("checked ciphertext"))
        .map_aster_err_ctx(
            "invalid Google Drive token ciphertext",
            AsterError::database_operation,
        )?;
    let nonce = Nonce::from_slice(&nonce);
    let plaintext = cipher(master_key)?
        .decrypt(
            nonce,
            aes_gcm::aead::Payload {
                msg: encrypted.as_slice(),
                aad,
            },
        )
        .map_aster_err_ctx(
            "failed to decrypt Google Drive token",
            AsterError::database_operation,
        )?;
    String::from_utf8(plaintext).map_aster_err_ctx(
        "Google Drive token plaintext is not UTF-8",
        AsterError::database_operation,
    )
}

fn google_drive_object_name(path: &str) -> String {
    path.rsplit('/')
        .find(|part| !part.trim().is_empty())
        .unwrap_or("asterdrive-blob")
        .chars()
        .take(255)
        .collect()
}

fn normalize_optional(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn escape_drive_query_value(value: &str) -> String {
    value.replace('\\', "\\\\").replace('\'', "\\'")
}

fn google_drive_managed_file_query(parent_id: &str, path: &str, policy_id: i64) -> String {
    format!(
        "trashed = false and '{}' in parents and appProperties has {{ key='{}' and value='true' }} and appProperties has {{ key='{}' and value='{}' }} and appProperties has {{ key='{}' and value='{}' }}",
        escape_drive_query_value(parent_id),
        APP_PROPERTY_MANAGED,
        APP_PROPERTY_STORAGE_PATH,
        escape_drive_query_value(path),
        APP_PROPERTY_POLICY_ID,
        policy_id,
    )
}

fn google_drive_upload_content_range(size: u64) -> Option<String> {
    if size == 0 {
        None
    } else {
        Some(format!("bytes 0-{}/{}", size - 1, size))
    }
}

fn google_drive_storage_error(
    kind: StorageErrorKind,
    subcode: ApiSubcode,
    message: impl Into<String>,
) -> AsterError {
    storage_driver_error_with_subcode(kind, subcode, message.into())
}

async fn map_google_drive_error(response: reqwest::Response, context: &str) -> AsterError {
    let status = response.status();
    let body = response.json::<GoogleDriveErrorResponse>().await.ok();
    let provider_error = body.as_ref();
    let details = body
        .as_ref()
        .and_then(google_drive_error_message)
        .unwrap_or_else(|| status.to_string());
    let (kind, subcode) = if google_drive_error_is_auth_failure(status, context, provider_error) {
        (
            StorageErrorKind::Auth,
            ApiSubcode::GoogleDriveConnectionExpired,
        )
    } else if status == reqwest::StatusCode::NOT_FOUND {
        (
            StorageErrorKind::NotFound,
            ApiSubcode::GoogleDriveRemoteNotFound,
        )
    } else if status == reqwest::StatusCode::TOO_MANY_REQUESTS
        || google_drive_error_is_rate_limited(provider_error)
    {
        (
            StorageErrorKind::RateLimited,
            ApiSubcode::GoogleDriveRateLimited,
        )
    } else if status == reqwest::StatusCode::FORBIDDEN {
        (
            StorageErrorKind::Permission,
            ApiSubcode::GoogleDrivePermissionDenied,
        )
    } else if status.is_server_error() {
        (
            StorageErrorKind::Transient,
            ApiSubcode::GoogleDriveTransient,
        )
    } else {
        (StorageErrorKind::Unknown, ApiSubcode::GoogleDriveTransient)
    };
    google_drive_storage_error(kind, subcode, format!("{context} failed: {details}"))
}

fn google_drive_error_is_auth_failure(
    status: reqwest::StatusCode,
    context: &str,
    error: Option<&GoogleDriveErrorResponse>,
) -> bool {
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return true;
    }
    if status != reqwest::StatusCode::BAD_REQUEST || !context.contains("token") {
        return false;
    }
    google_drive_error_reasons(error).any(|reason| {
        matches!(
            reason,
            "invalid_grant" | "invalid_client" | "invalid_request" | "unauthorized_client"
        )
    })
}

fn google_drive_error_is_rate_limited(error: Option<&GoogleDriveErrorResponse>) -> bool {
    google_drive_error_reasons(error).any(|reason| {
        let lower = reason.to_ascii_lowercase();
        lower.contains("ratelimit")
            || lower.contains("rate_limit")
            || lower.contains("quota")
            || lower.contains("limitexceeded")
    })
}

fn google_drive_error_reasons(
    error: Option<&GoogleDriveErrorResponse>,
) -> impl Iterator<Item = &str> {
    error
        .and_then(|error| error.error.as_ref())
        .into_iter()
        .flat_map(|error| {
            error
                .as_str()
                .into_iter()
                .chain(error.get("reason").and_then(Value::as_str))
                .chain(
                    error
                        .get("errors")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(|item| item.get("reason").and_then(Value::as_str)),
                )
        })
}

fn google_drive_error_message(error: &GoogleDriveErrorResponse) -> Option<String> {
    if let Some(description) = error.error_description.as_deref() {
        return Some(sanitize_error_fragment(description));
    }
    let error = error.error.as_ref()?;
    error
        .get("message")
        .and_then(Value::as_str)
        .or_else(|| error.as_str())
        .map(sanitize_error_fragment)
}

fn sanitize_error_fragment(value: &str) -> String {
    value
        .chars()
        .filter(|ch| !ch.is_control())
        .take(256)
        .collect::<String>()
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use futures::TryStreamExt;
    use reqwest::StatusCode;
    use serde_json::json;

    use super::{
        APP_PROPERTY_POLICY_ID, GoogleDriveErrorResponse, SizedReaderStream,
        google_drive_error_is_auth_failure, google_drive_error_is_rate_limited,
        google_drive_managed_file_query, google_drive_upload_content_range,
    };

    fn provider_error(value: serde_json::Value) -> GoogleDriveErrorResponse {
        serde_json::from_value(value).expect("provider error should deserialize")
    }

    #[test]
    fn rate_limit_reason_is_not_treated_as_permission_denied() {
        let error = provider_error(json!({
            "error": {
                "errors": [
                    {
                        "domain": "usageLimits",
                        "reason": "userRateLimitExceeded",
                        "message": "User rate limit exceeded"
                    }
                ],
                "code": 403,
                "message": "User rate limit exceeded"
            }
        }));

        assert!(google_drive_error_is_rate_limited(Some(&error)));
    }

    #[test]
    fn oauth_invalid_grant_is_treated_as_auth_failure() {
        let error = provider_error(json!({
            "error": "invalid_grant",
            "error_description": "Token has been expired or revoked."
        }));

        assert!(google_drive_error_is_auth_failure(
            StatusCode::BAD_REQUEST,
            "Google Drive token refresh",
            Some(&error)
        ));
    }

    #[test]
    fn zero_byte_upload_omits_content_range_header() {
        assert_eq!(google_drive_upload_content_range(0), None);
        assert_eq!(
            google_drive_upload_content_range(2).as_deref(),
            Some("bytes 0-1/2")
        );
    }

    #[test]
    fn managed_file_query_filters_policy_id() {
        let query = google_drive_managed_file_query("root", "path/it's.bin", 42);

        assert!(query.contains(APP_PROPERTY_POLICY_ID));
        assert!(query.contains("value='42'"));
        assert!(query.contains("path/it\\'s.bin"));
    }

    #[tokio::test]
    async fn sized_reader_stream_rejects_short_reader() {
        let stream = SizedReaderStream::new(std::io::Cursor::new(vec![1, 2]), 3);
        let error = stream
            .try_collect::<Vec<_>>()
            .await
            .expect_err("short reader should fail");

        assert_eq!(error.kind(), std::io::ErrorKind::UnexpectedEof);
    }

    #[tokio::test]
    async fn sized_reader_stream_rejects_long_reader() {
        let stream = SizedReaderStream::new(std::io::Cursor::new(vec![1, 2, 3]), 2);
        let error = stream
            .try_collect::<Vec<_>>()
            .await
            .expect_err("long reader should fail");

        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    }

    #[tokio::test]
    async fn sized_reader_stream_accepts_empty_reader() {
        let stream = SizedReaderStream::new(std::io::Cursor::new(Vec::<u8>::new()), 0);
        let chunks = stream
            .try_collect::<Vec<_>>()
            .await
            .expect("empty reader should match zero declared size");

        assert!(chunks.is_empty());
    }
}
