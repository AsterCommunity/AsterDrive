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
fn validate_config_requires_region_to_match_standard_endpoint() {
    let mut config = sample_config();
    config.region = "cn-beijing".to_string();
    let error = AlibabaOssDriver::validate_config(&config, &sample_credentials())
        .expect_err("mismatched standard endpoint region must fail");
    assert_eq!(error.kind(), StorageErrorKind::Misconfigured);
    assert!(
        error
            .message()
            .contains("does not match public endpoint region")
    );
    assert!(error.message().contains("cn-hangzhou"));

    config.endpoint = "https://oss-accelerate.aliyuncs.com".to_string();
    AlibabaOssDriver::validate_config(&config, &sample_credentials())
        .expect("accelerate endpoint should not be interpreted as a region");
}

#[test]
fn official_endpoint_region_parser_handles_nonregional_and_invalid_hosts() {
    assert!(official_oss_endpoint_region("not a url").is_err());
    assert_eq!(
        official_oss_endpoint_region("https://files.example.test").unwrap(),
        None
    );
    assert_eq!(
        official_oss_endpoint_region("file:///tmp/oss").unwrap(),
        None
    );
    assert_eq!(
        official_oss_endpoint_region("https://oss-.aliyuncs.com").unwrap(),
        None
    );
    assert_eq!(
        official_oss_endpoint_region("https://oss-cn-hangzhou-internal.aliyuncs.com").unwrap(),
        Some("cn-hangzhou".to_string())
    );
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

    let presigned = driver.extensions().presigned.expect("presigned extension");
    let request = presigned
        .presigned_put_request("docs/report.txt", Duration::from_secs(60))
        .await
        .expect("presigned PUT")
        .expect("presigned URL");

    assert!(request.url.starts_with(
        "https://asterdrive-test.oss-cn-hangzhou.aliyuncs.com/tenant/prefix/docs/report.txt"
    ));
    assert!(request.url.contains("x-oss-signature="));
    assert!(!request.url.contains("internal"));
    assert_eq!(
        request.headers.get("content-type").map(String::as_str),
        Some(signing::OSS_PRESIGNED_PUT_CONTENT_TYPE)
    );
    assert!(!presigned.presigned_single_put_requires_etag());
}

#[tokio::test]
async fn presigned_upload_part_request_uses_public_endpoint() {
    let mut config = sample_config();
    config.server_side_endpoint = "https://oss-cn-hangzhou-internal.aliyuncs.com".to_string();
    let driver =
        AlibabaOssDriver::new(config, sample_credentials()).expect("OSS driver should build");

    let request = driver
        .extensions()
        .multipart
        .expect("multipart extension")
        .presigned_upload_part_request("video.bin", "upload-id", 7, Duration::from_secs(60))
        .await
        .expect("presigned part URL");
    let parsed = Url::parse(&request.url).expect("valid OSS presigned part URL");
    let query = parsed
        .query_pairs()
        .into_owned()
        .collect::<std::collections::HashMap<_, _>>();

    assert_eq!(
        parsed.host_str(),
        Some("asterdrive-test.oss-cn-hangzhou.aliyuncs.com")
    );
    assert!(!request.url.contains("internal"));
    assert_eq!(query.get("partNumber").map(String::as_str), Some("7"));
    assert_eq!(query.get("uploadId").map(String::as_str), Some("upload-id"));
    assert!(query.contains_key("x-oss-signature"));
    assert_eq!(
        request.headers.get("content-type").map(String::as_str),
        Some(signing::OSS_PRESIGNED_PUT_CONTENT_TYPE)
    );
}

#[tokio::test]
async fn public_presigned_url_uses_cname_without_bucket_wire_path() {
    let mut config = sample_config();
    config.endpoint = "https://files.example.test".to_string();
    config.use_cname = true;
    config.server_side_endpoint = "https://oss-cn-hangzhou-internal.aliyuncs.com".to_string();
    let driver =
        AlibabaOssDriver::new(config, sample_credentials()).expect("OSS CNAME driver should build");

    let request = driver
        .extensions()
        .presigned
        .expect("presigned extension")
        .presigned_put_request("docs/report.txt", Duration::from_secs(60))
        .await
        .expect("presigned PUT")
        .expect("presigned URL");

    assert!(
        request
            .url
            .starts_with("https://files.example.test/tenant/prefix/docs/report.txt")
    );
    assert!(!request.url.contains("/asterdrive-test/"));
    assert!(request.url.contains("x-oss-signature="));
}

#[tokio::test]
async fn public_presigned_get_omits_unsupported_content_type_override() {
    let mut config = sample_config();
    config.server_side_endpoint = "https://oss-cn-hangzhou-internal.aliyuncs.com".to_string();
    let driver =
        AlibabaOssDriver::new(config, sample_credentials()).expect("OSS driver should build");

    let url = driver
        .extensions()
        .presigned
        .expect("presigned extension")
        .presigned_url(
            "files/photo.jpg",
            Duration::from_secs(300),
            PresignedDownloadOptions {
                response_cache_control: Some("private, max-age=0, must-revalidate".to_string()),
                response_content_disposition: Some(
                    "inline; filename*=UTF-8''photo@0.5x.jpg".to_string(),
                ),
                response_content_type: Some("image/jpeg".to_string()),
                ..Default::default()
            },
        )
        .await
        .expect("presigned GET")
        .expect("presigned URL");
    let parsed = Url::parse(&url).expect("valid OSS presigned URL");
    let raw_query = parsed.query().expect("presigned GET query");
    let query = parsed
        .query_pairs()
        .into_owned()
        .collect::<std::collections::HashMap<_, _>>();

    assert_eq!(
        parsed.host_str(),
        Some("asterdrive-test.oss-cn-hangzhou.aliyuncs.com")
    );
    assert!(!url.contains("internal"));
    assert_eq!(
        query.get("response-cache-control").map(String::as_str),
        Some("private, max-age=0, must-revalidate")
    );
    assert_eq!(
        query
            .get("response-content-disposition")
            .map(String::as_str),
        Some("inline; filename*=UTF-8''photo@0.5x.jpg")
    );
    assert!(!query.contains_key("response-content-type"));
    assert!(raw_query.contains("filename%2A%3DUTF-8%27%27photo%400.5x.jpg"));
    assert!(query.contains_key("x-oss-signature"));
}
