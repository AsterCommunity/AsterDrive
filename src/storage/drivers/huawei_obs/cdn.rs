//! Huawei CDN Type A / SHA-256 delivery, separate from native OBS signing.
//!
//! https://support.huaweicloud.com/usermanual-cdn/cdn_01_0040.html
//! Sign the encoded wire path, decimal start time, nonce, uid=0 and CDN key.
//! Expiration is start time + console validity; business queries are not signed.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use aster_drive_storage::{
    DirectDownloadOptions, DirectDownloadRequest, Result, StorageErrorKind, storage_driver_error,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use url::Url;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObsDownloadDelivery {
    #[default]
    ObsOrigin,
    HuaweiCdnTypeASha256,
}

pub(crate) const fn default_cdn_auth_validity_seconds() -> u32 {
    300
}

pub(crate) fn default_cdn_auth_param() -> String {
    "auth_key".into()
}

pub(crate) fn validate_cdn_auth_key(key: &str) -> Result<()> {
    match key {
        key if (6..=32).contains(&key.len())
            && key.bytes().all(|byte| byte.is_ascii_alphanumeric()) =>
        {
            Ok(())
        }
        _ => Err(storage_driver_error(
            StorageErrorKind::Misconfigured,
            "Huawei CDN authentication key must contain 6 to 32 ASCII letters or digits",
        )),
    }
}

pub(super) fn validate_cdn_auth_param(param: &str) -> Result<()> {
    match param {
        param
            if (1..=100).contains(&param.len())
                && param
                    .as_bytes()
                    .first()
                    .is_some_and(u8::is_ascii_alphabetic)
                && param
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_') =>
        {
            Ok(())
        }
        _ => Err(storage_driver_error(
            StorageErrorKind::Misconfigured,
            "Huawei CDN authentication parameter must start with an ASCII letter and contain 1 to 100 ASCII letters, digits, or underscores",
        )),
    }
}

pub(super) fn validate_cdn_auth_validity(validity: u32) -> Result<()> {
    match validity {
        1..=31_536_000 => Ok(()),
        _ => Err(storage_driver_error(
            StorageErrorKind::Misconfigured,
            "Huawei CDN authentication validity must be between 1 and 31536000 seconds",
        )),
    }
}

pub(super) fn validated_cdn_domain(value: &str) -> Result<Url> {
    let invalid = || {
        storage_driver_error(
            StorageErrorKind::Misconfigured,
            "Huawei CDN domain must be an HTTPS hostname without credentials, non-default port, path, query, fragment, backslashes, or control characters",
        )
    };
    if value.chars().any(char::is_control) || value.contains('\\') {
        return Err(invalid());
    }
    // Check the original path before Url folds dot segments, including %2e.
    let value = value.trim();
    let raw: http::Uri = value.parse().map_err(|_| invalid())?;
    if raw
        .authority()
        .is_some_and(|authority| authority.as_str().contains('@'))
    {
        return Err(invalid());
    }
    if !matches!(raw.path(), "" | "/") {
        return Err(invalid());
    }
    let url = Url::parse(value).map_err(|_| invalid())?;
    match (
        url.scheme(),
        url.username(),
        url.password(),
        url.port(),
        url.query(),
        url.fragment(),
        url.host(),
    ) {
        ("https", "", None, None, None, None, Some(url::Host::Domain(host)))
            if host != "localhost" =>
        {
            Ok(url)
        }
        _ => Err(invalid()),
    }
}

pub(super) struct CdnConfig {
    pub domain: Url,
    pub auth_key: String,
    pub validity_seconds: u32,
    pub auth_param: String,
    pub base_path: String,
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
                    "Huawei CDN signing clock is before Unix epoch",
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
        let validity = u64::from(self.validity_seconds);
        let seconds = expires.as_secs().min(validity);
        if seconds == 0 {
            return Err(storage_driver_error(
                StorageErrorKind::Misconfigured,
                "Huawei CDN link lifetime must be at least one second",
            ));
        }
        let deadline = now.checked_add(seconds).ok_or_else(|| {
            storage_driver_error(
                StorageErrorKind::Misconfigured,
                "Huawei CDN expiration overflow",
            )
        })?;
        // The edge adds console validity to the timestamp. Backdate a shorter
        // caller lifetime rather than returning a URL that outlives expires_at.
        let timestamp = deadline
            .checked_sub(validity)
            .filter(|value| *value > 0)
            .ok_or_else(|| {
                storage_driver_error(
                    StorageErrorKind::Misconfigured,
                    "Huawei CDN signing timestamp is invalid",
                )
            })?;
        let expires_at = i64::try_from(deadline)
            .ok()
            .and_then(|value| DateTime::<Utc>::from_timestamp(value, 0))
            .ok_or_else(|| {
                storage_driver_error(
                    StorageErrorKind::Misconfigured,
                    "Huawei CDN expiration is out of range",
                )
            })?;
        let key = aster_drive_storage::object_key::join_key_prefix(&self.base_path, path);
        if path.trim_matches('/').is_empty()
            || key.split('/').any(|part| matches!(part, "." | ".."))
            || key.chars().any(char::is_control)
            || key.contains('\\')
        {
            return Err(storage_driver_error(
                StorageErrorKind::Misconfigured,
                "Huawei CDN object path is invalid",
            ));
        }
        let mut url = self.domain.clone();
        url.path_segments_mut()
            .map_err(|_| {
                storage_driver_error(
                    StorageErrorKind::Misconfigured,
                    "Huawei CDN domain cannot be used as a path base",
                )
            })?
            .pop_if_empty()
            .extend(key.split('/'));
        let signature = hex::encode(Sha256::digest(
            format!("{}-{timestamp}-{nonce}-0-{}", url.path(), self.auth_key).as_bytes(),
        ));
        url.query_pairs_mut().append_pair(
            &self.auth_param,
            &format!("{timestamp}-{nonce}-0-{signature}"),
        );
        Ok(Some(DirectDownloadRequest::preauthenticated_url(
            url,
            Some(expires_at),
        )))
    }
}

#[cfg(test)]
mod tests;
