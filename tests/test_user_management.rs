//! 集成测试：`user_management`。

#[macro_use]
mod common;

use actix_web::test;
use sea_orm::EntityTrait;
use serde_json::Value;

fn avatar_upload_payload() -> (String, Vec<u8>) {
    let boundary = "----AsterAvatarBoundary".to_string();
    let image = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        8,
        8,
        image::Rgba([255, 120, 0, 255]),
    ));
    let mut png = std::io::Cursor::new(Vec::new());
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

// ── 策略删除保护：有 blob 引用则拒绝 ───────────────────────

#[actix_web::test]
async fn test_policy_delete_with_blobs_rejected() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    // 上传文件（会在默认策略创建 blob）
    let _file_id = upload_test_file!(app, token);

    // 获取策略 ID
    let req = test::TestRequest::get()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let policy_id = body["data"]["items"][0]["id"].as_i64().unwrap();

    // 尝试删除策略 → 应被拒绝（有 blob 引用）
    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/admin/policies/{policy_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        400,
        "should reject policy delete with blobs, got {}",
        resp.status()
    );
}

// ── 用户强制删除：级联清理所有数据 ─────────────────────────

#[actix_web::test]
async fn test_force_delete_user() {
    let state = common::setup().await;
    let avatar_base_path = state
        .runtime_config
        .get(aster_drive::config::avatar::AVATAR_DIR_KEY)
        .expect("avatar_dir should exist");
    let app = create_test_app!(state);

    // 注册第一个用户（admin，id=1）
    let (admin_token, _) = register_and_login!(app);

    let victim_id = admin_create_user!(
        app,
        admin_token,
        "victim",
        "victim@example.com",
        "password123"
    );
    let (victim_token, _) = login_user!(app, "victim", "password123");

    // 用第二个用户上传文件
    let _file_id = upload_test_file!(app, victim_token);

    // 用第二个用户上传头像
    let (boundary, payload) = avatar_upload_payload();
    let req = test::TestRequest::post()
        .uri("/api/v1/auth/profile/avatar/upload")
        .insert_header(("Cookie", common::access_cookie_header(&victim_token)))
        .insert_header(common::csrf_header_for(&victim_token))
        .insert_header((
            "Content-Type",
            format!("multipart/form-data; boundary={boundary}"),
        ))
        .set_payload(payload)
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let avatar_user_dir =
        std::path::PathBuf::from(&avatar_base_path).join(format!("user/{victim_id}"));
    let avatar_v1_dir = avatar_user_dir.join("v1");
    let avatar_512 = avatar_v1_dir.join("512.webp");
    let avatar_1024 = avatar_v1_dir.join("1024.webp");
    assert!(
        avatar_512.exists(),
        "avatar 512 should exist before force delete"
    );
    assert!(
        avatar_1024.exists(),
        "avatar 1024 should exist before force delete"
    );

    // 确认第二个用户可以在 admin 列表中看到
    let req = test::TestRequest::get()
        .uri("/api/v1/admin/users")
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let users = body["data"]["items"].as_array().unwrap();
    assert!(users.iter().any(|u| u["id"].as_i64() == Some(victim_id)));

    // admin 强制删除第二个用户
    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/admin/users/{victim_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        200,
        "force delete should succeed, got {}",
        resp.status()
    );

    // 确认用户不存在了
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/admin/users/{victim_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);
    assert!(
        !avatar_512.exists(),
        "avatar 512 should be deleted during force delete"
    );
    assert!(
        !avatar_1024.exists(),
        "avatar 1024 should be deleted during force delete"
    );
    assert!(
        !avatar_v1_dir.exists(),
        "avatar version dir should be deleted during force delete"
    );
    assert!(
        !avatar_user_dir.exists(),
        "avatar user dir should be deleted during force delete"
    );
}

#[actix_web::test]
async fn test_force_delete_user_preserves_team_upload_and_blob_ref() {
    let state = common::setup().await;
    let db = state.db.clone();
    let runtime_config = state.runtime_config.clone();
    let app = create_test_app!(state);

    let (admin_token, _) = register_and_login!(app);
    let victim_id = admin_create_user!(
        app,
        admin_token,
        "team-uploader",
        "team-uploader@example.com",
        "password123"
    );
    let (victim_token, _) = login_user!(app, "team-uploader", "password123");

    let req = test::TestRequest::post()
        .uri("/api/v1/teams")
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({ "name": "Force Delete Provenance" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let team_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/members"))
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({ "user_id": victim_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let boundary = "----ForceDeleteTeamBoundary";
    let payload = "------ForceDeleteTeamBoundary\r\n\
         Content-Disposition: form-data; name=\"file\"; filename=\"team-owned.txt\"\r\n\
         Content-Type: text/plain\r\n\r\n\
         still team data\r\n\
         ------ForceDeleteTeamBoundary--\r\n"
        .to_string();
    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/files/upload"))
        .insert_header(("Cookie", common::access_cookie_header(&victim_token)))
        .insert_header(common::csrf_header_for(&victim_token))
        .insert_header((
            "Content-Type",
            format!("multipart/form-data; boundary={boundary}"),
        ))
        .set_payload(payload)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let file_id = body["data"]["id"].as_i64().unwrap();

    let before_file = aster_drive::db::repository::file_repo::find_by_id(&db, file_id)
        .await
        .unwrap();
    assert_eq!(before_file.team_id, Some(team_id));
    assert_eq!(before_file.owner_user_id, None);
    assert_eq!(before_file.created_by_user_id, Some(victim_id));
    assert_eq!(before_file.created_by_username, "team-uploader");
    let blob_id = before_file.blob_id;
    let before_blob = aster_drive::db::repository::file_repo::find_blob_by_id(&db, blob_id)
        .await
        .unwrap();
    let before_ref_count = before_blob.ref_count;

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/admin/users/{victim_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    assert!(
        aster_drive::db::repository::user_repo::find_by_id(&db, victim_id)
            .await
            .is_err()
    );

    let after_file = aster_drive::db::repository::file_repo::find_by_id(&db, file_id)
        .await
        .unwrap();
    assert_eq!(after_file.team_id, Some(team_id));
    assert_eq!(after_file.owner_user_id, None);
    assert_eq!(after_file.created_by_user_id, None);
    assert_eq!(after_file.created_by_username, "team-uploader");

    let after_blob = aster_drive::db::repository::file_repo::find_blob_by_id(&db, blob_id)
        .await
        .unwrap();
    assert_eq!(after_blob.ref_count, before_ref_count);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/files/{file_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["owner_user_id"], Value::Null);
    assert_eq!(body["data"]["created_by_user_id"], Value::Null);
    assert_eq!(body["data"]["created_by_username"], "team-uploader");

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/files/{file_id}/download"))
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let status = resp.status();
    let body = test::read_body(resp).await;
    assert_eq!(status, 200, "{}", String::from_utf8_lossy(&body));
    assert_eq!(&body[..], b"still team data");

    let team_members = aster_drive::entities::team_member::Entity::find()
        .all(&db)
        .await
        .unwrap();
    assert!(
        team_members
            .iter()
            .all(|member| member.user_id != victim_id)
    );

    let mut max_versions =
        aster_drive::db::repository::config_repo::find_by_key(&db, "max_versions_per_file")
            .await
            .unwrap()
            .unwrap();
    max_versions.value = "1".to_string();
    runtime_config.apply(max_versions);

    let req = test::TestRequest::put()
        .uri(&format!("/api/v1/teams/{team_id}/files/{file_id}/content"))
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .insert_header(("Content-Type", "application/octet-stream"))
        .set_payload("team update one")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::put()
        .uri(&format!("/api/v1/teams/{team_id}/files/{file_id}/content"))
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .insert_header(("Content-Type", "application/octet-stream"))
        .set_payload("team update two")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let after_update_file = aster_drive::db::repository::file_repo::find_by_id(&db, file_id)
        .await
        .unwrap();
    assert_eq!(after_update_file.created_by_user_id, None);
    assert_eq!(after_update_file.created_by_username, "team-uploader");

    let versions = aster_drive::db::repository::version_repo::find_by_file_id(&db, file_id)
        .await
        .unwrap();
    assert_eq!(versions.len(), 1);
    assert_eq!(versions[0].size, "team update one".len() as i64);

    let after_update_storage = aster_drive::db::repository::team_repo::find_by_id(&db, team_id)
        .await
        .unwrap()
        .storage_used;
    assert_eq!(
        after_update_storage,
        "team update one".len() as i64 + "team update two".len() as i64
    );
}

#[actix_web::test]
async fn test_force_delete_user_with_gravatar_profile() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (admin_token, _) = register_and_login!(app);

    let victim_id = admin_create_user!(
        app,
        admin_token,
        "gravatar-victim",
        "gravatar-victim@example.com",
        "password123"
    );
    let (victim_token, _) = login_user!(app, "gravatar-victim", "password123");

    let req = test::TestRequest::put()
        .uri("/api/v1/auth/profile/avatar/source")
        .insert_header(("Cookie", common::access_cookie_header(&victim_token)))
        .insert_header(common::csrf_header_for(&victim_token))
        .set_json(serde_json::json!({
            "source": "gravatar"
        }))
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/admin/users/{victim_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn test_force_delete_user_tolerates_missing_avatar_object() {
    let state = common::setup().await;
    let avatar_base_path = state
        .runtime_config
        .get(aster_drive::config::avatar::AVATAR_DIR_KEY)
        .expect("avatar_dir should exist");
    let app = create_test_app!(state);
    let (admin_token, _) = register_and_login!(app);

    let victim_id = admin_create_user!(
        app,
        admin_token,
        "missavatar",
        "missavatar@example.com",
        "password123"
    );
    let (victim_token, _) = login_user!(app, "missavatar", "password123");

    let (boundary, payload) = avatar_upload_payload();
    let req = test::TestRequest::post()
        .uri("/api/v1/auth/profile/avatar/upload")
        .insert_header(("Cookie", common::access_cookie_header(&victim_token)))
        .insert_header(common::csrf_header_for(&victim_token))
        .insert_header((
            "Content-Type",
            format!("multipart/form-data; boundary={boundary}"),
        ))
        .set_payload(payload)
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let avatar_user_dir =
        std::path::PathBuf::from(&avatar_base_path).join(format!("user/{victim_id}"));
    let avatar_v1_dir = avatar_user_dir.join("v1");
    let avatar_512 = avatar_v1_dir.join("512.webp");
    let avatar_1024 = avatar_v1_dir.join("1024.webp");
    assert!(avatar_512.exists());
    assert!(avatar_1024.exists());

    std::fs::remove_file(&avatar_512).unwrap();
    assert!(!avatar_512.exists());
    assert!(avatar_1024.exists());

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/admin/users/{victim_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    assert!(!avatar_1024.exists());
    assert!(!avatar_v1_dir.exists());
    assert!(!avatar_user_dir.exists());
}

// ── 不能删除初始管理员 id=1 ────────────────────────────────

#[actix_web::test]
async fn test_admin_create_user_uses_default_quota_and_policy() {
    use aster_drive::db::repository::policy_group_repo;

    let state = common::setup().await;
    let expected_default_id = policy_group_repo::find_default_group(&state.db)
        .await
        .unwrap()
        .expect("default policy group should exist")
        .id;
    let app = create_test_app!(state);
    let (admin_token, _) = register_and_login!(app);

    let req = test::TestRequest::put()
        .uri("/api/v1/admin/config/default_storage_quota")
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({ "value": "1048576" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/users")
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "username": "quotauser",
            "email": "quotauser@example.com",
            "password": "password123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let user_id = body["data"]["id"].as_i64().unwrap();
    assert_eq!(body["data"]["storage_quota"], 1_048_576);
    assert!(user_id > 0);
    assert_eq!(
        body["data"]["policy_group_id"].as_i64().unwrap(),
        expected_default_id
    );
}

#[actix_web::test]
async fn test_cannot_delete_initial_admin() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (admin_token, _) = register_and_login!(app);

    // 尝试删除 id=1
    let req = test::TestRequest::delete()
        .uri("/api/v1/admin/users/1")
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        400,
        "should reject deleting initial admin, got {}",
        resp.status()
    );
}

// ── 不能删除 admin 角色用户 ────────────────────────────────

#[actix_web::test]
async fn test_cannot_delete_admin_role() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (admin_token, _) = register_and_login!(app);

    // 注册第二个用户
    let req = test::TestRequest::post()
        .uri("/api/v1/auth/register")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "username": "admin2",
            "email": "admin2@example.com",
            "password": "password123"
        }))
        .to_request();
    let _: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;

    // 获取 admin2 的 ID
    let req = test::TestRequest::get()
        .uri("/api/v1/admin/users")
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let users = body["data"]["items"].as_array().unwrap();
    let admin2_id = users.iter().find(|u| u["username"] == "admin2").unwrap()["id"]
        .as_i64()
        .unwrap();

    // 提升为 admin
    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/admin/users/{admin2_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({"role": "admin"}))
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    // 尝试删除 admin2 → 应被拒绝
    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/admin/users/{admin2_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        400,
        "should reject deleting admin role user, got {}",
        resp.status()
    );
}
