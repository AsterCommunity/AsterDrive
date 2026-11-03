//! Alibaba Cloud CDN Type A delivery, separate from OSS V4 origin signing.
//!
//! https://help.aliyun.com/zh/cdn/user-guide/type-a-signing
//! Type A authenticates the encoded path, timestamp, nonce and uid, not
//! arbitrary query parameters or response-header overrides.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use aster_drive_storage::{
    DirectDownloadOptions, DirectDownloadRequest, Result, StorageErrorKind, storage_driver_error,
};
use chrono::{DateTime, Utc};
use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};
use url::Url;

use super::CdnConfig;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OssDownloadDelivery {
    #[default]
    OssOrigin,
    AlibabaCdnTypeA,
}

pub(crate) const fn default_cdn_auth_validity_seconds() -> u32 {
    300
}

pub(crate) fn validate_cdn_auth_key(key: &str) -> Result<()> {
    match key {
        key if (6..=128).contains(&key.len())
            && key.bytes().all(|byte| byte.is_ascii_alphanumeric()) =>
        {
            Ok(())
        }
        _ => Err(storage_driver_error(
            StorageErrorKind::Misconfigured,
            "Alibaba CDN authentication key must contain 6 to 128 ASCII letters or digits",
        )),
    }
}

pub(super) fn validate_cdn_auth_validity(validity: u32) -> Result<()> {
    match validity {
        1..=31_536_000 => Ok(()),
        _ => Err(storage_driver_error(
            StorageErrorKind::Misconfigured,
            "Alibaba CDN authentication validity must be between 1 and 31536000 seconds",
        )),
    }
}

pub(super) fn validated_cdn_domain(value: &str) -> Result<Url> {
    let invalid = || {
        storage_driver_error(
            StorageErrorKind::Misconfigured,
            "Alibaba CDN domain must be an HTTPS hostname without credentials, path, query or fragment",
        )
    };
    let value = match value.trim() {
        value if value.chars().any(char::is_control) || value.contains('\\') => {
            return Err(invalid());
        }
        value => value,
    };
    // Check before Url normalizes dot segments (e.g. /prefix/.. to /).
    let raw: http::Uri = value.parse().map_err(|_| invalid())?;
    match raw.path() {
        "" | "/" => {}
        _ => return Err(invalid()),
    }
    let url = Url::parse(value).map_err(|_| invalid())?;
    match (
        url.scheme(),
        url.username(),
        url.password(),
        url.query(),
        url.fragment(),
        url.host(),
    ) {
        ("https", "", None, None, None, Some(url::Host::Domain(host))) if host != "localhost" => {
            Ok(url)
        }
        _ => Err(invalid()),
    }
}

impl CdnConfig {
    pub(super) fn download_request(
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
                    "Alibaba CDN signing clock is before Unix epoch",
                )
            })?
            .as_secs();
        self.download_request_at(
            path,
            expires,
            options,
            now,
            &uuid::Uuid::new_v4().simple().to_string(),
        )
    }

    fn download_request_at(
        &self,
        path: &str,
        expires: Duration,
        options: &DirectDownloadOptions,
        now: u64,
        nonce: &str,
    ) -> Result<Option<DirectDownloadRequest>> {
        let key = &self.auth_key;
        let validity = u64::from(self.validity_seconds);
        let seconds = match expires.as_secs().min(validity) {
            0 => {
                return Err(storage_driver_error(
                    StorageErrorKind::Misconfigured,
                    "Alibaba CDN link lifetime must be at least one second",
                ));
            }
            seconds => seconds,
        };
        let deadline = now.checked_add(seconds).ok_or_else(|| {
            storage_driver_error(
                StorageErrorKind::Misconfigured,
                "Alibaba CDN expiration overflow",
            )
        })?;
        // CDN expires timestamp + console validity, not timestamp itself.
        // Backdate shorter requests so expires_at matches the actual deadline.
        let timestamp = deadline
            .checked_sub(validity)
            .filter(|value| *value > 0)
            .ok_or_else(|| {
                storage_driver_error(
                    StorageErrorKind::Misconfigured,
                    "Alibaba CDN signing timestamp is invalid",
                )
            })?;
        let expires_at = i64::try_from(deadline)
            .ok()
            .and_then(|value| DateTime::<Utc>::from_timestamp(value, 0))
            .ok_or_else(|| {
                storage_driver_error(
                    StorageErrorKind::Misconfigured,
                    "Alibaba CDN expiration is out of range",
                )
            })?;

        match (
            options.require_download_name_match,
            options.allow_provider_response_headers,
            options.response_cache_control.is_some()
                || options.response_content_disposition.is_some()
                || options.response_content_type.is_some(),
        ) {
            (true, _, _) | (false, false, true) => return Ok(None),
            (false, true, _) | (false, false, false) => {}
        }
        let mut url = self.domain.clone();
        let object_key = aster_drive_storage::object_key::join_key_prefix(&self.base_path, path);
        match object_key.as_str() {
            key if key.is_empty()
                || key.split('/').any(|part| matches!(part, "." | ".."))
                || key.chars().any(char::is_control) =>
            {
                return Err(storage_driver_error(
                    StorageErrorKind::Misconfigured,
                    "Alibaba CDN object path is invalid",
                ));
            }
            _ => {}
        }
        url.path_segments_mut()
            .map_err(|_| {
                storage_driver_error(
                    StorageErrorKind::Misconfigured,
                    "Alibaba CDN domain cannot be used as a path base",
                )
            })?
            .pop_if_empty()
            .extend(object_key.split('/'));
        // Sign the encoded path actually sent by the browser, not a decoded
        // object key. Business query parameters are deliberately not emitted.
        let signature = hex::encode(Md5::digest(
            format!("{}-{timestamp}-{nonce}-0-{key}", url.path()).as_bytes(),
        ));
        url.query_pairs_mut()
            .append_pair("auth_key", &format!("{timestamp}-{nonce}-0-{signature}"));
        Ok(Some(DirectDownloadRequest::preauthenticated_url(
            url,
            Some(expires_at),
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::drivers::alibaba_oss::tests::{sample_config, sample_credentials};

    fn driver() -> CdnConfig {
        let mut config = sample_config();
        config.base_path.clear();
        config.cdn_domain = Some("https://cdn.example.test".into());
        config.download_delivery = OssDownloadDelivery::AlibabaCdnTypeA;
        let mut credentials = sample_credentials();
        credentials.cdn_auth_key = Some("aliyuncdnexp1234".into());
        match super::super::AlibabaOssDriver::new(config, credentials)
            .unwrap()
            .download_delivery
        {
            super::super::DownloadDelivery::AlibabaCdnTypeA(config) => config,
            super::super::DownloadDelivery::OssOrigin => panic!("test config must select CDN"),
        }
    }

    #[test]
    fn type_a_matches_official_digest_vector() {
        let result = driver()
            .download_request_at(
                "video/standard/test.mp4",
                Duration::from_secs(300),
                &Default::default(),
                1444435200,
                "0",
            )
            .unwrap()
            .unwrap();
        assert_eq!(
            result.url,
            concat!(
                "https://cdn.example.test/video/standard/test.mp4?auth_key=",
                "1444435200-0-0-",
                "23bf85053008f5c0e791667a313e28ce"
            )
        );
        assert_eq!(result.expires_at.unwrap().timestamp(), 1444435500);
        assert_eq!(
            result.credentials,
            aster_drive_storage::DirectDownloadCredentials::Omit
        );
    }

    #[test]
    fn type_a_bounds_real_expiration_by_caller_and_console_validity() {
        let driver = driver();
        for requested in [1, 60, 300, 301, 3600, u64::MAX] {
            let result = driver
                .download_request_at(
                    "file",
                    Duration::from_secs(requested),
                    &Default::default(),
                    1700000000,
                    "0",
                )
                .unwrap()
                .unwrap();
            let url = Url::parse(&result.url).unwrap();
            let auth = url.query_pairs().next().unwrap().1;
            let timestamp: u64 = auth.split('-').next().unwrap().parse().unwrap();
            assert!(timestamp <= 1700000000);
            assert_eq!(timestamp + 300, 1700000000 + requested.min(300));
            assert_eq!(
                result.expires_at.unwrap().timestamp(),
                i64::try_from(timestamp + 300).unwrap()
            );
        }
    }

    #[test]
    fn type_a_rejects_invalid_time_and_overflow() {
        let driver = driver();
        for (duration, now) in [
            (Duration::ZERO, 1700000000),
            (Duration::from_millis(500), 1700000000),
            (Duration::from_secs(1), 1),
            (Duration::from_secs(300), u64::MAX),
            (Duration::from_secs(300), u64::try_from(i64::MAX).unwrap()),
        ] {
            assert!(
                driver
                    .download_request_at("file", duration, &Default::default(), now, "0")
                    .is_err()
            );
        }
    }

    #[test]
    fn type_a_signs_exact_encoded_path_and_prefix() {
        let mut driver = driver();
        driver.base_path = "Tenant/Prefix".into();
        let result = driver
            .download_request_at(
                "报告 +#%2F/UPPER.txt",
                Duration::from_secs(300),
                &Default::default(),
                1700000000,
                "abc",
            )
            .unwrap()
            .unwrap();
        let url = Url::parse(&result.url).unwrap();
        assert_eq!(
            url.path(),
            "/Tenant/Prefix/%E6%8A%A5%E5%91%8A%20+%23%252F/UPPER.txt"
        );
        // Independent Python hashlib.md5 vector, using the wire path verbatim.
        assert_eq!(
            url.query_pairs().next().unwrap().1,
            "1700000000-abc-0-fd3550183ab77e18041bcae03bbfad7f"
        );
    }

    #[test]
    fn type_a_rejects_traversal_empty_keys_and_controls() {
        let driver = driver();
        for key in [
            "",
            ".",
            "..",
            "folder/../file",
            "folder/./file",
            "file\nname",
            "file\0name",
        ] {
            assert!(
                driver
                    .download_request_at(
                        key,
                        Duration::from_secs(300),
                        &Default::default(),
                        1700000000,
                        "0"
                    )
                    .is_err(),
                "{key:?}"
            );
        }
    }

    #[test]
    fn type_a_key_length_matches_console_contract() {
        for length in [6, 16, 32, 128] {
            validate_cdn_auth_key(&"a".repeat(length)).unwrap();
        }
        assert!(validate_cdn_auth_key(&"a".repeat(129)).is_err());
    }
}
