use super::native_thumbnail::is_cos_image_thumbnail_candidate;
use super::signing::cos_virtual_hosted_s3_endpoint;
use super::*;
use aster_drive_storage::traits::driver::StorageDriver;
use aster_drive_storage::traits::extensions::{
    NativeThumbnailRequest, NativeThumbnailStorageDriver,
};
use url::Url;

fn sample_config(endpoint: &str, bucket: &str) -> TencentCosDriverConfig {
    TencentCosDriverConfig {
        endpoint: endpoint.to_string(),
        bucket: bucket.to_string(),
        base_path: "tenant/prefix".to_string(),
        connect_timeout: std::time::Duration::from_secs(5),
        read_timeout: std::time::Duration::from_secs(30),
        operation_timeout: std::time::Duration::from_secs(3_600),
    }
}

fn sample_credentials() -> TencentCosStaticCredentials {
    TencentCosStaticCredentials {
        access_key: "AKIDEXAMPLE".to_string(),
        secret_key: "SECRETEXAMPLE".to_string(),
    }
}

fn sample_driver(endpoint: &str, bucket: &str) -> TencentCosDriver {
    TencentCosDriver::new(sample_config(endpoint, bucket), sample_credentials())
        .expect("driver should build")
}

fn query_value<'a>(url: &'a Url, key: &str) -> Option<std::borrow::Cow<'a, str>> {
    url.query_pairs()
        .find_map(|(candidate, value)| (candidate == key).then_some(value))
}

#[test]
fn validate_config_requires_cos_endpoint() {
    let err =
        TencentCosDriver::validate_config(&sample_config("", "bucket"), &sample_credentials())
            .expect_err("COS endpoint is required");

    assert_eq!(err.kind(), StorageErrorKind::Misconfigured);
    assert!(err.message().contains("COS endpoint is required"));
}

#[test]
fn validate_config_rejects_non_myqcloud_host() {
    let err = TencentCosDriver::validate_config(
        &sample_config("https://s3.amazonaws.com", "bucket"),
        &sample_credentials(),
    )
    .expect_err("non-COS host should fail");

    assert_eq!(err.kind(), StorageErrorKind::Misconfigured);
    assert!(err.message().contains("myqcloud.com"));
}

#[test]
fn validate_config_accepts_myqcloud_host() {
    TencentCosDriver::validate_config(
        &sample_config("https://cos.ap-guangzhou.myqcloud.com", "bucket-1250000000"),
        &sample_credentials(),
    )
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
    let driver = sample_driver("https://cos.ap-guangzhou.myqcloud.com", "bucket-1250000000");

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
    let driver = sample_driver(
        "https://bucket-1250000000.cos.ap-guangzhou.myqcloud.com",
        "bucket-1250000000",
    );

    let (url, _key) = driver.object_url("a.docx").expect("object URL");

    assert_eq!(
        url.host_str(),
        Some("bucket-1250000000.cos.ap-guangzhou.myqcloud.com")
    );
}

#[test]
fn signed_ci_thumbnail_url_contains_image_processing_and_signature_params() {
    let driver = sample_driver("https://cos.ap-guangzhou.myqcloud.com", "bucket-1250000000");

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

#[tokio::test]
async fn native_thumbnail_supports_only_cos_image_candidates() {
    let driver = sample_driver("https://cos.ap-guangzhou.myqcloud.com", "bucket-1250000000");

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
    let driver = sample_driver("https://cos.ap-guangzhou.myqcloud.com", "bucket-1250000000");

    assert!(driver.extensions().presigned.is_some());
    assert!(
        !driver
            .extensions()
            .presigned
            .expect("presigned capability")
            .presigned_single_put_requires_etag()
    );
    assert!(driver.extensions().list.is_some());
    assert!(driver.extensions().stream_upload.is_some());
    assert!(driver.extensions().multipart.is_some());
    assert!(driver.extensions().native_thumbnail.is_some());
}
