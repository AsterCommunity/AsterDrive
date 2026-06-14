use std::io::Cursor;
use std::time::{SystemTime, UNIX_EPOCH};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use md5::{Digest as Md5Digest, Md5};
use reqwest::StatusCode;
use reqwest::header::CONTENT_TYPE;
use xmltree::{Element, EmitterConfig, XMLNode};

use crate::api::api_error_code::ApiErrorCode;
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::storage::error::{
    StorageErrorKind, storage_driver_error, storage_driver_error_with_code,
};

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
        let body = response.text().await.map_aster_err_ctx(
            "read COS GET Bucket cors response",
            AsterError::storage_driver_error,
        )?;

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
        let body = response.text().await.map_aster_err_ctx(
            "read COS PUT Bucket cors response",
            AsterError::storage_driver_error,
        )?;

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
    let mut root = Element::new("CORSConfiguration");
    for rule in &config.rules {
        root.children
            .push(XMLNode::Element(cors_rule_element(rule)));
    }
    if let Some(response_vary) = config.response_vary {
        push_text_child(
            &mut root,
            "ResponseVary",
            if response_vary { "true" } else { "false" },
        );
    }

    let mut bytes = Vec::new();
    root.write_with_config(
        &mut bytes,
        EmitterConfig::new()
            .perform_indent(true)
            .write_document_declaration(true),
    )
    .map_aster_err_ctx("serialize COS CORS XML", AsterError::storage_driver_error)?;
    String::from_utf8(bytes)
        .map_aster_err_ctx("encode COS CORS XML", AsterError::storage_driver_error)
}

pub(crate) fn parse_cors_configuration(body: &str) -> Result<CosCorsConfiguration> {
    let root = Element::parse(Cursor::new(body.as_bytes()))
        .map_aster_err_ctx("parse COS CORS XML", AsterError::storage_driver_error)?;
    if !xml_name_matches(&root.name, "CORSConfiguration") {
        return Err(storage_driver_error(
            StorageErrorKind::Misconfigured,
            "COS CORS XML root is not CORSConfiguration",
        ));
    }

    let mut rules = Vec::new();
    for child in root.children.iter().filter_map(as_element) {
        if xml_name_matches(&child.name, "CORSRule") {
            rules.push(parse_cors_rule(child));
        }
    }

    Ok(CosCorsConfiguration {
        rules,
        response_vary: child_text(&root, "ResponseVary")
            .map(|value| value.eq_ignore_ascii_case("true")),
    })
}

fn cors_rule_element(rule: &CosCorsRule) -> Element {
    let mut element = Element::new("CORSRule");
    if let Some(id) = &rule.id {
        push_text_child(&mut element, "ID", id);
    }
    for value in &rule.allowed_origins {
        push_text_child(&mut element, "AllowedOrigin", value);
    }
    for value in &rule.allowed_methods {
        push_text_child(&mut element, "AllowedMethod", value);
    }
    for value in &rule.allowed_headers {
        push_text_child(&mut element, "AllowedHeader", value);
    }
    for value in &rule.expose_headers {
        push_text_child(&mut element, "ExposeHeader", value);
    }
    if let Some(value) = rule.max_age_seconds {
        push_text_child(&mut element, "MaxAgeSeconds", &value.to_string());
    }
    element
}

fn parse_cors_rule(element: &Element) -> CosCorsRule {
    CosCorsRule {
        id: child_text(element, "ID"),
        allowed_origins: child_texts(element, "AllowedOrigin"),
        allowed_methods: child_texts(element, "AllowedMethod"),
        allowed_headers: child_texts(element, "AllowedHeader"),
        expose_headers: child_texts(element, "ExposeHeader"),
        max_age_seconds: child_text(element, "MaxAgeSeconds")
            .and_then(|value| value.parse::<u32>().ok()),
    }
}

fn push_text_child(element: &mut Element, name: &str, value: &str) {
    let mut child = Element::new(name);
    child.children.push(XMLNode::Text(value.to_string()));
    element.children.push(XMLNode::Element(child));
}

fn child_texts(element: &Element, name: &str) -> Vec<String> {
    element
        .children
        .iter()
        .filter_map(as_element)
        .filter(|child| xml_name_matches(&child.name, name))
        .filter_map(element_text)
        .collect()
}

fn child_text(element: &Element, name: &str) -> Option<String> {
    child_texts(element, name).into_iter().next()
}

fn element_text(element: &Element) -> Option<String> {
    let text = element.get_text()?.trim().to_string();
    (!text.is_empty()).then_some(text)
}

fn as_element(node: &XMLNode) -> Option<&Element> {
    match node {
        XMLNode::Element(element) => Some(element),
        _ => None,
    }
}

fn xml_name_matches(actual: &str, expected: &str) -> bool {
    actual
        .rsplit_once(':')
        .map(|(_, local)| local)
        .unwrap_or(actual)
        == expected
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
    let root = Element::parse(Cursor::new(body.as_bytes())).ok()?;
    find_xml_tag_text(&root, tag)
}

fn find_xml_tag_text(element: &Element, tag: &str) -> Option<String> {
    if xml_name_matches(&element.name, tag) {
        return element_text(element);
    }
    element
        .children
        .iter()
        .filter_map(as_element)
        .find_map(|child| find_xml_tag_text(child, tag))
}

#[cfg(test)]
mod tests {
    use reqwest::StatusCode;

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
