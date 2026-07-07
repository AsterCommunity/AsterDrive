//! 集成测试：`admin`。

#[macro_use]
mod common;
use aster_drive::runtime::SharedRuntimeState;

use actix_web::{App, HttpResponse, HttpServer, test, web};
use chrono::{Duration, Utc};
use sea_orm::{ActiveModelTrait, Set};
use serde_json::Value;
use std::io::Cursor;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use aster_drive::db::repository::{
    audit_log_repo, background_task_repo, lock_repo, policy_repo, team_repo, user_repo,
};
use aster_drive::entities::{
    audit_log, background_task, file, file_blob, file_version, resource_lock,
};
use aster_drive::types::{
    AuditAction, BackgroundTaskKind, BackgroundTaskStatus, EntityType, StoredLockOwnerInfo,
    StoredTaskPayload, StoredTaskResult,
};

fn admin_get_request(token: &str, uri: &str) -> actix_web::test::TestRequest {
    let mut req = test::TestRequest::get().uri(uri);
    req = req.insert_header(("Cookie", common::access_cookie_header(token)));
    req.insert_header(common::csrf_header_for(token))
}

macro_rules! admin_get_json {
    ($app:expr, $token:expr, $uri:expr) => {{
        let req = admin_get_request(&$token, $uri).to_request();
        let resp = test::call_service(&$app, req).await;
        assert_eq!(resp.status(), 200, "GET {} should return 200", $uri);
        test::read_body_json(resp).await
    }};
}

fn json_string_values(items: &[Value], key: &str) -> Vec<String> {
    items
        .iter()
        .map(|item| {
            item[key]
                .as_str()
                .unwrap_or_else(|| panic!("{key} should be a string in {item}"))
                .to_string()
        })
        .collect()
}

fn json_i64_values(items: &[Value], key: &str) -> Vec<i64> {
    items
        .iter()
        .map(|item| {
            item[key]
                .as_i64()
                .unwrap_or_else(|| panic!("{key} should be an integer in {item}"))
        })
        .collect()
}

fn assert_generated_password_policy(generated_password: &str) {
    assert!(
        generated_password.len() >= 24,
        "generated password should be at least 24 characters"
    );
    assert!(
        generated_password.chars().any(|c| c.is_ascii_uppercase()),
        "generated password should include an uppercase letter"
    );
    assert!(
        generated_password.chars().any(|c| c.is_ascii_lowercase()),
        "generated password should include a lowercase letter"
    );
    assert!(
        generated_password.chars().any(|c| c.is_ascii_digit()),
        "generated password should include a digit"
    );
    assert!(
        generated_password
            .chars()
            .any(|c| c.is_ascii_graphic() && !c.is_ascii_alphanumeric()),
        "generated password should include a symbol"
    );
}

fn assert_blob_ref_health(
    blob: &Value,
    recorded_ref_count: i64,
    file_ref_count: i64,
    version_ref_count: i64,
    health: &str,
) {
    assert_eq!(blob["ref_count"], recorded_ref_count, "{blob}");
    assert_eq!(blob["file_ref_count"], file_ref_count, "{blob}");
    assert_eq!(blob["version_ref_count"], version_ref_count, "{blob}");
    assert_eq!(
        blob["actual_ref_count"],
        file_ref_count + version_ref_count,
        "{blob}"
    );
    assert_eq!(blob["health"], health, "{blob}");
}

fn avatar_upload_payload() -> (String, Vec<u8>) {
    let boundary = "----AsterAvatarBoundary".to_string();
    let image = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        8,
        8,
        image::Rgba([0, 160, 255, 255]),
    ));
    let mut png = Cursor::new(Vec::new());
    image.write_to(&mut png, image::ImageFormat::Png).unwrap();

    let mut body = Vec::new();
    body.extend_from_slice(
        format!(
            "--{boundary}\r\n\
             Content-Disposition: form-data; name=\"file\"; filename=\"avatar.png\"\r\n\
             Content-Type: image/png\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(&png.into_inner());
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    (boundary, body)
}

#[cfg(unix)]
fn write_fake_vips_command() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("aster-drive-vips-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("fake-vips");
    std::fs::write(
        &path,
        "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then\n  echo \"vips-8.16.0\"\n  exit 0\nfi\necho \"unexpected args: $@\" >&2\nexit 1\n",
    )
    .unwrap();
    let mut permissions = std::fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&path, permissions).unwrap();
    path
}

#[cfg(unix)]
fn write_fake_ffprobe_command() -> std::path::PathBuf {
    let dir =
        std::env::temp_dir().join(format!("aster-drive-ffprobe-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("fake-ffprobe");
    std::fs::write(
        &path,
        "#!/bin/sh\nif [ \"$1\" = \"-version\" ]; then\n  echo \"ffprobe version 7.1-test\"\n  exit 0\nfi\necho \"unexpected args: $@\" >&2\nexit 1\n",
    )
    .unwrap();
    let mut permissions = std::fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&path, permissions).unwrap();
    path
}

#[actix_web::test]
async fn test_admin_scope_requires_authentication() {
    let state = common::setup().await;
    let app = create_test_app!(state);

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/config/schema")
        .to_request();
    let err = test::try_call_service(&app, req).await.unwrap_err();
    let resp = err.error_response();
    assert_eq!(resp.status(), 401);
}

#[actix_web::test]
async fn test_admin_scope_rejects_non_admin_users() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (admin_token, _) = register_and_login!(app);
    admin_create_user!(
        app,
        admin_token,
        "plainadminscope",
        "plainadminscope@example.com",
        "password123"
    );
    let (token, _) = login_user!(app, "plainadminscope", "password123");

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/config/schema")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let err = test::try_call_service(&app, req).await.unwrap_err();
    let resp = err.error_response();
    assert_eq!(resp.status(), 403);
}

#[actix_web::test]
async fn test_admin_scope_allows_admin_users() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/config/schema")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert!(body["data"].is_array());
    let keys = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|item| item.get("key").and_then(Value::as_str))
        .collect::<Vec<_>>();
    assert!(keys.contains(&"auth_cookie_secure"));
    assert!(keys.contains(&"auth_allow_user_registration"));
    assert!(keys.contains(&"auth_register_activation_enabled"));
    assert!(keys.contains(&"auth_access_token_ttl_secs"));
    assert!(keys.contains(&"auth_refresh_token_ttl_secs"));
    assert!(keys.contains(&"mail_outbox_dispatch_interval_secs"));
    assert!(keys.contains(&"background_task_dispatch_interval_secs"));
    assert!(keys.contains(&"background_task_dispatch_idle_max_interval_secs"));
    assert!(keys.contains(&"background_task_max_concurrency"));
    assert!(keys.contains(&"background_task_archive_max_concurrency"));
    assert!(keys.contains(&"background_task_thumbnail_max_concurrency"));
    assert!(keys.contains(&"maintenance_cleanup_interval_secs"));
    assert!(keys.contains(&"blob_reconcile_interval_secs"));
    assert!(keys.contains(&"background_task_max_attempts"));
    assert!(keys.contains(&"team_member_list_max_limit"));
    assert!(keys.contains(&"task_list_max_limit"));
    assert!(keys.contains(&"avatar_max_upload_size_bytes"));
    assert!(keys.contains(&"archive_extract_max_staging_bytes"));
    assert!(keys.contains(&"archive_preview_enabled"));
    assert!(keys.contains(&"archive_preview_user_enabled"));
    assert!(keys.contains(&"archive_preview_share_enabled"));
    assert!(keys.contains(&"archive_preview_max_source_bytes"));
    assert!(keys.contains(&"archive_preview_max_entries"));
    assert!(keys.contains(&"archive_preview_max_manifest_bytes"));
    assert!(keys.contains(&"archive_preview_max_duration_secs"));
    assert!(keys.contains(&"archive_compress_enabled"));
    assert!(keys.contains(&"thumbnail_max_source_bytes"));
    assert!(keys.contains(&"thumbnail_max_dimension"));
    assert!(keys.contains(&"image_preview_max_dimension"));
    assert!(keys.contains(&"media_processing_registry_json"));
    assert!(keys.contains(&"media_metadata_enabled"));
    assert!(keys.contains(&"media_metadata_max_source_bytes"));
    assert!(!keys.contains(&"media_metadata_image_enabled"));
    assert!(!keys.contains(&"media_metadata_audio_enabled"));
    assert!(!keys.contains(&"media_metadata_video_enabled"));
    assert!(!keys.contains(&"media_metadata_ffprobe_command"));
    assert!(keys.contains(&"branding_title"));
    assert!(keys.contains(&"branding_description"));
    assert!(keys.contains(&"branding_favicon_url"));

    let auth_ttl = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["key"] == "auth_access_token_ttl_secs")
        .unwrap();
    assert_eq!(
        auth_ttl["label_i18n_key"],
        "settings_item_auth_access_token_ttl_secs_label"
    );
    assert_eq!(
        auth_ttl["description_i18n_key"],
        "settings_item_auth_access_token_ttl_secs_desc"
    );

    let register_toggle = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["key"] == "auth_allow_user_registration")
        .unwrap();
    assert_eq!(
        register_toggle["label_i18n_key"],
        "settings_item_auth_allow_user_registration_label"
    );
    assert_eq!(
        register_toggle["description_i18n_key"],
        "settings_item_auth_allow_user_registration_desc"
    );

    let task_attempts = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["key"] == "background_task_max_attempts")
        .unwrap();
    assert_eq!(
        task_attempts["label_i18n_key"],
        "settings_item_background_task_max_attempts_label"
    );
    assert_eq!(
        task_attempts["description_i18n_key"],
        "settings_item_background_task_max_attempts_desc"
    );
    assert_eq!(register_toggle["category"], "user.registration_and_login");

    let mail_smtp_host = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["key"] == "mail_smtp_host")
        .unwrap();
    assert!(
        mail_smtp_host["actions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|action| action["action"] == "send_test_email"
                && action["target_key"] == "mail"
                && action["presentation"]["category"] == "mail"
                && action["presentation"]["subcategory"] == "config")
    );

    let media_registry = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["key"] == "media_processing_registry_json")
        .unwrap();
    let media_actions = media_registry["actions"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|action| action["action"].as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        media_actions,
        ["test_vips_cli", "test_ffmpeg_cli", "test_ffprobe_cli"]
    );

    let offline_registry = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["key"] == "offline_download_engine_registry_json")
        .unwrap();
    assert!(
        offline_registry["actions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|action| action["action"] == "test_aria2_rpc"
                && action["draft_value_keys"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|key| key == "offline_download_aria2_rpc_secret"))
    );

    let register_activation_toggle = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["key"] == "auth_register_activation_enabled")
        .unwrap();
    assert_eq!(
        register_activation_toggle["label_i18n_key"],
        "settings_item_auth_register_activation_enabled_label"
    );
    assert_eq!(
        register_activation_toggle["description_i18n_key"],
        "settings_item_auth_register_activation_enabled_desc"
    );

    let passkey_login_toggle = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["key"] == "auth_passkey_login_enabled")
        .unwrap();
    assert_eq!(
        passkey_login_toggle["label_i18n_key"],
        "settings_item_auth_passkey_login_enabled_label"
    );
    assert_eq!(
        passkey_login_toggle["description_i18n_key"],
        "settings_item_auth_passkey_login_enabled_desc"
    );
    assert_eq!(
        passkey_login_toggle["category"],
        "user.registration_and_login"
    );

    let local_email_allowlist = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["key"] == "auth_local_email_allowlist")
        .unwrap();
    assert_eq!(
        local_email_allowlist["label_i18n_key"],
        "settings_item_auth_local_email_allowlist_label"
    );
    assert_eq!(
        local_email_allowlist["description_i18n_key"],
        "settings_item_auth_local_email_allowlist_desc"
    );
    assert_eq!(
        local_email_allowlist["category"],
        "user.registration_and_login"
    );

    let local_email_blocklist = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["key"] == "auth_local_email_blocklist")
        .unwrap();
    assert_eq!(
        local_email_blocklist["label_i18n_key"],
        "settings_item_auth_local_email_blocklist_label"
    );
    assert_eq!(
        local_email_blocklist["description_i18n_key"],
        "settings_item_auth_local_email_blocklist_desc"
    );
    assert_eq!(
        local_email_blocklist["category"],
        "user.registration_and_login"
    );

    let branding_title = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["key"] == "branding_title")
        .unwrap();
    assert_eq!(
        branding_title["label_i18n_key"],
        "settings_item_branding_title_label"
    );
    assert_eq!(
        branding_title["invalidates"],
        serde_json::json!(["frontend_config"])
    );

    let allow_registration = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["key"] == "auth_allow_user_registration")
        .unwrap();
    assert_eq!(
        allow_registration["invalidates"],
        serde_json::json!(["frontend_config"])
    );

    let preview_apps = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["key"] == "frontend_preview_apps_json")
        .unwrap();
    assert_eq!(
        preview_apps["invalidates"],
        serde_json::json!(["preview_apps"])
    );

    let media_processing = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["key"] == "media_processing_registry_json")
        .unwrap();
    assert_eq!(
        media_processing["invalidates"],
        serde_json::json!(["thumbnail_support", "media_data_support"])
    );

    let task_limit = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["key"] == "task_list_max_limit")
        .unwrap();
    assert_eq!(
        task_limit["label_i18n_key"],
        "settings_item_task_list_max_limit_label"
    );
    assert_eq!(task_limit["category"], "runtime.limits");

    let archive_preview_enabled = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["key"] == "archive_preview_enabled")
        .unwrap();
    assert_eq!(
        archive_preview_enabled["label_i18n_key"],
        "settings_item_archive_preview_enabled_label"
    );
    assert_eq!(
        archive_preview_enabled["category"],
        "file_processing.archive_preview"
    );

    let archive_compress_enabled = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["key"] == "archive_compress_enabled")
        .unwrap();
    assert_eq!(
        archive_compress_enabled["label_i18n_key"],
        "settings_item_archive_compress_enabled_label"
    );
    assert_eq!(
        archive_compress_enabled["description_i18n_key"],
        "settings_item_archive_compress_enabled_desc"
    );
    assert_eq!(
        archive_compress_enabled["category"],
        "file_processing.archive_build"
    );
    assert_eq!(archive_compress_enabled["value_type"], "boolean");
}

#[actix_web::test]
async fn test_admin_files_and_file_blobs_observability() {
    let state = common::setup().await;
    let default_policy = policy_repo::find_default(state.writer_db())
        .await
        .expect("default policy query should succeed")
        .expect("default policy should exist");
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    admin_create_user!(
        app,
        admin_token,
        "fileobserver",
        "fileobserver@example.com",
        "password123"
    );
    let (user_token, _) = login_user!(app, "fileobserver", "password123");

    let report_id = upload_test_file_named!(app, user_token, "admin-report.txt");
    let copy_id = upload_test_file_named!(app, user_token, "admin-copy.txt");
    let notes_id = upload_test_file_named!(app, user_token, "notes.txt");

    let files_body: Value = admin_get_json!(
        app,
        admin_token,
        "/api/v1/admin/files?sort_by=name&sort_order=asc&limit=2&offset=0"
    );
    let files = files_body["data"]["items"].as_array().unwrap();
    assert_eq!(files_body["data"]["limit"], 2);
    assert_eq!(files_body["data"]["offset"], 0);
    assert!(files_body["data"]["total"].as_u64().unwrap() >= 3);
    assert_eq!(files.len(), 2);
    let names = json_string_values(files, "name");
    assert_eq!(names, vec!["admin-copy.txt", "admin-report.txt"]);
    assert_eq!(files[0]["created_by"]["username"], "fileobserver");
    assert_eq!(
        files[0]["blob"]["policy_id"].as_i64().unwrap(),
        default_policy.id
    );

    let filtered_body: Value = admin_get_json!(
        app,
        admin_token,
        "/api/v1/admin/files?name=report&sort_by=id&sort_order=asc"
    );
    let filtered_files = filtered_body["data"]["items"].as_array().unwrap();
    assert_eq!(filtered_body["data"]["total"], 1);
    assert_eq!(filtered_files[0]["id"], report_id);
    assert_eq!(filtered_files[0]["created_by"]["username"], "fileobserver");

    let blob_id = filtered_files[0]["blob_id"].as_i64().unwrap();
    let copy_detail: Value =
        admin_get_json!(app, admin_token, &format!("/api/v1/admin/files/{copy_id}"));
    if copy_detail["data"]["blob_id"].as_i64().unwrap() != blob_id {
        file::ActiveModel {
            id: Set(copy_id),
            blob_id: Set(blob_id),
            updated_at: Set(Utc::now()),
            ..Default::default()
        }
        .update(state.writer_db())
        .await
        .expect("copy file should be repointed to report blob for reference coverage");
    }
    let by_blob_body: Value = admin_get_json!(
        app,
        admin_token,
        &format!("/api/v1/admin/files?blob_id={blob_id}&sort_by=id&sort_order=asc")
    );
    let by_blob_ids = json_i64_values(by_blob_body["data"]["items"].as_array().unwrap(), "id");
    assert_eq!(by_blob_ids, vec![report_id, copy_id]);

    let update_req = test::TestRequest::put()
        .uri(&format!("/api/v1/files/{report_id}/content"))
        .insert_header(("Cookie", common::access_cookie_header(&user_token)))
        .insert_header(common::csrf_header_for(&user_token))
        .insert_header(("Content-Type", "application/octet-stream"))
        .set_payload("updated admin report content")
        .to_request();
    let update_resp = test::call_service(&app, update_req).await;
    assert_eq!(update_resp.status(), 200);

    let detail_body: Value = admin_get_json!(
        app,
        admin_token,
        &format!("/api/v1/admin/files/{report_id}")
    );
    assert_eq!(detail_body["data"]["id"], report_id);
    assert_eq!(detail_body["data"]["name"], "admin-report.txt");
    assert_eq!(
        detail_body["data"]["created_by"]["username"],
        "fileobserver"
    );
    assert_eq!(detail_body["data"]["versions"].as_array().unwrap().len(), 1);
    assert_eq!(detail_body["data"]["versions"][0]["blob_id"], blob_id);
    assert_eq!(detail_body["data"]["versions"][0]["blob"]["id"], blob_id);

    let delete_req = test::TestRequest::delete()
        .uri(&format!("/api/v1/files/{notes_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&user_token)))
        .insert_header(common::csrf_header_for(&user_token))
        .to_request();
    let delete_resp = test::call_service(&app, delete_req).await;
    assert_eq!(delete_resp.status(), 200);
    let deleted_body: Value = admin_get_json!(
        app,
        admin_token,
        "/api/v1/admin/files?deleted=true&name=notes"
    );
    assert_eq!(deleted_body["data"]["total"], 1);
    assert_eq!(deleted_body["data"]["items"][0]["id"], notes_id);
    let live_body: Value = admin_get_json!(
        app,
        admin_token,
        "/api/v1/admin/files?deleted=false&name=notes"
    );
    assert_eq!(live_body["data"]["total"], 0);

    let content_hash_blob = file_blob::ActiveModel {
        hash: Set("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string()),
        size: Set(8),
        policy_id: Set(default_policy.id),
        storage_path: Set("content/hash.bin".to_string()),
        thumbnail_path: Set(None),
        thumbnail_processor: Set(None),
        thumbnail_version: Set(None),
        ref_count: Set(0),
        created_at: Set(Utc::now()),
        updated_at: Set(Utc::now()),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .expect("content hash blob should insert");
    let opaque_blob = file_blob::ActiveModel {
        hash: Set("remote-object-key".to_string()),
        size: Set(9),
        policy_id: Set(default_policy.id),
        storage_path: Set("opaque/path.bin".to_string()),
        thumbnail_path: Set(None),
        thumbnail_processor: Set(None),
        thumbnail_version: Set(None),
        ref_count: Set(0),
        created_at: Set(Utc::now()),
        updated_at: Set(Utc::now()),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .expect("opaque blob should insert");

    let blobs_body: Value = admin_get_json!(
        app,
        admin_token,
        &format!(
            "/api/v1/admin/file-blobs?policy_id={}&ref_count_min=0&size_min=0&sort_by=id&sort_order=asc&limit=100",
            default_policy.id
        )
    );
    let blobs = blobs_body["data"]["items"].as_array().unwrap();
    assert!(blobs.iter().any(|blob| blob["id"] == blob_id));
    let observed_blob = blobs.iter().find(|blob| blob["id"] == blob_id).unwrap();
    assert_eq!(observed_blob["uploader_count"], 1);
    assert_eq!(observed_blob["uploaders"][0]["username"], "fileobserver");
    let content_blob = blobs
        .iter()
        .find(|blob| blob["id"] == content_hash_blob.id)
        .unwrap();
    assert_eq!(content_blob["hash_kind"], "content_sha256");
    assert_blob_ref_health(content_blob, 0, 0, 0, "orphan");
    let opaque_item = blobs
        .iter()
        .find(|blob| blob["id"] == opaque_blob.id)
        .unwrap();
    assert_eq!(opaque_item["hash_kind"], "opaque");
    assert_blob_ref_health(opaque_item, 0, 0, 0, "orphan");

    let blob_filter_body: Value = admin_get_json!(
        app,
        admin_token,
        "/api/v1/admin/file-blobs?hash=remote-object&storage_path=opaque&ref_count_max=0"
    );
    assert_eq!(blob_filter_body["data"]["total"], 1);
    assert_eq!(blob_filter_body["data"]["items"][0]["id"], opaque_blob.id);

    let blob_detail_body: Value = admin_get_json!(
        app,
        admin_token,
        &format!("/api/v1/admin/file-blobs/{blob_id}")
    );
    assert_eq!(blob_detail_body["data"]["id"], blob_id);
    assert_blob_ref_health(&blob_detail_body["data"], 1, 1, 1, "ref_count_mismatch");
    let reference_file_ids =
        json_i64_values(blob_detail_body["data"]["files"].as_array().unwrap(), "id");
    assert_eq!(reference_file_ids, vec![copy_id]);
    assert_eq!(blob_detail_body["data"]["uploader_count"], 1);
    assert_eq!(
        blob_detail_body["data"]["uploaders"][0]["username"],
        "fileobserver"
    );
    assert_eq!(
        blob_detail_body["data"]["files"][0]["created_by"]["username"],
        "fileobserver"
    );
    let version_refs = blob_detail_body["data"]["file_versions"]
        .as_array()
        .unwrap();
    assert_eq!(version_refs.len(), 1);
    assert_eq!(version_refs[0]["file_id"], report_id);
    assert_eq!(version_refs[0]["version"], 1);
}

#[actix_web::test]
async fn test_admin_file_blob_health_states_and_reference_boundaries() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    let default_policy = policy_repo::find_default(state.reader_db())
        .await
        .unwrap()
        .unwrap();
    admin_create_user!(
        app,
        admin_token,
        "blobhealth",
        "blobhealth@example.com",
        "password123"
    );
    let (user_token, _) = login_user!(app, "blobhealth", "password123");

    let live_file_id = upload_test_file_named!(app, user_token, "blob-health-live.txt");
    let live_file_body: Value = admin_get_json!(
        app,
        admin_token,
        &format!("/api/v1/admin/files/{live_file_id}")
    );
    let live_blob_id = live_file_body["data"]["blob_id"].as_i64().unwrap();
    let live_blob_detail: Value = admin_get_json!(
        app,
        admin_token,
        &format!("/api/v1/admin/file-blobs/{live_blob_id}")
    );
    assert_blob_ref_health(&live_blob_detail["data"], 1, 1, 0, "healthy");

    let now = Utc::now();
    let version_only_blob = file_blob::ActiveModel {
        hash: Set("version-only-hash".to_string()),
        size: Set(33),
        policy_id: Set(default_policy.id),
        storage_path: Set("admin/version-only.bin".to_string()),
        thumbnail_path: Set(None),
        thumbnail_processor: Set(None),
        thumbnail_version: Set(None),
        ref_count: Set(1),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .expect("version-only blob should insert");
    file_version::ActiveModel {
        file_id: Set(live_file_id),
        blob_id: Set(version_only_blob.id),
        version: Set(99),
        size: Set(33),
        created_at: Set(now),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .expect("version-only ref should insert");
    let version_only_detail: Value = admin_get_json!(
        app,
        admin_token,
        &format!("/api/v1/admin/file-blobs/{}", version_only_blob.id)
    );
    assert_blob_ref_health(&version_only_detail["data"], 1, 0, 1, "healthy");
    assert_eq!(
        version_only_detail["data"]["file_versions"][0]["file_id"],
        live_file_id
    );

    let mismatch_blob = file_blob::ActiveModel {
        hash: Set("ref-mismatch-hash".to_string()),
        size: Set(44),
        policy_id: Set(default_policy.id),
        storage_path: Set("admin/ref-mismatch.bin".to_string()),
        thumbnail_path: Set(None),
        thumbnail_processor: Set(None),
        thumbnail_version: Set(None),
        ref_count: Set(7),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .expect("mismatch blob should insert");
    file::ActiveModel {
        id: Set(live_file_id),
        blob_id: Set(mismatch_blob.id),
        updated_at: Set(now),
        ..Default::default()
    }
    .update(state.writer_db())
    .await
    .expect("file should repoint to mismatch blob");
    let mismatch_detail: Value = admin_get_json!(
        app,
        admin_token,
        &format!("/api/v1/admin/file-blobs/{}", mismatch_blob.id)
    );
    assert_blob_ref_health(&mismatch_detail["data"], 7, 1, 0, "ref_count_mismatch");

    let cleanup_claimed_blob = file_blob::ActiveModel {
        hash: Set("cleanup-claimed-hash".to_string()),
        size: Set(55),
        policy_id: Set(default_policy.id),
        storage_path: Set("admin/cleanup-claimed.bin".to_string()),
        thumbnail_path: Set(None),
        thumbnail_processor: Set(None),
        thumbnail_version: Set(None),
        ref_count: Set(-1),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .expect("cleanup-claimed blob should insert");
    let cleanup_detail: Value = admin_get_json!(
        app,
        admin_token,
        &format!("/api/v1/admin/file-blobs/{}", cleanup_claimed_blob.id)
    );
    assert_blob_ref_health(&cleanup_detail["data"], -1, 0, 0, "cleanup_claimed");

    let orphan_blob = file_blob::ActiveModel {
        hash: Set("orphan-health-hash".to_string()),
        size: Set(66),
        policy_id: Set(default_policy.id),
        storage_path: Set("admin/orphan-health.bin".to_string()),
        thumbnail_path: Set(Some("thumbs/orphan-health.webp".to_string())),
        thumbnail_processor: Set(Some("test-processor".to_string())),
        thumbnail_version: Set(Some("test-version".to_string())),
        ref_count: Set(0),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .expect("orphan blob should insert");
    let orphan_detail: Value = admin_get_json!(
        app,
        admin_token,
        &format!("/api/v1/admin/file-blobs/{}", orphan_blob.id)
    );
    assert_blob_ref_health(&orphan_detail["data"], 0, 0, 0, "orphan");
    assert_eq!(
        orphan_detail["data"]["thumbnail_path"],
        "thumbs/orphan-health.webp"
    );
    assert_eq!(
        orphan_detail["data"]["thumbnail_processor"],
        "test-processor"
    );
    assert_eq!(orphan_detail["data"]["thumbnail_version"], "test-version");

    let list_body: Value = admin_get_json!(
        app,
        admin_token,
        &format!(
            "/api/v1/admin/file-blobs?policy_id={}&sort_by=id&sort_order=asc&limit=100",
            default_policy.id
        )
    );
    let blobs = list_body["data"]["items"].as_array().unwrap();
    let mismatch_item = blobs
        .iter()
        .find(|blob| blob["id"] == mismatch_blob.id)
        .unwrap();
    assert_blob_ref_health(mismatch_item, 7, 1, 0, "ref_count_mismatch");
    let cleanup_item = blobs
        .iter()
        .find(|blob| blob["id"] == cleanup_claimed_blob.id)
        .unwrap();
    assert_blob_ref_health(cleanup_item, -1, 0, 0, "cleanup_claimed");
    let orphan_item = blobs
        .iter()
        .find(|blob| blob["id"] == orphan_blob.id)
        .unwrap();
    assert_blob_ref_health(orphan_item, 0, 0, 0, "orphan");
}

#[actix_web::test]
async fn test_admin_create_blob_maintenance_task_audits_and_deduplicates_targets() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    admin_create_user!(
        app,
        admin_token,
        "blobmaint",
        "blobmaint@example.com",
        "password123"
    );
    let (user_token, _) = login_user!(app, "blobmaint", "password123");
    let file_id = upload_test_file_named!(app, user_token, "blob-maintenance.txt");
    let file = aster_drive::db::repository::file_repo::find_by_id(state.writer_db(), file_id)
        .await
        .expect("uploaded file should load");

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/file-blobs/maintenance")
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({
            "action": "integrity_check",
            "blob_ids": [file.blob_id, file.blob_id]
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["kind"], "blob_maintenance");
    assert_eq!(body["data"]["payload"]["kind"], "blob_maintenance");
    assert_eq!(body["data"]["payload"]["action"], "integrity_check");
    assert_eq!(
        body["data"]["payload"]["blob_ids"]
            .as_array()
            .expect("blob_ids should be an array")
            .len(),
        1
    );

    let audit_entries = audit_log_repo::find_with_filters(
        state.reader_db(),
        audit_log_repo::AuditLogQuery {
            user_id: None,
            action: Some(AuditAction::AdminCreateBlobMaintenanceTask.as_str()),
            entity_type: Some(aster_drive::types::AuditEntityType::Task.as_str()),
            entity_id: body["data"]["id"].as_i64(),
            after: None,
            before: None,
            limit: 10,
            offset: 0,
            sort_by: aster_drive::api::pagination::AdminAuditLogSortBy::CreatedAt,
            sort_order: aster_drive::api::pagination::SortOrder::Desc,
        },
    )
    .await
    .expect("audit query should succeed")
    .0;
    assert_eq!(audit_entries.len(), 1);
    assert!(
        audit_entries[0]
            .details
            .as_deref()
            .unwrap_or_default()
            .contains("\"integrity_check\"")
    );
}

#[actix_web::test]
async fn test_admin_create_blob_maintenance_task_without_targets_scans_all_blobs() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/file-blobs/maintenance")
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({
            "action": "ref_count_reconcile"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["kind"], "blob_maintenance");
    assert_eq!(body["data"]["payload"]["action"], "ref_count_reconcile");
    assert!(
        body["data"]["payload"].get("blob_ids").is_none()
            || body["data"]["payload"]["blob_ids"].is_null(),
        "omitted blob_ids should represent full blob maintenance scope"
    );
    assert_eq!(
        body["data"]["display_name"],
        "Reconcile references for all blobs"
    );
}

#[actix_web::test]
async fn test_admin_blob_maintenance_task_rejects_invalid_targets_and_permissions() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    admin_create_user!(
        app,
        admin_token,
        "blobmaintplain",
        "blobmaintplain@example.com",
        "password123"
    );
    let (plain_token, _) = login_user!(app, "blobmaintplain", "password123");

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/file-blobs/maintenance")
        .insert_header(("Cookie", common::access_cookie_header(&plain_token)))
        .insert_header(common::csrf_header_for(&plain_token))
        .set_json(serde_json::json!({
            "action": "integrity_check",
            "blob_ids": [1]
        }))
        .to_request();
    let err = test::try_call_service(&app, req).await.unwrap_err();
    assert_eq!(err.error_response().status(), 403);

    for (payload, expected_status) in [
        (
            serde_json::json!({"action":"integrity_check","blob_ids":[]}),
            400,
        ),
        (
            serde_json::json!({"action":"integrity_check","blob_ids":[-1]}),
            400,
        ),
        (
            serde_json::json!({"action":"integrity_check","blob_ids":[999999]}),
            404,
        ),
    ] {
        let req = test::TestRequest::post()
            .uri("/api/v1/admin/file-blobs/maintenance")
            .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
            .insert_header(common::csrf_header_for(&admin_token))
            .set_json(payload)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), expected_status);
    }
}

#[actix_web::test]
async fn test_admin_files_endpoints_reject_unauthorized_and_non_admin() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (admin_token, _) = register_and_login!(app);
    admin_create_user!(
        app,
        admin_token,
        "fileviewer",
        "fileviewer@example.com",
        "password123"
    );
    let (user_token, _) = login_user!(app, "fileviewer", "password123");

    for uri in [
        "/api/v1/admin/files",
        "/api/v1/admin/files/1",
        "/api/v1/admin/file-blobs",
        "/api/v1/admin/file-blobs/1",
    ] {
        let req = test::TestRequest::get().uri(uri).to_request();
        let err = test::try_call_service(&app, req).await.unwrap_err();
        assert_eq!(err.error_response().status(), 401, "{uri}");

        let req = test::TestRequest::get()
            .uri(uri)
            .insert_header(("Cookie", common::access_cookie_header(&user_token)))
            .insert_header(common::csrf_header_for(&user_token))
            .to_request();
        let err = test::try_call_service(&app, req).await.unwrap_err();
        assert_eq!(err.error_response().status(), 403, "{uri}");
    }
}

#[actix_web::test]
async fn test_admin_files_and_blobs_not_found() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (admin_token, _) = register_and_login!(app);

    for uri in [
        "/api/v1/admin/files/999999",
        "/api/v1/admin/file-blobs/999999",
    ] {
        let req = admin_get_request(&admin_token, uri).to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 404, "{uri}");
    }
}

#[actix_web::test]
async fn test_admin_template_variables_returns_mail_template_metadata() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/config/template-variables")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let groups = body["data"].as_array().unwrap();

    let password_reset = groups
        .iter()
        .find(|item| item["template_code"] == "password_reset")
        .unwrap();
    assert_eq!(password_reset["category"], "mail.template");
    assert_eq!(
        password_reset["label_i18n_key"],
        "settings_mail_template_group_password_reset"
    );

    let variables = password_reset["variables"].as_array().unwrap();
    assert!(variables.iter().any(|item| item["token"] == "{{username}}"));
    assert!(
        variables
            .iter()
            .any(|item| item["token"] == "{{reset_url}}")
    );
}

#[actix_web::test]
async fn test_admin_locks() {
    let state = common::setup().await;
    let app = create_test_app!(state);

    // 第一个用户自动成为 admin
    let (token, _) = register_and_login!(app);

    // 列出锁（应为空）
    let req = test::TestRequest::get()
        .uri("/api/v1/admin/locks")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["items"].as_array().unwrap().len(), 0);
    assert_eq!(body["data"]["total"], 0);

    // 清理过期锁
    let req = test::TestRequest::delete()
        .uri("/api/v1/admin/locks/expired")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["removed"], 0);
}

#[actix_web::test]
async fn test_admin_users() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    // 再注册两个普通用户
    for (username, email) in [
        ("user2", "user2@example.com"),
        ("user3", "user3@example.com"),
    ] {
        let req = test::TestRequest::post()
            .uri("/api/v1/auth/register")
            .peer_addr("127.0.0.1:12345".parse().unwrap())
            .set_json(serde_json::json!({
                "username": username,
                "email": email,
                "password": "password123"
            }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201);
    }

    // 分页列出用户
    let req = test::TestRequest::get()
        .uri("/api/v1/admin/users?limit=2&offset=0")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let data = &body["data"];
    let users = data["items"].as_array().unwrap();
    assert_eq!(data["limit"], 2);
    assert_eq!(data["offset"], 0);
    assert_eq!(data["total"], 3);
    assert_eq!(users.len(), 2);
    assert_eq!(users[0]["username"], "user3");
    assert_eq!(users[1]["username"], "user2");
    assert_eq!(users[0]["profile"]["avatar"]["source"], "none");
}

#[actix_web::test]
async fn test_admin_team_crud() {
    let state = common::setup().await;
    let default_group_id = state
        .policy_snapshot
        .system_default_policy_group()
        .expect("default policy group should exist")
        .id;
    let default_policy_id =
        aster_drive::db::repository::policy_repo::find_default(state.writer_db())
            .await
            .unwrap()
            .expect("default policy should exist")
            .id;
    let alternate_group_id = aster_drive::services::storage_policy::policy::create_group(
        &state,
        aster_drive::services::storage_policy::policy::CreateStoragePolicyGroupInput {
            name: "Operations Archive".to_string(),
            description: Some("Secondary team routing".to_string()),
            is_enabled: true,
            is_default: false,
            items: vec![
                aster_drive::services::storage_policy::policy::StoragePolicyGroupItemInput {
                    policy_id: default_policy_id,
                    priority: 1,
                    min_file_size: 0,
                    max_file_size: 0,
                },
            ],
        },
    )
    .await
    .unwrap()
    .id;
    let app = create_test_app!(state);
    let (admin_token, _) = register_and_login!(app);

    admin_create_user!(
        app,
        admin_token,
        "team-admin",
        "team-admin@example.com",
        "password123"
    );
    let (team_admin_token, _) = login_user!(app, "team-admin", "password123");

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/teams")
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({
            "name": "Operations",
            "description": "Shared operations workspace",
            "admin_identifier": "team-admin",
            "storage_quota": 536870912,
            "policy_group_id": default_group_id
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "success", "{body}");
    let team = &body["data"];
    let team_id = team["id"].as_i64().unwrap();
    assert_eq!(team["name"], "Operations");
    assert_eq!(team["created_by"]["username"], "testuser");
    assert_eq!(team["member_count"], 1);
    assert_eq!(team["storage_quota"], 536870912);
    assert_eq!(team["policy_group_id"], default_group_id);

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/teams?keyword=Operations")
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["total"], 1);
    assert_eq!(body["data"]["items"][0]["id"], team_id);

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/teams?keyword=erat")
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["total"], 1);
    assert_eq!(body["data"]["items"][0]["id"], team_id);

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/teams?keyword=op")
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["total"], 1);
    assert_eq!(body["data"]["items"][0]["id"], team_id);

    let req = test::TestRequest::get()
        .uri("/api/v1/teams")
        .insert_header(("Cookie", common::access_cookie_header(&team_admin_token)))
        .insert_header(common::csrf_header_for(&team_admin_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"][0]["id"], team_id);
    assert_eq!(body["data"][0]["my_role"], "admin");

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/admin/teams/{team_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({
            "name": "Operations Core",
            "description": "Updated by admin",
            "storage_quota": 1073741824,
            "policy_group_id": alternate_group_id
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["name"], "Operations Core");
    assert_eq!(body["data"]["description"], "Updated by admin");
    assert_eq!(body["data"]["storage_quota"], 1073741824);
    assert_eq!(body["data"]["policy_group_id"], alternate_group_id);

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/admin/teams/{team_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({
            "storage_quota": 0
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["storage_quota"], 0);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/register")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "username": "team-analyst",
            "email": "team-analyst@example.com",
            "password": "password123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/admin/teams/{team_id}/members?limit=1"))
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["total"], 1);
    assert_eq!(body["data"]["owner_count"], 0);
    assert_eq!(body["data"]["manager_count"], 1);
    assert_eq!(body["data"]["items"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"]["items"][0]["user"]["username"], "team-admin");
    assert_eq!(body["data"]["items"][0]["role"], "admin");

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/admin/teams/{team_id}/members"))
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({
            "identifier": "team-analyst",
            "role": "member"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let analyst_id = body["data"]["user_id"].as_i64().unwrap();
    assert_eq!(body["data"]["user"]["username"], "team-analyst");
    assert_eq!(body["data"]["role"], "member");

    let req = test::TestRequest::patch()
        .uri(&format!(
            "/api/v1/admin/teams/{team_id}/members/{analyst_id}"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({
            "role": "admin"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["role"], "admin");

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/admin/teams/{team_id}/members?keyword=naly"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["total"], 1);
    assert_eq!(body["data"]["items"][0]["user"]["username"], "team-analyst");

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/admin/teams/{team_id}/members?keyword=ly"))
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["total"], 1);
    assert_eq!(body["data"]["items"][0]["user"]["username"], "team-analyst");

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/admin/teams/{team_id}/members?role=admin&status=active&limit=1&offset=1"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["total"], 2);
    assert_eq!(body["data"]["limit"], 1);
    assert_eq!(body["data"]["offset"], 1);
    assert_eq!(body["data"]["owner_count"], 0);
    assert_eq!(body["data"]["manager_count"], 2);
    assert_eq!(body["data"]["items"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"]["items"][0]["user"]["username"], "team-analyst");

    let req = test::TestRequest::delete()
        .uri(&format!(
            "/api/v1/admin/teams/{team_id}/members/{analyst_id}"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/admin/teams/{team_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/admin/teams/{team_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert!(body["data"]["archived_at"].is_string());

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/admin/teams/{team_id}/members"))
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["items"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"]["items"][0]["user"]["username"], "team-admin");

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/teams")
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["total"], 0);

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/teams?archived=true")
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["total"], 1);
    assert_eq!(body["data"]["items"][0]["id"], team_id);

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/admin/teams/{team_id}/restore"))
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["id"], team_id);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/audit-logs"))
        .insert_header(("Cookie", common::access_cookie_header(&team_admin_token)))
        .insert_header(common::csrf_header_for(&team_admin_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let user_audit_items = body["data"]["items"].as_array().unwrap();
    let user_actions: Vec<&str> = user_audit_items
        .iter()
        .filter_map(|entry| entry["action"].as_str())
        .collect();
    assert!(user_actions.contains(&"admin_create_team"));
    assert!(user_actions.contains(&"admin_update_team"));
    assert!(user_actions.contains(&"team_member_add"));
    assert!(user_actions.contains(&"team_member_update"));
    assert!(user_actions.contains(&"team_member_remove"));
    assert!(user_actions.contains(&"admin_archive_team"));
    assert!(user_actions.contains(&"admin_restore_team"));
    assert!(
        user_audit_items
            .iter()
            .all(|entry| entry.get("ip_address").is_none())
    );
    assert!(
        user_audit_items
            .iter()
            .all(|entry| entry.get("details").is_none())
    );

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/admin/audit-logs?entity_type=team&entity_id={team_id}"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let admin_actions: Vec<&str> = body["data"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|entry| entry["action"].as_str())
        .collect();
    assert!(admin_actions.contains(&"admin_create_team"));
    assert!(admin_actions.contains(&"admin_update_team"));
    assert!(admin_actions.contains(&"team_member_add"));
    assert!(admin_actions.contains(&"team_member_update"));
    assert!(admin_actions.contains(&"team_member_remove"));
    assert!(admin_actions.contains(&"admin_archive_team"));
    assert!(admin_actions.contains(&"admin_restore_team"));

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/admin/teams/{team_id}/audit-logs"))
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let admin_team_audit_items = body["data"]["items"].as_array().unwrap();
    assert!(
        admin_team_audit_items
            .iter()
            .all(|entry| entry.get("ip_address").is_none())
    );
    assert!(
        admin_team_audit_items
            .iter()
            .all(|entry| entry.get("details").is_none())
    );

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/teams")
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["total"], 1);
}

#[actix_web::test]
async fn test_admin_teams_are_sorted_by_created_at_desc() {
    let state = common::setup().await;
    let default_group_id = state
        .policy_snapshot
        .system_default_policy_group()
        .expect("default policy group should exist")
        .id;
    let app = create_test_app!(state);
    let (admin_token, _) = register_and_login!(app);

    admin_create_user!(
        app,
        admin_token,
        "team-sort-admin",
        "team-sort-admin@example.com",
        "password123"
    );

    for team_name in ["First Team", "Second Team"] {
        let req = test::TestRequest::post()
            .uri("/api/v1/admin/teams")
            .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
            .insert_header(common::csrf_header_for(&admin_token))
            .set_json(serde_json::json!({
                "name": team_name,
                "admin_identifier": "team-sort-admin",
                "policy_group_id": default_group_id
            }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201);
    }

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/teams?limit=2&offset=0")
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let teams = body["data"]["items"].as_array().unwrap();
    assert_eq!(body["data"]["total"], 2);
    assert_eq!(teams.len(), 2);
    assert_eq!(teams[0]["name"], "Second Team");
    assert_eq!(teams[1]["name"], "First Team");
}

#[actix_web::test]
async fn test_admin_users_support_explicit_sorting() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    for (username, email) in [
        ("sort-user-gamma", "sort-user-gamma@example.com"),
        ("sort-user-alpha", "sort-user-alpha@example.com"),
        ("sort-user-beta", "sort-user-beta@example.com"),
    ] {
        admin_create_user!(app, token, username, email, "password123");
    }

    let body: Value = admin_get_json!(
        app,
        token,
        "/api/v1/admin/users?keyword=sort-user&sort_by=username&sort_order=asc&limit=10"
    );
    let users = body["data"]["items"].as_array().unwrap();
    assert_eq!(
        json_string_values(users, "username"),
        vec!["sort-user-alpha", "sort-user-beta", "sort-user-gamma"]
    );

    let body: Value = admin_get_json!(
        app,
        token,
        "/api/v1/admin/users?keyword=sort-user&sort_by=email&sort_order=desc&limit=10"
    );
    let users = body["data"]["items"].as_array().unwrap();
    assert_eq!(
        json_string_values(users, "email"),
        vec![
            "sort-user-gamma@example.com",
            "sort-user-beta@example.com",
            "sort-user-alpha@example.com"
        ]
    );
}

#[actix_web::test]
async fn test_admin_sort_query_rejects_unknown_values() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = admin_get_request(&token, "/api/v1/admin/users?sort_by=password_hash").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);

    let req = admin_get_request(&token, "/api/v1/admin/users?sort_order=random").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
}

#[actix_web::test]
async fn test_admin_teams_support_explicit_sorting() {
    let state = common::setup().await;
    let default_group_id = state
        .policy_snapshot
        .system_default_policy_group()
        .expect("default policy group should exist")
        .id;
    let app = create_test_app!(state);
    let (admin_token, _) = register_and_login!(app);

    admin_create_user!(
        app,
        admin_token,
        "tnsortadmin",
        "tnsortadmin@example.com",
        "password123"
    );

    for team_name in ["Team Sort Gamma", "Team Sort Alpha", "Team Sort Beta"] {
        let req = test::TestRequest::post()
            .uri("/api/v1/admin/teams")
            .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
            .insert_header(common::csrf_header_for(&admin_token))
            .set_json(serde_json::json!({
                "name": team_name,
                "admin_identifier": "tnsortadmin",
                "policy_group_id": default_group_id
            }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201);
    }

    let body: Value = admin_get_json!(
        app,
        admin_token,
        "/api/v1/admin/teams?sort_by=name&sort_order=asc&limit=10"
    );
    let teams = body["data"]["items"].as_array().unwrap();
    assert_eq!(
        json_string_values(teams, "name"),
        vec!["Team Sort Alpha", "Team Sort Beta", "Team Sort Gamma"]
    );

    let body: Value = admin_get_json!(
        app,
        admin_token,
        "/api/v1/admin/teams?sort_by=name&sort_order=desc&limit=10"
    );
    let teams = body["data"]["items"].as_array().unwrap();
    assert_eq!(
        json_string_values(teams, "name"),
        vec!["Team Sort Gamma", "Team Sort Beta", "Team Sort Alpha"]
    );
}

#[actix_web::test]
async fn test_admin_team_quota_rejects_negative_values() {
    let state = common::setup().await;
    let default_group_id = state
        .policy_snapshot
        .system_default_policy_group()
        .expect("default policy group should exist")
        .id;
    let app = create_test_app!(state);
    let (admin_token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/teams")
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({
            "name": "Negative Quota",
            "admin_identifier": "testuser",
            "storage_quota": -1,
            "policy_group_id": default_group_id
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["msg"], "storage_quota must be non-negative");

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/teams")
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({
            "name": "Quota Patch Target",
            "admin_identifier": "testuser",
            "storage_quota": 0,
            "policy_group_id": default_group_id
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let team_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/admin/teams/{team_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({
            "storage_quota": -1
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["msg"], "storage_quota must be non-negative");
}

#[actix_web::test]
async fn test_admin_team_quota_uses_default_when_omitted_and_allows_zero() {
    let state = common::setup().await;
    let default_group_id = state
        .policy_snapshot
        .system_default_policy_group()
        .expect("default policy group should exist")
        .id;
    let mut default_quota = aster_drive::db::repository::config_repo::find_by_key(
        state.writer_db(),
        "default_storage_quota",
    )
    .await
    .unwrap()
    .expect("default_storage_quota should exist");
    default_quota.value = "1048576".to_string();
    state.runtime_config.apply(default_quota);

    let app = create_test_app!(state);
    let (admin_token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/teams")
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({
            "name": "Default Quota Team",
            "admin_identifier": "testuser",
            "policy_group_id": default_group_id
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let team_id = body["data"]["id"].as_i64().unwrap();
    assert_eq!(body["data"]["storage_quota"], 1048576);

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/admin/teams/{team_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({
            "storage_quota": 0
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["storage_quota"], 0);
}

#[actix_web::test]
async fn test_admin_team_members_support_explicit_sorting() {
    let state = common::setup().await;
    let default_group_id = state
        .policy_snapshot
        .system_default_policy_group()
        .expect("default policy group should exist")
        .id;
    let app = create_test_app!(state);
    let (admin_token, _) = register_and_login!(app);

    admin_create_user!(
        app,
        admin_token,
        "tmmanager",
        "tmmanager@example.com",
        "password123"
    );
    admin_create_user!(
        app,
        admin_token,
        "tmalpha",
        "tmalpha@example.com",
        "password123"
    );
    admin_create_user!(
        app,
        admin_token,
        "tmzeta",
        "tmzeta@example.com",
        "password123"
    );

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/teams")
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({
            "name": "Team Member Sort",
            "admin_identifier": "tmmanager",
            "policy_group_id": default_group_id
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let team_id = body["data"]["id"].as_i64().unwrap();

    for (identifier, role) in [("tmzeta", "member"), ("tmalpha", "admin")] {
        let req = test::TestRequest::post()
            .uri(&format!("/api/v1/admin/teams/{team_id}/members"))
            .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
            .insert_header(common::csrf_header_for(&admin_token))
            .set_json(serde_json::json!({
                "identifier": identifier,
                "role": role
            }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201);
    }

    let body: Value = admin_get_json!(
        app,
        admin_token,
        &format!("/api/v1/admin/teams/{team_id}/members?sort_by=username&sort_order=asc&limit=10")
    );
    let members = body["data"]["items"].as_array().unwrap();
    let usernames = members
        .iter()
        .map(|item| item["user"]["username"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(usernames, vec!["tmalpha", "tmmanager", "tmzeta"]);

    let body: Value = admin_get_json!(
        app,
        admin_token,
        &format!("/api/v1/admin/teams/{team_id}/members?sort_by=role&sort_order=desc&limit=10")
    );
    let members = body["data"]["items"].as_array().unwrap();
    let roles = members
        .iter()
        .map(|item| item["role"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(roles, vec!["member", "admin", "admin"]);
}

#[actix_web::test]
async fn test_admin_policies_support_explicit_sorting() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    for (name, bucket) in [
        ("Policy Sort Zeta", "zeta-bucket"),
        ("Policy Sort Alpha", "alpha-bucket"),
    ] {
        let req = test::TestRequest::post()
            .uri("/api/v1/admin/policies")
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .set_json(serde_json::json!({
                "name": name,
                "driver_type": "s3",
                "endpoint": "https://s3.example.com",
                "bucket": bucket,
                "access_key": "ak",
                "secret_key": "sk",
                "base_path": "sort-tests"
            }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201);
    }

    let body: Value = admin_get_json!(
        app,
        token,
        "/api/v1/admin/policies?sort_by=name&sort_order=asc&limit=10"
    );
    let policies = body["data"]["items"].as_array().unwrap();
    let names = json_string_values(policies, "name");
    assert_eq!(names[0], "Policy Sort Alpha");
    assert_eq!(names[1], "Policy Sort Zeta");

    let body: Value = admin_get_json!(
        app,
        token,
        "/api/v1/admin/policies?sort_by=bucket&sort_order=desc&limit=10"
    );
    let policies = body["data"]["items"].as_array().unwrap();
    let buckets = json_string_values(policies, "bucket");
    assert!(
        buckets
            .iter()
            .position(|bucket| bucket == "zeta-bucket")
            .unwrap()
            < buckets
                .iter()
                .position(|bucket| bucket == "alpha-bucket")
                .unwrap()
    );
}

#[actix_web::test]
async fn test_admin_policy_groups_support_explicit_sorting() {
    let state = common::setup().await;
    let default_policy_id = policy_repo::find_default(state.writer_db())
        .await
        .unwrap()
        .expect("default policy should exist")
        .id;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    for (name, enabled) in [
        ("Policy Group Sort Zeta", true),
        ("Policy Group Sort Alpha", false),
    ] {
        let req = test::TestRequest::post()
            .uri("/api/v1/admin/policy-groups")
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .set_json(serde_json::json!({
                "name": name,
                "description": "sort regression",
                "is_enabled": enabled,
                "is_default": false,
                "items": [{
                    "policy_id": default_policy_id,
                    "priority": 1,
                    "min_file_size": 0,
                    "max_file_size": 0
                }]
            }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201);
    }

    let body: Value = admin_get_json!(
        app,
        token,
        "/api/v1/admin/policy-groups?sort_by=name&sort_order=asc&limit=10"
    );
    let groups = body["data"]["items"].as_array().unwrap();
    let names = json_string_values(groups, "name");
    assert_eq!(names[0], "Default Policy Group");
    assert_eq!(names[1], "Policy Group Sort Alpha");
    assert_eq!(names[2], "Policy Group Sort Zeta");

    let body: Value = admin_get_json!(
        app,
        token,
        "/api/v1/admin/policy-groups?sort_by=is_enabled&sort_order=asc&limit=10"
    );
    let groups = body["data"]["items"].as_array().unwrap();
    assert_eq!(groups[0]["name"], "Policy Group Sort Alpha");
    assert_eq!(groups[0]["is_enabled"], false);
}

#[actix_web::test]
async fn test_admin_policy_group_migration_updates_users_and_teams() {
    let state = common::setup().await;
    let default_policy_id = policy_repo::find_default(state.writer_db())
        .await
        .unwrap()
        .expect("default policy should exist")
        .id;
    let source_group = aster_drive::services::storage_policy::policy::create_group(
        &state,
        aster_drive::services::storage_policy::policy::CreateStoragePolicyGroupInput {
            name: "Migration Source Group".to_string(),
            description: Some("source assignments".to_string()),
            is_enabled: true,
            is_default: false,
            items: vec![
                aster_drive::services::storage_policy::policy::StoragePolicyGroupItemInput {
                    policy_id: default_policy_id,
                    priority: 1,
                    min_file_size: 0,
                    max_file_size: 0,
                },
            ],
        },
    )
    .await
    .expect("source policy group should be created");
    let target_group = aster_drive::services::storage_policy::policy::create_group(
        &state,
        aster_drive::services::storage_policy::policy::CreateStoragePolicyGroupInput {
            name: "Migration Target Group".to_string(),
            description: Some("target assignments".to_string()),
            is_enabled: true,
            is_default: false,
            items: vec![
                aster_drive::services::storage_policy::policy::StoragePolicyGroupItemInput {
                    policy_id: default_policy_id,
                    priority: 1,
                    min_file_size: 0,
                    max_file_size: 0,
                },
            ],
        },
    )
    .await
    .expect("target policy group should be created");
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);

    let migrated_user_id = admin_create_user!(
        app,
        admin_token,
        "pgmigrateduser",
        "policy-group-migrated-user@example.com",
        "password123"
    );
    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/admin/users/{migrated_user_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({
            "policy_group_id": source_group.id
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    admin_create_user!(
        app,
        admin_token,
        "pgactiveadm",
        "policy-group-active-team-admin@example.com",
        "password123"
    );
    admin_create_user!(
        app,
        admin_token,
        "pgarchadm",
        "policy-group-archived-team-admin@example.com",
        "password123"
    );

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/teams")
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({
            "name": "Policy Group Active Team",
            "description": "active team should migrate",
            "admin_identifier": "pgactiveadm",
            "storage_quota": 0,
            "policy_group_id": source_group.id
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let active_team_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/teams")
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({
            "name": "Policy Group Archived Team",
            "description": "archived team should still migrate",
            "admin_identifier": "pgarchadm",
            "storage_quota": 0,
            "policy_group_id": source_group.id
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let archived_team_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/admin/teams/{archived_team_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/v1/admin/policy-groups/{}/migrate-assignments",
            source_group.id
        ))
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({
            "target_group_id": target_group.id
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "success", "{body}");
    assert_eq!(body["data"]["affected_users"], 1);
    assert_eq!(body["data"]["affected_teams"], 2);
    assert_eq!(body["data"]["migrated_assignments"], 3);

    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/v1/admin/policy-groups/{}/migrate-assignments",
            source_group.id
        ))
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({
            "target_group_id": target_group.id
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "success", "{body}");
    assert_eq!(body["data"]["affected_users"], 0);
    assert_eq!(body["data"]["affected_teams"], 0);
    assert_eq!(body["data"]["migrated_assignments"], 0);

    let migrated_user = user_repo::find_by_id(state.writer_db(), migrated_user_id)
        .await
        .expect("migrated user should exist");
    assert_eq!(migrated_user.policy_group_id, Some(target_group.id));
    let active_team = team_repo::find_by_id(state.writer_db(), active_team_id)
        .await
        .expect("active team should exist");
    assert_eq!(active_team.policy_group_id, Some(target_group.id));
    assert!(active_team.archived_at.is_none());
    let archived_team = team_repo::find_by_id(state.writer_db(), archived_team_id)
        .await
        .expect("archived team should exist");
    assert_eq!(archived_team.policy_group_id, Some(target_group.id));
    assert!(archived_team.archived_at.is_some());
}

#[actix_web::test]
async fn test_admin_remote_nodes_support_explicit_sorting() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    for (name, base_url, enabled) in [
        ("Remote Sort Alpha", "https://alpha.example.com/node/", true),
        ("Remote Sort Zeta", "https://zeta.example.com/node/", false),
    ] {
        let req = test::TestRequest::post()
            .uri("/api/v1/admin/remote-nodes")
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .set_json(serde_json::json!({
                "name": name,
                "base_url": base_url,
                "is_enabled": enabled
            }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201);
    }

    let body: Value = admin_get_json!(
        app,
        token,
        "/api/v1/admin/remote-nodes?sort_by=base_url&sort_order=desc&limit=10"
    );
    let nodes = body["data"]["items"].as_array().unwrap();
    assert_eq!(
        json_string_values(nodes, "base_url"),
        vec![
            "https://zeta.example.com/node",
            "https://alpha.example.com/node"
        ]
    );

    let body: Value = admin_get_json!(
        app,
        token,
        "/api/v1/admin/remote-nodes?sort_by=is_enabled&sort_order=asc&limit=10"
    );
    let nodes = body["data"]["items"].as_array().unwrap();
    assert_eq!(nodes[0]["name"], "Remote Sort Zeta");
    assert_eq!(nodes[0]["is_enabled"], false);
}

#[actix_web::test]
async fn test_admin_shares_support_explicit_sorting() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    for (filename, max_downloads) in [
        ("share-sort-three.txt", 3),
        ("share-sort-one.txt", 1),
        ("share-sort-two.txt", 2),
    ] {
        let file_id = upload_test_file_named!(app, token, filename);
        let req = test::TestRequest::post()
            .uri("/api/v1/shares")
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .set_json(serde_json::json!({
                "target": { "type": "file", "id": file_id },
                "max_downloads": max_downloads
            }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201);
    }

    let body: Value = admin_get_json!(
        app,
        token,
        "/api/v1/admin/shares?sort_by=max_downloads&sort_order=asc&limit=10"
    );
    let shares = body["data"]["items"].as_array().unwrap();
    assert_eq!(json_i64_values(shares, "max_downloads"), vec![1, 2, 3]);

    let body: Value = admin_get_json!(
        app,
        token,
        "/api/v1/admin/shares?sort_by=max_downloads&sort_order=desc&limit=10"
    );
    let shares = body["data"]["items"].as_array().unwrap();
    assert_eq!(json_i64_values(shares, "max_downloads"), vec![3, 2, 1]);
}

#[actix_web::test]
async fn test_admin_locks_support_explicit_sorting() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let now = Utc::now();

    for (idx, (path, shared, deep)) in [
        ("/sort/zeta.txt", false, false),
        ("/sort/alpha.txt", true, true),
        ("/sort/beta.txt", false, true),
    ]
    .into_iter()
    .enumerate()
    {
        lock_repo::create(
            state.writer_db(),
            resource_lock::ActiveModel {
                token: Set(format!("urn:uuid:{}", uuid::Uuid::new_v4())),
                entity_type: Set(EntityType::File),
                entity_id: Set(10_000 + idx as i64),
                path: Set(path.to_string()),
                owner_id: Set(Some(1)),
                owner_info: Set(Some(StoredLockOwnerInfo(
                    r#"{"kind":"text","value":"sort-test"}"#.to_string(),
                ))),
                timeout_at: Set(Some(now + Duration::hours(1))),
                shared: Set(shared),
                deep: Set(deep),
                created_at: Set(now),
                ..Default::default()
            },
        )
        .await
        .expect("lock should be inserted");
    }

    let body: Value = admin_get_json!(
        app,
        token,
        "/api/v1/admin/locks?sort_by=path&sort_order=asc&limit=10"
    );
    let locks = body["data"]["items"].as_array().unwrap();
    assert_eq!(
        json_string_values(locks, "path"),
        vec!["/sort/alpha.txt", "/sort/beta.txt", "/sort/zeta.txt"]
    );

    let body: Value = admin_get_json!(
        app,
        token,
        "/api/v1/admin/locks?sort_by=shared&sort_order=desc&limit=10"
    );
    let locks = body["data"]["items"].as_array().unwrap();
    assert_eq!(locks[0]["path"], "/sort/alpha.txt");
    assert_eq!(locks[0]["shared"], true);
}

#[actix_web::test]
async fn test_admin_tasks_support_explicit_sorting_and_id_tiebreaker() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let now = Utc::now();
    let mut inserted_ids = Vec::new();

    for display_name in ["Task Sort Zeta", "Task Sort Alpha", "Task Sort Beta"] {
        let task = background_task_repo::create(
            state.writer_db(),
            background_task::ActiveModel {
                kind: Set(BackgroundTaskKind::SystemRuntime),
                status: Set(BackgroundTaskStatus::Pending),
                creator_user_id: Set(Some(1)),
                team_id: Set(None),
                share_id: Set(None),
                display_name: Set(display_name.to_string()),
                payload_json: Set(StoredTaskPayload(
                    r#"{"task_name":"background-task-dispatch"}"#.to_string(),
                )),
                result_json: Set(None),
                runtime_json: Set(None),
                steps_json: Set(None),
                progress_current: Set(5),
                progress_total: Set(10),
                status_text: Set(Some("sort regression".to_string())),
                attempt_count: Set(0),
                max_attempts: Set(3),
                next_run_at: Set(now),
                processing_started_at: Set(None),
                last_heartbeat_at: Set(None),
                lease_expires_at: Set(None),
                started_at: Set(None),
                finished_at: Set(None),
                last_error: Set(None),
                failure_can_retry: Set(None),
                expires_at: Set(now + Duration::hours(24)),
                created_at: Set(now),
                updated_at: Set(now),
                ..Default::default()
            },
        )
        .await
        .expect("task should be inserted");
        inserted_ids.push(task.id);
    }

    let body: Value = admin_get_json!(
        app,
        token,
        "/api/v1/admin/tasks?sort_by=display_name&sort_order=asc&limit=10"
    );
    let tasks = body["data"]["items"].as_array().unwrap();
    assert_eq!(
        json_string_values(tasks, "display_name"),
        vec!["Task Sort Alpha", "Task Sort Beta", "Task Sort Zeta"]
    );

    inserted_ids.sort();
    inserted_ids.reverse();
    let body: Value = admin_get_json!(
        app,
        token,
        "/api/v1/admin/tasks?sort_by=progress&sort_order=desc&limit=3"
    );
    let tasks = body["data"]["items"].as_array().unwrap();
    assert_eq!(json_i64_values(tasks, "id"), inserted_ids);
}

#[actix_web::test]
async fn test_admin_audit_logs_support_explicit_sorting() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let now = Utc::now();
    let marker = uuid::Uuid::new_v4();

    for (entity_name, ip_address) in [
        (format!("Audit Sort {marker} Zeta"), "10.0.0.3"),
        (format!("Audit Sort {marker} Alpha"), "10.0.0.1"),
        (format!("Audit Sort {marker} Beta"), "10.0.0.2"),
    ] {
        audit_log_repo::create(
            state.writer_db(),
            audit_log::ActiveModel {
                user_id: Set(1),
                action: Set(AuditAction::AdminUpdateUser),
                entity_type: Set("user".to_string()),
                entity_id: Set(Some(entity_name.len() as i64)),
                entity_name: Set(Some(entity_name)),
                details: Set(None),
                ip_address: Set(Some(ip_address.to_string())),
                user_agent: Set(None),
                created_at: Set(now),
                ..Default::default()
            },
        )
        .await
        .expect("audit log should be inserted");
    }

    let body: Value = admin_get_json!(
        app,
        token,
        "/api/v1/admin/audit-logs?entity_type=user&action=admin_update_user&sort_by=entity_name&sort_order=asc&limit=10"
    );
    let logs = body["data"]["items"].as_array().unwrap();
    let logs: Vec<Value> = logs
        .iter()
        .filter(|log| {
            log["entity_name"]
                .as_str()
                .is_some_and(|name| name.contains(&marker.to_string()))
        })
        .cloned()
        .collect();
    assert_eq!(
        json_string_values(&logs, "entity_name"),
        vec![
            format!("Audit Sort {marker} Alpha"),
            format!("Audit Sort {marker} Beta"),
            format!("Audit Sort {marker} Zeta"),
        ]
    );

    let body: Value = admin_get_json!(
        app,
        token,
        "/api/v1/admin/audit-logs?entity_type=user&action=admin_update_user&sort_by=ip_address&sort_order=desc&limit=10"
    );
    let logs = body["data"]["items"].as_array().unwrap();
    let logs: Vec<Value> = logs
        .iter()
        .filter(|log| {
            log["entity_name"]
                .as_str()
                .is_some_and(|name| name.contains(&marker.to_string()))
        })
        .cloned()
        .collect();
    assert_eq!(
        json_string_values(&logs, "ip_address"),
        vec!["10.0.0.3", "10.0.0.2", "10.0.0.1"]
    );
}

#[actix_web::test]
async fn test_admin_audit_logs_skip_invalid_entity_type_rows() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let now = Utc::now();
    let marker = uuid::Uuid::new_v4();

    for (entity_type, entity_name) in [
        ("not_a_real_entity", format!("Audit Invalid {marker}")),
        ("file", format!("Audit Valid {marker}")),
    ] {
        audit_log_repo::create(
            state.writer_db(),
            audit_log::ActiveModel {
                user_id: Set(1),
                action: Set(AuditAction::FileUpload),
                entity_type: Set(entity_type.to_string()),
                entity_id: Set(Some(1)),
                entity_name: Set(Some(entity_name)),
                details: Set(None),
                ip_address: Set(None),
                user_agent: Set(None),
                created_at: Set(now),
                ..Default::default()
            },
        )
        .await
        .expect("audit log should be inserted");
    }

    let body: Value = admin_get_json!(
        app,
        token,
        "/api/v1/admin/audit-logs?action=file_upload&sort_by=created_at&sort_order=asc&limit=10"
    );
    let names: Vec<String> = body["data"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|item| item["entity_name"].as_str())
        .filter(|name| name.contains(&marker.to_string()))
        .map(ToOwned::to_owned)
        .collect();
    assert!(names.contains(&format!("Audit Valid {marker}")));
    assert!(!names.contains(&format!("Audit Invalid {marker}")));
}

#[actix_web::test]
async fn test_admin_overview() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let now = Utc::now();

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/register")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "username": "overview-user",
            "email": "overview-user@example.com",
            "password": "password123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    background_task_repo::create(
        state.writer_db(),
        background_task::ActiveModel {
            kind: Set(BackgroundTaskKind::SystemRuntime),
            status: Set(BackgroundTaskStatus::Succeeded),
            creator_user_id: Set(None),
            team_id: Set(None),
            share_id: Set(None),
            display_name: Set("Trash cleanup".to_string()),
            payload_json: Set(StoredTaskPayload(
                r#"{"task_name":"trash-cleanup"}"#.to_string(),
            )),
            result_json: Set(None),
            runtime_json: Set(None),
            steps_json: Set(None),
            progress_current: Set(1),
            progress_total: Set(1),
            status_text: Set(Some("cleaned up 2 expired trash entries".to_string())),
            attempt_count: Set(0),
            max_attempts: Set(1),
            next_run_at: Set(now),
            processing_started_at: Set(None),
            last_heartbeat_at: Set(None),
            started_at: Set(Some(now - Duration::seconds(5))),
            finished_at: Set(Some(now - Duration::seconds(1))),
            last_error: Set(None),
            expires_at: Set(now + Duration::hours(24)),
            created_at: Set(now - Duration::seconds(5)),
            updated_at: Set(now - Duration::seconds(1)),
            ..Default::default()
        },
    )
    .await
    .expect("background task event should be inserted");

    background_task_repo::create(
        state.writer_db(),
        background_task::ActiveModel {
            kind: Set(BackgroundTaskKind::SystemRuntime),
            status: Set(BackgroundTaskStatus::Failed),
            creator_user_id: Set(None),
            team_id: Set(None),
            share_id: Set(None),
            display_name: Set("System health check".to_string()),
            payload_json: Set(StoredTaskPayload(
                r#"{"task_name":"system-health-check"}"#.to_string(),
            )),
            result_json: Set(Some(StoredTaskResult(
                serde_json::json!({
                    "duration_ms": 1_000,
                    "summary": "cache degraded",
                    "system_health": {
                        "status": "degraded",
                        "components": [
                            {
                                "name": "database",
                                "status": "healthy",
                                "message": "database ping succeeded",
                            },
                            {
                                "name": "cache",
                                "status": "degraded",
                                "message": "configured cache backend 'redis' is using active backend 'memory'",
                            },
                            {
                                "name": "remote_nodes",
                                "status": "healthy",
                                "message": "checked 1 remote node: 1 healthy, 0 failed, 0 skipped",
                            },
                        ],
                    },
                })
                .to_string(),
            ))),
            runtime_json: Set(None),
            steps_json: Set(None),
            progress_current: Set(0),
            progress_total: Set(1),
            status_text: Set(Some("cache degraded".to_string())),
            attempt_count: Set(0),
            max_attempts: Set(1),
            next_run_at: Set(now - Duration::seconds(10)),
            processing_started_at: Set(None),
            last_heartbeat_at: Set(None),
            started_at: Set(Some(now - Duration::seconds(11))),
            finished_at: Set(Some(now - Duration::seconds(10))),
            last_error: Set(Some(
                "cache=degraded: configured cache backend 'redis' is using active backend 'memory'"
                    .to_string(),
            )),
            expires_at: Set(now + Duration::hours(24)),
            created_at: Set(now - Duration::seconds(11)),
            updated_at: Set(now - Duration::seconds(10)),
            ..Default::default()
        },
    )
    .await
    .expect("system health event should be inserted");

    let file_id = upload_test_file!(app, token);

    let req = test::TestRequest::post()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "target": { "type": "file", "id": file_id }
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/overview?days=3&timezone=Asia/Shanghai&event_limit=1")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let data = &body["data"];

    assert_eq!(data["timezone"], "Asia/Shanghai");
    assert_eq!(data["days"], 3);
    assert_eq!(data["stats"]["total_users"], 2);
    assert_eq!(data["stats"]["active_users"], 2);
    assert_eq!(data["stats"]["disabled_users"], 0);
    assert_eq!(data["stats"]["total_files"], 1);
    assert_eq!(data["stats"]["total_blobs"], 1);
    assert_eq!(data["stats"]["total_shares"], 1);
    assert!(data["stats"]["total_file_bytes"].as_i64().unwrap() > 0);
    assert!(data["stats"]["total_blob_bytes"].as_i64().unwrap() > 0);
    assert_eq!(
        data["stats"]["total_file_bytes"],
        data["stats"]["total_blob_bytes"]
    );
    assert!(data["stats"]["audit_events_today"].as_u64().unwrap() >= 5);
    assert_eq!(data["stats"]["new_users_today"], 2);
    assert_eq!(data["stats"]["uploads_today"], 1);
    assert_eq!(data["stats"]["shares_today"], 1);
    assert_eq!(data["system_health"]["status"], "degraded");
    assert_eq!(data["system_health"]["summary"], "cache degraded");
    assert_eq!(
        data["system_health"]["details"],
        "cache=degraded: configured cache backend 'redis' is using active backend 'memory'"
    );
    let health_components = data["system_health"]["components"].as_array().unwrap();
    assert_eq!(health_components.len(), 3);
    assert_eq!(health_components[0]["name"], "database");
    assert_eq!(health_components[0]["status"], "healthy");
    assert_eq!(health_components[1]["name"], "cache");
    assert_eq!(health_components[1]["status"], "degraded");
    assert_eq!(
        health_components[1]["message"],
        "configured cache backend 'redis' is using active backend 'memory'"
    );
    assert!(!data["system_health"]["task_id"].is_null());
    assert!(!data["system_health"]["checked_at"].is_null());

    let reports = data["daily_reports"].as_array().unwrap();
    assert_eq!(reports.len(), 3);
    let shanghai_today = chrono::Utc::now()
        .with_timezone(&chrono_tz::Asia::Shanghai)
        .date_naive();
    assert_eq!(
        reports[0]["date"],
        shanghai_today.format("%Y-%m-%d").to_string()
    );
    assert_eq!(
        reports[1]["date"],
        (shanghai_today - chrono::Duration::days(1))
            .format("%Y-%m-%d")
            .to_string()
    );
    assert_eq!(
        reports[2]["date"],
        (shanghai_today - chrono::Duration::days(2))
            .format("%Y-%m-%d")
            .to_string()
    );
    assert_eq!(reports[0]["new_users"], 2);
    assert_eq!(reports[0]["sign_ins"], 1);
    assert_eq!(reports[0]["uploads"], 1);
    assert_eq!(reports[0]["share_creations"], 1);

    let events = data["recent_events"].as_array().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["action"], "share_create");

    let background_tasks = data["recent_background_tasks"].as_array().unwrap();
    assert_eq!(background_tasks.len(), 1);
    assert_eq!(background_tasks[0]["kind"], "system_runtime");
    assert_eq!(background_tasks[0]["display_name"], "Trash cleanup");
    assert_eq!(background_tasks[0]["status"], "succeeded");
    assert_eq!(
        background_tasks[0]["status_text"],
        "cleaned up 2 expired trash entries"
    );
}

#[actix_web::test]
async fn test_admin_overview_rejects_invalid_timezone() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/overview?timezone=Not/AZone")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
}

#[actix_web::test]
async fn test_admin_overview_batches_large_audit_daily_reports() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let target_day = Utc::now().date_naive() - Duration::days(1);
    let target_at = target_day
        .and_hms_opt(12, 0, 0)
        .expect("midday should be valid")
        .and_utc();
    let marker = uuid::Uuid::new_v4();
    let inserted_events = 1_005_u64;
    let mut models = Vec::new();
    for index in 0..inserted_events {
        models.push(audit_log::ActiveModel {
            user_id: Set(1),
            action: Set(AuditAction::FileUpload),
            entity_type: Set("file".to_string()),
            entity_id: Set(Some(
                i64::try_from(index).expect("test index should fit i64"),
            )),
            entity_name: Set(Some(format!("Overview batch {marker} #{index}"))),
            details: Set(None),
            ip_address: Set(None),
            user_agent: Set(None),
            created_at: Set(target_at),
            ..Default::default()
        });
    }
    for chunk in models.chunks(100) {
        audit_log_repo::create_many(state.writer_db(), chunk.to_vec())
            .await
            .expect("audit log batch should be inserted");
    }

    let body: Value = admin_get_json!(
        app,
        token,
        "/api/v1/admin/overview?days=3&timezone=UTC&event_limit=1"
    );
    let reports = body["data"]["daily_reports"].as_array().unwrap();
    let target_report = reports
        .iter()
        .find(|report| report["date"] == target_day.to_string())
        .expect("target day report should be present");
    assert_eq!(target_report["uploads"], inserted_events);
    assert_eq!(target_report["total_events"], inserted_events);
}

#[actix_web::test]
async fn test_admin_tasks_lists_all_recorded_tasks() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let now = Utc::now();

    let req = test::TestRequest::post()
        .uri("/api/v1/teams")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Admin Tasks Team"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let team_body: Value = test::read_body_json(resp).await;
    let team_id = team_body["data"]["id"].as_i64().unwrap();

    let system_task = background_task_repo::create(
        state.writer_db(),
        background_task::ActiveModel {
            kind: Set(BackgroundTaskKind::SystemRuntime),
            status: Set(BackgroundTaskStatus::Succeeded),
            creator_user_id: Set(None),
            team_id: Set(None),
            share_id: Set(None),
            display_name: Set("Blob reconcile".to_string()),
            payload_json: Set(StoredTaskPayload(
                r#"{"task_name":"blob-reconcile"}"#.to_string(),
            )),
            result_json: Set(None),
            runtime_json: Set(None),
            steps_json: Set(None),
            progress_current: Set(1),
            progress_total: Set(1),
            status_text: Set(Some("reconciled 12 blobs".to_string())),
            attempt_count: Set(0),
            max_attempts: Set(1),
            next_run_at: Set(now),
            processing_started_at: Set(None),
            last_heartbeat_at: Set(None),
            started_at: Set(Some(now - Duration::minutes(4))),
            finished_at: Set(Some(now - Duration::minutes(3))),
            last_error: Set(None),
            expires_at: Set(now + Duration::hours(24)),
            created_at: Set(now - Duration::minutes(4)),
            updated_at: Set(now - Duration::minutes(3)),
            ..Default::default()
        },
    )
    .await
    .expect("system task should be inserted");

    let team_task = background_task_repo::create(
        state.writer_db(),
        background_task::ActiveModel {
            kind: Set(BackgroundTaskKind::ArchiveCompress),
            status: Set(BackgroundTaskStatus::Failed),
            creator_user_id: Set(Some(1)),
            team_id: Set(Some(team_id)),
            share_id: Set(None),
            display_name: Set("Compress team archive".to_string()),
            payload_json: Set(StoredTaskPayload(
                r#"{"file_ids":[],"folder_ids":[1],"archive_name":"team.zip","target_folder_id":null}"#
                    .to_string(),
            )),
            result_json: Set(None),
            runtime_json: Set(None),
            steps_json: Set(None),
            progress_current: Set(2),
            progress_total: Set(4),
            status_text: Set(Some("compressing".to_string())),
            attempt_count: Set(1),
            max_attempts: Set(3),
            next_run_at: Set(now),
            processing_started_at: Set(None),
            last_heartbeat_at: Set(None),
            started_at: Set(Some(now - Duration::minutes(2))),
            finished_at: Set(Some(now - Duration::minutes(1))),
            last_error: Set(Some("zip writer failed".to_string())),
            expires_at: Set(now + Duration::hours(24)),
            created_at: Set(now - Duration::minutes(2)),
            updated_at: Set(now - Duration::minutes(1)),
            ..Default::default()
        },
    )
    .await
    .expect("team task should be inserted");

    let personal_task = background_task_repo::create(
        state.writer_db(),
        background_task::ActiveModel {
            kind: Set(BackgroundTaskKind::ArchiveExtract),
            status: Set(BackgroundTaskStatus::Processing),
            creator_user_id: Set(Some(1)),
            team_id: Set(None),
            share_id: Set(None),
            display_name: Set("Extract upload".to_string()),
            payload_json: Set(StoredTaskPayload(
                r##"{"file_id":1,"source_file_name":"upload.zip","target_folder_id":2,"output_folder_name":"upload"}"##
                    .to_string(),
            )),
            result_json: Set(None),
            runtime_json: Set(None),
            steps_json: Set(None),
            progress_current: Set(3),
            progress_total: Set(5),
            status_text: Set(Some("extracting files".to_string())),
            attempt_count: Set(0),
            max_attempts: Set(1),
            next_run_at: Set(now),
            processing_started_at: Set(Some(now - Duration::seconds(40))),
            last_heartbeat_at: Set(Some(now - Duration::seconds(5))),
            lease_expires_at: Set(Some(now + Duration::seconds(55))),
            started_at: Set(Some(now - Duration::seconds(40))),
            finished_at: Set(None),
            last_error: Set(None),
            expires_at: Set(now + Duration::hours(24)),
            created_at: Set(now - Duration::seconds(40)),
            updated_at: Set(now - Duration::seconds(5)),
            ..Default::default()
        },
    )
    .await
    .expect("personal task should be inserted");

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/tasks?limit=2")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;

    assert_eq!(body["data"]["limit"], 2);
    assert_eq!(body["data"]["offset"], 0);
    assert_eq!(body["data"]["total"], 3);

    let items = body["data"]["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["id"], personal_task.id);
    assert_eq!(items[0]["kind"], "archive_extract");
    assert_eq!(items[0]["status"], "processing");
    assert_eq!(items[0]["creator"]["id"], 1);
    assert_eq!(items[0]["creator"]["username"], "testuser");
    assert!(items[0]["team_id"].is_null());
    assert_eq!(items[0]["progress_percent"], 60);
    assert!(items[0]["lease_expires_at"].is_string());

    assert_eq!(items[1]["id"], team_task.id);
    assert_eq!(items[1]["kind"], "archive_compress");
    assert_eq!(items[1]["status"], "failed");
    assert_eq!(items[1]["creator"]["id"], 1);
    assert_eq!(items[1]["creator"]["username"], "testuser");
    assert_eq!(items[1]["team_id"], team_id);
    assert_eq!(items[1]["last_error"], "zip writer failed");

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/tasks?limit=2&offset=2")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let items = body["data"]["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["id"], system_task.id);
    assert_eq!(items[0]["kind"], "system_runtime");
    assert!(items[0]["creator"].is_null());
    assert!(items[0]["team_id"].is_null());
}

#[actix_web::test]
async fn test_admin_tasks_cleanup_uses_explicit_finished_before() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let now = Utc::now();

    let insert_task = |kind: BackgroundTaskKind,
                       status: BackgroundTaskStatus,
                       finished_at: Option<chrono::DateTime<Utc>>,
                       updated_at: chrono::DateTime<Utc>,
                       display_name: &str| {
        let payload_json = match kind {
            BackgroundTaskKind::SystemRuntime => {
                StoredTaskPayload(r#"{"task_name":"background-task-dispatch"}"#.to_string())
            }
            BackgroundTaskKind::ArchiveExtract => StoredTaskPayload(
                r##"{"file_id":1,"source_file_name":"upload.zip","target_folder_id":null,"output_folder_name":"upload"}"##
                    .to_string(),
            ),
            BackgroundTaskKind::ArchiveCompress => StoredTaskPayload(
                r#"{"file_ids":[],"folder_ids":[1],"archive_name":"archive.zip","target_folder_id":null}"#
                    .to_string(),
            ),
            BackgroundTaskKind::ArchivePreviewGenerate => StoredTaskPayload(
                r#"{"file_id":1,"source_file_name":"archive.zip","source_blob_id":1,"source_hash":"hash","limit_signature":"source=1"}"#
                    .to_string(),
            ),
            BackgroundTaskKind::ThumbnailGenerate => StoredTaskPayload(
                r#"{"blob_id":1,"blob_hash":"hash","source_file_name":"image.png","source_mime_type":"image/png","processor":"images"}"#
                    .to_string(),
            ),
            BackgroundTaskKind::ImagePreviewGenerate => StoredTaskPayload(
                r#"{"blob_id":1,"blob_hash":"hash","source_file_name":"image.png","source_mime_type":"image/png","processor":"images"}"#
                    .to_string(),
            ),
            BackgroundTaskKind::MediaMetadataExtract => StoredTaskPayload(
                r#"{"blob_id":1,"blob_hash":"hash","source_file_name":"image.png","source_mime_type":"image/png","media_kind":"image"}"#
                    .to_string(),
            ),
            BackgroundTaskKind::StoragePolicyTempCleanup => StoredTaskPayload(
                r#"{"policy":{"id":1,"name":"Deleted policy","driver_type":"local","endpoint":"","bucket":"","access_key":"","secret_key":"","base_path":"/tmp/asterdrive-deleted-policy","remote_node_id":null,"max_file_size":0,"allowed_types":"[]","options":"{}","is_default":false,"chunk_size":5242880},"remote_node":null,"temp_keys":["files/temp-object"],"multipart_uploads":[]}"#
                    .to_string(),
            ),
            BackgroundTaskKind::StoragePolicyMigration => StoredTaskPayload(
                r#"{"source_policy_id":1,"target_policy_id":2,"delete_source_after_success":false,"plan_hash":"hash","source_policy_updated_at":"2026-01-01T00:00:00Z","target_policy_updated_at":"2026-01-01T00:00:00Z"}"#
                    .to_string(),
            ),
            BackgroundTaskKind::BlobMaintenance => StoredTaskPayload(
                r#"{"action":"integrity_check","blob_ids":[1]}"#.to_string(),
            ),
            BackgroundTaskKind::OfflineDownload => StoredTaskPayload(
                r#"{"url":"https://example.com/archive.zip","filename":"archive.zip","target_folder_id":null,"expected_sha256":null,"source_display_url":"https://example.com/archive.zip"}"#
                    .to_string(),
            ),
            BackgroundTaskKind::TrashPurgeAll => {
                StoredTaskPayload(r#"{}"#.to_string())
            }
        };

        background_task_repo::create(
            state.writer_db(),
            background_task::ActiveModel {
                kind: Set(kind),
                status: Set(status),
                creator_user_id: Set(Some(1)),
                team_id: Set(None),
                share_id: Set(None),
                display_name: Set(display_name.to_string()),
                payload_json: Set(payload_json),
                result_json: Set(None),
                runtime_json: Set(None),
                steps_json: Set(None),
                progress_current: Set(1),
                progress_total: Set(1),
                status_text: Set(Some("done".to_string())),
                attempt_count: Set(0),
                max_attempts: Set(1),
                next_run_at: Set(updated_at),
                processing_started_at: Set(None),
                last_heartbeat_at: Set(None),
                lease_expires_at: Set(None),
                started_at: Set(finished_at),
                finished_at: Set(finished_at),
                last_error: Set(None),
                expires_at: Set(now + Duration::hours(24)),
                created_at: Set(updated_at),
                updated_at: Set(updated_at),
                ..Default::default()
            },
        )
    };

    let old_failed = insert_task(
        BackgroundTaskKind::SystemRuntime,
        BackgroundTaskStatus::Failed,
        Some(now - Duration::hours(72)),
        now - Duration::hours(72),
        "Old failed runtime task",
    )
    .await
    .expect("old failed task should be inserted");
    let recent_failed = insert_task(
        BackgroundTaskKind::SystemRuntime,
        BackgroundTaskStatus::Failed,
        Some(now - Duration::hours(2)),
        now - Duration::hours(2),
        "Recent failed runtime task",
    )
    .await
    .expect("recent failed task should be inserted");
    let old_succeeded = insert_task(
        BackgroundTaskKind::SystemRuntime,
        BackgroundTaskStatus::Succeeded,
        Some(now - Duration::hours(96)),
        now - Duration::hours(96),
        "Old succeeded runtime task",
    )
    .await
    .expect("old succeeded task should be inserted");
    let other_kind = insert_task(
        BackgroundTaskKind::ArchiveExtract,
        BackgroundTaskStatus::Failed,
        Some(now - Duration::hours(96)),
        now - Duration::hours(96),
        "Old failed extract task",
    )
    .await
    .expect("other kind task should be inserted");
    let active_task = insert_task(
        BackgroundTaskKind::SystemRuntime,
        BackgroundTaskStatus::Processing,
        None,
        now - Duration::hours(96),
        "Active runtime task",
    )
    .await
    .expect("active task should be inserted");

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/tasks/cleanup")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "finished_before": (now - Duration::hours(24)).to_rfc3339(),
            "kind": "system_runtime",
            "status": "failed"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let status = resp.status();
    let body_bytes = test::read_body(resp).await;
    assert_eq!(
        status,
        200,
        "cleanup failed: {}",
        String::from_utf8_lossy(&body_bytes)
    );
    let body: Value =
        serde_json::from_slice(&body_bytes).expect("cleanup response should be valid json");
    assert_eq!(body["data"]["removed"], 1);

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/tasks?limit=10")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let ids = body["data"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|item| item["id"].as_i64())
        .collect::<Vec<_>>();

    assert!(!ids.contains(&old_failed.id));
    assert!(ids.contains(&recent_failed.id));
    assert!(ids.contains(&old_succeeded.id));
    assert!(ids.contains(&other_kind.id));
    assert!(ids.contains(&active_task.id));
}

#[actix_web::test]
async fn test_admin_can_read_uploaded_user_avatar() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (admin_token, _) = register_and_login!(app);

    let user_id = admin_create_user!(
        app,
        admin_token,
        "avatar-user",
        "avatar-user@example.com",
        "password123"
    );
    let (user_token, _) = login_user!(app, "avatar-user", "password123");

    let (boundary, payload) = avatar_upload_payload();
    let req = test::TestRequest::post()
        .uri("/api/v1/auth/profile/avatar/upload")
        .insert_header(("Cookie", common::access_cookie_header(&user_token)))
        .insert_header(common::csrf_header_for(&user_token))
        .insert_header((
            "Content-Type",
            format!("multipart/form-data; boundary={boundary}"),
        ))
        .set_payload(payload)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/admin/users/{user_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["profile"]["avatar"]["source"], "upload");
    assert_eq!(
        body["data"]["profile"]["avatar"]["url_512"],
        format!("/admin/users/{user_id}/avatar/512?v=1")
    );

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/admin/users/{user_id}/avatar/512"))
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.headers().get("content-type").unwrap(), "image/webp");
}

#[actix_web::test]
async fn test_admin_can_read_user_display_name() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (admin_token, _) = register_and_login!(app);

    let user_id = admin_create_user!(
        app,
        admin_token,
        "named-user",
        "named-user@example.com",
        "password123"
    );
    let (user_token, _) = login_user!(app, "named-user", "password123");

    let req = test::TestRequest::patch()
        .uri("/api/v1/auth/profile")
        .insert_header(("Cookie", common::access_cookie_header(&user_token)))
        .insert_header(common::csrf_header_for(&user_token))
        .set_json(serde_json::json!({
            "display_name": "Named User"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/users")
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let listed_user = body["data"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == user_id)
        .expect("created user should be listed");
    assert_eq!(listed_user["profile"]["display_name"], "Named User");

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/admin/users/{user_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["profile"]["display_name"], "Named User");
}

#[actix_web::test]
async fn test_admin_create_user() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/users")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "username": "newuser",
            "email": "newuser@example.com",
            "password": "password123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let user = &body["data"]["user"];
    assert_eq!(user["username"], "newuser");
    assert_eq!(user["email"], "newuser@example.com");
    assert_eq!(user["role"], "user");
    assert_eq!(user["status"], "active");
    assert_eq!(user["must_change_password"], false);
    assert_eq!(user["storage_quota"], 0);
    assert!(user.get("password_hash").is_none());
    assert!(body["data"].get("generated_password").is_none());

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/users?keyword=newuser")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["total"], 1);
    assert_eq!(body["data"]["items"][0]["username"], "newuser");
}

#[actix_web::test]
async fn test_admin_create_user_can_force_change_with_explicit_password() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/users")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "username": "manualforce",
            "email": "manualforce@example.com",
            "password": "password123",
            "must_change_password": true
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["user"]["must_change_password"], true);
    assert!(body["data"].get("generated_password").is_none());

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/login")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "identifier": "manualforce",
            "password": "password123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let access = common::extract_cookie(&resp, "aster_access").expect("access cookie missing");
    assert!(!access.is_empty());
    let csrf = common::extract_cookie(&resp, "aster_csrf").expect("csrf cookie missing");
    assert!(!csrf.is_empty());
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["status"], "password_change_required");
}

#[actix_web::test]
async fn test_admin_create_user_rejects_short_explicit_password_without_generation() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/users")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "username": "shortpassuser",
            "email": "shortpassuser@example.com",
            "password": "1234567"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert!(body["data"].is_null());
    assert!(body.get("generated_password").is_none());

    let user = user_repo::find_by_username(state.writer_db(), "shortpassuser")
        .await
        .unwrap();
    assert!(user.is_none());
}

#[actix_web::test]
async fn test_admin_create_user_generates_temporary_password_when_omitted() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/users")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "username": "generateduser",
            "email": "generateduser@example.com"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let user_id = body["data"]["user"]["id"].as_i64().unwrap();
    let generated_password = body["data"]["generated_password"]
        .as_str()
        .expect("generated password should be returned once")
        .to_string();
    assert_generated_password_policy(&generated_password);
    assert_eq!(body["data"]["user"]["must_change_password"], true);
    let user = user_repo::find_by_id(state.writer_db(), user_id)
        .await
        .unwrap();
    assert!(user.must_change_password);
    assert!(!user.password_hash.contains(&generated_password));

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/admin/users/{user_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert!(body["data"].get("generated_password").is_none());

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/users?keyword=generateduser")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["total"], 1);
    assert!(body["data"]["items"][0].get("generated_password").is_none());

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/login")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "identifier": "generateduser",
            "password": generated_password
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let access = common::extract_cookie(&resp, "aster_access").expect("access cookie missing");
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["status"], "password_change_required");

    let req = test::TestRequest::put()
        .uri("/api/v1/auth/password")
        .insert_header(("Cookie", common::access_cookie_header(&access)))
        .insert_header(common::csrf_header_for(&access))
        .set_json(serde_json::json!({
            "current_password": "wrongpassword",
            "new_password": "newsecret456"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        401,
        "wrong current password must not clear forced password change"
    );

    let req = test::TestRequest::put()
        .uri("/api/v1/auth/password")
        .insert_header(("Cookie", common::access_cookie_header(&access)))
        .insert_header(common::csrf_header_for(&access))
        .set_json(serde_json::json!({
            "current_password": generated_password,
            "new_password": "newsecret456"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let user = user_repo::find_by_id(state.writer_db(), user_id)
        .await
        .unwrap();
    assert!(!user.must_change_password);
}

#[actix_web::test]
async fn test_admin_create_user_generates_temporary_password_for_null_empty_and_whitespace() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    for (username, password) in [
        ("nullpass", serde_json::Value::Null),
        ("emptypass", serde_json::json!("")),
        ("spacepass", serde_json::json!("   \t\n")),
        ("explicitfalse", serde_json::json!("")),
    ] {
        let req = test::TestRequest::post()
            .uri("/api/v1/admin/users")
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .peer_addr("127.0.0.1:12345".parse().unwrap())
            .set_json(serde_json::json!({
                "username": username,
                "email": format!("{username}@example.com"),
                "password": password,
                "must_change_password": false
            }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201);
        let body: Value = test::read_body_json(resp).await;
        let generated_password = body["data"]["generated_password"]
            .as_str()
            .unwrap_or_else(|| panic!("temporary password should be generated for {username}"));
        assert_generated_password_policy(generated_password);
        assert_eq!(body["data"]["user"]["must_change_password"], true);
    }
}

#[actix_web::test]
async fn test_admin_create_user_rejects_overlong_generated_password_request_fields() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let long_username = "u".repeat(17);

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/users")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "username": long_username,
            "email": "badgenerated@example.com"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert!(body["data"].is_null());
    assert!(body.get("generated_password").is_none());
}

#[actix_web::test]
async fn test_non_admin_cannot_create_user() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (admin_token, _) = register_and_login!(app);
    admin_create_user!(
        app,
        admin_token,
        "plainuser",
        "plainuser@example.com",
        "password123"
    );
    let (token, _) = login_user!(app, "plainuser", "password123");

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/users")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "username": "blockeduser",
            "email": "blockeduser@example.com",
            "password": "password123"
        }))
        .to_request();
    let err = test::try_call_service(&app, req).await.unwrap_err();
    let resp = err.error_response();
    assert_eq!(resp.status(), 403);
}

#[actix_web::test]
async fn test_admin_users_server_side_filters() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    for (username, email) in [
        ("filter-alice", "filter-alice@example.com"),
        ("filter-bob", "filter-bob@example.com"),
        ("filter-charlie", "filter-charlie@example.com"),
    ] {
        let req = test::TestRequest::post()
            .uri("/api/v1/auth/register")
            .peer_addr("127.0.0.1:12345".parse().unwrap())
            .set_json(serde_json::json!({
                "username": username,
                "email": email,
                "password": "password123"
            }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201);
    }

    // 提升 alice 为 admin，禁用 bob
    let req = test::TestRequest::get()
        .uri("/api/v1/admin/users")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let users = body["data"]["items"].as_array().unwrap();
    let alice_id = users
        .iter()
        .find(|u| u["username"] == "filter-alice")
        .unwrap()["id"]
        .as_i64()
        .unwrap();
    let bob_id = users
        .iter()
        .find(|u| u["username"] == "filter-bob")
        .unwrap()["id"]
        .as_i64()
        .unwrap();

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/admin/users/{alice_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({"role": "admin"}))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/admin/users/{bob_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({"status": "disabled"}))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/users?keyword=alice")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let items = body["data"]["items"].as_array().unwrap();
    assert_eq!(body["data"]["total"], 1);
    assert_eq!(items[0]["username"], "filter-alice");

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/users?keyword=ice")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let items = body["data"]["items"].as_array().unwrap();
    assert_eq!(body["data"]["total"], 1);
    assert_eq!(items[0]["username"], "filter-alice");

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/users?keyword=ce")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let items = body["data"]["items"].as_array().unwrap();
    assert_eq!(body["data"]["total"], 1);
    assert_eq!(items[0]["username"], "filter-alice");

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/users?role=admin")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let items = body["data"]["items"].as_array().unwrap();
    assert_eq!(body["data"]["total"], 2);
    assert!(items.iter().all(|u| u["role"] == "admin"));

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/users?status=disabled")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let items = body["data"]["items"].as_array().unwrap();
    assert_eq!(body["data"]["total"], 1);
    assert_eq!(items[0]["username"], "filter-bob");
}

#[actix_web::test]
async fn test_admin_policies() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Archive S3",
            "driver_type": "s3",
            "endpoint": "https://s3.example.com",
            "bucket": "archive",
            "access_key": "ak",
            "secret_key": "sk",
            "base_path": "backups"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    // 列出策略分页，新建的应排在最前面
    let req = test::TestRequest::get()
        .uri("/api/v1/admin/policies?limit=1&offset=0")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let policies = body["data"]["items"].as_array().unwrap();
    assert_eq!(body["data"]["limit"], 1);
    assert_eq!(body["data"]["offset"], 0);
    assert_eq!(body["data"]["total"], 2);
    assert_eq!(policies.len(), 1);
    assert_eq!(policies[0]["name"], "Archive S3");
    assert_eq!(policies[0]["is_default"], false);
}

#[actix_web::test]
async fn test_admin_config() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    // 设置配置
    let req = test::TestRequest::put()
        .uri("/api/v1/admin/config/test_key")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "value": "test_value" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    // 读取配置
    let req = test::TestRequest::get()
        .uri("/api/v1/admin/config/test_key")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["value"], "test_value");

    // 列出所有配置
    let req = test::TestRequest::get()
        .uri("/api/v1/admin/config")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert!(!body["data"]["items"].as_array().unwrap().is_empty());
    assert!(body["data"]["total"].as_u64().unwrap() >= 1);

    // schema 里应暴露后台任务并发上限配置
    let req = test::TestRequest::get()
        .uri("/api/v1/admin/config/schema")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert!(body["data"].as_array().unwrap().iter().any(|item| {
        item["key"] == "background_task_max_concurrency"
            && item["category"] == "runtime.background_task"
    }));
    assert!(body["data"].as_array().unwrap().iter().any(|item| {
        item["key"] == "background_task_archive_max_concurrency"
            && item["category"] == "runtime.background_task"
    }));
    assert!(body["data"].as_array().unwrap().iter().any(|item| {
        item["key"] == "background_task_thumbnail_max_concurrency"
            && item["category"] == "runtime.background_task"
    }));
    assert!(body["data"].as_array().unwrap().iter().any(|item| {
        item["key"] == "background_task_storage_migration_max_concurrency"
            && item["category"] == "runtime.background_task"
    }));

    // 删除配置
    let req = test::TestRequest::delete()
        .uri("/api/v1/admin/config/test_key")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn test_admin_config_validates_media_derivative_dimensions() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    for (key, value) in [
        ("thumbnail_max_dimension", "1".to_string()),
        (
            "image_preview_max_dimension",
            aster_drive::config::operations::MAX_DERIVATIVE_MAX_DIMENSION.to_string(),
        ),
    ] {
        let req = test::TestRequest::put()
            .uri(&format!("/api/v1/admin/config/{key}"))
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .set_json(serde_json::json!({ "value": value.as_str() }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200, "{key}={value} should be accepted");

        let req = test::TestRequest::get()
            .uri(&format!("/api/v1/admin/config/{key}"))
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let body: Value = test::read_body_json(resp).await;
        assert_eq!(body["data"]["value"].as_str(), Some(value.as_str()));
    }

    let invalid_values = [
        "0".to_string(),
        "-1".to_string(),
        "12.5".to_string(),
        "abc".to_string(),
        (u64::from(aster_drive::config::operations::MAX_DERIVATIVE_MAX_DIMENSION) + 1).to_string(),
    ];
    for key in ["thumbnail_max_dimension", "image_preview_max_dimension"] {
        let req = test::TestRequest::get()
            .uri(&format!("/api/v1/admin/config/{key}"))
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let body: Value = test::read_body_json(resp).await;
        let original_value = body["data"]["value"].as_str().unwrap().to_string();

        for invalid in &invalid_values {
            let req = test::TestRequest::put()
                .uri(&format!("/api/v1/admin/config/{key}"))
                .insert_header(("Cookie", common::access_cookie_header(&token)))
                .insert_header(common::csrf_header_for(&token))
                .set_json(serde_json::json!({ "value": invalid.as_str() }))
                .to_request();
            let resp = test::call_service(&app, req).await;
            let status = resp.status();
            let body: Value = test::read_body_json(resp).await;
            assert_eq!(status, 400, "{key}={invalid} should be rejected: {body:#?}");

            let req = test::TestRequest::get()
                .uri(&format!("/api/v1/admin/config/{key}"))
                .insert_header(("Cookie", common::access_cookie_header(&token)))
                .insert_header(common::csrf_header_for(&token))
                .to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(resp.status(), 200);
            let body: Value = test::read_body_json(resp).await;
            assert_eq!(
                body["data"]["value"].as_str(),
                Some(original_value.as_str())
            );
        }
    }
}

#[actix_web::test]
async fn test_admin_email_code_mfa_requires_complete_mail_config() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::put()
        .uri("/api/v1/admin/config/auth_email_code_login_enabled")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "value": "true" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let status = resp.status();
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(status, 400, "{body:#?}");
    assert!(
        body["msg"]
            .as_str()
            .unwrap()
            .contains("email code MFA requires complete SMTP mail configuration")
    );

    for (key, value) in [
        ("mail_smtp_host", "smtp.example.com"),
        ("mail_from_address", "noreply@example.com"),
        ("mail_smtp_username", "smtp-user"),
    ] {
        let req = test::TestRequest::put()
            .uri(&format!("/api/v1/admin/config/{key}"))
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .set_json(serde_json::json!({ "value": value }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
    }

    let req = test::TestRequest::put()
        .uri("/api/v1/admin/config/auth_email_code_login_enabled")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "value": "true" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let status = resp.status();
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(status, 400, "{body:#?}");
    assert!(
        body["msg"]
            .as_str()
            .unwrap()
            .contains("email code MFA requires complete SMTP mail configuration")
    );

    let req = test::TestRequest::put()
        .uri("/api/v1/admin/config/mail_smtp_password")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "value": "smtp-pass" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::put()
        .uri("/api/v1/admin/config/auth_email_code_login_enabled")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "value": "true" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let status = resp.status();
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(status, 200, "{body:#?}");
    assert_eq!(body["data"]["value"], "true");
}

#[actix_web::test]
async fn test_admin_mail_config_changes_disable_email_code_mfa_when_incomplete() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    for (key, value) in [
        ("mail_smtp_host", "smtp.example.com"),
        ("mail_from_address", "noreply@example.com"),
        ("auth_email_code_login_enabled", "true"),
    ] {
        let req = test::TestRequest::put()
            .uri(&format!("/api/v1/admin/config/{key}"))
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .set_json(serde_json::json!({ "value": value }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
    }

    let req = test::TestRequest::put()
        .uri("/api/v1/admin/config/mail_smtp_host")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "value": "" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let status = resp.status();
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(status, 200, "{body:#?}");

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/config/auth_email_code_login_enabled")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let status = resp.status();
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(status, 200, "{body:#?}");
    assert_eq!(body["data"]["value"], "false");
}

#[actix_web::test]
async fn test_admin_smtp_credential_mismatch_disables_email_code_mfa() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    for (key, value) in [
        ("mail_smtp_host", "smtp.example.com"),
        ("mail_from_address", "noreply@example.com"),
        ("auth_email_code_login_enabled", "true"),
    ] {
        let req = test::TestRequest::put()
            .uri(&format!("/api/v1/admin/config/{key}"))
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .set_json(serde_json::json!({ "value": value }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
    }

    let req = test::TestRequest::put()
        .uri("/api/v1/admin/config/mail_smtp_username")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "value": "smtp-user" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let status = resp.status();
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(status, 200, "{body:#?}");

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/config/auth_email_code_login_enabled")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let status = resp.status();
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(status, 200, "{body:#?}");
    assert_eq!(body["data"]["value"], "false");
}

#[actix_web::test]
async fn test_admin_config_action_sends_test_email() {
    let state = common::setup().await;
    let mail_sender = state.mail_sender.clone();
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/config/mail/action")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "action": "send_test_email",
            "target_email": "deliver@example.com"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["data"]["message"],
        "Test email sent to deliver@example.com"
    );

    let memory_sender = aster_drive::services::mail::sender::memory_sender_ref(&mail_sender)
        .expect("memory mail sender should be available in tests");
    let message = memory_sender
        .last_message()
        .expect("test email should be sent");
    assert_eq!(message.to.address, "deliver@example.com");
    assert_eq!(message.subject, "AsterDrive SMTP test");
    assert!(message.text_body.contains("Triggered by: testuser"));
}

#[actix_web::test]
async fn test_admin_config_action_defaults_to_admin_email() {
    let state = common::setup().await;
    let mail_sender = state.mail_sender.clone();
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/config/mail/action")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "action": "send_test_email"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let memory_sender = aster_drive::services::mail::sender::memory_sender_ref(&mail_sender)
        .expect("memory mail sender should be available in tests");
    let message = memory_sender
        .last_message()
        .expect("test email should be sent");
    assert_eq!(message.to.address, "test@example.com");
}

#[cfg(unix)]
#[actix_web::test]
async fn test_admin_config_action_tests_vips_command_from_draft() {
    let fake_vips = write_fake_vips_command();
    let fake_vips_command = fake_vips.to_string_lossy().to_string();
    let draft_value = serde_json::json!({
        "version": 1,
        "processors": [
            {
                "kind": "vips_cli",
                "enabled": false,
                "extensions": ["heic"],
                "config": {
                    "command": fake_vips_command
                }
            },
            {
                "kind": "images",
                "enabled": true
            }
        ]
    })
    .to_string();
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/config/media_processing_registry_json/action")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "action": "test_vips_cli",
            "value": draft_value
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let message = body["data"]["message"]
        .as_str()
        .expect("config action should return a message");
    assert!(message.contains("is available"));
    assert!(message.contains("vips-8.16.0"));
    assert!(message.contains(fake_vips.to_string_lossy().as_ref()));

    let _ = std::fs::remove_dir_all(
        fake_vips
            .parent()
            .expect("fake vips script should have a parent directory"),
    );
}

#[cfg(unix)]
#[actix_web::test]
async fn test_admin_config_action_tests_media_processing_ffprobe_command_from_draft() {
    let fake_ffprobe = write_fake_ffprobe_command();
    let fake_ffprobe_command = fake_ffprobe.to_string_lossy().to_string();
    let draft_value = serde_json::json!({
        "version": 2,
        "processors": [
            {
                "kind": "ffprobe_cli",
                "enabled": false,
                "uses": ["metadata:video"],
                "extensions": ["mp4"],
                "config": {
                    "command": fake_ffprobe_command
                }
            },
            {
                "kind": "images",
                "enabled": true
            }
        ]
    })
    .to_string();
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/config/media_processing_registry_json/action")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "action": "test_ffprobe_cli",
            "value": draft_value
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let message = body["data"]["message"]
        .as_str()
        .expect("config action should return a message");
    assert!(message.contains("is available"));
    assert!(message.contains("ffprobe version 7.1-test"));
    assert!(message.contains(fake_ffprobe.to_string_lossy().as_ref()));

    let _ = std::fs::remove_dir_all(
        fake_ffprobe
            .parent()
            .expect("fake ffprobe script should have a parent directory"),
    );
}

async fn spawn_aria2_bad_request_server() -> String {
    let listener =
        std::net::TcpListener::bind(("127.0.0.1", 0)).expect("aria2 probe test server should bind");
    let port = listener
        .local_addr()
        .expect("aria2 probe test server address should resolve")
        .port();
    let server = HttpServer::new(|| {
        App::new().default_service(web::to(|| async { HttpResponse::BadRequest().finish() }))
    })
    .listen(listener)
    .expect("aria2 probe test server should listen")
    .run();
    actix_web::rt::spawn(server);
    format!("http://127.0.0.1:{port}/jsonrpc")
}

async fn spawn_aria2_unauthorized_server() -> String {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0))
        .expect("aria2 unauthorized test server should bind");
    let port = listener
        .local_addr()
        .expect("aria2 unauthorized test server address should resolve")
        .port();
    let server = HttpServer::new(|| {
        App::new().default_service(web::to(|| async {
            HttpResponse::BadRequest().json(serde_json::json!({
                "jsonrpc": "2.0",
                "id": "asterdrive-test",
                "error": {
                    "code": 1,
                    "message": "Unauthorized"
                }
            }))
        }))
    })
    .listen(listener)
    .expect("aria2 unauthorized test server should listen")
    .run();
    actix_web::rt::spawn(server);
    format!("http://127.0.0.1:{port}/jsonrpc")
}

async fn spawn_aria2_secret_check_server(expected_secret: &'static str) -> String {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0))
        .expect("aria2 secret-check test server should bind");
    let port = listener
        .local_addr()
        .expect("aria2 secret-check test server address should resolve")
        .port();
    let server = HttpServer::new(move || {
        App::new().default_service(web::to(move |body: web::Json<Value>| async move {
            let expected = format!("token:{expected_secret}");
            let received = body["params"]
                .as_array()
                .and_then(|params| params.first())
                .and_then(|value| value.as_str());
            if received == Some(expected.as_str()) {
                return HttpResponse::Ok().json(serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": body["id"].clone(),
                    "result": {
                        "version": "1.37.0-test",
                        "enabledFeatures": []
                    }
                }));
            }

            HttpResponse::BadRequest().json(serde_json::json!({
                "jsonrpc": "2.0",
                "id": body["id"].clone(),
                "error": {
                    "code": 1,
                    "message": "Unauthorized"
                }
            }))
        }))
    })
    .listen(listener)
    .expect("aria2 secret-check test server should listen")
    .run();
    actix_web::rt::spawn(server);
    format!("http://127.0.0.1:{port}/jsonrpc")
}

#[actix_web::test]
async fn test_admin_config_action_tests_aria2_rpc_from_draft_and_returns_probe_code() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let rpc_url = spawn_aria2_bad_request_server().await;

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/config/offline_download_engine_registry_json/action")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "action": "test_aria2_rpc",
            "value": r#"{"version":1,"engines":[{"kind":"aria2","enabled":true},{"kind":"builtin","enabled":true}]}"#,
            "draft_values": {
                "offline_download_engine_registry_json": r#"{"version":1,"engines":[{"kind":"aria2","enabled":true},{"kind":"builtin","enabled":true}]}"#,
                "offline_download_aria2_rpc_url": rpc_url,
                "offline_download_aria2_rpc_secret": "draft-secret",
                "offline_download_aria2_request_timeout_secs": "2"
            }
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "offline_download.aria2_rpc_probe_failed");
    assert_eq!(body["error"]["retryable"], false);
    assert!(body["error"].get("code").is_none());
    assert!(body["error"].get("subcode").is_none());
    let message = body["msg"]
        .as_str()
        .expect("aria2 probe failure should return a message");
    assert!(message.contains("aria2 RPC probe failed"));
    assert!(message.contains("HTTP 400 Bad Request"));
    assert!(!message.contains("Storage Driver Error"));
}

#[actix_web::test]
async fn test_admin_config_action_tests_aria2_rpc_wrong_secret_returns_auth_code() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let rpc_url = spawn_aria2_unauthorized_server().await;

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/config/offline_download_engine_registry_json/action")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "action": "test_aria2_rpc",
            "value": r#"{"version":1,"engines":[{"kind":"aria2","enabled":true},{"kind":"builtin","enabled":true}]}"#,
            "draft_values": {
                "offline_download_engine_registry_json": r#"{"version":1,"engines":[{"kind":"aria2","enabled":true},{"kind":"builtin","enabled":true}]}"#,
                "offline_download_aria2_rpc_url": rpc_url,
                "offline_download_aria2_rpc_secret": "wrong-secret",
                "offline_download_aria2_request_timeout_secs": "2"
            }
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "offline_download.aria2_rpc_auth_failed");
    assert_eq!(body["error"]["retryable"], false);
    assert!(body["error"].get("code").is_none());
    assert!(body["error"].get("subcode").is_none());
    let message = body["msg"]
        .as_str()
        .expect("aria2 auth failure should return a message");
    assert!(message.contains("authentication failed"));
    assert!(message.contains("offline_download_aria2_rpc_secret"));
    assert!(!message.contains("HTTP 400"));
    assert!(!message.contains("Storage Driver Error"));
}

#[actix_web::test]
async fn test_admin_config_action_tests_aria2_rpc_uses_redacted_secret_when_sent_as_draft() {
    let state = common::setup().await;
    let rpc_url = spawn_aria2_secret_check_server("***REDACTED***").await;
    aster_drive::services::ops::config::set(&state, "offline_download_aria2_rpc_url", &rpc_url, 1)
        .await
        .expect("saved aria2 RPC URL should update");
    aster_drive::services::ops::config::set(
        &state,
        "offline_download_aria2_rpc_secret",
        "saved-secret",
        1,
    )
    .await
    .expect("saved aria2 RPC secret should update");
    aster_drive::services::ops::config::set(
        &state,
        "offline_download_aria2_request_timeout_secs",
        "2",
        1,
    )
    .await
    .expect("saved aria2 RPC timeout should update");

    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let req = test::TestRequest::post()
        .uri("/api/v1/admin/config/offline_download_engine_registry_json/action")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "action": "test_aria2_rpc",
            "draft_values": {
                "offline_download_aria2_rpc_url": rpc_url,
                "offline_download_aria2_rpc_secret": "***REDACTED***"
            }
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "success");
    assert!(
        body["data"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("aria2 RPC ready"))
    );
}

#[actix_web::test]
async fn test_admin_shares() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let file_id = upload_test_file!(app, token);

    // 创建分享
    let req = test::TestRequest::post()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "target": { "type": "file", "id": file_id }
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let share_id = body["data"]["id"].as_i64().unwrap();

    // admin 列出所有分享
    let req = test::TestRequest::get()
        .uri("/api/v1/admin/shares")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["items"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"]["total"], 1);

    // admin 删除分享
    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/admin/shares/{share_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn test_admin_force_unlock() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let file_id = upload_test_file!(app, token);

    // 锁定文件
    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/files/{file_id}/lock"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "locked": true }))
        .to_request();
    test::call_service(&app, req).await;

    // admin 列出锁
    let req = test::TestRequest::get()
        .uri("/api/v1/admin/locks")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let locks = body["data"]["items"].as_array().unwrap();
    assert_eq!(body["data"]["total"], 1);
    assert_eq!(locks.len(), 1);
    let lock_id = locks[0]["id"].as_i64().unwrap();

    // admin 强制解锁
    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/admin/locks/{lock_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    // 文件应该可以删除了
    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/files/{file_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn test_admin_batch_update_user() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    // 创建普通用户
    let req = test::TestRequest::post()
        .uri("/api/v1/auth/register")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "username": "batchuser",
            "email": "batchuser@example.com",
            "password": "password123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let user_id = body["data"]["id"].as_i64().unwrap();
    assert_eq!(body["data"]["role"], "user");
    assert_eq!(body["data"]["status"], "active");
    assert_eq!(body["data"]["storage_quota"], 0);
    assert_eq!(body["data"]["email_verified"], false);

    // 单次 PATCH 同时更新 email_verified + role + status + storage_quota
    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/admin/users/{user_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "email_verified": true,
            "role": "admin",
            "status": "disabled",
            "storage_quota": 1073741824
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let user = &body["data"];
    assert_eq!(user["email_verified"], true);
    assert_eq!(user["role"], "admin");
    assert_eq!(user["status"], "disabled");
    assert_eq!(user["storage_quota"], 1073741824);

    // 验证 GET 也返回更新后的值
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/admin/users/{user_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["email_verified"], true);
    assert_eq!(body["data"]["role"], "admin");
    assert_eq!(body["data"]["status"], "disabled");
    assert_eq!(body["data"]["storage_quota"], 1073741824);
}

#[actix_web::test]
async fn test_admin_cannot_unverify_initial_admin() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::patch()
        .uri("/api/v1/admin/users/1")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "email_verified": false
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["msg"], "cannot unverify the initial admin account");
}

#[actix_web::test]
async fn test_admin_cannot_demote_initial_admin() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::patch()
        .uri("/api/v1/admin/users/1")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "role": "user"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["msg"], "cannot demote the initial admin account");
}

#[actix_web::test]
async fn test_admin_cannot_disable_initial_admin() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::patch()
        .uri("/api/v1/admin/users/1")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "status": "disabled"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["msg"], "cannot disable the initial admin account");
}

#[actix_web::test]
async fn test_admin_can_reset_user_password() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (admin_token, _) = register_and_login!(app);

    let user_id = admin_create_user!(
        app,
        admin_token,
        "resetuser",
        "resetuser@example.com",
        "password123"
    );
    let (old_access, old_refresh) = login_user!(app, "resetuser", "password123");

    let req = test::TestRequest::put()
        .uri(&format!("/api/v1/admin/users/{user_id}/password"))
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({
            "password": "resetpass789"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri("/api/v1/auth/me")
        .insert_header(("Cookie", common::access_cookie_header(&old_access)))
        .insert_header(common::csrf_header_for(&old_access))
        .to_request();
    let result = test::try_call_service(&app, req).await;
    match result {
        Ok(resp) => assert_eq!(resp.status(), 401),
        Err(err) => {
            let resp = err.error_response();
            assert_eq!(resp.status(), 401);
        }
    }

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/refresh")
        .insert_header(("Cookie", common::refresh_cookie_header(&old_refresh)))
        .insert_header(common::csrf_header_for(&old_refresh))
        .to_request();
    let result = test::try_call_service(&app, req).await;
    match result {
        Ok(resp) => assert_eq!(resp.status(), 401),
        Err(err) => {
            let resp = err.error_response();
            assert_eq!(resp.status(), 401);
        }
    }

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/login")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "identifier": "resetuser",
            "password": "password123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 401);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/login")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "identifier": "resetuser",
            "password": "resetpass789"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn test_admin_can_revoke_user_sessions() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (admin_token, _) = register_and_login!(app);

    let user_id = admin_create_user!(
        app,
        admin_token,
        "revokeuser",
        "revokeuser@example.com",
        "password123"
    );
    let (user_access, user_refresh) = login_user!(app, "revokeuser", "password123");

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/admin/users/{user_id}/sessions/revoke"))
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri("/api/v1/auth/me")
        .insert_header(("Cookie", common::access_cookie_header(&user_access)))
        .insert_header(common::csrf_header_for(&user_access))
        .to_request();
    assert_service_status!(app, req, 401);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/refresh")
        .insert_header(("Cookie", common::refresh_cookie_header(&user_refresh)))
        .insert_header(common::csrf_header_for(&user_refresh))
        .to_request();
    assert_service_status!(app, req, 401);
}

#[actix_web::test]
async fn test_admin_role_change_removes_admin_access_without_revoking_session() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (admin_token, _) = register_and_login!(app);

    let user_id = admin_create_user!(
        app,
        admin_token,
        "managedadmin",
        "managedadmin@example.com",
        "password123"
    );

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/admin/users/{user_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({ "role": "admin" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let (elevated_access, elevated_refresh) = login_user!(app, "managedadmin", "password123");

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/config/schema")
        .insert_header(("Cookie", common::access_cookie_header(&elevated_access)))
        .insert_header(common::csrf_header_for(&elevated_access))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/admin/users/{user_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({ "role": "user" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri("/api/v1/auth/me")
        .insert_header(("Cookie", common::access_cookie_header(&elevated_access)))
        .insert_header(common::csrf_header_for(&elevated_access))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/refresh")
        .insert_header(("Cookie", common::refresh_cookie_header(&elevated_refresh)))
        .insert_header(common::csrf_header_for(&elevated_refresh))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let rotated_access = common::extract_cookie(&resp, "aster_access").unwrap();

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/config/schema")
        .insert_header(("Cookie", common::access_cookie_header(&elevated_access)))
        .insert_header(common::csrf_header_for(&elevated_access))
        .to_request();
    assert_service_status!(app, req, 403);

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/config/schema")
        .insert_header(("Cookie", common::access_cookie_header(&rotated_access)))
        .insert_header(common::csrf_header_for(&rotated_access))
        .to_request();
    assert_service_status!(app, req, 403);
}

#[actix_web::test]
async fn test_non_admin_cannot_reset_user_password() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (admin_token, _) = register_and_login!(app);

    let user_id = admin_create_user!(
        app,
        admin_token,
        "victimreset",
        "victimreset@example.com",
        "password123"
    );
    admin_create_user!(
        app,
        admin_token,
        "plainuser",
        "plainuser@example.com",
        "password123"
    );
    let (user_token, _) = login_user!(app, "plainuser", "password123");

    let req = test::TestRequest::put()
        .uri(&format!("/api/v1/admin/users/{user_id}/password"))
        .insert_header(("Cookie", common::access_cookie_header(&user_token)))
        .insert_header(common::csrf_header_for(&user_token))
        .set_json(serde_json::json!({
            "password": "resetpass789"
        }))
        .to_request();
    let err = test::try_call_service(&app, req).await.unwrap_err();
    let resp = err.error_response();
    assert_eq!(resp.status(), 403);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/login")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "identifier": "victimreset",
            "password": "password123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/users")
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn test_admin_update_user_rejects_negative_storage_quota() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::patch()
        .uri("/api/v1/admin/users/1")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "storage_quota": -1
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["msg"], "storage_quota must be non-negative");
}
