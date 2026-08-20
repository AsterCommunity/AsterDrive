//! 集成测试：`public_media_data_support`。

use crate::common;

use actix_web::test;
use aster_drive::api::api_error_code::ApiErrorCode;
use serde_json::{Value, json};

fn available_test_command() -> String {
    std::env::current_exe()
        .expect("current test executable path should be available")
        .to_string_lossy()
        .into_owned()
}

async fn create_storage_native_media_metadata_policy(
    state: &aster_drive::runtime::PrimaryAppState,
    mut connection: aster_drive::storage::StorageConnectorConnectionInput,
    name: &str,
    extensions: Vec<String>,
) -> aster_drive_model::entities::storage_policy::Model {
    connection.behavior = aster_drive_storage::StoragePolicyBehaviorConfig {
        storage_native_media_metadata_enabled: true,
        storage_native_media_metadata_extensions: extensions,
        ..Default::default()
    };
    let created = aster_drive::services::storage_policy::policy::create(
        state,
        aster_drive::services::storage_policy::policy::CreateStoragePolicyInput {
            name: name.to_string(),
            connection,
            max_file_size: 0,
            chunk_size: Some(5_242_880),
            is_default: false,
            allowed_types: None,
        },
    )
    .await
    .expect("storage-native policy should be created through the connector service");
    aster_drive::db::repository::policy_repo::find_by_id(state.writer_db(), created.id)
        .await
        .expect("created storage-native policy should be queryable")
}

#[actix_web::test]
async fn test_public_media_data_support_returns_default_capabilities() {
    let state = common::setup().await;
    let app = create_test_app!(state);

    let req = test::TestRequest::get()
        .uri("/api/v1/public/media-data-support")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers()
            .get("Cache-Control")
            .and_then(|value| value.to_str().ok()),
        Some("public, max-age=60")
    );

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["version"], 1);
    assert_eq!(body["data"]["enabled"], true);
    assert_eq!(body["data"]["max_source_bytes"], 256 * 1024 * 1024);
    assert_eq!(body["data"]["kinds"]["image"]["enabled"], true);
    assert_eq!(body["data"]["kinds"]["image"]["match"], "extensions");
    assert!(
        body["data"]["kinds"]["image"]["extensions"]
            .as_array()
            .expect("image extensions should be an array")
            .iter()
            .any(|value| value == "jpg")
    );
    assert_eq!(body["data"]["kinds"]["audio"]["enabled"], true);
    assert!(
        body["data"]["kinds"]["audio"]["extensions"]
            .as_array()
            .expect("audio extensions should be an array")
            .iter()
            .any(|value| value == "mp3")
    );
    assert_eq!(body["data"]["kinds"]["video"]["enabled"], false);
}

#[actix_web::test]
async fn test_public_media_data_support_exposes_enabled_ffprobe_extensions() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::put()
        .uri("/api/v1/admin/config/media_processing_registry_json")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(json!({
            "value": json!({
                "version": 2,
                "processors": [
                    {
                        "kind": "ffprobe_cli",
                        "enabled": true,
                        "uses": ["metadata:video"],
                        "extensions": ["MP4", ".mov"],
                        "config": {
                            "command": available_test_command()
                        }
                    },
                    {
                        "kind": "images",
                        "enabled": true,
                        "uses": ["metadata:image"]
                    },
                    {
                        "kind": "lofty",
                        "enabled": true,
                        "uses": ["metadata:audio"]
                    }
                ]
            })
            .to_string()
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri("/api/v1/public/media-data-support")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["kinds"]["video"]["enabled"], true);
    assert_eq!(body["data"]["kinds"]["video"]["match"], "extensions");
    assert_eq!(
        body["data"]["kinds"]["video"]["extensions"],
        json!(["mov", "mp4"])
    );
}

#[actix_web::test]
async fn test_heic_metadata_support_is_independent_from_effective_thumbnail_support() {
    let state = common::setup().await;
    state.runtime_config.apply(common::system_config_model(
        "media_processing_registry_json",
        &json!({
            "version": 2,
            "processors": [
                {
                    "kind": "vips_cli",
                    "enabled": true,
                    "uses": ["thumbnail:image"],
                    "extensions": ["heic"],
                    "config": { "command": "/definitely-missing/aster-vips" }
                },
                {
                    "kind": "images",
                    "enabled": true,
                    "uses": ["thumbnail:image", "metadata:image"]
                },
                {
                    "kind": "lofty",
                    "enabled": true,
                    "uses": ["thumbnail:audio", "metadata:audio"]
                }
            ]
        })
        .to_string(),
    ));
    let app = create_test_app!(state);

    let media_req = test::TestRequest::get()
        .uri("/api/v1/public/media-data-support")
        .to_request();
    let media_resp = test::call_service(&app, media_req).await;
    assert_eq!(media_resp.status(), 200);
    let media_body: Value = test::read_body_json(media_resp).await;
    let metadata_extensions = media_body["data"]["kinds"]["image"]["extensions"]
        .as_array()
        .expect("image metadata extensions should be present");
    assert!(metadata_extensions.iter().any(|value| value == "heic"));

    let thumbnail_req = test::TestRequest::get()
        .uri("/api/v1/public/thumbnail-support")
        .to_request();
    let thumbnail_resp = test::call_service(&app, thumbnail_req).await;
    assert_eq!(thumbnail_resp.status(), 200);
    let thumbnail_body: Value = test::read_body_json(thumbnail_resp).await;
    let thumbnail_extensions = thumbnail_body["data"]["image_thumbnail"]["extensions"]
        .as_array()
        .expect("image thumbnail extensions should be present");
    assert!(!thumbnail_extensions.iter().any(|value| value == "heic"));
}

#[actix_web::test]
async fn test_public_media_data_support_cache_is_invalidated_after_config_update() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::get()
        .uri("/api/v1/public/media-data-support")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["enabled"], true);
    assert_eq!(body["data"]["kinds"]["image"]["enabled"], true);

    let req = test::TestRequest::put()
        .uri("/api/v1/admin/config/media_metadata_enabled")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(json!({ "value": "false" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri("/api/v1/public/media-data-support")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["enabled"], false);
    assert_eq!(body["data"]["kinds"]["image"]["enabled"], false);
    assert_eq!(body["data"]["kinds"]["audio"]["enabled"], false);
    assert_eq!(body["data"]["kinds"]["video"]["enabled"], false);
}

#[actix_web::test]
async fn test_public_media_data_support_includes_storage_native_policy_extensions() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let policy = create_storage_native_media_metadata_policy(
        &state,
        common::tencent_cos_connection(
            "https://bucket-1250000000.cos.ap-guangzhou.myqcloud.com",
            "bucket-1250000000",
            "",
            "AKID",
            "SECRET",
        ),
        "Native Metadata",
        vec![" .MP4 ".to_string(), "mp4".to_string(), ".m4a".to_string()],
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/api/v1/public/media-data-support")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["kinds"]["audio"]["enabled"], true);
    assert_eq!(body["data"]["kinds"]["video"]["enabled"], true);
    assert_eq!(
        body["data"]["kinds"]["video"]["extensions"],
        json!(["m4a", "mp4"])
    );
    assert!(
        body["data"]["kinds"]["audio"]["extensions"]
            .as_array()
            .expect("audio extensions should be an array")
            .iter()
            .any(|value| value == "m4a")
    );
    assert!(
        !state.driver_registry.has_cached_driver_for_test(policy.id),
        "public media support must not instantiate a cold storage-native policy driver"
    );
}

#[actix_web::test]
async fn test_storage_native_media_metadata_rejects_unsupported_connector() {
    let state = common::setup().await;
    let mut connection = common::s3_connection(
        "https://s3.example.com",
        "unsupported-native-metadata",
        "",
        "AKID",
        "SECRET",
    );
    connection.behavior = aster_drive_storage::StoragePolicyBehaviorConfig {
        storage_native_media_metadata_enabled: true,
        storage_native_media_metadata_extensions: vec!["zzrawmedia".to_string()],
        ..Default::default()
    };
    let error = aster_drive::services::storage_policy::policy::create(
        &state,
        aster_drive::services::storage_policy::policy::CreateStoragePolicyInput {
            name: "Unsupported Native Metadata".to_string(),
            connection,
            max_file_size: 0,
            chunk_size: Some(5_242_880),
            is_default: false,
            allowed_types: None,
        },
    )
    .await
    .expect_err("unsupported connector must reject storage-native media metadata");

    assert_eq!(
        error.api_error_code(),
        ApiErrorCode::PolicyNativeMediaMetadataUnsupported
    );
}
