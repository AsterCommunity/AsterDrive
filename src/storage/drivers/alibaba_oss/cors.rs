//! Alibaba OSS bucket CORS protocol.
//!
//! Provider references: [`GetBucketCors`](https://help.aliyun.com/zh/oss/developer-reference/getbucketcors)
//! and [`PutBucketCors`](https://help.aliyun.com/zh/oss/developer-reference/putbucketcors).

use std::sync::Arc;

use aster_forge_http::read_reqwest_body_limited;
use aster_forge_xml::{ElementRef, NodeRef, OwnedDocument, ParseOptions, XmlStreamWriter};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use chrono::Utc;
use md5::{Digest as _, Md5};
use reqwest::StatusCode;
use url::Url;

use crate::api::api_error_code::ApiErrorCode;
use crate::errors::{AsterError, MapAsterErr, Result, storage_driver_error_with_code};
use aster_drive_storage::error::{StorageErrorKind, storage_driver_error};

use super::{AlibabaOssDriverConfig, AlibabaOssStaticCredentials, normalize_oss_endpoint, signing};

const CORS_XML_MAX_BYTES: usize = 16 * 1024;
const CORS_ERROR_MAX_BYTES: usize = 8 * 1024;
const CORS_MAX_RULES: usize = 20;
const REQUIRED_EXPOSE_HEADERS: &[&str] = &[
    "ETag",
    "Content-Range",
    "Accept-Ranges",
    "Content-Disposition",
];

#[derive(Debug, Clone, PartialEq, Eq)]
struct CorsRule {
    origins: Vec<String>,
    methods: Vec<String>,
    headers: Vec<String>,
    expose_headers: Vec<String>,
    max_age_seconds: Option<u32>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
struct CorsConfiguration {
    rules: Vec<CorsRule>,
    response_vary: Option<bool>,
}

pub(crate) struct OssCorsClient {
    client: aster_forge_http::HttpClient,
    url: Url,
    bucket: String,
    region: String,
    access_key: String,
    secret_key: String,
}

pub(crate) struct OssCorsApplyResult {
    pub bucket: String,
    pub origins: Vec<String>,
    pub changed: bool,
    pub preserved_rule_count: usize,
    pub request_id: Option<String>,
}

impl OssCorsClient {
    pub(super) fn new(
        config: &AlibabaOssDriverConfig,
        credentials: &AlibabaOssStaticCredentials,
    ) -> Result<Self> {
        let (endpoint, use_cname) = match config.server_side_endpoint.trim() {
            "" => (config.endpoint.as_str(), config.use_cname),
            endpoint => (endpoint, false),
        };
        let normalized =
            normalize_oss_endpoint(endpoint, &config.bucket, use_cname, "OSS CORS endpoint")?;
        let mut url = Url::parse(&normalized.endpoint)
            .map_aster_err_ctx("parse OSS CORS endpoint", AsterError::storage_driver_error)?;
        if !use_cname {
            let host = url.host_str().ok_or_else(|| {
                AsterError::from(storage_driver_error(
                    StorageErrorKind::Misconfigured,
                    "OSS CORS endpoint has no host",
                ))
            })?;
            url.set_host(Some(&format!("{}.{}", normalized.bucket, host)))
                .map_err(|_| {
                    AsterError::from(storage_driver_error(
                        StorageErrorKind::Misconfigured,
                        "invalid OSS CORS bucket host",
                    ))
                })?;
        }
        url.set_path("/");
        url.set_query(Some("cors"));
        let client = aster_drive_outbound::ClientConfig::new(
            aster_drive_outbound::Profile::ProviderControl,
            crate::config::OUTBOUND_HTTP_USER_AGENT,
        )
        .connect_timeout(config.connect_timeout)
        .read_timeout(config.read_timeout)
        .timeout(config.operation_timeout)
        .build()
        .map_aster_err_ctx("build OSS CORS client", AsterError::storage_driver_error)?;
        Ok(Self {
            client,
            url,
            bucket: normalized.bucket,
            region: config.region.clone(),
            access_key: credentials.access_key.clone(),
            secret_key: credentials.secret_key.clone(),
        })
    }

    pub(crate) async fn configure(&self, origins: &[String]) -> Result<OssCorsApplyResult> {
        validate_origins(origins)?;
        let (mut config, get_request_id) = self.get().await?;
        let desired = desired_rule(origins);
        let preserved_rule_count = config.rules.len();
        if !needs_rule(&config.rules, origins)? {
            return Ok(OssCorsApplyResult {
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
        Ok(OssCorsApplyResult {
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
            "OSS GetBucketCors response",
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
            return Ok((
                CorsConfiguration {
                    rules: Vec::new(),
                    response_vary: None,
                },
                request_id,
            ));
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
            "OSS PutBucketCors response",
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
        let headers = signing::signed_bucket_request_headers(
            signing::OssSigningContext {
                method,
                bucket: &self.bucket,
                region: &self.region,
                access_key: &self.access_key,
                secret_key: &self.secret_key,
                now: Utc::now(),
            },
            xml.as_ref().map(|_| "application/xml"),
            md5.as_deref(),
        )
        .map_aster_err_ctx("sign OSS CORS request", AsterError::storage_driver_error)?;
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
            .map_aster_err_ctx("OSS CORS request", AsterError::storage_driver_error)
    }
}

fn desired_rule(origins: &[String]) -> CorsRule {
    CorsRule {
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
            "OSS CORS requires explicit public site origins",
        ));
    }
    for origin in origins {
        let url = Url::parse(origin).map_err(|_| invalid_cors("invalid OSS CORS origin"))?;
        if !matches!(url.scheme(), "http" | "https")
            || origin.contains('*')
            || url.origin().ascii_serialization() != *origin
        {
            return Err(invalid_cors(
                "OSS CORS requires canonical HTTP(S) origins without wildcards, paths or credentials",
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
        None => pattern.eq_ignore_ascii_case(value),
    }
}

fn rule_matches(rule: &CorsRule, origin: &str, method: &str, headers: &[&str]) -> bool {
    rule.origins
        .iter()
        .any(|pattern| wildcard_matches(pattern, origin))
        && rule
            .methods
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(method))
        && headers.iter().all(|header| {
            rule.headers
                .iter()
                // OSS permits one '*' anywhere in AllowedHeader. Header names
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

/// OSS selects the first rule matching the origin, method, and requested
/// headers. Appending a rule is only safe when no earlier rule shadows a
/// browser request that needs the headers AsterDrive's direct paths consume.
fn needs_rule(rules: &[CorsRule], origins: &[String]) -> Result<bool> {
    let requests: &[(&str, &[&str])] = &[
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
                        "an earlier matching OSS CORS rule lacks required exposed headers; adjust that rule in the Alibaba Cloud console (no rules were written)",
                    )
                    .into());
                }
                Some(_) => {}
            }
        }
    }
    Ok(missing)
}

#[cfg(test)]
fn equivalent(left: &CorsRule, right: &CorsRule) -> bool {
    fn sorted(values: &[String]) -> Vec<String> {
        let mut values = values
            .iter()
            .map(|value| value.to_ascii_lowercase())
            .collect::<Vec<_>>();
        values.sort_unstable();
        values.dedup();
        values
    }
    sorted(&left.origins) == sorted(&right.origins)
        && sorted(&left.methods) == sorted(&right.methods)
        && sorted(&left.headers) == sorted(&right.headers)
        && sorted(&left.expose_headers) == sorted(&right.expose_headers)
        && left.max_age_seconds == right.max_age_seconds
}

fn xml_options(max_size: usize) -> ParseOptions {
    ParseOptions::new()
        .max_size(max_size)
        .max_depth(12)
        .max_elements(300)
}

fn parse(body: &[u8]) -> Result<CorsConfiguration> {
    let doc = OwnedDocument::from_reader_with_options(body, &xml_options(CORS_XML_MAX_BYTES))
        .map_aster_err_ctx("parse OSS CORS XML", AsterError::storage_driver_error)?;
    let root = doc.root();
    if root.name() != "CORSConfiguration" {
        return Err(invalid_cors("unexpected OSS CORS root"));
    }
    if root.attributes().next().is_some() {
        return Err(invalid_cors(
            "OSS CORS root contains unsupported attributes",
        ));
    }
    let mut rules = Vec::new();
    let mut response_vary = None;
    for child in root.child_elements() {
        match child.name() {
            "CORSRule" => rules.push(parse_rule(&child)?),
            "ResponseVary" if response_vary.is_none() => {
                response_vary = Some(parse_bool(&child)?);
            }
            _ => return Err(invalid_cors("OSS CORS contains an unsupported field")),
        }
    }
    if rules.len() > CORS_MAX_RULES {
        return Err(invalid_cors("OSS CORS exceeds 20 rules"));
    }
    Ok(CorsConfiguration {
        rules,
        response_vary,
    })
}

fn parse_rule(element: &ElementRef<'_, Arc<[u8]>>) -> Result<CorsRule> {
    if element.attributes().next().is_some() {
        return Err(invalid_cors(
            "OSS CORS rule contains unsupported attributes",
        ));
    }
    let mut rule = CorsRule {
        origins: Vec::new(),
        methods: Vec::new(),
        headers: Vec::new(),
        expose_headers: Vec::new(),
        max_age_seconds: None,
    };
    for child in element.child_elements() {
        if child.attributes().next().is_some() {
            return Err(invalid_cors(
                "OSS CORS field contains unsupported attributes",
            ));
        }
        let value = child
            .text()
            .ok_or_else(|| invalid_cors("OSS CORS field has no value"))?;
        let value = value.trim();
        if value.is_empty() || child.child_elements().next().is_some() {
            return Err(invalid_cors("OSS CORS field has invalid content"));
        }
        match child.name() {
            "AllowedOrigin" => rule.origins.push(value.to_string()),
            "AllowedMethod" => rule.methods.push(value.to_string()),
            "AllowedHeader" => rule.headers.push(value.to_string()),
            "ExposeHeader" => rule.expose_headers.push(value.to_string()),
            "MaxAgeSeconds" if rule.max_age_seconds.is_none() => {
                rule.max_age_seconds = Some(
                    value
                        .parse()
                        .map_err(|_| invalid_cors("invalid OSS CORS MaxAgeSeconds"))?,
                );
            }
            _ => return Err(invalid_cors("OSS CORS rule contains an unsupported field")),
        }
    }
    if rule.origins.is_empty() || rule.methods.is_empty() {
        return Err(invalid_cors("OSS CORS rule lacks origins or methods"));
    }
    Ok(rule)
}

fn parse_bool(element: &ElementRef<'_, Arc<[u8]>>) -> Result<bool> {
    if element.attributes().next().is_some() || element.child_elements().next().is_some() {
        return Err(invalid_cors(
            "OSS CORS ResponseVary contains unsupported content",
        ));
    }
    match element.text().as_deref().map(str::trim) {
        Some("true") => Ok(true),
        Some("false") => Ok(false),
        _ => Err(invalid_cors("invalid OSS CORS ResponseVary")),
    }
}

fn serialize(config: &CorsConfiguration) -> Result<String> {
    if config.rules.len() > CORS_MAX_RULES {
        return Err(invalid_cors("OSS CORS would exceed 20 rules"));
    }
    let mut bytes = Vec::new();
    let mut writer = XmlStreamWriter::new(&mut bytes)
        .map_aster_err_ctx("create OSS CORS XML", AsterError::storage_driver_error)?;
    writer
        .start("CORSConfiguration")
        .map_aster_err_ctx("write OSS CORS root", AsterError::storage_driver_error)?;
    for rule in &config.rules {
        writer
            .start("CORSRule")
            .map_aster_err_ctx("write OSS CORS rule", AsterError::storage_driver_error)?;
        for (name, values) in [
            ("AllowedOrigin", &rule.origins),
            ("AllowedMethod", &rule.methods),
            ("AllowedHeader", &rule.headers),
            ("ExposeHeader", &rule.expose_headers),
        ] {
            for value in values {
                writer
                    .start(name)
                    .map_aster_err_ctx("write OSS CORS field", AsterError::storage_driver_error)?;
                writer
                    .text(value)
                    .map_aster_err_ctx("write OSS CORS value", AsterError::storage_driver_error)?;
                writer
                    .end_element()
                    .map_aster_err_ctx("close OSS CORS field", AsterError::storage_driver_error)?;
            }
        }
        if let Some(value) = rule.max_age_seconds {
            writer
                .start("MaxAgeSeconds")
                .map_aster_err_ctx("write OSS CORS max age", AsterError::storage_driver_error)?;
            writer.text(&value.to_string()).map_aster_err_ctx(
                "write OSS CORS max age value",
                AsterError::storage_driver_error,
            )?;
            writer
                .end_element()
                .map_aster_err_ctx("close OSS CORS max age", AsterError::storage_driver_error)?;
        }
        writer
            .end_element()
            .map_aster_err_ctx("close OSS CORS rule", AsterError::storage_driver_error)?;
    }
    if let Some(value) = config.response_vary {
        writer.start("ResponseVary").map_aster_err_ctx(
            "write OSS CORS ResponseVary",
            AsterError::storage_driver_error,
        )?;
        writer
            .text(if value { "true" } else { "false" })
            .map_aster_err_ctx(
                "write OSS CORS ResponseVary value",
                AsterError::storage_driver_error,
            )?;
        writer.end_element().map_aster_err_ctx(
            "close OSS CORS ResponseVary",
            AsterError::storage_driver_error,
        )?;
    }
    writer
        .end_element()
        .map_aster_err_ctx("close OSS CORS root", AsterError::storage_driver_error)?;
    writer
        .finish()
        .map_aster_err_ctx("finalize OSS CORS XML", AsterError::storage_driver_error)?;
    if bytes.len() > CORS_XML_MAX_BYTES {
        return Err(invalid_cors("OSS CORS XML would exceed 16 KB"));
    }
    String::from_utf8(bytes)
        .map_aster_err_ctx("encode OSS CORS XML", AsterError::storage_driver_error)
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
        .get("x-oss-request-id")?
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
        format!("OSS {operation} failed: HTTP {status}, provider code {code_text}{request_suffix}");
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
            "oss:GetBucketCors"
        } else {
            "oss:PutBucketCors"
        };
        storage_driver_error_with_code(
            kind,
            ApiErrorCode::StoragePermission,
            format!("{message}; grant {permission} on the target bucket"),
        )
    } else {
        AsterError::from(storage_driver_error(kind, message))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::drivers::alibaba_oss::tests::{
        sample_config as sample_driver_config, sample_credentials,
    };
    use chrono::TimeZone;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[test]
    fn selects_current_bucket_and_server_side_endpoint() {
        let mut config = sample_driver_config();
        let credentials = sample_credentials();
        let client = OssCorsClient::new(&config, &credentials).unwrap();
        assert_eq!(client.bucket, "asterdrive-test");
        assert_eq!(
            client.url.as_str(),
            "https://asterdrive-test.oss-cn-hangzhou.aliyuncs.com/?cors"
        );

        config.endpoint = "https://asterdrive-test.oss-cn-hangzhou.aliyuncs.com".into();
        let client = OssCorsClient::new(&config, &credentials).unwrap();
        assert_eq!(
            client.url.as_str(),
            "https://asterdrive-test.oss-cn-hangzhou.aliyuncs.com/?cors"
        );

        config.endpoint = "https://files.example.test".into();
        config.use_cname = true;
        let client = OssCorsClient::new(&config, &credentials).unwrap();
        assert_eq!(client.url.as_str(), "https://files.example.test/?cors");

        config.server_side_endpoint = "https://oss-cn-hangzhou-internal.aliyuncs.com".into();
        let client = OssCorsClient::new(&config, &credentials).unwrap();
        assert_eq!(
            client.url.as_str(),
            "https://asterdrive-test.oss-cn-hangzhou-internal.aliyuncs.com/?cors"
        );
    }

    fn sample_config() -> CorsConfiguration {
        CorsConfiguration {
            rules: vec![CorsRule {
                origins: vec!["https://other.example".into()],
                methods: vec!["GET".into()],
                headers: vec!["Range".into()],
                expose_headers: vec!["ETag".into()],
                max_age_seconds: Some(30),
            }],
            response_vary: Some(true),
        }
    }

    #[test]
    fn preserves_existing_rules_and_order_on_xml_roundtrip() {
        let config = sample_config();
        assert_eq!(
            parse(serialize(&config).unwrap().as_bytes()).unwrap(),
            config
        );
        let mut merged = config.clone();
        merged
            .rules
            .push(desired_rule(&["https://drive.example".into()]));
        let xml = serialize(&merged).unwrap();
        assert_eq!(parse(xml.as_bytes()).unwrap().rules[0], config.rules[0]);
        assert!(xml.find("other.example").unwrap() < xml.find("drive.example").unwrap());
    }

    #[test]
    fn equivalent_rules_ignore_field_order_without_claiming_partial_matches() {
        let desired = desired_rule(&["https://drive.example".into()]);
        let mut reordered = desired.clone();
        reordered.methods.reverse();
        reordered.expose_headers.reverse();
        assert!(equivalent(&desired, &reordered));
        reordered.headers.push("Authorization".into());
        assert!(!equivalent(&desired, &reordered));
    }

    #[test]
    fn rejects_unknown_fields_attributes_and_capacity_before_write() {
        let valid = serialize(&sample_config()).unwrap();
        assert!(
            parse(
                valid
                    .replace("<CORSRule>", "<CORSRule><Unknown>x</Unknown>")
                    .as_bytes()
            )
            .is_err()
        );
        assert!(
            parse(
                valid
                    .replace("<CORSRule>", "<CORSRule unknown=\"x\">")
                    .as_bytes()
            )
            .is_err()
        );
        assert!(
            parse(
                valid
                    .replace("<AllowedMethod>", "<AllowedMethod unknown=\"x\">")
                    .as_bytes()
            )
            .is_err()
        );
        assert!(parse(&vec![b'x'; CORS_XML_MAX_BYTES + 1]).is_err());
        let mut config = sample_config();
        config.rules = vec![config.rules[0].clone(); CORS_MAX_RULES + 1];
        assert!(serialize(&config).is_err());
        config.rules.truncate(CORS_MAX_RULES);
        config.rules[0].origins = vec!["x".repeat(CORS_XML_MAX_BYTES)];
        assert!(serialize(&config).is_err());
    }

    #[test]
    fn bucket_signing_uses_oss_v4_and_cors_query() {
        let now = Utc.with_ymd_and_hms(2023, 12, 16, 12, 0, 0).unwrap();
        let headers = signing::signed_bucket_request_headers(
            signing::OssSigningContext {
                method: "GET",
                bucket: "sample-bucket",
                region: "cn-hangzhou",
                access_key: "ak",
                secret_key: "sk",
                now,
            },
            None,
            None,
        )
        .unwrap();
        let auth = headers
            .iter()
            .find(|(name, _)| name == "authorization")
            .unwrap();
        assert_eq!(
            auth.1,
            "OSS4-HMAC-SHA256 Credential=ak/20231216/cn-hangzhou/oss/aliyun_v4_request,Signature=769c7f9a227bbb7f2d712cef5d6c43a4a3d46464d55ec89f62d4cab89b23c472"
        );
        assert!(!auth.1.contains("sk"));
    }

    async fn read_request(stream: &mut tokio::net::TcpStream) -> String {
        let mut bytes = Vec::new();
        let mut buffer = [0u8; 4096];
        loop {
            let count = stream.read(&mut buffer).await.unwrap();
            assert!(count > 0);
            bytes.extend_from_slice(&buffer[..count]);
            if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&bytes[..end]);
                let length = headers
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length: ")
                            .and_then(|value| value.parse::<usize>().ok())
                    })
                    .unwrap_or(0);
                if bytes.len() >= end + 4 + length {
                    break;
                }
            }
        }
        String::from_utf8(bytes).unwrap()
    }

    #[tokio::test]
    async fn get_then_put_uses_bucket_cors_endpoint_and_preserves_rules() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let mut url = Url::parse(&endpoint).unwrap();
        url.set_query(Some("cors"));
        let client = OssCorsClient {
            client: aster_forge_http::HttpClient::builder()
                .user_agent(crate::config::OUTBOUND_HTTP_USER_AGENT)
                .build()
                .unwrap(),
            url,
            bucket: "sample-bucket".into(),
            region: "cn-hangzhou".into(),
            access_key: "ak".into(),
            secret_key: "sk".into(),
        };
        let server = tokio::spawn(async move {
            let (mut get, _) = listener.accept().await.unwrap();
            let request = read_request(&mut get).await;
            assert!(request.starts_with("GET /?cors HTTP/1.1"));
            assert!(request.lines().any(|line| {
                line.split_once(':').is_some_and(|(name, value)| {
                    name.eq_ignore_ascii_case("user-agent")
                        && value.trim() == crate::config::OUTBOUND_HTTP_USER_AGENT
                })
            }));
            assert!(
                request
                    .to_ascii_lowercase()
                    .contains("authorization: oss4-hmac-sha256")
            );
            let body = serialize(&sample_config()).unwrap();
            get.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nx-oss-request-id: GET123\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
            let (mut put, _) = listener.accept().await.unwrap();
            let request = read_request(&mut put).await;
            assert!(request.starts_with("PUT /?cors HTTP/1.1"));
            assert!(request.to_ascii_lowercase().contains("content-md5:"));
            assert!(
                request
                    .to_ascii_lowercase()
                    .contains("authorization: oss4-hmac-sha256")
            );
            let body = request.split("\r\n\r\n").nth(1).unwrap();
            let config = parse(body.as_bytes()).unwrap();
            assert_eq!(config.rules[0], sample_config().rules[0]);
            assert_eq!(
                config.rules[1],
                desired_rule(&["https://drive.example".into()])
            );
            put.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nx-oss-request-id: PUT123\r\nConnection: close\r\n\r\n").await.unwrap();
        });
        let result = client
            .configure(&["https://drive.example".into()])
            .await
            .unwrap();
        assert!(result.changed);
        assert_eq!(result.preserved_rule_count, 1);
        assert_eq!(result.request_id.as_deref(), Some("PUT123"));
        server.await.unwrap();
    }

    #[tokio::test]
    async fn equivalent_rule_skips_put_and_read_failure_never_overwrites() {
        let origins = vec!["https://drive.example".into()];
        let mut shadow = desired_rule(&origins);
        shadow.methods = vec!["PUT".into()];
        shadow.headers = vec!["Content-*".into()];
        shadow.expose_headers.clear();
        for (body, status, expected_error) in [
            (
                serialize(&CorsConfiguration {
                    rules: vec![desired_rule(&["https://drive.example".into()])],
                    response_vary: None,
                })
                .unwrap(),
                "200 OK",
                None,
            ),
            (
                "<Error><Code>AccessDenied</Code><Message>private</Message></Error>".into(),
                "403 Forbidden",
                Some("oss:GetBucketCors"),
            ),
            (
                serialize(&CorsConfiguration {
                    rules: vec![shadow, desired_rule(&origins)],
                    response_vary: None,
                })
                .unwrap(),
                "200 OK",
                Some("no rules were written"),
            ),
        ] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let mut url =
                Url::parse(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
            url.set_query(Some("cors"));
            let client = OssCorsClient {
                client: aster_forge_http::HttpClient::builder()
                    .user_agent(crate::config::OUTBOUND_HTTP_USER_AGENT)
                    .build()
                    .unwrap(),
                url,
                bucket: "sample-bucket".into(),
                region: "cn-hangzhou".into(),
                access_key: "ak".into(),
                secret_key: "sk".into(),
            };
            let server = tokio::spawn(async move {
                let (mut get, _) = listener.accept().await.unwrap();
                assert!(
                    read_request(&mut get)
                        .await
                        .starts_with("GET /?cors HTTP/1.1")
                );
                get.write_all(
                    format!(
                        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
                assert!(
                    tokio::time::timeout(std::time::Duration::from_millis(100), listener.accept())
                        .await
                        .is_err(),
                    "GET must not be followed by PUT"
                );
            });
            let result = client.configure(&["https://drive.example".into()]).await;
            match expected_error {
                None => assert!(!result.unwrap().changed),
                Some(expected) => {
                    let error = result.err().expect("configuration must fail").to_string();
                    assert!(error.contains(expected), "{error}");
                    assert!(!error.contains("private"));
                }
            }
            server.await.unwrap();
        }
    }

    #[tokio::test]
    async fn no_configuration_creates_rule_and_write_failure_is_redacted() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut url = Url::parse(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
        url.set_query(Some("cors"));
        let client = OssCorsClient {
            client: aster_forge_http::HttpClient::builder()
                .user_agent(crate::config::OUTBOUND_HTTP_USER_AGENT)
                .build()
                .unwrap(),
            url,
            bucket: "sample-bucket".into(),
            region: "cn-hangzhou".into(),
            access_key: "ak".into(),
            secret_key: "sk".into(),
        };
        let server = tokio::spawn(async move {
            let (mut get, _) = listener.accept().await.unwrap();
            read_request(&mut get).await;
            let body = "<Error><Code>NoSuchCORSConfiguration</Code></Error>";
            get.write_all(format!("HTTP/1.1 404 Not Found\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
            let (mut put, _) = listener.accept().await.unwrap();
            let request = read_request(&mut put).await;
            let config = parse(request.split("\r\n\r\n").nth(1).unwrap().as_bytes()).unwrap();
            assert_eq!(config.rules.len(), 1);
            assert_eq!(
                config.rules[0],
                desired_rule(&["https://drive.example".into()])
            );
            let body = "<Error><Code>AccessDenied</Code><Message>private</Message></Error>";
            put.write_all(format!("HTTP/1.1 403 Forbidden\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
        });
        let error = client
            .configure(&["https://drive.example".into()])
            .await
            .err()
            .unwrap()
            .to_string();
        assert!(error.contains("oss:PutBucketCors"));
        assert!(!error.contains("private"));
        server.await.unwrap();
    }

    #[test]
    fn rejects_rules_that_shadow_required_reads_before_appending() {
        let shadowing = CorsRule {
            origins: vec!["*".into()],
            methods: vec!["GET".into(), "HEAD".into()],
            headers: vec!["*".into()],
            expose_headers: vec!["Content-Length".into()],
            max_age_seconds: Some(60),
        };
        let error = needs_rule(
            std::slice::from_ref(&shadowing),
            &["https://drive.example".into()],
        )
        .expect_err("a first matching rule without ETag must block the write");
        assert!(error.to_string().contains("earlier matching OSS CORS rule"));

        let desired = desired_rule(&["https://drive.example".into()]);
        let error = needs_rule(&[shadowing, desired], &["https://drive.example".into()])
            .expect_err("a later equivalent rule cannot bypass an earlier shadowing rule");
        assert!(error.to_string().contains("no rules were written"));

        let mut put_shadow = desired_rule(&["https://drive.example".into()]);
        put_shadow.expose_headers.retain(|header| header != "ETag");
        put_shadow.methods = vec!["PUT".into()];
        let error = needs_rule(&[put_shadow], &["https://drive.example".into()])
            .expect_err("a matching PUT rule without ETag must block multipart uploads");
        assert!(error.to_string().contains("no rules were written"));
    }

    #[test]
    fn partial_header_wildcards_cannot_hide_an_earlier_put_rule() {
        let origins = vec!["https://drive.example".into()];
        let desired = desired_rule(&origins);
        for pattern in ["Content-*", "CONTENT-*", "*-Type", "cOn*TyPe", "*"] {
            let mut shadow = desired.clone();
            shadow.methods = vec!["PUT".into()];
            shadow.headers = vec![pattern.into()];
            shadow.expose_headers.clear();
            assert!(rule_matches(&shadow, &origins[0], "PUT", &["content-type"]));
            assert!(
                needs_rule(&[shadow, desired.clone()], &origins).is_err(),
                "{pattern}"
            );
        }
        let mut put = desired.clone();
        put.methods = vec!["PUT".into()];
        put.headers = vec!["Content-*".into()];
        put.expose_headers = vec!["etag".into()];
        assert!(!needs_rule(&[put.clone(), desired], &origins).unwrap());
        put.headers = vec!["X-Custom-*".into()];
        assert!(!rule_matches(&put, &origins[0], "PUT", &["content-type"]));
        put.headers = vec!["con**type".into()];
        assert!(!rule_matches(&put, &origins[0], "PUT", &["content-type"]));
    }

    #[test]
    fn appends_when_existing_rules_leave_a_request_unmatched() {
        let existing = CorsRule {
            origins: vec!["https://drive.example".into()],
            methods: vec!["GET".into(), "HEAD".into()],
            headers: vec!["*".into()],
            expose_headers: REQUIRED_EXPOSE_HEADERS
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
            max_age_seconds: Some(60),
        };
        assert!(needs_rule(&[existing], &["https://drive.example".into()]).unwrap());
    }

    #[test]
    fn response_errors_are_redacted_and_actionable() {
        let error = response_error(
            StatusCode::FORBIDDEN,
            b"<Error><Code>AccessDenied</Code><Message>secret</Message></Error>",
            "PutBucketCors",
            None,
        );
        assert!(error.to_string().contains("oss:PutBucketCors"));
        assert!(!error.to_string().contains("secret"));
        assert!(!error.to_string().contains("<Error>"));

        for provider_code in [
            "InvalidAccessKeyId",
            "SignatureDoesNotMatch",
            "RequestTimeTooSkewed",
        ] {
            let body = format!("<Error><Code>{provider_code}</Code></Error>");
            let error = response_error(
                StatusCode::FORBIDDEN,
                body.as_bytes(),
                "GetBucketCors",
                None,
            );
            let message = error.to_string();
            assert!(message.contains(provider_code));
            assert!(!message.contains("grant oss:GetBucketCors"));
        }
    }
}
