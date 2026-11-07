use super::*;

#[test]
fn cos_descriptor_advertises_server_upload_without_browser_write_capabilities() {
    let descriptor = TencentCosConnector::descriptor_definition();
    assert!(descriptor.upload_workflows.stream_upload);
    assert!(descriptor.upload_workflows.object_multipart_upload);
    assert!(!descriptor.upload_workflows.presigned_upload);
    assert!(
        !descriptor
            .upload_workflows
            .object_multipart_upload_capabilities
            .unwrap()
            .presigned_part_upload
    );
}

#[test]
fn saved_cos_browser_upload_mode_moves_to_relay_without_accepting_invalid_modes() {
    let values = serde_json::json!({
        "endpoint": "https://cos.ap-guangzhou.myqcloud.com",
        "bucket": "bucket-1250000000",
        "base_path": "",
        "object_storage_download_strategy": "relay_stream",
        "object_storage_upload_strategy": "presigned"
    });
    let input = crate::storage::connectors::test_support::connection_config(
        TencentCosConnector::ID,
        1,
        values.clone(),
    );
    let normalized = TencentCosConnector
        .validate_connector_config(&input)
        .unwrap();
    assert_eq!(
        normalized.values["object_storage_upload_strategy"],
        "relay_stream"
    );
    let policy = crate::storage::connectors::test_support::policy(
        TencentCosConnector::ID,
        1,
        values,
        aster_drive_storage::StoragePolicyBehaviorConfig::default(),
    );
    assert_eq!(
        TencentCosConnector.upload_transport(&policy).unwrap(),
        StorageConnectorUploadTransport::ObjectStorage(ObjectStorageUploadStrategy::RelayStream)
    );
    let mut invalid = input;
    invalid.values.insert(
        "object_storage_upload_strategy".into(),
        serde_json::json!("unknown"),
    );
    assert!(
        TencentCosConnector
            .validate_connector_config(&invalid)
            .is_err()
    );
}

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
