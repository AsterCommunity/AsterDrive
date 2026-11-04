use super::{
    MultipartSession, QiniuDriver, auth,
    http::{self, ServiceKind},
    invalid,
};
use aster_drive_storage::traits::extensions::{ExactSizeReader, checked_upload_size};
use aster_drive_storage::{
    BrowserUploadMethod, BrowserUploadRequest, MultipartStorageCapabilities,
    MultipartStorageDriver, MultipartUploadMode, Result, StorageErrorKind, UploadedMultipartPart,
    storage_driver_error,
};
use bytes::Bytes;
use reqwest::Method;
use serde::Deserialize;
use serde_json::json;
use std::time::Duration;
use tokio::io::AsyncRead;
use tokio_util::io::ReaderStream;
use url::Url;

#[derive(Deserialize)]
struct Initiated {
    #[serde(rename = "uploadId")]
    upload_id: String,
    #[serde(rename = "expireAt")]
    expires_at: i64,
}
#[derive(Deserialize)]
struct PartResult {
    etag: String,
}
#[derive(Deserialize)]
pub(super) struct Part {
    #[serde(rename = "partNumber")]
    pub(super) number: i32,
    pub(super) size: i64,
    pub(super) etag: String,
}
#[derive(Deserialize)]
struct PartList {
    #[serde(default)]
    parts: Vec<Part>,
    #[serde(default, rename = "partNumberMarker")]
    marker: i32,
}

impl QiniuDriver {
    fn session(&self, path: &str, id: &str) -> Result<MultipartSession> {
        let session: MultipartSession = serde_json::from_str(id).map_err(|_| invalid("invalid native Kodo multipart session; legacy S3 uploads must be canceled before upgrading"))?;
        if session.key != self.key(path)?
            || session.bucket != self.config.bucket
            || session.upload_id.is_empty()
        {
            return Err(invalid(
                "Kodo upload session does not belong to this object/bucket",
            ));
        }
        if session.expires_at <= chrono::Utc::now().timestamp() {
            return Err(storage_driver_error(
                StorageErrorKind::NotFound,
                "Kodo multipart session expired",
            ));
        }
        Ok(session)
    }
    async fn multipart_url(&self, key: &str, id: Option<&str>) -> Result<Url> {
        let mut url = self.endpoint(ServiceKind::Upload).await?;
        {
            let mut segments = url
                .path_segments_mut()
                .map_err(|_| invalid("invalid Kodo upload URL"))?;
            segments.clear().extend([
                "buckets",
                self.config.bucket.as_str(),
                "objects",
                auth::encode(key).as_str(),
                "uploads",
            ]);
            if let Some(id) = id {
                segments.push(id);
            }
        }
        Ok(url)
    }
    async fn session_request(
        &self,
        method: Method,
        path: &str,
        id: &str,
    ) -> Result<(reqwest::RequestBuilder, MultipartSession)> {
        let session = self.session(path, id)?;
        let token = self.upload_token(
            &session.key,
            session
                .expires_at
                .min(chrono::Utc::now().timestamp() + 3600),
            None,
            false,
        )?;
        let url = self
            .multipart_url(&session.key, Some(&session.upload_id))
            .await?;
        Ok((
            self.client
                .request(method, url)
                .header("authorization", format!("UpToken {token}")),
            session,
        ))
    }
    pub(super) async fn uploaded_parts(&self, path: &str, id: &str) -> Result<Vec<Part>> {
        let mut marker = 0;
        let mut parts = Vec::new();
        loop {
            let (request, _) = self.session_request(Method::GET, path, id).await?;
            let response: PartList = http::decode(
                request
                    .query(&[("max-parts", 1000), ("part-number-marker", marker)])
                    .send()
                    .await
                    .map_err(http::transport_error)?,
            )
            .await?;
            for part in response.parts {
                validate_part_number(part.number)?;
                if part.number <= marker || part.size < 0 || part.etag.is_empty() {
                    return Err(invalid("invalid Kodo uploaded-part record"));
                }
                parts.push(part);
            }
            if parts.len() > 10_000 {
                return Err(invalid("Kodo returned too many parts"));
            }
            if response.marker == 0 {
                break;
            }
            if response.marker <= marker {
                return Err(invalid("Kodo part cursor did not advance"));
            }
            marker = response.marker;
            if parts.len() > 10_000 {
                return Err(invalid("Kodo returned too many parts"));
            }
        }
        parts.sort_by_key(|p| p.number);
        if parts.windows(2).any(|p| p[0].number == p[1].number) {
            return Err(invalid("Kodo returned duplicate parts"));
        }
        Ok(parts)
    }
    async fn native_part_request(
        &self,
        path: &str,
        id: &str,
        number: i32,
        expires: Duration,
    ) -> Result<BrowserUploadRequest> {
        validate_part_number(number)?;
        if expires.as_secs() == 0 || expires > Duration::from_secs(3600) {
            return Err(invalid(
                "Kodo upload token TTL must be between 1 and 3600 seconds",
            ));
        }
        let session = self.session(path, id)?;
        let mut url = self
            .multipart_url(&session.key, Some(&session.upload_id))
            .await?;
        url.path_segments_mut()
            .map_err(|_| invalid("invalid upload URL"))?
            .push(&number.to_string());
        let seconds =
            i64::try_from(expires.as_secs()).map_err(|_| invalid("Kodo token TTL overflow"))?;
        let token = self.upload_token(
            &session.key,
            session
                .expires_at
                .min(chrono::Utc::now().timestamp() + seconds),
            None,
            true,
        )?;
        Ok(BrowserUploadRequest {
            url: url.to_string(),
            method: BrowserUploadMethod::Put,
            headers: std::collections::BTreeMap::from([(
                "content-type".into(),
                "application/octet-stream".into(),
            )]),
            authorization: Some(aster_drive_storage::BrowserUploadAuthorization {
                scheme: "UpToken".into(),
                token,
            }),
            completion: Some(aster_drive_storage::BrowserUploadCompletion::ProviderReconciled),
            body: None,
            response_headers: Vec::new(),
        })
    }
    async fn upload_part_body(
        &self,
        path: &str,
        id: &str,
        number: i32,
        body: reqwest::Body,
        size: u64,
    ) -> Result<String> {
        validate_part_number(number)?;
        if size > 1024 * 1024 * 1024 {
            return Err(invalid("Kodo part exceeds 1 GiB"));
        }
        let request = self
            .native_part_request(path, id, number, Duration::from_secs(3600))
            .await?;
        let mut builder = self.client.put(&request.url).header("content-length", size);
        if let Some(auth) = request.authorization {
            builder = builder.header("authorization", format!("{} {}", auth.scheme, auth.token));
        }
        for (name, value) in request.headers {
            builder = builder.header(name, value);
        }
        let result: PartResult = http::decode(
            builder
                .body(body)
                .send()
                .await
                .map_err(http::transport_error)?,
        )
        .await?;
        if result.etag.is_empty() {
            return Err(invalid("Kodo upload part returned no ETag"));
        }
        Ok(result.etag)
    }
}
fn validate_part_number(number: i32) -> Result<()> {
    if !(1..=10_000).contains(&number) {
        return Err(invalid("Kodo part number must be between 1 and 10000"));
    }
    Ok(())
}

#[async_trait::async_trait]
impl MultipartStorageDriver for QiniuDriver {
    fn capabilities(&self) -> MultipartStorageCapabilities {
        MultipartStorageCapabilities {
            min_part_size: 1024 * 1024,
            max_part_size: Some(1024 * 1024 * 1024),
            max_parts: 10_000,
            max_object_size: None,
            upload_mode: MultipartUploadMode::NativeStreaming,
        }
    }
    async fn create_multipart_upload(&self, path: &str) -> Result<String> {
        let key = self.key(path)?;
        let token = self.upload_token(&key, chrono::Utc::now().timestamp() + 3600, None, false)?;
        let url = self.multipart_url(&key, None).await?;
        let response: Initiated = http::decode(
            self.client
                .post(url)
                .header("authorization", format!("UpToken {token}"))
                .send()
                .await
                .map_err(http::transport_error)?,
        )
        .await?;
        if response.upload_id.is_empty() || response.expires_at <= chrono::Utc::now().timestamp() {
            return Err(invalid("Kodo returned an invalid multipart session"));
        }
        serde_json::to_string(&MultipartSession {
            upload_id: response.upload_id,
            expires_at: response.expires_at,
            key,
            bucket: self.config.bucket.clone(),
        })
        .map_err(|_| invalid("cannot encode Kodo multipart session"))
    }
    async fn browser_part_request(
        &self,
        path: &str,
        id: &str,
        number: i32,
        expires: Duration,
    ) -> Result<BrowserUploadRequest> {
        self.native_part_request(path, id, number, expires).await
    }
    async fn complete_multipart_upload(
        &self,
        path: &str,
        id: &str,
        mut parts: Vec<(i32, String)>,
    ) -> Result<()> {
        if parts.is_empty() || parts.len() > 10_000 {
            return Err(invalid("invalid Kodo completion part count"));
        }
        parts.sort_by_key(|part| part.0);
        for (number, _) in &parts {
            validate_part_number(*number)?;
        }
        if parts.windows(2).any(|p| p[0].0 == p[1].0) {
            return Err(invalid("duplicate Kodo part number"));
        }
        let actual = self.uploaded_parts(path, id).await?;
        let actual = actual
            .into_iter()
            .map(|part| (part.number, part))
            .collect::<std::collections::BTreeMap<_, _>>();
        for (index, (number, etag)) in parts.iter().enumerate() {
            let part = actual
                .get(number)
                .ok_or_else(|| invalid("Kodo completion references an absent part"))?;
            if (!etag.is_empty() && part.etag != etag.trim_matches('"'))
                || part.size > 1024 * 1024 * 1024
                || (index + 1 != parts.len() && part.size < 1024 * 1024)
            {
                return Err(storage_driver_error(
                    StorageErrorKind::Precondition,
                    "Kodo completion ETag/part size does not match provider state",
                ));
            }
        }
        let (request, _) = self.session_request(Method::POST, path, id).await?;
        let receipts = actual
            .into_values()
            .map(|p| (p.number, p.etag))
            .collect::<std::collections::BTreeMap<_, _>>();
        let payload = json!({"parts": parts.into_iter().map(|(number, _)| json!({"partNumber":number,"etag":receipts.get(&number)})).collect::<Vec<_>>()});
        http::checked(
            request
                .json(&payload)
                .send()
                .await
                .map_err(http::transport_error)?,
        )
        .await?;
        Ok(())
    }
    async fn upload_multipart_part(
        &self,
        path: &str,
        id: &str,
        number: i32,
        data: &[u8],
    ) -> Result<String> {
        self.upload_multipart_part_bytes(path, id, number, Bytes::copy_from_slice(data))
            .await
    }
    async fn upload_multipart_part_bytes(
        &self,
        path: &str,
        id: &str,
        number: i32,
        data: Bytes,
    ) -> Result<String> {
        let size = u64::try_from(data.len()).map_err(|_| invalid("Kodo part size overflow"))?;
        self.upload_part_body(path, id, number, data.into(), size)
            .await
    }
    async fn upload_multipart_part_reader(
        &self,
        path: &str,
        id: &str,
        number: i32,
        reader: Box<dyn AsyncRead + Unpin + Send + Sync>,
        size: i64,
    ) -> Result<String> {
        let size = checked_upload_size(size, "Kodo multipart part size")?;
        let stream = ReaderStream::with_capacity(ExactSizeReader::new(reader, size), 64 * 1024);
        self.upload_part_body(path, id, number, reqwest::Body::wrap_stream(stream), size)
            .await
    }
    async fn abort_multipart_upload(&self, path: &str, id: &str) -> Result<()> {
        let (request, _) = match self.session_request(Method::DELETE, path, id).await {
            Ok(value) => value,
            Err(error) if error.kind() == StorageErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error),
        };
        match http::checked(request.send().await.map_err(http::transport_error)?).await {
            Ok(_) => Ok(()),
            Err(error) if error.kind() == StorageErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    }
    async fn list_uploaded_part_details(
        &self,
        path: &str,
        id: &str,
    ) -> Result<Vec<UploadedMultipartPart>> {
        Ok(self
            .uploaded_parts(path, id)
            .await?
            .into_iter()
            .map(|p| UploadedMultipartPart {
                part_number: p.number,
                size: p.size,
            })
            .collect())
    }
}
