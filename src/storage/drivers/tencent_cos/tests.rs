use super::native_thumbnail::is_cos_image_thumbnail_candidate;
use super::signing::cos_virtual_hosted_s3_endpoint;
use super::*;
use crate::entities::storage_policy;
use crate::storage::driver::StorageDriver;
use crate::storage::extensions::{
    NativePreviewMode, NativePreviewOpenMode, NativePreviewRequest, NativePreviewStorageDriver,
    NativeThumbnailRequest, NativeThumbnailStorageDriver,
};
use crate::types::{DriverType, StoredStoragePolicyAllowedTypes, StoredStoragePolicyOptions};
use chrono::Utc;
use std::time::Duration;
use url::Url;

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
    assert!(sign.contains("q-url-param-list=imagemogr2%2fthumbnail%2f320x240%3e%2fformat%2fwebp"));
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
        source_mime_type: "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
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
