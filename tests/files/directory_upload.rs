//! 目录上传集成测试

use crate::common;

use actix_web::test;
use aster_drive::db::repository::folder_repo;
use aster_drive::services::{files::upload, workspace::team};
use aster_drive_model::entities::folder;
use aster_forge_utils::numbers::i64_to_usize;
use sea_orm::{ActiveModelTrait, Set};
use serde_json::Value;

#[actix_web::test]
async fn test_create_empty_with_relative_path_creates_nested_folders() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/files/new")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "ignored.txt",
            "relative_path": "docs/guides/empty.txt"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["name"], "empty.txt");
    assert_eq!(body["data"]["size"], 0);

    let req = test::TestRequest::get()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let docs_id = body["data"]["folders"][0]["id"].as_i64().unwrap();

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/folders/{docs_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let guides_id = body["data"]["folders"][0]["id"].as_i64().unwrap();

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/folders/{guides_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["files"][0]["name"], "empty.txt");
}

#[actix_web::test]
async fn test_create_empty_with_single_segment_relative_path_uses_exact_filename() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/files/new")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "ignored.txt",
            "relative_path": "root-empty.txt"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["name"], "root-empty.txt");
    assert_eq!(body["data"]["size"], 0);
}

#[actix_web::test]
async fn test_create_empty_with_invalid_relative_parent_leaves_no_partial_path() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/files/new")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "ignored.txt",
            "relative_path": "docs/CON/empty.txt"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);

    let req = test::TestRequest::get()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert!(body["data"]["folders"].as_array().unwrap().is_empty());
}

#[actix_web::test]
async fn test_direct_upload_with_relative_path_creates_nested_folders() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let boundary = "----DirUploadBoundary123";
    let payload = "------DirUploadBoundary123\r\n\
         Content-Disposition: form-data; name=\"file\"; filename=\"hello.txt\"\r\n\
         Content-Type: text/plain\r\n\r\n\
         hello nested world\r\n\
         ------DirUploadBoundary123--\r\n";
    let req = test::TestRequest::post()
        .uri("/api/v1/files/upload?relative_path=docs/guides/hello.txt")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .insert_header((
            "Content-Type",
            format!("multipart/form-data; boundary={boundary}"),
        ))
        .set_payload(payload)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["name"], "hello.txt");

    let req = test::TestRequest::get()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let docs_id = body["data"]["folders"]
        .as_array()
        .unwrap()
        .iter()
        .find(|folder| folder["name"] == "docs")
        .and_then(|folder| folder["id"].as_i64())
        .expect("docs folder should exist");

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/folders/{docs_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let guides_id = body["data"]["folders"]
        .as_array()
        .unwrap()
        .iter()
        .find(|folder| folder["name"] == "guides")
        .and_then(|folder| folder["id"].as_i64())
        .expect("guides folder should exist");

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/folders/{guides_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["files"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"]["files"][0]["name"], "hello.txt");

    let docs = folder_repo::find_by_id(&db, docs_id).await.unwrap();
    let guides = folder_repo::find_by_id(&db, guides_id).await.unwrap();
    assert_eq!(docs.created_by_username, "testuser");
    assert_eq!(guides.created_by_username, "testuser");
}

#[actix_web::test]
async fn test_staged_empty_upload_without_declared_size_uses_name_mode_for_relative_path() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let boundary = "----StagedEmptyUploadBoundary";
    let plain_payload = "------StagedEmptyUploadBoundary\r\n\
         Content-Disposition: form-data; name=\"file\"; filename=\"plain-empty.txt\"\r\n\
         Content-Type: application/octet-stream\r\n\r\n\
         \r\n\
         ------StagedEmptyUploadBoundary--\r\n";
    let req = test::TestRequest::post()
        .uri("/api/v1/files/upload")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .insert_header((
            "Content-Type",
            format!("multipart/form-data; boundary={boundary}"),
        ))
        .set_payload(plain_payload)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["name"], "plain-empty.txt");
    assert_eq!(body["data"]["size"], 0);

    let relative_payload = "------StagedEmptyUploadBoundary\r\n\
         Content-Disposition: form-data; name=\"file\"; filename=\"ignored.txt\"\r\n\
         Content-Type: application/octet-stream\r\n\r\n\
         \r\n\
         ------StagedEmptyUploadBoundary--\r\n";
    let req = test::TestRequest::post()
        .uri("/api/v1/files/upload?relative_path=exact-empty.txt")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .insert_header((
            "Content-Type",
            format!("multipart/form-data; boundary={boundary}"),
        ))
        .set_payload(relative_payload)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["name"], "exact-empty.txt");
    assert_eq!(body["data"]["size"], 0);
}

#[actix_web::test]
async fn test_init_upload_with_relative_path_reuses_existing_directories() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    for _ in 0..2 {
        let req = test::TestRequest::post()
            .uri("/api/v1/files/upload/init")
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .set_json(serde_json::json!({
                "filename": "hello.txt",
                "total_size": 10_485_760,
                "relative_path": "docs/guides/hello.txt"
            }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201);
    }

    let req = test::TestRequest::get()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let root_folders = body["data"]["folders"].as_array().unwrap();
    assert_eq!(root_folders.len(), 1);
    assert_eq!(root_folders[0]["name"], "docs");

    let docs_id = root_folders[0]["id"].as_i64().unwrap();
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/folders/{docs_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let child_folders = body["data"]["folders"].as_array().unwrap();
    assert_eq!(child_folders.len(), 1);
    assert_eq!(child_folders[0]["name"], "guides");

    let docs = folder_repo::find_by_id(&db, docs_id).await.unwrap();
    let guides_id = child_folders[0]["id"].as_i64().unwrap();
    let guides = folder_repo::find_by_id(&db, guides_id).await.unwrap();
    assert_eq!(docs.created_by_username, "testuser");
    assert_eq!(guides.created_by_username, "testuser");
}

#[actix_web::test]
async fn test_concurrent_init_uploads_share_new_nested_parent_path() {
    let state = common::setup_with_pool_size(4).await;
    let app = create_test_app!(state.clone());
    let user_id =
        common::setup_test_account_via_api(&app, "testuser", "test@example.com", "password123")
            .await;
    let relative_root = format!("parallel-{}/nested", uuid::Uuid::new_v4());
    let first_path = format!("{relative_root}/first.bin");
    let second_path = format!("{relative_root}/second.bin");

    let (first, second) = tokio::join!(
        upload::init_upload(
            &state,
            user_id,
            "first.bin",
            10_485_760,
            None,
            Some(&first_path),
        ),
        upload::init_upload(
            &state,
            user_id,
            "second.bin",
            10_485_760,
            None,
            Some(&second_path),
        ),
    );

    assert!(
        first.is_ok(),
        "first concurrent init should succeed: {}",
        first
            .as_ref()
            .err()
            .map(ToString::to_string)
            .unwrap_or_default()
    );
    assert!(
        second.is_ok(),
        "second concurrent init should reuse the created parents: {}",
        second
            .as_ref()
            .err()
            .map(ToString::to_string)
            .unwrap_or_default()
    );

    let root_name = relative_root
        .split('/')
        .next()
        .expect("parallel path should have a root segment");
    let root = folder_repo::find_by_name_in_parent(state.writer_db(), user_id, None, root_name)
        .await
        .expect("parallel root lookup should succeed")
        .expect("parallel root should exist");
    let root_count = folder_repo::find_children(state.writer_db(), user_id, None)
        .await
        .expect("parallel root listing should succeed")
        .into_iter()
        .filter(|folder| folder.name == root_name)
        .count();
    assert_eq!(root_count, 1, "concurrent init must create one root parent");
    let nested =
        folder_repo::find_by_name_in_parent(state.writer_db(), user_id, Some(root.id), "nested")
            .await
            .expect("parallel nested lookup should succeed")
            .expect("parallel nested folder should exist");
    assert_eq!(
        folder_repo::find_children(state.writer_db(), user_id, Some(root.id))
            .await
            .expect("parallel child lookup should succeed")
            .len(),
        1,
        "concurrent init must create one nested parent"
    );
    assert_eq!(nested.parent_id, Some(root.id));
}

#[tokio::test]
async fn test_concurrent_team_init_uploads_share_new_nested_parent_path() {
    let state = common::setup_with_pool_size(4).await;
    let owner = common::create_test_account(
        &state,
        "parallel-team",
        "parallel-team-owner@example.com",
        "password123",
    )
    .await
    .expect("team owner should be created");
    let team = team::create_team(
        &state,
        owner.id,
        team::CreateTeamInput {
            name: "Parallel folder upload team".to_string(),
            description: None,
        },
    )
    .await
    .expect("team should be created");
    let relative_root = format!("parallel-team-{}/nested", uuid::Uuid::new_v4());
    let first_path = format!("{relative_root}/first.bin");
    let second_path = format!("{relative_root}/second.bin");

    let (first, second) = tokio::join!(
        upload::init_upload_for_team(
            &state,
            team.id,
            owner.id,
            "first.bin",
            10_485_760,
            None,
            Some(&first_path),
        ),
        upload::init_upload_for_team(
            &state,
            team.id,
            owner.id,
            "second.bin",
            10_485_760,
            None,
            Some(&second_path),
        ),
    );

    assert!(
        first.is_ok(),
        "first concurrent team init should succeed: {}",
        first
            .as_ref()
            .err()
            .map(ToString::to_string)
            .unwrap_or_default()
    );
    assert!(
        second.is_ok(),
        "second concurrent team init should reuse the created parents: {}",
        second
            .as_ref()
            .err()
            .map(ToString::to_string)
            .unwrap_or_default()
    );

    let root_name = relative_root
        .split('/')
        .next()
        .expect("parallel team path should have a root segment");
    let roots = folder_repo::find_team_children(state.writer_db(), team.id, None)
        .await
        .expect("parallel team root listing should succeed");
    let matching_roots: Vec<_> = roots
        .iter()
        .filter(|folder| folder.name == root_name)
        .collect();
    assert_eq!(
        matching_roots.len(),
        1,
        "team init must create one root parent"
    );
    let root = matching_roots[0];
    let nested = folder_repo::find_by_name_in_team_parent(
        state.writer_db(),
        team.id,
        Some(root.id),
        "nested",
    )
    .await
    .expect("parallel team nested lookup should succeed")
    .expect("parallel team nested folder should exist");
    assert_eq!(
        folder_repo::find_team_children(state.writer_db(), team.id, Some(root.id))
            .await
            .expect("parallel team child lookup should succeed")
            .len(),
        1,
        "team init must create one nested parent"
    );
    assert_eq!(nested.parent_id, Some(root.id));
}

#[tokio::test]
async fn test_conflicting_folder_insert_reloads_existing_personal_and_team_parent() {
    let state = common::setup_with_pool_size(2).await;
    let owner = common::create_test_account(
        &state,
        "folder-conflict",
        "folder-conflict-owner@example.com",
        "password123",
    )
    .await
    .expect("folder conflict owner should be created");
    let now = chrono::Utc::now();

    let personal = folder::ActiveModel {
        name: Set("personal-conflict-parent".to_string()),
        owner_user_id: Set(Some(owner.id)),
        created_by_user_id: Set(Some(owner.id)),
        created_by_username: Set(owner.username.clone()),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .expect("personal conflict parent should be inserted");
    let personal_result = folder_repo::create_or_find_by_name_in_parent(
        state.writer_db(),
        folder::ActiveModel {
            name: Set(personal.name.clone()),
            owner_user_id: Set(Some(owner.id)),
            created_by_user_id: Set(Some(owner.id)),
            created_by_username: Set(owner.username.clone()),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        },
        owner.id,
        None,
        &personal.name,
    )
    .await
    .expect("personal conflict should reload the existing parent");
    assert_eq!(personal_result.id, personal.id);

    let team = team::create_team(
        &state,
        owner.id,
        team::CreateTeamInput {
            name: "Folder conflict team".to_string(),
            description: None,
        },
    )
    .await
    .expect("folder conflict team should be created");
    let team_folder = folder::ActiveModel {
        name: Set("team-conflict-parent".to_string()),
        team_id: Set(Some(team.id)),
        created_by_user_id: Set(Some(owner.id)),
        created_by_username: Set(owner.username.clone()),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .expect("team conflict parent should be inserted");
    let team_result = folder_repo::create_or_find_by_name_in_team_parent(
        state.writer_db(),
        folder::ActiveModel {
            name: Set(team_folder.name.clone()),
            team_id: Set(Some(team.id)),
            created_by_user_id: Set(Some(owner.id)),
            created_by_username: Set(owner.username),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        },
        team.id,
        None,
        &team_folder.name,
    )
    .await
    .expect("team conflict should reload the existing parent");
    assert_eq!(team_result.id, team_folder.id);
}

#[actix_web::test]
async fn test_init_upload_with_relative_path_uses_parent_folder_policy() {
    use sea_orm::{ActiveModelTrait, Set};

    let state = common::setup().await;
    let db = state.writer_db().clone();
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let policy_base_path = std::env::temp_dir()
        .join(format!(
            "test-relative-path-folder-policy-{}",
            uuid::Uuid::new_v4()
        ))
        .to_string_lossy()
        .into_owned();

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Tiny Folder Policy",
            "connection": common::local_connection_json(policy_base_path),
            "max_file_size": 8,
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
        .set_json(serde_json::json!({ "name": "policy-root" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let folder_id = body["data"]["id"].as_i64().unwrap();

    let mut folder: aster_drive_model::entities::folder::ActiveModel =
        folder_repo::find_by_id(&db, folder_id)
            .await
            .unwrap()
            .into();
    folder.policy_id = Set(Some(policy_id));
    folder.update(&db).await.unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/files/upload/init")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "filename": "ignored.txt",
            "folder_id": folder_id,
            "relative_path": "nested/too-large.txt",
            "total_size": 9
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["msg"], "file size 9 exceeds limit 8");
}

#[actix_web::test]
async fn test_relative_path_rejects_empty_segment() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/files/upload/init")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "filename": "bad.txt",
            "total_size": 10_485_760,
            "relative_path": "docs//bad.txt"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
}

#[actix_web::test]
async fn test_chunked_upload_with_relative_path_and_auto_rename() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let relative_path = "docs/chunked.txt";
    let total_size = 10_485_760i64;

    for expected_name in ["chunked.txt", "chunked (1).txt"] {
        let req = test::TestRequest::post()
            .uri("/api/v1/files/upload/init")
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .set_json(serde_json::json!({
                "filename": "chunked.txt",
                "total_size": total_size,
                "relative_path": relative_path
            }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201);
        let body: Value = test::read_body_json(resp).await;
        assert_eq!(body["data"]["mode"], "chunked");
        let upload_id = body["data"]["upload_id"].as_str().unwrap();
        let chunk_size = body["data"]["chunk_size"].as_i64().unwrap();
        let total_chunks = body["data"]["total_chunks"].as_i64().unwrap();

        for i in 0..total_chunks {
            let expected_chunk_size = if i == total_chunks - 1 {
                i64_to_usize(
                    total_size - chunk_size * (total_chunks - 1),
                    "final directory upload chunk size",
                )
                .expect("final directory upload chunk size should fit usize")
            } else {
                i64_to_usize(chunk_size, "directory upload chunk size")
                    .expect("directory upload chunk size should fit usize")
            };
            let chunk_data = vec![b'A'; expected_chunk_size];
            let req = test::TestRequest::put()
                .uri(&format!("/api/v1/files/upload/{upload_id}/{i}"))
                .insert_header(("Cookie", common::access_cookie_header(&token)))
                .insert_header(common::csrf_header_for(&token))
                .insert_header(("Content-Type", "application/octet-stream"))
                .set_payload(chunk_data)
                .to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(resp.status(), 200);
        }

        let req = test::TestRequest::post()
            .uri(&format!("/api/v1/files/upload/{upload_id}/complete"))
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201);
        let body: Value = test::read_body_json(resp).await;
        assert_eq!(body["data"]["name"], expected_name);
    }

    let req = test::TestRequest::get()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let docs_id = body["data"]["folders"][0]["id"].as_i64().unwrap();

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/folders/{docs_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let names: Vec<&str> = body["data"]["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|file| file["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"chunked.txt"));
    assert!(names.contains(&"chunked (1).txt"));
}

#[actix_web::test]
async fn test_relative_path_normalizes_nfd_segments_to_nfc() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let relative_path = urlencoding::encode("cafe\u{0301}/hello.txt");

    let boundary = "----DirUploadBoundaryNormalize";
    let payload = "------DirUploadBoundaryNormalize\r\n\
         Content-Disposition: form-data; name=\"file\"; filename=\"hello.txt\"\r\n\
         Content-Type: text/plain\r\n\r\n\
         hello nested world\r\n\
         ------DirUploadBoundaryNormalize--\r\n";
    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/v1/files/upload?relative_path={relative_path}"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .insert_header((
            "Content-Type",
            format!("multipart/form-data; boundary={boundary}"),
        ))
        .set_payload(payload)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::get()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let root_folders = body["data"]["folders"].as_array().unwrap();
    assert_eq!(root_folders.len(), 1);
    assert_eq!(root_folders[0]["name"], "caf\u{00e9}");
}

#[actix_web::test]
async fn test_relative_path_rejects_windows_reserved_segment() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/files/upload/init")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "filename": "hello.txt",
            "total_size": 10_485_760,
            "relative_path": "docs/CON/hello.txt"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
}
