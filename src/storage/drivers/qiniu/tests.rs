use std::time::Duration;

use aster_drive_storage::traits::extensions::PresignedStorageDriver;
use aster_drive_storage::traits::multipart::MultipartStorageDriver;
use aster_drive_storage::{StorageDriver, StorageErrorKind};

use super::{QiniuDriver, QiniuDriverConfig, QiniuStaticCredentials};

fn config() -> QiniuDriverConfig {
    QiniuDriverConfig {
        endpoint: "https://s3.example.test".to_string(),
        bucket: "archive".to_string(),
        base_path: "tenant-a".to_string(),
        region: "cn-east-1".to_string(),
        path_style: true,
        connect_timeout: Duration::from_secs(5),
        read_timeout: Duration::from_secs(30),
        operation_timeout: Duration::from_secs(3_600),
    }
}

fn credentials() -> QiniuStaticCredentials {
    QiniuStaticCredentials {
        access_key: "access-key".to_string(),
        secret_key: "secret-key".to_string(),
    }
}

#[test]
fn validates_required_s3_compatible_connection_values() {
    assert!(QiniuDriver::validate_config(&config(), &credentials()).is_ok());

    let mut invalid_endpoint = config();
    invalid_endpoint.endpoint = "s3.example.test".to_string();
    assert!(QiniuDriver::validate_config(&invalid_endpoint, &credentials()).is_err());

    let mut missing_endpoint = config();
    missing_endpoint.endpoint.clear();
    let error = QiniuDriver::validate_config(&missing_endpoint, &credentials())
        .expect_err("Qiniu endpoint is required");
    assert_eq!(error.kind(), StorageErrorKind::Misconfigured);

    let mut invalid_region = config();
    invalid_region.region = "cn east/1".to_string();
    assert!(QiniuDriver::validate_config(&invalid_region, &credentials()).is_err());

    let mut empty_credentials = credentials();
    empty_credentials.access_key.clear();
    assert_eq!(
        QiniuDriver::validate_config(&config(), &empty_credentials)
            .expect_err("empty access key should fail")
            .kind(),
        StorageErrorKind::Auth
    );
}

#[test]
fn exposes_standard_s3_compatible_capabilities() {
    let driver = QiniuDriver::new(config(), credentials()).expect("driver should build");

    assert!(driver.supports_efficient_range());
    assert!(driver.extensions().presigned.is_some());
    assert!(driver.extensions().list.is_some());
    assert!(driver.extensions().stream_upload.is_some());
    assert!(driver.extensions().multipart.is_some());
    assert!(driver.extensions().native_thumbnail.is_none());
    assert!(driver.extensions().native_media_metadata.is_none());
}

#[tokio::test]
async fn presigned_requests_use_sigv4_and_path_style() {
    let driver = QiniuDriver::new(config(), credentials()).expect("driver should build");
    let presigned = driver
        .presigned_put_request("reports/2026.txt", Duration::from_secs(60))
        .await
        .expect("presigning should succeed")
        .expect("S3 driver should create a request");

    assert!(
        presigned
            .url
            .starts_with("https://s3.example.test/archive/tenant-a/reports/2026.txt?")
    );
    assert!(presigned.url.contains("X-Amz-Algorithm=AWS4-HMAC-SHA256"));
    assert!(presigned.url.contains("X-Amz-Credential=access-key%2F"));
    assert!(presigned.url.contains("X-Amz-Signature="));
}

#[tokio::test]
async fn presigned_requests_support_virtual_hosted_style_when_endpoint_allows_it() {
    let mut config = config();
    config.path_style = false;
    let driver = QiniuDriver::new(config, credentials()).expect("driver should build");
    let presigned = driver
        .presigned_put_request("reports/2026.txt", Duration::from_secs(60))
        .await
        .expect("presigning should succeed")
        .expect("S3 driver should create a request");
    let url = url::Url::parse(&presigned.url).expect("presigned URL should parse");

    assert_eq!(url.host_str(), Some("archive.s3.example.test"));
    assert_eq!(url.path(), "/tenant-a/reports/2026.txt");
    assert!(
        url.query_pairs()
            .any(|(key, value)| { key == "X-Amz-Algorithm" && value == "AWS4-HMAC-SHA256" })
    );
}

#[tokio::test]
async fn multipart_part_presigning_uses_standard_s3_query_parameters() {
    let driver = QiniuDriver::new(config(), credentials()).expect("driver should build");
    let presigned = driver
        .presigned_upload_part_request("reports/2026.txt", "upload-id", 3, Duration::from_secs(60))
        .await
        .expect("part presigning should succeed");

    assert!(presigned.url.contains("partNumber=3"));
    assert!(presigned.url.contains("uploadId=upload-id"));
    assert!(presigned.url.contains("X-Amz-Signature="));
}
