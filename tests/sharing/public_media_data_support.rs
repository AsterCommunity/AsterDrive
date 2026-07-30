//! 集成测试：`public_media_data_support`。

use crate::common;
use aster_drive::runtime::SharedRuntimeState;

use actix_web::test;
use sea_orm::Set;
use serde_json::{Value, json};

fn extension_values<'a>(kind_support: &'a Value, name: &str) -> &'a [Value] {
    kind_support[name]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
}

fn available_test_command() -> String {
    std::env::current_exe()
        .expect("current test executable path should be available")
        .to_string_lossy()
        .into_owned()
}

async fn create_storage_native_media_metadata_policy(
    state: &aster_drive::runtime::PrimaryAppState,
    driver_type: aster_drive_model::types::DriverType,
    name: &str,
    extensions: Vec<String>,
) -> aster_drive_model::entities::storage_policy::Model {
    let now = chrono::Utc::now();
    let options = aster_drive_model::types::serialize_storage_policy_options(
        &aster_drive_model::types::StoragePolicyOptions {
            storage_native_processing_enabled: Some(true),
            storage_native_media_metadata_enabled: Some(true),
            media_metadata_extensions: extensions,
            ..Default::default()
        },
    )
    .expect("storage policy options should serialize");
    let policy = aster_drive::db::repository::policy_repo::create(
        state.writer_db(),
        aster_drive_model::entities::storage_policy::ActiveModel {
            name: Set(name.to_string()),
            driver_type: Set(driver_type),
            endpoint: Set("https://bucket-1250000000.cos.ap-guangzhou.myqcloud.com".to_string()),
            bucket: Set("bucket-1250000000".to_string()),
            access_key: Set("AKID".to_string()),
            secret_key: Set("SECRET".to_string()),
            base_path: Set(String::new()),
            max_file_size: Set(0),
            allowed_types: Set(aster_drive_model::types::StoredStoragePolicyAllowedTypes::empty()),
            options: Set(options),
            is_default: Set(false),
            chunk_size: Set(5_242_880),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        },
    )
    .await
    .expect("policy should be created");
    state
        .policy_snapshot
        .reload(state.reader_db())
        .await
        .expect("policy snapshot should reload");
    policy
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
        aster_drive_model::types::DriverType::TencentCos,
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
async fn test_public_media_data_support_ignores_storage_native_options_for_unsupported_driver() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let policy = create_storage_native_media_metadata_policy(
        &state,
        aster_drive_model::types::DriverType::S3,
        "Unsupported Native Metadata",
        vec!["zzrawmedia".to_string()],
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/api/v1/public/media-data-support")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    let video_extensions = extension_values(&body["data"]["kinds"]["video"], "extensions");
    let audio_extensions = extension_values(&body["data"]["kinds"]["audio"], "extensions");
    assert!(!video_extensions.iter().any(|value| value == "zzrawmedia"));
    assert!(!audio_extensions.iter().any(|value| value == "zzrawmedia"));
    assert!(
        !state.driver_registry.has_cached_driver_for_test(policy.id),
        "unsupported public media policy must not be instantiated just to reject capability"
    );
}
