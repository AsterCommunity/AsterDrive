//! Tencent CDN Type D delivery, separate from COS origin Q-Sign.
//!
//! https://cloud.tencent.com/document/product/228/41625
//! Only sign/t are emitted: Type D does not authenticate response overrides.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use aster_drive_storage::{
    DirectDownloadOptions, DirectDownloadRequest, Result, StorageErrorKind, storage_driver_error,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use url::Url;

use super::TencentCosDriver;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CosDownloadDelivery {
    #[default]
    CosOrigin,
    CustomOrigin,
    TencentCdnTypeDSha256,
}

pub const fn default_cdn_auth_validity_seconds() -> u32 {
    300
}

pub fn validate_cdn_auth_key(key: &str) -> Result<()> {
    if !(6..=32).contains(&key.len()) || !key.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
        return Err(storage_driver_error(
            StorageErrorKind::Misconfigured,
            "Tencent CDN authentication key must contain 6 to 32 ASCII letters or digits",
        ));
    }
    Ok(())
}

impl TencentCosDriver {
    pub(super) fn cdn_download_request(
        &self,
        path: &str,
        expires: Duration,
        options: &DirectDownloadOptions,
    ) -> Result<Option<DirectDownloadRequest>> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| {
                storage_driver_error(
                    StorageErrorKind::Misconfigured,
                    "CDN signing clock is before Unix epoch",
                )
            })?
            .as_secs();
        self.cdn_download_request_at(path, expires, options, now)
    }

    fn cdn_download_request_at(
        &self,
        path: &str,
        expires: Duration,
        options: &DirectDownloadOptions,
        now: u64,
    ) -> Result<Option<DirectDownloadRequest>> {
        // Fail closed even if the caller would otherwise use relay fallback.
        let key = self.cdn_auth_key.as_deref().ok_or_else(|| {
            storage_driver_error(
                StorageErrorKind::Misconfigured,
                "Tencent CDN authentication key is required",
            )
        })?;
        validate_cdn_auth_key(key)?;
        let validity = u64::from(self.cdn_auth_validity_seconds);
        if !(1..=3600).contains(&validity) {
            return Err(storage_driver_error(
                StorageErrorKind::Misconfigured,
                "Tencent CDN authentication validity must be between 1 and 3600 seconds",
            ));
        }
        let seconds = expires.as_secs().min(validity);
        if seconds == 0 {
            return Err(storage_driver_error(
                StorageErrorKind::Misconfigured,
                "Tencent CDN link lifetime must be at least one second",
            ));
        }
        let deadline = now.checked_add(seconds).ok_or_else(|| {
            storage_driver_error(
                StorageErrorKind::Misconfigured,
                "Tencent CDN link expiration overflow",
            )
        })?;
        // CDN expires timestamp + configured validity, not timestamp itself.
        // Backdate short requests rather than granting the full configured TTL.
        let timestamp = deadline
            .checked_sub(validity)
            .filter(|value| *value > 0)
            .ok_or_else(|| {
                storage_driver_error(
                    StorageErrorKind::Misconfigured,
                    "Tencent CDN signing timestamp is invalid",
                )
            })?;
        let expires_at = i64::try_from(deadline)
            .ok()
            .and_then(|value| DateTime::<Utc>::from_timestamp(value, 0))
            .ok_or_else(|| {
                storage_driver_error(
                    StorageErrorKind::Misconfigured,
                    "Tencent CDN expiration is out of range",
                )
            })?;

        if !options.allow_provider_response_headers
            && (options.response_cache_control.is_some()
                || options.response_content_disposition.is_some()
                || options.response_content_type.is_some())
            || options.require_download_name_match
        {
            return Ok(None);
        }
        let base = self.download_base_url.as_deref().ok_or_else(|| {
            storage_driver_error(
                StorageErrorKind::Misconfigured,
                "Tencent CDN download URL is required",
            )
        })?;
        let mut url = validated_base_url(base)?;
        let object_key = self.full_key(path);
        if object_key.is_empty()
            || object_key.split('/').any(|part| matches!(part, "." | ".."))
            || object_key.chars().any(char::is_control)
        {
            return Err(storage_driver_error(
                StorageErrorKind::Misconfigured,
                "Tencent CDN object path is invalid",
            ));
        }
        url.path_segments_mut()
            .map_err(|_| {
                storage_driver_error(
                    StorageErrorKind::Misconfigured,
                    "Tencent CDN download URL cannot be used as a path base",
                )
            })?
            .pop_if_empty()
            .extend(object_key.split('/'));
        // Sign the encoded path actually sent by the browser; never decode %2F
        // into a path separator or decode a literal percent twice.
        let signature = hex::encode(Sha256::digest(
            format!("{key}{}{timestamp}", url.path()).as_bytes(),
        ));
        url.query_pairs_mut()
            .append_pair("sign", &signature)
            .append_pair("t", &timestamp.to_string());
        Ok(Some(DirectDownloadRequest::preauthenticated_url(
            url,
            Some(expires_at),
        )))
    }
}

pub(super) fn validated_base_url(value: &str) -> Result<Url> {
    let invalid = || {
        storage_driver_error(
            StorageErrorKind::Misconfigured,
            "invalid Tencent CDN download URL",
        )
    };
    if value.chars().any(char::is_control) || value.contains('\\') {
        return Err(invalid());
    }
    let raw: http::Uri = value.parse().map_err(|_| invalid())?;
    for part in raw.path().split('/') {
        let decoded = percent_encoding::percent_decode_str(part)
            .decode_utf8()
            .map_err(|_| invalid())?;
        if matches!(decoded.as_ref(), "." | "..")
            || decoded.chars().any(char::is_control)
            || decoded.contains(['/', '\\'])
        {
            return Err(invalid());
        }
    }
    let url = Url::parse(value).map_err(|_| invalid())?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !matches!(url.host(), Some(url::Host::Domain(host)) if host != "localhost")
    {
        return Err(invalid());
    }
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::drivers::tencent_cos::tests::{sample_config, sample_credentials};

    fn driver() -> TencentCosDriver {
        let mut config =
            sample_config("https://cos.ap-guangzhou.myqcloud.com", "bucket-1250000000");
        config.download_delivery = CosDownloadDelivery::TencentCdnTypeDSha256;
        config.download_base_url = Some("https://cdn.example.test".into());
        config.base_path.clear();
        let mut credentials = sample_credentials();
        credentials.cdn_auth_key = Some("dimtm5evg50ijsx2hvuwyfoiu65".into());
        TencentCosDriver::new(config, credentials).unwrap()
    }

    #[test]
    fn type_d_sha256_matches_independent_vector() {
        let request = driver()
            .cdn_download_request_at(
                "test.jpg",
                Duration::from_secs(300),
                &Default::default(),
                1700000000,
            )
            .unwrap()
            .unwrap();
        // Independently checked with Python hashlib.sha256(b'dimtm5evg50ijsx2hvuwyfoiu65/test.jpg1700000000').hexdigest().
        assert_eq!(
            request.url,
            "https://cdn.example.test/test.jpg?sign=f54e3bda1c8bc1a84b74925e18223dd212f20f8ab77e75b03c5cd55f8534e193&t=1700000000"
        );
        assert_eq!(request.expires_at.unwrap().timestamp(), 1700000300);
        assert_eq!(
            request.credentials,
            aster_drive_storage::DirectDownloadCredentials::Omit
        );
    }

    #[test]
    fn type_d_bounds_actual_expiration_without_future_timestamps() {
        let driver = driver();
        for requested in [1, 60, 300, 301, 3600, u64::MAX] {
            let result = driver
                .cdn_download_request_at(
                    "file",
                    Duration::from_secs(requested),
                    &Default::default(),
                    1700000000,
                )
                .unwrap()
                .unwrap();
            let url = Url::parse(&result.url).unwrap();
            let timestamp: u64 = url
                .query_pairs()
                .find(|(key, _)| key == "t")
                .unwrap()
                .1
                .parse()
                .unwrap();
            assert!(timestamp <= 1700000000);
            assert_eq!(timestamp + 300, 1700000000 + requested.min(300));
            assert_eq!(
                result.expires_at.unwrap().timestamp(),
                i64::try_from(timestamp + 300).unwrap()
            );
        }
        for duration in [Duration::ZERO, Duration::from_millis(999)] {
            assert!(
                driver
                    .cdn_download_request_at("file", duration, &Default::default(), 1700000000)
                    .is_err()
            );
        }
        assert!(
            driver
                .cdn_download_request_at(
                    "file",
                    Duration::from_secs(300),
                    &Default::default(),
                    u64::MAX
                )
                .is_err()
        );
        assert!(
            driver
                .cdn_download_request_at("file", Duration::from_secs(1), &Default::default(), 1)
                .is_err()
        );
    }

    #[test]
    fn type_d_never_emits_unsigned_response_overrides() {
        let driver = driver();
        for options in [
            DirectDownloadOptions {
                response_cache_control: Some("private".into()),
                ..Default::default()
            },
            DirectDownloadOptions {
                response_content_disposition: Some("attachment".into()),
                ..Default::default()
            },
            DirectDownloadOptions {
                response_content_type: Some("text/html".into()),
                ..Default::default()
            },
            DirectDownloadOptions {
                require_download_name_match: true,
                ..Default::default()
            },
        ] {
            assert!(
                driver
                    .cdn_download_request_at("file", Duration::from_secs(60), &options, 1700000000)
                    .unwrap()
                    .is_none()
            );
        }
        let options = DirectDownloadOptions {
            allow_provider_response_headers: true,
            response_content_type: Some("image/png".into()),
            ..Default::default()
        };
        let result = driver
            .cdn_download_request_at("file", Duration::from_secs(60), &options, 1700000000)
            .unwrap()
            .unwrap();
        let url = Url::parse(&result.url).unwrap();
        assert_eq!(url.query_pairs().count(), 2);
        assert!(
            url.query_pairs()
                .all(|(key, _)| matches!(key.as_ref(), "sign" | "t"))
        );
    }

    #[test]
    fn type_d_preserves_encoded_keys_prefix_and_case() {
        let mut driver = driver();
        driver.download_base_url = Some("https://cdn.example.test/assets/v1/".into());
        driver.base_path = "Tenant".into();
        let result = driver
            .cdn_download_request_at(
                "报告 +#%2F/UPPER.txt",
                Duration::from_secs(60),
                &Default::default(),
                1700000000,
            )
            .unwrap()
            .unwrap();
        let url = Url::parse(&result.url).unwrap();
        assert_eq!(
            url.path(),
            "/assets/v1/Tenant/%E6%8A%A5%E5%91%8A%20+%23%252F/UPPER.txt"
        );
        let stamp = url
            .query_pairs()
            .find(|(key, _)| key == "t")
            .unwrap()
            .1
            .into_owned();
        let expected = hex::encode(Sha256::digest(
            format!("dimtm5evg50ijsx2hvuwyfoiu65{}{stamp}", url.path()).as_bytes(),
        ));
        assert_eq!(
            url.query_pairs().find(|(key, _)| key == "sign").unwrap().1,
            expected
        );
        for path in ["../other", "a/../other", "a/./other", "file\n"] {
            assert!(
                driver
                    .cdn_download_request_at(
                        path,
                        Duration::from_secs(60),
                        &Default::default(),
                        1700000000
                    )
                    .is_err()
            );
        }
    }

    #[test]
    fn type_d_rejects_unsafe_urls_and_credentials() {
        for url in [
            "http://cdn.test",
            "https://localhost",
            "https://127.0.0.1",
            "https://[::1]",
            "https://u:p@cdn.test",
            "https://cdn.test:444",
            "https://cdn.test?a=1",
            "https://cdn.test#f",
            "https://cdn.test/a/../b",
            "https://cdn.test/%2e%2e/b",
            "https://cdn.test/a%2fb",
            "https://cdn.test/%00",
            "https://cdn.test\\evil",
            "https://cdn.test/\n",
        ] {
            assert!(validated_base_url(url).is_err(), "{url:?}");
        }
        for key in [
            "",
            "short",
            "secret!",
            "secret key",
            "secret\n",
            "密码密码",
            &"a".repeat(33),
        ] {
            assert!(validate_cdn_auth_key(key).is_err());
        }
        for key in ["abc123", &"a".repeat(32)] {
            validate_cdn_auth_key(key).unwrap();
        }
    }

    #[tokio::test]
    async fn cdn_mode_keeps_presigned_upload_on_the_cos_origin() {
        use aster_drive_storage::traits::extensions::PresignedUploadStorageDriver;
        let driver = driver();
        let request = driver
            .presigned_put_request("upload.jpg", Duration::from_secs(60))
            .await
            .unwrap()
            .unwrap();
        let url = Url::parse(&request.url).unwrap();
        assert_eq!(
            url.host_str(),
            Some("bucket-1250000000.cos.ap-guangzhou.myqcloud.com")
        );
        assert!(url.query_pairs().any(|(key, _)| key == "q-signature"));
        assert!(!url.query_pairs().any(|(key, _)| key == "sign"));
    }

    #[test]
    fn malformed_runtime_credentials_never_fall_back_to_public_delivery() {
        let mut driver = driver();
        driver.cdn_auth_key = None;
        let options = DirectDownloadOptions {
            response_content_disposition: Some("attachment".into()),
            ..Default::default()
        };
        assert!(
            driver
                .cdn_download_request_at("file", Duration::from_secs(60), &options, 1700000000)
                .is_err()
        );
        driver.cdn_auth_key = Some("ValidKey012345".into());
        for validity in [0, 3601, u32::MAX] {
            driver.cdn_auth_validity_seconds = validity;
            assert!(
                driver
                    .cdn_download_request_at(
                        "file",
                        Duration::from_secs(60),
                        &Default::default(),
                        1700000000
                    )
                    .is_err()
            );
        }
    }
}
