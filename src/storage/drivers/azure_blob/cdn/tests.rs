use std::collections::HashMap;
use std::time::Duration;

use aster_drive_storage::{
    DirectDownloadOptions, DirectDownloadStorageDriver, PresignedUploadStorageDriver,
};
use base64::Engine as _;
use hmac::{Hmac, KeyInit, Mac};

use super::*;
use crate::storage::drivers::azure_blob::{AzureBlobDriver, AzureBlobStaticCredentials};

fn config() -> AzureBlobDriverConfig {
    AzureBlobDriverConfig {
        endpoint: "http://127.0.0.1:10000/account".into(),
        container: "photos".into(),
        base_path: "base path".into(),
        download_delivery: AzureBlobDownloadDelivery::AzureFrontDoorSas,
        download_base_url: "https://files.example.test/".into(),
        ..Default::default()
    }
}

fn driver(config: AzureBlobDriverConfig) -> AzureBlobDriver {
    AzureBlobDriver::new(
        config,
        AzureBlobStaticCredentials {
            account_name: "account".into(),
            account_key: "c2VjcmV0".into(),
        },
    )
    .unwrap()
}

#[test]
fn front_door_configuration_has_explicit_mode_and_bounded_cache_time() {
    for address in [
        "",
        "http://files.example.test",
        "https://user:fixture_value@files.example.test",
        "https://files.example.test/container",
        "https://files.example.test/prefix/..",
        "https://files.example.test?sig=hidden",
        "https://files.example.test#fragment",
    ] {
        let mut config = config();
        config.download_base_url = address.into();
        let error = match FrontDoorConfig::from_config(&config) {
            Ok(_) => panic!("invalid Front Door address accepted: {address}"),
            Err(error) => error,
        };
        assert!(!error.message().contains("sig=hidden"));
        assert!(!error.message().contains("user:fixture_value"));
    }
    for seconds in [0, MAX_CACHE_MAX_AGE_SECONDS] {
        let mut config = config();
        config.azure_cdn_cache_max_age_seconds = seconds;
        assert!(FrontDoorConfig::from_config(&config).is_ok());
    }
    let mut config = config();
    config.azure_cdn_cache_max_age_seconds = MAX_CACHE_MAX_AGE_SECONDS + 1;
    assert!(FrontDoorConfig::from_config(&config).is_err());
    config.download_delivery = AzureBlobDownloadDelivery::AzureBlobOrigin;
    config.download_base_url = "unused invalid value".into();
    assert!(FrontDoorConfig::from_config(&config).unwrap().is_none());
}

#[tokio::test]
async fn front_door_signs_encoded_paths_and_cache_headers_with_account_identity() {
    let path = "中文/100%2F +.pdf";
    let disposition = "attachment; filename=fallback.pdf; filename*=UTF-8''%E6%8A%A5%E5%91%8A.pdf";
    for (seconds, expected_cache) in [
        (0, "private, max-age=0, must-revalidate"),
        (300, "public, max-age=0, s-maxage=300"),
    ] {
        let mut config = config();
        config.azure_cdn_cache_max_age_seconds = seconds;
        let driver = driver(config);
        let request = driver
            .resolve_download_url(
                path,
                Duration::from_secs(300),
                DirectDownloadOptions {
                    response_cache_control: Some("public, max-age=99999".into()),
                    response_content_disposition: Some(disposition.into()),
                    response_content_type: Some("application/pdf".into()),
                    ..Default::default()
                },
            )
            .await
            .unwrap()
            .unwrap();
        let url = Url::parse(&request.url).unwrap();
        assert_eq!(url.host_str(), Some("files.example.test"));
        assert_eq!(url.scheme(), "https");
        assert_eq!(
            url.path(),
            "/photos/base%20path/%E4%B8%AD%E6%96%87/100%252F%20%2B%2Epdf"
        );
        let query: HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(query["rscc"], expected_cache);
        assert_eq!(query["rscd"], disposition);
        assert_eq!(query["rsct"], "application/pdf");
        assert_eq!(query["spr"], "https");
        assert_eq!(query["sp"], "r");
        // Independent service-SAS vector: real account and decoded object path,
        // not the Front Door host or URL-encoded path/response header values.
        let string_to_sign = format!(
            "r\n{}\n{}\n/blob/account/photos/base path/{path}\n\n\nhttps\n2023-11-03\nb\n\n\n{expected_cache}\n{disposition}\n\n\napplication/pdf",
            query["st"], query["se"]
        );
        let fixture_value = base64::engine::general_purpose::STANDARD
            .decode("c2VjcmV0")
            .unwrap();
        let mut mac = Hmac::<sha2::Sha256>::new_from_slice(&fixture_value).unwrap();
        mac.update(string_to_sign.as_bytes());
        assert_eq!(
            query["sig"],
            base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes())
        );
    }
}

#[tokio::test]
async fn front_door_keeps_upload_container_block_and_copy_source_urls_on_origin() {
    let driver = driver(config());
    let upload = driver
        .presigned_put_request("object", Duration::from_secs(300))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(upload.headers["x-ms-blob-type"], "BlockBlob");
    for url in [
        upload.url,
        driver
            .container_url("rl", Duration::from_secs(300))
            .unwrap()
            .to_string(),
        driver
            .block_blob_url("object", "cw", Duration::from_secs(300))
            .unwrap()
            .to_string(),
        driver
            .blob_url(
                &driver.endpoint,
                "object",
                "r",
                Duration::from_secs(300),
                Default::default(),
            )
            .unwrap()
            .to_string(),
    ] {
        let url = Url::parse(&url).unwrap();
        assert_eq!(url.host_str(), Some("127.0.0.1"));
        assert!(url.path().starts_with("/account/photos"));
        let query: HashMap<_, _> = url.query_pairs().into_owned().collect();
        for field in ["rscc", "rscd", "rsct"] {
            assert!(!query.contains_key(field));
        }
    }
}

#[tokio::test]
async fn source_mode_ignores_dormant_front_door_settings_and_preserves_response_options() {
    let mut config = config();
    config.download_delivery = AzureBlobDownloadDelivery::AzureBlobOrigin;
    config.download_base_url = "unused invalid value".into();
    config.azure_cdn_cache_max_age_seconds = u32::MAX;
    let driver = driver(config);
    let request = driver
        .resolve_download_url(
            "object",
            Duration::from_secs(300),
            DirectDownloadOptions {
                response_cache_control: Some("private, no-store".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .unwrap();
    let url = Url::parse(&request.url).unwrap();
    assert_eq!(url.host_str(), Some("127.0.0.1"));
    let query: HashMap<_, _> = url.query_pairs().into_owned().collect();
    assert_eq!(query["rscc"], "private, no-store");
}
