use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use aster_forge_http::read_reqwest_body_limited;
use aster_forge_xml::{OwnedDocument, ParseOptions, XmlStreamWriter};
use azure_core::{credentials::Secret, hmac::hmac_sha256};
use reqwest::{Method, StatusCode, Url};
use serde::Serialize;

use crate::errors::{AsterError, MapAsterErr, Result};
use aster_drive_storage::error::{StorageErrorKind, storage_driver_error};

use super::{AZURE_STORAGE_VERSION, AzureBlobDriver};

#[cfg(test)]
mod tests;

const MAX_XML_BYTES: usize = 64 * 1024;
const RULE_FIELDS: [&str; 5] = [
    "AllowedOrigins",
    "AllowedMethods",
    "AllowedHeaders",
    "ExposedHeaders",
    "MaxAgeInSeconds",
];

#[derive(Debug, Clone, PartialEq, Eq)]
struct CorsRule(BTreeMap<String, String>);

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AzureCorsResult {
    pub account: String,
    pub service_endpoint: String,
    pub scope: &'static str,
    pub allowed_origins: Vec<String>,
    pub changed: bool,
    pub preserved_rule_count: usize,
    pub request_id: Option<String>,
}

fn invalid(message: &str) -> AsterError {
    storage_driver_error(StorageErrorKind::Misconfigured, format!("Azure Blob CORS: {message}. Review Blob service CORS in Azure Portal; existing rules were not replaced")).into()
}

impl AzureBlobDriver {
    pub(crate) fn cors_target(&self, origins: &[String]) -> Result<AzureCorsResult> {
        let mut url = self.cors_service_url()?;
        url.set_query(None);
        Ok(AzureCorsResult {
            account: self.account_name.clone(),
            service_endpoint: url.to_string(),
            scope: "account_blob_service_all_containers",
            allowed_origins: origins.to_vec(),
            changed: false,
            preserved_rule_count: 0,
            request_id: None,
        })
    }

    fn cors_service_url(&self) -> Result<Url> {
        let mut url =
            Url::parse(&self.endpoint).map_err(|_| invalid("invalid service endpoint"))?;
        let host = url
            .host_str()
            .ok_or_else(|| invalid("missing service host"))?;
        let loopback = host == "localhost"
            || host
                .parse::<std::net::IpAddr>()
                .is_ok_and(|ip| ip.is_loopback());
        let public_host = [
            "blob.core.windows.net",
            "blob.core.chinacloudapi.cn",
            "blob.core.usgovcloudapi.net",
            "blob.core.cloudapi.de",
        ]
        .iter()
        .any(|suffix| host == format!("{}.{suffix}", self.account_name));
        let emulator_path = format!("/{}", self.account_name);
        if !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || (!loopback
                && (!public_host
                    || url.scheme() != "https"
                    || url.port().is_some()
                    || url.path() != "/"))
            || (loopback && url.path().trim_end_matches('/') != emulator_path)
        {
            return Err(invalid(
                "use the account's primary Blob service endpoint, without container, prefix, SAS, CDN or Front Door",
            ));
        }
        if loopback {
            url.set_path(&format!("{emulator_path}/"));
        }
        url.set_query(Some("restype=service&comp=properties"));
        Ok(url)
    }

    async fn cors_request(
        &self,
        method: Method,
        body: Option<String>,
    ) -> Result<(String, Option<String>)> {
        let url = self.cors_service_url()?;
        let date = chrono::Utc::now()
            .format("%a, %d %b %Y %H:%M:%S GMT")
            .to_string();
        let content_type = if body.is_some() {
            "application/xml"
        } else {
            ""
        };
        let signature = shared_key_signature(
            &self.account_name,
            &self.account_key,
            method.as_str(),
            &url,
            &date,
            content_type,
            body.as_ref().map_or(0, String::len),
        )?;
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(30))
            .build()
            .map_aster_err(AsterError::storage_driver_error)?;
        let mut request = client
            .request(method, url)
            .header("x-ms-date", date)
            .header("x-ms-version", AZURE_STORAGE_VERSION)
            .header(
                "Authorization",
                format!("SharedKey {}:{signature}", self.account_name),
            );
        if let Some(body) = body {
            request = request.header("Content-Type", content_type).body(body);
        }
        let response = request.send().await.map_err(|_| {
            AsterError::from(storage_driver_error(
                StorageErrorKind::Transient,
                "Azure Blob service properties request failed; check endpoint connectivity",
            ))
        })?;
        let status = response.status();
        let request_id = response
            .headers()
            .get("x-ms-request-id")
            .and_then(|v| v.to_str().ok())
            .filter(|v| v.len() <= 128 && v.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
            .map(str::to_owned);
        let bytes = read_reqwest_body_limited(
            response,
            "Azure Blob service properties",
            MAX_XML_BYTES,
            AsterError::storage_driver_error,
        )
        .await?;
        if !status.is_success() {
            let body = String::from_utf8_lossy(&bytes);
            return Err(response_error(status, &body, request_id.as_deref()));
        }
        let body =
            String::from_utf8(bytes).map_err(|_| invalid("invalid service properties encoding"))?;
        Ok((body, request_id))
    }

    pub(crate) async fn configure_asterdrive_cors(
        &self,
        origins: &[String],
    ) -> Result<AzureCorsResult> {
        let mut result = self.cors_target(origins)?;
        let (body, request_id) = self.cors_request(Method::GET, None).await?;
        let mut rules = parse_rules(&body)?;
        result.preserved_rule_count = rules.len();
        result.request_id = request_id;
        let desired = desired_rule(origins);
        if !needs_append(&rules, &desired)? {
            return Ok(result);
        }
        rules.push(desired);
        let xml = rules_xml(&rules)?;
        let (_, request_id) = self.cors_request(Method::PUT, Some(xml)).await?;
        result.changed = true;
        result.request_id = request_id;
        let (body, _) = self.cors_request(Method::GET, None).await?;
        if parse_rules(&body)? != rules {
            return Err(storage_driver_error(
                StorageErrorKind::Precondition,
                "Azure Blob CORS changed during verification; review service CORS and retry",
            )
            .into());
        }
        Ok(result)
    }
}

fn shared_key_signature(
    account: &str,
    key: &str,
    method: &str,
    url: &Url,
    date: &str,
    content_type: &str,
    length: usize,
) -> Result<String> {
    let length = if length == 0 {
        String::new()
    } else {
        length.to_string()
    };
    let standard = [
        method,
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
    let resource = format!("/{account}{}\ncomp:properties\nrestype:service", url.path());
    hmac_sha256(
        &format!("{standard}\n{canonical_headers}{resource}"),
        &Secret::new(key.to_owned()),
    )
    .map_err(|_| invalid("unable to sign service properties request with the account key"))
}

fn desired_rule(origins: &[String]) -> CorsRule {
    CorsRule(RULE_FIELDS.into_iter().zip([
        origins.join(","),
        "PUT,GET,HEAD".into(),
        "content-type,range,x-ms-blob-type,x-ms-version".into(),
        "ETag,Content-Length,Content-Range,Content-Disposition,Accept-Ranges,x-ms-request-id".into(),
        "600".into(),
    ]).map(|(key, value)| (key.into(), value)).collect())
}

fn values(rule: &CorsRule, name: &str) -> BTreeSet<String> {
    rule.0[name]
        .split(',')
        .map(str::trim)
        .map(|value| {
            if matches!(name, "AllowedHeaders" | "ExposedHeaders") {
                value.to_ascii_lowercase()
            } else {
                value.to_owned()
            }
        })
        .collect()
}

fn equivalent(left: &CorsRule, right: &CorsRule) -> bool {
    RULE_FIELDS.iter().all(|name| {
        if *name == "MaxAgeInSeconds" {
            left.0[*name].parse::<u32>().ok() == right.0[*name].parse::<u32>().ok()
        } else {
            values(left, name) == values(right, name)
        }
    })
}

fn origin_matches(pattern: &str, origin: &str) -> bool {
    if pattern == "*" || pattern == origin {
        return true;
    }
    if let Some((prefix, suffix)) = pattern.split_once('*') {
        return origin.starts_with(prefix)
            && origin.ends_with(suffix)
            && origin.len() >= prefix.len() + suffix.len();
    }
    false
}

fn needs_append(rules: &[CorsRule], desired: &CorsRule) -> Result<bool> {
    for rule in rules {
        if equivalent(rule, desired) {
            return Ok(false);
        }
        if values(rule, "AllowedOrigins").iter().any(|pattern| {
            values(desired, "AllowedOrigins")
                .iter()
                .any(|origin| origin_matches(pattern, origin))
        }) && !values(rule, "AllowedMethods").is_disjoint(&values(desired, "AllowedMethods"))
        {
            return Err(invalid(
                "an earlier rule overlaps the requested origins and methods; automatic merging could change rule evaluation",
            ));
        }
    }
    Ok(true)
}

fn parse_rules(body: &str) -> Result<Vec<CorsRule>> {
    let doc = OwnedDocument::from_reader_with_options(
        body.as_bytes(),
        &ParseOptions::new()
            .max_size(MAX_XML_BYTES)
            .max_depth(16)
            .max_elements(1000),
    )
    .map_err(|_| invalid("invalid service properties XML"))?;
    let root = doc.root();
    if root.name() != "StorageServiceProperties" {
        return Err(invalid("unexpected service properties root"));
    }
    let cors: Vec<_> = root
        .child_elements()
        .filter(|e| e.name() == "Cors")
        .collect();
    if cors.len() != 1 {
        return Err(invalid(
            "service properties must contain exactly one Cors element",
        ));
    }
    if cors[0].namespace().is_some() || cors[0].attributes().next().is_some() {
        return Err(invalid("unknown CORS attributes or namespace"));
    }
    let mut rules = Vec::new();
    for element in cors[0].child_elements() {
        if element.name() != "CorsRule"
            || element.namespace().is_some()
            || element.attributes().next().is_some()
        {
            return Err(invalid("unknown Cors element"));
        }
        let mut fields = BTreeMap::new();
        for child in element.child_elements() {
            if !RULE_FIELDS.contains(&child.name())
                || child.namespace().is_some()
                || child.attributes().next().is_some()
                || child.child_elements().next().is_some()
                || fields
                    .insert(
                        child.name().to_owned(),
                        child.text().unwrap_or_default().trim().to_owned(),
                    )
                    .is_some()
            {
                return Err(invalid("unknown or duplicate CORS rule field"));
            }
        }
        if fields.len() != RULE_FIELDS.len() || fields["MaxAgeInSeconds"].parse::<u32>().is_err() {
            return Err(invalid("incomplete CORS rule"));
        }
        rules.push(CorsRule(fields));
    }
    validate_limits(&rules)?;
    Ok(rules)
}

fn validate_limits(rules: &[CorsRule]) -> Result<()> {
    if rules.len() > 5
        || rules
            .iter()
            .flat_map(|rule| rule.0.values())
            .map(String::len)
            .sum::<usize>()
            > 2048
    {
        return Err(invalid("merging exceeds the five-rule or 2 KiB limit"));
    }
    for rule in rules {
        for name in ["AllowedOrigins", "AllowedHeaders", "ExposedHeaders"] {
            let entries: Vec<_> = rule.0[name]
                .split(',')
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .collect();
            let prefixed = entries.iter().filter(|value| value.contains('*')).count();
            if entries.iter().any(|v| v.chars().count() > 256)
                || (name == "AllowedOrigins" && entries.len() > 64)
                || (name != "AllowedOrigins" && (entries.len() - prefixed > 64 || prefixed > 2))
            {
                return Err(invalid("CORS origin or header limits exceeded"));
            }
        }
    }
    Ok(())
}

fn rules_xml(rules: &[CorsRule]) -> Result<String> {
    validate_limits(rules)?;
    let mut bytes = Vec::new();
    let mut writer =
        XmlStreamWriter::new(&mut bytes).map_aster_err(AsterError::storage_driver_error)?;
    let write = |writer: &mut XmlStreamWriter<&mut Vec<u8>>| -> std::result::Result<(), aster_forge_xml::Error> {
        writer.start("StorageServiceProperties")?;
        writer.start("Cors")?;
        for rule in rules {
            writer.start("CorsRule")?;
            for field in RULE_FIELDS {
                writer.start(field)?;
                writer.text(&rule.0[field])?;
                writer.end_element()?;
            }
            writer.end_element()?;
        }
        writer.end_element()?;
        writer.end_element()?;
        Ok(())
    };
    write(&mut writer).map_aster_err(AsterError::storage_driver_error)?;
    writer
        .finish()
        .map_aster_err(AsterError::storage_driver_error)?;
    String::from_utf8(bytes).map_aster_err(AsterError::storage_driver_error)
}

fn response_error(status: StatusCode, body: &str, request_id: Option<&str>) -> AsterError {
    let code = OwnedDocument::from_reader_with_options(
        body.as_bytes(),
        &ParseOptions::new()
            .max_size(MAX_XML_BYTES)
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
        StatusCode::FORBIDDEN => StorageErrorKind::Permission,
        StatusCode::UNAUTHORIZED => StorageErrorKind::Auth,
        StatusCode::BAD_REQUEST => StorageErrorKind::Misconfigured,
        StatusCode::TOO_MANY_REQUESTS => StorageErrorKind::RateLimited,
        status if status.is_server_error() => StorageErrorKind::Transient,
        _ => StorageErrorKind::Unknown,
    };
    let hint = if status == StatusCode::FORBIDDEN {
        " Verify the account name/key and that Shared Key access is allowed; Get/Set Blob Service Properties require account-level service access. Container SAS and Blob Data Contributor alone are insufficient. This action does not support Entra ID."
    } else {
        ""
    };
    storage_driver_error(kind, format!("Azure Blob service properties failed: HTTP {status}, code={code}, request_id={}.{hint}", request_id.unwrap_or("unavailable"))).into()
}
