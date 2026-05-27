#![cfg(all(debug_assertions, feature = "openapi"))]
//! OpenAPI 生成测试。

use aster_drive::api::api_error_code::ApiErrorCode;
use aster_drive::api::openapi::ApiDoc;
use aster_drive::api::subcode::ApiSubcode;
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
fn api_subcode_openapi_schema_uses_wire_values() {
    let value = serde_json::to_value(ApiDoc::openapi()).unwrap();
    let schema = &value["components"]["schemas"]["ApiSubcode"];
    let values = schema["enum"]
        .as_array()
        .expect("ApiSubcode schema should have enum values")
        .iter()
        .map(|value| {
            value
                .as_str()
                .expect("ApiSubcode enum value should be string")
        })
        .collect::<Vec<_>>();

    assert_eq!(schema["type"], "string");
    assert_eq!(values.len(), ApiSubcode::ALL.len());
    for subcode in ApiSubcode::ALL {
        assert!(
            values.contains(&subcode.as_str()),
            "OpenAPI schema missing {}",
            subcode.as_str()
        );
    }
    assert!(!values.contains(&"ArchivePreviewDisabled"));
    assert!(!values.contains(&"PolicyUploadSessionsExist"));
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
fn api_error_info_openapi_code_references_api_error_code_schema() {
    let value = serde_json::to_value(ApiDoc::openapi()).unwrap();
    let code = &value["components"]["schemas"]["ApiErrorInfo"]["properties"]["code"];

    assert_eq!(
        code["$ref"],
        serde_json::json!("#/components/schemas/ApiErrorCode")
    );
    assert!(code.get("enum").is_none());
}

#[test]
fn api_error_info_openapi_subcode_references_api_subcode_schema() {
    let value = serde_json::to_value(ApiDoc::openapi()).unwrap();
    let subcode = &value["components"]["schemas"]["ApiErrorInfo"]["properties"]["subcode"];

    assert_eq!(
        subcode["oneOf"],
        serde_json::json!([
            { "type": "null" },
            { "$ref": "#/components/schemas/ApiSubcode" }
        ])
    );
    assert!(subcode.get("enum").is_none());
}
