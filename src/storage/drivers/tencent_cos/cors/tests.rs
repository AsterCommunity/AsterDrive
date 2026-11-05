use reqwest::StatusCode;

use crate::api::api_error_code::ApiErrorCode;

use super::{
    ASTERDRIVE_COS_CORS_RULE_ID, COS_CORS_XML_MAX_BYTES, COS_ERROR_XML_MAX_BYTES,
    CosCorsConfiguration, asterdrive_cors_rule, build_cors_configuration_xml, content_md5_base64,
    cos_cors_response_error, extract_xml_tag, parse_cors_configuration,
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
fn existing_matching_cos_rule_cannot_shadow_required_headers() {
    let mut shadow = asterdrive_cors_rule(&["https://drive.example.com".to_string()]);
    shadow.id = Some("other-app".into());
    shadow.allowed_methods = vec!["PUT".into()];
    shadow.expose_headers.clear();
    assert!(super::validate_rule_order(&[shadow], &["https://drive.example.com".into()]).is_err());
}

#[test]
fn replacing_owned_cos_rule_keeps_its_existing_position() {
    let mut owned = asterdrive_cors_rule(&["https://old.example.com".to_string()]);
    owned.id = Some(ASTERDRIVE_COS_CORS_RULE_ID.to_string());
    let other = asterdrive_cors_rule(&["https://other.example.com".to_string()]);
    let mut rules = vec![owned, other];
    super::replace_asterdrive_rule(&mut rules, &["https://drive.example.com".to_string()]);
    assert_eq!(rules[0].id.as_deref(), Some(ASTERDRIVE_COS_CORS_RULE_ID));
    assert_eq!(rules[0].allowed_origins, vec!["https://drive.example.com"]);
    assert_eq!(rules[1].allowed_origins, vec!["https://other.example.com"]);
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
    <cos:MaxAgeSeconds>0</cos:MaxAgeSeconds>
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
    assert_eq!(parsed.rules[0].max_age_seconds, Some(0));
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
    parse_cors_configuration(&exact_size).expect("COS CORS XML at exact size limit should parse");
    let over_size = format!("{exact_size} ");
    let error =
        parse_cors_configuration(&over_size).expect_err("COS CORS XML over size limit should fail");
    assert!(error.message().contains("byte limit"));

    let exact_depth = format!(
        "<CORSConfiguration>{}{}</CORSConfiguration>",
        "<n>".repeat(15),
        "</n>".repeat(15)
    );
    aster_forge_xml::OwnedDocument::from_reader_with_options(
        exact_depth.as_bytes(),
        &super::cors_xml_options(),
    )
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
        "<n/>".repeat(super::COS_CORS_MAX_ELEMENTS - 1)
    );
    aster_forge_xml::OwnedDocument::from_reader_with_options(
        exact_elements.as_bytes(),
        &super::cors_xml_options(),
    )
    .expect("COS CORS XML at exact element limit should parse");
    let over_elements = format!(
        "<CORSConfiguration>{}</CORSConfiguration>",
        "<n/>".repeat(super::COS_CORS_MAX_ELEMENTS)
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
        None,
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
        None,
    );

    assert_eq!(error.api_error_code(), ApiErrorCode::StorageMisconfigured);
    assert!(!error.message().contains("Missing required header"));
    assert!(error.message().contains("code=InvalidRequest"));
    assert!(error.message().contains("request_id=req-2"));
}

#[test]
fn content_md5_base64_matches_cos_required_header_format() {
    assert_eq!(content_md5_base64(b"hello"), "XUFAKrxLKna5cZ2REBfFkg==");
}

#[test]
fn error_metadata_is_bounded_and_raw_provider_messages_are_not_exposed() {
    for body in [
        "<Error><Code>AccessDenied</Code><Message>SECRETEXAMPLE https://private.test/?q-signature=secret</Message><RequestId>xml-request</RequestId></Error>",
        "<Error><Code>https://private.test/?token=secret</Code><RequestId>secret?token=value</RequestId></Error>",
        "SECRETEXAMPLE https://private.test/?q-signature=secret",
    ] {
        let error = cos_cors_response_error(
            StatusCode::FORBIDDEN,
            body,
            "GET Bucket cors",
            Some("header-request"),
        );
        assert!(error.message().contains("header-request"));
        assert!(error.message().contains("name/cos:GetBucketCORS"));
        assert!(!error.message().contains("SECRETEXAMPLE"));
        assert!(!error.message().contains("private.test"));
        assert!(!error.message().contains("xml-request"));
    }
    for value in [
        String::new(),
        "x".repeat(129),
        "bad\nrequest".into(),
        "https://private.test/?token=x".into(),
    ] {
        assert!(super::safe_cos_error_identifier(&value).is_none());
    }
    assert!(super::safe_cos_error_identifier(&"x".repeat(128)).is_some());
}

#[test]
fn cors_write_limits_apply_after_xml_escaping_at_exact_byte_and_rule_boundaries() {
    let rule = asterdrive_cors_rule(&["https://drive.test".into()]);
    let mut config = CosCorsConfiguration {
        rules: vec![
            super::CosCorsRule {
                id: None,
                allowed_origins: vec!["https://other.test".into()],
                allowed_methods: vec!["GET".into()],
                allowed_headers: vec![],
                expose_headers: vec![],
                max_age_seconds: None
            };
            100
        ],
        response_vary: Some(true),
    };
    let xml = build_cors_configuration_xml(&config).unwrap();
    assert_eq!(parse_cors_configuration(&xml).unwrap(), config);
    config.rules.push(rule);
    assert!(
        build_cors_configuration_xml(&config)
            .unwrap_err()
            .message()
            .contains("100")
    );
    config.rules.truncate(1);
    config.rules[0].id = Some(String::new());
    let initial = build_cors_configuration_xml(&config).unwrap().len();
    config.rules[0].id = Some("x".repeat(COS_CORS_XML_MAX_BYTES - initial));
    assert_eq!(
        build_cors_configuration_xml(&config).unwrap().len(),
        COS_CORS_XML_MAX_BYTES
    );
    config.rules[0].id.as_mut().unwrap().push('x');
    assert!(build_cors_configuration_xml(&config).is_err());
    config.rules[0].id = Some("&".repeat(COS_CORS_XML_MAX_BYTES / 4));
    assert!(build_cors_configuration_xml(&config).is_err());
}

#[test]
fn preserves_no_id_multivalue_rules_and_refuses_lossy_configuration_reads() {
    let body = "<CORSConfiguration><CORSRule><AllowedOrigin>https://one.test</AllowedOrigin><AllowedOrigin>https://two.test</AllowedOrigin><AllowedMethod>GET</AllowedMethod><AllowedMethod>HEAD</AllowedMethod><AllowedHeader>Range</AllowedHeader><ExposeHeader>ETag</ExposeHeader><ExposeHeader>Content-Range</ExposeHeader><MaxAgeSeconds>300</MaxAgeSeconds></CORSRule><ResponseVary>false</ResponseVary></CORSConfiguration>";
    let parsed = parse_cors_configuration(body).unwrap();
    assert!(parsed.rules[0].id.is_none());
    assert_eq!(parsed.rules[0].allowed_origins.len(), 2);
    assert_eq!(
        parse_cors_configuration(&build_cors_configuration_xml(&parsed).unwrap()).unwrap(),
        parsed
    );
    for bad in [
        body.replace(
            "</CORSConfiguration>",
            "<FutureFlag>true</FutureFlag></CORSConfiguration>",
        ),
        body.replace("</CORSRule>", "<FutureFlag>true</FutureFlag></CORSRule>"),
        body.replace(
            "<MaxAgeSeconds>300</MaxAgeSeconds>",
            "<MaxAgeSeconds>bad</MaxAgeSeconds>",
        ),
        body.replace(
            "<MaxAgeSeconds>300</MaxAgeSeconds>",
            "<MaxAgeSeconds>300</MaxAgeSeconds><MaxAgeSeconds>600</MaxAgeSeconds>",
        ),
        body.replace(
            "<ResponseVary>false</ResponseVary>",
            "<ResponseVary>bad</ResponseVary>",
        ),
        body.replace(
            "<ResponseVary>false</ResponseVary>",
            "<ResponseVary>false</ResponseVary><ResponseVary>true</ResponseVary>",
        ),
        "<CORSConfiguration><CORSRule/></CORSConfiguration>".into(),
        body.replace(
            "<AllowedHeader>Range</AllowedHeader>",
            "<AllowedHeader><nested>Range</nested></AllowedHeader>",
        ),
    ] {
        assert!(
            parse_cors_configuration(&bad).is_err(),
            "accepted lossy input {bad}"
        );
    }
}
