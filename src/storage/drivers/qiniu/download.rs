use super::{QiniuDriver, invalid};
use aster_drive_storage::traits::extensions::DirectDownloadStorageDriver;
use aster_drive_storage::{DirectDownloadOptions, DirectDownloadRequest, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use url::Url;

// Go SDK storagev2/downloader/signers.go and storage/bucket_get.go use 3 minutes.
// Default Kodo source domains reject distant expiry times (including one hour).
pub const fn default_download_auth_validity_seconds() -> u32 {
    180
}

fn default_source_ttl(domain: &str, requested: Duration) -> Duration {
    let is_default = Url::parse(domain).is_ok_and(|url| {
        url.host_str()
            .and_then(|host| host.strip_suffix(".qiniucs.com"))
            .is_some_and(|host| {
                host.rsplit('.')
                    .next()
                    .is_some_and(|service| service.starts_with("kodo-"))
            })
    });
    if is_default {
        requested.min(Duration::from_secs(300))
    } else {
        requested
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QiniuDownloadDelivery {
    #[default]
    Origin,
    QiniuCdn,
}

pub(super) fn validate_domain(value: &str) -> Result<Url> {
    let url = Url::parse(value.trim())
        .map_err(|_| invalid("configure a bound Kodo HTTPS download origin/domain"))?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
    {
        return Err(invalid(
            "Kodo download domains must be HTTPS origins without credentials, path, query or fragment; use base_path for the object prefix",
        ));
    }
    Ok(url)
}

impl QiniuDriver {
    pub(super) fn download_ttl(&self, domain: &str, requested: Duration) -> Duration {
        default_source_ttl(
            domain,
            requested.min(Duration::from_secs(u64::from(
                self.config.download_auth_validity_seconds,
            ))),
        )
    }

    pub(super) fn source_read_deadline(&self) -> Result<i64> {
        let ttl = self.download_ttl(
            &self.config.download_origin,
            Duration::from_secs(u64::from(self.config.download_auth_validity_seconds)),
        );
        let seconds =
            i64::try_from(ttl.as_secs()).map_err(|_| invalid("Kodo source expiry overflow"))?;
        chrono::Utc::now()
            .timestamp()
            .checked_add(seconds)
            .ok_or_else(|| invalid("Kodo source deadline overflow"))
    }
    pub(super) fn private_url(
        &self,
        domain: &str,
        path: &str,
        deadline: i64,
        query: &[(&str, &str)],
    ) -> Result<String> {
        let key = self.key(path)?;
        let mut url = Url::parse(domain).map_err(|_| invalid("invalid Kodo download origin"))?;
        url.path_segments_mut()
            .map_err(|_| invalid("invalid Kodo download origin"))?
            .clear()
            .extend(key.split('/'));
        url.query_pairs_mut()
            .extend_pairs(query.iter().copied())
            .append_pair("e", &deadline.to_string());
        // Go net/url.Values escapes '*', unlike WHATWG form serialization.
        // Keep filename* and special filenames byte-identical to the Go SDK.
        if let Some(query) = url.query().filter(|query| query.contains('*')) {
            let encoded = query.replace('*', "%2A");
            url.set_query(Some(&encoded));
        }
        let signature = self.sign(url.as_str().as_bytes())?;
        Ok(format!("{url}&token={signature}"))
    }
}

#[async_trait::async_trait]
impl DirectDownloadStorageDriver for QiniuDriver {
    async fn resolve_download_url(
        &self,
        path: &str,
        expires: Duration,
        options: DirectDownloadOptions,
    ) -> Result<Option<DirectDownloadRequest>> {
        if expires.as_secs() == 0 || expires > Duration::from_secs(7 * 24 * 3600) {
            return Err(invalid(
                "Kodo download TTL must be between 1 second and 7 days",
            ));
        }
        // Origin and CDN use native MIME/cache headers. Signed attname preserves
        // the logical attachment name for full GETs and Range requests alike.
        let attachment = match options.response_content_disposition.as_deref() {
            Some(value) => {
                reqwest::header::HeaderValue::from_str(value)
                    .map_err(|_| invalid("invalid Kodo download disposition"))?;
                value
                    .split(';')
                    .next()
                    .is_some_and(|kind| kind.trim().eq_ignore_ascii_case("attachment"))
            }
            None => true,
        };
        if options
            .download_name
            .as_deref()
            .is_some_and(|name| name.is_empty() || name.chars().any(char::is_control))
        {
            return Err(invalid("invalid Kodo download filename"));
        }
        let query = match (attachment, options.download_name.as_deref()) {
            (true, Some(name)) => vec![("attname", name)],
            (true, None) | (false, _) => Vec::new(),
        };
        let domain = match self.config.download_delivery {
            QiniuDownloadDelivery::Origin => self.config.download_origin.as_str(),
            QiniuDownloadDelivery::QiniuCdn => self
                .config
                .download_base_url
                .as_deref()
                .ok_or_else(|| invalid("Kodo CDN domain is required"))?,
        };
        let expires = self.download_ttl(domain, expires);
        let seconds =
            i64::try_from(expires.as_secs()).map_err(|_| invalid("Kodo download TTL overflow"))?;
        let deadline = chrono::Utc::now()
            .timestamp()
            .checked_add(seconds)
            .ok_or_else(|| invalid("Kodo download deadline overflow"))?;
        Ok(Some(DirectDownloadRequest::preauthenticated_url(
            self.private_url(domain, path, deadline, &query)?,
            chrono::DateTime::from_timestamp(deadline, 0),
        )))
    }
}
