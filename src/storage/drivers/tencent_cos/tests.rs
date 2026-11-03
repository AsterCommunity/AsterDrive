use super::native_thumbnail::is_cos_image_thumbnail_candidate;
use super::signing::cos_s3_endpoint;
use super::*;
use aster_drive_storage::traits::driver::StorageDriver;
use aster_drive_storage::traits::extensions::{
    DirectDownloadStorageDriver, NativeThumbnailRequest, NativeThumbnailStorageDriver,
};
use url::Url;

pub(super) fn sample_config(endpoint: &str, bucket: &str) -> TencentCosDriverConfig {
    TencentCosDriverConfig {
        download_delivery: Default::default(),
        cdn_auth_validity_seconds: 300,
        download_base_url: None,
        use_cname: false,
        endpoint: endpoint.to_string(),
        bucket: bucket.to_string(),
        base_path: "tenant/prefix".to_string(),
        connect_timeout: std::time::Duration::from_secs(5),
        read_timeout: std::time::Duration::from_secs(30),
        operation_timeout: std::time::Duration::from_secs(3_600),
    }
}

pub(super) fn sample_credentials() -> TencentCosStaticCredentials {
    TencentCosStaticCredentials {
        cdn_auth_key: None,
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
fn cos_s3_endpoint_strips_bucket_host() {
    let endpoint = cos_s3_endpoint(
        "https://bucket-1250000000.cos.ap-guangzhou.myqcloud.com",
        "bucket-1250000000",
        false,
    )
    .expect("COS S3 endpoint");

    assert_eq!(endpoint, "https://cos.ap-guangzhou.myqcloud.com");
}

#[test]
fn cos_s3_endpoint_keeps_root_host() {
    let endpoint = cos_s3_endpoint(
        "https://cos.ap-guangzhou.myqcloud.com",
        "bucket-1250000000",
        false,
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

    assert!(driver.extensions().direct_download.is_some());
    assert!(driver.extensions().presigned_upload.is_some());
    assert!(
        !driver
            .extensions()
            .presigned_upload
            .expect("presigned capability")
            .presigned_single_put_requires_etag()
    );
    assert!(driver.extensions().list.is_some());
    assert!(driver.extensions().stream_upload.is_some());
    assert!(driver.extensions().multipart.is_some());
    assert!(driver.extensions().native_thumbnail.is_some());
}

#[test]
fn cos_reports_upload_part_limits() {
    let driver = TencentCosDriver::new(
        sample_config("https://cos.ap-guangzhou.myqcloud.com", "bucket-1250000000"),
        sample_credentials(),
    )
    .expect("COS driver");
    let capability = driver
        .extensions()
        .multipart
        .expect("multipart capability")
        .capabilities();
    assert_eq!(capability.min_part_size, 1024 * 1024);
    assert_eq!(capability.max_part_size, Some(5 * 1024 * 1024 * 1024));
    assert_eq!(capability.max_parts, 10_000);
}

#[tokio::test]
async fn custom_download_base_url_builds_and_signs_the_final_host() {
    let mut config = sample_config("https://cos.ap-guangzhou.myqcloud.com", "bucket-1250000000");
    config.download_base_url = Some("https://cdn.example.test/assets".to_string());
    config.download_delivery = super::CosDownloadDelivery::CustomOrigin;
    let driver = TencentCosDriver::new(config, sample_credentials()).expect("COS driver");

    let request = driver
        .resolve_download_url(
            "docs/报告 +#%.txt",
            std::time::Duration::from_secs(300),
            aster_drive_storage::DirectDownloadOptions {
                response_content_disposition: Some(
                    "attachment; filename*=UTF-8''report.txt".to_string(),
                ),
                ..Default::default()
            },
        )
        .await
        .expect("custom download URL")
        .expect("direct download request");
    let url = Url::parse(&request.url).expect("valid custom download URL");
    let query = url
        .query_pairs()
        .into_owned()
        .collect::<std::collections::HashMap<_, _>>();

    assert_eq!(url.host_str(), Some("cdn.example.test"));
    assert_eq!(
        url.path(),
        "/assets/tenant/prefix/docs/%E6%8A%A5%E5%91%8A%20+%23%25.txt"
    );
    assert_eq!(query.get("q-header-list").map(String::as_str), Some("host"));
    assert_eq!(
        query.get("q-sign-algorithm").map(String::as_str),
        Some("sha1")
    );
    assert_eq!(
        query
            .get("response-content-disposition")
            .map(String::as_str),
        Some("attachment; filename*=UTF-8''report.txt")
    );
    assert!(query.contains_key("q-signature"));
    assert!(request.expires_at.is_some());
}

#[test]
fn cname_endpoint_accepts_custom_host_and_preserves_bucket_like_label() {
    let mut config = sample_config(
        "https://bucket-1250000000.files.example.test",
        "bucket-1250000000",
    );
    config.use_cname = true;
    TencentCosDriver::validate_config(&config, &sample_credentials()).expect("CNAME config");
    assert_eq!(
        cos_s3_endpoint(&config.endpoint, &config.bucket, true).unwrap(),
        config.endpoint
    );
}

#[test]
fn cname_endpoint_rejects_ambiguous_url_components() {
    for endpoint in [
        "https://user:pass@files.example.test",
        "https://files.example.test?query=value",
        "https://files.example.test#fragment",
        "https://files.example.test/prefix",
        "https://files.example.test/prefix/",
        "https://files.example.test\\prefix",
        "https://files.example.test/\n",
        "ftp://files.example.test",
    ] {
        let mut config = sample_config(endpoint, "bucket-1250000000");
        config.use_cname = true;
        assert!(
            TencentCosDriver::validate_config(&config, &sample_credentials()).is_err(),
            "accepted {endpoint:?}"
        );
    }
}

#[tokio::test]
async fn cname_driver_presigns_final_host_and_encoded_object_key() {
    use aster_drive_storage::MultipartStorageDriver;
    let mut config = sample_config("https://files.example.test:9443", "bucket-1250000000");
    config.use_cname = true;
    let driver = TencentCosDriver::new(config, sample_credentials()).unwrap();
    let key = "目录/a b+%2F#?.txt";
    let ttl = Duration::from_secs(300);
    let download = driver
        .resolve_download_url(key, ttl, DirectDownloadOptions::default())
        .await
        .unwrap()
        .unwrap();
    let upload = driver
        .presigned_put_request(key, ttl)
        .await
        .unwrap()
        .unwrap();
    let part = driver
        .presigned_upload_part_request(key, "upload+/%id", 7, ttl)
        .await
        .unwrap();
    for request_url in [&download.url, &upload.url, &part.url] {
        let url = Url::parse(request_url).unwrap();
        assert_eq!(url.host_str(), Some("files.example.test"));
        assert_eq!(url.port(), Some(9443));
        assert_eq!(
            percent_encoding::percent_decode_str(url.path())
                .decode_utf8()
                .unwrap(),
            format!("/tenant/prefix/{key}")
        );
        assert_eq!(query_value(&url, "q-header-list").as_deref(), Some("host"));
        assert!(query_value(&url, "q-signature").is_some());
        assert!(!request_url.contains("X-Amz-"));
    }
    let part_url = Url::parse(&part.url).unwrap();
    assert_eq!(
        query_value(&part_url, "uploadId").as_deref(),
        Some("upload+/%id")
    );
    assert_eq!(query_value(&part_url, "partNumber").as_deref(), Some("7"));
}

#[test]
fn cname_native_processing_and_cors_use_bucket_bound_host() {
    let mut config = sample_config("https://files.example.test", "bucket-1250000000");
    config.use_cname = true;
    let driver = TencentCosDriver::new(config, sample_credentials()).unwrap();
    let key = "目录/a b+%2F#?.png";
    for signed in [
        driver.signed_ci_thumbnail_url(key, 320, 240).unwrap(),
        driver.signed_ci_media_info_url(key).unwrap(),
    ] {
        let url = Url::parse(&signed).unwrap();
        assert_eq!(url.host_str(), Some("files.example.test"));
        assert_eq!(
            percent_encoding::percent_decode_str(url.path())
                .decode_utf8()
                .unwrap(),
            format!("/tenant/prefix/{key}")
        );
        assert!(
            query_value(&url, "sign")
                .unwrap()
                .contains("q-header-list=host")
        );
    }
    let url = driver.bucket_cors_url().unwrap();
    assert_eq!(url.as_str(), "https://files.example.test/?cors");
    let headers = driver
        .signed_cos_request_headers("PUT", &url, &[], "1700000000;1700000600")
        .unwrap();
    assert!(
        headers[reqwest::header::AUTHORIZATION]
            .to_str()
            .unwrap()
            .contains("q-url-param-list=cors")
    );
}

#[tokio::test]
async fn default_origin_ignores_dormant_delivery_url_and_custom_origin_requires_one() {
    let mut config = sample_config("https://cos.ap-guangzhou.myqcloud.com", "bucket-1250000000");
    config.download_base_url = Some("https://download.example.test/assets".into());
    let driver = TencentCosDriver::new(config.clone(), sample_credentials()).unwrap();
    let result = driver
        .resolve_download_url("file.txt", Duration::from_secs(60), Default::default())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        Url::parse(&result.url).unwrap().host_str(),
        Some("bucket-1250000000.cos.ap-guangzhou.myqcloud.com")
    );
    config.download_delivery = super::CosDownloadDelivery::CustomOrigin;
    config.download_base_url = None;
    assert!(TencentCosDriver::new(config.clone(), sample_credentials()).is_err());
    config.download_base_url = Some("".into());
    assert!(TencentCosDriver::new(config, sample_credentials()).is_err());
}

#[tokio::test]
async fn custom_download_origin_keeps_upload_and_multipart_on_storage_endpoint() {
    use aster_drive_storage::MultipartStorageDriver;
    let mut config = sample_config("https://cos.ap-guangzhou.myqcloud.com", "bucket-1250000000");
    config.download_delivery = super::CosDownloadDelivery::CustomOrigin;
    config.download_base_url = Some("https://download.example.test/assets".into());
    let driver = TencentCosDriver::new(config, sample_credentials()).unwrap();
    for request in [
        driver
            .presigned_put_request("file", Duration::from_secs(60))
            .await
            .unwrap()
            .unwrap(),
        driver
            .presigned_upload_part_request("file", "upload-id", 1, Duration::from_secs(60))
            .await
            .unwrap(),
    ] {
        assert_eq!(
            Url::parse(&request.url).unwrap().host_str(),
            Some("bucket-1250000000.cos.ap-guangzhou.myqcloud.com")
        );
    }
}
