//! 腾讯云 COS 存储驱动。
//!
//! 基础对象读写复用 S3 兼容驱动；COS/CI 文档预览使用 COS 原生 query
//! 签名，因为 CI 处理参数必须参与签名，不能追加在普通 S3 presigned URL 后面。

use std::time::Duration;

use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use hmac::{Hmac, KeyInit, Mac};
use percent_encoding::{AsciiSet, CONTROLS, percent_encode};
use sha1::{Digest, Sha1};
use url::Url;

use super::s3::S3DriverOptions;
use super::s3_compatible::{S3CompatibleDriver, S3CompatibleProvider};
use super::s3_config::normalize_s3_endpoint_and_bucket;
use crate::entities::storage_policy;
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::storage::error::{StorageErrorKind, storage_driver_error};
use crate::storage::extensions::{
    NativePreviewMode, NativePreviewOpenMode, NativePreviewRequest, NativePreviewResult,
    NativePreviewStorageDriver, NativeThumbnailRequest, NativeThumbnailStorageDriver,
};
use crate::storage::object_key;

type HmacSha1 = Hmac<Sha1>;

const COS_NATIVE_PREVIEW_PROVIDER: &str = "tencent_cos_ci";
const COS_NATIVE_PREVIEW_VERSION: &str = "cos-ci-doc-preview-html-v1";
const COS_SIGN_ALGORITHM: &str = "sha1";
const MAX_COS_PREVIEW_TTL: Duration = Duration::from_secs(60 * 60);
const MAX_COS_THUMBNAIL_TTL: Duration = Duration::from_secs(5 * 60);
const COS_PATH_ENCODE_SET: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'%')
    .add(b'<')
    .add(b'>')
    .add(b'?')
    .add(b'`')
    .add(b'{')
    .add(b'}');
const COS_QUERY_ENCODE_SET: &AsciiSet = &COS_PATH_ENCODE_SET
    .add(b'!')
    .add(b'$')
    .add(b'&')
    .add(b'\'')
    .add(b'(')
    .add(b')')
    .add(b'*')
    .add(b'+')
    .add(b',')
    .add(b'/')
    .add(b':')
    .add(b';')
    .add(b'=')
    .add(b'@')
    .add(b'[')
    .add(b'\\')
    .add(b']')
    .add(b'^')
    .add(b'|');

pub struct TencentCosDriver {
    storage: S3CompatibleDriver,
    endpoint: String,
    bucket: String,
    access_key: String,
    secret_key: String,
    base_path: String,
}

impl TencentCosDriver {
    pub fn validate_policy(policy: &storage_policy::Model) -> Result<()> {
        S3CompatibleDriver::validate_policy(policy)?;
        let normalized = normalize_s3_endpoint_and_bucket(&policy.endpoint, &policy.bucket)
            .map_err(Self::rewrap_message_as_storage_error)?;
        if normalized.endpoint.trim().is_empty() {
            return Err(storage_driver_error(
                StorageErrorKind::Misconfigured,
                "COS endpoint is required",
            ));
        }
        let endpoint = Url::parse(&normalized.endpoint)
            .map_aster_err_ctx("parse COS endpoint", AsterError::storage_driver_error)?;
        let host = endpoint.host_str().unwrap_or_default();
        if !host.ends_with(".myqcloud.com") {
            return Err(storage_driver_error(
                StorageErrorKind::Misconfigured,
                "COS endpoint must use a Tencent COS myqcloud.com host",
            ));
        }
        Ok(())
    }

    pub fn new(policy: &storage_policy::Model) -> Result<Self> {
        Self::validate_policy(policy)?;
        let normalized = normalize_s3_endpoint_and_bucket(&policy.endpoint, &policy.bucket)
            .map_err(Self::rewrap_message_as_storage_error)?;
        let mut storage_policy = policy.clone();
        storage_policy.endpoint =
            cos_virtual_hosted_s3_endpoint(&normalized.endpoint, &normalized.bucket)?;
        storage_policy.bucket = normalized.bucket.clone();
        let storage = S3CompatibleDriver::new_with_s3_options(
            &storage_policy,
            S3DriverOptions::virtual_hosted_style(),
        )?;

        Ok(Self {
            storage,
            endpoint: normalized.endpoint,
            bucket: normalized.bucket,
            access_key: policy.access_key.clone(),
            secret_key: policy.secret_key.clone(),
            base_path: policy.base_path.clone(),
        })
    }

    pub fn s3_driver(&self) -> std::sync::Arc<super::s3::S3Driver> {
        self.storage.s3_driver()
    }

    fn rewrap_message_as_storage_error(error: AsterError) -> AsterError {
        storage_driver_error(StorageErrorKind::Misconfigured, error.message())
    }

    fn full_key(&self, path: &str) -> String {
        object_key::join_key_prefix(&self.base_path, path)
    }

    fn object_url(&self, path: &str) -> Result<(Url, String)> {
        let key = self.full_key(path);
        let mut url = Url::parse(&self.endpoint)
            .map_aster_err_ctx("parse COS endpoint", AsterError::storage_driver_error)?;
        let host = url.host_str().ok_or_else(|| {
            storage_driver_error(StorageErrorKind::Misconfigured, "COS endpoint missing host")
        })?;
        if !host.starts_with(&format!("{}.", self.bucket)) {
            let virtual_host = format!("{}.{}", self.bucket, host);
            url.set_host(Some(&virtual_host)).map_aster_err_ctx(
                "build COS virtual-hosted URL",
                AsterError::storage_driver_error,
            )?;
        }

        let endpoint_path = url.path().trim_matches('/');
        let object_path = if endpoint_path.is_empty() {
            key.clone()
        } else {
            format!("{endpoint_path}/{key}")
        };
        url.set_path(&format!("/{object_path}"));
        url.set_query(None);
        url.set_fragment(None);
        Ok((url, key))
    }

    fn signed_ci_preview_url(&self, path: &str, expires: Duration) -> Result<NativePreviewResult> {
        let expires = clamp_cos_ttl(expires, MAX_COS_PREVIEW_TTL, "preview");
        let now = Utc::now();
        let start = now.timestamp();
        let end = (now
            + chrono::Duration::from_std(expires)
                .map_aster_err_ctx("COS preview expiry", AsterError::storage_driver_error)?)
        .timestamp();
        let key_time = format!("{start};{end}");
        let ci_params = [("ci-process", "doc-preview"), ("dstType", "html")];
        let (url, key) = self.signed_cos_query_url(path, &ci_params, &key_time)?;

        Ok(NativePreviewResult {
            url: String::from(url),
            provider: COS_NATIVE_PREVIEW_PROVIDER.to_string(),
            expires_at: Utc.timestamp_opt(end, 0).single().unwrap_or(now),
            cache_key: Some(format!(
                "{COS_NATIVE_PREVIEW_PROVIDER}:{COS_NATIVE_PREVIEW_VERSION}:{key}"
            )),
            version: Some(COS_NATIVE_PREVIEW_VERSION.to_string()),
            open_mode: NativePreviewOpenMode::Iframe,
        })
    }

    fn signed_ci_thumbnail_url(
        &self,
        path: &str,
        max_width: u32,
        max_height: u32,
    ) -> Result<String> {
        let width = max_width.max(1);
        let height = max_height.max(1);
        let now = Utc::now();
        let start = now.timestamp();
        let end = (now
            + chrono::Duration::from_std(MAX_COS_THUMBNAIL_TTL)
                .map_aster_err_ctx("COS thumbnail expiry", AsterError::storage_driver_error)?)
        .timestamp();
        let key_time = format!("{start};{end}");
        let process = format!("imageMogr2/thumbnail/{width}x{height}>/format/webp");
        let params = [(process.as_str(), "")];
        let (url, _) = self.signed_cos_query_url(path, &params, &key_time)?;
        Ok(String::from(url))
    }

    fn signed_cos_query_url(
        &self,
        path: &str,
        params: &[(&str, &str)],
        key_time: &str,
    ) -> Result<(Url, String)> {
        let (mut url, key) = self.object_url(path)?;
        let host = url.host_str().ok_or_else(|| {
            storage_driver_error(
                StorageErrorKind::Misconfigured,
                "COS object URL missing host",
            )
        })?;
        let path_for_sign = url.path().to_string();
        let url_param_list = canonical_param_list(params);
        let http_params = canonical_params(params);
        let http_headers = format!("host={}", percent_encode_lower(host));
        let http_string = format!("get\n{path_for_sign}\n{http_params}\n{http_headers}\n");
        let string_to_sign = format!(
            "{COS_SIGN_ALGORITHM}\n{key_time}\n{}\n",
            sha1_hex(http_string.as_bytes())
        );
        let sign_key = hmac_sha1_hex(self.secret_key.as_bytes(), key_time.as_bytes())?;
        let signature = hmac_sha1_hex(sign_key.as_bytes(), string_to_sign.as_bytes())?;
        let authorization = format!(
            "q-sign-algorithm={COS_SIGN_ALGORITHM}&q-ak={}&q-sign-time={key_time}&q-key-time={key_time}&q-header-list=host&q-url-param-list={url_param_list}&q-signature={signature}",
            self.access_key
        );

        {
            let mut query = url.query_pairs_mut();
            for (key, value) in params {
                query.append_pair(key, value);
            }
            query.append_pair("sign", &authorization);
        }
        Ok((url, key))
    }
}

impl S3CompatibleProvider for TencentCosDriver {
    fn s3_compatible_driver(&self) -> &S3CompatibleDriver {
        &self.storage
    }

    fn as_provider_native_preview(&self) -> Option<&dyn NativePreviewStorageDriver> {
        Some(self)
    }

    fn as_provider_native_thumbnail(&self) -> Option<&dyn NativeThumbnailStorageDriver> {
        Some(self)
    }
}

fn cos_virtual_hosted_s3_endpoint(endpoint: &str, bucket: &str) -> Result<String> {
    let mut url = Url::parse(endpoint)
        .map_aster_err_ctx("parse COS endpoint", AsterError::storage_driver_error)?;
    let host = url
        .host_str()
        .ok_or_else(|| {
            storage_driver_error(StorageErrorKind::Misconfigured, "COS endpoint missing host")
        })?
        .to_string();

    if let Some(root_host) = host.strip_prefix(&format!("{bucket}.")) {
        url.set_host(Some(root_host)).map_aster_err_ctx(
            "build COS S3 API endpoint",
            AsterError::storage_driver_error,
        )?;
    }
    url.set_query(None);
    url.set_fragment(None);
    Ok(String::from(url).trim_end_matches('/').to_string())
}

fn canonical_param_list(params: &[(&str, &str)]) -> String {
    let mut names = params
        .iter()
        .map(|(key, _)| percent_encode_query_lower(key))
        .collect::<Vec<_>>();
    names.sort();
    names.join(";")
}

fn canonical_params(params: &[(&str, &str)]) -> String {
    let mut normalized = params
        .iter()
        .map(|(key, value)| {
            (
                percent_encode_query_lower(key),
                percent_encode_query_lower(value),
            )
        })
        .collect::<Vec<_>>();
    normalized.sort_by(|a, b| a.0.cmp(&b.0));
    normalized
        .into_iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("&")
}

fn clamp_cos_ttl(requested: Duration, max: Duration, label: &str) -> Duration {
    if requested > max {
        tracing::warn!(
            requested_secs = requested.as_secs(),
            max_secs = max.as_secs(),
            "COS native {label} TTL exceeds max, clamping"
        );
        max
    } else if requested.is_zero() {
        tracing::warn!("COS native {label} zero TTL requested, falling back to max");
        max
    } else {
        requested
    }
}

fn percent_encode_lower(value: &str) -> String {
    percent_encode(value.as_bytes(), COS_PATH_ENCODE_SET)
        .to_string()
        .to_ascii_lowercase()
}

fn percent_encode_query_lower(value: &str) -> String {
    percent_encode(value.as_bytes(), COS_QUERY_ENCODE_SET)
        .to_string()
        .to_ascii_lowercase()
}

fn sha1_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha1::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn hmac_sha1_hex(key: &[u8], message: &[u8]) -> Result<String> {
    let mut mac = HmacSha1::new_from_slice(key)
        .map_aster_err_ctx("COS HMAC-SHA1 key", AsterError::storage_driver_error)?;
    mac.update(message);
    Ok(hex::encode(mac.finalize().into_bytes()))
}

fn is_cos_doc_preview_candidate(file_name: &str, mime_type: &str) -> bool {
    const EXTENSIONS: &[&str] = &[
        "doc", "docx", "ppt", "pptx", "xls", "xlsx", "pdf", "txt", "rtf", "odt", "ods", "odp",
    ];
    let extension = std::path::Path::new(file_name)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.trim().to_ascii_lowercase());

    extension
        .as_deref()
        .map(|value| EXTENSIONS.contains(&value))
        .unwrap_or(false)
        || mime_type == "application/pdf"
}

fn is_cos_image_thumbnail_candidate(mime_type: &str) -> bool {
    matches!(
        mime_type.trim().to_ascii_lowercase().as_str(),
        "image/jpeg"
            | "image/jpg"
            | "image/png"
            | "image/webp"
            | "image/gif"
            | "image/bmp"
            | "image/tiff"
    )
}

#[async_trait]
impl NativePreviewStorageDriver for TencentCosDriver {
    async fn create_native_preview(
        &self,
        request: &NativePreviewRequest,
    ) -> Result<Option<NativePreviewResult>> {
        if request.mode != NativePreviewMode::HtmlDocument
            || !is_cos_doc_preview_candidate(&request.source_file_name, &request.source_mime_type)
        {
            return Ok(None);
        }

        self.signed_ci_preview_url(&request.storage_path, request.expires)
            .map(Some)
    }
}

#[async_trait]
impl NativeThumbnailStorageDriver for TencentCosDriver {
    async fn get_native_thumbnail(
        &self,
        request: &NativeThumbnailRequest,
    ) -> Result<Option<Vec<u8>>> {
        if !is_cos_image_thumbnail_candidate(&request.source_mime_type) {
            return Ok(None);
        }

        let url = self.signed_ci_thumbnail_url(
            &request.storage_path,
            request.max_width,
            request.max_height,
        )?;
        if let Ok(parsed_url) = Url::parse(&url) {
            tracing::debug!(
                processor = "storage_native",
                provider = COS_NATIVE_PREVIEW_PROVIDER,
                host = parsed_url.host_str(),
                path = parsed_url.path(),
                "requesting COS native thumbnail"
            );
        }
        let response = reqwest::Client::new()
            .get(url)
            .send()
            .await
            .map_aster_err_ctx(
                "COS native thumbnail request",
                AsterError::storage_driver_error,
            )?;
        let status = response.status();
        if !status.is_success() {
            tracing::debug!(
                processor = "storage_native",
                provider = COS_NATIVE_PREVIEW_PROVIDER,
                http_status = %status,
                "COS native thumbnail request returned non-success status"
            );
            return Err(storage_driver_error(
                if status == reqwest::StatusCode::NOT_FOUND {
                    StorageErrorKind::NotFound
                } else if status == reqwest::StatusCode::FORBIDDEN
                    || status == reqwest::StatusCode::UNAUTHORIZED
                {
                    StorageErrorKind::Auth
                } else if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                    StorageErrorKind::RateLimited
                } else if status.is_server_error() {
                    StorageErrorKind::Transient
                } else {
                    StorageErrorKind::Unsupported
                },
                format!("COS native thumbnail request failed with HTTP {status}"),
            ));
        }
        let bytes = response.bytes().await.map_aster_err_ctx(
            "COS native thumbnail body",
            AsterError::storage_driver_error,
        )?;
        Ok(Some(bytes.to_vec()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::storage_policy;
    use crate::storage::driver::StorageDriver;
    use crate::storage::extensions::{NativePreviewStorageDriver, NativeThumbnailStorageDriver};
    use crate::types::{DriverType, StoredStoragePolicyAllowedTypes, StoredStoragePolicyOptions};

    fn sample_policy(endpoint: &str, bucket: &str) -> storage_policy::Model {
        storage_policy::Model {
            id: 1,
            name: "Tencent COS".to_string(),
            driver_type: DriverType::TencentCos,
            endpoint: endpoint.to_string(),
            bucket: bucket.to_string(),
            access_key: "AKIDEXAMPLE".to_string(),
            secret_key: "SECRETEXAMPLE".to_string(),
            base_path: "tenant/prefix".to_string(),
            remote_node_id: None,
            max_file_size: 0,
            allowed_types: StoredStoragePolicyAllowedTypes::empty(),
            options: StoredStoragePolicyOptions::empty(),
            is_default: false,
            chunk_size: 0,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    fn query_value<'a>(url: &'a Url, key: &str) -> Option<std::borrow::Cow<'a, str>> {
        url.query_pairs()
            .find_map(|(candidate, value)| (candidate == key).then_some(value))
    }

    #[test]
    fn validate_policy_requires_cos_endpoint() {
        let err = TencentCosDriver::validate_policy(&sample_policy("", "bucket"))
            .expect_err("COS endpoint is required");

        assert_eq!(err.code(), "E031");
        assert!(err.message().contains("COS endpoint is required"));
    }

    #[test]
    fn validate_policy_rejects_non_myqcloud_host() {
        let err =
            TencentCosDriver::validate_policy(&sample_policy("https://s3.amazonaws.com", "bucket"))
                .expect_err("non-COS host should fail");

        assert_eq!(err.code(), "E031");
        assert!(err.message().contains("myqcloud.com"));
    }

    #[test]
    fn validate_policy_accepts_myqcloud_host() {
        TencentCosDriver::validate_policy(&sample_policy(
            "https://cos.ap-guangzhou.myqcloud.com",
            "bucket-1250000000",
        ))
        .expect("COS endpoint should pass");
    }

    #[test]
    fn cos_virtual_hosted_s3_endpoint_strips_bucket_host() {
        let endpoint = cos_virtual_hosted_s3_endpoint(
            "https://bucket-1250000000.cos.ap-guangzhou.myqcloud.com",
            "bucket-1250000000",
        )
        .expect("COS S3 endpoint");

        assert_eq!(endpoint, "https://cos.ap-guangzhou.myqcloud.com");
    }

    #[test]
    fn cos_virtual_hosted_s3_endpoint_keeps_root_host() {
        let endpoint = cos_virtual_hosted_s3_endpoint(
            "https://cos.ap-guangzhou.myqcloud.com",
            "bucket-1250000000",
        )
        .expect("COS S3 endpoint");

        assert_eq!(endpoint, "https://cos.ap-guangzhou.myqcloud.com");
    }

    #[test]
    fn object_url_uses_virtual_host_and_base_path() {
        let driver = TencentCosDriver::new(&sample_policy(
            "https://cos.ap-guangzhou.myqcloud.com",
            "bucket-1250000000",
        ))
        .expect("driver should build");

        let (url, key) = driver
            .object_url("docs/report 1.docx")
            .expect("object URL should build");

        assert_eq!(key, "tenant/prefix/docs/report 1.docx");
        assert_eq!(
            url.host_str(),
            Some("bucket-1250000000.cos.ap-guangzhou.myqcloud.com")
        );
        assert_eq!(url.path(), "/tenant/prefix/docs/report%201.docx");
        assert!(url.query().is_none());
    }

    #[test]
    fn object_url_does_not_duplicate_virtual_host_bucket() {
        let driver = TencentCosDriver::new(&sample_policy(
            "https://bucket-1250000000.cos.ap-guangzhou.myqcloud.com",
            "bucket-1250000000",
        ))
        .expect("driver should build");

        let (url, _key) = driver.object_url("a.docx").expect("object URL");

        assert_eq!(
            url.host_str(),
            Some("bucket-1250000000.cos.ap-guangzhou.myqcloud.com")
        );
    }

    #[test]
    fn signed_ci_preview_url_contains_required_ci_and_signature_params() {
        let driver = TencentCosDriver::new(&sample_policy(
            "https://cos.ap-guangzhou.myqcloud.com",
            "bucket-1250000000",
        ))
        .expect("driver should build");

        let result = driver
            .signed_ci_preview_url("docs/report.docx", Duration::from_secs(300))
            .expect("signed preview URL");
        let url = Url::parse(&result.url).expect("preview URL should parse");
        let sign = query_value(&url, "sign").expect("sign query parameter");

        assert_eq!(result.provider, COS_NATIVE_PREVIEW_PROVIDER);
        assert_eq!(result.version.as_deref(), Some(COS_NATIVE_PREVIEW_VERSION));
        assert_eq!(result.open_mode, NativePreviewOpenMode::Iframe);
        assert!(result.cache_key.as_deref().is_some_and(|value| {
            value == "tencent_cos_ci:cos-ci-doc-preview-html-v1:tenant/prefix/docs/report.docx"
        }));
        assert_eq!(
            query_value(&url, "ci-process").as_deref(),
            Some("doc-preview")
        );
        assert_eq!(query_value(&url, "dstType").as_deref(), Some("html"));
        assert!(sign.contains("q-sign-algorithm=sha1"));
        assert!(sign.contains("q-ak=AKIDEXAMPLE"));
        assert!(sign.contains("q-header-list=host"));
        assert!(sign.contains("q-url-param-list=ci-process;dsttype"));
        assert!(sign.contains("q-signature="));
    }

    #[test]
    fn signed_ci_thumbnail_url_contains_image_processing_and_signature_params() {
        let driver = TencentCosDriver::new(&sample_policy(
            "https://cos.ap-guangzhou.myqcloud.com",
            "bucket-1250000000",
        ))
        .expect("driver should build");

        let signed = driver
            .signed_ci_thumbnail_url("images/photo.png", 320, 240)
            .expect("signed thumbnail URL");
        let url = Url::parse(&signed).expect("thumbnail URL should parse");
        let sign = query_value(&url, "sign").expect("sign query parameter");

        assert!(url.query_pairs().any(|(key, value)| key
            == "imageMogr2/thumbnail/320x240>/format/webp"
            && value.is_empty()));
        assert!(sign.contains("q-sign-algorithm=sha1"));
        assert!(sign.contains("q-ak=AKIDEXAMPLE"));
        assert!(sign.contains("q-header-list=host"));
        assert!(
            sign.contains("q-url-param-list=imagemogr2%2fthumbnail%2f320x240%3e%2fformat%2fwebp")
        );
        assert!(sign.contains("q-signature="));
    }

    #[test]
    fn signed_ci_preview_url_clamps_zero_and_excessive_ttl() {
        let driver = TencentCosDriver::new(&sample_policy(
            "https://cos.ap-guangzhou.myqcloud.com",
            "bucket-1250000000",
        ))
        .expect("driver should build");

        let zero_ttl = driver
            .signed_ci_preview_url("docs/report.docx", Duration::ZERO)
            .expect("zero TTL should clamp");
        let too_large_ttl = driver
            .signed_ci_preview_url("docs/report.docx", Duration::from_secs(24 * 60 * 60))
            .expect("large TTL should clamp");

        for result in [zero_ttl, too_large_ttl] {
            let signed_window = result
                .expires_at
                .signed_duration_since(Utc::now())
                .num_seconds();
            assert!(
                (0..=MAX_COS_PREVIEW_TTL.as_secs() as i64).contains(&signed_window),
                "expected clamped TTL, got {signed_window}s"
            );
        }
    }

    #[tokio::test]
    async fn native_preview_only_supports_html_document_candidates() {
        let driver = TencentCosDriver::new(&sample_policy(
            "https://cos.ap-guangzhou.myqcloud.com",
            "bucket-1250000000",
        ))
        .expect("driver should build");

        let base_request = NativePreviewRequest {
            storage_path: "docs/report.docx".to_string(),
            source_file_name: "report.docx".to_string(),
            source_mime_type:
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
                    .to_string(),
            mode: NativePreviewMode::HtmlDocument,
            expires: Duration::from_secs(300),
        };

        assert!(
            driver
                .create_native_preview(&base_request)
                .await
                .expect("preview should build")
                .is_some()
        );

        let mut unsupported_mode = base_request.clone();
        unsupported_mode.mode = NativePreviewMode::PdfDocument;
        assert!(
            driver
                .create_native_preview(&unsupported_mode)
                .await
                .expect("unsupported mode should not error")
                .is_none()
        );

        let mut unsupported_extension = base_request;
        unsupported_extension.source_file_name = "archive.zip".to_string();
        unsupported_extension.source_mime_type = "application/zip".to_string();
        assert!(
            driver
                .create_native_preview(&unsupported_extension)
                .await
                .expect("unsupported extension should not error")
                .is_none()
        );
    }

    #[tokio::test]
    async fn native_thumbnail_supports_only_cos_image_candidates() {
        let driver = TencentCosDriver::new(&sample_policy(
            "https://cos.ap-guangzhou.myqcloud.com",
            "bucket-1250000000",
        ))
        .expect("driver should build");

        let unsupported = NativeThumbnailRequest {
            storage_path: "docs/report.pdf".to_string(),
            source_mime_type: "application/pdf".to_string(),
            max_width: 320,
            max_height: 240,
        };

        assert!(
            driver
                .get_native_thumbnail(&unsupported)
                .await
                .expect("unsupported mime should not call COS")
                .is_none()
        );
        assert!(is_cos_image_thumbnail_candidate("image/webp"));
        assert!(is_cos_image_thumbnail_candidate("image/png"));
        assert!(!is_cos_image_thumbnail_candidate("image/svg+xml"));
    }

    #[test]
    fn s3_compatible_capabilities_are_available_on_cos_driver() {
        let driver = TencentCosDriver::new(&sample_policy(
            "https://cos.ap-guangzhou.myqcloud.com",
            "bucket-1250000000",
        ))
        .expect("driver should build");

        assert!(driver.as_presigned().is_some());
        assert!(driver.as_list().is_some());
        assert!(driver.as_stream_upload().is_some());
        assert!(driver.as_multipart().is_some());
        assert!(driver.as_native_preview().is_some());
        assert!(driver.as_native_thumbnail().is_some());
    }
}
