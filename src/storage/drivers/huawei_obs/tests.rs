use std::time::Duration;

use aster_drive_storage::{StorageDriver, StorageErrorKind};

use super::{HuaweiObsDriver, HuaweiObsDriverConfig, HuaweiObsStaticCredentials};

fn config(endpoint: &str, use_cname: bool) -> HuaweiObsDriverConfig {
    HuaweiObsDriverConfig {
        endpoint: endpoint.to_string(),
        bucket: "archive-bucket".to_string(),
        base_path: "tenant-a".to_string(),
        region: "cn-north-4".to_string(),
        use_cname,
        download_base_url: None,
        cdn_auth_type: "type_a".to_string(),
        cdn_expire_time: 3600,
        cdn_sign_arg: "sign".to_string(),
        cdn_time_arg: "timestamp".to_string(),
        cdn_signing_algorithm: "md5".to_string(),
        connect_timeout: Duration::from_secs(5),
        read_timeout: Duration::from_secs(30),
        operation_timeout: Duration::from_secs(3_600),
    }
}

fn credentials() -> HuaweiObsStaticCredentials {
    HuaweiObsStaticCredentials {
        access_key: "access-key".to_string(),
        secret_key: "secret-key".to_string(),
        cdn_primary_key: None,
        cdn_backup_key: None,
    }
}

#[test]
fn normalizes_bucket_prefixed_official_endpoint() {
    let normalized = HuaweiObsDriver::normalize_endpoint(
        "https://archive-bucket.obs.cn-north-4.myhuaweicloud.com/",
        "archive-bucket",
        " CN-NORTH-4 ",
        false,
    )
    .expect("official OBS endpoint");

    assert_eq!(
        normalized.endpoint,
        "https://obs.cn-north-4.myhuaweicloud.com"
    );
    assert_eq!(normalized.bucket, "archive-bucket");
    assert_eq!(normalized.region, "cn-north-4");
}

#[test]
fn rejects_incompatible_endpoint_and_addressing_combinations() {
    let error = HuaweiObsDriver::normalize_endpoint(
        "https://s3.example.com",
        "archive-bucket",
        "cn-north-4",
        false,
    )
    .expect_err("generic S3 endpoint must not pass as native OBS");
    assert_eq!(error.kind(), StorageErrorKind::Misconfigured);
    assert!(error.message().contains("virtual-hosted endpoint"));

    let error = HuaweiObsDriver::normalize_endpoint(
        "https://obs.cn-north-4.myhuaweicloud.com",
        "archive-bucket",
        "cn-north-4",
        true,
    )
    .expect_err("official endpoint must not be marked as a custom domain");
    assert!(error.message().contains("custom-domain addressing"));

    HuaweiObsDriver::normalize_endpoint(
        "https://archive-bucket.obs.cn-north-4.myhuaweicloud.com",
        "archive-bucket",
        "cn-north-4",
        true,
    )
    .expect_err("bucket-prefixed official endpoint is not a custom domain");
}

#[test]
fn rejects_invalid_region_and_bucket_names() {
    let error = HuaweiObsDriver::normalize_endpoint(
        "https://obs.cn-north-4.myhuaweicloud.com",
        "archive-bucket",
        "cn_north_4",
        false,
    )
    .expect_err("underscore is not a valid OBS region character");
    assert_eq!(error.kind(), StorageErrorKind::Misconfigured);
    assert!(error.message().contains("obs_region"));

    let error = HuaweiObsDriver::normalize_endpoint(
        "https://obs.cn-north-4.myhuaweicloud.com",
        "Ab",
        "cn-north-4",
        false,
    )
    .expect_err("short uppercase bucket must be rejected");
    assert_eq!(error.kind(), StorageErrorKind::Misconfigured);
    assert!(error.message().contains("bucket"));
}

#[test]
fn exposes_s3_shaped_runtime_capabilities_under_obs_signing() {
    let driver = HuaweiObsDriver::new(
        config("https://obs.cn-north-4.myhuaweicloud.com", false),
        credentials(),
    )
    .expect("valid OBS driver");

    assert!(driver.supports_efficient_range());
    assert!(driver.extensions().direct_download.is_some());
    assert!(driver.extensions().presigned_upload.is_some());
    assert!(driver.extensions().list.is_some());
    assert!(driver.extensions().stream_upload.is_some());
    assert!(driver.extensions().multipart.is_some());
}

#[test]
fn obs_reports_upload_part_limits() {
    let driver = HuaweiObsDriver::new(
        config("https://obs.cn-north-4.myhuaweicloud.com", false),
        credentials(),
    )
    .expect("valid OBS driver");
    let capability = driver
        .extensions()
        .multipart
        .expect("multipart capability")
        .capabilities();
    assert_eq!(capability.min_part_size, 100 * 1024);
    assert_eq!(capability.max_part_size, Some(5 * 1024 * 1024 * 1024));
    assert_eq!(capability.max_parts, 10_000);
}

#[test]
fn custom_domain_allows_region_to_be_omitted() {
    let mut config = config("https://files.example.com", true);
    config.region.clear();
    HuaweiObsDriver::new(config, credentials()).expect("custom-domain OBS driver");

    HuaweiObsDriver::normalize_endpoint(
        "https://archive-bucket.example.com",
        "archive-bucket",
        "",
        true,
    )
    .expect("a valid custom domain may start with the bucket name");
}
