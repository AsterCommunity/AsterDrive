use super::*;
use crate::storage::connectors::test_support::connection_config;
use serde_json::{Value, json};

fn values() -> Value {
    json!({"endpoint":"https://account.blob.core.windows.net", "bucket":"archive", "base_path":"tenant", "object_storage_upload_strategy":"relay_stream", "object_storage_download_strategy":"presigned"})
}

#[test]
fn legacy_defaults_and_front_door_round_trip() {
    let connector = AzureBlobConnector;
    let old = connector
        .validate_connector_config(&connection_config(AzureBlobConnector::ID, 1, values()))
        .unwrap();
    assert_eq!(old.values["download_delivery"], "blob_origin");
    let decoded: AzureBlobConnectorConfigV1 = serde_json::from_value(values()).unwrap();
    assert_eq!(
        decoded.download_delivery,
        AzureBlobDownloadDelivery::BlobOrigin
    );
    assert_eq!(decoded.download_base_url, None);
    let mut input = values();
    input["download_delivery"] = json!("front_door_client_sas");
    input["download_base_url"] = json!(" https://files.example.com/ ");
    let normalized = connector
        .validate_connector_config(&connection_config(AzureBlobConnector::ID, 1, input))
        .unwrap();
    assert_eq!(
        normalized.values["download_base_url"],
        "https://files.example.com"
    );
    assert_eq!(
        connector.validate_connector_config(&normalized).unwrap(),
        normalized
    );
    let fields = connector.descriptor().fields;
    assert!(connector.descriptor().capabilities.custom_download_base_url);
    let url = fields
        .iter()
        .find(|field| field.name == "download_base_url")
        .unwrap();
    assert_eq!(url.allowed_endpoint_protocols, vec!["https:"]);
    assert_eq!(url.required_when.len(), 1);
    assert_eq!(url.visible_when, url.required_when);
    assert!(!url.secret);
    connector.localization().unwrap();
}

#[test]
fn rejects_unsupported_modes_and_missing_or_unsafe_delivery_urls() {
    for (mode, url) in [
        ("front_door_client_sas", Value::Null),
        (
            "front_door_client_sas",
            json!("https://files.example.com/assets"),
        ),
        ("front_door_client_sas", json!("http://files.example.com")),
        ("origin_sas_injection", json!("https://files.example.com")),
    ] {
        let mut input = values();
        input["download_delivery"] = json!(mode);
        input["download_base_url"] = url;
        assert!(
            AzureBlobConnector
                .validate_connector_config(&connection_config(AzureBlobConnector::ID, 1, input))
                .is_err()
        );
    }
}
