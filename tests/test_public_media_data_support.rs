//! 集成测试：`public_media_data_support`。

#[macro_use]
mod common;

use actix_web::test;
use async_trait::async_trait;
use sea_orm::Set;
use serde_json::{Value, json};
use tokio::io::AsyncRead;

use aster_drive::errors::Result;
use aster_drive::storage::{
    BlobMetadata, NativeMediaMetadataRequest, NativeMediaMetadataResult,
    NativeMediaMetadataStorageDriver, StorageDriver,
};

struct NativeMediaMetadataSupportDriver;

#[async_trait]
impl StorageDriver for NativeMediaMetadataSupportDriver {
    async fn put(&self, path: &str, _data: &[u8]) -> Result<String> {
        Ok(path.to_string())
    }

    async fn get(&self, _path: &str) -> Result<Vec<u8>> {
        Ok(Vec::new())
    }

    async fn get_stream(&self, _path: &str) -> Result<Box<dyn AsyncRead + Unpin + Send>> {
        Ok(Box::new(std::io::Cursor::new(Vec::new())))
    }

    async fn delete(&self, _path: &str) -> Result<()> {
        Ok(())
    }

    async fn exists(&self, _path: &str) -> Result<bool> {
        Ok(true)
    }

    async fn metadata(&self, _path: &str) -> Result<BlobMetadata> {
        Ok(BlobMetadata {
            size: 0,
            content_type: None,
        })
    }

    fn as_native_media_metadata(&self) -> Option<&dyn NativeMediaMetadataStorageDriver> {
        Some(self)
    }
}

#[async_trait]
impl NativeMediaMetadataStorageDriver for NativeMediaMetadataSupportDriver {
    async fn get_native_media_metadata(
        &self,
        _request: &NativeMediaMetadataRequest,
    ) -> Result<Option<NativeMediaMetadataResult>> {
        Ok(None)
    }
}

fn available_test_command() -> String {
    std::env::current_exe()
        .expect("current test executable path should be available")
        .to_string_lossy()
        .into_owned()
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

    let now = chrono::Utc::now();
    let options = aster_drive::types::serialize_storage_policy_options(
        &aster_drive::types::StoragePolicyOptions {
            storage_native_processing_enabled: Some(true),
            storage_native_media_metadata_enabled: Some(true),
            media_metadata_extensions: vec![
                " .MP4 ".to_string(),
                "mp4".to_string(),
                ".m4a".to_string(),
            ],
            ..Default::default()
        },
    )
    .expect("storage policy options should serialize");
    let policy = aster_drive::db::repository::policy_repo::create(
        state.writer_db(),
        aster_drive::entities::storage_policy::ActiveModel {
            name: Set("Native Metadata".to_string()),
            driver_type: Set(aster_drive::types::DriverType::TencentCos),
            endpoint: Set("https://bucket-1250000000.cos.ap-guangzhou.myqcloud.com".to_string()),
            bucket: Set("bucket-1250000000".to_string()),
            access_key: Set("AKID".to_string()),
            secret_key: Set("SECRET".to_string()),
            base_path: Set(String::new()),
            max_file_size: Set(0),
            allowed_types: Set(aster_drive::types::StoredStoragePolicyAllowedTypes::empty()),
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
    state.driver_registry.insert_for_test(
        policy.id,
        std::sync::Arc::new(NativeMediaMetadataSupportDriver),
    );

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
}
