#![cfg(all(debug_assertions, feature = "openapi"))]
//! OpenAPI 生成测试。

use aster_drive::api::api_error_code::ApiErrorCode;
use aster_drive::api::openapi::ApiDoc;
use std::fs;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::OpenApi;

#[test]
fn generate_openapi() {
    let doc = ApiDoc::openapi();
    let json = serde_json::to_string_pretty(&doc).unwrap();
    fs::create_dir_all("./frontend-panel/generated").expect("Unable to create directory");
    fs::write("./frontend-panel/generated/openapi.json", json)
        .expect("Unable to write OpenAPI spec");
}

#[test]
fn api_error_code_openapi_schema_uses_wire_values() {
    let value = serde_json::to_value(ApiDoc::openapi()).unwrap();
    let schema = &value["components"]["schemas"]["ApiErrorCode"];
    let values = schema["enum"]
        .as_array()
        .expect("ApiErrorCode schema should have enum values")
        .iter()
        .map(|value| {
            value
                .as_str()
                .expect("ApiErrorCode enum value should be string")
        })
        .collect::<Vec<_>>();

    assert_eq!(schema["type"], "string");
    assert_eq!(values.len(), ApiErrorCode::ALL.len());
    for code in ApiErrorCode::ALL {
        assert!(
            values.contains(&code.as_str()),
            "OpenAPI schema missing {}",
            code.as_str()
        );
    }
    assert!(!values.contains(&"AuthFailed"));
    assert!(!values.contains(&"StorageTransient"));
    assert!(!values.contains(&"remote.dynamic"));
}

#[test]
fn api_error_code_openapi_schema_has_unique_values() {
    let value = serde_json::to_value(ApiDoc::openapi()).unwrap();
    let values = value["components"]["schemas"]["ApiErrorCode"]["enum"]
        .as_array()
        .expect("ApiErrorCode schema should have enum values");
    let mut seen = std::collections::HashSet::new();

    for value in values {
        let value = value
            .as_str()
            .expect("ApiErrorCode enum value should be string");
        assert!(seen.insert(value), "duplicate ApiErrorCode value {value}");
    }
}

#[test]
fn api_error_info_openapi_exposes_retryable_only() {
    let value = serde_json::to_value(ApiDoc::openapi()).unwrap();
    let info = &value["components"]["schemas"]["ApiErrorInfo"];
    let properties = info["properties"]
        .as_object()
        .expect("ApiErrorInfo should have properties");

    assert!(properties.contains_key("retryable"));
    assert!(!properties.contains_key("code"));
    assert!(!properties.contains_key("internal_code"));
    assert!(!properties.contains_key("subcode"));
    assert!(!properties.contains_key("api_code"));
}

#[test]
fn api_response_openapi_code_references_api_error_code_schema() {
    let value = serde_json::to_value(ApiDoc::openapi()).unwrap();
    let schemas = value["components"]["schemas"]
        .as_object()
        .expect("components schemas should be object");
    let responses = schemas
        .iter()
        .filter(|(name, _)| name.starts_with("ApiResponse_"));
    let mut checked = 0;

    for (name, schema) in responses {
        let code = &schema["properties"]["code"];
        assert_eq!(
            code["$ref"],
            serde_json::json!("#/components/schemas/ApiErrorCode"),
            "{name} should reference ApiErrorCode for code"
        );
        assert!(
            code.get("enum").is_none(),
            "{name} code should not inline enum values"
        );
        checked += 1;
    }

    assert!(checked > 0, "at least one ApiResponse schema should exist");
}

#[test]
fn presigned_upload_openapi_exposes_driver_owned_request_contract() {
    let value = serde_json::to_value(ApiDoc::openapi()).unwrap();
    let schemas = value["components"]["schemas"]
        .as_object()
        .expect("components schemas should be object");

    let request = &schemas["PresignedUploadRequest"];
    let request_properties = request["properties"]
        .as_object()
        .expect("presigned request properties should be object");
    assert!(request_properties.contains_key("url"));
    assert!(request_properties.contains_key("headers"));
    assert!(
        request_properties["url"]["description"]
            .as_str()
            .is_some_and(|description| description.contains("URL"))
    );
    assert!(
        request_properties["headers"]["description"]
            .as_str()
            .is_some_and(|description| description.contains("原样转发"))
    );

    let init_properties = schemas["InitUploadResponse"]["properties"]
        .as_object()
        .expect("init upload response properties should be object");
    assert!(init_properties.contains_key("presigned_request"));
    assert!(init_properties.contains_key("presigned_form_request"));
    assert!(!init_properties.contains_key("presigned_url"));
    assert!(!init_properties.contains_key("presigned_headers"));
}

#[test]
fn storage_connector_action_openapi_exposes_plugin_owned_contracts() {
    let value = serde_json::to_value(ApiDoc::openapi()).unwrap();
    let schemas = value["components"]["schemas"]
        .as_object()
        .expect("components schemas should be object");

    assert!(!schemas.contains_key("StoragePolicyExecutableAction"));
    assert!(!schemas.contains_key("StorageConnectorAffordanceAction"));
    assert!(!schemas.contains_key("ExecuteDraftStoragePolicyActionReq"));
    assert!(!schemas.contains_key("ExecuteSavedStoragePolicyActionReq"));

    let connector_config = &schemas["ConnectorConfigEnvelope"];
    assert_eq!(
        connector_config["properties"]["values"]["additionalProperties"]["$ref"],
        "#/components/schemas/StorageConnectorFieldValue"
    );

    let action = &schemas["StorageConnectorActionDescriptor"];
    let action_properties = action["properties"]
        .as_object()
        .expect("action descriptor properties should be object");
    for field in [
        "action_id",
        "label_key",
        "description_key",
        "kind",
        "endpoints",
        "fields",
        "requires_saved_policy",
        "requires_authorization",
        "mutates_remote_state",
        "requires_confirmation",
    ] {
        assert!(
            action_properties.contains_key(field),
            "missing action field {field}"
        );
    }
    assert!(!action_properties.contains_key("policy_action"));
    assert!(!action_properties.contains_key("affordance_action"));

    let kinds = schemas["StorageConnectorActionKind"]["enum"]
        .as_array()
        .expect("action kinds should be enum");
    assert!(kinds.contains(&serde_json::json!("custom")));
    assert!(!kinds.contains(&serde_json::json!("policy_action")));

    for request in [
        "ExecuteDraftStorageConnectorActionInput",
        "ExecuteSavedStorageConnectorActionInput",
    ] {
        let properties = schemas[request]["properties"]
            .as_object()
            .expect("action request properties should be object");
        assert!(properties.contains_key("action_id"));
        assert_eq!(
            properties["values"]["additionalProperties"]["$ref"],
            "#/components/schemas/StorageConnectorFieldValue"
        );
    }

    assert_eq!(
        schemas["StorageConnectorActionOutput"]["additionalProperties"],
        true
    );
}
