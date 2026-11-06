//! Account-wide Blob service CORS. Azure selects the first origin/method match;
//! omitted service properties remain unchanged by Set Blob Service Properties.
//! https://learn.microsoft.com/rest/api/storageservices/set-blob-service-properties
//! https://learn.microsoft.com/rest/api/storageservices/cross-origin-resource-sharing--cors--support-for-the-azure-storage-services

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use aster_drive_storage::{
    MapStorageErr, Result, StorageError, StorageErrorKind, storage_driver_error,
};
use aster_forge_http::read_reqwest_body_limited;
use aster_forge_utils::net::is_loopback_host;
use aster_forge_xml::{BorrowedDocument, ParseOptions, XmlStreamWriter};
use reqwest::{Method, StatusCode, Url};
use serde::Serialize;

use super::AzureBlobDriver;

#[cfg(test)]
mod tests;

const AZURE_CORS_XML_MAX_BYTES: usize = 64 * 1024;
const AZURE_CORS_RULE_FIELDS: [&str; 5] = [
    "AllowedOrigins",
    "AllowedMethods",
    "AllowedHeaders",
    "ExposedHeaders",
    "MaxAgeInSeconds",
];

#[derive(Debug, Clone, PartialEq, Eq)]
struct AzureBlobCorsRule(BTreeMap<String, String>);

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AzureBlobCorsApplyResult {
    pub account: String,
    pub service_endpoint: String,
    pub scope: &'static str,
    pub allowed_origins: Vec<String>,
    pub changed: bool,
    pub preserved_rule_count: usize,
    pub request_id: Option<String>,
}

fn invalid_cors_configuration(message: &str) -> StorageError {
    storage_driver_error(
        StorageErrorKind::Misconfigured,
        format!(
            "Azure Blob CORS: {message}. Review Blob service CORS in Azure Portal before retrying"
        ),
    )
}

fn cors_verification_error(error: StorageError, write_request_id: Option<&str>) -> StorageError {
    storage_driver_error(
        error.kind(),
        format!(
            "Azure Blob accepted the CORS write but verification failed: {}. Write request_id={}. Inspect Blob service CORS before retrying",
            error.message(),
            write_request_id.unwrap_or("unavailable"),
        ),
    )
}

impl AzureBlobDriver {
    fn blob_service_properties_url(&self) -> Result<Url> {
        let mut url = Url::parse(&self.endpoint)
            .map_err(|_| invalid_cors_configuration("invalid service endpoint"))?;
        let host = url
            .host_str()
            .ok_or_else(|| invalid_cors_configuration("missing service host"))?;
        let loopback = is_loopback_host(host);
        let public_host = [
            "blob.core.windows.net",
            "blob.core.chinacloudapi.cn",
            "blob.core.usgovcloudapi.net",
            "blob.core.cloudapi.de",
        ]
        .iter()
        .any(|suffix| host == format!("{}.{suffix}", self.account_name));
        let emulator_path = format!("/{}", self.account_name);
        match &url {
            url if !url.username().is_empty()
                || url.password().is_some()
                || url.query().is_some()
                || url.fragment().is_some() =>
            {
                return Err(invalid_cors_configuration(
                    "use the account's primary Blob service endpoint, without container, prefix, SAS, CDN or Front Door",
                ));
            }
            _ => {}
        }
        match loopback {
            true if url.path().trim_end_matches('/') == emulator_path => {
                url.set_path(&format!("{emulator_path}/"));
            }
            false
                if public_host
                    && url.scheme() == "https"
                    && url.port().is_none()
                    && url.path() == "/" => {}
            _ => {
                return Err(invalid_cors_configuration(
                    "use the account's primary Blob service endpoint, without container, prefix, SAS, CDN or Front Door",
                ));
            }
        }
        url.set_query(Some("restype=service&comp=properties"));
        Ok(url)
    }

    async fn request_blob_service_properties(
        &self,
        client: &aster_forge_http::HttpClient,
        method: Method,
        body: Option<String>,
    ) -> Result<(String, Option<String>)> {
        let url = self.blob_service_properties_url()?;
        let date = chrono::Utc::now()
            .format("%a, %d %b %Y %H:%M:%S GMT")
            .to_string();
        let content_type = match body {
            Some(_) => "application/xml",
            None => "",
        };
        let mut request = client.request(method.clone(), url);
        request = match body {
            Some(body) => request.header("Content-Type", content_type).body(body),
            None => request,
        };
        let mut request = request.build().map_err(|_| {
            storage_driver_error(
                StorageErrorKind::Misconfigured,
                "build Azure Blob service properties request",
            )
        })?;
        self.sign_blob_service_properties_request(&mut request, &date)?;
        let response = client.execute(request).await.map_err(|_| {
            storage_driver_error(
                StorageErrorKind::Transient,
                "Azure Blob service properties request failed; check endpoint connectivity",
            )
        })?;
        let status = response.status();
        let request_id = response
            .headers()
            .get("x-ms-request-id")
            .and_then(|v| v.to_str().ok())
            .filter(|v| v.len() <= 128 && v.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
            .map(str::to_owned);
        // Set Blob Service Properties returns 202 with no response body.
        match status {
            status if method == Method::PUT && status.is_success() => {
                return Ok((String::new(), request_id));
            }
            _ => {}
        }
        let bytes = match read_reqwest_body_limited(
            response,
            "Azure Blob service properties",
            AZURE_CORS_XML_MAX_BYTES,
            |message| storage_driver_error(StorageErrorKind::Unknown, message),
        )
        .await
        {
            Ok(bytes) => bytes,
            Err(_) if !status.is_success() => {
                return Err(azure_cors_response_error(status, "", request_id.as_deref()));
            }
            Err(error) => return Err(error),
        };
        match status.is_success() {
            false => Err(azure_cors_response_error(
                status,
                &String::from_utf8_lossy(&bytes),
                request_id.as_deref(),
            )),
            true => String::from_utf8(bytes)
                .map(|body| (body, request_id))
                .map_err(|_| invalid_cors_configuration("invalid service properties encoding")),
        }
    }

    pub(crate) async fn configure_asterdrive_cors(
        &self,
        origins: &[String],
    ) -> Result<AzureBlobCorsApplyResult> {
        let mut service_endpoint = self.blob_service_properties_url()?;
        service_endpoint.set_query(None);
        // CORS management exists only for this explicit action. Ordinary driver
        // construction and object I/O never build or retain this client.
        let client =
            aster_drive_outbound::ClientConfig::new(aster_drive_outbound::Profile::ProviderControl)
                .connect_timeout(Duration::from_secs(5))
                .timeout(Duration::from_secs(30))
                .build()
                .map_err(|_| {
                    storage_driver_error(
                        StorageErrorKind::Misconfigured,
                        "build Azure Blob CORS action client",
                    )
                })?;
        let (body, request_id) = self
            .request_blob_service_properties(&client, Method::GET, None)
            .await?;
        let mut rules = parse_cors_configuration(&body)?;
        let mut result = AzureBlobCorsApplyResult {
            account: self.account_name.clone(),
            service_endpoint: service_endpoint.to_string(),
            scope: "account_blob_service_all_containers",
            allowed_origins: origins.to_vec(),
            changed: false,
            preserved_rule_count: rules.len(),
            request_id,
        };
        let desired = asterdrive_cors_rule(origins);
        match needs_asterdrive_cors_rule(&rules, &desired)? {
            false => return Ok(result),
            true => rules.push(desired),
        }
        let xml = build_cors_configuration_xml(&rules)?;
        let (_, request_id) = self
            .request_blob_service_properties(&client, Method::PUT, Some(xml))
            .await?;
        result.changed = true;
        result.request_id = request_id;
        let (body, read_request_id) = self
            .request_blob_service_properties(&client, Method::GET, None)
            .await
            .map_err(|error| cors_verification_error(error, result.request_id.as_deref()))?;
        let observed = parse_cors_configuration(&body)
            .map_err(|error| cors_verification_error(error, result.request_id.as_deref()))?;
        match observed.len() == rules.len()
            && observed
                .iter()
                .zip(&rules)
                .all(|(observed, expected)| cors_rules_equivalent(observed, expected))
        {
            true => result.request_id = result.request_id.or(read_request_id),
            false => {
                return Err(cors_verification_error(
                    storage_driver_error(
                        StorageErrorKind::Precondition,
                        "Blob service CORS readback differs; avoid concurrent CORS edits",
                    ),
                    result.request_id.as_deref(),
                ));
            }
        }
        Ok(result)
    }
}

fn asterdrive_cors_rule(origins: &[String]) -> AzureBlobCorsRule {
    AzureBlobCorsRule(AZURE_CORS_RULE_FIELDS.into_iter().zip([
        origins.join(","),
        "PUT,GET,HEAD".into(),
        "content-type,range,x-ms-blob-type,x-ms-version".into(),
        "ETag,Content-Length,Content-Range,Content-Disposition,Accept-Ranges,x-ms-request-id".into(),
        "600".into(),
    ]).map(|(key, value)| (key.into(), value)).collect())
}

fn cors_rule_values(rule: &AzureBlobCorsRule, name: &str) -> BTreeSet<String> {
    rule.0[name]
        .split(',')
        .map(str::trim)
        .map(|value| match name {
            "AllowedHeaders" | "ExposedHeaders" => value.to_ascii_lowercase(),
            _ => value.to_owned(),
        })
        .collect()
}

fn cors_rules_equivalent(left: &AzureBlobCorsRule, right: &AzureBlobCorsRule) -> bool {
    AZURE_CORS_RULE_FIELDS.iter().all(|name| match *name {
        "MaxAgeInSeconds" => {
            left.0[*name].parse::<u32>().ok() == right.0[*name].parse::<u32>().ok()
        }
        _ => cors_rule_values(left, name) == cors_rule_values(right, name),
    })
}

fn cors_origin_matches(pattern: &str, origin: &str) -> bool {
    match pattern {
        "*" => true,
        pattern if pattern == origin => true,
        pattern => match pattern.split_once('*') {
            Some((prefix, suffix)) => {
                origin.starts_with(prefix)
                    && origin.ends_with(suffix)
                    && origin.len() >= prefix.len() + suffix.len()
            }
            None => false,
        },
    }
}

fn needs_asterdrive_cors_rule(
    rules: &[AzureBlobCorsRule],
    desired: &AzureBlobCorsRule,
) -> Result<bool> {
    let desired_origins = cors_rule_values(desired, "AllowedOrigins");
    let desired_methods = cors_rule_values(desired, "AllowedMethods");
    // Azure selects the first rule matching origin and method before checking
    // request headers. Appending cannot repair a shadowing earlier rule.
    for rule in rules {
        match rule {
            rule if cors_rules_equivalent(rule, desired) => return Ok(false),
            rule if cors_rule_values(rule, "AllowedOrigins")
                .iter()
                .any(|pattern| {
                    desired_origins
                        .iter()
                        .any(|origin| cors_origin_matches(pattern, origin))
                })
                && !cors_rule_values(rule, "AllowedMethods").is_disjoint(&desired_methods) =>
            {
                return Err(invalid_cors_configuration(
                    "an earlier rule overlaps the requested origins and methods; automatic merging could change rule evaluation",
                ));
            }
            _ => {}
        }
    }
    Ok(true)
}

fn parse_cors_configuration(body: &str) -> Result<Vec<AzureBlobCorsRule>> {
    let doc = BorrowedDocument::parse_with_options(
        body.as_bytes(),
        &ParseOptions::new()
            .max_size(AZURE_CORS_XML_MAX_BYTES)
            .max_depth(16)
            .max_elements(1000),
    )
    .map_err(|_| invalid_cors_configuration("invalid service properties XML"))?;
    let root = doc.root();
    match root.name() {
        "StorageServiceProperties" => {}
        _ => {
            return Err(invalid_cors_configuration(
                "unexpected service properties root",
            ));
        }
    }
    let cors: Vec<_> = root
        .child_elements()
        .filter(|e| e.name() == "Cors")
        .collect();
    let cors = match cors.as_slice() {
        [cors] => cors,
        _ => {
            return Err(invalid_cors_configuration(
                "service properties must contain exactly one Cors element",
            ));
        }
    };
    match (cors.namespace(), cors.attributes().next()) {
        (None, None) => {}
        _ => {
            return Err(invalid_cors_configuration(
                "unknown CORS attributes or namespace",
            ));
        }
    }
    let mut rules = Vec::new();
    for element in cors.child_elements() {
        match (
            element.name(),
            element.namespace(),
            element.attributes().next(),
        ) {
            ("CorsRule", None, None) => {}
            _ => return Err(invalid_cors_configuration("unknown Cors element")),
        }
        let mut fields = BTreeMap::new();
        for child in element.child_elements() {
            match child {
                child
                    if AZURE_CORS_RULE_FIELDS.contains(&child.name())
                        && child.namespace().is_none()
                        && child.attributes().next().is_none()
                        && child.child_elements().next().is_none() => {}
                _ => {
                    return Err(invalid_cors_configuration(
                        "unknown or duplicate CORS rule field",
                    ));
                }
            }
            match fields.insert(
                child.name().to_owned(),
                child.text().unwrap_or_default().trim().to_owned(),
            ) {
                None => {}
                Some(_) => {
                    return Err(invalid_cors_configuration(
                        "unknown or duplicate CORS rule field",
                    ));
                }
            }
        }
        match fields.get("MaxAgeInSeconds") {
            Some(age)
                if fields.len() == AZURE_CORS_RULE_FIELDS.len() && age.parse::<u32>().is_ok() => {}
            _ => return Err(invalid_cors_configuration("incomplete CORS rule")),
        }
        rules.push(AzureBlobCorsRule(fields));
    }
    validate_cors_configuration_limits(&rules)?;
    Ok(rules)
}

fn validate_cors_configuration_limits(rules: &[AzureBlobCorsRule]) -> Result<()> {
    let size = rules
        .iter()
        .flat_map(|rule| rule.0.values())
        .map(String::len)
        .sum::<usize>();
    match (rules.len(), size) {
        (0..=5, 0..=2048) => {}
        _ => {
            return Err(invalid_cors_configuration(
                "merging exceeds the five-rule or 2 KiB limit",
            ));
        }
    }
    for rule in rules {
        for name in ["AllowedOrigins", "AllowedHeaders", "ExposedHeaders"] {
            let entries: Vec<_> = rule.0[name]
                .split(',')
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .collect();
            let prefixed = entries.iter().filter(|value| value.contains('*')).count();
            match entries.iter().find(|entry| entry.chars().count() > 256) {
                Some(_) => Err(invalid_cors_configuration(
                    "CORS origin or header limits exceeded",
                )),
                None => Ok(()),
            }?;
            match name {
                "AllowedOrigins" if entries.len() > 64 => {
                    return Err(invalid_cors_configuration(
                        "CORS origin or header limits exceeded",
                    ));
                }
                "AllowedHeaders" | "ExposedHeaders"
                    if entries.len() - prefixed > 64 || prefixed > 2 =>
                {
                    return Err(invalid_cors_configuration(
                        "CORS origin or header limits exceeded",
                    ));
                }
                _ => {}
            }
        }
    }
    Ok(())
}

fn build_cors_configuration_xml(rules: &[AzureBlobCorsRule]) -> Result<String> {
    validate_cors_configuration_limits(rules)?;
    let mut bytes = Vec::new();
    let mut writer = XmlStreamWriter::new(&mut bytes).map_storage_err_ctx(
        StorageErrorKind::Misconfigured,
        "create Azure Blob CORS XML writer",
    )?;
    // Only Cors is sent: Azure leaves all omitted service properties unchanged.
    let write_configuration = (|| -> std::result::Result<(), aster_forge_xml::Error> {
        writer.start("StorageServiceProperties")?;
        writer.start("Cors")?;
        for rule in rules {
            writer.start("CorsRule")?;
            for field in AZURE_CORS_RULE_FIELDS {
                writer.start(field)?;
                writer.text(&rule.0[field])?;
                writer.end_element()?;
            }
            writer.end_element()?;
        }
        writer.end_element()?;
        writer.end_element()?;
        Ok(())
    })();
    write_configuration
        .map_storage_err_ctx(StorageErrorKind::Misconfigured, "write Azure Blob CORS XML")?;
    writer.finish().map_storage_err_ctx(
        StorageErrorKind::Misconfigured,
        "finish Azure Blob CORS XML",
    )?;
    String::from_utf8(bytes).map_storage_err_ctx(
        StorageErrorKind::Misconfigured,
        "encode Azure Blob CORS XML",
    )
}

fn azure_cors_response_error(
    status: StatusCode,
    body: &str,
    request_id: Option<&str>,
) -> StorageError {
    let code = BorrowedDocument::parse_with_options(
        body.as_bytes(),
        &ParseOptions::new()
            .max_size(AZURE_CORS_XML_MAX_BYTES)
            .max_depth(8)
            .max_elements(100),
    )
    .ok()
    .and_then(|doc| {
        doc.root()
            .child_elements()
            .find(|e| e.name() == "Code")
            .and_then(|e| e.text().map(|v| v.to_string()))
    })
    .filter(|v| v.len() <= 100 && v.chars().all(|c| c.is_ascii_alphanumeric()))
    .unwrap_or_else(|| "Unknown".into());
    let kind = match status {
        StatusCode::BAD_REQUEST => StorageErrorKind::Misconfigured,
        StatusCode::TOO_MANY_REQUESTS => StorageErrorKind::RateLimited,
        status => AzureBlobDriver::classify_azure_http_error(
            azure_core::http::StatusCode::from(status.as_u16()),
            Some(&code),
        ),
    };
    let hint = match status {
        StatusCode::FORBIDDEN => {
            " Verify the account name/key and that Shared Key access is allowed; Get/Set Blob Service Properties require account-level service access. Container SAS and Blob Data Contributor alone are insufficient. This action does not support Entra ID."
        }
        _ => "",
    };
    storage_driver_error(
        kind,
        format!(
            "Azure Blob service properties failed: HTTP {status}, code={code}, request_id={}.{hint}",
            request_id.unwrap_or("unavailable")
        ),
    )
}
