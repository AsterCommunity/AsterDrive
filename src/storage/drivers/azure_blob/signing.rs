//! Azure Blob authentication belongs to the driver, for object I/O and service actions.
//!
//! Service SAS signs a scoped resource and expiry into a query token. Shared Key
//! authenticates an HTTP request, including its method, headers and service query.
//! These are distinct Azure formats; both use the SDK's HMAC implementation.
//! https://learn.microsoft.com/rest/api/storageservices/create-service-sas
//! https://learn.microsoft.com/rest/api/storageservices/authorize-with-shared-key

use std::time::Duration;

use aster_drive_storage::{Result, StorageErrorKind, storage_driver_error};
use azure_core::{credentials::Secret, hmac::hmac_sha256};
use chrono::{SecondsFormat, Utc};

use super::{AZURE_STORAGE_VERSION, AzureBlobDriver, AzureBlobResponseHeaders};

impl AzureBlobDriver {
    pub(super) fn service_sas_query(
        &self,
        blob_name: Option<&str>,
        permissions: &str,
        expires: Duration,
        response_headers: AzureBlobResponseHeaders<'_>,
    ) -> Result<String> {
        let starts_on = Utc::now() - chrono::Duration::minutes(5);
        let expires_on = Utc::now()
            + chrono::Duration::from_std(expires).map_err(|error| {
                storage_driver_error(
                    StorageErrorKind::Misconfigured,
                    format!("invalid Azure Blob SAS expiry: {error}"),
                )
            })?;
        let signed_start = starts_on.to_rfc3339_opts(SecondsFormat::Secs, true);
        let signed_expiry = expires_on.to_rfc3339_opts(SecondsFormat::Secs, true);
        let canonicalized_resource = match blob_name {
            Some(blob_name) => format!(
                "/blob/{}/{}/{}",
                self.account_name, self.container, blob_name
            ),
            None => format!("/blob/{}/{}", self.account_name, self.container),
        };
        let signed_resource = if blob_name.is_some() { "b" } else { "c" };
        let signed_protocol = self.sas_protocol();

        // Service SAS string-to-sign for version 2020-12-06+.
        // Empty fields are significant and must remain as blank lines.
        let string_to_sign = [
            permissions,             // signed permissions
            &signed_start,           // signed start
            &signed_expiry,          // signed expiry
            &canonicalized_resource, // canonicalized resource
            "",                      // signed identifier
            "",                      // signed IP
            signed_protocol,         // signed protocol
            AZURE_STORAGE_VERSION,   // signed version
            signed_resource,         // signed resource
            "",                      // signed snapshot time
            "",                      // signed encryption scope
            response_headers.cache_control.unwrap_or(""),
            response_headers.content_disposition.unwrap_or(""),
            "", // rsce
            "", // rscl
            response_headers.content_type.unwrap_or(""),
        ]
        .join("\n");
        let signature = hmac_sha256(&string_to_sign, &Secret::new(self.account_key.clone()))
            .map_err(|error| {
                storage_driver_error(
                    StorageErrorKind::Auth,
                    format!("sign Azure Blob SAS failed: {error}"),
                )
            })?;

        let mut serializer = url::form_urlencoded::Serializer::new(String::new());
        if let Some(value) = response_headers.cache_control {
            serializer.append_pair("rscc", value);
        }
        if let Some(value) = response_headers.content_disposition {
            serializer.append_pair("rscd", value);
        }
        if let Some(value) = response_headers.content_type {
            serializer.append_pair("rsct", value);
        }
        serializer
            .append_pair("sv", AZURE_STORAGE_VERSION)
            .append_pair("spr", signed_protocol)
            .append_pair("st", &signed_start)
            .append_pair("se", &signed_expiry)
            .append_pair("sr", signed_resource)
            .append_pair("sp", permissions)
            .append_pair("sig", &signature);
        Ok(serializer.finish())
    }
}

impl AzureBlobDriver {
    /// Signs the actual service-properties request so headers and body length
    /// cannot drift from the values sent by the HTTP client.
    pub(super) fn sign_blob_service_properties_request(
        &self,
        request: &mut reqwest::Request,
        date: &str,
    ) -> Result<()> {
        let content_type = request
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .map(|value| value.to_str())
            .transpose()
            .map_err(|_| {
                storage_driver_error(
                    StorageErrorKind::Misconfigured,
                    "invalid Azure Blob request content type",
                )
            })?
            .unwrap_or("");
        let length = match request.body() {
            None => String::new(),
            Some(body) => match body.as_bytes() {
                Some([]) => String::new(),
                Some(bytes) => bytes.len().to_string(),
                None => {
                    return Err(storage_driver_error(
                        StorageErrorKind::Misconfigured,
                        "Azure Blob service properties require a buffered XML body",
                    ));
                }
            },
        };
        let standard = [
            request.method().as_str(),
            "",
            "",
            &length,
            "",
            content_type,
            "",
            "",
            "",
            "",
            "",
            "",
        ]
        .join("\n");
        let canonical_headers = format!("x-ms-date:{date}\nx-ms-version:{AZURE_STORAGE_VERSION}\n");
        let resource = format!(
            "/{}{}\ncomp:properties\nrestype:service",
            self.account_name,
            request.url().path()
        );
        let signature = hmac_sha256(
            &format!("{standard}\n{canonical_headers}{resource}"),
            &Secret::new(self.account_key.clone()),
        )
        .map_err(|_| {
            storage_driver_error(
                StorageErrorKind::Auth,
                "unable to sign Azure Blob service properties request with the account key",
            )
        })?;
        for (name, value) in [
            ("x-ms-date", date.to_owned()),
            ("x-ms-version", AZURE_STORAGE_VERSION.to_owned()),
            (
                "authorization",
                format!("SharedKey {}:{signature}", self.account_name),
            ),
        ] {
            request.headers_mut().insert(
                reqwest::header::HeaderName::from_static(name),
                reqwest::header::HeaderValue::from_str(&value).map_err(|_| {
                    storage_driver_error(
                        StorageErrorKind::Misconfigured,
                        "invalid Azure Blob authentication header",
                    )
                })?,
            );
        }
        Ok(())
    }
}
