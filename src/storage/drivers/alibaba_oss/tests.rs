use std::time::Duration;

use aster_drive_storage::StorageDriver;

use super::*;

fn sample_config() -> AlibabaOssDriverConfig {
    AlibabaOssDriverConfig {
        endpoint: "https://oss-cn-hangzhou.aliyuncs.com".to_string(),
        server_side_endpoint: String::new(),
        region: "cn-hangzhou".to_string(),
        bucket: "asterdrive-test".to_string(),
        base_path: "tenant/prefix".to_string(),
        use_cname: false,
        connect_timeout: Duration::from_secs(5),
        read_timeout: Duration::from_secs(30),
        operation_timeout: Duration::from_secs(3_600),
    }
}

fn sample_credentials() -> AlibabaOssStaticCredentials {
    AlibabaOssStaticCredentials {
        access_key: "ak".to_string(),
        secret_key: "sk".to_string(),
    }
}

#[test]
fn validate_config_accepts_public_oss_endpoint() {
    AlibabaOssDriver::validate_config(&sample_config(), &sample_credentials())
        .expect("valid OSS config");
}

#[test]
fn validate_config_accepts_bucket_qualified_endpoint() {
    let mut config = sample_config();
    config.endpoint = "https://asterdrive-test.oss-cn-hangzhou.aliyuncs.com".to_string();

    AlibabaOssDriver::validate_config(&config, &sample_credentials())
        .expect("bucket-qualified OSS endpoint");
}

#[test]
fn validate_config_accepts_custom_domain_only_in_cname_mode() {
    let mut config = sample_config();
    config.endpoint = "https://files.example.test".to_string();
    let err = AlibabaOssDriver::validate_config(&config, &sample_credentials())
        .expect_err("custom domain without CNAME mode should fail");
    assert_eq!(err.kind(), StorageErrorKind::Misconfigured);
    assert!(err.message().contains("CNAME"));

    config.use_cname = true;
    AlibabaOssDriver::validate_config(&config, &sample_credentials())
        .expect("custom domain in CNAME mode");
}

#[test]
fn validate_config_rejects_oss_host_in_cname_mode() {
    let mut config = sample_config();
    config.use_cname = true;

    let err = AlibabaOssDriver::validate_config(&config, &sample_credentials())
        .expect_err("provider endpoint is not a CNAME domain");
    assert_eq!(err.kind(), StorageErrorKind::Misconfigured);
    assert!(err.message().contains("custom-domain"));
}

#[test]
fn validate_config_checks_server_side_endpoint_as_provider_endpoint() {
    let mut config = sample_config();
    config.server_side_endpoint = "https://oss-cn-hangzhou-internal.aliyuncs.com".to_string();
    AlibabaOssDriver::validate_config(&config, &sample_credentials())
        .expect("valid internal OSS endpoint");

    config.server_side_endpoint = "https://internal.example.test".to_string();
    let err = AlibabaOssDriver::validate_config(&config, &sample_credentials())
        .expect_err("non-provider server endpoint should fail");
    assert_eq!(err.kind(), StorageErrorKind::Misconfigured);
}

#[test]
fn validate_config_rejects_invalid_bucket_and_region() {
    let mut config = sample_config();
    config.bucket = "Invalid_Bucket".to_string();
    let err = AlibabaOssDriver::validate_config(&config, &sample_credentials())
        .expect_err("invalid bucket should fail");
    assert!(err.message().contains("OSS bucket"));

    config = sample_config();
    config.region = "".to_string();
    let err = AlibabaOssDriver::validate_config(&config, &sample_credentials())
        .expect_err("missing region should fail");
    assert!(err.message().contains("OSS region is required"));
}

#[test]
fn driver_exposes_s3_compatible_capabilities_with_public_presigning() {
    let driver = AlibabaOssDriver::new(sample_config(), sample_credentials())
        .expect("OSS driver should build");
    let extensions = driver.extensions();

    assert!(driver.supports_efficient_range());
    assert!(extensions.presigned.is_some());
    assert!(extensions.list.is_some());
    assert!(extensions.stream_upload.is_some());
    assert!(extensions.multipart.is_some());
}

#[tokio::test]
async fn public_presigned_url_ignores_server_side_endpoint() {
    let mut config = sample_config();
    config.server_side_endpoint = "https://oss-cn-hangzhou-internal.aliyuncs.com".to_string();
    let driver =
        AlibabaOssDriver::new(config, sample_credentials()).expect("OSS driver should build");

    let url = driver
        .extensions()
        .presigned
        .expect("presigned extension")
        .presigned_put_url("docs/report.txt", Duration::from_secs(60))
        .await
        .expect("presigned PUT")
        .expect("presigned URL");

    assert!(url.starts_with(
        "https://asterdrive-test.oss-cn-hangzhou.aliyuncs.com/tenant/prefix/docs/report.txt"
    ));
    assert!(url.contains("x-oss-signature="));
    assert!(!url.contains("internal"));
}

#[tokio::test]
async fn public_presigned_url_uses_cname_without_bucket_wire_path() {
    let mut config = sample_config();
    config.endpoint = "https://files.example.test".to_string();
    config.use_cname = true;
    config.server_side_endpoint = "https://oss-cn-hangzhou-internal.aliyuncs.com".to_string();
    let driver =
        AlibabaOssDriver::new(config, sample_credentials()).expect("OSS CNAME driver should build");

    let url = driver
        .extensions()
        .presigned
        .expect("presigned extension")
        .presigned_put_url("docs/report.txt", Duration::from_secs(60))
        .await
        .expect("presigned PUT")
        .expect("presigned URL");

    assert!(url.starts_with("https://files.example.test/tenant/prefix/docs/report.txt"));
    assert!(!url.contains("/asterdrive-test/"));
    assert!(url.contains("x-oss-signature="));
}
