//! Native Qiniu Kodo object storage driver.
//!
//! The driver keeps Qiniu request signing and wire formats private to this
//! module. Callers only observe AsterDrive storage traits and structured errors.

use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use bytes::Bytes;
use futures::StreamExt;
use hmac::{Hmac, Mac, digest::KeyInit};
use reqwest::{Client, Method, Url};
use serde::{Deserialize, Serialize};
use sha1::Sha1;
use std::{collections::BTreeMap, time::Duration};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio_util::io::StreamReader;

use aster_drive_storage::traits::driver::{
    BlobMetadata, PresignedDownloadOptions, PresignedFormUploadRequest, PresignedUploadRequest,
    StorageDriver,
};
use aster_drive_storage::traits::extensions::{
    ListStorageDriver, PresignedStorageDriver, StorageDriverExtensions, StreamUploadDriver,
};
use aster_drive_storage::traits::multipart::{MultipartStorageDriver, UploadedMultipartPart};
use aster_drive_storage::{MapStorageErr, Result, StorageErrorKind, storage_driver_error};

type HmacSha1 = Hmac<Sha1>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QiniuRegionEndpoints {
    pub upload: String,
    pub manage: String,
    pub list: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QiniuDriverConfig {
    pub bucket: String,
    pub region: String,
    pub download_domain: String,
    pub object_prefix: String,
    pub endpoints: QiniuRegionEndpoints,
    pub connect_timeout: Duration,
    pub read_timeout: Duration,
    pub operation_timeout: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QiniuStaticCredentials {
    pub access_key: String,
    pub secret_key: String,
}

pub struct QiniuDriver {
    config: QiniuDriverConfig,
    credentials: QiniuStaticCredentials,
    client: Client,
}

#[derive(Debug, Deserialize)]
struct StatResponse {
    #[serde(default)]
    fsize: u64,
    #[serde(default)]
    mime_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ListResponse {
    #[serde(default)]
    items: Vec<ListItem>,
    #[serde(default)]
    marker: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ListItem {
    key: String,
}

#[derive(Debug, Deserialize)]
struct MultipartInitResponse {
    #[serde(rename = "uploadId")]
    upload_id: String,
}

#[derive(Debug, Deserialize)]
struct MultipartPartItem {
    #[serde(rename = "partNumber")]
    part_number: i32,
    #[serde(default)]
    size: i64,
}

#[derive(Debug, Deserialize)]
struct MultipartListResponse {
    #[serde(default)]
    items: Vec<MultipartPartItem>,
}

impl QiniuDriver {
    pub fn new(config: QiniuDriverConfig, credentials: QiniuStaticCredentials) -> Result<Self> {
        Self::validate_config(&config, &credentials)?;
        let client = Client::builder()
            .connect_timeout(config.connect_timeout)
            .read_timeout(config.read_timeout)
            .timeout(config.operation_timeout)
            .build()
            .map_storage_err_ctx(StorageErrorKind::Misconfigured, "build Qiniu HTTP client")?;
        Ok(Self {
            config,
            credentials,
            client,
        })
    }

    pub fn validate_config(
        config: &QiniuDriverConfig,
        credentials: &QiniuStaticCredentials,
    ) -> Result<()> {
        if config.bucket.trim().is_empty() || config.bucket.contains('/') {
            return Err(storage_driver_error(
                StorageErrorKind::Misconfigured,
                "Qiniu bucket must be a non-empty name without '/'",
            ));
        }
        if config.region.trim().is_empty() {
            return Err(storage_driver_error(
                StorageErrorKind::Misconfigured,
                "Qiniu region cannot be empty",
            ));
        }
        Url::parse(&config.download_domain).map_err(|error| {
            storage_driver_error(
                StorageErrorKind::Misconfigured,
                format!("invalid Qiniu download domain: {error}"),
            )
        })?;
        if credentials.access_key.trim().is_empty() || credentials.secret_key.trim().is_empty() {
            return Err(storage_driver_error(
                StorageErrorKind::Auth,
                "Qiniu AccessKey and SecretKey are required",
            ));
        }
        Ok(())
    }

    fn key(&self, path: &str) -> String {
        let path = path.trim_start_matches('/');
        if self.config.object_prefix.is_empty() {
            path.to_string()
        } else if path.is_empty() {
            self.config.object_prefix.clone()
        } else {
            format!(
                "{}/{}",
                self.config.object_prefix.trim_end_matches('/'),
                path
            )
        }
    }

    fn public_key(&self, key: &str) -> String {
        let mut url = self
            .config
            .download_domain
            .trim_end_matches('/')
            .to_string();
        url.push('/');
        url.push_str(
            &percent_encoding::percent_encode(key.as_bytes(), percent_encoding::NON_ALPHANUMERIC)
                .to_string(),
        );
        url
    }

    fn entry(&self, key: &str) -> String {
        URL_SAFE_NO_PAD.encode(format!("{}:{}", self.config.bucket, key))
    }

    fn upload_token(&self, key: &str, expires: Duration) -> Result<String> {
        #[derive(Serialize)]
        struct Policy<'a> {
            scope: String,
            deadline: u64,
            #[serde(rename = "insertOnly")]
            insert_only: u8,
            #[serde(skip_serializing_if = "Option::is_none")]
            mime_limit: Option<&'a str>,
        }
        let deadline = std::time::SystemTime::now()
            .checked_add(expires)
            .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|value| value.as_secs())
            .ok_or_else(|| {
                storage_driver_error(
                    StorageErrorKind::Misconfigured,
                    "invalid Qiniu token expiry",
                )
            })?;
        let policy = serde_json::to_vec(&Policy {
            scope: format!("{}:{}", self.config.bucket, key),
            deadline,
            insert_only: 0,
            mime_limit: None,
        })
        .map_storage_err_ctx(
            StorageErrorKind::Misconfigured,
            "serialize Qiniu upload policy",
        )?;
        let encoded = URL_SAFE_NO_PAD.encode(policy);
        Ok(format!(
            "{}:{}:{}",
            self.credentials.access_key,
            sign(&self.credentials.secret_key, encoded.as_bytes()),
            encoded
        ))
    }

    fn authorization(&self, method: &Method, url: &Url, body: &[u8]) -> String {
        let mut data = format!("{} {}", method.as_str(), url.path());
        if let Some(query) = url.query() {
            data.push('?');
            data.push_str(query);
        }
        data.push('\n');
        data.push_str(&String::from_utf8_lossy(body));
        format!(
            "QBox {}:{}",
            self.credentials.access_key,
            sign(&self.credentials.secret_key, data.as_bytes())
        )
    }

    async fn request_json<T: for<'de> Deserialize<'de>>(
        &self,
        method: Method,
        url: Url,
        body: Option<Bytes>,
    ) -> Result<T> {
        let body_bytes = body.unwrap_or_default();
        let mut request = self.client.request(method.clone(), url.clone()).header(
            "Authorization",
            self.authorization(&method, &url, &body_bytes),
        );
        if !body_bytes.is_empty() {
            request = request
                .header("Content-Type", "application/json")
                .body(body_bytes.clone());
        }
        let response = request
            .send()
            .await
            .map_err(|error| map_reqwest_error("Qiniu request", error))?;
        let status = response.status();
        let bytes = response
            .bytes()
            .await
            .map_err(|error| map_reqwest_error("read Qiniu response", error))?;
        if !status.is_success() {
            return Err(provider_error(
                "Qiniu request failed",
                status.as_u16(),
                &bytes,
            ));
        }
        serde_json::from_slice(&bytes)
            .map_storage_err_ctx(StorageErrorKind::Unknown, "decode Qiniu response")
    }

    async fn send_bytes(&self, method: Method, url: Url, body: Bytes) -> Result<reqwest::Response> {
        let response = self
            .client
            .request(method.clone(), url.clone())
            .header("Authorization", self.authorization(&method, &url, &body))
            .header("Content-Type", "application/octet-stream")
            .body(body)
            .send()
            .await
            .map_err(|error| map_reqwest_error("Qiniu upload request", error))?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.bytes().await.unwrap_or_default();
            return Err(provider_error(
                "Qiniu upload request failed",
                status.as_u16(),
                &body,
            ));
        }
        Ok(response)
    }

    fn manage_url(&self, path: &str) -> Result<Url> {
        Url::parse(&format!(
            "{}/{}",
            self.config.endpoints.manage.trim_end_matches('/'),
            path
        ))
        .map_storage_err_ctx(
            StorageErrorKind::Misconfigured,
            "build Qiniu management URL",
        )
    }

    fn list_url(&self, query: &str) -> Result<Url> {
        Url::parse(&format!(
            "{}/list?{}",
            self.config.endpoints.list.trim_end_matches('/'),
            query
        ))
        .map_storage_err_ctx(StorageErrorKind::Misconfigured, "build Qiniu list URL")
    }

    fn upload_url(&self, path: &str) -> Result<Url> {
        Url::parse(&format!(
            "{}{}",
            self.config.endpoints.upload.trim_end_matches('/'),
            path
        ))
        .map_storage_err_ctx(StorageErrorKind::Misconfigured, "build Qiniu upload URL")
    }

    async fn stat_key(&self, key: &str) -> Result<BlobMetadata> {
        let response: StatResponse = self
            .request_json(
                Method::GET,
                self.manage_url(&format!("stat/{}", self.entry(key)))?,
                None,
            )
            .await?;
        Ok(BlobMetadata {
            size: response.fsize,
            content_type: response.mime_type,
        })
    }

    async fn upload_form(&self, key: &str, data: Bytes) -> Result<()> {
        let token = self.upload_token(key, Duration::from_secs(3600))?;
        let form = reqwest::multipart::Form::new()
            .text("token", token)
            .text("key", key.to_string())
            .part(
                "file",
                reqwest::multipart::Part::bytes(data.to_vec()).file_name("upload"),
            );
        let response = self
            .client
            .post(self.config.endpoints.upload.clone())
            .multipart(form)
            .send()
            .await
            .map_err(|error| map_reqwest_error("Qiniu form upload", error))?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.bytes().await.unwrap_or_default();
            return Err(provider_error(
                "Qiniu form upload failed",
                status.as_u16(),
                &body,
            ));
        }
        Ok(())
    }

    fn private_download_url(&self, key: &str, expires: Duration) -> Result<String> {
        let mut url = Url::parse(&self.public_key(key))
            .map_storage_err_ctx(StorageErrorKind::Misconfigured, "build Qiniu download URL")?;
        let deadline = std::time::SystemTime::now()
            .checked_add(expires)
            .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|value| value.as_secs())
            .ok_or_else(|| {
                storage_driver_error(
                    StorageErrorKind::Misconfigured,
                    "invalid Qiniu download expiry",
                )
            })?;
        url.query_pairs_mut()
            .append_pair("e", &deadline.to_string());
        let signing = format!("{}\n", url.as_str());
        url.query_pairs_mut().append_pair(
            "token",
            &format!(
                "{}:{}",
                self.credentials.access_key,
                sign(&self.credentials.secret_key, signing.as_bytes())
            ),
        );
        Ok(url.to_string())
    }
}

#[async_trait]
impl StorageDriver for QiniuDriver {
    async fn put(&self, path: &str, data: &[u8]) -> Result<String> {
        let key = self.key(path);
        self.upload_form(&key, Bytes::copy_from_slice(data)).await?;
        Ok(path.to_string())
    }

    async fn get(&self, path: &str) -> Result<Vec<u8>> {
        let url = self.private_download_url(&self.key(path), Duration::from_secs(300))?;
        self.client
            .get(url)
            .send()
            .await
            .map_err(|error| map_reqwest_error("Qiniu get", error))?
            .error_for_status()
            .map_err(|error| map_reqwest_error("Qiniu get status", error))?
            .bytes()
            .await
            .map(|bytes| bytes.to_vec())
            .map_err(|error| map_reqwest_error("Qiniu read object", error))
    }

    async fn get_stream(&self, path: &str) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
        let url = self.private_download_url(&self.key(path), Duration::from_secs(300))?;
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|error| map_reqwest_error("Qiniu get stream", error))?
            .error_for_status()
            .map_err(|error| map_reqwest_error("Qiniu get stream status", error))?;
        let stream = response
            .bytes_stream()
            .map(|item| item.map_err(|error| std::io::Error::other(error.to_string())));
        Ok(Box::new(StreamReader::new(stream)))
    }

    async fn get_range(
        &self,
        path: &str,
        offset: u64,
        length: Option<u64>,
    ) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
        let url = self.private_download_url(&self.key(path), Duration::from_secs(300))?;
        let range = match length {
            Some(0) => return Ok(Box::new(tokio::io::empty())),
            Some(length) => format!("bytes={offset}-{}", offset + length - 1),
            None => format!("bytes={offset}-"),
        };
        let response = self
            .client
            .get(url)
            .header("Range", range)
            .send()
            .await
            .map_err(|error| map_reqwest_error("Qiniu get range", error))?
            .error_for_status()
            .map_err(|error| map_reqwest_error("Qiniu get range status", error))?;
        let stream = response
            .bytes_stream()
            .map(|item| item.map_err(|error| std::io::Error::other(error.to_string())));
        Ok(Box::new(StreamReader::new(stream)))
    }

    fn supports_efficient_range(&self) -> bool {
        true
    }

    async fn delete(&self, path: &str) -> Result<()> {
        let _: serde_json::Value = self
            .request_json(
                Method::POST,
                self.manage_url(&format!("delete/{}", self.entry(&self.key(path))))?,
                None,
            )
            .await?;
        Ok(())
    }

    async fn exists(&self, path: &str) -> Result<bool> {
        match self.stat_key(&self.key(path)).await {
            Ok(_) => Ok(true),
            Err(error) if error.kind() == StorageErrorKind::NotFound => Ok(false),
            Err(error) => Err(error),
        }
    }

    async fn metadata(&self, path: &str) -> Result<BlobMetadata> {
        self.stat_key(&self.key(path)).await
    }

    async fn copy_object(&self, src_path: &str, dest_path: &str) -> Result<String> {
        let source = self.entry(&self.key(src_path));
        let destination = self.entry(&self.key(dest_path));
        let _: serde_json::Value = self
            .request_json(
                Method::POST,
                self.manage_url(&format!("copy/{source}/{destination}"))?,
                None,
            )
            .await?;
        Ok(dest_path.to_string())
    }

    fn extensions(&self) -> StorageDriverExtensions<'_> {
        StorageDriverExtensions {
            presigned: Some(self),
            list: Some(self),
            stream_upload: Some(self),
            multipart: Some(self),
            ..Default::default()
        }
    }
}

#[async_trait]
impl PresignedStorageDriver for QiniuDriver {
    async fn presigned_url(
        &self,
        path: &str,
        expires: Duration,
        _options: PresignedDownloadOptions,
    ) -> Result<Option<String>> {
        Ok(Some(self.private_download_url(&self.key(path), expires)?))
    }

    async fn presigned_put_request(
        &self,
        _path: &str,
        _expires: Duration,
    ) -> Result<Option<PresignedUploadRequest>> {
        Ok(None)
    }

    async fn presigned_form_upload_request(
        &self,
        path: &str,
        expires: Duration,
    ) -> Result<Option<PresignedFormUploadRequest>> {
        let key = self.key(path);
        let mut fields = BTreeMap::new();
        fields.insert("token".to_string(), self.upload_token(&key, expires)?);
        fields.insert("key".to_string(), key);
        Ok(Some(PresignedFormUploadRequest {
            url: self.config.endpoints.upload.clone(),
            fields,
        }))
    }

    fn presigned_single_put_requires_etag(&self) -> bool {
        false
    }
}

#[async_trait]
impl ListStorageDriver for QiniuDriver {
    async fn list_paths(&self, prefix: Option<&str>) -> Result<Vec<String>> {
        let mut marker: Option<String> = None;
        let provider_prefix = self.key(prefix.unwrap_or(""));
        let mut paths = Vec::new();
        loop {
            let mut query = format!(
                "bucket={}&limit=1000",
                percent_encoding::percent_encode(
                    self.config.bucket.as_bytes(),
                    percent_encoding::NON_ALPHANUMERIC,
                )
            );
            if !provider_prefix.is_empty() {
                query.push_str("&prefix=");
                query.push_str(
                    &percent_encoding::percent_encode(
                        provider_prefix.as_bytes(),
                        percent_encoding::NON_ALPHANUMERIC,
                    )
                    .to_string(),
                );
            }
            if let Some(value) = marker.as_deref() {
                query.push_str("&marker=");
                query.push_str(
                    &percent_encoding::percent_encode(
                        value.as_bytes(),
                        percent_encoding::NON_ALPHANUMERIC,
                    )
                    .to_string(),
                );
            }
            let response: ListResponse = self
                .request_json(Method::GET, self.list_url(&query)?, None)
                .await?;
            for item in response.items {
                paths.push(
                    item.key
                        .strip_prefix(&self.config.object_prefix)
                        .unwrap_or(&item.key)
                        .trim_start_matches('/')
                        .to_string(),
                );
            }
            marker = response.marker;
            if marker.is_none() {
                break;
            }
        }
        Ok(paths)
    }
}

#[async_trait]
impl StreamUploadDriver for QiniuDriver {
    async fn put_reader(
        &self,
        storage_path: &str,
        mut reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
        size: i64,
    ) -> Result<String> {
        if size < 0 {
            return Err(storage_driver_error(
                StorageErrorKind::Misconfigured,
                "Qiniu put_reader size cannot be negative",
            ));
        }
        let expected = usize::try_from(size)
            .map_storage_err_ctx(StorageErrorKind::Misconfigured, "Qiniu put_reader size")?;
        let mut data = Vec::with_capacity(expected);
        reader
            .read_to_end(&mut data)
            .await
            .map_storage_err_ctx(StorageErrorKind::Transient, "read Qiniu upload stream")?;
        if data.len() != expected {
            return Err(storage_driver_error(
                StorageErrorKind::Transient,
                "Qiniu upload stream size mismatch",
            ));
        }
        self.put(storage_path, &data).await
    }

    async fn put_file(&self, storage_path: &str, local_path: &str) -> Result<String> {
        let file = tokio::fs::File::open(local_path)
            .await
            .map_storage_err_ctx(StorageErrorKind::Transient, "open Qiniu upload file")?;
        let size = file
            .metadata()
            .await
            .map_storage_err_ctx(StorageErrorKind::Transient, "stat Qiniu upload file")?
            .len();
        self.put_reader(
            storage_path,
            Box::new(file),
            i64::try_from(size)
                .map_storage_err_ctx(StorageErrorKind::Misconfigured, "Qiniu upload file size")?,
        )
        .await
    }
}

#[async_trait]
impl MultipartStorageDriver for QiniuDriver {
    async fn create_multipart_upload(&self, path: &str) -> Result<String> {
        let key = self.key(path);
        let response: MultipartInitResponse = self
            .request_json(
                Method::POST,
                self.upload_url(&format!(
                    "/buckets/{}/objects/{}/uploads",
                    self.config.bucket,
                    URL_SAFE_NO_PAD.encode(key.as_bytes())
                ))?,
                None,
            )
            .await?;
        Ok(response.upload_id)
    }

    async fn presigned_upload_part_request(
        &self,
        path: &str,
        upload_id: &str,
        part_number: i32,
        _expires: Duration,
    ) -> Result<PresignedUploadRequest> {
        let key = self.key(path);
        let url = self.upload_url(&format!(
            "/buckets/{}/objects/{}/uploads/{}/{}",
            self.config.bucket,
            URL_SAFE_NO_PAD.encode(key.as_bytes()),
            upload_id,
            part_number
        ))?;
        Ok(PresignedUploadRequest::from_header_pairs(
            url.to_string(),
            [("authorization", self.authorization(&Method::PUT, &url, &[]))],
        ))
    }

    async fn complete_multipart_upload(
        &self,
        path: &str,
        upload_id: &str,
        mut parts: Vec<(i32, String)>,
    ) -> Result<()> {
        parts.sort_by_key(|(number, _)| *number);
        #[derive(Serialize)]
        struct Part<'a> {
            #[serde(rename = "partNumber")]
            part_number: i32,
            etag: &'a str,
        }
        let body = serde_json::to_vec(
            &parts
                .iter()
                .map(|(part_number, etag)| Part {
                    part_number: *part_number,
                    etag,
                })
                .collect::<Vec<_>>(),
        )
        .map_storage_err_ctx(
            StorageErrorKind::Misconfigured,
            "serialize Qiniu multipart completion",
        )?;
        let key = self.key(path);
        let _: serde_json::Value = self
            .request_json(
                Method::POST,
                self.upload_url(&format!(
                    "/buckets/{}/objects/{}/uploads/{}",
                    self.config.bucket,
                    URL_SAFE_NO_PAD.encode(key.as_bytes()),
                    upload_id
                ))?,
                Some(Bytes::from(body)),
            )
            .await?;
        Ok(())
    }

    async fn upload_multipart_part_bytes(
        &self,
        path: &str,
        upload_id: &str,
        part_number: i32,
        data: Bytes,
    ) -> Result<String> {
        if !(1..=10_000).contains(&part_number) {
            return Err(storage_driver_error(
                StorageErrorKind::Misconfigured,
                "Qiniu multipart part number must be between 1 and 10000",
            ));
        }
        let key = self.key(path);
        let url = self.upload_url(&format!(
            "/buckets/{}/objects/{}/uploads/{}/{}",
            self.config.bucket,
            URL_SAFE_NO_PAD.encode(key.as_bytes()),
            upload_id,
            part_number
        ))?;
        let response = self.send_bytes(Method::PUT, url, data).await?;
        if let Some(etag) = response
            .headers()
            .get("etag")
            .and_then(|value| value.to_str().ok())
        {
            return Ok(etag.to_string());
        }
        let body = response.bytes().await.unwrap_or_default();
        serde_json::from_slice::<serde_json::Value>(&body)
            .ok()
            .and_then(|value| {
                value
                    .get("etag")
                    .and_then(|value| value.as_str())
                    .map(str::to_string)
            })
            .ok_or_else(|| {
                storage_driver_error(
                    StorageErrorKind::Unknown,
                    "Qiniu multipart part response missing ETag",
                )
            })
    }

    async fn upload_multipart_part(
        &self,
        path: &str,
        upload_id: &str,
        part_number: i32,
        data: &[u8],
    ) -> Result<String> {
        self.upload_multipart_part_bytes(path, upload_id, part_number, Bytes::copy_from_slice(data))
            .await
    }

    async fn abort_multipart_upload(&self, path: &str, upload_id: &str) -> Result<()> {
        let key = self.key(path);
        let _: serde_json::Value = self
            .request_json(
                Method::DELETE,
                self.upload_url(&format!(
                    "/buckets/{}/objects/{}/uploads/{}",
                    self.config.bucket,
                    URL_SAFE_NO_PAD.encode(key.as_bytes()),
                    upload_id
                ))?,
                None,
            )
            .await?;
        Ok(())
    }

    async fn list_uploaded_part_details(
        &self,
        path: &str,
        upload_id: &str,
    ) -> Result<Vec<UploadedMultipartPart>> {
        let key = self.key(path);
        let response: MultipartListResponse = self
            .request_json(
                Method::GET,
                self.upload_url(&format!(
                    "/buckets/{}/objects/{}/uploads/{}",
                    self.config.bucket,
                    URL_SAFE_NO_PAD.encode(key.as_bytes()),
                    upload_id
                ))?,
                None,
            )
            .await?;
        Ok(response
            .items
            .into_iter()
            .map(|part| UploadedMultipartPart {
                part_number: part.part_number,
                size: part.size,
            })
            .collect())
    }
}

fn sign(secret: &str, data: &[u8]) -> String {
    let Ok(mut mac) = HmacSha1::new_from_slice(secret.as_bytes()) else {
        return String::new();
    };
    mac.update(data);
    URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
}

fn map_reqwest_error(context: &str, error: reqwest::Error) -> aster_drive_storage::StorageError {
    let kind = if error.is_timeout() || error.is_connect() {
        StorageErrorKind::Transient
    } else {
        StorageErrorKind::Unknown
    };
    storage_driver_error(kind, format!("{context}: network request failed"))
}

fn provider_error(context: &str, status: u16, body: &[u8]) -> aster_drive_storage::StorageError {
    let kind = match status {
        401 => StorageErrorKind::Auth,
        403 => StorageErrorKind::Permission,
        404 => StorageErrorKind::NotFound,
        409 => StorageErrorKind::Precondition,
        429 => StorageErrorKind::RateLimited,
        400..=499 => StorageErrorKind::Misconfigured,
        500..=599 => StorageErrorKind::Transient,
        _ => StorageErrorKind::Unknown,
    };
    let detail = serde_json::from_slice::<serde_json::Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("error")
                .and_then(|value| value.as_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| format!("HTTP {status}"));
    storage_driver_error(kind, format!("{context}: {detail}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upload_token_contains_scope_and_three_segments() {
        let driver = QiniuDriver {
            config: QiniuDriverConfig {
                bucket: "bucket".to_string(),
                region: "z0".to_string(),
                download_domain: "https://download.example.test".to_string(),
                object_prefix: String::new(),
                endpoints: QiniuRegionEndpoints {
                    upload: "https://up.example.test".to_string(),
                    manage: "https://rs.example.test".to_string(),
                    list: "https://rsf.example.test".to_string(),
                },
                connect_timeout: Duration::from_secs(1),
                read_timeout: Duration::from_secs(1),
                operation_timeout: Duration::from_secs(1),
            },
            credentials: QiniuStaticCredentials {
                access_key: "ak".to_string(),
                secret_key: "sk".to_string(),
            },
            client: Client::new(),
        };
        let token = driver
            .upload_token("files/object", Duration::from_secs(60))
            .unwrap();
        assert_eq!(token.split(':').count(), 3);
        assert!(token.ends_with(&URL_SAFE_NO_PAD.encode(br#"files/object"#)) == false);
    }
}
