use std::time::Duration;

use aster_drive_storage::StorageDriver;

use super::cdn::OssDownloadDelivery;
use super::*;

pub(super) fn sample_config() -> AlibabaOssDriverConfig {
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
        download_delivery: OssDownloadDelivery::OssOrigin,
        cdn_domain: None,
        cdn_auth_validity_seconds: 300,
    }
}

pub(super) fn sample_credentials() -> AlibabaOssStaticCredentials {
    AlibabaOssStaticCredentials {
        access_key: "ak".to_string(),
        secret_key: "sk".to_string(),
        cdn_auth_key: Some("TestCdnKey123".to_string()),
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

    config.endpoint = "https://oss-accelerate-overseas.aliyuncs.com".to_string();
    AlibabaOssDriver::validate_config(&config, &sample_credentials())
        .expect("overseas accelerate endpoint should not be interpreted as a region");

    config.endpoint = "https://oss-cn-hangzhou-internal.aliyuncs.com".to_string();
    config.region = "cn-hangzhou".to_string();
    AlibabaOssDriver::validate_config(&config, &sample_credentials())
        .expect("internal endpoint region should match after suffix removal");

    config.endpoint = "https://oss-cn-hangzhou.aliyuncs.com".to_string();
    config.region = "CN-HANGZHOU".to_string();
    AlibabaOssDriver::validate_config(&config, &sample_credentials())
        .expect("official OSS region comparison should be case-insensitive");
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
    assert!(extensions.direct_download.is_some());
    assert!(extensions.presigned_upload.is_some());
    assert!(extensions.list.is_some());
    assert!(extensions.stream_upload.is_some());
    assert!(extensions.multipart.is_some());
    let multipart = extensions.multipart.expect("multipart extension");
    assert_eq!(multipart.capabilities().min_part_size, 100 * 1024);
    assert_eq!(multipart.capabilities().max_parts, 10_000);
}

#[tokio::test]
async fn public_presigned_url_ignores_server_side_endpoint() {
    let mut config = sample_config();
    config.server_side_endpoint = "https://oss-cn-hangzhou-internal.aliyuncs.com".to_string();
    let driver =
        AlibabaOssDriver::new(config, sample_credentials()).expect("OSS driver should build");

    let presigned = driver
        .extensions()
        .presigned_upload
        .expect("presigned upload extension");
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
        .presigned_upload
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
async fn cname_presigned_requests_preserve_encoded_keys_and_final_domain() {
    let mut config = sample_config();
    config.endpoint = "https://asterdrive-test.files.example.test:9443".to_string();
    config.use_cname = true;
    let driver = AlibabaOssDriver::new(config, sample_credentials()).unwrap();
    let key = "目录/a b+%2F#?.txt";
    let ttl = Duration::from_secs(300);
    let download = driver
        .resolve_download_url(key, ttl, Default::default())
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
    for request in [&download.url, &upload.url, &part.url] {
        let url = Url::parse(request).unwrap();
        assert_eq!(url.host_str(), Some("asterdrive-test.files.example.test"));
        assert_eq!(url.port(), Some(9443));
        assert_eq!(
            percent_encoding::percent_decode_str(url.path())
                .decode_utf8()
                .unwrap(),
            format!("/tenant/prefix/{key}")
        );
        assert!(url.query_pairs().any(|(key, _)| key == "x-oss-signature"));
        assert!(!request.contains("X-Amz-"));
    }
    let part_url = Url::parse(&part.url).unwrap();
    let query = part_url
        .query_pairs()
        .collect::<std::collections::HashMap<_, _>>();
    assert_eq!(query.get("uploadId").unwrap(), "upload+/%id");
    assert_eq!(query.get("partNumber").unwrap(), "7");
}

#[tokio::test]
async fn public_presigned_get_omits_unsupported_content_type_override() {
    let mut config = sample_config();
    config.server_side_endpoint = "https://oss-cn-hangzhou-internal.aliyuncs.com".to_string();
    let driver =
        AlibabaOssDriver::new(config, sample_credentials()).expect("OSS driver should build");

    let url = driver
        .extensions()
        .direct_download
        .expect("presigned extension")
        .resolve_download_url(
            "files/photo.jpg",
            Duration::from_secs(300),
            DirectDownloadOptions {
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
    let parsed = Url::parse(&url.url).expect("valid OSS presigned URL");
    let raw_query = parsed.query().expect("presigned GET query");
    let query = parsed
        .query_pairs()
        .into_owned()
        .collect::<std::collections::HashMap<_, _>>();

    assert_eq!(
        parsed.host_str(),
        Some("asterdrive-test.oss-cn-hangzhou.aliyuncs.com")
    );
    assert!(!url.url.contains("internal"));
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

#[tokio::test]
async fn cdn_download_url_with_type_a_auth() {
    let mut config = sample_config();
    config.cdn_domain = Some(" https://cdn.example.com/ ".into());
    config.download_delivery = OssDownloadDelivery::AlibabaCdnTypeA;
    let driver = AlibabaOssDriver::new(config, sample_credentials()).unwrap();
    let result = driver
        .resolve_download_url(
            "files/document.pdf",
            Duration::from_secs(300),
            Default::default(),
        )
        .await
        .unwrap()
        .unwrap();
    let url = Url::parse(&result.url).unwrap();
    assert_eq!(url.host_str(), Some("cdn.example.com"));
    assert_eq!(url.path(), "/tenant/prefix/files/document.pdf");
    let auth = url
        .query_pairs()
        .find(|(key, _)| key == "auth_key")
        .unwrap()
        .1;
    let parts: Vec<_> = auth.split('-').collect();
    assert_eq!(parts.len(), 4);
    assert_eq!(parts[1].len(), 32);
    assert!(parts[1].bytes().all(|b| b.is_ascii_hexdigit()));
    assert_eq!(parts[2], "0");
    assert_eq!(parts[3].len(), 32);
    assert_eq!(
        result.expires_at.unwrap().timestamp(),
        parts[0].parse::<i64>().unwrap() + 300
    );
}

#[tokio::test]
async fn cdn_blob_download_uses_cdn_with_provider_headers_allowed() {
    let mut config = sample_config();
    config.cdn_domain = Some("https://cdn.example.com".into());
    config.download_delivery = OssDownloadDelivery::AlibabaCdnTypeA;
    let driver = AlibabaOssDriver::new(config, sample_credentials()).unwrap();
    let result = driver
        .resolve_download_url(
            "files/document.pdf",
            Duration::from_secs(60),
            DirectDownloadOptions {
                allow_provider_response_headers: true,
                download_name: Some("renamed.pdf".into()),
                response_cache_control: Some("private, max-age=0, must-revalidate".into()),
                response_content_disposition: Some("attachment; filename=\"renamed.pdf\"".into()),
                response_content_type: Some("application/pdf".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .unwrap();
    let url = Url::parse(&result.url).unwrap();
    assert_eq!(url.host_str(), Some("cdn.example.com"));
    assert_eq!(url.query_pairs().count(), 1);
    assert!(url.query_pairs().all(|(key, _)| key == "auth_key"));
}

#[tokio::test]
async fn no_cdn_uses_oss_v4_without_cdn_key() {
    let mut credentials = sample_credentials();
    credentials.cdn_auth_key = None;
    let driver = AlibabaOssDriver::new(sample_config(), credentials).unwrap();
    let result = driver
        .resolve_download_url(
            "files/video.mp4",
            Duration::from_secs(300),
            Default::default(),
        )
        .await
        .unwrap()
        .unwrap();
    assert!(result.url.contains("oss-cn-hangzhou.aliyuncs.com"));
    assert!(result.url.contains("x-oss-signature="));
    assert!(!result.url.contains("auth_key="));
}

#[tokio::test]
async fn oss_origin_delivery_does_not_activate_stored_cdn_settings() {
    let mut config = sample_config();
    config.cdn_domain = Some("https://cdn.example.com".into());
    let mut credentials = sample_credentials();
    credentials.cdn_auth_key = None;
    let driver = AlibabaOssDriver::new(config, credentials).unwrap();
    let result = driver
        .resolve_download_url("file", Duration::from_secs(60), Default::default())
        .await
        .unwrap()
        .unwrap();
    assert!(result.url.contains("x-oss-signature="));
    assert!(!result.url.contains("cdn.example.com"));
}

#[test]
fn explicit_cdn_delivery_requires_a_domain() {
    let mut config = sample_config();
    config.download_delivery = OssDownloadDelivery::AlibabaCdnTypeA;
    assert!(AlibabaOssDriver::validate_config(&config, &sample_credentials()).is_err());
}

#[test]
fn cdn_config_rejects_unsafe_domains() {
    for domain in [
        "http://cdn.example.com",
        "https://user:secret@cdn.example.com",
        "https://cdn.example.com/prefix",
        "https://cdn.example.com/prefix/..",
        "https://cdn.example.com/%2e",
        "https://cdn.example.com?x=1",
        "https://cdn.example.com#fragment",
        "https://cdn.example.com/\\path",
        "https://cdn.exa\nmple.com",
        "https://127.0.0.1",
        "https://localhost",
    ] {
        let mut config = sample_config();
        config.cdn_domain = Some(domain.into());
        config.download_delivery = OssDownloadDelivery::AlibabaCdnTypeA;
        let error =
            AlibabaOssDriver::validate_config(&config, &sample_credentials()).expect_err(domain);
        assert_eq!(error.kind(), StorageErrorKind::Misconfigured);
    }
}

#[test]
fn cdn_config_requires_valid_key_and_validity() {
    let mut config = sample_config();
    config.cdn_domain = Some("https://cdn.example.com".into());
    config.download_delivery = OssDownloadDelivery::AlibabaCdnTypeA;
    for key in [
        None,
        Some(""),
        Some("short"),
        Some("with-hyphen"),
        Some(" leading123"),
        Some("尾部secret"),
    ] {
        let mut credentials = sample_credentials();
        credentials.cdn_auth_key = key.map(str::to_owned);
        assert!(AlibabaOssDriver::validate_config(&config, &credentials).is_err());
    }
    for validity in [0, 31_536_001, u32::MAX] {
        config.cdn_auth_validity_seconds = validity;
        assert!(AlibabaOssDriver::validate_connection_config(&config).is_err());
    }
    for validity in [1, 300, 31_536_000] {
        config.cdn_auth_validity_seconds = validity;
        AlibabaOssDriver::validate_config(&config, &sample_credentials()).unwrap();
    }
}

#[tokio::test]
async fn cdn_strict_response_headers_request_relay_instead_of_unsigned_overrides() {
    let mut config = sample_config();
    config.cdn_domain = Some("https://cdn.example.com".into());
    config.download_delivery = OssDownloadDelivery::AlibabaCdnTypeA;
    let driver = AlibabaOssDriver::new(config, sample_credentials()).unwrap();
    for options in [
        DirectDownloadOptions {
            response_content_disposition: Some("attachment; filename=report.pdf".into()),
            ..Default::default()
        },
        DirectDownloadOptions {
            response_content_type: Some("text/html".into()),
            ..Default::default()
        },
        DirectDownloadOptions {
            response_cache_control: Some("private".into()),
            ..Default::default()
        },
        DirectDownloadOptions {
            require_download_name_match: true,
            allow_provider_response_headers: true,
            ..Default::default()
        },
    ] {
        assert!(
            driver
                .resolve_download_url("file", Duration::from_secs(60), options)
                .await
                .unwrap()
                .is_none()
        );
    }
}

#[tokio::test]
async fn cdn_and_cname_keep_presigned_uploads_on_oss_v4() {
    let mut config = sample_config();
    config.cdn_domain = Some("https://cdn.example.com".into());
    config.download_delivery = OssDownloadDelivery::AlibabaCdnTypeA;
    config.endpoint = "https://files.example.test".into();
    config.use_cname = true;
    config.server_side_endpoint = "https://oss-cn-hangzhou-internal.aliyuncs.com".into();
    let driver = AlibabaOssDriver::new(config, sample_credentials()).unwrap();
    for request in [
        driver
            .presigned_put_request("objects/report.txt", Duration::from_secs(60))
            .await
            .unwrap()
            .unwrap(),
        driver
            .presigned_upload_part_request(
                "objects/report.txt",
                "upload-id",
                1,
                Duration::from_secs(60),
            )
            .await
            .unwrap(),
    ] {
        let url = Url::parse(&request.url).unwrap();
        assert_eq!(url.host_str(), Some("files.example.test"));
        assert_eq!(url.path(), "/tenant/prefix/objects/report.txt");
        assert!(url.query_pairs().any(|(key, _)| key == "x-oss-signature"));
        assert!(!url.query_pairs().any(|(key, _)| key == "auth_key"));
    }
}
