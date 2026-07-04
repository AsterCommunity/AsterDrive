use super::native_thumbnail::is_cos_image_thumbnail_candidate;
use super::signing::cos_virtual_hosted_s3_endpoint;
use super::*;
use crate::entities::storage_policy;
use crate::storage::traits::driver::StorageDriver;
use crate::storage::traits::extensions::{NativeThumbnailRequest, NativeThumbnailStorageDriver};
use crate::types::{DriverType, StoredStoragePolicyAllowedTypes, StoredStoragePolicyOptions};
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
        remote_storage_target_key: None,
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
    assert!(driver.as_native_thumbnail().is_some());
}
