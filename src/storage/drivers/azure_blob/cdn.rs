//! Azure Front Door uses Blob service SAS, including the response overrides.
//! The edge must forward and cache by the entire query string. Cache retention
//! is independent of SAS expiry; cache hits do not reauthenticate at Blob.
//! https://learn.microsoft.com/azure/frontdoor/scenario-storage-blobs
//! https://learn.microsoft.com/azure/frontdoor/front-door-caching

use aster_drive_storage::{Result, StorageErrorKind, storage_driver_error};
use serde::{Deserialize, Serialize};
use url::Url;

use super::AzureBlobDriverConfig;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AzureBlobDownloadDelivery {
    #[default]
    AzureBlobOrigin,
    AzureFrontDoorSas,
}

pub(crate) const MAX_CACHE_MAX_AGE_SECONDS: u32 = 31_622_400;

#[derive(Clone)]
pub(crate) struct FrontDoorConfig {
    pub base_url: Url,
    pub cache_max_age_seconds: u32,
}

impl FrontDoorConfig {
    pub(crate) fn from_config(config: &AzureBlobDriverConfig) -> Result<Option<Self>> {
        if config.download_delivery == AzureBlobDownloadDelivery::AzureBlobOrigin {
            return Ok(None);
        }
        let invalid = || {
            storage_driver_error(
                StorageErrorKind::Misconfigured,
                "Azure Front Door download address must be an HTTPS domain without credentials, path, query or fragment",
            )
        };
        let value = config.download_base_url.trim();
        let raw: http::Uri = value.parse().map_err(|_| invalid())?;
        if !matches!(raw.path(), "" | "/") || value.chars().any(char::is_control) {
            return Err(invalid());
        }
        let base_url = Url::parse(value).map_err(|_| invalid())?;
        if base_url.scheme() != "https"
            || base_url.host_str().is_none()
            || !base_url.username().is_empty()
            || base_url.password().is_some()
            || base_url.query().is_some()
            || base_url.fragment().is_some()
        {
            return Err(invalid());
        }
        if config.azure_cdn_cache_max_age_seconds > MAX_CACHE_MAX_AGE_SECONDS {
            return Err(storage_driver_error(
                StorageErrorKind::Misconfigured,
                "Azure CDN cache time must be between 0 and 31622400 seconds",
            ));
        }
        Ok(Some(Self {
            base_url,
            cache_max_age_seconds: config.azure_cdn_cache_max_age_seconds,
        }))
    }
}

#[cfg(test)]
mod tests;
