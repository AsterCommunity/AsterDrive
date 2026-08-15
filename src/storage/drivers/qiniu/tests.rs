use std::time::Duration;

use aster_drive_storage::traits::extensions::PresignedStorageDriver;
use aster_drive_storage::traits::multipart::MultipartStorageDriver;
use aster_drive_storage::{StorageDriver, StorageErrorKind};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::{QiniuDriver, QiniuDriverConfig, QiniuStaticCredentials};

fn config() -> QiniuDriverConfig {
    QiniuDriverConfig {
        endpoint: "https://s3.cn-east-1.qiniucs.com".to_string(),
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

fn short_timeout_config(endpoint: String) -> QiniuDriverConfig {
    QiniuDriverConfig {
        endpoint,
        bucket: "archive".to_string(),
        base_path: "qiniu-error-contract".to_string(),
        region: "cn-east-1".to_string(),
        path_style: true,
        connect_timeout: Duration::from_millis(100),
        read_timeout: Duration::from_millis(100),
        operation_timeout: Duration::from_millis(300),
    }
}

async fn s3_error_endpoint(status: u16, code: &'static str) -> String {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("S3 error fixture should bind");
    let address = listener.local_addr().expect("S3 error fixture address");
    tokio::spawn(async move {
        let (mut stream, _) = listener
            .accept()
            .await
            .expect("S3 error fixture should accept");
        let mut request = Vec::new();
        let header_end = loop {
            let mut buffer = [0u8; 4096];
            let read = stream
                .read(&mut buffer)
                .await
                .expect("S3 error fixture should read request");
            if read == 0 {
                return;
            }
            request.extend_from_slice(&buffer[..read]);
            if let Some(offset) = request.windows(4).position(|window| window == b"\r\n\r\n") {
                break offset + 4;
            }
        };
        let headers = String::from_utf8_lossy(&request[..header_end]);
        let content_length = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().ok())
                    .flatten()
            })
            .unwrap_or_default();
        while request.len() - header_end < content_length {
            let mut buffer = [0u8; 4096];
            let read = stream
                .read(&mut buffer)
                .await
                .expect("S3 error fixture should read request body");
            if read == 0 {
                break;
            }
            request.extend_from_slice(&buffer[..read]);
        }
        let body = format!(
            "<Error><Code>{code}</Code><Message>fixture failure</Message><RequestId>qiniu-wrapper</RequestId></Error>"
        );
        let reason = match status {
            401 => "Unauthorized",
            403 => "Forbidden",
            404 => "Not Found",
            _ => "Error",
        };
        let response = format!(
            "HTTP/1.1 {status} {reason}\r\ncontent-type: application/xml\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
        stream
            .write_all(response.as_bytes())
            .await
            .expect("S3 error fixture should write response");
    });
    format!("http://{address}")
}

#[test]
fn validates_required_s3_compatible_connection_values() {
    assert!(QiniuDriver::validate_config(&config(), &credentials()).is_ok());

    let mut invalid_endpoint = config();
    invalid_endpoint.endpoint = "s3.cn-east-1.qiniucs.com".to_string();
    assert!(QiniuDriver::validate_config(&invalid_endpoint, &credentials()).is_err());

    let mut missing_endpoint = config();
    missing_endpoint.endpoint.clear();
    let error = QiniuDriver::validate_config(&missing_endpoint, &credentials())
        .expect_err("Qiniu endpoint is required");
    assert_eq!(error.kind(), StorageErrorKind::Misconfigured);

    let mut missing_bucket = config();
    missing_bucket.bucket.clear();
    let error = QiniuDriver::validate_config(&missing_bucket, &credentials())
        .expect_err("Qiniu S3 space name is required");
    assert_eq!(error.kind(), StorageErrorKind::Misconfigured);
    assert!(error.message().contains("Qiniu S3 space name is required"));

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

    assert!(std::sync::Arc::strong_count(&driver.s3_driver()) >= 2);

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
            .starts_with("https://s3.cn-east-1.qiniucs.com/archive/tenant-a/reports/2026.txt?")
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

    assert_eq!(url.host_str(), Some("archive.s3.cn-east-1.qiniucs.com"));
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

#[tokio::test]
async fn wrapper_preserves_shared_s3_service_error_classification() {
    for (status, code, expected_kind) in [
        (401, "InvalidAccessKeyId", StorageErrorKind::Auth),
        (403, "AccessDenied", StorageErrorKind::Permission),
        (404, "NoSuchKey", StorageErrorKind::NotFound),
    ] {
        let endpoint = s3_error_endpoint(status, code).await;
        let driver = QiniuDriver::new(short_timeout_config(endpoint), credentials())
            .expect("Qiniu wrapper should build");
        let error = driver
            .get("missing-object")
            .await
            .expect_err("fixture response should fail through Qiniu wrapper");
        assert_eq!(error.kind(), expected_kind);
        assert!(error.message().contains(code));
        assert!(error.message().contains("request_id=qiniu-wrapper"));
    }
}

#[tokio::test]
async fn wrapper_preserves_shared_s3_network_and_timeout_classification() {
    let unused =
        std::net::TcpListener::bind(("127.0.0.1", 0)).expect("unused endpoint should bind");
    let endpoint = format!(
        "http://{}",
        unused.local_addr().expect("unused endpoint address")
    );
    drop(unused);
    let driver = QiniuDriver::new(short_timeout_config(endpoint), credentials())
        .expect("Qiniu wrapper should build");
    let error = driver
        .put("network-failure", b"payload")
        .await
        .expect_err("closed endpoint should fail");
    assert_eq!(error.kind(), StorageErrorKind::Transient);

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("timeout fixture should bind");
    let endpoint = format!(
        "http://{}",
        listener.local_addr().expect("timeout fixture address")
    );
    let fixture = tokio::spawn(async move {
        let (_stream, _) = listener
            .accept()
            .await
            .expect("timeout fixture should accept");
        tokio::time::sleep(Duration::from_secs(2)).await;
    });
    let driver = QiniuDriver::new(short_timeout_config(endpoint), credentials())
        .expect("Qiniu wrapper should build");
    let error = driver
        .put("timeout", b"payload")
        .await
        .expect_err("hanging endpoint should time out");
    assert_eq!(error.kind(), StorageErrorKind::Transient);
    fixture.abort();
}
