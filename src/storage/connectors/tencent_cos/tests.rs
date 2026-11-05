use super::*;

fn runtime(value: &str) -> crate::config::RuntimeConfig {
    let runtime = crate::config::RuntimeConfig::new();
    runtime.apply(aster_forge_db::system_config::Model {
        id: 1,
        key: site_url::PUBLIC_SITE_URL_KEY.into(),
        value: value.into(),
        value_type: aster_forge_config::ConfigValueType::String,
        requires_restart: false,
        is_sensitive: false,
        source: aster_forge_config::ConfigSource::System,
        visibility: aster_forge_config::ConfigVisibility::Private,
        namespace: String::new(),
        category: String::new(),
        description: String::new(),
        updated_at: chrono::Utc::now(),
        updated_by: None,
    });
    runtime
}

#[test]
fn cos_cors_uses_browser_origins_and_rejects_entire_invalid_configuration() {
    assert_eq!(resolve_cos_cors_allowed_origins(&runtime(
        r#"["https://DRIVE.test:443","https://drive.test","http://drive.test:80","https://drive.test:8443","https://[::1]:443"]"#,
    )).unwrap(), vec!["https://drive.test", "http://drive.test", "https://drive.test:8443", "https://[::1]"]);
    assert!(resolve_cos_cors_allowed_origins(&crate::config::RuntimeConfig::new()).is_err());
    for value in [
        "",
        "[]",
        "not-json",
        r#"["*"]"#,
        r#"["https://*.test"]"#,
        r#"["https://good.test","https://bad.test/path"]"#,
        r#"["ftp://bad.test"]"#,
        r#"["https://u:p@bad.test"]"#,
        r#"["https://bad.test?query=1"]"#,
    ] {
        assert!(
            resolve_cos_cors_allowed_origins(&runtime(value)).is_err(),
            "accepted {value}"
        );
    }
}
