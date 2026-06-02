use super::S3Driver;
use super::S3DriverOptions;
use super::presigned::{MAX_PRESIGN_TTL, clamp_presign_ttl};
use crate::entities::storage_policy;
use crate::errors::AsterError;
use crate::storage::error::StorageErrorKind;
use crate::storage::traits::driver::{StorageDriver, StoragePathVisitor};
use crate::storage::traits::extensions::{ListStorageDriver, PresignedStorageDriver};
use crate::storage::traits::multipart::MultipartStorageDriver;
use crate::types::{StoragePolicyOptions, serialize_storage_policy_options};
use aws_sdk_s3::config::{BehaviorVersion, Credentials, Region};
use aws_smithy_http_client::test_util::{ReplayEvent, StaticReplayClient, capture_request};
use aws_smithy_types::body::SdkBody;
use bytes::Bytes;
use std::time::Duration;

fn mocked_driver(
    response: http::Response<SdkBody>,
) -> (
    S3Driver,
    aws_smithy_http_client::test_util::CaptureRequestReceiver,
) {
    let (http_client, request) = capture_request(Some(response));
    let config = aws_sdk_s3::Config::builder()
        .behavior_version(BehaviorVersion::latest())
        .http_client(http_client)
        .credentials_provider(Credentials::new(
            "test-access-key",
            "test-secret-key",
            None,
            None,
            "s3-unit-test",
        ))
        .region(Region::new("us-east-1"))
        .build();

    (
        S3Driver {
            client: aws_sdk_s3::Client::from_conf(config),
            bucket: "test-bucket".to_string(),
            base_path: String::new(),
        },
        request,
    )
}

fn replay_driver(events: Vec<ReplayEvent>, base_path: &str) -> (S3Driver, StaticReplayClient) {
    let http_client = StaticReplayClient::new(events);
    let config = aws_sdk_s3::Config::builder()
        .behavior_version(BehaviorVersion::latest())
        .http_client(http_client.clone())
        .credentials_provider(Credentials::new(
            "test-access-key",
            "test-secret-key",
            None,
            None,
            "s3-unit-test",
        ))
        .region(Region::new("us-east-1"))
        .build();

    (
        S3Driver {
            client: aws_sdk_s3::Client::from_conf(config),
            bucket: "test-bucket".to_string(),
            base_path: base_path.to_string(),
        },
        http_client,
    )
}

fn replay_event(body: &str) -> ReplayEvent {
    ReplayEvent::new(
        http::Request::builder()
            .uri("https://example.invalid/")
            .body(SdkBody::empty())
            .expect("expected request"),
        http::Response::builder()
            .status(200)
            .header("content-type", "application/xml")
            .body(SdkBody::from(body.to_string()))
            .expect("mocked response"),
    )
}

#[tokio::test]
async fn capacity_info_returns_unsupported_without_s3_request() {
    let response = http::Response::builder()
        .status(200)
        .body(SdkBody::empty())
        .expect("unused mocked response");
    let (driver, request) = mocked_driver(response);

    let err = driver
        .capacity_info()
        .await
        .expect_err("S3 capacity should be explicitly unsupported");

    assert_eq!(
        err.storage_error_kind(),
        Some(StorageErrorKind::Unsupported)
    );
    assert!(err.message().contains("does not expose standardized"));
    request.expect_no_request();
}

fn assert_storage_driver_error(err: AsterError, expected_kind: StorageErrorKind) {
    assert_eq!(err.code(), "E031");
    assert_eq!(err.storage_error_kind(), Some(expected_kind));
    assert!(
        err.message().contains("http_status=404"),
        "expected raw HTTP status in '{}'",
        err.message()
    );
    assert!(
        err.message().contains("code=NoSuchBucket"),
        "expected S3 error code in '{}'",
        err.message()
    );
    assert!(
        err.message()
            .contains("message=The specified bucket does not exist"),
        "expected S3 error message in '{}'",
        err.message()
    );
    assert!(
        err.message().contains("request_id=req-123"),
        "expected S3 request_id in '{}'",
        err.message()
    );
    assert!(
        err.message().contains("extended_request_id=ext-456"),
        "expected S3 extended_request_id in '{}'",
        err.message()
    );
}

fn sample_policy(endpoint: &str, bucket: &str) -> storage_policy::Model {
    storage_policy::Model {
        id: 1,
        name: "S3".to_string(),
        driver_type: crate::types::DriverType::S3,
        endpoint: endpoint.to_string(),
        bucket: bucket.to_string(),
        access_key: "key".to_string(),
        secret_key: "secret".to_string(),
        base_path: String::new(),
        remote_node_id: None,
        max_file_size: 0,
        allowed_types: crate::types::StoredStoragePolicyAllowedTypes::empty(),
        options: crate::types::StoredStoragePolicyOptions::empty(),
        is_default: false,
        chunk_size: 0,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }
}

#[test]
fn new_normalizes_r2_bucket_path() {
    let driver = S3Driver::new(&sample_policy(
        "https://demo-account.r2.cloudflarestorage.com/photos",
        "",
    ))
    .expect("normalized R2 driver");

    assert_eq!(driver.bucket, "photos");
}

#[test]
fn new_maps_r2_validation_errors_to_storage_driver_errors() {
    let err = match S3Driver::new(&sample_policy("https://pub-demo.r2.dev", "photos")) {
        Ok(_) => panic!("public R2 endpoint should fail"),
        Err(err) => err,
    };

    assert_eq!(err.code(), "E031");
    assert!(
        err.message().contains("Cloudflare R2 endpoint"),
        "expected R2 validation context in '{}'",
        err.message()
    );
}

#[test]
fn new_applies_timeout_config_from_policy_options() {
    let mut policy = sample_policy("https://s3.example.test", "bucket");
    policy.options = serialize_storage_policy_options(&StoragePolicyOptions {
        s3_connect_timeout_secs: Some(9),
        s3_read_timeout_secs: Some(45),
        s3_operation_timeout_secs: Some(1_200),
        ..Default::default()
    })
    .expect("options should serialize");

    let driver = S3Driver::new(&policy).expect("driver should build with timeout config");
    let timeout_config = driver
        .client
        .config()
        .timeout_config()
        .expect("timeout config should be present");

    assert_eq!(
        timeout_config.connect_timeout(),
        Some(Duration::from_secs(9))
    );
    assert_eq!(timeout_config.read_timeout(), Some(Duration::from_secs(45)));
    assert_eq!(
        timeout_config.operation_timeout(),
        Some(Duration::from_secs(1_200))
    );
}

#[tokio::test]
async fn presigned_put_url_uses_configured_addressing_style() {
    let path_style_driver = S3Driver::new(&sample_policy("https://s3.example.test", "bucket"))
        .expect("path-style driver");
    let virtual_hosted_driver = S3Driver::new_with_options(
        &sample_policy("https://s3.example.test", "bucket"),
        S3DriverOptions::virtual_hosted_style(),
    )
    .expect("virtual-hosted driver");

    let path_style = path_style_driver
        .presigned_put_url("folder/file.txt", Duration::from_secs(60))
        .await
        .expect("path-style presign")
        .expect("path-style URL");
    let virtual_hosted = virtual_hosted_driver
        .presigned_put_url("folder/file.txt", Duration::from_secs(60))
        .await
        .expect("virtual-hosted presign")
        .expect("virtual-hosted URL");

    let path_style = url::Url::parse(&path_style).expect("path-style URL parse");
    let virtual_hosted = url::Url::parse(&virtual_hosted).expect("virtual-hosted URL parse");

    assert_eq!(path_style.host_str(), Some("s3.example.test"));
    assert_eq!(path_style.path(), "/bucket/folder/file.txt");
    assert_eq!(virtual_hosted.host_str(), Some("bucket.s3.example.test"));
    assert_eq!(virtual_hosted.path(), "/folder/file.txt");
}

#[tokio::test]
async fn put_surfaces_s3_service_error_details() {
    let response = http::Response::builder()
        .status(404)
        .header("x-amz-request-id", "req-123")
        .header("x-amz-id-2", "ext-456")
        .body(SdkBody::from(
            r#"<?xml version="1.0" encoding="UTF-8"?>
                <Error>
                    <Code>NoSuchBucket</Code>
                    <Message>The specified bucket does not exist</Message>
                    <RequestId>ignored-in-body</RequestId>
                </Error>"#,
        ))
        .expect("mocked response");
    let (driver, request) = mocked_driver(response);

    let err = driver.put("foo.txt", b"hello").await.unwrap_err();
    request.expect_request();

    assert_storage_driver_error(err, StorageErrorKind::Misconfigured);
}

#[tokio::test]
async fn put_surfaces_raw_http_error_when_metadata_missing() {
    let response = http::Response::builder()
        .status(403)
        .header("content-type", "text/plain")
        .body(SdkBody::from("upstream denied this request"))
        .expect("mocked response");
    let (driver, request) = mocked_driver(response);

    let err = driver.put("foo.txt", b"hello").await.unwrap_err();
    request.expect_request();

    assert_eq!(err.code(), "E031");
    assert!(
        err.message().contains("http_status=403"),
        "expected raw HTTP status in '{}'",
        err.message()
    );
    assert!(
        err.message().contains("content_type=text/plain"),
        "expected content type in '{}'",
        err.message()
    );
    assert!(
        err.message()
            .contains("raw_body=upstream denied this request"),
        "expected raw body preview in '{}'",
        err.message()
    );
    assert_eq!(err.storage_error_kind(), Some(StorageErrorKind::Permission));
}

#[tokio::test]
async fn abort_multipart_upload_maps_no_such_upload_to_not_found() {
    let response = http::Response::builder()
        .status(404)
        .header("x-amz-request-id", "req-404")
        .body(SdkBody::from(
            r#"<?xml version="1.0" encoding="UTF-8"?>
                <Error>
                    <Code>NoSuchUpload</Code>
                    <Message>The specified multipart upload does not exist</Message>
                </Error>"#,
        ))
        .expect("mocked response");
    let (driver, request) = mocked_driver(response);

    let err = driver
        .abort_multipart_upload("foo.txt", "upload-1")
        .await
        .unwrap_err();
    request.expect_request();

    assert_eq!(err.code(), "E031");
    assert_eq!(err.storage_error_kind(), Some(StorageErrorKind::NotFound));
}

#[tokio::test]
async fn copy_object_url_encodes_source_key() {
    let response = http::Response::builder()
        .status(200)
        .body(SdkBody::from(
            r#"<?xml version="1.0" encoding="UTF-8"?>
                <CopyObjectResult><ETag>"abc"</ETag></CopyObjectResult>"#,
        ))
        .expect("mocked response");
    let (driver, request) = mocked_driver(response);

    driver
        .copy_object("folder with space/中文 file+1.txt", "dest/key")
        .await
        .expect("copy should succeed");

    let captured = request.expect_request();
    let copy_source = captured
        .headers()
        .get("x-amz-copy-source")
        .expect("copy-source header");
    // 空格 → %20，中文 → UTF-8 percent-encoded，`+` → %2B
    assert!(
        copy_source.contains("%20"),
        "expected space encoded in '{copy_source}'"
    );
    assert!(
        copy_source.contains("%2B"),
        "expected '+' encoded in '{copy_source}'"
    );
    assert!(
        !copy_source.contains(' '),
        "raw space should not remain in '{copy_source}'"
    );
    // bucket 与 key 之间的 `/` 必须保留为分隔符
    assert!(
        copy_source.starts_with("test-bucket/"),
        "bucket prefix missing in '{copy_source}'"
    );
}

#[tokio::test]
async fn get_range_sends_native_range_header() {
    let response = http::Response::builder()
        .status(206)
        .body(SdkBody::from("world"))
        .expect("mocked response");
    let (driver, request) = mocked_driver(response);

    driver
        .get_range("obj", 7, Some(5))
        .await
        .expect("range should succeed");

    let captured = request.expect_request();
    let range = captured
        .headers()
        .get("range")
        .expect("Range header must be sent");
    // HTTP Range 闭区间，7..=11
    assert_eq!(range, "bytes=7-11");
}

#[tokio::test]
async fn get_range_without_length_sends_open_ended_range() {
    let response = http::Response::builder()
        .status(206)
        .body(SdkBody::from("tail"))
        .expect("mocked response");
    let (driver, request) = mocked_driver(response);

    driver
        .get_range("obj", 100, None)
        .await
        .expect("open-ended range should succeed");

    let captured = request.expect_request();
    let range = captured.headers().get("range").expect("Range header");
    assert_eq!(range, "bytes=100-");
}

#[test]
fn clamp_presign_ttl_caps_at_max() {
    let clamped = clamp_presign_ttl(std::time::Duration::from_secs(7 * 24 * 3600), "t");
    assert_eq!(clamped, MAX_PRESIGN_TTL);
}

#[test]
fn clamp_presign_ttl_passes_through_when_in_range() {
    let req = std::time::Duration::from_secs(60);
    assert_eq!(clamp_presign_ttl(req, "t"), req);
}

#[test]
fn clamp_presign_ttl_replaces_zero_with_max() {
    let clamped = clamp_presign_ttl(std::time::Duration::ZERO, "t");
    assert_eq!(clamped, MAX_PRESIGN_TTL);
}

#[tokio::test]
async fn list_paths_paginates_strips_base_path_and_sorts_results() {
    let (driver, http_client) = replay_driver(
        vec![
            replay_event(
                r#"<?xml version="1.0" encoding="UTF-8"?>
                <ListBucketResult>
                    <IsTruncated>true</IsTruncated>
                    <Contents><Key>base/files/b.txt</Key></Contents>
                    <Contents><Key>base/files/a.txt</Key></Contents>
                    <Contents><Key>baseball/files/ignored.txt</Key></Contents>
                    <Contents><Size>123</Size></Contents>
                    <NextContinuationToken>token-1</NextContinuationToken>
                </ListBucketResult>"#,
            ),
            replay_event(
                r#"<?xml version="1.0" encoding="UTF-8"?>
                <ListBucketResult>
                    <IsTruncated>false</IsTruncated>
                    <Contents><Key>base/files/c.txt</Key></Contents>
                </ListBucketResult>"#,
            ),
        ],
        "base",
    );

    let paths = driver
        .list_paths(Some("files/"))
        .await
        .expect("list should succeed");

    assert_eq!(
        paths,
        vec![
            "files/a.txt".to_string(),
            "files/b.txt".to_string(),
            "files/c.txt".to_string()
        ]
    );

    let requests = http_client
        .actual_requests()
        .map(|request| request.uri().to_string())
        .collect::<Vec<_>>();
    assert_eq!(requests.len(), 2);
    assert!(
        requests[0].contains("list-type=2"),
        "expected list_objects_v2 query in '{}'",
        requests[0]
    );
    assert!(
        requests[0].contains("prefix=base%2Ffiles%2F"),
        "expected scoped prefix in '{}'",
        requests[0]
    );
    assert!(
        requests[1].contains("continuation-token=token-1"),
        "expected continuation token in '{}'",
        requests[1]
    );
}

struct CollectingVisitor {
    paths: Vec<String>,
}

impl StoragePathVisitor for CollectingVisitor {
    fn visit_path(&mut self, path: String) -> crate::errors::Result<()> {
        self.paths.push(path);
        Ok(())
    }
}

#[tokio::test]
async fn scan_paths_streams_matching_paths_without_sorting() {
    let (driver, _http_client) = replay_driver(
        vec![replay_event(
            r#"<?xml version="1.0" encoding="UTF-8"?>
            <ListBucketResult>
                <IsTruncated>false</IsTruncated>
                <Contents><Key>base/z.txt</Key></Contents>
                <Contents><Key>other/ignored.txt</Key></Contents>
                <Contents><Key>base/a.txt</Key></Contents>
            </ListBucketResult>"#,
        )],
        "base/",
    );
    let mut visitor = CollectingVisitor { paths: Vec::new() };

    driver
        .scan_paths(None, &mut visitor)
        .await
        .expect("scan should succeed");

    assert_eq!(
        visitor.paths,
        vec!["z.txt".to_string(), "a.txt".to_string()]
    );
}

#[tokio::test]
async fn presigned_url_includes_download_response_overrides() {
    let response = http::Response::builder()
        .status(200)
        .body(SdkBody::empty())
        .expect("mocked response");
    let (mut driver, _request) = mocked_driver(response);
    driver.base_path = "base".to_string();

    let url = driver
        .presigned_url(
            "folder/file name.txt",
            Duration::from_secs(60),
            crate::storage::traits::driver::PresignedDownloadOptions {
                response_cache_control: Some("private, max-age=60".to_string()),
                response_content_disposition: Some(
                    "attachment; filename=\"file name.txt\"".to_string(),
                ),
                response_content_type: Some("text/plain".to_string()),
            },
        )
        .await
        .expect("presigned URL should build")
        .expect("S3 should return a URL");
    let parsed = reqwest::Url::parse(&url).expect("presigned URL should parse");
    let query = parsed
        .query_pairs()
        .into_owned()
        .collect::<std::collections::HashMap<_, _>>();

    assert!(
        parsed.path().contains("/base/folder/file%20name.txt"),
        "expected base path and encoded object key in '{}'",
        parsed.path()
    );
    assert_eq!(
        query.get("response-cache-control").map(String::as_str),
        Some("private, max-age=60")
    );
    assert_eq!(
        query
            .get("response-content-disposition")
            .map(String::as_str),
        Some("attachment; filename=\"file name.txt\"")
    );
    assert_eq!(
        query.get("response-content-type").map(String::as_str),
        Some("text/plain")
    );
}

#[tokio::test]
async fn presigned_put_url_includes_base_path() {
    let response = http::Response::builder()
        .status(200)
        .body(SdkBody::empty())
        .expect("mocked response");
    let (mut driver, _request) = mocked_driver(response);
    driver.base_path = "base".to_string();

    let url = driver
        .presigned_put_url("upload.bin", Duration::from_secs(60))
        .await
        .expect("presigned PUT URL should build")
        .expect("S3 should return a URL");
    let parsed = reqwest::Url::parse(&url).expect("presigned URL should parse");

    assert!(
        parsed.path().contains("/base/upload.bin"),
        "expected base path in '{}'",
        parsed.path()
    );
    assert!(
        parsed
            .query_pairs()
            .any(|(key, value)| key == "X-Amz-Algorithm" && value == "AWS4-HMAC-SHA256"),
        "expected SigV4 query parameters in '{url}'"
    );
}

#[tokio::test]
async fn create_multipart_upload_returns_upload_id() {
    let response = http::Response::builder()
        .status(200)
        .body(SdkBody::from(
            r#"<?xml version="1.0" encoding="UTF-8"?>
                <InitiateMultipartUploadResult>
                    <Bucket>test-bucket</Bucket>
                    <Key>base/object.bin</Key>
                    <UploadId>upload-123</UploadId>
                </InitiateMultipartUploadResult>"#,
        ))
        .expect("mocked response");
    let (mut driver, request) = mocked_driver(response);
    driver.base_path = "base".to_string();

    let upload_id = driver
        .create_multipart_upload("object.bin")
        .await
        .expect("multipart upload should start");

    assert_eq!(upload_id, "upload-123");
    let captured = request.expect_request();
    let uri = captured.uri().to_string();
    assert!(
        uri.contains("/base/object.bin"),
        "expected base path in '{uri}'"
    );
    assert!(
        uri.contains("uploads"),
        "expected multipart uploads query in '{uri}'"
    );
}

#[tokio::test]
async fn create_multipart_upload_rejects_missing_upload_id() {
    let response = http::Response::builder()
        .status(200)
        .body(SdkBody::from(
            r#"<?xml version="1.0" encoding="UTF-8"?>
                <InitiateMultipartUploadResult>
                    <Bucket>test-bucket</Bucket>
                    <Key>object.bin</Key>
                </InitiateMultipartUploadResult>"#,
        ))
        .expect("mocked response");
    let (driver, request) = mocked_driver(response);

    let err = driver
        .create_multipart_upload("object.bin")
        .await
        .expect_err("missing upload_id should fail");
    request.expect_request();

    assert_eq!(err.storage_error_kind(), Some(StorageErrorKind::Unknown));
    assert!(err.message().contains("missing upload_id"));
}

#[tokio::test]
async fn presigned_upload_part_url_includes_upload_context() {
    let response = http::Response::builder()
        .status(200)
        .body(SdkBody::empty())
        .expect("mocked response");
    let (mut driver, _request) = mocked_driver(response);
    driver.base_path = "base".to_string();

    let url = driver
        .presigned_upload_part_url("object.bin", "upload-123", 7, Duration::from_secs(60))
        .await
        .expect("presigned part URL should build");
    let parsed = reqwest::Url::parse(&url).expect("presigned URL should parse");
    let query = parsed
        .query_pairs()
        .into_owned()
        .collect::<std::collections::HashMap<_, _>>();

    assert!(
        parsed.path().contains("/base/object.bin"),
        "expected base path in '{}'",
        parsed.path()
    );
    assert_eq!(
        query.get("uploadId").map(String::as_str),
        Some("upload-123")
    );
    assert_eq!(query.get("partNumber").map(String::as_str), Some("7"));
}

#[tokio::test]
async fn upload_multipart_part_rejects_missing_etag() {
    let response = http::Response::builder()
        .status(200)
        .body(SdkBody::from(
            r#"<?xml version="1.0" encoding="UTF-8"?>
                <UploadPartResult />"#,
        ))
        .expect("mocked response");
    let (driver, request) = mocked_driver(response);

    let err = driver
        .upload_multipart_part("object.bin", "upload-123", 1, b"part")
        .await
        .expect_err("missing ETag should fail");
    request.expect_request();

    assert_eq!(err.storage_error_kind(), Some(StorageErrorKind::Unknown));
    assert!(err.message().contains("missing ETag"));
}

#[tokio::test]
async fn upload_multipart_part_bytes_sends_request_body_without_slice_vec_roundtrip() {
    let response = http::Response::builder()
        .status(200)
        .header("ETag", "\"part-etag\"")
        .body(SdkBody::from(
            r#"<?xml version="1.0" encoding="UTF-8"?>
                <UploadPartResult><ETag>"part-etag"</ETag></UploadPartResult>"#,
        ))
        .expect("mocked response");
    let (driver, request) = mocked_driver(response);

    let etag = driver
        .upload_multipart_part_bytes(
            "object.bin",
            "upload-123",
            3,
            Bytes::from_static(b"relay-part"),
        )
        .await
        .expect("multipart part should upload");

    let captured = request.expect_request();
    assert_eq!(etag, "\"part-etag\"");
    let uri = captured.uri().to_string();
    assert!(
        uri.contains("partNumber=3"),
        "expected partNumber in '{uri}'"
    );
    assert!(
        uri.contains("uploadId=upload-123"),
        "expected uploadId in '{uri}'"
    );
    assert_eq!(
        captured.body().bytes(),
        Some(&b"relay-part"[..]),
        "request body should be the provided Bytes payload"
    );
}

#[tokio::test]
async fn list_uploaded_parts_paginates_with_next_part_marker() {
    let (driver, http_client) = replay_driver(
        vec![
            replay_event(
                r#"<?xml version="1.0" encoding="UTF-8"?>
                <ListPartsResult>
                    <IsTruncated>true</IsTruncated>
                    <NextPartNumberMarker>2</NextPartNumberMarker>
                    <Part><PartNumber>1</PartNumber><ETag>"etag-1"</ETag></Part>
                    <Part><PartNumber>2</PartNumber><ETag>"etag-2"</ETag></Part>
                </ListPartsResult>"#,
            ),
            replay_event(
                r#"<?xml version="1.0" encoding="UTF-8"?>
                <ListPartsResult>
                    <IsTruncated>false</IsTruncated>
                    <Part><PartNumber>3</PartNumber><ETag>"etag-3"</ETag></Part>
                </ListPartsResult>"#,
            ),
        ],
        "base",
    );

    let parts = driver
        .list_uploaded_parts("object.bin", "upload-123")
        .await
        .expect("parts should list");

    assert_eq!(parts, vec![1, 2, 3]);
    let requests = http_client
        .actual_requests()
        .map(|request| request.uri().to_string())
        .collect::<Vec<_>>();
    assert_eq!(requests.len(), 2);
    assert!(
        requests[0].contains("uploadId=upload-123"),
        "expected upload id in '{}'",
        requests[0]
    );
    assert!(
        requests[1].contains("part-number-marker=2"),
        "expected pagination marker in '{}'",
        requests[1]
    );
}
