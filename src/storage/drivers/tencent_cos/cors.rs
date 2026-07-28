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
use crate::http::read_reqwest_body_limited;
use aster_drive_storage::error::{StorageErrorKind, storage_driver_error};

use super::{TencentCosDriver, non_empty_xml_text};

pub(crate) const ASTERDRIVE_COS_CORS_RULE_ID: &str = "asterdrive-presigned-access";
const CORS_XML_CONTENT_TYPE: &str = "application/xml";
const CONTENT_MD5_HEADER: &str = "Content-MD5";
const COS_CORS_XML_MAX_BYTES: usize = 64 * 1024;
const COS_ERROR_XML_MAX_BYTES: usize = 16 * 1024;

fn cors_xml_options() -> ParseOptions {
    ParseOptions::new()
        .max_size(COS_CORS_XML_MAX_BYTES)
        .max_depth(16)
        .max_elements(500)
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
        if status == StatusCode::NOT_FOUND {
            return Ok(CosCorsConfiguration {
                rules: Vec::new(),
                response_vary: None,
            });
        }
        let body_limit = if status.is_success() {
            COS_CORS_XML_MAX_BYTES
        } else {
            COS_ERROR_XML_MAX_BYTES
        };
        let body = read_reqwest_body_limited(
            response,
            "COS GET Bucket cors response body",
            body_limit,
            AsterError::storage_driver_error,
        )
        .await?;
        let body = String::from_utf8(body).map_aster_err_ctx(
            "decode COS GET Bucket cors response",
            AsterError::storage_driver_error,
        )?;

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
        if status.is_success() {
            return Ok(request_id);
        }
        let body = read_reqwest_body_limited(
            response,
            "COS PUT Bucket cors response body",
            COS_ERROR_XML_MAX_BYTES,
            AsterError::storage_driver_error,
        )
        .await?;
        let body = String::from_utf8(body).map_aster_err_ctx(
            "decode COS PUT Bucket cors response",
            AsterError::storage_driver_error,
        )?;

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
    for child in root.children() {
        if let NodeRef::Element(child) = child
            && child.name() == "CORSRule"
        {
            rules.push(parse_cors_rule(&child));
        }
    }

    Ok(CosCorsConfiguration {
        rules,
        response_vary: child_text(&root, "ResponseVary")
            .map(|value| value.eq_ignore_ascii_case("true")),
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

fn parse_cors_rule(element: &ElementRef<'_, Arc<[u8]>>) -> CosCorsRule {
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
mod tests {
    use reqwest::StatusCode;

    use crate::api::api_error_code::ApiErrorCode;

    use super::{
        ASTERDRIVE_COS_CORS_RULE_ID, COS_CORS_XML_MAX_BYTES, COS_ERROR_XML_MAX_BYTES,
        CosCorsConfiguration, asterdrive_cors_rule, build_cors_configuration_xml,
        content_md5_base64, cos_cors_response_error, extract_xml_tag, parse_cors_configuration,
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
    fn cors_xml_roundtrip_escapes_text_and_preserves_all_fields() {
        let config = CosCorsConfiguration {
            rules: vec![super::CosCorsRule {
                id: Some("rule<&>\"'".to_string()),
                allowed_origins: vec!["https://example.com/?a=1&b=<two>".to_string()],
                allowed_methods: vec!["PUT".to_string()],
                allowed_headers: vec!["x-test<&>".to_string()],
                expose_headers: vec!["ETag".to_string()],
                max_age_seconds: Some(123),
            }],
            response_vary: Some(false),
        };

        let xml = build_cors_configuration_xml(&config).expect("CORS XML should serialize");
        assert!(xml.contains("&lt;"));
        assert!(xml.contains("&amp;"));
        assert_eq!(
            parse_cors_configuration(&xml).expect("serialized CORS XML should parse"),
            config
        );
    }

    #[test]
    fn cors_xml_enforces_size_depth_element_and_declaration_boundaries() {
        let size_prefix = "<CORSConfiguration><!--";
        let size_suffix = "--></CORSConfiguration>";
        let exact_size = format!(
            "{size_prefix}{}{size_suffix}",
            "x".repeat(COS_CORS_XML_MAX_BYTES - size_prefix.len() - size_suffix.len())
        );
        assert_eq!(exact_size.len(), COS_CORS_XML_MAX_BYTES);
        parse_cors_configuration(&exact_size)
            .expect("COS CORS XML at exact size limit should parse");
        let over_size = format!("{exact_size} ");
        let error = parse_cors_configuration(&over_size)
            .expect_err("COS CORS XML over size limit should fail");
        assert!(error.message().contains("byte limit"));

        let exact_depth = format!(
            "<CORSConfiguration>{}{}</CORSConfiguration>",
            "<n>".repeat(15),
            "</n>".repeat(15)
        );
        parse_cors_configuration(&exact_depth)
            .expect("COS CORS XML at exact depth limit should parse");
        let over_depth = format!(
            "<CORSConfiguration>{}{}</CORSConfiguration>",
            "<n>".repeat(16),
            "</n>".repeat(16)
        );
        let error = parse_cors_configuration(&over_depth)
            .expect_err("COS CORS XML over depth limit should fail");
        assert!(error.message().contains("nesting depth"));

        let exact_elements = format!(
            "<CORSConfiguration>{}</CORSConfiguration>",
            "<n/>".repeat(499)
        );
        parse_cors_configuration(&exact_elements)
            .expect("COS CORS XML at exact element limit should parse");
        let over_elements = format!(
            "<CORSConfiguration>{}</CORSConfiguration>",
            "<n/>".repeat(500)
        );
        let error = parse_cors_configuration(&over_elements)
            .expect_err("COS CORS XML over element limit should fail");
        assert!(error.message().contains("element count"));

        for (name, xml, expected) in [
            (
                "DTD",
                "<!DOCTYPE CORSConfiguration [<!ENTITY x 'boom'>]><CORSConfiguration>&x;</CORSConfiguration>".to_string(),
                "DTD and custom entity",
            ),
            (
                "standalone entity",
                "<!ENTITY x 'boom'><CORSConfiguration/>".to_string(),
                "DTD and custom entity",
            ),
            (
                "malformed",
                "<CORSConfiguration><CORSRule></CORSConfiguration>".to_string(),
                "XML",
            ),
            (
                "trailing document",
                "<CORSConfiguration/><extra/>".to_string(),
                "XML",
            ),
        ] {
            let error = parse_cors_configuration(&xml).expect_err(name);
            assert!(
                error.message().contains(expected),
                "{name} boundary returned unexpected error: {}",
                error.message()
            );
        }
    }

    #[test]
    fn cos_error_xml_enforces_exact_size_and_declaration_boundaries() {
        let prefix = "<Error><Code>AccessDenied</Code><!--";
        let suffix = "--></Error>";
        let exact = format!(
            "{prefix}{}{suffix}",
            "x".repeat(COS_ERROR_XML_MAX_BYTES - prefix.len() - suffix.len())
        );
        assert_eq!(exact.len(), COS_ERROR_XML_MAX_BYTES);
        assert_eq!(
            extract_xml_tag(&exact, "Code").as_deref(),
            Some("AccessDenied")
        );

        let over = format!("{exact} ");
        assert_eq!(extract_xml_tag(&over, "Code"), None);
        assert_eq!(
            extract_xml_tag(
                "<!DOCTYPE Error [<!ENTITY x 'expanded'>]><Error><Message>&x;</Message></Error>",
                "Message",
            ),
            None
        );
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
