//! 集成测试：`public_thumbnail_support`。

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

async fn create_storage_native_thumbnail_policy(
    state: &aster_drive::runtime::PrimaryAppState,
    mut connection: aster_drive::storage::StorageConnectorConnectionInput,
    name: &str,
    extensions: Vec<String>,
) -> aster_drive_model::entities::storage_policy::Model {
    connection.behavior = aster_drive_storage::StoragePolicyBehaviorConfig {
        storage_native_thumbnail_enabled: true,
        storage_native_thumbnail_extensions: extensions,
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
async fn test_public_thumbnail_support_returns_default_builtin_extensions() {
    let state = common::setup().await;
    let app = create_test_app!(state);

    let req = test::TestRequest::get()
        .uri("/api/v1/public/thumbnail-support")
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
    assert!(body["data"].get("mime_types").is_none());
    assert_eq!(body["data"]["image_preview"]["enabled"], true);
    assert_eq!(body["data"]["image_thumbnail"]["enabled"], true);
    assert_eq!(body["data"]["audio_thumbnail"]["enabled"], true);
    assert_eq!(body["data"]["video_thumbnail"]["enabled"], false);
    assert!(body["data"].get("extensions").is_none());

    let image_preview_extensions = body["data"]["image_preview"]["extensions"]
        .as_array()
        .expect("image preview extensions should be an array");
    let audio_thumbnail_extensions = body["data"]["audio_thumbnail"]["extensions"]
        .as_array()
        .expect("audio thumbnail extensions should be an array");
    assert!(image_preview_extensions.iter().any(|value| value == "png"));
    assert!(image_preview_extensions.iter().any(|value| value == "jpg"));
    assert!(image_preview_extensions.iter().any(|value| value == "tiff"));
    assert!(!image_preview_extensions.iter().any(|value| value == "mp3"));
    assert!(
        audio_thumbnail_extensions
            .iter()
            .any(|value| value == "mp3")
    );
    assert!(
        audio_thumbnail_extensions
            .iter()
            .any(|value| value == "flac")
    );
    assert!(body["data"]["video_thumbnail"].get("extensions").is_none());
}

#[actix_web::test]
async fn test_public_thumbnail_support_merges_builtin_and_enabled_vips_extensions() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let command = available_test_command();

    let req = test::TestRequest::put()
        .uri("/api/v1/admin/config/media_processing_registry_json")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(json!({
            "value": json!({
                "version": 1,
                "processors": [
                    {
                        "kind": "vips_cli",
                        "enabled": true,
                        "extensions": ["HEIC", ".avif", "NEF", "raw", ".custom-vips"],
                        "config": {
                            "command": command
                        }
                    },
                    {
                        "kind": "ffmpeg_cli",
                        "enabled": true,
                        "uses": ["thumbnail:video"],
                        "extensions": ["mp4"],
                        "config": {
                            "command": command
                        }
                    },
                    {
                        "kind": "images",
                        "enabled": true
                    }
                ]
            })
            .to_string()
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri("/api/v1/public/thumbnail-support")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    assert!(body["data"].get("extensions").is_none());

    let image_preview_extensions = body["data"]["image_preview"]["extensions"]
        .as_array()
        .expect("image preview extensions should be an array");
    let video_thumbnail_extensions = body["data"]["video_thumbnail"]["extensions"]
        .as_array()
        .expect("video thumbnail extensions should be an array");
    assert!(image_preview_extensions.iter().any(|value| value == "png"));
    assert!(image_preview_extensions.iter().any(|value| value == "heic"));
    assert!(image_preview_extensions.iter().any(|value| value == "avif"));
    assert!(image_preview_extensions.iter().any(|value| value == "nef"));
    assert!(image_preview_extensions.iter().any(|value| value == "raw"));
    assert!(
        image_preview_extensions
            .iter()
            .any(|value| value == "custom-vips")
    );
    assert!(!image_preview_extensions.iter().any(|value| value == "mp4"));
    assert!(
        video_thumbnail_extensions
            .iter()
            .any(|value| value == "mp4")
    );
}

#[actix_web::test]
async fn test_public_thumbnail_support_omits_configured_extensions_when_cli_is_unavailable() {
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
                    "extensions": ["heic", "heif"],
                    "config": { "command": "/definitely-missing/aster-vips" }
                },
                {
                    "kind": "images",
                    "enabled": true,
                    "uses": ["thumbnail:image", "metadata:image"]
                }
            ]
        })
        .to_string(),
    ));
    let app = create_test_app!(state);

    let req = test::TestRequest::get()
        .uri("/api/v1/public/thumbnail-support")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let extensions = body["data"]["image_thumbnail"]["extensions"]
        .as_array()
        .expect("effective image thumbnail extensions should be present");

    assert!(extensions.iter().any(|value| value == "png"));
    assert!(!extensions.iter().any(|value| value == "heic"));
    assert!(!extensions.iter().any(|value| value == "heif"));
}

#[actix_web::test]
async fn test_public_thumbnail_support_backfills_old_lofty_uses() {
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
                        "kind": "lofty",
                        "enabled": true,
                        "uses": ["metadata:audio"],
                        "extensions": ["mp3"]
                    },
                    {
                        "kind": "images",
                        "enabled": true,
                        "uses": ["thumbnail:image", "metadata:image"]
                    }
                ]
            })
            .to_string()
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri("/api/v1/public/thumbnail-support")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    let audio_thumbnail_extensions = body["data"]["audio_thumbnail"]["extensions"]
        .as_array()
        .expect("audio thumbnail extensions should be an array");
    assert!(
        audio_thumbnail_extensions
            .iter()
            .any(|value| value == "mp3")
    );
}

#[actix_web::test]
async fn test_public_thumbnail_support_cache_is_invalidated_after_config_update() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let command = available_test_command();

    let req = test::TestRequest::get()
        .uri("/api/v1/public/thumbnail-support")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let image_thumbnail_extensions = body["data"]["image_thumbnail"]["extensions"]
        .as_array()
        .expect("image thumbnail extensions should be an array");
    assert!(
        !image_thumbnail_extensions
            .iter()
            .any(|value| value == "heic")
    );

    let req = test::TestRequest::put()
        .uri("/api/v1/admin/config/media_processing_registry_json")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(json!({
            "value": json!({
                "version": 1,
                "processors": [
                    {
                        "kind": "vips_cli",
                        "enabled": true,
                        "extensions": ["heic"],
                        "config": {
                            "command": command
                        }
                    },
                    {
                        "kind": "images",
                        "enabled": true
                    }
                ]
            })
            .to_string()
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri("/api/v1/public/thumbnail-support")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    let image_thumbnail_extensions = body["data"]["image_thumbnail"]["extensions"]
        .as_array()
        .expect("image thumbnail extensions should be an array");
    assert!(
        image_thumbnail_extensions
            .iter()
            .any(|value| value == "heic")
    );
}

#[actix_web::test]
async fn test_public_thumbnail_support_includes_storage_native_policy_without_caching_driver() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let policy = create_storage_native_thumbnail_policy(
        &state,
        common::tencent_cos_connection(
            "https://bucket-1250000000.cos.ap-guangzhou.myqcloud.com",
            "bucket-1250000000",
            "",
            "AKID",
            "SECRET",
        ),
        "Native Thumbnail",
        vec![" .HEIF ".to_string(), ".heif".to_string()],
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/api/v1/public/thumbnail-support")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let body: Value = test::read_body_json(resp).await;
    let image_thumbnail_extensions = body["data"]["image_thumbnail"]["extensions"]
        .as_array()
        .expect("image thumbnail extensions should be an array");
    assert!(
        image_thumbnail_extensions
            .iter()
            .any(|value| value == "heif")
    );
    assert!(
        !state.driver_registry.has_cached_driver_for_test(policy.id),
        "public thumbnail support must not instantiate a cold storage-native policy driver"
    );
}

#[actix_web::test]
async fn test_storage_native_thumbnail_rejects_unsupported_connector() {
    let state = common::setup().await;
    let mut connection = common::s3_connection(
        "https://s3.example.com",
        "unsupported-native-thumbnail",
        "",
        "AKID",
        "SECRET",
    );
    connection.behavior = aster_drive_storage::StoragePolicyBehaviorConfig {
        storage_native_thumbnail_enabled: true,
        storage_native_thumbnail_extensions: vec!["zzrawthumb".to_string()],
        ..Default::default()
    };
    let error = aster_drive::services::storage_policy::policy::create(
        &state,
        aster_drive::services::storage_policy::policy::CreateStoragePolicyInput {
            name: "Unsupported Native Thumbnail".to_string(),
            connection,
            max_file_size: 0,
            chunk_size: Some(5_242_880),
            is_default: false,
            allowed_types: None,
        },
    )
    .await
    .expect_err("unsupported connector must reject storage-native thumbnail processing");

    assert_eq!(
        error.api_error_code(),
        ApiErrorCode::PolicyNativeThumbnailUnsupported
    );
}
