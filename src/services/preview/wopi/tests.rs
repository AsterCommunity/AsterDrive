//! WOPI 服务测试。

use std::collections::BTreeMap;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::json;

use super::discovery::{
    append_wopi_src, build_discovered_apps, ensure_request_source_allowed, expand_action_url,
    parse_discovery_xml, resolve_discovery_action_url, trusted_origins_for_app,
};
use super::operations::parse_wopi_max_expected_size;
use super::targets::{
    PutRelativeTargetMode, decode_wopi_filename, encode_wopi_filename,
    normalize_relative_target_name, normalize_requested_rename_target, parse_put_relative_request,
};
use super::types::{
    DiscoveredWopiApp, WOPI_FILE_NAME_MAX_LEN, WopiCheckFileInfo, WopiRequestSource,
};
use crate::services::preview::apps::{
    PreviewAppProvider, PreviewOpenMode, PublicPreviewAppConfig, PublicPreviewAppDefinition,
};
use aster_forge_xml::DEFAULT_XML_MAX_TEXT_BYTES;

fn test_wopi_app() -> PublicPreviewAppDefinition {
    PublicPreviewAppDefinition {
        key: "onlyoffice".to_string(),
        provider: PreviewAppProvider::Wopi,
        icon: "/icon.svg".to_string(),
        enabled: true,
        labels: BTreeMap::new(),
        extensions: vec!["docx".to_string()],
        config: PublicPreviewAppConfig {
            mode: Some(PreviewOpenMode::Iframe),
            action_url: Some(
                "http://localhost:8080/hosting/wopi/word/edit?WOPISrc={{wopi_src}}".to_string(),
            ),
            discovery_url: Some("http://localhost:8080/hosting/discovery".to_string()),
            allowed_origins: vec!["http://127.0.0.1:8080".to_string()],
            ..Default::default()
        },
    }
}

#[test]
fn append_wopi_src_adds_query_parameter() {
    let url = append_wopi_src(
        "https://office.example.com/hosting/wopi/word/edit?lang=zh-CN",
        "https://drive.example.com/api/v1/wopi/files/7",
    )
    .unwrap();
    assert!(url.contains("lang=zh-CN"));
    assert!(url.contains("WOPISrc=https%3A%2F%2Fdrive.example.com%2Fapi%2Fv1%2Fwopi%2Ffiles%2F7"));
}

#[test]
fn expand_action_url_resolves_discovery_placeholders() {
    let url = expand_action_url(
        "https://office.example.com/hosting/wopi/word/view?mobile=1&<ui=UI_LLCC&><rs=DC_LLCC&><wopisrc=WOPI_SOURCE&>",
        "https://drive.example.com/api/v1/wopi/files/7",
    )
    .unwrap();

    assert!(url.contains("mobile=1"));
    assert!(url.contains("wopisrc=https%3A%2F%2Fdrive.example.com%2Fapi%2Fv1%2Fwopi%2Ffiles%2F7"));
    assert!(!url.contains("<ui="));
    assert!(!url.contains("<wopisrc="));
}

#[test]
fn parse_discovery_xml_extracts_named_actions() {
    let discovery = parse_discovery_xml(
        r#"
            <wopi-discovery>
              <net-zone name="external-http">
                <app name="application/vnd.openxmlformats-officedocument.wordprocessingml.document">
                  <action name="edit" ext="docx" urlsrc="https://office.example.com/word/edit?" />
                  <action name="view" ext="docx" urlsrc="https://office.example.com/word/view?" />
                </app>
              </net-zone>
            </wopi-discovery>
            "#,
    )
    .unwrap();

    assert_eq!(
        discovery
            .find_action_url(
                "edit",
                Some("docx"),
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            )
            .as_deref(),
        Some("https://office.example.com/word/edit?")
    );
}

#[test]
fn build_discovered_apps_groups_actions_by_app_name() {
    let discovery = parse_discovery_xml(
        r#"
            <wopi-discovery>
              <net-zone name="external-http">
                <app name="Word" favIconUrl="https://office.example.com/word.ico">
                  <action name="view" ext="doc" urlsrc="https://office.example.com/word/view?" />
                  <action name="view" ext="docx" urlsrc="https://office.example.com/word/view?" />
                  <action name="edit" ext="docx" urlsrc="https://office.example.com/word/edit?" />
                </app>
                <app name="Excel" favIconUrl="https://office.example.com/excel.ico">
                  <action name="view" ext="xls" urlsrc="https://office.example.com/excel/view?" />
                  <action name="view" ext="xlsx" urlsrc="https://office.example.com/excel/view?" />
                </app>
                <app name="Pdf" favIconUrl="https://office.example.com/pdf.ico">
                  <action name="view" ext="pdf" urlsrc="https://office.example.com/pdf/view?" />
                </app>
              </net-zone>
            </wopi-discovery>
            "#,
    )
    .unwrap();

    let apps = build_discovered_apps(&discovery);

    assert_eq!(apps.len(), 3);
    assert_eq!(
        apps[0],
        DiscoveredWopiApp {
            action: "edit".to_string(),
            extensions: vec!["doc".to_string(), "docx".to_string()],
            icon_url: Some("https://office.example.com/word.ico".to_string()),
            key_suffix: "word".to_string(),
            label: "Word".to_string(),
        }
    );
    assert_eq!(
        apps[1],
        DiscoveredWopiApp {
            action: "view".to_string(),
            extensions: vec!["xls".to_string(), "xlsx".to_string()],
            icon_url: Some("https://office.example.com/excel.ico".to_string()),
            key_suffix: "excel".to_string(),
            label: "Excel".to_string(),
        }
    );
    assert_eq!(
        apps[2],
        DiscoveredWopiApp {
            action: "view".to_string(),
            extensions: vec!["pdf".to_string()],
            icon_url: Some("https://office.example.com/pdf.ico".to_string()),
            key_suffix: "pdf".to_string(),
            label: "Pdf".to_string(),
        }
    );
}

#[test]
fn resolve_discovery_action_url_prefers_editable_actions_for_legacy_view_configs() {
    let discovery = parse_discovery_xml(
        r#"
            <wopi-discovery>
              <net-zone name="external-http">
                <app name="Word">
                  <action name="view" ext="doc" urlsrc="https://office.example.com/word/view?" />
                  <action name="view" ext="docx" urlsrc="https://office.example.com/word/view?" />
                  <action name="edit" ext="docx" urlsrc="https://office.example.com/word/edit?" />
                </app>
              </net-zone>
            </wopi-discovery>
            "#,
    )
    .unwrap();

    assert_eq!(
        resolve_discovery_action_url(
            &discovery,
            "view",
            Some("docx"),
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        )
        .as_deref(),
        Some("https://office.example.com/word/edit?")
    );
    assert_eq!(
        resolve_discovery_action_url(&discovery, "edit", Some("doc"), "application/msword")
            .as_deref(),
        Some("https://office.example.com/word/view?")
    );
}

#[test]
fn resolve_discovery_action_url_prefers_plain_edit_for_default_edit_configs() {
    let discovery = parse_discovery_xml(
        r#"
            <wopi-discovery>
              <net-zone name="external-http">
                <app name="Word">
                  <action name="embededit" ext="docx" urlsrc="https://office.example.com/word/embededit?" />
                  <action name="edit" ext="docx" urlsrc="https://office.example.com/word/edit?" />
                  <action name="view" ext="docx" urlsrc="https://office.example.com/word/view?" />
                </app>
              </net-zone>
            </wopi-discovery>
            "#,
    )
    .unwrap();

    assert_eq!(
        resolve_discovery_action_url(
            &discovery,
            "edit",
            Some("docx"),
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        )
        .as_deref(),
        Some("https://office.example.com/word/edit?")
    );
}

#[test]
fn resolve_discovery_action_url_respects_explicit_embededit_configs() {
    let discovery = parse_discovery_xml(
        r#"
            <wopi-discovery>
              <net-zone name="external-http">
                <app name="Word">
                  <action name="edit" ext="docx" urlsrc="https://office.example.com/word/edit?" />
                  <action name="embededit" ext="docx" urlsrc="https://office.example.com/word/embededit?" />
                  <action name="view" ext="docx" urlsrc="https://office.example.com/word/view?" />
                </app>
              </net-zone>
            </wopi-discovery>
            "#,
    )
    .unwrap();

    assert_eq!(
        resolve_discovery_action_url(
            &discovery,
            "embededit",
            Some("docx"),
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        )
        .as_deref(),
        Some("https://office.example.com/word/embededit?")
    );
}

#[test]
fn resolve_discovery_action_url_respects_explicit_mobileedit_configs() {
    let discovery = parse_discovery_xml(
        r#"
            <wopi-discovery>
              <net-zone name="external-http">
                <app name="Word">
                  <action name="edit" ext="docx" urlsrc="https://office.example.com/word/edit?" />
                  <action name="mobileedit" ext="docx" urlsrc="https://office.example.com/word/mobileedit?" />
                  <action name="view" ext="docx" urlsrc="https://office.example.com/word/view?" />
                </app>
              </net-zone>
            </wopi-discovery>
            "#,
    )
    .unwrap();

    assert_eq!(
        resolve_discovery_action_url(
            &discovery,
            "mobileedit",
            Some("docx"),
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        )
        .as_deref(),
        Some("https://office.example.com/word/mobileedit?")
    );
}

#[test]
fn resolve_discovery_action_url_tries_custom_action_before_edit_fallbacks() {
    let discovery = parse_discovery_xml(
        r#"
            <wopi-discovery>
              <net-zone name="external-http">
                <app name="Word">
                  <action name="edit" ext="docx" urlsrc="https://office.example.com/word/edit?" />
                  <action name="previewedit" ext="docx" urlsrc="https://office.example.com/word/previewedit?" />
                </app>
              </net-zone>
            </wopi-discovery>
            "#,
    )
    .unwrap();

    assert_eq!(
        resolve_discovery_action_url(
            &discovery,
            "previewedit",
            Some("docx"),
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        )
        .as_deref(),
        Some("https://office.example.com/word/previewedit?")
    );
}

#[test]
fn resolve_discovery_action_url_falls_back_when_plain_edit_is_missing() {
    let discovery = parse_discovery_xml(
        r#"
            <wopi-discovery>
              <net-zone name="external-http">
                <app name="Word">
                  <action name="embededit" ext="docx" urlsrc="https://office.example.com/word/embededit?" />
                  <action name="view" ext="docx" urlsrc="https://office.example.com/word/view?" />
                </app>
              </net-zone>
            </wopi-discovery>
            "#,
    )
    .unwrap();

    assert_eq!(
        resolve_discovery_action_url(
            &discovery,
            "edit",
            Some("docx"),
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        )
        .as_deref(),
        Some("https://office.example.com/word/embededit?")
    );
}

#[test]
fn resolve_discovery_action_url_falls_back_to_view_when_no_edit_action_matches() {
    let discovery = parse_discovery_xml(
        r#"
            <wopi-discovery>
              <net-zone name="external-http">
                <app name="Word">
                  <action name="view" ext="docx" urlsrc="https://office.example.com/word/view?" />
                </app>
              </net-zone>
            </wopi-discovery>
            "#,
    )
    .unwrap();

    assert_eq!(
        resolve_discovery_action_url(
            &discovery,
            "edit",
            Some("docx"),
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        )
        .as_deref(),
        Some("https://office.example.com/word/view?")
    );
}

#[test]
fn build_discovered_apps_prefers_plain_edit_over_embededit() {
    let discovery = parse_discovery_xml(
        r#"
            <wopi-discovery>
              <net-zone name="external-http">
                <app name="Word">
                  <action name="embededit" ext="docx" urlsrc="https://office.example.com/word/embededit?" />
                  <action name="edit" ext="docx" urlsrc="https://office.example.com/word/edit?" />
                </app>
              </net-zone>
            </wopi-discovery>
            "#,
    )
    .unwrap();

    let apps = build_discovered_apps(&discovery);

    assert_eq!(apps.len(), 1);
    assert_eq!(apps[0].action, "edit");
    assert_eq!(apps[0].extensions, vec!["docx".to_string()]);
}

#[test]
fn build_discovered_apps_uses_embededit_when_plain_edit_is_missing() {
    let discovery = parse_discovery_xml(
        r#"
            <wopi-discovery>
              <net-zone name="external-http">
                <app name="Word">
                  <action name="embededit" ext="docx" urlsrc="https://office.example.com/word/embededit?" />
                  <action name="view" ext="doc" urlsrc="https://office.example.com/word/view?" />
                </app>
              </net-zone>
            </wopi-discovery>
            "#,
    )
    .unwrap();

    let apps = build_discovered_apps(&discovery);

    assert_eq!(apps.len(), 1);
    assert_eq!(apps[0].action, "embededit");
    assert_eq!(
        apps[0].extensions,
        vec!["docx".to_string(), "doc".to_string()]
    );
}

#[test]
fn build_discovered_apps_uses_view_when_no_editable_actions_exist() {
    let discovery = parse_discovery_xml(
        r#"
            <wopi-discovery>
              <net-zone name="external-http">
                <app name="Word">
                  <action name="view" ext="doc" urlsrc="https://office.example.com/word/view?" />
                  <action name="mobileview" ext="docx" urlsrc="https://office.example.com/word/mobileview?" />
                </app>
              </net-zone>
            </wopi-discovery>
            "#,
    )
    .unwrap();

    let apps = build_discovered_apps(&discovery);

    assert_eq!(apps.len(), 1);
    assert_eq!(apps[0].action, "view");
    assert_eq!(
        apps[0].extensions,
        vec!["doc".to_string(), "docx".to_string()]
    );
}

#[test]
fn trusted_origins_merge_explicit_and_derived_origins() {
    let origins = trusted_origins_for_app(&test_wopi_app());
    assert!(
        origins
            .iter()
            .any(|origin| origin == "http://localhost:8080")
    );
    assert!(
        origins
            .iter()
            .any(|origin| origin == "http://127.0.0.1:8080")
    );
}

#[test]
fn request_source_check_accepts_matching_origin_or_missing_headers() {
    let app = test_wopi_app();

    ensure_request_source_allowed(
        &app,
        &WopiRequestSource {
            origin: Some("http://localhost:8080"),
            referer: None,
            ..Default::default()
        },
    )
    .unwrap();

    ensure_request_source_allowed(
        &app,
        &WopiRequestSource {
            origin: None,
            referer: Some("http://localhost:8080/hosting/wopi/word/edit"),
            ..Default::default()
        },
    )
    .unwrap();

    ensure_request_source_allowed(
        &app,
        &WopiRequestSource {
            origin: None,
            referer: None,
            ..Default::default()
        },
    )
    .unwrap();
}

#[test]
fn request_source_check_rejects_untrusted_origin() {
    let err = ensure_request_source_allowed(
        &test_wopi_app(),
        &WopiRequestSource {
            origin: Some("https://evil.example.com"),
            referer: None,
            ..Default::default()
        },
    )
    .unwrap_err();

    assert!(err.message().contains("untrusted WOPI request origin"));
}

#[test]
fn check_file_info_serializes_user_can_not_write_relative() {
    let info = WopiCheckFileInfo {
        base_file_name: "doc.docx".to_string(),
        file_name_max_length: Some(WOPI_FILE_NAME_MAX_LEN),
        owner_id: "1".to_string(),
        size: 123,
        user_id: "2".to_string(),
        user_can_not_write_relative: false,
        user_can_rename: true,
        user_info: Some("pane-state".to_string()),
        user_can_write: true,
        read_only: false,
        supports_get_lock: true,
        supports_locks: true,
        supports_extended_lock_length: Some(true),
        supports_rename: true,
        supports_user_info: Some(true),
        supports_update: true,
        version: "hash".to_string(),
    };

    let payload = serde_json::to_value(info).unwrap();
    assert_eq!(payload["UserCanNotWriteRelative"], json!(false));
    assert_eq!(payload["SupportsExtendedLockLength"], json!(true));
}

#[test]
fn utf7_roundtrip_handles_non_ascii_targets() {
    let encoded = encode_wopi_filename("副本 文档.docx");
    let decoded = decode_wopi_filename(&encoded).unwrap();
    assert_eq!(decoded, "副本 文档.docx");
}

#[test]
fn parse_put_relative_request_allows_extension_only_suggested_target() {
    let request = parse_put_relative_request("report 1.docx", Some(".docx"), None, None).unwrap();

    match request.target_mode {
        PutRelativeTargetMode::Suggested(name) => assert_eq!(name, "report 1.docx"),
        PutRelativeTargetMode::Relative { .. } => panic!("expected suggested target"),
    }
}

#[test]
fn normalize_relative_target_name_normalizes_nfd_and_rejects_windows_reserved_name() {
    assert_eq!(
        normalize_relative_target_name("cafe\u{0301}.docx").unwrap(),
        "caf\u{00e9}.docx"
    );
    assert!(normalize_relative_target_name("CON.docx").is_err());
}

#[test]
fn normalize_requested_rename_target_normalizes_nfd_and_rejects_windows_reserved_name() {
    assert_eq!(
        normalize_requested_rename_target("report 1.docx", Some("cafe\u{0301}")).unwrap(),
        "caf\u{00e9}.docx"
    );
    assert!(normalize_requested_rename_target("report 1.docx", Some("NUL")).is_err());
}

#[test]
fn parse_discovery_xml_extracts_proof_keys() {
    let mut modulus_bytes = vec![0_u8; 256];
    modulus_bytes[0] = 0x80;
    modulus_bytes[255] = 1;
    let modulus = STANDARD.encode(modulus_bytes);
    let exponent = STANDARD.encode([1_u8, 0, 1]);
    let discovery = parse_discovery_xml(&format!(
        r#"
            <wopi-discovery>
              <proof-key modulus="{modulus}" exponent="{exponent}" />
              <net-zone name="external-http">
                <app name="Word">
                  <action name="view" ext="docx" urlsrc="https://office.example.com/word/view?" />
                </app>
              </net-zone>
            </wopi-discovery>
            "#
    ))
    .unwrap();

    assert!(discovery.proof_keys().is_some());
}

#[test]
fn parse_discovery_xml_rejects_xml_safety_boundaries() {
    assert!(parse_discovery_xml("<!DOCTYPE root><root/>").is_err());
    assert!(parse_discovery_xml("junk<wopi-discovery><action name=\"view\" urlsrc=\"https://office.example/view?\"/></wopi-discovery>").is_err());

    let mut deep = String::new();
    for _ in 0..=aster_forge_xml::DEFAULT_XML_MAX_DEPTH {
        deep.push_str("<x>");
    }
    deep.push_str(r#"<action name="view" urlsrc="https://office.example/view?"/>"#);
    for _ in 0..=aster_forge_xml::DEFAULT_XML_MAX_DEPTH {
        deep.push_str("</x>");
    }
    assert!(parse_discovery_xml(&deep).is_err());

    let comment = "x".repeat(DEFAULT_XML_MAX_TEXT_BYTES + 1);
    let oversized = format!(
        "<!--{comment}--><wopi-discovery><action name=\"view\" urlsrc=\"https://office.example/view?\"/></wopi-discovery>"
    );
    assert!(parse_discovery_xml(&oversized).is_err());
}

#[test]
fn parse_discovery_xml_accepts_exact_attribute_limit_and_rejects_one_more() {
    let mut attributes =
        String::from(r#"name="view" urlsrc="https://office.example/view?" ext="docx" "#);
    for index in 0..61 {
        attributes.push_str(&format!("x{index}=\"value\" "));
    }
    let exact = format!("<wopi-discovery><action {attributes}/></wopi-discovery>");
    let discovery = parse_discovery_xml(&exact).expect("exact attribute budget should pass");
    assert_eq!(
        discovery
            .find_action_url("view", Some("docx"), "application/octet-stream")
            .as_deref(),
        Some("https://office.example/view?")
    );

    let over = exact.replace("x60=\"value\"", "x60=\"value\" x61=\"value\"");
    assert!(parse_discovery_xml(&over).is_err());
}

#[test]
fn parse_discovery_xml_rejects_duplicate_or_incomplete_proof_keys() {
    let mut modulus_bytes = vec![0_u8; 256];
    modulus_bytes[0] = 0x80;
    modulus_bytes[255] = 1;
    let modulus = STANDARD.encode(modulus_bytes);
    let exponent = STANDARD.encode([1_u8, 0, 1]);
    let duplicate = format!(
        r#"<wopi-discovery>
          <proof-key modulus="{modulus}" exponent="{exponent}"/>
          <proof-key modulus="{modulus}" exponent="{exponent}"/>
          <action name="view" urlsrc="https://office.example/view?"/>
        </wopi-discovery>"#
    );
    assert!(parse_discovery_xml(&duplicate).is_err());
    assert!(parse_discovery_xml(
        r#"<wopi-discovery><proof-key modulus="AQID"/><action name="view" urlsrc="https://office.example/view?"/></wopi-discovery>"#
    )
    .is_err());
}

#[test]
fn parse_wopi_max_expected_size_accepts_u32_range() {
    assert_eq!(parse_wopi_max_expected_size(None).unwrap(), None);
    assert_eq!(parse_wopi_max_expected_size(Some("0")).unwrap(), Some(0));
    assert_eq!(
        parse_wopi_max_expected_size(Some("4294967295")).unwrap(),
        Some(4_294_967_295)
    );
}

#[test]
fn parse_wopi_max_expected_size_rejects_invalid_values() {
    assert!(parse_wopi_max_expected_size(Some("-1")).is_err());
    assert!(parse_wopi_max_expected_size(Some("4294967296")).is_err());
    assert!(parse_wopi_max_expected_size(Some("abc")).is_err());
}
