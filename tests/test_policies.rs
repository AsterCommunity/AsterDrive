//! 存储策略管理测试

#[macro_use]
mod common;

use actix_web::test;
use chrono::{Duration, Utc};
use serde_json::Value;

struct PolicyUploadSessionSpec<'a> {
    upload_id: &'a str,
    policy_id: i64,
    user_id: i64,
    s3_temp_key: Option<&'a str>,
}

async fn create_policy_upload_session(
    state: &aster_drive::runtime::PrimaryAppState,
    spec: PolicyUploadSessionSpec<'_>,
) {
    use aster_drive::db::repository::upload_session_repo;
    use aster_drive::types::UploadSessionStatus;
    use sea_orm::Set;

    let now = Utc::now();
    upload_session_repo::create(
        &state.db,
        aster_drive::entities::upload_session::ActiveModel {
            id: Set(spec.upload_id.to_string()),
            user_id: Set(spec.user_id),
            team_id: Set(None),
            filename: Set("pending-policy-upload.bin".to_string()),
            total_size: Set(10),
            chunk_size: Set(5),
            total_chunks: Set(2),
            received_count: Set(1),
            folder_id: Set(None),
            policy_id: Set(spec.policy_id),
            status: Set(UploadSessionStatus::Uploading),
            s3_temp_key: Set(spec.s3_temp_key.map(str::to_string)),
            s3_multipart_id: Set(None),
            file_id: Set(None),
            created_at: Set(now),
            expires_at: Set(now + Duration::hours(1)),
            updated_at: Set(now),
        },
    )
    .await
    .unwrap();
}

#[actix_web::test]
async fn test_user_default_policy_switch_updates_snapshot_immediately() {
    use aster_drive::services::{auth_service, file_service, policy_service, user_service};
    use aster_drive::types::DriverType;

    let state = common::setup().await;
    let user = auth_service::register(
        &state,
        "policysnapsw",
        "policy-snapshot-switch@example.com",
        "password123",
    )
    .await
    .unwrap();

    let initial_policy = file_service::resolve_policy(&state, user.id, None)
        .await
        .unwrap();

    let alternate_base_path = format!("/tmp/asterdrive-policy-switch-{}", uuid::Uuid::new_v4());
    std::fs::create_dir_all(&alternate_base_path).unwrap();
    let alternate_policy = policy_service::create(
        &state,
        policy_service::CreateStoragePolicyInput {
            name: "Alternate Local".to_string(),
            connection: policy_service::StoragePolicyConnectionInput {
                driver_type: DriverType::Local,
                endpoint: String::new(),
                bucket: String::new(),
                access_key: String::new(),
                secret_key: String::new(),
                base_path: alternate_base_path.clone(),
                remote_node_id: None,
            },
            max_file_size: 0,
            chunk_size: None,
            is_default: false,
            allowed_types: None,
            options: None,
        },
    )
    .await
    .unwrap();

    assert_ne!(alternate_policy.id, initial_policy.id);

    let alternate_group = policy_service::create_group(
        &state,
        policy_service::CreateStoragePolicyGroupInput {
            name: "Alternate Group".to_string(),
            description: Some("Snapshot switch target".to_string()),
            is_enabled: true,
            is_default: false,
            items: vec![policy_service::StoragePolicyGroupItemInput {
                policy_id: alternate_policy.id,
                priority: 1,
                min_file_size: 0,
                max_file_size: 0,
            }],
        },
    )
    .await
    .unwrap();

    user_service::update(
        &state,
        user_service::UpdateUserInput {
            id: user.id,
            email_verified: None,
            role: None,
            status: None,
            storage_quota: None,
            policy_group_id: Some(alternate_group.id),
        },
    )
    .await
    .unwrap();

    assert_eq!(
        state.policy_snapshot.resolve_default_policy_id(user.id),
        Some(alternate_policy.id)
    );

    let resolved_after_switch = file_service::resolve_policy(&state, user.id, None)
        .await
        .unwrap();
    assert_eq!(resolved_after_switch.id, alternate_policy.id);
}

#[actix_web::test]
async fn test_seed_policy_groups_backfills_missing_users_to_default_group() {
    use aster_drive::db::repository::{policy_group_repo, user_repo};
    use aster_drive::services::{auth_service, policy_service};
    use sea_orm::{ActiveModelTrait, Set};

    let state = common::setup().await;
    let user = auth_service::register(
        &state,
        "policybackfill",
        "policy-backfill@example.com",
        "password123",
    )
    .await
    .unwrap();
    let default_group = policy_group_repo::find_default_group(&state.db)
        .await
        .unwrap()
        .expect("default group should exist");

    let mut user_active: aster_drive::entities::user::ActiveModel = user.into();
    user_active.policy_group_id = Set(None);
    user_active.update(&state.db).await.unwrap();

    policy_service::ensure_policy_groups_seeded(&state.db)
        .await
        .unwrap();

    let updated = user_repo::find_by_email(&state.db, "policy-backfill@example.com")
        .await
        .unwrap()
        .expect("user should exist");
    assert_eq!(updated.policy_group_id, Some(default_group.id));
}

#[actix_web::test]
async fn test_policy_crud() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    // 列出策略（应有 1 个默认）
    let req = test::TestRequest::get()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["items"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"]["total"], 1);

    // 创建新策略
    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Test S3",
            "driver_type": "s3",
            "endpoint": "http://localhost:9000",
            "bucket": "test-bucket",
            "access_key": "minioadmin",
            "secret_key": "minioadmin",
            "base_path": "",
            "max_file_size": 104857600,
            "chunk_size": 8388608,
            "options": serde_json::json!({
                "s3_upload_strategy": "presigned"
            })
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["name"], "Test S3");
    assert_eq!(body["data"]["chunk_size"], 8_388_608);
    assert_eq!(body["data"]["options"]["s3_upload_strategy"], "presigned");
    let policy_id = body["data"]["id"].as_i64().unwrap();

    // 获取单个
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/admin/policies/{policy_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    // 更新策略
    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/admin/policies/{policy_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Renamed S3" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["name"], "Renamed S3");

    // 删除策略
    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/admin/policies/{policy_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    // 只剩默认策略
    let req = test::TestRequest::get()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["items"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"]["total"], 1);
}

#[actix_web::test]
async fn test_policy_delete_rejects_upload_sessions_unless_forced() {
    use aster_drive::db::repository::{policy_repo, upload_session_repo};

    let state = common::setup().await;
    let db = state.db.clone();
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);

    let base_path = format!(
        "/tmp/asterdrive-policy-upload-session-{}",
        uuid::Uuid::new_v4()
    );
    std::fs::create_dir_all(&base_path).unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Session Guard Policy",
            "driver_type": "local",
            "base_path": base_path,
            "chunk_size": 5_242_880,
            "max_file_size": 0
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let policy_id = body["data"]["id"].as_i64().unwrap();

    let user = aster_drive::db::repository::user_repo::find_by_username(&db, "testuser")
        .await
        .unwrap()
        .expect("registered user should exist");
    let upload_id = uuid::Uuid::new_v4().to_string();
    let temp_dir = std::path::PathBuf::from(aster_drive::utils::paths::upload_temp_dir(
        &state.config.server.upload_temp_dir,
        &upload_id,
    ));
    tokio::fs::create_dir_all(&temp_dir).await.unwrap();
    tokio::fs::write(temp_dir.join("chunk-0"), b"partial")
        .await
        .unwrap();

    create_policy_upload_session(
        &state,
        PolicyUploadSessionSpec {
            upload_id: &upload_id,
            policy_id,
            user_id: user.id,
            s3_temp_key: None,
        },
    )
    .await;

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/admin/policies/{policy_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["msg"],
        "cannot delete policy: 1 upload session(s) still reference it"
    );

    assert!(
        policy_repo::find_by_id(&db, policy_id).await.is_ok(),
        "policy should remain after guarded delete"
    );
    assert!(
        upload_session_repo::find_by_id(&db, &upload_id)
            .await
            .is_ok(),
        "upload session should remain after guarded delete"
    );
    assert!(
        temp_dir.exists(),
        "local upload temp directory should remain after guarded delete"
    );

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/admin/policies/{policy_id}?force=true"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    assert!(
        policy_repo::find_by_id(&db, policy_id).await.is_err(),
        "policy should be deleted by forced delete"
    );
    assert!(
        upload_session_repo::find_by_id(&db, &upload_id)
            .await
            .is_err(),
        "forced delete should remove upload sessions"
    );
    assert!(
        !temp_dir.exists(),
        "forced delete should remove local upload temp directory"
    );
}

#[actix_web::test]
async fn test_policy_force_delete_schedules_late_temp_object_cleanup() {
    use aster_drive::db::repository::{background_task_repo, policy_repo, upload_session_repo};
    use aster_drive::entities::background_task;
    use aster_drive::services::task_service;
    use aster_drive::types::{BackgroundTaskKind, BackgroundTaskStatus};
    use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};

    let state = common::setup().await;
    let db = state.db.clone();
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);

    let base_path = format!(
        "/tmp/asterdrive-policy-late-temp-cleanup-{}",
        uuid::Uuid::new_v4()
    );
    std::fs::create_dir_all(&base_path).unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Late Temp Cleanup Policy",
            "driver_type": "local",
            "base_path": base_path,
            "chunk_size": 5_242_880,
            "max_file_size": 0
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let policy_id = body["data"]["id"].as_i64().unwrap();

    let user = aster_drive::db::repository::user_repo::find_by_username(&db, "testuser")
        .await
        .unwrap()
        .expect("registered user should exist");
    let upload_id = uuid::Uuid::new_v4().to_string();
    let temp_key = format!("files/late-orphan-{}.bin", uuid::Uuid::new_v4());
    create_policy_upload_session(
        &state,
        PolicyUploadSessionSpec {
            upload_id: &upload_id,
            policy_id,
            user_id: user.id,
            s3_temp_key: Some(&temp_key),
        },
    )
    .await;

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/admin/policies/{policy_id}?force=true"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    assert!(
        policy_repo::find_by_id(&db, policy_id).await.is_err(),
        "policy should be deleted by forced delete"
    );
    assert!(
        upload_session_repo::find_by_id(&db, &upload_id)
            .await
            .is_err(),
        "forced delete should remove upload session"
    );

    let cleanup_task = background_task::Entity::find()
        .filter(background_task::Column::Kind.eq(BackgroundTaskKind::StoragePolicyTempCleanup))
        .one(&db)
        .await
        .unwrap()
        .expect("force delete should schedule delayed temp cleanup");
    assert_eq!(cleanup_task.status, BackgroundTaskStatus::Pending);
    let payload: Value = serde_json::from_str(cleanup_task.payload_json.as_ref()).unwrap();
    assert_eq!(payload["policy"]["id"], policy_id);
    assert_eq!(payload["temp_keys"][0], temp_key);

    let object_path = std::path::Path::new(&base_path).join(&temp_key);
    tokio::fs::create_dir_all(object_path.parent().unwrap())
        .await
        .unwrap();
    tokio::fs::write(&object_path, b"late presigned write")
        .await
        .unwrap();
    assert!(
        object_path.exists(),
        "test should create the late orphan object after policy deletion"
    );

    let mut active: background_task::ActiveModel = cleanup_task.clone().into();
    active.next_run_at = Set(Utc::now() - Duration::seconds(1));
    active.update(&db).await.unwrap();

    let stats = task_service::dispatch_due(&state).await.unwrap();
    assert_eq!(stats.claimed, 1);
    assert_eq!(stats.succeeded, 1);
    assert!(
        !object_path.exists(),
        "delayed cleanup should delete late temp object using policy snapshot"
    );

    let stored_task = background_task_repo::find_by_id(&db, cleanup_task.id)
        .await
        .unwrap();
    assert_eq!(stored_task.status, BackgroundTaskStatus::Succeeded);
    let result: Value =
        serde_json::from_str(stored_task.result_json.as_ref().unwrap().as_ref()).unwrap();
    assert_eq!(result["deleted_objects"], 1);
    assert_eq!(result["failed_objects"], 0);
}

#[actix_web::test]
async fn test_policy_force_delete_still_rejects_blob_references() {
    use aster_drive::db::repository::{file_repo, policy_group_repo, policy_repo};
    use aster_drive::services::{file_service, policy_service, user_service};
    use aster_drive::types::DriverType;

    let state = common::setup().await;
    let db = state.db.clone();
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let user = aster_drive::db::repository::user_repo::find_by_username(&db, "testuser")
        .await
        .unwrap()
        .expect("registered user should exist");

    let base_path = format!("/tmp/asterdrive-policy-force-blob-{}", uuid::Uuid::new_v4());
    std::fs::create_dir_all(&base_path).unwrap();
    let policy = policy_service::create(
        &state,
        policy_service::CreateStoragePolicyInput {
            name: "Blob Guard Policy".to_string(),
            connection: policy_service::StoragePolicyConnectionInput {
                driver_type: DriverType::Local,
                endpoint: String::new(),
                bucket: String::new(),
                access_key: String::new(),
                secret_key: String::new(),
                base_path,
                remote_node_id: None,
            },
            max_file_size: 0,
            chunk_size: None,
            is_default: false,
            allowed_types: None,
            options: None,
        },
    )
    .await
    .unwrap();

    let group = policy_service::create_group(
        &state,
        policy_service::CreateStoragePolicyGroupInput {
            name: "Blob Guard Group".to_string(),
            description: None,
            is_enabled: true,
            is_default: false,
            items: vec![policy_service::StoragePolicyGroupItemInput {
                policy_id: policy.id,
                priority: 1,
                min_file_size: 0,
                max_file_size: 0,
            }],
        },
    )
    .await
    .unwrap();

    user_service::update(
        &state,
        user_service::UpdateUserInput {
            id: user.id,
            email_verified: None,
            role: None,
            status: None,
            storage_quota: None,
            policy_group_id: Some(group.id),
        },
    )
    .await
    .unwrap();

    let temp_path = aster_drive::utils::paths::temp_file_path(
        &state.config.server.temp_dir,
        &uuid::Uuid::new_v4().to_string(),
    );
    tokio::fs::create_dir_all(&state.config.server.temp_dir)
        .await
        .unwrap();
    tokio::fs::write(&temp_path, b"blob reference")
        .await
        .unwrap();
    let file = file_service::store_from_temp(
        &state,
        user.id,
        file_service::StoreFromTempRequest::new(
            None,
            "blob-reference.txt",
            &temp_path,
            b"blob reference".len() as i64,
        ),
    )
    .await
    .unwrap();
    let blob = file_repo::find_blob_by_id(&db, file.blob_id).await.unwrap();
    assert_eq!(blob.policy_id, policy.id);

    let default_group = policy_group_repo::find_default_group(&db)
        .await
        .unwrap()
        .expect("default policy group should exist");
    user_service::update(
        &state,
        user_service::UpdateUserInput {
            id: user.id,
            email_verified: None,
            role: None,
            status: None,
            storage_quota: None,
            policy_group_id: Some(default_group.id),
        },
    )
    .await
    .unwrap();

    policy_service::delete_group(&state, group.id)
        .await
        .unwrap();

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/admin/policies/{}?force=true", policy.id))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["msg"],
        "cannot delete policy: 1 blob(s) still reference it"
    );
    assert!(
        policy_repo::find_by_id(&db, policy.id).await.is_ok(),
        "force must not delete a policy referenced by blobs"
    );
}

#[actix_web::test]
async fn test_policy_rejects_storage_native_thumbnail_for_unsupported_driver() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Native Thumbnail Local",
            "driver_type": "local",
            "base_path": "/tmp/test-native-thumbnail-local",
            "max_file_size": 0,
            "is_default": false,
            "options": {
                "thumbnail_processor": "storage_native",
                "thumbnail_extensions": ["png", ".jpg"]
            }
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);

    let body: Value = test::read_body_json(resp).await;
    assert!(
        body["msg"]
            .as_str()
            .unwrap_or_default()
            .contains("does not expose storage-native thumbnail processing")
    );
}

#[actix_web::test]
async fn test_user_policy_assignment() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    // 获取默认策略 ID
    let req = test::TestRequest::get()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let policy_id = body["data"]["items"][0]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policy-groups")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Dedicated User Group",
            "description": "Single binding target",
            "is_enabled": true,
            "is_default": false,
            "items": [
                {
                    "policy_id": policy_id,
                    "priority": 1,
                    "min_file_size": 0,
                    "max_file_size": 0
                }
            ]
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let group_id = body["data"]["id"].as_i64().unwrap();

    // 获取用户 ID
    let req = test::TestRequest::get()
        .uri("/api/v1/admin/users")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let user_id = body["data"]["items"][0]["id"].as_i64().unwrap();

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/admin/users/{user_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "policy_group_id": group_id
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["policy_group_id"], group_id);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/admin/users/{user_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["policy_group_id"], group_id);

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/admin/users/{user_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "policy_group_id": serde_json::Value::Null
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
}

// ── 系统策略 default 唯一性 ─────────────────────────────────

#[actix_web::test]
async fn test_system_policy_default_uniqueness() {
    use aster_drive::db::repository::policy_group_repo;

    let state = common::setup().await;
    let db = state.db.clone();
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    // 创建第二个策略并设为 default
    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "New Default",
            "driver_type": "local",
            "base_path": "/tmp/test-new-default",
            "max_file_size": 0,
            "is_default": true
        }))
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let new_default_id = body["data"]["id"].as_i64().unwrap();

    // 列出所有策略，应只有一个 is_default=true
    let req = test::TestRequest::get()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let policies = body["data"]["items"].as_array().unwrap();
    let default_count = policies.iter().filter(|p| p["is_default"] == true).count();
    assert_eq!(
        default_count, 1,
        "should have exactly 1 default policy, got {default_count}"
    );

    let default_group = policy_group_repo::find_default_group(&db)
        .await
        .unwrap()
        .expect("default group should exist");
    let items = policy_group_repo::find_group_items(&db, default_group.id)
        .await
        .unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].policy_id, new_default_id);
}

#[actix_web::test]
async fn test_patch_policy_promotes_existing_policy_to_default() {
    use aster_drive::db::repository::policy_group_repo;

    let state = common::setup().await;
    let db = state.db.clone();
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Patch To Default",
            "driver_type": "local",
            "base_path": "/tmp/test-patch-default",
            "max_file_size": 0,
            "is_default": false
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let policy_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/admin/policies/{policy_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "is_default": true }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["is_default"], true);

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let policies = body["data"]["items"].as_array().unwrap();
    let default_ids: Vec<i64> = policies
        .iter()
        .filter(|policy| policy["is_default"] == true)
        .map(|policy| policy["id"].as_i64().unwrap())
        .collect();

    assert_eq!(default_ids, vec![policy_id]);

    let default_group = policy_group_repo::find_default_group(&db)
        .await
        .unwrap()
        .expect("default group should exist");
    let items = policy_group_repo::find_group_items(&db, default_group.id)
        .await
        .unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].policy_id, policy_id);
}

#[actix_web::test]
async fn test_set_only_default_rejects_missing_policy_without_clearing_default() {
    use aster_drive::db::repository::policy_repo;

    let state = common::setup().await;
    let original_default = policy_repo::find_default(&state.db)
        .await
        .unwrap()
        .expect("default policy should exist");

    let err = policy_repo::set_only_default(&state.db, i64::MAX)
        .await
        .unwrap_err();
    assert!(err.message().contains("policy"));

    let current_default = policy_repo::find_default(&state.db)
        .await
        .unwrap()
        .expect("default policy should still exist");
    assert_eq!(current_default.id, original_default.id);
}

#[actix_web::test]
async fn test_cannot_disable_default_policy_group() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/policy-groups")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let groups = body["data"]["items"]
        .as_array()
        .expect("policy group list should be an array");
    let group_id = groups
        .iter()
        .find(|item| item["is_default"].as_bool() == Some(true))
        .and_then(|item| item["id"].as_i64())
        .expect("default policy group should exist in list");

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/admin/policy-groups/{group_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "is_enabled": false }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["msg"],
        "cannot disable the default storage policy group; set another group as default first"
    );
}

#[actix_web::test]
async fn test_policy_groups_are_sorted_by_created_at_desc() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let policy_id = body["data"]["items"][0]["id"].as_i64().unwrap();

    for group_name in ["First Group", "Second Group"] {
        let req = test::TestRequest::post()
            .uri("/api/v1/admin/policy-groups")
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .set_json(serde_json::json!({
                "name": group_name,
                "description": format!("{group_name} description"),
                "is_enabled": true,
                "is_default": false,
                "items": [
                    {
                        "policy_id": policy_id,
                        "priority": 1,
                        "min_file_size": 0,
                        "max_file_size": 0
                    }
                ]
            }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201);
    }

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/policy-groups?limit=3&offset=0")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let groups = body["data"]["items"].as_array().unwrap();
    assert_eq!(body["data"]["total"], 3);
    assert_eq!(groups.len(), 3);
    assert_eq!(groups[0]["name"], "Second Group");
    assert_eq!(groups[1]["name"], "First Group");
}

#[actix_web::test]
async fn test_cannot_disable_assigned_policy_group() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let policy_id = body["data"]["items"][0]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policy-groups")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Assigned Group",
            "description": "Used by one user",
            "is_enabled": true,
            "is_default": false,
            "items": [
                {
                    "policy_id": policy_id,
                    "priority": 1,
                    "min_file_size": 0,
                    "max_file_size": 0
                }
            ]
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let group_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/users")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let user_id = body["data"]["items"][0]["id"].as_i64().unwrap();

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/admin/users/{user_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "policy_group_id": group_id
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/admin/policy-groups/{group_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "is_enabled": false }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["msg"],
        "cannot disable policy group: 1 user assignment(s) still reference it"
    );
}

#[actix_web::test]
async fn test_cannot_assign_disabled_policy_group_to_user() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let policy_id = body["data"]["items"][0]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policy-groups")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Legacy Disabled Group",
            "description": "Disabled after assignment",
            "is_enabled": true,
            "is_default": false,
            "items": [
                {
                    "policy_id": policy_id,
                    "priority": 1,
                    "min_file_size": 0,
                    "max_file_size": 0
                }
            ]
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let group_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/admin/policy-groups/{group_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "is_enabled": false }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/users")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let user_id = body["data"]["items"][0]["id"].as_i64().unwrap();

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/admin/users/{user_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "policy_group_id": group_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["msg"], "cannot assign a disabled storage policy group");
}

#[actix_web::test]
async fn test_cannot_disable_or_delete_policy_group_assigned_to_team() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let policy_id = body["data"]["items"][0]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policy-groups")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Team Bound Group",
            "description": "Referenced by a team",
            "is_enabled": true,
            "is_default": false,
            "items": [
                {
                    "policy_id": policy_id,
                    "priority": 1,
                    "min_file_size": 0,
                    "max_file_size": 0
                }
            ]
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let group_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/register")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "username": "teampolicyadmin",
            "email": "teampolicyadmin@example.com",
            "password": "password123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/teams")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Policy Bound Team",
            "admin_identifier": "teampolicyadmin",
            "policy_group_id": group_id
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/admin/policy-groups/{group_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "is_enabled": false }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["msg"],
        "cannot disable policy group: 1 team assignment(s) still reference it"
    );

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/admin/policy-groups/{group_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["msg"],
        "cannot delete policy group: 1 team assignment(s) still reference it"
    );
}

#[actix_web::test]
async fn test_migrate_policy_group_users_moves_assignments_and_preserves_default() {
    use aster_drive::services::auth_service;

    let state = common::setup().await;
    let admin_user = auth_service::register(
        &state,
        "pgmigrate-admin",
        "pgmigrate-admin@example.com",
        "password123",
    )
    .await
    .unwrap();
    let user_with_source_only = auth_service::register(
        &state,
        "pgmigrate1",
        "pgmigrate1@example.com",
        "password123",
    )
    .await
    .unwrap();
    let user_with_existing_target = auth_service::register(
        &state,
        "pgmigrate2",
        "pgmigrate2@example.com",
        "password123",
    )
    .await
    .unwrap();
    let app = create_test_app!(state);
    let req = test::TestRequest::post()
        .uri("/api/v1/auth/login")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "identifier": admin_user.username,
            "password": "password123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let token = common::extract_cookie(&resp, "aster_access").unwrap();

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let policy_id = body["data"]["items"][0]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policy-groups")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Source Group",
            "description": "Users will be migrated away",
            "is_enabled": true,
            "is_default": false,
            "items": [
                {
                    "policy_id": policy_id,
                    "priority": 1,
                    "min_file_size": 0,
                    "max_file_size": 0
                }
            ]
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let source_group_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policy-groups")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Target Group",
            "description": "Users land here after migration",
            "is_enabled": true,
            "is_default": false,
            "items": [
                {
                    "policy_id": policy_id,
                    "priority": 1,
                    "min_file_size": 0,
                    "max_file_size": 0
                }
            ]
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let target_group_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/admin/users/{}", user_with_source_only.id))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "policy_group_id": source_group_id
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::patch()
        .uri(&format!(
            "/api/v1/admin/users/{}",
            user_with_existing_target.id
        ))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "policy_group_id": target_group_id
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/v1/admin/policy-groups/{source_group_id}/migrate-users"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "target_group_id": target_group_id
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["source_group_id"], source_group_id);
    assert_eq!(body["data"]["target_group_id"], target_group_id);
    assert_eq!(body["data"]["affected_users"], 1);
    assert_eq!(body["data"]["migrated_assignments"], 1);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/admin/users/{}", user_with_source_only.id))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["policy_group_id"], target_group_id);

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/admin/users/{}",
            user_with_existing_target.id
        ))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["policy_group_id"], target_group_id);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/admin/policy-groups/{source_group_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["is_default"], false);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/admin/policy-groups/{target_group_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["is_default"], false);
}

#[actix_web::test]
async fn test_cannot_migrate_policy_group_users_to_disabled_group() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let policy_id = body["data"]["items"][0]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policy-groups")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Migration Source",
            "description": "source",
            "is_enabled": true,
            "is_default": false,
            "items": [
                {
                    "policy_id": policy_id,
                    "priority": 1,
                    "min_file_size": 0,
                    "max_file_size": 0
                }
            ]
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let source_group_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policy-groups")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Disabled Target",
            "description": "target",
            "is_enabled": false,
            "is_default": false,
            "items": [
                {
                    "policy_id": policy_id,
                    "priority": 1,
                    "min_file_size": 0,
                    "max_file_size": 0
                }
            ]
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let target_group_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/v1/admin/policy-groups/{source_group_id}/migrate-users"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "target_group_id": target_group_id
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["msg"],
        "cannot migrate users to a disabled storage policy group"
    );
}

// ── 不能删除唯一的默认系统策略 ──────────────────────────────

#[actix_web::test]
async fn test_cannot_delete_only_default_policy() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    // 获取默认策略 ID
    let req = test::TestRequest::get()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let policy_id = body["data"]["items"][0]["id"].as_i64().unwrap();

    // 尝试删除唯一默认策略 → 应被拒绝
    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/admin/policies/{policy_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        400,
        "should reject deleting only default policy, got {}",
        resp.status()
    );
}

#[actix_web::test]
async fn test_cannot_delete_builtin_system_policy_even_after_switching_default() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let initial_policies = body["data"]["items"].as_array().unwrap();
    assert_eq!(
        initial_policies.len(),
        1,
        "fresh setup should contain exactly one built-in policy"
    );
    let built_in_policy_id = initial_policies[0]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Replacement Default",
            "driver_type": "local",
            "base_path": format!("/tmp/test-replacement-default-{}", uuid::Uuid::new_v4()),
            "max_file_size": 0,
            "is_default": true
        }))
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/admin/policies/{built_in_policy_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        400,
        "should reject deleting built-in policy #{built_in_policy_id}, got {}",
        resp.status()
    );

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let policies = body["data"]["items"].as_array().unwrap();
    assert!(
        policies
            .iter()
            .any(|policy| policy["id"].as_i64() == Some(built_in_policy_id)),
        "built-in policy #{built_in_policy_id} should still exist after failed delete"
    );
}

// ── 不能取消唯一的默认系统策略 ──────────────────────────────

#[actix_web::test]
async fn test_cannot_unset_only_default_policy() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    // 获取默认策略 ID
    let req = test::TestRequest::get()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let policy_id = body["data"]["items"][0]["id"].as_i64().unwrap();

    // 尝试取消 default → 应被拒绝
    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/admin/policies/{policy_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({"is_default": false}))
        .to_request();
    let resp: actix_web::dev::ServiceResponse = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        400,
        "should reject unsetting only default, got {}",
        resp.status()
    );
}

// ── 用户绑定策略组的运行时校验 ─────────────────────────────

#[actix_web::test]
async fn test_resolve_policy_fails_without_user_policy_group() {
    use aster_drive::db::repository::user_repo;
    use aster_drive::services::{auth_service, file_service};
    use sea_orm::{ActiveModelTrait, Set};

    let state = common::setup().await;
    let user = auth_service::register(
        &state,
        "nogroup-user",
        "nogroup-user@example.com",
        "password123",
    )
    .await
    .unwrap();

    let model = user_repo::find_by_id(&state.db, user.id).await.unwrap();
    let mut active: aster_drive::entities::user::ActiveModel = model.into();
    active.policy_group_id = Set(None);
    active.updated_at = Set(chrono::Utc::now());
    active.update(&state.db).await.unwrap();
    state.policy_snapshot.reload(&state.db).await.unwrap();

    let err = file_service::resolve_policy(&state, user.id, None)
        .await
        .unwrap_err();
    assert_eq!(err.code(), "E030");
    assert!(err.message().contains("no storage policy group assigned"));
}

#[actix_web::test]
async fn test_resolve_policy_fails_for_disabled_assigned_policy_group() {
    use aster_drive::db::repository::{policy_group_repo, user_repo};
    use aster_drive::services::{auth_service, file_service};
    use sea_orm::{ActiveModelTrait, Set};

    let state = common::setup().await;
    let user = auth_service::register(
        &state,
        "disabledgrpusr",
        "disabled-group-user@example.com",
        "password123",
    )
    .await
    .unwrap();

    let default_policy = aster_drive::db::repository::policy_repo::find_default(&state.db)
        .await
        .unwrap()
        .unwrap();
    let now = chrono::Utc::now();
    let group = policy_group_repo::create_group(
        &state.db,
        aster_drive::entities::storage_policy_group::ActiveModel {
            name: Set("Disabled Assigned Group".to_string()),
            description: Set(String::new()),
            is_enabled: Set(true),
            is_default: Set(false),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    policy_group_repo::create_group_item(
        &state.db,
        aster_drive::entities::storage_policy_group_item::ActiveModel {
            group_id: Set(group.id),
            policy_id: Set(default_policy.id),
            priority: Set(1),
            min_file_size: Set(0),
            max_file_size: Set(0),
            created_at: Set(now),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let user_model = user_repo::find_by_id(&state.db, user.id).await.unwrap();
    let mut user_active: aster_drive::entities::user::ActiveModel = user_model.into();
    user_active.policy_group_id = Set(Some(group.id));
    user_active.updated_at = Set(chrono::Utc::now());
    user_active.update(&state.db).await.unwrap();

    let group_model = policy_group_repo::find_group_by_id(&state.db, group.id)
        .await
        .unwrap();
    let mut group_active: aster_drive::entities::storage_policy_group::ActiveModel =
        group_model.into();
    group_active.is_enabled = Set(false);
    group_active.updated_at = Set(chrono::Utc::now());
    group_active.update(&state.db).await.unwrap();

    state.policy_snapshot.reload(&state.db).await.unwrap();

    let err = file_service::resolve_policy(&state, user.id, None)
        .await
        .unwrap_err();
    assert_eq!(err.code(), "E005");
    assert!(err.message().contains("is disabled"));
}

#[actix_web::test]
async fn test_resolve_policy_fails_when_policy_group_has_no_matching_rule() {
    use aster_drive::db::repository::{policy_group_repo, policy_repo, user_repo};
    use aster_drive::services::{auth_service, file_service, policy_service};
    use aster_drive::types::DriverType;
    use sea_orm::{ActiveModelTrait, Set};

    let state = common::setup().await;
    let user = auth_service::register(
        &state,
        "gappolicyuser",
        "gap-policy-user@example.com",
        "password123",
    )
    .await
    .unwrap();

    let default_policy = policy_repo::find_default(&state.db).await.unwrap().unwrap();
    let overflow_path = format!("/tmp/asterdrive-gap-policy-{}", uuid::Uuid::new_v4());
    std::fs::create_dir_all(&overflow_path).unwrap();
    let overflow_policy = policy_service::create(
        &state,
        policy_service::CreateStoragePolicyInput {
            name: "Gap Overflow Policy".to_string(),
            connection: policy_service::StoragePolicyConnectionInput {
                driver_type: DriverType::Local,
                endpoint: String::new(),
                bucket: String::new(),
                access_key: String::new(),
                secret_key: String::new(),
                base_path: overflow_path.clone(),
                remote_node_id: None,
            },
            max_file_size: 0,
            chunk_size: None,
            is_default: false,
            allowed_types: None,
            options: None,
        },
    )
    .await
    .unwrap();

    let now = chrono::Utc::now();
    let group = policy_group_repo::create_group(
        &state.db,
        aster_drive::entities::storage_policy_group::ActiveModel {
            name: Set("Gap Rule Group".to_string()),
            description: Set(String::new()),
            is_enabled: Set(true),
            is_default: Set(false),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    for (priority, policy_id, min_file_size, max_file_size) in [
        (1, default_policy.id, 0, 10),
        (2, overflow_policy.id, 20, 0),
    ] {
        policy_group_repo::create_group_item(
            &state.db,
            aster_drive::entities::storage_policy_group_item::ActiveModel {
                group_id: Set(group.id),
                policy_id: Set(policy_id),
                priority: Set(priority),
                min_file_size: Set(min_file_size),
                max_file_size: Set(max_file_size),
                created_at: Set(now),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    }

    let user_model = user_repo::find_by_id(&state.db, user.id).await.unwrap();
    let mut user_active: aster_drive::entities::user::ActiveModel = user_model.into();
    user_active.policy_group_id = Set(Some(group.id));
    user_active.updated_at = Set(now);
    user_active.update(&state.db).await.unwrap();
    state.policy_snapshot.reload(&state.db).await.unwrap();

    let err = file_service::resolve_policy_for_size(&state, user.id, None, 15)
        .await
        .unwrap_err();
    assert_eq!(err.code(), "E005");
    assert!(err.message().contains("no storage policy rule"));
}

#[actix_web::test]
async fn test_policy_delete_clears_folder_policy_reference() {
    use aster_drive::db::repository::folder_repo;

    let state = common::setup().await;
    let db = state.db.clone();
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Folder Override Policy",
            "driver_type": "local",
            "base_path": "/tmp/test-folder-override-policy",
            "max_file_size": 0,
            "is_default": false
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let policy_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "override-folder" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let folder_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/folders/{folder_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "policy_id": policy_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let folder = folder_repo::find_by_id(&db, folder_id).await.unwrap();
    assert_eq!(folder.policy_id, Some(policy_id));

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/admin/policies/{policy_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let folder = folder_repo::find_by_id(&db, folder_id).await.unwrap();
    assert_eq!(folder.policy_id, None);
}

#[actix_web::test]
async fn test_folder_patch_can_clear_policy_with_null() {
    use aster_drive::db::repository::folder_repo;

    let state = common::setup().await;
    let db = state.db.clone();
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Nullable Folder Override Policy",
            "driver_type": "local",
            "base_path": "/tmp/test-nullable-folder-override-policy",
            "max_file_size": 0,
            "is_default": false
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let policy_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "nullable-override-folder" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let folder_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/folders/{folder_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "policy_id": policy_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let folder = folder_repo::find_by_id(&db, folder_id).await.unwrap();
    assert_eq!(folder.policy_id, Some(policy_id));

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/folders/{folder_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "policy_id": null }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert!(body["data"]["policy_id"].is_null());

    let folder = folder_repo::find_by_id(&db, folder_id).await.unwrap();
    assert_eq!(folder.policy_id, None);
}

#[actix_web::test]
async fn test_policy_connection_endpoints_for_local_driver() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let stored_base_path = format!("/tmp/test-policy-connection-{}", uuid::Uuid::new_v4());
    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Connection Test Policy",
            "driver_type": "local",
            "base_path": stored_base_path,
            "max_file_size": 0,
            "is_default": false
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let policy_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/admin/policies/{policy_id}/test"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    assert!(!std::path::Path::new(&format!("{stored_base_path}/_aster_connection_test")).exists());

    let temp_base_path = format!("/tmp/test-policy-params-{}", uuid::Uuid::new_v4());
    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies/test")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "driver_type": "local",
            "base_path": temp_base_path
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    assert!(!std::path::Path::new(&format!("{temp_base_path}/_aster_connection_test")).exists());
}

#[actix_web::test]
async fn test_policy_create_and_params_reject_incomplete_s3_credentials_as_bad_request() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Incomplete S3",
            "driver_type": "s3",
            "endpoint": "https://s3.example.com",
            "bucket": "archive"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], 1000);
    assert_eq!(
        body["msg"],
        "access_key is required for S3-compatible storage policies"
    );

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies/test")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "driver_type": "s3",
            "endpoint": "https://s3.example.com",
            "bucket": "archive"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], 1000);
    assert_eq!(
        body["msg"],
        "access_key is required for S3-compatible storage policies"
    );
}

#[actix_web::test]
async fn test_policy_update_rejects_clearing_existing_s3_secret_as_bad_request() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Valid S3",
            "driver_type": "s3",
            "endpoint": "https://s3.example.com",
            "bucket": "archive",
            "access_key": "AKIA",
            "secret_key": "SECRET"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let policy_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/admin/policies/{policy_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "secret_key": ""
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], 1000);
    assert_eq!(
        body["msg"],
        "secret_key is required for S3-compatible storage policies"
    );

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/admin/policies/{policy_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Still Valid S3"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}
