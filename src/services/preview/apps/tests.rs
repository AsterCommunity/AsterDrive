//! 预览应用服务测试。

use serde_json::{Value, json};

use super::{
    PREVIEW_APPS_CONFIG_KEY, PreviewAppProvider, PreviewOpenMode, PublicPreviewAppConfig,
    PublicPreviewAppDefinition, default_public_preview_apps,
    normalize::parse_public_preview_apps_config, normalize_public_preview_apps_config_value,
};

fn minimum_builtin_apps_json() -> Vec<Value> {
    vec![
        json!({
            "key": "builtin.image",
            "provider": "builtin",
            "icon": "Eye",
            "labels": { "en": "Image preview" }
        }),
        json!({
            "key": "builtin.video",
            "provider": "builtin",
            "icon": "Monitor",
            "labels": { "en": "Video preview" }
        }),
        json!({
            "key": "builtin.audio",
            "provider": "builtin",
            "icon": "FileAudio",
            "labels": { "en": "Audio preview" }
        }),
        json!({
            "key": "builtin.pdf",
            "provider": "builtin",
            "icon": "FileText",
            "labels": { "en": "PDF preview" },
            "extensions": ["pdf"]
        }),
        json!({
            "key": "builtin.markdown",
            "provider": "builtin",
            "icon": "Eye",
            "labels": { "en": "Markdown preview" },
            "extensions": ["md", "markdown"]
        }),
        json!({
            "key": "builtin.table",
            "provider": "builtin",
            "icon": "Table",
            "labels": { "en": "Table preview" },
            "extensions": ["csv", "tsv"],
            "config": { "delimiter": "auto" }
        }),
        json!({
            "key": "builtin.formatted",
            "provider": "builtin",
            "icon": "BracketsCurly",
            "labels": { "en": "Formatted view" },
            "extensions": ["json", "xml"]
        }),
        json!({
            "key": "builtin.code",
            "provider": "builtin",
            "icon": "FileCode",
            "labels": { "en": "Source view" }
        }),
        json!({
            "key": "builtin.try_text",
            "provider": "builtin",
            "icon": "FileCode",
            "labels": { "en": "Open as text" }
        }),
    ]
}

#[test]
fn default_preview_apps_serialize_and_parse() {
    let raw = serde_json::to_string(&default_public_preview_apps()).unwrap();
    let parsed = parse_public_preview_apps_config(&raw).unwrap();
    assert_eq!(parsed.version, 2);
    assert!(parsed.apps.iter().any(|app| {
        app.key == "builtin.formatted"
            && app.extensions.iter().any(|extension| extension == "json")
            && app.extensions.iter().any(|extension| extension == "xml")
    }));
    assert!(parsed.apps.iter().any(|app| {
        app.key == "builtin.code"
            && app
                .labels
                .get("en")
                .is_some_and(|label| label == "Source view")
            && app
                .labels
                .get("zh")
                .is_some_and(|label| label == "源码视图")
    }));
    assert!(parsed.apps.iter().any(|app| {
        app.key == "builtin.office_microsoft"
            && app.extensions.iter().any(|extension| extension == "docx")
    }));
    assert!(parsed.apps.iter().any(|app| {
        app.key == "builtin.archive"
            && app.extensions.iter().any(|extension| extension == "zip")
            && app
                .labels
                .get("zh")
                .is_some_and(|label| label == "压缩包预览")
    }));
    assert!(
        !parsed
            .apps
            .iter()
            .any(|app| app.key == "builtin.tencent_cos_ci")
    );
}

#[test]
fn preview_apps_json_is_normalized_and_pretty_printed() {
    let mut config = default_public_preview_apps();
    config.apps.push(PublicPreviewAppDefinition {
        key: " custom.viewer ".to_string(),
        provider: PreviewAppProvider::UrlTemplate,
        icon: "Globe".to_string(),
        enabled: true,
        labels: std::collections::BTreeMap::from([
            (" EN ".to_string(), " Viewer ".to_string()),
            ("zh".to_string(), " 查看器 ".to_string()),
        ]),
        extensions: vec![" MP4 ".to_string()],
        config: PublicPreviewAppConfig {
            mode: Some(PreviewOpenMode::Iframe),
            url_template: Some(
                " https://viewer.example.com/?url={{file_preview_url}} ".to_string(),
            ),
            allowed_origins: vec![
                " https://viewer.example.com ".to_string(),
                "https://viewer.example.com".to_string(),
            ],
            ..Default::default()
        },
    });

    let raw = serde_json::to_string(&config).unwrap();

    let normalized = normalize_public_preview_apps_config_value(&raw).unwrap();
    let normalized_json: Value = serde_json::from_str(&normalized).unwrap();

    assert!(
        normalized_json["apps"]
            .as_array()
            .is_some_and(|apps| apps.iter().any(|app| {
                app["key"] == "custom.viewer"
                    && app["labels"]["en"] == "Viewer"
                    && app["labels"]["zh"] == "查看器"
                    && app["config"]["mode"] == "iframe"
                    && app["extensions"] == json!(["mp4"])
            }))
    );
}

#[test]
fn preview_apps_reject_native_preview_provider() {
    let raw = json!({
        "version": 2,
        "apps": [
            {
                "key": "custom.native",
                "provider": "native_preview",
                "icon": "/static/preview-apps/file.svg",
                "labels": { "en": "Native" },
                "extensions": ["docx"]
            }
        ]
    })
    .to_string();

    let error = normalize_public_preview_apps_config_value(&raw).unwrap_err();

    assert!(
        error
            .message()
            .contains("unsupported preview app provider 'native_preview'"),
        "unexpected error: {}",
        error.message()
    );
}

#[test]
fn preview_apps_reject_legacy_rules_field() {
    let raw = json!({
        "version": 2,
        "apps": minimum_builtin_apps_json(),
        "rules": []
    })
    .to_string();

    let error = normalize_public_preview_apps_config_value(&raw).unwrap_err();
    assert!(error.to_string().contains("unknown field `rules`"));
}

#[test]
fn preview_apps_reject_legacy_label_i18n_key_field() {
    let raw = json!({
        "version": 2,
        "apps": [
            {
                "key": "builtin.image",
                "provider": "builtin",
                "icon": "Eye",
                "label_i18n_key": "open_with_image"
            },
            {
                "key": "builtin.video",
                "provider": "builtin",
                "icon": "Monitor",
                "labels": { "en": "Video preview" }
            },
            {
                "key": "builtin.audio",
                "provider": "builtin",
                "icon": "FileAudio",
                "labels": { "en": "Audio preview" }
            },
            {
                "key": "builtin.pdf",
                "provider": "builtin",
                "icon": "FileText",
                "labels": { "en": "PDF preview" },
                "extensions": ["pdf"]
            },
            {
                "key": "builtin.markdown",
                "provider": "builtin",
                "icon": "Eye",
                "labels": { "en": "Markdown preview" },
                "extensions": ["md", "markdown"]
            },
            {
                "key": "builtin.table",
                "provider": "builtin",
                "icon": "Table",
                "labels": { "en": "Table preview" },
                "extensions": ["csv", "tsv"],
                "config": { "delimiter": "auto" }
            },
            {
                "key": "builtin.formatted",
                "provider": "builtin",
                "icon": "BracketsCurly",
                "labels": { "en": "Formatted view" },
                "extensions": ["json", "xml"]
            },
            {
                "key": "builtin.code",
                "provider": "builtin",
                "icon": "FileCode",
                "labels": { "en": "Source view" }
            },
            {
                "key": "builtin.try_text",
                "provider": "builtin",
                "icon": "FileCode",
                "labels": { "en": "Open as text" }
            }
        ]
    })
    .to_string();

    let error = normalize_public_preview_apps_config_value(&raw).unwrap_err();
    assert!(error.to_string().contains("unknown field `label_i18n_key`"));
}

#[test]
fn preview_apps_constant_key_matches_expected_name() {
    assert_eq!(PREVIEW_APPS_CONFIG_KEY, "frontend_preview_apps_json");
}

#[test]
fn preview_apps_require_explicit_provider_fields() {
    let mut raw = serde_json::to_value(default_public_preview_apps()).unwrap();
    raw["apps"]
        .as_array_mut()
        .and_then(|apps| apps.first_mut())
        .and_then(Value::as_object_mut)
        .expect("default preview app should be an object")
        .remove("provider");

    let error = normalize_public_preview_apps_config_value(&raw.to_string()).unwrap_err();
    assert!(error.to_string().contains("missing field `provider`"));
}

#[test]
fn preview_apps_restore_missing_core_builtins_without_external_viewers() {
    let raw = json!({
        "version": 2,
        "apps": minimum_builtin_apps_json()
    })
    .to_string();

    let normalized = normalize_public_preview_apps_config_value(&raw).unwrap();
    let normalized_json: Value = serde_json::from_str(&normalized).unwrap();

    assert!(normalized_json["apps"].as_array().is_some_and(|apps| {
        apps.iter().any(|app| app["key"] == "builtin.archive")
            && !apps
                .iter()
                .any(|app| app["key"] == "builtin.office_microsoft")
    }));
}

#[test]
fn preview_apps_restore_all_core_builtins_when_config_is_empty() {
    let raw = json!({
        "version": 2,
        "apps": []
    })
    .to_string();

    let normalized = normalize_public_preview_apps_config_value(&raw).unwrap();
    let normalized_json: Value = serde_json::from_str(&normalized).unwrap();

    assert!(normalized_json["apps"].as_array().is_some_and(|apps| {
        apps.iter().any(|app| app["key"] == "builtin.image")
            && apps.iter().any(|app| app["key"] == "builtin.code")
            && apps.iter().any(|app| app["key"] == "builtin.archive")
    }));
}

#[test]
fn preview_apps_allow_empty_icon_and_trim_it() {
    let mut apps = minimum_builtin_apps_json();
    apps.push(json!({
        "key": "custom.viewer",
        "provider": "url_template",
        "icon": "   ",
        "labels": { "en": "Viewer" },
        "extensions": ["txt"],
        "config": {
            "mode": "iframe",
            "url_template": "https://viewer.example.com/?src={{file_preview_url}}"
        }
    }));

    let raw = json!({
        "version": 2,
        "apps": apps
    })
    .to_string();

    let normalized = normalize_public_preview_apps_config_value(&raw).unwrap();
    let normalized_json: Value = serde_json::from_str(&normalized).unwrap();

    assert!(normalized_json["apps"].as_array().is_some_and(|apps| {
        apps.iter()
            .any(|app| app["key"] == "custom.viewer" && app["icon"] == "")
    }));
}
