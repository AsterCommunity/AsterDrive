//! Native Huawei OBS bucket CORS. Protocol and signing evidence is pinned in
//! developer-docs/zh-CN/design/huawei-obs-cors-contract.md.
//! This client never uses the separate CDN download domain.

use std::sync::Arc;

use aster_forge_http::read_reqwest_body_limited;
use aster_forge_xml::{ElementRef, NodeRef, OwnedDocument, ParseOptions, XmlStreamWriter};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use md5::{Digest as _, Md5};
use reqwest::StatusCode;
use url::Url;

use crate::api::api_error_code::ApiErrorCode;
use crate::config::OUTBOUND_HTTP_USER_AGENT;
use crate::errors::{AsterError, MapAsterErr, Result, storage_driver_error_with_code};
use aster_drive_storage::error::{StorageErrorKind, storage_driver_error};

use super::{HuaweiObsDriver, HuaweiObsDriverConfig, HuaweiObsStaticCredentials, signing};

// Local safety bounds: never truncate a document before writing it back.
const CORS_XML_MAX_BYTES: usize = 64 * 1024;
const CORS_ERROR_MAX_BYTES: usize = 8 * 1024;
const CORS_MAX_RULES: usize = 100;
const REQUIRED_EXPOSE_HEADERS: &[&str] = &[
    "ETag",
    "Content-Range",
    "Accept-Ranges",
    "Content-Disposition",
];

#[derive(Debug, Clone, PartialEq, Eq)]
struct CorsRule {
    id: Option<String>,
    origins: Vec<String>,
    methods: Vec<String>,
    headers: Vec<String>,
    expose_headers: Vec<String>,
    max_age_seconds: Option<i64>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
struct CorsConfiguration {
    rules: Vec<CorsRule>,
}

pub(crate) struct ObsCorsClient {
    client: reqwest::Client,
    url: Url,
    bucket: String,
    use_cname: bool,
    credentials: HuaweiObsStaticCredentials,
}

#[derive(serde::Serialize)]
pub(crate) struct ObsCorsApplyResult {
    pub bucket: String,
    pub origins: Vec<String>,
    pub changed: bool,
    pub preserved_rule_count: usize,
    pub request_id: Option<String>,
}

impl ObsCorsClient {
    pub(crate) fn new(
        config: &HuaweiObsDriverConfig,
        credentials: &HuaweiObsStaticCredentials,
    ) -> Result<Self> {
        let use_cname = config.use_cname;
        let normalized = HuaweiObsDriver::normalize_endpoint(
            &config.endpoint,
            &config.bucket,
            &config.region,
            use_cname,
        )?;
        let mut url = Url::parse(&normalized.endpoint)
            .map_aster_err_ctx("parse OBS CORS endpoint", AsterError::storage_driver_error)?;
        if !use_cname {
            let host = url.host_str().ok_or_else(|| {
                AsterError::from(storage_driver_error(
                    StorageErrorKind::Misconfigured,
                    "OBS CORS endpoint has no host",
                ))
            })?;
            url.set_host(Some(&format!("{}.{}", normalized.bucket, host)))
                .map_err(|_| {
                    AsterError::from(storage_driver_error(
                        StorageErrorKind::Misconfigured,
                        "invalid OBS CORS bucket host",
                    ))
                })?;
        }
        url.set_path("/");
        url.set_query(Some("cors"));
        let client = reqwest::Client::builder()
            .connect_timeout(config.connect_timeout)
            .read_timeout(config.read_timeout)
            .timeout(config.operation_timeout)
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(OUTBOUND_HTTP_USER_AGENT)
            .build()
            .map_aster_err_ctx("build OBS CORS client", AsterError::storage_driver_error)?;
        Ok(Self {
            client,
            url,
            bucket: normalized.bucket,
            use_cname,
            credentials: HuaweiObsStaticCredentials {
                access_key: credentials.access_key.clone(),
                secret_key: credentials.secret_key.clone(),
                cdn_auth_key: None,
            },
        })
    }

    pub(crate) async fn configure(&self, origins: &[String]) -> Result<ObsCorsApplyResult> {
        validate_origins(origins)?;
        let (mut config, get_request_id) = self.get().await?;
        let desired = desired_rule(origins);
        let preserved_rule_count = config.rules.len();
        if !needs_rule(&config.rules, origins)? {
            return Ok(ObsCorsApplyResult {
                bucket: self.bucket.clone(),
                origins: origins.to_vec(),
                changed: false,
                preserved_rule_count,
                request_id: get_request_id,
            });
        }
        config.rules.push(desired);
        let xml = serialize(&config)?;
        let request_id = self.put(xml).await?;
        Ok(ObsCorsApplyResult {
            bucket: self.bucket.clone(),
            origins: origins.to_vec(),
            changed: true,
            preserved_rule_count,
            request_id,
        })
    }

    async fn get(&self) -> Result<(CorsConfiguration, Option<String>)> {
        let response = self.request("GET", None).await?;
        let status = response.status();
        let request_id = response_request_id(&response);
        let body = read_reqwest_body_limited(
            response,
            "OBS GetBucketCors response",
            if status.is_success() {
                CORS_XML_MAX_BYTES
            } else {
                CORS_ERROR_MAX_BYTES
            },
            AsterError::storage_driver_error,
        )
        .await?;
        if status == StatusCode::NOT_FOUND
            && error_code(&body).as_deref() == Some("NoSuchCORSConfiguration")
        {
            return Ok((CorsConfiguration { rules: Vec::new() }, request_id));
        }
        if !status.is_success() {
            return Err(response_error(
                status,
                &body,
                "GetBucketCors",
                request_id.as_deref(),
            ));
        }
        Ok((parse(&body)?, request_id))
    }

    async fn put(&self, xml: String) -> Result<Option<String>> {
        let response = self.request("PUT", Some(xml)).await?;
        let status = response.status();
        let request_id = response_request_id(&response);
        if status.is_success() {
            return Ok(request_id);
        }
        let body = read_reqwest_body_limited(
            response,
            "OBS PutBucketCors response",
            CORS_ERROR_MAX_BYTES,
            AsterError::storage_driver_error,
        )
        .await?;
        Err(response_error(
            status,
            &body,
            "PutBucketCors",
            request_id.as_deref(),
        ))
    }

    async fn request(&self, method: &str, xml: Option<String>) -> Result<reqwest::Response> {
        let md5 = xml
            .as_ref()
            .map(|body| BASE64_STANDARD.encode(Md5::digest(body.as_bytes())));
        let headers = signing::signed_cors_request_headers(
            method,
            &self.url,
            &self.bucket,
            self.use_cname,
            &self.credentials,
            md5.as_deref(),
            std::time::SystemTime::now(),
        )
        .map_aster_err_ctx("sign OBS CORS request", AsterError::storage_driver_error)?;
        let mut request = self.client.request(
            if method == "GET" {
                reqwest::Method::GET
            } else {
                reqwest::Method::PUT
            },
            self.url.clone(),
        );
        for (name, value) in headers {
            request = request.header(name, value);
        }
        if let Some(body) = xml {
            request = request.body(body);
        }
        request
            .send()
            .await
            .map_aster_err_ctx("OBS CORS request", AsterError::storage_driver_error)
    }
}

fn desired_rule(origins: &[String]) -> CorsRule {
    CorsRule {
        id: None,
        origins: origins.to_vec(),
        methods: vec!["PUT".into(), "GET".into(), "HEAD".into()],
        headers: vec!["Content-Type".into(), "Range".into()],
        expose_headers: vec![
            "ETag".into(),
            "Content-Length".into(),
            "Content-Range".into(),
            "Accept-Ranges".into(),
            "Content-Disposition".into(),
        ],
        max_age_seconds: Some(600),
    }
}

fn validate_origins(origins: &[String]) -> Result<()> {
    if origins.is_empty() {
        return Err(invalid_cors(
            "OBS CORS requires explicit public site origins",
        ));
    }
    for origin in origins {
        let url = Url::parse(origin).map_err(|_| invalid_cors("invalid OBS CORS origin"))?;
        if !matches!(url.scheme(), "http" | "https")
            || origin.contains('*')
            || url.origin().ascii_serialization() != *origin
        {
            return Err(invalid_cors(
                "OBS CORS requires canonical HTTP(S) origins without wildcards, paths or credentials",
            ));
        }
    }
    Ok(())
}

fn wildcard_matches(pattern: &str, value: &str) -> bool {
    match pattern.split_once('*') {
        Some((prefix, suffix)) => {
            !suffix.contains('*')
                && value.len() >= prefix.len() + suffix.len()
                && value.starts_with(prefix)
                && value.ends_with(suffix)
        }
        None => pattern == value,
    }
}

fn rule_matches(rule: &CorsRule, origin: &str, method: &str, headers: &[&str]) -> bool {
    rule.origins.iter().any(|pattern| {
        wildcard_matches(&pattern.to_ascii_lowercase(), &origin.to_ascii_lowercase())
    }) && rule
        .methods
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(method))
        && headers.iter().all(|header| {
            rule.headers
                .iter()
                // OBS permits one '*' anywhere in AllowedHeader. Header names
                // are case-insensitive even when the wildcard is partial.
                .any(|candidate| {
                    wildcard_matches(
                        &candidate.to_ascii_lowercase(),
                        &header.to_ascii_lowercase(),
                    )
                })
        })
}

fn exposes_required_headers(rule: &CorsRule, method: &str) -> bool {
    let required_headers: &[&str] = match method {
        "PUT" => &["ETag"],
        "GET" | "HEAD" => REQUIRED_EXPOSE_HEADERS,
        _ => &[],
    };
    required_headers.iter().all(|required| {
        rule.expose_headers
            .iter()
            .any(|exposed| exposed.eq_ignore_ascii_case(required))
    })
}

/// Conservatively check the first matching rule before appending: an earlier
/// rule that hides required browser response headers may shadow later rules.
fn needs_rule(rules: &[CorsRule], origins: &[String]) -> Result<bool> {
    for rule in rules {
        for pattern in rule.origins.iter().chain(&rule.headers) {
            if pattern.bytes().filter(|b| *b == b'*').count() > 1
                || pattern.bytes().any(|b| {
                    matches!(
                        b,
                        b'?' | b'['
                            | b']'
                            | b'('
                            | b')'
                            | b'+'
                            | b'\\'
                            | b'{'
                            | b'}'
                            | b'^'
                            | b'$'
                            | b'|'
                    )
                })
            {
                return Err(storage_driver_error(StorageErrorKind::Precondition, "OBS CORS contains an unsupported matching pattern; adjust it in the Huawei Cloud console (no rules were written)").into());
            }
        }
    }
    let requests: &[(&str, &[&str])] = &[
        ("PUT", &[]),
        ("PUT", &["content-type"]),
        ("GET", &[]),
        ("GET", &["range"]),
        ("HEAD", &[]),
    ];
    let mut missing = false;
    for origin in origins {
        for (method, headers) in requests {
            let matching = rules
                .iter()
                .find(|rule| rule_matches(rule, origin, method, headers));
            match matching {
                None => missing = true,
                Some(rule) if !exposes_required_headers(rule, method) => {
                    return Err(storage_driver_error(
                        StorageErrorKind::Precondition,
                        "an earlier matching OBS CORS rule lacks required exposed headers; adjust that rule in the Huawei Cloud console (no rules were written)",
                    )
                    .into());
                }
                Some(_) => {}
            }
        }
    }
    Ok(missing)
}

fn xml_options(max_size: usize) -> ParseOptions {
    ParseOptions::new()
        .max_size(max_size)
        .max_depth(12)
        .max_elements(3000)
}

fn parse(body: &[u8]) -> Result<CorsConfiguration> {
    let doc = OwnedDocument::from_reader_with_options(body, &xml_options(CORS_XML_MAX_BYTES))
        .map_aster_err_ctx("parse OBS CORS XML", AsterError::storage_driver_error)?;
    let root = doc.root();
    if root.name() != "CORSConfiguration" {
        return Err(invalid_cors("unexpected OBS CORS root"));
    }
    if root.prefix().is_some()
        || root.attributes().next().is_some()
        || !root.namespace().is_none_or(valid_obs_namespace)
    {
        return Err(invalid_cors(
            "OBS CORS root contains unsupported namespace or attributes",
        ));
    }
    let namespace = root.namespace();
    reject_container_text(&root)?;
    let mut rules = Vec::new();
    for child in root.child_elements() {
        match child.name() {
            "CORSRule" => rules.push(parse_rule(&child, namespace)?),
            _ => return Err(invalid_cors("OBS CORS contains an unsupported field")),
        }
    }
    if rules.len() > CORS_MAX_RULES {
        return Err(invalid_cors("OBS CORS exceeds 100 rules"));
    }
    Ok(CorsConfiguration { rules })
}

fn valid_obs_namespace(namespace: &str) -> bool {
    let Ok(url) = Url::parse(namespace) else {
        return false;
    };
    url.scheme() == "http"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
        && url.path() == "/doc/2015-06-30/"
        && url.host_str().is_some_and(|host| {
            host.starts_with("obs.") && super::is_any_official_obs_endpoint(host)
        })
}

fn reject_container_text(element: &ElementRef<'_, Arc<[u8]>>) -> Result<()> {
    if element.children().any(|node| matches!(node, NodeRef::Text(text) | NodeRef::CData(text) if !text.trim().is_empty())) {
        return Err(invalid_cors("OBS CORS container contains unsupported text"));
    }
    Ok(())
}

fn parse_rule(element: &ElementRef<'_, Arc<[u8]>>, namespace: Option<&str>) -> Result<CorsRule> {
    if element.namespace() != namespace
        || element.prefix().is_some()
        || element.attributes().next().is_some()
    {
        return Err(invalid_cors(
            "OBS CORS rule contains unsupported attributes",
        ));
    }
    reject_container_text(element)?;
    let mut rule = CorsRule {
        id: None,
        origins: Vec::new(),
        methods: Vec::new(),
        headers: Vec::new(),
        expose_headers: Vec::new(),
        max_age_seconds: None,
    };
    for child in element.child_elements() {
        if child.namespace() != namespace
            || child.prefix().is_some()
            || child.attributes().next().is_some()
        {
            return Err(invalid_cors(
                "OBS CORS field contains unsupported attributes",
            ));
        }
        let value = child
            .text()
            .ok_or_else(|| invalid_cors("OBS CORS field has no value"))?;
        let value = value.as_ref();
        if value.trim().is_empty() || child.child_elements().next().is_some() {
            return Err(invalid_cors("OBS CORS field has invalid content"));
        }
        match child.name() {
            "ID" if rule.id.is_none() => rule.id = Some(value.to_string()),
            "AllowedOrigin" => rule.origins.push(value.to_string()),
            "AllowedMethod" => rule.methods.push(value.to_string()),
            "AllowedHeader" => rule.headers.push(value.to_string()),
            "ExposeHeader" => rule.expose_headers.push(value.to_string()),
            "MaxAgeSeconds" if rule.max_age_seconds.is_none() => {
                rule.max_age_seconds = Some(
                    value
                        .trim()
                        .parse()
                        .map_err(|_| invalid_cors("invalid OBS CORS MaxAgeSeconds"))?,
                );
            }
            _ => return Err(invalid_cors("OBS CORS rule contains an unsupported field")),
        }
    }
    if rule.origins.is_empty() || rule.methods.is_empty() {
        return Err(invalid_cors("OBS CORS rule lacks origins or methods"));
    }
    Ok(rule)
}

fn serialize(config: &CorsConfiguration) -> Result<String> {
    if config.rules.len() > CORS_MAX_RULES {
        return Err(invalid_cors("OBS CORS would exceed 100 rules"));
    }
    let mut bytes = Vec::new();
    let mut writer = XmlStreamWriter::new(&mut bytes)
        .map_aster_err_ctx("create OBS CORS XML", AsterError::storage_driver_error)?;
    writer
        .start("CORSConfiguration")
        .map_aster_err_ctx("write OBS CORS root", AsterError::storage_driver_error)?;
    for rule in &config.rules {
        writer
            .start("CORSRule")
            .map_aster_err_ctx("write OBS CORS rule", AsterError::storage_driver_error)?;
        if let Some(id) = &rule.id {
            writer
                .start("ID")
                .map_aster_err_ctx("write OBS CORS ID", AsterError::storage_driver_error)?;
            writer
                .text(id)
                .map_aster_err_ctx("write OBS CORS ID value", AsterError::storage_driver_error)?;
            writer
                .end_element()
                .map_aster_err_ctx("close OBS CORS ID", AsterError::storage_driver_error)?;
        }
        for (name, values) in [
            ("AllowedOrigin", &rule.origins),
            ("AllowedMethod", &rule.methods),
            ("AllowedHeader", &rule.headers),
            ("ExposeHeader", &rule.expose_headers),
        ] {
            for value in values {
                writer
                    .start(name)
                    .map_aster_err_ctx("write OBS CORS field", AsterError::storage_driver_error)?;
                writer
                    .text(value)
                    .map_aster_err_ctx("write OBS CORS value", AsterError::storage_driver_error)?;
                writer
                    .end_element()
                    .map_aster_err_ctx("close OBS CORS field", AsterError::storage_driver_error)?;
            }
        }
        if let Some(value) = rule.max_age_seconds {
            writer
                .start("MaxAgeSeconds")
                .map_aster_err_ctx("write OBS CORS max age", AsterError::storage_driver_error)?;
            writer.text(&value.to_string()).map_aster_err_ctx(
                "write OBS CORS max age value",
                AsterError::storage_driver_error,
            )?;
            writer
                .end_element()
                .map_aster_err_ctx("close OBS CORS max age", AsterError::storage_driver_error)?;
        }
        writer
            .end_element()
            .map_aster_err_ctx("close OBS CORS rule", AsterError::storage_driver_error)?;
    }
    writer
        .end_element()
        .map_aster_err_ctx("close OBS CORS root", AsterError::storage_driver_error)?;
    writer
        .finish()
        .map_aster_err_ctx("finalize OBS CORS XML", AsterError::storage_driver_error)?;
    if bytes.len() > CORS_XML_MAX_BYTES {
        return Err(invalid_cors("OBS CORS XML would exceed 64 KB"));
    }
    String::from_utf8(bytes)
        .map_aster_err_ctx("encode OBS CORS XML", AsterError::storage_driver_error)
}

fn invalid_cors(message: &str) -> AsterError {
    AsterError::from(storage_driver_error(
        StorageErrorKind::Misconfigured,
        message,
    ))
}

fn response_request_id(response: &reqwest::Response) -> Option<String> {
    response
        .headers()
        .get("x-obs-request-id")?
        .to_str()
        .ok()
        .filter(|value| {
            value.len() <= 128
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
        .map(str::to_string)
}

fn error_code(body: &[u8]) -> Option<String> {
    let doc =
        OwnedDocument::from_reader_with_options(body, &xml_options(CORS_ERROR_MAX_BYTES)).ok()?;
    if doc.root().name() != "Error" {
        return None;
    }
    doc.root().children().find_map(|child| match child {
        NodeRef::Element(element) if element.name() == "Code" => {
            element.text().map(|value| value.trim().to_string())
        }
        _ => None,
    })
}

fn response_error(
    status: StatusCode,
    body: &[u8],
    operation: &str,
    request_id: Option<&str>,
) -> AsterError {
    let code = error_code(body).filter(|value| {
        value.len() <= 80 && value.bytes().all(|byte| byte.is_ascii_alphanumeric())
    });
    let code_text = code.as_deref().unwrap_or("Unknown");
    let request_suffix = request_id
        .map(|request_id| format!(", request ID {request_id}"))
        .unwrap_or_default();
    let message =
        format!("OBS {operation} failed: HTTP {status}, provider code {code_text}{request_suffix}");
    let kind = match status {
        StatusCode::UNAUTHORIZED => StorageErrorKind::Auth,
        StatusCode::FORBIDDEN => StorageErrorKind::Permission,
        StatusCode::BAD_REQUEST => StorageErrorKind::Misconfigured,
        StatusCode::CONFLICT | StatusCode::PRECONDITION_FAILED => StorageErrorKind::Precondition,
        StatusCode::TOO_MANY_REQUESTS => StorageErrorKind::RateLimited,
        status if status.is_server_error() => StorageErrorKind::Transient,
        _ => StorageErrorKind::Unknown,
    };
    if kind == StorageErrorKind::Permission && code.as_deref() == Some("AccessDenied") {
        let permission = if operation == "GetBucketCors" {
            "obs:bucket:GetBucketCORS"
        } else {
            "obs:bucket:PutBucketCORS"
        };
        storage_driver_error_with_code(
            kind,
            ApiErrorCode::StoragePermission,
            format!(
                "{message}; verify {permission} on the target bucket, bucket/user policy and request date"
            ),
        )
    } else {
        AsterError::from(storage_driver_error(kind, message))
    }
}

#[cfg(test)]
mod tests;
