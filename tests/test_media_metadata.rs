//! 集成测试：`media_metadata`。

#[macro_use]
mod common;

use actix_web::test;
use aster_drive::db::repository::{
    background_task_repo, config_repo, file_repo, media_metadata_repo,
};
use aster_drive::entities::{file, file_blob};
use aster_drive::types::{
    BackgroundTaskKind, BackgroundTaskStatus, FileCategory, SystemConfigSource,
    SystemConfigValueType,
};
use base64::Engine;
use sea_orm::{ActiveModelTrait, Set};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

fn tiny_png() -> Vec<u8> {
    let mut buf = std::io::Cursor::new(Vec::new());
    let encoder = image::codecs::png::PngEncoder::new(&mut buf);
    image::ImageEncoder::write_image(encoder, &[255, 0, 0], 1, 1, image::ExtendedColorType::Rgb8)
        .unwrap();
    buf.into_inner()
}

enum TiffValue<'a> {
    Ascii(&'a str),
    Short(u16),
    Rational(u32, u32),
    SRational(i32, i32),
}

fn push_u16_le(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn push_u32_le(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn push_i32_le(bytes: &mut Vec<u8>, value: i32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn tiny_jpeg_with_exif() -> Vec<u8> {
    let mut jpeg = Vec::new();
    let encoder = image::codecs::jpeg::JpegEncoder::new(&mut jpeg);
    image::ImageEncoder::write_image(encoder, &[255, 0, 0], 1, 1, image::ExtendedColorType::Rgb8)
        .unwrap();

    let mut entries = vec![
        (0x010f, TiffValue::Ascii("SONY")),
        (0x0110, TiffValue::Ascii("ILCE-7CM2")),
        (0x0112, TiffValue::Short(1)),
        (0x0131, TiffValue::Ascii("ILCE-7CM2 v1.01")),
        (0x013b, TiffValue::Ascii("Aaron Liu")),
        (0x829a, TiffValue::Rational(1, 200)),
        (0x829d, TiffValue::Rational(28, 10)),
        (0x8827, TiffValue::Short(100)),
        (0x9003, TiffValue::Ascii("2024:04:01 05:44:11")),
        (0x9204, TiffValue::SRational(0, 10)),
        (0x9209, TiffValue::Short(0)),
        (0x920a, TiffValue::Rational(28, 1)),
        (0xa405, TiffValue::Short(28)),
        (0xa433, TiffValue::Ascii("TAMRON")),
        (0xa434, TiffValue::Ascii("E 28-200mm F2.8-5.6 A071")),
    ];
    entries.sort_by_key(|(tag, _)| *tag);

    let mut tiff = Vec::new();
    tiff.extend_from_slice(b"II");
    push_u16_le(&mut tiff, 42);
    push_u32_le(&mut tiff, 8);
    push_u16_le(&mut tiff, entries.len().try_into().unwrap());

    let ifd_len = 2 + entries.len() * 12 + 4;
    let mut data = Vec::new();
    for (tag, value) in entries {
        push_u16_le(&mut tiff, tag);
        match value {
            TiffValue::Ascii(value) => {
                let mut ascii = value.as_bytes().to_vec();
                ascii.push(0);
                push_u16_le(&mut tiff, 2);
                push_u32_le(&mut tiff, ascii.len().try_into().unwrap());
                if ascii.len() <= 4 {
                    tiff.extend_from_slice(&ascii);
                    tiff.resize(tiff.len() + 4 - ascii.len(), 0);
                } else {
                    push_u32_le(&mut tiff, (8 + ifd_len + data.len()).try_into().unwrap());
                    data.extend_from_slice(&ascii);
                }
            }
            TiffValue::Short(value) => {
                push_u16_le(&mut tiff, 3);
                push_u32_le(&mut tiff, 1);
                push_u16_le(&mut tiff, value);
                push_u16_le(&mut tiff, 0);
            }
            TiffValue::Rational(numerator, denominator) => {
                push_u16_le(&mut tiff, 5);
                push_u32_le(&mut tiff, 1);
                push_u32_le(&mut tiff, (8 + ifd_len + data.len()).try_into().unwrap());
                push_u32_le(&mut data, numerator);
                push_u32_le(&mut data, denominator);
            }
            TiffValue::SRational(numerator, denominator) => {
                push_u16_le(&mut tiff, 10);
                push_u32_le(&mut tiff, 1);
                push_u32_le(&mut tiff, (8 + ifd_len + data.len()).try_into().unwrap());
                push_i32_le(&mut data, numerator);
                push_i32_le(&mut data, denominator);
            }
        }
    }
    push_u32_le(&mut tiff, 0);
    tiff.extend_from_slice(&data);

    let mut app1_payload = b"Exif\0\0".to_vec();
    app1_payload.extend_from_slice(&tiff);
    let app1_len: u16 = (app1_payload.len() + 2).try_into().unwrap();
    let mut result = Vec::with_capacity(jpeg.len() + app1_payload.len() + 4);
    result.extend_from_slice(&jpeg[..2]);
    result.extend_from_slice(&[0xff, 0xe1]);
    result.extend_from_slice(&app1_len.to_be_bytes());
    result.extend_from_slice(&app1_payload);
    result.extend_from_slice(&jpeg[2..]);
    result
}

fn tiny_mp4() -> Vec<u8> {
    base64::engine::general_purpose::STANDARD
        .decode("AAAAIGZ0eXBpc29tAAACAGlzb21pc28yYXZjMW1wNDEAAAN1bW9vdgAAAGxtdmhkAAAAAAAAAAAAAAAAAAAD6AAAAMgAAQAAAQAAAAAAAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAgAAAp90cmFrAAAAXHRraGQAAAADAAAAAAAAAAAAAAABAAAAAAAAAMgAAAAAAAAAAAAAAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAAAAABAAAAAABAAAAAQAAAAAAAkZWR0cwAAABxlbHN0AAAAAAAAAAEAAADIAAAEAAABAAAAAAIXbWRpYQAAACBtZGhkAAAAAAAAAAAAAAAAAAAyAAAACgBVxAAAAAAALWhkbHIAAAAAAAAAAHZpZGUAAAAAAAAAAAAAAABWaWRlb0hhbmRsZXIAAAABwm1pbmYAAAAUdm1oZAAAAAEAAAAAAAAAAAAAACRkaW5mAAAAHGRyZWYAAAAAAAAAAQAAAAx1cmwgAAAAAQAAAYJzdGJsAAAAvnN0c2QAAAAAAAAAAQAAAK5hdmMxAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAAAABAAEABIAAAASAAAAAAAAAABFUxhdmM2Mi4yOC4xMDAgbGlieDI2NAAAAAAAAAAAAAAAGP//AAAANGF2Y0MBZAAK/+EAF2dkAAqs2V7ARAAAAwAEAAADAMg8SJZYAQAGaOvjyyLA/fj4AAAAABBwYXNwAAAAAQAAAAEAAAAUYnRydAAAAAAAAHcQAAAAAAAAABhzdHRzAAAAAAAAAAEAAAAFAAACAAAAABRzdHNzAAAAAAAAAAEAAAABAAAAOGN0dHMAAAAAAAAABQAAAAEAAAQAAAAAAQAACgAAAAABAAAEAAAAAAEAAAAAAAAAAQAAAgAAAAAcc3RzYwAAAAAAAAABAAAAAQAAAAUAAAABAAAAKHN0c3oAAAAAAAAAAAAAAAUAAALKAAAADAAAAAwAAAAMAAAADAAAABRzdGNvAAAAAAAAAAEAAAOlAAAAYnVkdGEAAABabWV0YQAAAAAAAAAhaGRscgAAAAAAAAAAbWRpcmFwcGwAAAAAAAAAAAAAAAAtaWxzdAAAACWpdG9vAAAAHWRhdGEAAAABAAAAAExhdmY2Mi4xMi4xMDAAAAAIZnJlZQAAAwJtZGF0AAACrgYF//+q3EXpvebZSLeWLNgg2SPu73gyNjQgLSBjb3JlIDE2NSByMzIyMiBiMzU2MDVhIC0gSC4yNjQvTVBFRy00IEFWQyBjb2RlYyAtIENvcHlsZWZ0IDIwMDMtMjAyNSAtIGh0dHA6Ly93d3cudmlkZW9sYW4ub3JnL3gyNjQuaHRtbCAtIG9wdGlvbnM6IGNhYmFjPTEgcmVmPTMgZGVibG9jaz0xOjA6MCBhbmFseXNlPTB4MzoweDExMyBtZT1oZXggc3VibWU9NyBwc3k9MSBwc3lfcmQ9MS4wMDowLjAwIG1peGVkX3JlZj0xIG1lX3JhbmdlPTE2IGNocm9tYV9tZT0xIHRyZWxsaXM9MSA4eDhkY3Q9MSBjcW09MCBkZWFkem9uZT0yMSwxMSBmYXN0X3Bza2lwPTEgY2hyb21hX3FwX29mZnNldD0tMiB0aHJlYWRzPTEgbG9va2FoZWFkX3RocmVhZHM9MSBzbGljZWRfdGhyZWFkcz0wIG5yPTAgZGVjaW1hdGU9MSBpbnRlcmxhY2VkPTAgYmx1cmF5X2NvbXBhdD0wIGNvbnN0cmFpbmVkX2ludHJhPTAgYmZyYW1lcz0zIGJfcHlyYW1pZD0yIGJfYWRhcHQ9MSBiX2JpYXM9MCBkaXJlY3Q9MSB3ZWlnaHRiPTEgb3Blbl9nb3A9MCB3ZWlnaHRwPTIga2V5aW50PTI1MCBrZXlpbnRfbWluPTI1IHNjZW5lY3V0PTQwIGludHJhX3JlZnJlc2g9MCByY19sb29rYWhlYWQ9NDAgcmM9Y3JmIG1idHJlZT0xIGNyZj0yMy4wIHFjb21wPTAuNjAgcXBtaW49MCBxcG1heD02OSBxcHN0ZXA9NCBpcF9yYXRpbz0xLjQwIGFxPTE6MS4wMACAAAAAFGWIhAAz//7fMvgUzcWJzsyAXJ6XAAAACEGaJGxCv/7AAAAACEGeQniF/8GBAAAACAGeYXRCv8SAAAAACAGeY2pCv8SB")
        .expect("embedded tiny mp4 fixture should decode")
}

#[cfg(unix)]
fn write_fake_ffprobe_metadata_command() -> std::path::PathBuf {
    let dir =
        std::env::temp_dir().join(format!("aster-drive-ffprobe-meta-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("fake-ffprobe");
    std::fs::write(
        &path,
        "#!/bin/sh\ncat <<'JSON'\n{\"streams\":[{\"codec_type\":\"video\",\"codec_name\":\"h264\",\"profile\":\"High\",\"width\":32,\"height\":18,\"duration\":\"2.500000\",\"avg_frame_rate\":\"30000/1001\",\"bit_rate\":\"8400000\",\"pix_fmt\":\"yuv420p10le\",\"bits_per_raw_sample\":\"10\",\"color_space\":\"bt2020nc\",\"color_transfer\":\"smpte2084\",\"color_primaries\":\"bt2020\",\"side_data_list\":[{\"side_data_type\":\"Display Matrix\",\"rotation\":90}],\"tags\":{\"creation_time\":\"2024-04-01T05:44:11.000000Z\"}},{\"codec_type\":\"audio\",\"codec_name\":\"aac\",\"channels\":2,\"sample_rate\":\"48000\",\"bit_rate\":\"192000\"},{\"codec_type\":\"subtitle\",\"codec_name\":\"subrip\"}],\"format\":{\"format_name\":\"mov,mp4,m4a,3gp,3g2,mj2\",\"duration\":\"2.500000\",\"bit_rate\":\"9100000\",\"tags\":{\"creation_time\":\"2024-04-01T05:44:11.000000Z\"}}}\nJSON\n",
    )
    .unwrap();
    let mut permissions = std::fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&path, permissions).unwrap();
    path
}

async fn upload_file_bytes(
    app: &impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
    token: &str,
    filename: &str,
    content_type: &str,
    bytes: &[u8],
) -> i64 {
    let boundary = "----MediaMetadataBoundary";
    let mut payload = Vec::new();
    payload.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    payload.extend_from_slice(
        format!("Content-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\n")
            .as_bytes(),
    );
    payload.extend_from_slice(format!("Content-Type: {content_type}\r\n\r\n").as_bytes());
    payload.extend_from_slice(bytes);
    payload.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

    let req = test::TestRequest::post()
        .uri("/api/v1/files/upload")
        .insert_header(("Cookie", common::access_cookie_header(token)))
        .insert_header(common::csrf_header_for(token))
        .insert_header((
            "Content-Type",
            format!("multipart/form-data; boundary={boundary}"),
        ))
        .set_payload(payload)
        .to_request();
    let resp = test::call_service(app, req).await;
    assert_eq!(resp.status(), 201, "upload should return 201");
    let body: Value = test::read_body_json(resp).await;
    body["data"]["id"].as_i64().unwrap()
}

async fn request_media_metadata(
    app: &impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
    token: &str,
    file_id: i64,
) -> actix_web::dev::ServiceResponse {
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/files/{file_id}/media-metadata"))
        .insert_header(("Cookie", common::access_cookie_header(token)))
        .insert_header(common::csrf_header_for(token))
        .to_request();
    test::call_service(app, req).await
}

async fn duplicate_file_for_same_blob(
    state: &aster_drive::runtime::PrimaryAppState,
    source_file_id: i64,
    name: &str,
) -> i64 {
    let source = file_repo::find_by_id(&state.db, source_file_id)
        .await
        .unwrap();
    let now = chrono::Utc::now();
    file_repo::increment_blob_ref_count(&state.db, source.blob_id)
        .await
        .unwrap();
    file::ActiveModel {
        name: Set(name.to_string()),
        folder_id: Set(source.folder_id),
        team_id: Set(source.team_id),
        blob_id: Set(source.blob_id),
        size: Set(source.size),
        owner_user_id: Set(source.owner_user_id),
        created_by_user_id: Set(source.created_by_user_id),
        created_by_username: Set(source.created_by_username),
        mime_type: Set(source.mime_type),
        extension: Set(source.extension),
        compound_extension: Set(source.compound_extension),
        file_category: Set(source.file_category),
        created_at: Set(now),
        updated_at: Set(now),
        deleted_at: Set(None),
        is_locked: Set(false),
        ..Default::default()
    }
    .insert(&state.db)
    .await
    .unwrap()
    .id
}

async fn set_system_config(state: &aster_drive::runtime::PrimaryAppState, key: &str, value: &str) {
    config_repo::upsert(&state.db, key, value, 1).await.unwrap();
    let mut model = config_repo::find_by_key(&state.db, key)
        .await
        .unwrap()
        .unwrap();
    model.source = SystemConfigSource::System;
    model.value_type = if value == "true" || value == "false" {
        SystemConfigValueType::Boolean
    } else {
        SystemConfigValueType::String
    };
    state.runtime_config.apply(model);
}

async fn set_media_processing_registry(
    state: &aster_drive::runtime::PrimaryAppState,
    value: serde_json::Value,
) {
    set_system_config(
        state,
        "media_processing_registry_json",
        &serde_json::to_string_pretty(&value).unwrap(),
    )
    .await;
}

async fn insert_synthetic_media_file(
    state: &aster_drive::runtime::PrimaryAppState,
    name: &str,
    mime_type: &str,
    category: FileCategory,
    bytes: &[u8],
) -> i64 {
    let policy = aster_drive::db::repository::policy_repo::find_default(&state.db)
        .await
        .unwrap()
        .expect("default policy should exist");
    let driver = state.driver_registry.get_driver(&policy).unwrap();
    let hash = hex::encode(Sha256::digest(bytes));
    let storage_path = aster_drive::utils::storage_path_from_blob_key(&hash);
    driver.put(&storage_path, bytes).await.unwrap();
    let now = chrono::Utc::now();
    let blob = file_blob::ActiveModel {
        hash: Set(hash),
        size: Set(bytes.len() as i64),
        policy_id: Set(policy.id),
        storage_path: Set(storage_path),
        ref_count: Set(1),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&state.db)
    .await
    .unwrap();
    file::ActiveModel {
        name: Set(name.to_string()),
        folder_id: Set(None),
        team_id: Set(None),
        blob_id: Set(blob.id),
        size: Set(bytes.len() as i64),
        owner_user_id: Set(Some(1)),
        created_by_user_id: Set(Some(1)),
        created_by_username: Set("testuser".to_string()),
        mime_type: Set(mime_type.to_string()),
        extension: Set(String::new()),
        compound_extension: Set(None),
        file_category: Set(category),
        created_at: Set(now),
        updated_at: Set(now),
        deleted_at: Set(None),
        is_locked: Set(false),
        ..Default::default()
    }
    .insert(&state.db)
    .await
    .unwrap()
    .id
}

#[actix_web::test]
async fn file_media_metadata_extracts_image_and_reuses_blob_cache() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let file_id = upload_file_bytes(&app, &token, "cover.png", "image/png", &tiny_png()).await;

    let resp = request_media_metadata(&app, &token, file_id).await;
    assert_eq!(resp.status(), 202);
    assert_eq!(
        resp.headers()
            .get("Retry-After")
            .and_then(|value| value.to_str().ok()),
        Some("2")
    );

    let task = background_task_repo::list_recent(&state.db, 16)
        .await
        .unwrap()
        .into_iter()
        .find(|task| task.kind == BackgroundTaskKind::MediaMetadataExtract)
        .expect("media metadata task should be queued");
    assert_eq!(task.status, BackgroundTaskStatus::Pending);
    let payload: Value = serde_json::from_str(task.payload_json.as_ref()).unwrap();
    assert_eq!(payload["kind"], "image");
    assert_eq!(task.max_attempts, 3);

    let stats = aster_drive::services::task_service::drain(&state)
        .await
        .expect("task drain should succeed");
    assert_eq!(stats.succeeded, 1);

    let resp = request_media_metadata(&app, &token, file_id).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["kind"], "image");
    assert_eq!(body["data"]["status"], "ready");
    assert_eq!(body["data"]["metadata"]["kind"], "image");
    assert_eq!(body["data"]["metadata"]["width"], 1);
    assert_eq!(body["data"]["metadata"]["height"], 1);

    assert_eq!(
        background_task_repo::list_recent(&state.db, 16)
            .await
            .unwrap()
            .into_iter()
            .filter(|task| task.kind == BackgroundTaskKind::MediaMetadataExtract)
            .count(),
        1
    );

    let second_file_id = duplicate_file_for_same_blob(&state, file_id, "cover-copy.png").await;
    let resp = request_media_metadata(&app, &token, second_file_id).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["kind"], "image");
    assert_eq!(body["data"]["status"], "ready");
    assert_eq!(body["data"]["metadata"]["kind"], "image");

    let tasks = background_task_repo::list_recent(&state.db, 16)
        .await
        .unwrap();
    assert_eq!(
        tasks
            .iter()
            .filter(|task| task.kind == BackgroundTaskKind::MediaMetadataExtract)
            .count(),
        1
    );
    let payload: Value = serde_json::from_str(
        tasks
            .iter()
            .find(|task| task.kind == BackgroundTaskKind::MediaMetadataExtract)
            .expect("media metadata task should still exist")
            .payload_json
            .as_ref(),
    )
    .unwrap();
    assert_eq!(payload["kind"], "image");
    assert_eq!(stats.succeeded, 1);
}

#[actix_web::test]
async fn file_media_metadata_extracts_image_exif_fields() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let file_id = upload_file_bytes(
        &app,
        &token,
        "camera.jpg",
        "image/jpeg",
        &tiny_jpeg_with_exif(),
    )
    .await;

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/files/{file_id}/media-metadata"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 202);

    let stats = aster_drive::services::task_service::drain(&state)
        .await
        .expect("task drain should succeed");
    assert_eq!(stats.succeeded, 1);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/files/{file_id}/media-metadata"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let metadata = &body["data"]["metadata"];
    assert_eq!(metadata["kind"], "image");
    assert_eq!(metadata["width"], 1);
    assert_eq!(metadata["height"], 1);
    assert_eq!(metadata["camera_make"], "SONY");
    assert_eq!(metadata["camera_model"], "ILCE-7CM2");
    assert_eq!(metadata["lens_make"], "TAMRON");
    assert_eq!(metadata["lens_model"], "E 28-200mm F2.8-5.6 A071");
    assert_eq!(metadata["f_number"], 2.8);
    assert_eq!(metadata["exposure_time_seconds"], 0.005);
    assert_eq!(metadata["iso"], 100);
    assert_eq!(metadata["exposure_bias_ev"], 0.0);
    assert_eq!(metadata["flash_fired"], false);
    assert_eq!(metadata["flash_mode"], 0);
    assert_eq!(metadata["focal_length_mm"], 28.0);
    assert_eq!(metadata["focal_length_35mm"], 28);
    assert_eq!(metadata["taken_at"], "2024-04-01T05:44:11");
    assert_eq!(metadata["orientation"], 1);
    assert_eq!(metadata["artist"], "Aaron Liu");
    assert_eq!(metadata["software"], "ILCE-7CM2 v1.01");
}

#[actix_web::test]
async fn file_media_metadata_returns_unsupported_for_video_when_ffprobe_processor_is_disabled() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let file_id = insert_synthetic_media_file(
        &state,
        "clip.mp4",
        "video/mp4",
        FileCategory::Video,
        &tiny_mp4(),
    )
    .await;

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/files/{file_id}/media-metadata"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["kind"], "video");
    assert_eq!(body["data"]["status"], "unsupported");
    assert_eq!(body["data"]["parser"], "unsupported");
    assert!(body["data"]["metadata"].is_null());

    let file = file_repo::find_by_id(&state.db, file_id).await.unwrap();
    let record = media_metadata_repo::find_by_blob_id(&state.db, file.blob_id)
        .await
        .unwrap();
    assert!(record.is_none());
    assert_eq!(
        background_task_repo::list_recent(&state.db, 16)
            .await
            .unwrap()
            .into_iter()
            .filter(|task| task.kind == BackgroundTaskKind::MediaMetadataExtract)
            .count(),
        0
    );
}

#[actix_web::test]
async fn share_media_metadata_uses_same_pending_response_shape() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let file_id = upload_file_bytes(&app, &token, "shared.png", "image/png", &tiny_png()).await;

    let req = test::TestRequest::post()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(json!({
            "target": {
                "type": "file",
                "id": file_id
            }
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let share_token = body["data"]["token"].as_str().unwrap();

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/s/{share_token}/media-metadata"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 202);
    assert_eq!(
        resp.headers()
            .get("Retry-After")
            .and_then(|value| value.to_str().ok()),
        Some("2")
    );
}

#[actix_web::test]
async fn media_metadata_disabled_returns_unsupported_without_task() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let req = test::TestRequest::put()
        .uri("/api/v1/admin/config/media_metadata_enabled")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(json!({ "value": "false" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let file_id = upload_file_bytes(&app, &token, "disabled.png", "image/png", &tiny_png()).await;
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/files/{file_id}/media-metadata"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["kind"], "image");
    assert_eq!(body["data"]["status"], "unsupported");
    assert_eq!(body["data"]["parser"], "disabled");

    assert_eq!(
        background_task_repo::list_recent(&state.db, 16)
            .await
            .unwrap()
            .into_iter()
            .filter(|task| task.kind == BackgroundTaskKind::MediaMetadataExtract)
            .count(),
        0
    );
}

#[actix_web::test]
async fn media_metadata_processor_disabled_returns_unsupported_without_task() {
    let state = common::setup().await;
    set_media_processing_registry(
        &state,
        json!({
            "version": 2,
            "processors": [
                {
                    "kind": "images",
                    "enabled": false,
                    "uses": ["thumbnail:image", "metadata:image"]
                },
                {
                    "kind": "lofty",
                    "enabled": true,
                    "uses": ["thumbnail:audio", "metadata:audio"]
                }
            ]
        }),
    )
    .await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let file_id =
        upload_file_bytes(&app, &token, "disabled-kind.png", "image/png", &tiny_png()).await;

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/files/{file_id}/media-metadata"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["kind"], "image");
    assert_eq!(body["data"]["status"], "unsupported");
    assert_eq!(body["data"]["parser"], "unsupported");

    assert_eq!(
        background_task_repo::list_recent(&state.db, 16)
            .await
            .unwrap()
            .into_iter()
            .filter(|task| task.kind == BackgroundTaskKind::MediaMetadataExtract)
            .count(),
        0
    );
}

#[cfg(unix)]
#[actix_web::test]
async fn video_media_metadata_uses_configured_ffprobe_command() {
    let fake_ffprobe = write_fake_ffprobe_metadata_command();
    let state = common::setup().await;
    set_media_processing_registry(
        &state,
        json!({
            "version": 2,
            "processors": [
                {
                    "kind": "ffprobe_cli",
                    "enabled": true,
                    "uses": ["metadata:video"],
                    "extensions": ["mp4"],
                    "config": {
                        "command": fake_ffprobe.to_string_lossy()
                    }
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
        }),
    )
    .await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let file_id = insert_synthetic_media_file(
        &state,
        "configured-clip.mp4",
        "video/mp4",
        FileCategory::Video,
        &tiny_mp4(),
    )
    .await;

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/files/{file_id}/media-metadata"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 202);

    let stats = aster_drive::services::task_service::drain(&state)
        .await
        .expect("task drain should succeed");
    assert_eq!(stats.succeeded, 1);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/files/{file_id}/media-metadata"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["status"], "ready");
    assert_eq!(body["data"]["parser"], "ffprobe");
    assert_eq!(body["data"]["metadata"]["width"], 32);
    assert_eq!(body["data"]["metadata"]["height"], 18);
    assert_eq!(body["data"]["metadata"]["display_width"], 18);
    assert_eq!(body["data"]["metadata"]["display_height"], 32);
    assert_eq!(body["data"]["metadata"]["rotation_degrees"], 90);
    assert_eq!(body["data"]["metadata"]["duration_ms"], 2500);
    assert_eq!(body["data"]["metadata"]["frame_rate"], "30000/1001");
    assert_eq!(body["data"]["metadata"]["video_bitrate"], 8_400_000);
    assert_eq!(body["data"]["metadata"]["overall_bitrate"], 9_100_000);
    assert_eq!(body["data"]["metadata"]["pixel_format"], "yuv420p10le");
    assert_eq!(body["data"]["metadata"]["bit_depth"], 10);
    assert_eq!(body["data"]["metadata"]["color_space"], "bt2020nc");
    assert_eq!(body["data"]["metadata"]["color_transfer"], "smpte2084");
    assert_eq!(body["data"]["metadata"]["color_primaries"], "bt2020");
    assert_eq!(body["data"]["metadata"]["hdr_format"], "HDR10");
    assert_eq!(body["data"]["metadata"]["audio_codec"], "aac");
    assert_eq!(body["data"]["metadata"]["audio_channels"], 2);
    assert_eq!(body["data"]["metadata"]["audio_sample_rate"], 48_000);
    assert_eq!(body["data"]["metadata"]["audio_bitrate"], 192_000);
    assert_eq!(body["data"]["metadata"]["audio_stream_count"], 1);
    assert_eq!(body["data"]["metadata"]["subtitle_stream_count"], 1);
    assert_eq!(
        body["data"]["metadata"]["creation_time"],
        "2024-04-01T05:44:11.000000Z"
    );

    let _ = std::fs::remove_dir_all(
        fake_ffprobe
            .parent()
            .expect("fake ffprobe script should have a parent directory"),
    );
}
