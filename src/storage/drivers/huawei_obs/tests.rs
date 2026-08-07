use std::time::Duration;

use aster_drive_storage::{StorageDriver, StorageErrorKind};

use super::{
    HuaweiObsAddressingMode, HuaweiObsDriver, HuaweiObsDriverConfig, HuaweiObsSigningMode,
    HuaweiObsStaticCredentials,
};

fn config(endpoint: &str, addressing_mode: HuaweiObsAddressingMode) -> HuaweiObsDriverConfig {
    HuaweiObsDriverConfig {
        endpoint: endpoint.to_string(),
        bucket: "archive-bucket".to_string(),
        base_path: "tenant-a".to_string(),
        region: "cn-north-4".to_string(),
        addressing_mode,
        signing_mode: HuaweiObsSigningMode::Obs,
        connect_timeout: Duration::from_secs(5),
        read_timeout: Duration::from_secs(30),
        operation_timeout: Duration::from_secs(3_600),
    }
}

fn credentials() -> HuaweiObsStaticCredentials {
    HuaweiObsStaticCredentials {
        access_key: "access-key".to_string(),
        secret_key: "secret-key".to_string(),
    }
}

#[test]
fn normalizes_bucket_prefixed_official_endpoint() {
    let normalized = HuaweiObsDriver::normalize_endpoint(
        "https://archive-bucket.obs.cn-north-4.myhuaweicloud.com/",
        "archive-bucket",
        " CN-NORTH-4 ",
        HuaweiObsAddressingMode::VirtualHosted,
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
        HuaweiObsAddressingMode::VirtualHosted,
    )
    .expect_err("generic S3 endpoint must not pass as native OBS");
    assert_eq!(error.kind(), StorageErrorKind::Misconfigured);
    assert!(error.message().contains("virtual-hosted endpoint"));

    let error = HuaweiObsDriver::normalize_endpoint(
        "https://obs.cn-north-4.myhuaweicloud.com",
        "archive-bucket",
        "cn-north-4",
        HuaweiObsAddressingMode::CustomDomain,
    )
    .expect_err("official endpoint must not be marked as a custom domain");
    assert!(error.message().contains("custom-domain addressing"));

    HuaweiObsDriver::normalize_endpoint(
        "https://archive-bucket.obs.cn-north-4.myhuaweicloud.com",
        "archive-bucket",
        "cn-north-4",
        HuaweiObsAddressingMode::CustomDomain,
    )
    .expect_err("bucket-prefixed official endpoint is not a custom domain");
}

#[test]
fn exposes_s3_shaped_runtime_capabilities_under_obs_signing() {
    let driver = HuaweiObsDriver::new(
        config(
            "https://obs.cn-north-4.myhuaweicloud.com",
            HuaweiObsAddressingMode::VirtualHosted,
        ),
        credentials(),
    )
    .expect("valid OBS driver");

    assert!(driver.supports_efficient_range());
    assert!(driver.extensions().presigned.is_some());
    assert!(driver.extensions().list.is_some());
    assert!(driver.extensions().stream_upload.is_some());
    assert!(driver.extensions().multipart.is_some());
}

#[tokio::test]
async fn presigned_put_request_uses_obs_signature_and_driver_owned_contract() {
    let driver = HuaweiObsDriver::new(
        config(
            "https://obs.cn-north-4.myhuaweicloud.com",
            HuaweiObsAddressingMode::VirtualHosted,
        ),
        credentials(),
    )
    .expect("valid OBS driver");

    let presigned = driver
        .extensions()
        .presigned
        .expect("OBS presigned capability");
    assert!(presigned.presigned_single_put_requires_etag());
    let request = presigned
        .presigned_put_request("docs/report.txt", Duration::from_secs(60))
        .await
        .expect("OBS presigned PUT should build")
        .expect("OBS should expose presigned PUT");

    let url = url::Url::parse(&request.url).expect("OBS presigned PUT URL");
    let query = url
        .query_pairs()
        .collect::<std::collections::HashMap<_, _>>();
    assert_eq!(
        query.get("AccessKeyId").map(|value| value.as_ref()),
        Some("access-key")
    );
    assert!(query.contains_key("Expires"));
    assert!(query.contains_key("Signature"));
    assert!(!query.contains_key("X-Amz-Signature"));
}

#[test]
fn custom_domain_allows_region_to_be_omitted() {
    let mut config = config(
        "https://files.example.com",
        HuaweiObsAddressingMode::CustomDomain,
    );
    config.region.clear();
    HuaweiObsDriver::new(config, credentials()).expect("custom-domain OBS driver");

    HuaweiObsDriver::normalize_endpoint(
        "https://archive-bucket.example.com",
        "archive-bucket",
        "",
        HuaweiObsAddressingMode::CustomDomain,
    )
    .expect("a valid custom domain may start with the bucket name");
}
