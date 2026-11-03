//! Front Door Standard/Premium with client service SAS, unchanged origin paths,
//! and caching disabled. SAS response overrides are authenticated by Blob.
//! https://learn.microsoft.com/azure/frontdoor/scenario-storage-blobs
//! https://learn.microsoft.com/rest/api/storageservices/create-service-sas

use std::time::Duration;

use aster_drive_storage::{DirectDownloadOptions, Result, StorageErrorKind, storage_driver_error};
use azure_core::http::Url;
use serde::{Deserialize, Serialize};

use super::{
    AzureBlobDriver, AzureBlobDriverConfig, percent_encode_blob_path, percent_encode_path_segment,
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AzureBlobDownloadDelivery {
    #[default]
    BlobOrigin,
    FrontDoorClientSas,
}

fn invalid_config() -> aster_drive_storage::StorageError {
    storage_driver_error(
        StorageErrorKind::Misconfigured,
        "Azure download delivery requires an HTTPS origin URL without credentials, non-default port, path, query, fragment, backslashes, or control characters; static website endpoints are unsupported",
    )
}

pub(super) fn validate_blob_path(value: &str) -> Result<()> {
    if value.contains('\\')
        || value.chars().any(char::is_control)
        || value
            .split('/')
            .any(|segment| segment == "." || segment == "..")
    {
        return Err(storage_driver_error(
            StorageErrorKind::Misconfigured,
            "Azure Blob path contains unsafe segments or control characters",
        ));
    }
    Ok(())
}

pub(super) fn delivery_base_url(value: &str) -> Result<Url> {
    if value.contains('\\') || value.chars().any(char::is_control) {
        return Err(invalid_config());
    }
    let value = value.trim();
    let (_, authority_and_path) = value.split_once("://").ok_or_else(invalid_config)?;
    let url = Url::parse(value).map_err(|_| invalid_config())?;
    // Require an origin, including for custom domains. No implicit path rewrite.
    let authority = authority_and_path.split('/').next().unwrap_or("");
    let raw_path = authority_and_path
        .split_once('/')
        .map(|(_, path)| path)
        .unwrap_or("");
    if url.scheme() != "https"
        || url.host_str().is_none()
        || authority.contains('@')
        || url.port().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !raw_path.is_empty()
        || url.path() != "/"
        || super::is_static_website_host(url.host_str().unwrap_or(""))
    {
        return Err(invalid_config());
    }
    Ok(url)
}

impl AzureBlobDriver {
    pub fn validate_delivery_config(config: &AzureBlobDriverConfig) -> Result<()> {
        validate_blob_path(&config.base_path)?;
        if config.download_delivery == AzureBlobDownloadDelivery::FrontDoorClientSas {
            delivery_base_url(
                config
                    .download_base_url
                    .as_deref()
                    .ok_or_else(invalid_config)?,
            )?;
            // A Front Door origin must be a Blob service root, never an Azurite
            // account path or a website endpoint. Backend operations stay here.
            delivery_base_url(&config.endpoint)?;
        }
        Ok(())
    }

    pub(super) fn download_url(
        &self,
        path: &str,
        expires: Duration,
        mut options: DirectDownloadOptions,
    ) -> Result<Url> {
        let key = self.full_key(path);
        validate_blob_path(&key)?;
        for value in [
            &options.response_cache_control,
            &options.response_content_disposition,
            &options.response_content_type,
        ]
        .into_iter()
        .flatten()
        {
            if value.chars().any(char::is_control) {
                return Err(storage_driver_error(
                    StorageErrorKind::Misconfigured,
                    "Azure Blob response header contains control characters",
                ));
            }
        }
        let endpoint = match self.download_delivery {
            AzureBlobDownloadDelivery::BlobOrigin => &self.endpoint,
            AzureBlobDownloadDelivery::FrontDoorClientSas => {
                // Even a valid SAS may outlive its permission in an edge cache.
                // Sign no-store and require the operator to disable route cache.
                options.response_cache_control = Some("private, no-store".into());
                self.download_base_url.as_ref().ok_or_else(invalid_config)?
            }
        };
        Url::parse(&format!(
            "{}/{}/{}?{}",
            endpoint,
            percent_encode_path_segment(&self.container),
            percent_encode_blob_path(&key),
            self.service_sas_query(Some(&key), "r", expires, &options)?
        ))
        .map_err(|_| invalid_config())
    }
}
