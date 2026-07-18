use std::time::{SystemTime, UNIX_EPOCH};

use aster_forge_xml::{XmlEvent, XmlSafetyPolicy, XmlWalkError, XmlWriter, walk_validated_xml};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use md5::{Digest as Md5Digest, Md5};
use reqwest::StatusCode;
use reqwest::header::CONTENT_TYPE;

use crate::api::api_error_code::ApiErrorCode;
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::storage::error::{
    StorageErrorKind, storage_driver_error, storage_driver_error_with_code,
};
use crate::storage::http_body::read_reqwest_response_body_limited;

use super::TencentCosDriver;

pub(crate) const ASTERDRIVE_COS_CORS_RULE_ID: &str = "asterdrive-presigned-access";
const CORS_XML_CONTENT_TYPE: &str = "application/xml";
const CONTENT_MD5_HEADER: &str = "Content-MD5";

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
        let preserved_rule_count = existing
            .rules
            .iter()
            .filter(|rule| rule.id.as_deref() != Some(ASTERDRIVE_COS_CORS_RULE_ID))
            .count();
        let replaced_existing_rule = preserved_rule_count != existing.rules.len();
        existing
            .rules
            .retain(|rule| rule.id.as_deref() != Some(ASTERDRIVE_COS_CORS_RULE_ID));
        existing.rules.push(asterdrive_cors_rule(allowed_origins));
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
        let body = read_reqwest_response_body_limited(
            response,
            "read COS GET Bucket cors response",
            XmlSafetyPolicy::untrusted().max_input_bytes,
            AsterError::storage_driver_error,
        )
        .await?;
        let body = String::from_utf8(body).map_err(|_| {
            AsterError::storage_driver_error("COS GET Bucket cors response is not UTF-8")
        })?;

        if status == StatusCode::NOT_FOUND {
            return Ok(CosCorsConfiguration {
                rules: Vec::new(),
                response_vary: None,
            });
        }
        if !status.is_success() {
            return Err(cos_cors_response_error(status, &body, "GET Bucket cors"));
        }
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
            .map(str::to_string);
        let body = read_reqwest_response_body_limited(
            response,
            "read COS PUT Bucket cors response",
            XmlSafetyPolicy::untrusted().max_input_bytes,
            AsterError::storage_driver_error,
        )
        .await?;
        let body = String::from_utf8(body).map_err(|_| {
            AsterError::storage_driver_error("COS PUT Bucket cors response is not UTF-8")
        })?;

        if status.is_success() {
            return Ok(request_id);
        }
        Err(cos_cors_response_error(status, &body, "PUT Bucket cors"))
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

pub(crate) fn build_cors_configuration_xml(config: &CosCorsConfiguration) -> Result<String> {
    let mut bytes = Vec::new();
    let mut writer = XmlWriter::new(&mut bytes);
    writer
        .declaration("1.0", Some("UTF-8"))
        .map_aster_err_ctx("serialize COS CORS XML", AsterError::storage_driver_error)?;
    writer
        .start("CORSConfiguration", std::iter::empty())
        .map_aster_err_ctx("serialize COS CORS XML", AsterError::storage_driver_error)?;
    for rule in &config.rules {
        write_cors_rule(&mut writer, rule)?;
    }
    if let Some(response_vary) = config.response_vary {
        write_text_element(
            &mut writer,
            "ResponseVary",
            if response_vary { "true" } else { "false" },
        )?;
    }
    writer
        .end("CORSConfiguration")
        .map_aster_err_ctx("serialize COS CORS XML", AsterError::storage_driver_error)?;
    writer
        .finish()
        .map_aster_err_ctx("serialize COS CORS XML", AsterError::storage_driver_error)?;
    String::from_utf8(bytes)
        .map_aster_err_ctx("encode COS CORS XML", AsterError::storage_driver_error)
}

pub(crate) fn parse_cors_configuration(body: &str) -> Result<CosCorsConfiguration> {
    let mut stack = Vec::<String>::new();
    let mut text = Vec::<String>::new();
    let mut rules = Vec::new();
    let mut current_rule = None;
    let mut response_vary = None;
    let mut root_seen = false;

    walk_validated_xml(body.as_bytes(), XmlSafetyPolicy::untrusted(), |event| {
        match event {
            XmlEvent::Start(element) => {
                let name = element.local_name().to_string();
                if !root_seen {
                    root_seen = true;
                    if name != "CORSConfiguration" {
                        return Err(storage_driver_error(
                            StorageErrorKind::Misconfigured,
                            "COS CORS XML root is not CORSConfiguration",
                        ));
                    }
                }
                if name == "CORSRule" {
                    current_rule = Some(CosCorsRule {
                        id: None,
                        allowed_origins: Vec::new(),
                        allowed_methods: Vec::new(),
                        allowed_headers: Vec::new(),
                        expose_headers: Vec::new(),
                        max_age_seconds: None,
                    });
                }
                stack.push(name);
                text.push(String::new());
            }
            XmlEvent::Text(value) | XmlEvent::CData(value) => {
                if let Some(text) = text.last_mut() {
                    text.push_str(&value);
                }
            }
            XmlEvent::End { .. } => {
                let Some(name) = stack.pop() else {
                    return Err(AsterError::storage_driver_error("parse COS CORS XML"));
                };
                let value = text.pop().unwrap_or_default().trim().to_string();
                let parent = stack.last().map(String::as_str);
                if parent == Some("CORSRule") {
                    if let Some(rule) = current_rule.as_mut() {
                        apply_cors_rule_value(rule, &name, value);
                    }
                } else if parent == Some("CORSConfiguration") && name == "ResponseVary" {
                    response_vary = (!value.is_empty()).then(|| value.eq_ignore_ascii_case("true"));
                }
                if name == "CORSRule"
                    && let Some(rule) = current_rule.take()
                {
                    rules.push(rule);
                }
            }
            _ => {}
        }
        Ok(())
    })
    .map_err(|error| match error {
        XmlWalkError::Xml(error) => AsterError::storage_driver_error(error.to_string()),
        XmlWalkError::Visitor(error) => error,
    })?;
    Ok(CosCorsConfiguration {
        rules,
        response_vary,
    })
}

fn write_cors_rule<W: std::io::Write>(writer: &mut XmlWriter<W>, rule: &CosCorsRule) -> Result<()> {
    writer
        .start("CORSRule", std::iter::empty())
        .map_aster_err_ctx("serialize COS CORS XML", AsterError::storage_driver_error)?;
    if let Some(id) = &rule.id {
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
    if let Some(max_age_seconds) = rule.max_age_seconds {
        write_text_element(writer, "MaxAgeSeconds", &max_age_seconds.to_string())?;
    }
    writer
        .end("CORSRule")
        .map_aster_err_ctx("serialize COS CORS XML", AsterError::storage_driver_error)
}

fn write_text_element<W: std::io::Write>(
    writer: &mut XmlWriter<W>,
    name: &str,
    value: &str,
) -> Result<()> {
    writer
        .text_element(name, value)
        .map_aster_err_ctx("serialize COS CORS XML", AsterError::storage_driver_error)
}

fn apply_cors_rule_value(rule: &mut CosCorsRule, name: &str, value: String) {
    if value.is_empty() {
        return;
    }
    match name {
        "ID" => {
            rule.id.get_or_insert(value);
        }
        "AllowedOrigin" => rule.allowed_origins.push(value),
        "AllowedMethod" => rule.allowed_methods.push(value),
        "AllowedHeader" => rule.allowed_headers.push(value),
        "ExposeHeader" => rule.expose_headers.push(value),
        "MaxAgeSeconds" => rule.max_age_seconds = value.parse().ok(),
        _ => {}
    }
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

fn cos_cors_response_error(status: StatusCode, body: &str, action: &str) -> AsterError {
    let code = extract_xml_tag(body, "Code");
    let message = extract_xml_tag(body, "Message").unwrap_or_else(|| {
        body.trim()
            .chars()
            .take(300)
            .collect::<String>()
            .trim()
            .to_string()
    });
    let request_id = extract_xml_tag(body, "RequestId")
        .map(|id| format!(" request_id={id}"))
        .unwrap_or_default();
    let error_code = code.map(|code| format!(" code={code}")).unwrap_or_default();
    let detail = if message.is_empty() {
        format!("Tencent COS {action} failed with HTTP {status}{error_code}{request_id}")
    } else {
        format!("Tencent COS {action} failed with HTTP {status}{error_code}{request_id}: {message}")
    };

    let kind = match status {
        StatusCode::BAD_REQUEST => StorageErrorKind::Misconfigured,
        StatusCode::UNAUTHORIZED => StorageErrorKind::Auth,
        StatusCode::FORBIDDEN => StorageErrorKind::Permission,
        StatusCode::PRECONDITION_FAILED | StatusCode::CONFLICT => StorageErrorKind::Precondition,
        StatusCode::TOO_MANY_REQUESTS => StorageErrorKind::RateLimited,
        status if status.is_server_error() => StorageErrorKind::Transient,
        _ => StorageErrorKind::Unknown,
    };
    if kind == StorageErrorKind::Permission {
        storage_driver_error_with_code(
            kind,
            ApiErrorCode::StoragePermission,
            format!("{detail}. The Tencent COS credential needs name/cos:PutBucketCORS."),
        )
    } else {
        storage_driver_error(kind, detail)
    }
}

fn extract_xml_tag(body: &str, tag: &str) -> Option<String> {
    let mut stack = Vec::<String>::new();
    let mut text = Vec::<String>::new();
    let mut found = None;
    walk_validated_xml(body.as_bytes(), XmlSafetyPolicy::untrusted(), |event| {
        match event {
            XmlEvent::Start(element) => {
                stack.push(element.local_name().to_string());
                text.push(String::new());
            }
            XmlEvent::Text(value) | XmlEvent::CData(value) => {
                if let Some(text) = text.last_mut() {
                    text.push_str(&value);
                }
            }
            XmlEvent::End { .. } => {
                let Some(name) = stack.pop() else {
                    return Err(());
                };
                let value = text.pop().unwrap_or_default().trim().to_string();
                if name == tag && !value.is_empty() {
                    found.get_or_insert(value);
                }
            }
            _ => {}
        }
        Ok(())
    })
    .ok()?;
    found
}

#[cfg(test)]
mod tests {
    use reqwest::StatusCode;

    use aster_forge_xml::{DEFAULT_XML_MAX_DEPTH, DEFAULT_XML_MAX_TEXT_BYTES};

    use crate::api::api_error_code::ApiErrorCode;

    use super::{
        ASTERDRIVE_COS_CORS_RULE_ID, CosCorsConfiguration, asterdrive_cors_rule,
        build_cors_configuration_xml, content_md5_base64, cos_cors_response_error,
        parse_cors_configuration,
    };

    #[test]
    fn asterdrive_cors_xml_contains_browser_direct_access_headers() {
        let config = CosCorsConfiguration {
            rules: vec![asterdrive_cors_rule(&[
                "https://drive.example.com".to_string(),
                "https://admin.example.com".to_string(),
            ])],
            response_vary: Some(true),
        };

        let xml = build_cors_configuration_xml(&config).expect("CORS XML");

        assert!(xml.contains("<ID>asterdrive-presigned-access</ID>"));
        assert!(xml.contains("<AllowedOrigin>https://drive.example.com</AllowedOrigin>"));
        assert!(xml.contains("<AllowedOrigin>https://admin.example.com</AllowedOrigin>"));
        assert!(xml.contains("<AllowedMethod>PUT</AllowedMethod>"));
        assert!(xml.contains("<AllowedMethod>GET</AllowedMethod>"));
        assert!(xml.contains("<AllowedMethod>HEAD</AllowedMethod>"));
        assert!(xml.contains("<AllowedHeader>*</AllowedHeader>"));
        assert!(xml.contains("<ExposeHeader>ETag</ExposeHeader>"));
        assert!(xml.contains("<ExposeHeader>Content-Range</ExposeHeader>"));
        assert!(xml.contains("<ExposeHeader>Content-Disposition</ExposeHeader>"));
        assert!(xml.contains("<MaxAgeSeconds>600</MaxAgeSeconds>"));
        assert!(xml.contains("<ResponseVary>true</ResponseVary>"));
    }

    #[test]
    fn parses_and_preserves_existing_cos_cors_rules() {
        let xml = r#"
<CORSConfiguration>
  <CORSRule>
    <ID>other-app</ID>
    <AllowedOrigin>https://other.example.com</AllowedOrigin>
    <AllowedMethod>GET</AllowedMethod>
    <AllowedHeader>Authorization</AllowedHeader>
    <ExposeHeader>ETag</ExposeHeader>
    <MaxAgeSeconds>300</MaxAgeSeconds>
  </CORSRule>
  <ResponseVary>false</ResponseVary>
</CORSConfiguration>
"#;

        let parsed = parse_cors_configuration(xml).expect("parse CORS XML");

        assert_eq!(parsed.rules.len(), 1);
        assert_eq!(parsed.rules[0].id.as_deref(), Some("other-app"));
        assert_eq!(
            parsed.rules[0].allowed_origins,
            vec!["https://other.example.com".to_string()]
        );
        assert_eq!(parsed.response_vary, Some(false));
        assert_ne!(
            parsed.rules[0].id.as_deref(),
            Some(ASTERDRIVE_COS_CORS_RULE_ID)
        );
    }

    #[test]
    fn parses_namespaced_cos_cors_xml_and_ignores_blank_values() {
        let xml = r#"
<cos:CORSConfiguration xmlns:cos="http://cos.example.com/doc">
  <cos:CORSRule>
    <cos:ID>  </cos:ID>
    <cos:AllowedOrigin>https://drive.example.com</cos:AllowedOrigin>
    <cos:AllowedOrigin>  </cos:AllowedOrigin>
    <cos:AllowedMethod>PUT</cos:AllowedMethod>
    <cos:AllowedHeader>*</cos:AllowedHeader>
    <cos:ExposeHeader>x-cos-request-id</cos:ExposeHeader>
    <cos:MaxAgeSeconds>not-a-number</cos:MaxAgeSeconds>
  </cos:CORSRule>
  <cos:ResponseVary>TRUE</cos:ResponseVary>
</cos:CORSConfiguration>
"#;

        let parsed = parse_cors_configuration(xml).expect("parse namespaced CORS XML");

        assert_eq!(parsed.rules.len(), 1);
        assert_eq!(parsed.rules[0].id, None);
        assert_eq!(
            parsed.rules[0].allowed_origins,
            vec!["https://drive.example.com".to_string()]
        );
        assert_eq!(parsed.rules[0].max_age_seconds, None);
        assert_eq!(parsed.response_vary, Some(true));
    }

    #[test]
    fn rejects_xml_with_unexpected_root() {
        let error = parse_cors_configuration("<Error><Code>NoSuchCORSConfiguration</Code></Error>")
            .expect_err("unexpected root should fail");

        assert!(error.message().contains("root is not CORSConfiguration"));
    }

    #[test]
    fn cors_parser_covers_xml_safety_boundaries() {
        for xml in [
            "<!DOCTYPE CORSConfiguration><CORSConfiguration/>",
            "junk<CORSConfiguration/>",
            "<CORSConfiguration/>junk",
        ] {
            assert!(parse_cors_configuration(xml).is_err());
        }

        let comment = "x".repeat(DEFAULT_XML_MAX_TEXT_BYTES + 1);
        assert!(
            parse_cors_configuration(&format!("<!--{comment}--><CORSConfiguration/>")).is_err()
        );

        let mut exact_depth = String::from("<CORSConfiguration>");
        for _ in 1..DEFAULT_XML_MAX_DEPTH {
            exact_depth.push_str("<x>");
        }
        for _ in 1..DEFAULT_XML_MAX_DEPTH {
            exact_depth.push_str("</x>");
        }
        exact_depth.push_str("</CORSConfiguration>");
        assert!(parse_cors_configuration(&exact_depth).is_ok());

        let mut over_depth = String::from("<CORSConfiguration>");
        for _ in 0..DEFAULT_XML_MAX_DEPTH {
            over_depth.push_str("<x>");
        }
        for _ in 0..DEFAULT_XML_MAX_DEPTH {
            over_depth.push_str("</x>");
        }
        over_depth.push_str("</CORSConfiguration>");
        assert!(parse_cors_configuration(&over_depth).is_err());
    }

    #[test]
    fn cors_parser_accepts_exact_attribute_budget_and_rejects_one_more() {
        let mut attributes = String::new();
        for index in 0..64 {
            attributes.push_str(&format!("x{index}=\"value\" "));
        }
        assert!(parse_cors_configuration(&format!("<CORSConfiguration {attributes}/>")).is_ok());
        attributes.push_str("overflow=\"value\" ");
        assert!(parse_cors_configuration(&format!("<CORSConfiguration {attributes}/>")).is_err());
    }

    #[test]
    fn maps_cos_cors_permission_error_to_storage_permission_code() {
        let error = cos_cors_response_error(
            StatusCode::FORBIDDEN,
            r#"<Error><Code>AccessDenied</Code><Message>Forbidden.</Message><RequestId>req-1</RequestId></Error>"#,
            "PUT Bucket cors",
        );

        assert_eq!(
            error.api_error_code_override(),
            Some(ApiErrorCode::StoragePermission)
        );
        assert!(error.message().contains("name/cos:PutBucketCORS"));
        assert!(error.message().contains("request_id=req-1"));
    }

    #[test]
    fn maps_cos_cors_bad_request_to_storage_misconfigured_code() {
        let error = cos_cors_response_error(
            StatusCode::BAD_REQUEST,
            r#"<Error><Code>InvalidRequest</Code><Message>Missing required header for this request: Content-MD5</Message><RequestId>req-2</RequestId></Error>"#,
            "PUT Bucket cors",
        );

        assert_eq!(error.api_error_code(), ApiErrorCode::StorageMisconfigured);
        assert!(error.message().contains("Missing required header"));
        assert!(error.message().contains("code=InvalidRequest"));
        assert!(error.message().contains("request_id=req-2"));
    }

    #[test]
    fn content_md5_base64_matches_cos_required_header_format() {
        assert_eq!(content_md5_base64(b"hello"), "XUFAKrxLKna5cZ2REBfFkg==");
    }
}
