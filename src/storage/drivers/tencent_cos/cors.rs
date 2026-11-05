//! Tencent Cloud COS bucket CORS protocol.
//!
//! Provider references: [`GET Bucket cors`](https://cloud.tencent.com/document/product/436/8274)
//! and [`PUT Bucket cors`](https://cloud.tencent.com/document/product/436/8279).

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use aster_forge_xml::{
    ElementRef, NodeRef, OwnedDocument, ParseOptions, XmlStreamWriter, XmlWriteOptions,
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use md5::{Digest as Md5Digest, Md5};
use reqwest::StatusCode;
use reqwest::header::CONTENT_TYPE;

use crate::api::api_error_code::ApiErrorCode;
use crate::errors::{AsterError, MapAsterErr, Result, storage_driver_error_with_code};
use aster_drive_storage::error::{StorageErrorKind, storage_driver_error};
use aster_forge_http::read_reqwest_body_limited;

use super::{TencentCosDriver, non_empty_xml_text};

pub(crate) const ASTERDRIVE_COS_CORS_RULE_ID: &str = "asterdrive-presigned-access";
const CORS_XML_CONTENT_TYPE: &str = "application/xml";
const CONTENT_MD5_HEADER: &str = "Content-MD5";
const COS_CORS_XML_MAX_BYTES: usize = 64 * 1024;
const COS_ERROR_XML_MAX_BYTES: usize = 16 * 1024;
const COS_CORS_MAX_RULES: usize = 100;
const COS_CORS_MAX_ELEMENTS: usize = 8192;

fn cors_xml_options() -> ParseOptions {
    ParseOptions::new()
        .max_size(COS_CORS_XML_MAX_BYTES)
        .max_depth(16)
        .max_elements(COS_CORS_MAX_ELEMENTS)
}

fn error_xml_options() -> ParseOptions {
    ParseOptions::new()
        .max_size(COS_ERROR_XML_MAX_BYTES)
        .max_depth(8)
        .max_elements(100)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CosCorsRule {
    pub id: Option<String>,
    pub allowed_origins: Vec<String>,
    pub allowed_methods: Vec<String>,
    pub allowed_headers: Vec<String>,
    pub expose_headers: Vec<String>,
    pub max_age_seconds: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CosCorsConfiguration {
    pub rules: Vec<CosCorsRule>,
    pub response_vary: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TencentCosCorsApplyResult {
    pub rule_id: String,
    pub allowed_origins: Vec<String>,
    pub request_id: Option<String>,
    pub preserved_rule_count: usize,
    pub replaced_existing_rule: bool,
    pub response_vary: bool,
}

impl TencentCosDriver {
    pub(crate) async fn configure_asterdrive_cors(
        &self,
        allowed_origins: &[String],
    ) -> Result<TencentCosCorsApplyResult> {
        let mut existing = self.get_bucket_cors().await?;
        validate_rule_order(&existing.rules, allowed_origins)?;
        let preserved_rule_count = existing
            .rules
            .iter()
            .filter(|rule| rule.id.as_deref() != Some(ASTERDRIVE_COS_CORS_RULE_ID))
            .count();
        let replaced_existing_rule = preserved_rule_count != existing.rules.len();
        replace_asterdrive_rule(&mut existing.rules, allowed_origins);
        existing.response_vary = Some(true);

        let request_id = self.put_bucket_cors(&existing).await?;
        Ok(TencentCosCorsApplyResult {
            rule_id: ASTERDRIVE_COS_CORS_RULE_ID.to_string(),
            allowed_origins: allowed_origins.to_vec(),
            request_id,
            preserved_rule_count,
            replaced_existing_rule,
            response_vary: true,
        })
    }

    async fn get_bucket_cors(&self) -> Result<CosCorsConfiguration> {
        let url = self.bucket_cors_url()?;
        let key_time = cos_key_time()?;
        let headers = self.signed_cos_request_headers("GET", &url, &[], &key_time)?;
        let response = self
            .client
            .get(url)
            .headers(headers)
            .send()
            .await
            .map_aster_err_ctx("COS GET Bucket cors", AsterError::storage_driver_error)?;
        let status = response.status();
        let request_id = response
            .headers()
            .get("x-cos-request-id")
            .and_then(|value| value.to_str().ok())
            .and_then(safe_cos_error_identifier);
        let body_limit = match status {
            status if status.is_success() => COS_CORS_XML_MAX_BYTES,
            _ => COS_ERROR_XML_MAX_BYTES,
        };
        let body = read_reqwest_body_limited(
            response,
            "COS GET Bucket cors response body",
            body_limit,
            AsterError::storage_driver_error,
        )
        .await
        .map_err(|error| {
            if status.is_success() {
                error
            } else {
                cos_cors_response_error(status, "", "GET Bucket cors", request_id.as_deref())
            }
        })?;
        // Error status and request identity remain authoritative even when an
        // intermediary returns a binary or otherwise malformed response body.
        if !status.is_success() {
            let body = String::from_utf8_lossy(&body);
            if status == StatusCode::NOT_FOUND
                && extract_xml_tag(&body, "Code").as_deref() == Some("NoSuchCORSConfiguration")
            {
                return Ok(CosCorsConfiguration {
                    rules: Vec::new(),
                    response_vary: None,
                });
            }
            return Err(cos_cors_response_error(
                status,
                &body,
                "GET Bucket cors",
                request_id.as_deref(),
            ));
        }
        let body = String::from_utf8(body).map_aster_err_ctx(
            "decode COS GET Bucket cors response",
            AsterError::storage_driver_error,
        )?;

        parse_cors_configuration(&body)
    }

    async fn put_bucket_cors(&self, config: &CosCorsConfiguration) -> Result<Option<String>> {
        let url = self.bucket_cors_url()?;
        let body = build_cors_configuration_xml(config)?;
        let content_md5 = content_md5_base64(body.as_bytes());
        let key_time = cos_key_time()?;
        let headers = self.signed_cos_request_headers(
            "PUT",
            &url,
            &[
                (CONTENT_TYPE.as_str(), CORS_XML_CONTENT_TYPE),
                (CONTENT_MD5_HEADER, content_md5.as_str()),
            ],
            &key_time,
        )?;
        let response = self
            .client
            .put(url)
            .headers(headers)
            .body(body)
            .send()
            .await
            .map_aster_err_ctx("COS PUT Bucket cors", AsterError::storage_driver_error)?;
        let status = response.status();
        let request_id = response
            .headers()
            .get("x-cos-request-id")
            .and_then(|value| value.to_str().ok())
            .and_then(safe_cos_error_identifier);
        if status.is_success() {
            return Ok(request_id);
        }
        let body = read_reqwest_body_limited(
            response,
            "COS PUT Bucket cors response body",
            COS_ERROR_XML_MAX_BYTES,
            AsterError::storage_driver_error,
        )
        .await
        .map_err(|_| {
            cos_cors_response_error(status, "", "PUT Bucket cors", request_id.as_deref())
        })?;
        let body = String::from_utf8_lossy(&body);

        Err(cos_cors_response_error(
            status,
            &body,
            "PUT Bucket cors",
            request_id.as_deref(),
        ))
    }
}

pub(crate) fn asterdrive_cors_rule(allowed_origins: &[String]) -> CosCorsRule {
    CosCorsRule {
        id: Some(ASTERDRIVE_COS_CORS_RULE_ID.to_string()),
        allowed_origins: allowed_origins.to_vec(),
        allowed_methods: vec!["PUT".to_string(), "GET".to_string(), "HEAD".to_string()],
        allowed_headers: vec![
            "*".to_string(),
            "Content-Type".to_string(),
            "Range".to_string(),
            "x-cos-*".to_string(),
        ],
        expose_headers: vec![
            "ETag".to_string(),
            "Content-Length".to_string(),
            "Content-Range".to_string(),
            "Content-Disposition".to_string(),
            "Accept-Ranges".to_string(),
            "x-cos-request-id".to_string(),
            "x-cos-hash-crc64ecma".to_string(),
        ],
        max_age_seconds: Some(600),
    }
}

const COS_REQUIRED_EXPOSE_HEADERS: &[&str] = &[
    "ETag",
    "Content-Range",
    "Content-Disposition",
    "Accept-Ranges",
];

fn wildcard_matches(pattern: &str, value: &str) -> bool {
    match pattern.split_once('*') {
        Some((prefix, suffix)) => {
            !suffix.contains('*')
                && value.len() >= prefix.len() + suffix.len()
                && value.starts_with(prefix)
                && value.ends_with(suffix)
        }
        None => pattern.eq_ignore_ascii_case(value),
    }
}

fn rule_matches(rule: &CosCorsRule, origin: &str, method: &str, headers: &[&str]) -> bool {
    rule.allowed_origins
        .iter()
        .any(|pattern| wildcard_matches(pattern, origin))
        && rule
            .allowed_methods
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(method))
        && headers.iter().all(|header| {
            rule.allowed_headers
                .iter()
                .any(|candidate| candidate == "*" || candidate.eq_ignore_ascii_case(header))
        })
}

fn exposes_required_headers(rule: &CosCorsRule, method: &str) -> bool {
    let required = match method {
        "PUT" => &COS_REQUIRED_EXPOSE_HEADERS[..1],
        "GET" | "HEAD" => COS_REQUIRED_EXPOSE_HEADERS,
        _ => &[],
    };
    required.iter().all(|header| {
        rule.expose_headers
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(header))
    })
}

fn validate_rule_order(rules: &[CosCorsRule], origins: &[String]) -> Result<()> {
    let requests: &[(&str, &[&str])] = &[
        ("PUT", &["content-type"]),
        ("GET", &[]),
        ("GET", &["range"]),
        ("HEAD", &[]),
    ];
    for origin in origins {
        for (method, headers) in requests {
            let matching = rules.iter().find(|rule| {
                rule.id.as_deref() != Some(ASTERDRIVE_COS_CORS_RULE_ID)
                    && rule_matches(rule, origin, method, headers)
            });
            if let Some(rule) = matching
                && !exposes_required_headers(rule, method)
            {
                return Err(AsterError::from(storage_driver_error(
                    StorageErrorKind::Precondition,
                    "an earlier matching COS CORS rule lacks required exposed headers; adjust that rule in the Tencent COS console (no rules were written)",
                )));
            }
        }
    }
    Ok(())
}

fn replace_asterdrive_rule(rules: &mut Vec<CosCorsRule>, origins: &[String]) {
    let desired = asterdrive_cors_rule(origins);
    if let Some(index) = rules
        .iter()
        .position(|rule| rule.id.as_deref() == Some(ASTERDRIVE_COS_CORS_RULE_ID))
    {
        rules[index] = desired;
    } else {
        rules.push(desired);
    }
}

pub(crate) fn build_cors_configuration_xml(config: &CosCorsConfiguration) -> Result<String> {
    if config.rules.len() > COS_CORS_MAX_RULES {
        return Err(AsterError::storage_driver_error(
            "COS CORS supports at most 100 rules; existing rules were not changed",
        ));
    }
    let mut bytes = Vec::new();
    let mut writer = XmlStreamWriter::with_options(
        &mut bytes,
        XmlWriteOptions::new().write_document_declaration(true),
    )
    .map_aster_err_ctx(
        "create COS CORS XML writer",
        AsterError::storage_driver_error,
    )?;

    writer
        .start("CORSConfiguration")
        .map_aster_err_ctx("start CORSConfiguration", AsterError::storage_driver_error)?;
    for rule in &config.rules {
        write_cors_rule(&mut writer, rule)
            .map_aster_err_ctx("write CORSRule", AsterError::storage_driver_error)?;
    }
    if let Some(response_vary) = config.response_vary {
        let value = if response_vary { "true" } else { "false" };
        write_text_element(&mut writer, "ResponseVary", value)
            .map_aster_err_ctx("write ResponseVary", AsterError::storage_driver_error)?;
    }
    writer
        .end_element()
        .map_aster_err_ctx("close CORSConfiguration", AsterError::storage_driver_error)?;

    writer
        .finish()
        .map_aster_err_ctx("finalize COS CORS XML", AsterError::storage_driver_error)?;
    if bytes.len() > COS_CORS_XML_MAX_BYTES {
        return Err(AsterError::storage_driver_error(
            "COS CORS XML exceeds 64 KiB; existing rules were not changed",
        ));
    }
    String::from_utf8(bytes)
        .map_aster_err_ctx("encode COS CORS XML", AsterError::storage_driver_error)
}

pub(crate) fn parse_cors_configuration(body: &str) -> Result<CosCorsConfiguration> {
    let doc = OwnedDocument::from_reader_with_options(body.as_bytes(), &cors_xml_options())
        .map_aster_err_ctx("parse COS CORS XML", AsterError::storage_driver_error)?;
    let root = doc.root();
    if root.name() != "CORSConfiguration" {
        return Err(AsterError::from(storage_driver_error(
            StorageErrorKind::Misconfigured,
            "COS CORS XML root is not CORSConfiguration",
        )));
    }

    let mut rules = Vec::new();
    let mut response_vary = None;
    for child in root.child_elements() {
        match child.name() {
            "CORSRule" => rules.push(parse_cors_rule(&child)?),
            "ResponseVary"
                if response_vary.is_none() && child.child_elements().next().is_none() =>
            {
                response_vary = match element_text(&child).as_deref() {
                    Some(value) if value.eq_ignore_ascii_case("true") => Some(true),
                    Some(value) if value.eq_ignore_ascii_case("false") => Some(false),
                    _ => {
                        return Err(AsterError::storage_driver_error(
                            "invalid COS CORS ResponseVary",
                        ));
                    }
                };
            }
            _ => {
                return Err(AsterError::storage_driver_error(
                    "unsupported or duplicate COS CORS field; existing configuration was not changed",
                ));
            }
        }
    }
    if rules.len() > COS_CORS_MAX_RULES {
        return Err(AsterError::storage_driver_error(
            "COS CORS supports at most 100 rules",
        ));
    }

    Ok(CosCorsConfiguration {
        rules,
        response_vary,
    })
}

fn write_text_element(
    writer: &mut XmlStreamWriter<&mut Vec<u8>>,
    name: &str,
    value: &str,
) -> std::result::Result<(), aster_forge_xml::Error> {
    writer.start(name)?;
    writer.text(value)?;
    writer.end_element()
}

fn write_cors_rule(
    writer: &mut XmlStreamWriter<&mut Vec<u8>>,
    rule: &CosCorsRule,
) -> std::result::Result<(), aster_forge_xml::Error> {
    writer.start("CORSRule")?;
    if let Some(ref id) = rule.id {
        write_text_element(writer, "ID", id)?;
    }
    for value in &rule.allowed_origins {
        write_text_element(writer, "AllowedOrigin", value)?;
    }
    for value in &rule.allowed_methods {
        write_text_element(writer, "AllowedMethod", value)?;
    }
    for value in &rule.allowed_headers {
        write_text_element(writer, "AllowedHeader", value)?;
    }
    for value in &rule.expose_headers {
        write_text_element(writer, "ExposeHeader", value)?;
    }
    if let Some(ref value) = rule.max_age_seconds {
        write_text_element(writer, "MaxAgeSeconds", &value.to_string())?;
    }
    writer.end_element()?;
    Ok(())
}

fn parse_cors_rule(element: &ElementRef<'_, Arc<[u8]>>) -> Result<CosCorsRule> {
    for child in element.child_elements() {
        if !matches!(
            child.name(),
            "ID" | "AllowedOrigin"
                | "AllowedMethod"
                | "AllowedHeader"
                | "ExposeHeader"
                | "MaxAgeSeconds"
        ) || child.child_elements().next().is_some()
        {
            return Err(AsterError::storage_driver_error(
                "unsupported COS CORS rule field; existing configuration was not changed",
            ));
        }
    }
    for name in ["ID", "MaxAgeSeconds"] {
        if element
            .child_elements()
            .filter(|child| child.name() == name)
            .count()
            > 1
        {
            return Err(AsterError::storage_driver_error(
                "duplicate COS CORS rule field",
            ));
        }
    }
    let rule = CosCorsRule {
        id: child_text(element, "ID"),
        allowed_origins: child_texts(element, "AllowedOrigin"),
        allowed_methods: child_texts(element, "AllowedMethod"),
        allowed_headers: child_texts(element, "AllowedHeader"),
        expose_headers: child_texts(element, "ExposeHeader"),
        max_age_seconds: child_text(element, "MaxAgeSeconds")
            .map(|value| {
                value
                    .parse::<u32>()
                    .map_err(|_| AsterError::storage_driver_error("invalid COS CORS MaxAgeSeconds"))
            })
            .transpose()?,
    };
    if rule.allowed_origins.is_empty() || rule.allowed_methods.is_empty() {
        return Err(AsterError::storage_driver_error(
            "COS CORS rule requires origins and methods",
        ));
    }
    Ok(rule)
}

fn child_texts(element: &ElementRef<'_, Arc<[u8]>>, name: &str) -> Vec<String> {
    element
        .child_elements()
        .filter(|child| child.name() == name)
        .filter_map(|child| non_empty_xml_text(child.text().as_deref()))
        .collect()
}

fn child_text(element: &ElementRef<'_, Arc<[u8]>>, name: &str) -> Option<String> {
    element
        .child_elements()
        .filter(|child| child.name() == name)
        .find_map(|child| non_empty_xml_text(child.text().as_deref()))
}

fn element_text(element: &ElementRef<'_, Arc<[u8]>>) -> Option<String> {
    non_empty_xml_text(element.text().as_deref())
}

fn cos_key_time() -> Result<String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_aster_err_ctx("COS signing clock", AsterError::storage_driver_error)?
        .as_secs();
    Ok(format!("{now};{}", now + 300))
}

fn content_md5_base64(body: &[u8]) -> String {
    BASE64_STANDARD.encode(Md5::digest(body))
}

fn safe_cos_error_identifier(value: &str) -> Option<String> {
    (!value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_=/.".contains(&b)))
    .then(|| value.to_string())
}

fn cos_cors_response_error(
    status: StatusCode,
    body: &str,
    action: &str,
    header_request_id: Option<&str>,
) -> AsterError {
    let code = extract_xml_tag(body, "Code").and_then(|value| safe_cos_error_identifier(&value));
    // Provider messages and raw bodies may echo credentials or signed URLs.
    let request_id = header_request_id
        .and_then(safe_cos_error_identifier)
        .or_else(|| {
            extract_xml_tag(body, "RequestId").and_then(|value| safe_cos_error_identifier(&value))
        })
        .map(|id| format!(" request_id={id}"))
        .unwrap_or_default();
    let error_code = code.map(|code| format!(" code={code}")).unwrap_or_default();
    let detail = format!("Tencent COS {action} failed with HTTP {status}{error_code}{request_id}");

    let kind = match status {
        StatusCode::BAD_REQUEST => StorageErrorKind::Misconfigured,
        StatusCode::UNAUTHORIZED => StorageErrorKind::Auth,
        StatusCode::FORBIDDEN => StorageErrorKind::Permission,
        StatusCode::NOT_FOUND => StorageErrorKind::NotFound,
        StatusCode::PRECONDITION_FAILED | StatusCode::CONFLICT => StorageErrorKind::Precondition,
        StatusCode::TOO_MANY_REQUESTS => StorageErrorKind::RateLimited,
        status if status.is_server_error() => StorageErrorKind::Transient,
        _ => StorageErrorKind::Unknown,
    };
    if kind == StorageErrorKind::Permission {
        let permission = if action == "GET Bucket cors" {
            "name/cos:GetBucketCORS"
        } else {
            "name/cos:PutBucketCORS"
        };
        storage_driver_error_with_code(
            kind,
            ApiErrorCode::StoragePermission,
            format!("{detail}. The Tencent COS credential needs {permission}."),
        )
    } else {
        AsterError::from(storage_driver_error(kind, detail))
    }
}

fn extract_xml_tag(body: &str, tag: &str) -> Option<String> {
    let doc =
        OwnedDocument::from_reader_with_options(body.as_bytes(), &error_xml_options()).ok()?;
    find_xml_tag_text(&doc.root(), tag)
}

fn find_xml_tag_text(element: &ElementRef<'_, Arc<[u8]>>, tag: &str) -> Option<String> {
    if element.name() == tag {
        return element_text(element);
    }
    element
        .children()
        .filter_map(|child| match child {
            NodeRef::Element(e) => Some(e),
            _ => None,
        })
        .find_map(|child| find_xml_tag_text(&child, tag))
}

#[cfg(test)]
#[path = "cors/http_tests.rs"]
mod http_tests;

#[cfg(test)]
#[path = "cors/tests.rs"]
mod tests;
