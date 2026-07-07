//! 集成测试：`thumbnail`。

#[macro_use]
mod common;

use actix_web::test;
use aster_drive::db::repository::{background_task_repo, file_repo, policy_repo};
use aster_drive::runtime::{PrimaryAppState, SharedRuntimeState};
use aster_drive::types::{
    BackgroundTaskKind, BackgroundTaskStatus, MediaProcessorKind, StoragePolicyOptions,
    serialize_storage_policy_options,
};
use base64::Engine;
use image::GenericImageView;
use sea_orm::{ActiveModelTrait, Set};
use serde_json::{Value, json};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

/// 生成一个最小的 1x1 红色 PNG。
fn tiny_png() -> Vec<u8> {
    let mut buf = std::io::Cursor::new(Vec::new());
    let encoder = image::codecs::png::PngEncoder::new(&mut buf);
    image::ImageEncoder::write_image(encoder, &[255, 0, 0], 1, 1, image::ExtendedColorType::Rgb8)
        .unwrap();
    buf.into_inner()
}

fn tiny_webp() -> Vec<u8> {
    let image =
        image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(1, 1, image::Rgb([255, 0, 0])));
    let mut buf = std::io::Cursor::new(Vec::new());
    image.write_to(&mut buf, image::ImageFormat::WebP).unwrap();
    buf.into_inner()
}

fn tiny_mp4() -> Vec<u8> {
    base64::engine::general_purpose::STANDARD
        .decode("AAAAIGZ0eXBpc29tAAACAGlzb21pc28yYXZjMW1wNDEAAAN1bW9vdgAAAGxtdmhkAAAAAAAAAAAAAAAAAAAD6AAAAMgAAQAAAQAAAAAAAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAgAAAp90cmFrAAAAXHRraGQAAAADAAAAAAAAAAAAAAABAAAAAAAAAMgAAAAAAAAAAAAAAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAAAAABAAAAAABAAAAAQAAAAAAAkZWR0cwAAABxlbHN0AAAAAAAAAAEAAADIAAAEAAABAAAAAAIXbWRpYQAAACBtZGhkAAAAAAAAAAAAAAAAAAAyAAAACgBVxAAAAAAALWhkbHIAAAAAAAAAAHZpZGUAAAAAAAAAAAAAAABWaWRlb0hhbmRsZXIAAAABwm1pbmYAAAAUdm1oZAAAAAEAAAAAAAAAAAAAACRkaW5mAAAAHGRyZWYAAAAAAAAAAQAAAAx1cmwgAAAAAQAAAYJzdGJsAAAAvnN0c2QAAAAAAAAAAQAAAK5hdmMxAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAAAABAAEABIAAAASAAAAAAAAAABFUxhdmM2Mi4yOC4xMDAgbGlieDI2NAAAAAAAAAAAAAAAGP//AAAANGF2Y0MBZAAK/+EAF2dkAAqs2V7ARAAAAwAEAAADAMg8SJZYAQAGaOvjyyLA/fj4AAAAABBwYXNwAAAAAQAAAAEAAAAUYnRydAAAAAAAAHcQAAAAAAAAABhzdHRzAAAAAAAAAAEAAAAFAAACAAAAABRzdHNzAAAAAAAAAAEAAAABAAAAOGN0dHMAAAAAAAAABQAAAAEAAAQAAAAAAQAACgAAAAABAAAEAAAAAAEAAAAAAAAAAQAAAgAAAAAcc3RzYwAAAAAAAAABAAAAAQAAAAUAAAABAAAAKHN0c3oAAAAAAAAAAAAAAAUAAALKAAAADAAAAAwAAAAMAAAADAAAABRzdGNvAAAAAAAAAAEAAAOlAAAAYnVkdGEAAABabWV0YQAAAAAAAAAhaGRscgAAAAAAAAAAbWRpcmFwcGwAAAAAAAAAAAAAAAAtaWxzdAAAACWpdG9vAAAAHWRhdGEAAAABAAAAAExhdmY2Mi4xMi4xMDAAAAAIZnJlZQAAAwJtZGF0AAACrgYF//+q3EXpvebZSLeWLNgg2SPu73gyNjQgLSBjb3JlIDE2NSByMzIyMiBiMzU2MDVhIC0gSC4yNjQvTVBFRy00IEFWQyBjb2RlYyAtIENvcHlsZWZ0IDIwMDMtMjAyNSAtIGh0dHA6Ly93d3cudmlkZW9sYW4ub3JnL3gyNjQuaHRtbCAtIG9wdGlvbnM6IGNhYmFjPTEgcmVmPTMgZGVibG9jaz0xOjA6MCBhbmFseXNlPTB4MzoweDExMyBtZT1oZXggc3VibWU9NyBwc3k9MSBwc3lfcmQ9MS4wMDowLjAwIG1peGVkX3JlZj0xIG1lX3JhbmdlPTE2IGNocm9tYV9tZT0xIHRyZWxsaXM9MSA4eDhkY3Q9MSBjcW09MCBkZWFkem9uZT0yMSwxMSBmYXN0X3Bza2lwPTEgY2hyb21hX3FwX29mZnNldD0tMiB0aHJlYWRzPTEgbG9va2FoZWFkX3RocmVhZHM9MSBzbGljZWRfdGhyZWFkcz0wIG5yPTAgZGVjaW1hdGU9MSBpbnRlcmxhY2VkPTAgYmx1cmF5X2NvbXBhdD0wIGNvbnN0cmFpbmVkX2ludHJhPTAgYmZyYW1lcz0zIGJfcHlyYW1pZD0yIGJfYWRhcHQ9MSBiX2JpYXM9MCBkaXJlY3Q9MSB3ZWlnaHRiPTEgb3Blbl9nb3A9MCB3ZWlnaHRwPTIga2V5aW50PTI1MCBrZXlpbnRfbWluPTI1IHNjZW5lY3V0PTQwIGludHJhX3JlZnJlc2g9MCByY19sb29rYWhlYWQ9NDAgcmM9Y3JmIG1idHJlZT0xIGNyZj0yMy4wIHFjb21wPTAuNjAgcXBtaW49MCBxcG1heD02OSBxcHN0ZXA9NCBpcF9yYXRpbz0xLjQwIGFxPTE6MS4wMACAAAAAFGWIhAAz//7fMvgUzcWJzsyAXJ6XAAAACEGaJGxCv/7AAAAACEGeQniF/8GBAAAACAGeYXRCv8SAAAAACAGeY2pCv8SB")
        .expect("embedded tiny mp4 fixture should decode")
}

fn current_thumb_path(blob_hash: &str) -> String {
    format!(
        "_thumb/images/1/{}/{}/{}.webp",
        &blob_hash[..2],
        &blob_hash[2..4],
        blob_hash
    )
}

fn vips_thumb_path(blob_hash: &str) -> String {
    format!(
        "_thumb/vips-cli/1/{}/{}/{}.webp",
        &blob_hash[..2],
        &blob_hash[2..4],
        blob_hash
    )
}

fn ffmpeg_thumb_path(blob_hash: &str) -> String {
    format!(
        "_thumb/ffmpeg-cli/1/{}/{}/{}.webp",
        &blob_hash[..2],
        &blob_hash[2..4],
        blob_hash
    )
}

fn thumbnail_registry_json_with_vips_command(command: &str) -> String {
    json!({
        "version": 1,
        "processors": [
            {
                "enabled": true,
                "kind": "vips_cli",
                "config": {
                    "command": command,
                }
            }
        ]
    })
    .to_string()
}

fn ffmpeg_command_for_tests() -> Option<String> {
    [
        std::env::var("ASTER_TEST_FFMPEG_COMMAND").ok(),
        Some("ffmpeg".to_string()),
    ]
    .into_iter()
    .flatten()
    .find(|candidate| aster_drive::config::media_processing::command_is_available(candidate))
}

#[cfg(unix)]
fn write_fake_vips_thumbnail_command() -> (std::path::PathBuf, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "aster-drive-thumbnail-vips-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&dir).unwrap();

    let output_fixture_path = dir.join("fixture.webp");
    std::fs::write(&output_fixture_path, tiny_webp()).unwrap();
    let input_log_path = dir.join("input-path.txt");

    let script_path = dir.join("fake-vips");
    std::fs::write(
        &script_path,
        format!(
            "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then\n  echo \"vips-8.16.0\"\n  exit 0\nfi\nif [ \"$1\" = \"thumbnail\" ]; then\n  printf '%s' \"$2\" > '{}'\n  cp '{}' \"$3\"\n  exit 0\nfi\necho \"unexpected args: $@\" >&2\nexit 1\n",
            input_log_path.display(),
            output_fixture_path.display()
        ),
    )
    .unwrap();
    let mut permissions = std::fs::metadata(&script_path).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&script_path, permissions).unwrap();
    (script_path, input_log_path)
}

macro_rules! upload_file_bytes {
    ($app:expr, $token:expr, $filename:expr, $content_type:expr, $bytes:expr) => {{
        let boundary = "----TestBound";
        let mut payload = Vec::new();
        payload.extend_from_slice(b"------TestBound\r\n");
        payload.extend_from_slice(
            format!(
                "Content-Disposition: form-data; name=\"file\"; filename=\"{}\"\r\n",
                $filename
            )
            .as_bytes(),
        );
        payload.extend_from_slice(format!("Content-Type: {}\r\n\r\n", $content_type).as_bytes());
        payload.extend_from_slice(($bytes).as_ref());
        payload.extend_from_slice(b"\r\n------TestBound--\r\n");

        let req = test::TestRequest::post()
            .uri("/api/v1/files/upload")
            .insert_header(("Cookie", common::access_cookie_header(&$token)))
            .insert_header(common::csrf_header_for(&$token))
            .insert_header((
                "Content-Type",
                format!("multipart/form-data; boundary={boundary}"),
            ))
            .set_payload(payload)
            .to_request();
        let resp: actix_web::dev::ServiceResponse = test::call_service(&$app, req).await;
        assert_eq!(resp.status(), 201, "upload should return 201");
        let body: Value = test::read_body_json(resp).await;
        body["data"]["id"].as_i64().unwrap()
    }};
}

macro_rules! request_thumbnail {
    ($app:expr, $token:expr, $file_id:expr) => {{
        let req = test::TestRequest::get()
            .uri(&format!("/api/v1/files/{}/thumbnail", $file_id))
            .insert_header(("Cookie", common::access_cookie_header(&$token)))
            .insert_header(common::csrf_header_for(&$token))
            .to_request();
        test::call_service(&$app, req).await
    }};
}

async fn thumbnail_task_display_name(state: &PrimaryAppState, file_id: i64) -> String {
    thumbnail_task_display_name_for_processor(state, file_id, MediaProcessorKind::Images).await
}

async fn thumbnail_task_display_name_for_processor(
    state: &PrimaryAppState,
    file_id: i64,
    processor: MediaProcessorKind,
) -> String {
    let file = file_repo::find_by_id(state.writer_db(), file_id)
        .await
        .unwrap();
    format!(
        "Generate thumbnail for blob #{} via {}",
        file.blob_id,
        thumbnail_processor_display_name(processor)
    )
}

fn thumbnail_processor_display_name(processor: MediaProcessorKind) -> &'static str {
    match processor {
        MediaProcessorKind::Images => "AsterDrive built-in",
        MediaProcessorKind::Lofty => "AsterDrive built-in audio",
        _ => processor.as_str(),
    }
}

async fn blob_for_file(
    state: &PrimaryAppState,
    file_id: i64,
) -> aster_drive::entities::file_blob::Model {
    let file = file_repo::find_by_id(state.writer_db(), file_id)
        .await
        .unwrap();
    file_repo::find_blob_by_id(state.writer_db(), file.blob_id)
        .await
        .unwrap()
}

async fn thumbnail_task_count(state: &PrimaryAppState, file_id: i64) -> usize {
    let display_name = thumbnail_task_display_name(state, file_id).await;
    background_task_repo::list_recent(state.writer_db(), 32)
        .await
        .unwrap()
        .into_iter()
        .filter(|task| {
            task.kind == BackgroundTaskKind::ThumbnailGenerate && task.display_name == display_name
        })
        .count()
}

async fn enable_default_policy_storage_native_thumbnail(state: &PrimaryAppState) {
    let policy = policy_repo::find_default(state.writer_db())
        .await
        .unwrap()
        .expect("default policy should exist");
    let mut active: aster_drive::entities::storage_policy::ActiveModel = policy.into();
    active.options = Set(serialize_storage_policy_options(&StoragePolicyOptions {
        thumbnail_processor: Some(MediaProcessorKind::StorageNative),
        thumbnail_extensions: vec!["png".to_string()],
        ..Default::default()
    })
    .unwrap());
    active.update(state.writer_db()).await.unwrap();
    state
        .policy_snapshot
        .reload(state.writer_db())
        .await
        .unwrap();
}

async fn latest_thumbnail_task(
    state: &PrimaryAppState,
    file_id: i64,
) -> aster_drive::entities::background_task::Model {
    latest_thumbnail_task_for_processor(state, file_id, MediaProcessorKind::Images).await
}

async fn latest_thumbnail_task_for_processor(
    state: &PrimaryAppState,
    file_id: i64,
    processor: MediaProcessorKind,
) -> aster_drive::entities::background_task::Model {
    let display_name = thumbnail_task_display_name_for_processor(state, file_id, processor).await;
    background_task_repo::find_latest_by_kind_and_display_name(
        state.writer_db(),
        BackgroundTaskKind::ThumbnailGenerate,
        &display_name,
    )
    .await
    .unwrap()
    .expect("thumbnail task should exist")
}

#[actix_web::test]
async fn test_thumbnail_task_creation_triggers_dispatch_wakeup() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);

    let file_id = upload_file_bytes!(app, token, "wakeup.png", "image/png", tiny_png());
    let notified = state.background_task_dispatch_wakeup.notified();
    tokio::pin!(notified);

    let resp = request_thumbnail!(app, token, file_id);
    assert_eq!(resp.status(), 202);

    tokio::time::timeout(std::time::Duration::from_secs(1), &mut notified)
        .await
        .expect("thumbnail task creation should wake the dispatcher");
}

#[actix_web::test]
async fn test_thumbnail_returns_202_when_not_ready() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);

    let file_id = upload_file_bytes!(app, token, "test.png", "image/png", tiny_png());
    let resp = request_thumbnail!(app, token, file_id);

    assert_eq!(resp.status(), 202);
    assert_eq!(
        resp.headers()
            .get("Retry-After")
            .and_then(|value| value.to_str().ok()),
        Some("2")
    );

    let task = latest_thumbnail_task(&state, file_id).await;
    assert_eq!(task.progress_current, 0);
    assert_eq!(task.progress_total, 4);
    assert_eq!(task.max_attempts, 1);
    let steps: Vec<Value> = serde_json::from_str(task.steps_json.as_ref().unwrap().as_ref())
        .expect("thumbnail task steps should be valid json");
    assert_eq!(steps.len(), 4);
    assert_eq!(steps[0]["key"], "waiting");
    assert_eq!(steps[1]["key"], "inspect_source");
}

#[actix_web::test]
async fn test_thumbnail_audio_schedules_lofty_processor_for_legacy_metadata_only_config() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
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

    let file_id = upload_file_bytes!(
        app,
        token,
        "The Score - Real Life.mp3",
        "audio/mpeg",
        b"not a real mp3"
    );
    let resp = request_thumbnail!(app, token, file_id);
    assert_eq!(resp.status(), 202);

    // The uploaded bytes are intentionally not decoded here: this only verifies that
    // legacy Lofty configs declaring metadata:audio are normalized into thumbnail
    // scheduling support. The Lofty renderer path covers embedded-artwork decoding.
    let task =
        latest_thumbnail_task_for_processor(&state, file_id, MediaProcessorKind::Lofty).await;
    assert_eq!(task.status, BackgroundTaskStatus::Pending);
    let payload: Value = serde_json::from_str(task.payload_json.as_ref()).unwrap();
    assert_eq!(payload["processor"], "lofty");
    assert_eq!(payload["source_file_name"], "The Score - Real Life.mp3");
}

#[actix_web::test]
async fn test_thumbnail_returns_200_after_generation() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);

    let file_id = upload_file_bytes!(app, token, "test.png", "image/png", tiny_png());

    let first = request_thumbnail!(app, token, file_id);
    assert_eq!(first.status(), 202);

    aster_drive::services::task::drain(&state)
        .await
        .unwrap();

    let task = latest_thumbnail_task(&state, file_id).await;
    assert_eq!(task.status, BackgroundTaskStatus::Succeeded);
    assert_eq!(task.max_attempts, 1);
    let blob = blob_for_file(&state, file_id).await;
    let expected_thumbnail_path = current_thumb_path(&blob.hash);
    assert_eq!(
        blob.thumbnail_path.as_deref(),
        Some(expected_thumbnail_path.as_str())
    );
    assert_eq!(blob.thumbnail_processor.as_deref(), Some("images"));
    assert_eq!(blob.thumbnail_version.as_deref(), Some("1"));

    let resp = request_thumbnail!(app, token, file_id);
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers()
            .get("Content-Type")
            .and_then(|value| value.to_str().ok()),
        Some("image/webp")
    );

    let cache_control = resp
        .headers()
        .get("Cache-Control")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    assert!(cache_control.contains("private"));
    assert!(cache_control.contains("must-revalidate"));
    assert!(!cache_control.contains("public"));
    assert!(!cache_control.contains("immutable"));
}

#[actix_web::test]
async fn test_thumbnail_returns_304_for_matching_if_none_match() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);

    let file_id = upload_file_bytes!(app, token, "test.png", "image/png", tiny_png());

    let first = request_thumbnail!(app, token, file_id);
    assert_eq!(first.status(), 202);

    aster_drive::services::task::drain(&state)
        .await
        .unwrap();

    let resp = request_thumbnail!(app, token, file_id);
    assert_eq!(resp.status(), 200);
    let etag = resp
        .headers()
        .get("ETag")
        .and_then(|value| value.to_str().ok())
        .expect("thumbnail response should include ETag")
        .to_string();
    assert!(etag.contains("thumb-images-1-"));

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/files/{file_id}/thumbnail"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .insert_header(("If-None-Match", etag.as_str()))
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), 304);
    assert_eq!(
        resp.headers()
            .get("ETag")
            .and_then(|value| value.to_str().ok()),
        Some(etag.as_str())
    );
    assert_eq!(
        resp.headers()
            .get("Cache-Control")
            .and_then(|value| value.to_str().ok()),
        Some("private, max-age=0, must-revalidate")
    );
}

#[actix_web::test]
async fn test_thumbnail_non_image_returns_bad_request_without_task() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);

    let file_id = upload_test_file!(app, token);
    let resp = request_thumbnail!(app, token, file_id);

    assert_eq!(resp.status(), 400);
    let tasks = background_task_repo::list_recent(state.writer_db(), 16)
        .await
        .unwrap();
    assert!(
        tasks
            .into_iter()
            .all(|task| task.kind != BackgroundTaskKind::ThumbnailGenerate)
    );
}

#[actix_web::test]
async fn test_thumbnail_dedup_same_blob() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);

    let file_id = upload_file_bytes!(app, token, "test.png", "image/png", tiny_png());

    for _ in 0..5 {
        let resp = request_thumbnail!(app, token, file_id);
        let status = resp.status().as_u16();
        assert!(
            status == 202 || status == 200,
            "thumbnail request should be pending or ready, got {status}"
        );
    }

    assert_eq!(thumbnail_task_count(&state, file_id).await, 1);

    aster_drive::services::task::drain(&state)
        .await
        .unwrap();

    let resp = request_thumbnail!(app, token, file_id);
    assert_eq!(resp.status(), 200);
    assert_eq!(thumbnail_task_count(&state, file_id).await, 1);
}

#[actix_web::test]
async fn test_thumbnail_failed_task_returns_not_found_without_requeue() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);

    let invalid_png = b"not-a-real-png".to_vec();
    let file_id = upload_file_bytes!(app, token, "broken.png", "image/png", invalid_png);

    let first = request_thumbnail!(app, token, file_id);
    assert_eq!(first.status(), 202);

    aster_drive::services::task::drain(&state)
        .await
        .unwrap();

    let task = latest_thumbnail_task(&state, file_id).await;
    assert_eq!(task.status, BackgroundTaskStatus::Failed);
    assert_eq!(task.attempt_count, 1);

    let count_before = thumbnail_task_count(&state, file_id).await;
    assert_eq!(count_before, 1);

    let resp = request_thumbnail!(app, token, file_id);
    assert_eq!(resp.status(), 404);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], json!("not_found"));

    for _ in 0..3 {
        let resp = request_thumbnail!(app, token, file_id);
        assert_eq!(resp.status(), 404);
    }

    let count_after = thumbnail_task_count(&state, file_id).await;
    assert_eq!(count_after, count_before);
}

#[actix_web::test]
async fn test_thumbnail_vips_cli_missing_command_falls_back_to_images() {
    let state = common::setup().await;
    state.runtime_config.apply(common::system_config_model(
        "media_processing_registry_json",
        &thumbnail_registry_json_with_vips_command("definitely-missing-vips-cli"),
    ));

    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let file_id = upload_file_bytes!(app, token, "test.png", "image/png", tiny_png());

    let first = request_thumbnail!(app, token, file_id);
    assert_eq!(first.status(), 202);

    aster_drive::services::task::drain(&state)
        .await
        .unwrap();

    let task = latest_thumbnail_task(&state, file_id).await;
    assert_eq!(task.status, BackgroundTaskStatus::Succeeded);
    assert_eq!(thumbnail_task_count(&state, file_id).await, 1);

    let second = request_thumbnail!(app, token, file_id);
    assert_eq!(second.status(), 200);
}

#[cfg(unix)]
#[actix_web::test]
async fn test_thumbnail_heic_uses_vips_cli_processor_when_extension_matches() {
    let state = common::setup().await;
    let (fake_vips, input_log_path) = write_fake_vips_thumbnail_command();
    state.runtime_config.apply(common::system_config_model(
        "media_processing_registry_json",
        &json!({
            "version": 1,
            "processors": [
                {
                    "kind": "vips_cli",
                    "enabled": true,
                    "extensions": ["heic"],
                    "config": {
                        "command": fake_vips
                    }
                },
                {
                    "kind": "images",
                    "enabled": true
                }
            ]
        })
        .to_string(),
    ));

    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let file_id = upload_file_bytes!(
        app,
        token,
        "capture.heic",
        "image/heic",
        b"fake-heic".to_vec()
    );

    let first = request_thumbnail!(app, token, file_id);
    assert_eq!(first.status(), 202);

    aster_drive::services::task::drain(&state)
        .await
        .unwrap();

    let task =
        latest_thumbnail_task_for_processor(&state, file_id, MediaProcessorKind::VipsCli).await;
    assert_eq!(task.status, BackgroundTaskStatus::Succeeded);

    let blob = blob_for_file(&state, file_id).await;
    let expected_thumbnail_path = vips_thumb_path(&blob.hash);
    assert_eq!(
        blob.thumbnail_path.as_deref(),
        Some(expected_thumbnail_path.as_str())
    );
    assert_eq!(blob.thumbnail_processor.as_deref(), Some("vips-cli"));
    assert_eq!(blob.thumbnail_version.as_deref(), Some("1"));

    let second = request_thumbnail!(app, token, file_id);
    assert_eq!(second.status(), 200);
    assert_eq!(
        second
            .headers()
            .get("Content-Type")
            .and_then(|value| value.to_str().ok()),
        Some("image/webp")
    );

    let logged_input_path = std::fs::read_to_string(&input_log_path).unwrap();
    assert!(
        logged_input_path.ends_with("/source.heic"),
        "expected fake vips input path to preserve the source extension, got {logged_input_path}"
    );
}

#[actix_web::test]
async fn test_thumbnail_mp4_uses_ffmpeg_cli_processor_when_extension_matches() {
    let Some(ffmpeg_command) = ffmpeg_command_for_tests() else {
        eprintln!("skipping ffmpeg_cli thumbnail test because ffmpeg is unavailable");
        return;
    };

    let state = common::setup().await;
    state.runtime_config.apply(common::system_config_model(
        "media_processing_registry_json",
        &json!({
            "version": 1,
            "processors": [
                {
                    "kind": "ffmpeg_cli",
                    "enabled": true,
                    "extensions": ["mp4"],
                    "config": {
                        "command": ffmpeg_command
                    }
                },
                {
                    "kind": "images",
                    "enabled": true
                }
            ]
        })
        .to_string(),
    ));

    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let file_id = upload_file_bytes!(app, token, "clip.mp4", "video/mp4", tiny_mp4());

    let first = request_thumbnail!(app, token, file_id);
    assert_eq!(first.status(), 202);

    aster_drive::services::task::drain(&state)
        .await
        .unwrap();

    let task =
        latest_thumbnail_task_for_processor(&state, file_id, MediaProcessorKind::FfmpegCli).await;
    assert_eq!(task.status, BackgroundTaskStatus::Succeeded);

    let blob = blob_for_file(&state, file_id).await;
    let expected_thumbnail_path = ffmpeg_thumb_path(&blob.hash);
    assert_eq!(
        blob.thumbnail_path.as_deref(),
        Some(expected_thumbnail_path.as_str())
    );
    assert_eq!(blob.thumbnail_processor.as_deref(), Some("ffmpeg-cli"));
    assert_eq!(blob.thumbnail_version.as_deref(), Some("1"));

    let second = request_thumbnail!(app, token, file_id);
    assert_eq!(second.status(), 200);
    assert_eq!(
        second
            .headers()
            .get("Content-Type")
            .and_then(|value| value.to_str().ok()),
        Some("image/webp")
    );

    let image = image::load_from_memory(&test::read_body(second).await).unwrap();
    assert_eq!(image.dimensions(), (16, 16));
}

#[actix_web::test]
async fn test_thumbnail_storage_native_processor_without_driver_capability_skips_to_images() {
    let state = common::setup().await;
    enable_default_policy_storage_native_thumbnail(&state).await;

    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let file_id = upload_file_bytes!(app, token, "test.png", "image/png", tiny_png());

    let first = request_thumbnail!(app, token, file_id);
    assert_eq!(first.status(), 202);

    aster_drive::services::task::drain(&state)
        .await
        .unwrap();

    let task = latest_thumbnail_task(&state, file_id).await;
    assert_eq!(task.status, BackgroundTaskStatus::Succeeded);
    assert_eq!(thumbnail_task_count(&state, file_id).await, 1);

    let second = request_thumbnail!(app, token, file_id);
    assert_eq!(second.status(), 200);
}
