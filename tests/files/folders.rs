//! 集成测试：`folders`。

use crate::common;

use actix_web::test;
use aster_drive::db::repository::{
    background_task_repo, file_repo, folder_repo, folder_tree_operation_repo, lock_repo,
    policy_repo, user_repo,
};
use aster_drive::services::events::storage_change::StorageChangeKind;
use aster_drive_model::entities::{background_task, file, file_blob, folder as folder_entity};
use aster_drive_model::types::{
    BackgroundTaskKind, BackgroundTaskStatus, EntityType, StoredTaskPayload,
};
use aster_forge_file_classification::FileCategory;
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, Set, sea_query::Expr,
};
use serde_json::Value;
use std::time::Duration;

#[actix_web::test]
async fn test_folders_crud() {
    let state = common::setup().await;
    let app = create_test_app!(state);

    let (token, _) = register_and_login!(app);

    // 列出根目录（应为空）
    let req = test::TestRequest::get()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["folders"], Value::Array(vec![]));
    assert_eq!(body["data"]["files"], Value::Array(vec![]));

    // 创建文件夹
    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Documents" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let folder_id = body["data"]["id"].as_i64().unwrap();
    assert_eq!(body["data"]["name"], "Documents");

    // 列出根目录（应有 1 个文件夹）
    let req = test::TestRequest::get()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["folders"].as_array().unwrap().len(), 1);

    // 重命名文件夹
    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/folders/{folder_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "My Docs" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["name"], "My Docs");

    // 删除文件夹
    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/folders/{folder_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn test_large_folder_delete_and_restore_dispatch_bounded_tasks() {
    const FILE_COUNT: usize =
        aster_drive::services::files::folder::REST_FOLDER_TREE_SYNCHRONOUS_MAXIMUM_RESOURCES;
    const SYNCHRONOUS_FILE_COUNT: usize = FILE_COUNT - 1;
    const INSERT_BATCH: usize = 400;

    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Task-sized folder" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let folder_id = body["data"]["id"].as_i64().unwrap();

    let user = user_repo::find_by_username(state.writer_db(), "testuser")
        .await
        .unwrap()
        .expect("registered test user should exist");
    let policy = policy_repo::find_default(state.writer_db())
        .await
        .unwrap()
        .expect("default policy should exist");
    let now = Utc::now();
    let blob = file_blob::ActiveModel {
        hash: Set("folder-tree-task-fixture".to_string()),
        size: Set(0),
        policy_id: Set(policy.id),
        storage_path: Set("folder-tree-task-fixture".to_string()),
        ref_count: Set(i32::try_from(FILE_COUNT).unwrap()),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .expect("fixture blob should insert");

    for batch_start in (0..SYNCHRONOUS_FILE_COUNT).step_by(INSERT_BATCH) {
        let batch_end = (batch_start + INSERT_BATCH).min(SYNCHRONOUS_FILE_COUNT);
        let models = (batch_start..batch_end)
            .map(|index| file::ActiveModel {
                name: Set(format!("fixture-{index:05}.txt")),
                folder_id: Set(Some(folder_id)),
                team_id: Set(None),
                blob_id: Set(blob.id),
                size: Set(0),
                owner_user_id: Set(Some(user.id)),
                created_by_user_id: Set(Some(user.id)),
                created_by_username: Set(user.username.clone()),
                mime_type: Set("text/plain".to_string()),
                extension: Set("txt".to_string()),
                compound_extension: Set(None),
                file_category: Set(FileCategory::Document),
                created_at: Set(now),
                updated_at: Set(now),
                deleted_at: Set(None),
                ..Default::default()
            })
            .collect();
        file_repo::create_many(state.writer_db(), models)
            .await
            .expect("fixture files should insert");
    }

    // Root + 9,999 files is exactly the 10,000-resource synchronous budget.
    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/folders/{folder_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        count_files_with_deleted_state(&state, folder_id, true).await,
        SYNCHRONOUS_FILE_COUNT as u64
    );

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/trash/folder/{folder_id}/restore"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        count_files_with_deleted_state(&state, folder_id, false).await,
        SYNCHRONOUS_FILE_COUNT as u64
    );

    file_repo::create(
        state.writer_db(),
        file::ActiveModel {
            name: Set(format!("fixture-{SYNCHRONOUS_FILE_COUNT:05}.txt")),
            folder_id: Set(Some(folder_id)),
            team_id: Set(None),
            blob_id: Set(blob.id),
            size: Set(0),
            owner_user_id: Set(Some(user.id)),
            created_by_user_id: Set(Some(user.id)),
            created_by_username: Set(user.username.clone()),
            mime_type: Set("text/plain".to_string()),
            extension: Set("txt".to_string()),
            compound_extension: Set(None),
            file_category: Set(FileCategory::Document),
            created_at: Set(now),
            updated_at: Set(now),
            deleted_at: Set(None),
            ..Default::default()
        },
    )
    .await
    .expect("resource-limit-plus-one file should insert");

    // Root + 10,000 files exceeds the budget by exactly one and must queue.
    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/folders/{folder_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 202);
    let body: Value = test::read_body_json(resp).await;
    let delete_task_id = body["data"]["id"].as_i64().unwrap();
    assert_eq!(body["data"]["kind"], "folder_tree_mutation");

    let locked_file = file_repo::find_by_name_in_folder(
        state.writer_db(),
        user.id,
        Some(folder_id),
        "fixture-00000.txt",
    )
    .await
    .unwrap()
    .expect("fixture file should exist");
    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/files/{}/lock", locked_file.id))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "locked": true }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let stats = aster_drive::services::task::drain(&state)
        .await
        .expect("locked large folder task should drain as a failed task");
    assert_eq!(stats.failed, 1);
    assert_eq!(stats.succeeded, 0);
    assert_eq!(
        count_files_with_deleted_state(&state, folder_id, false).await,
        FILE_COUNT as u64
    );
    assert!(
        folder_repo::find_by_id(state.writer_db(), folder_id)
            .await
            .unwrap()
            .deleted_at
            .is_none()
    );
    assert_eq!(
        folder_tree_operation_repo::count(state.writer_db(), delete_task_id)
            .await
            .unwrap(),
        0
    );
    assert!(
        lock_repo::find_all_by_entity(state.writer_db(), EntityType::Folder, folder_id)
            .await
            .unwrap()
            .is_empty()
    );

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/files/{}/lock", locked_file.id))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "locked": false }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/tasks/{delete_task_id}/retry"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let stats = aster_drive::services::task::drain(&state)
        .await
        .expect("large folder delete task should drain");
    let delete_task = background_task_repo::find_by_id(state.writer_db(), delete_task_id)
        .await
        .expect("large folder delete task should still exist");
    assert_eq!(
        stats.failed, 0,
        "large folder delete task failed: {:?}",
        delete_task.last_error
    );
    assert_eq!(stats.succeeded, 1);
    assert_folder_tree_task_result(&app, &token, None, delete_task_id, FILE_COUNT, 1).await;
    assert_eq!(
        count_files_with_deleted_state(&state, folder_id, true).await,
        FILE_COUNT as u64
    );
    assert!(
        folder_repo::find_by_id(state.writer_db(), folder_id)
            .await
            .unwrap()
            .deleted_at
            .is_some()
    );
    assert_eq!(
        folder_tree_operation_repo::count(state.writer_db(), delete_task_id)
            .await
            .unwrap(),
        0
    );
    assert!(
        lock_repo::find_all_by_entity(state.writer_db(), EntityType::Folder, folder_id)
            .await
            .unwrap()
            .is_empty()
    );

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/trash/folder/{folder_id}/restore"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 202);
    let body: Value = test::read_body_json(resp).await;
    let restore_task_id = body["data"]["id"].as_i64().unwrap();

    let stats = aster_drive::services::task::drain(&state)
        .await
        .expect("large folder restore task should drain");
    assert_eq!(stats.failed, 0);
    assert_eq!(stats.succeeded, 1);
    assert_folder_tree_task_result(&app, &token, None, restore_task_id, FILE_COUNT, 1).await;
    assert_eq!(
        count_files_with_deleted_state(&state, folder_id, false).await,
        FILE_COUNT as u64
    );
    assert!(
        folder_repo::find_by_id(state.writer_db(), folder_id)
            .await
            .unwrap()
            .deleted_at
            .is_none()
    );
    assert_eq!(
        folder_tree_operation_repo::count(state.writer_db(), restore_task_id)
            .await
            .unwrap(),
        0
    );
    assert!(
        lock_repo::find_all_by_entity(state.writer_db(), EntityType::Folder, folder_id)
            .await
            .unwrap()
            .is_empty()
    );

    let team = aster_drive::services::workspace::team::create_team(
        &state,
        user.id,
        aster_drive::services::workspace::team::CreateTeamInput {
            name: "Folder tree task team".to_string(),
            description: None,
        },
    )
    .await
    .expect("fixture team should be created");
    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{}/folders", team.id))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Team task-sized folder" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let team_folder_id = body["data"]["id"].as_i64().unwrap();

    file::Entity::update_many()
        .col_expr(file::Column::FolderId, Expr::value(Some(team_folder_id)))
        .col_expr(file::Column::TeamId, Expr::value(Some(team.id)))
        .col_expr(file::Column::OwnerUserId, Expr::value(Option::<i64>::None))
        .filter(file::Column::FolderId.eq(folder_id))
        .exec(state.writer_db())
        .await
        .expect("fixture files should move into team scope");

    let req = test::TestRequest::delete()
        .uri(&format!(
            "/api/v1/teams/{}/folders/{team_folder_id}",
            team.id
        ))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 202);
    let body: Value = test::read_body_json(resp).await;
    let team_delete_task_id = body["data"]["id"].as_i64().unwrap();

    let stats = aster_drive::services::task::drain(&state)
        .await
        .expect("large team folder delete task should drain");
    assert_eq!(stats.failed, 0);
    assert_eq!(stats.succeeded, 1);
    assert_folder_tree_task_result(
        &app,
        &token,
        Some(team.id),
        team_delete_task_id,
        FILE_COUNT,
        1,
    )
    .await;
    assert_eq!(
        count_files_with_deleted_state(&state, team_folder_id, true).await,
        FILE_COUNT as u64
    );

    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/v1/teams/{}/trash/folder/{team_folder_id}/restore",
            team.id
        ))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 202);
    let body: Value = test::read_body_json(resp).await;
    let team_restore_task_id = body["data"]["id"].as_i64().unwrap();

    let stats = aster_drive::services::task::drain(&state)
        .await
        .expect("large team folder restore task should drain");
    assert_eq!(stats.failed, 0);
    assert_eq!(stats.succeeded, 1);
    assert_folder_tree_task_result(
        &app,
        &token,
        Some(team.id),
        team_restore_task_id,
        FILE_COUNT,
        1,
    )
    .await;
    assert_eq!(
        count_files_with_deleted_state(&state, team_folder_id, false).await,
        FILE_COUNT as u64
    );
    assert!(
        lock_repo::find_all_by_entity(state.writer_db(), EntityType::Folder, team_folder_id,)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        folder_tree_operation_repo::count(state.writer_db(), team_restore_task_id)
            .await
            .unwrap(),
        0
    );
}

#[actix_web::test]
async fn test_folder_tree_staging_is_idempotent_and_cascades_with_task_record() {
    let state = common::setup().await;
    let now = Utc::now();
    let task = background_task_repo::create(
        state.writer_db(),
        background_task::ActiveModel {
            kind: Set(BackgroundTaskKind::FolderTreeMutation),
            status: Set(BackgroundTaskStatus::Pending),
            creator_user_id: Set(None),
            team_id: Set(None),
            share_id: Set(None),
            display_name: Set("folder-tree staging cascade fixture".to_string()),
            payload_json: Set(StoredTaskPayload(
                r#"{"folder_id":1,"operation":"delete"}"#.to_string(),
            )),
            result_json: Set(None),
            runtime_json: Set(None),
            steps_json: Set(None),
            progress_current: Set(0),
            progress_total: Set(0),
            status_text: Set(None),
            attempt_count: Set(0),
            max_attempts: Set(1),
            next_run_at: Set(now),
            processing_token: Set(0),
            processing_started_at: Set(None),
            last_heartbeat_at: Set(None),
            lease_expires_at: Set(None),
            started_at: Set(None),
            finished_at: Set(None),
            last_error: Set(None),
            failure_can_retry: Set(None),
            expires_at: Set(now + chrono::Duration::hours(1)),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        },
    )
    .await
    .expect("folder-tree task fixture should insert");

    assert_eq!(
        folder_tree_operation_repo::stage_ids(
            state.writer_db(),
            task.id,
            EntityType::Folder,
            &[41, 41],
        )
        .await
        .expect("duplicate staging IDs should insert idempotently"),
        1
    );
    assert_eq!(
        folder_tree_operation_repo::stage_ids(
            state.writer_db(),
            task.id,
            EntityType::Folder,
            &[41],
        )
        .await
        .expect("retry staging should remain idempotent"),
        1
    );
    assert_eq!(
        folder_tree_operation_repo::count(state.writer_db(), task.id)
            .await
            .unwrap(),
        1
    );

    background_task::Entity::delete_by_id(task.id)
        .exec(state.writer_db())
        .await
        .expect("task fixture should delete");
    assert_eq!(
        folder_tree_operation_repo::count(state.writer_db(), task.id)
            .await
            .unwrap(),
        0,
        "staging rows must be removed by the background-task foreign-key cascade"
    );
}

#[actix_web::test]
async fn test_rest_folder_tree_frontier_and_depth_exact_boundaries() {
    const MAXIMUM_FRONTIER: usize = 2_000;
    const MAXIMUM_DEPTH: usize = 128;
    const INSERT_BATCH: usize = 400;

    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let user = user_repo::find_by_username(state.writer_db(), "testuser")
        .await
        .unwrap()
        .expect("registered test user should exist");
    let now = Utc::now();

    let frontier_root = aster_drive::services::files::folder::create(
        &state,
        user.id,
        "Frontier boundary root",
        None,
    )
    .await
    .expect("frontier boundary root should be created");
    for batch_start in (0..MAXIMUM_FRONTIER).step_by(INSERT_BATCH) {
        let batch_end = (batch_start + INSERT_BATCH).min(MAXIMUM_FRONTIER);
        let models = (batch_start..batch_end)
            .map(|index| folder_entity::ActiveModel {
                name: Set(format!("frontier-child-{index:04}")),
                parent_id: Set(Some(frontier_root.id)),
                team_id: Set(None),
                owner_user_id: Set(Some(user.id)),
                created_by_user_id: Set(Some(user.id)),
                created_by_username: Set(user.username.clone()),
                policy_id: Set(None),
                created_at: Set(now),
                updated_at: Set(now),
                deleted_at: Set(None),
                ..Default::default()
            })
            .collect();
        folder_repo::create_many(state.writer_db(), models)
            .await
            .expect("frontier boundary children should insert");
    }

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/folders/{}", frontier_root.id))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        200,
        "frontier at the exact limit should stay synchronous"
    );

    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/v1/trash/folder/{}/restore",
            frontier_root.id
        ))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        200,
        "frontier restore at the exact limit should stay synchronous"
    );

    aster_drive::services::files::folder::create(
        &state,
        user.id,
        "frontier-child-over-limit",
        Some(frontier_root.id),
    )
    .await
    .expect("frontier limit plus one child should be created");
    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/folders/{}", frontier_root.id))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 202, "frontier limit plus one should queue");
    let body: Value = test::read_body_json(resp).await;
    let frontier_task_id = body["data"]["id"].as_i64().unwrap();

    let depth_root =
        aster_drive::services::files::folder::create(&state, user.id, "Depth boundary root", None)
            .await
            .expect("depth boundary root should be created");
    let mut deepest_id = depth_root.id;
    for depth in 1..=MAXIMUM_DEPTH {
        deepest_id = aster_drive::services::files::folder::create(
            &state,
            user.id,
            &format!("depth-child-{depth:03}"),
            Some(deepest_id),
        )
        .await
        .expect("depth boundary child should be created")
        .id;
    }

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/folders/{}", depth_root.id))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        200,
        "depth at the exact limit should stay synchronous"
    );

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/trash/folder/{}/restore", depth_root.id))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        200,
        "depth restore at the exact limit should stay synchronous"
    );

    aster_drive::services::files::folder::create(
        &state,
        user.id,
        "depth-child-over-limit",
        Some(deepest_id),
    )
    .await
    .expect("depth limit plus one child should be created");
    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/folders/{}", depth_root.id))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 202, "depth limit plus one should queue");
    let body: Value = test::read_body_json(resp).await;
    let depth_task_id = body["data"]["id"].as_i64().unwrap();

    let stats = aster_drive::services::task::drain(&state)
        .await
        .expect("shape-boundary folder tasks should drain");
    assert_eq!(stats.failed, 0);
    assert_eq!(stats.succeeded, 2);
    assert_folder_tree_task_result(
        &app,
        &token,
        None,
        frontier_task_id,
        0,
        MAXIMUM_FRONTIER + 2,
    )
    .await;
    assert_folder_tree_task_result(&app, &token, None, depth_task_id, 0, MAXIMUM_DEPTH + 2).await;

    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/v1/trash/folder/{}/restore",
            frontier_root.id
        ))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        202,
        "frontier limit plus one restore should queue"
    );
    let body: Value = test::read_body_json(resp).await;
    let frontier_restore_task_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/trash/folder/{}/restore", depth_root.id))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        202,
        "depth limit plus one restore should queue"
    );
    let body: Value = test::read_body_json(resp).await;
    let depth_restore_task_id = body["data"]["id"].as_i64().unwrap();

    let stats = aster_drive::services::task::drain(&state)
        .await
        .expect("shape-boundary folder restore tasks should drain");
    assert_eq!(stats.failed, 0);
    assert_eq!(stats.succeeded, 2);
    assert_folder_tree_task_result(
        &app,
        &token,
        None,
        frontier_restore_task_id,
        0,
        MAXIMUM_FRONTIER + 2,
    )
    .await;
    assert_folder_tree_task_result(
        &app,
        &token,
        None,
        depth_restore_task_id,
        0,
        MAXIMUM_DEPTH + 2,
    )
    .await;
}

async fn assert_folder_tree_task_result<S>(
    app: &S,
    token: &str,
    team_id: Option<i64>,
    task_id: i64,
    expected_files: usize,
    expected_folders: usize,
) where
    S: actix_web::dev::Service<
            actix_http::Request,
            Response = actix_web::dev::ServiceResponse,
            Error = actix_web::Error,
        >,
{
    let task_path = team_id.map_or_else(
        || format!("/api/v1/tasks/{task_id}"),
        |team_id| format!("/api/v1/teams/{team_id}/tasks/{task_id}"),
    );
    let req = test::TestRequest::get()
        .uri(&task_path)
        .insert_header(("Cookie", common::access_cookie_header(token)))
        .insert_header(common::csrf_header_for(token))
        .to_request();
    let resp = test::call_service(app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["status"], "succeeded");
    assert_eq!(body["data"]["result"]["kind"], "folder_tree_mutation");
    assert_eq!(body["data"]["result"]["file_count"], expected_files);
    assert_eq!(body["data"]["result"]["folder_count"], expected_folders);
}

async fn count_files_with_deleted_state(
    state: &aster_drive::runtime::PrimaryAppState,
    folder_id: i64,
    deleted: bool,
) -> u64 {
    let deleted_filter = if deleted {
        file::Column::DeletedAt.is_not_null()
    } else {
        file::Column::DeletedAt.is_null()
    };
    file::Entity::find()
        .filter(file::Column::FolderId.eq(folder_id))
        .filter(deleted_filter)
        .count(state.writer_db())
        .await
        .unwrap()
}

#[actix_web::test]
async fn test_folder_lock_unlock() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());

    let (token, _) = register_and_login!(app);

    // 创建文件夹
    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Locked Folder" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let folder_id = body["data"]["id"].as_i64().unwrap();
    let mut storage_events = state.storage_change_bus.subscribe();

    // 锁定
    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/folders/{folder_id}/lock"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "locked": true }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let event = tokio::time::timeout(Duration::from_secs(1), storage_events.recv())
        .await
        .expect("folder lock should publish storage change event")
        .expect("storage change channel should stay open");
    assert_eq!(event.kind, StorageChangeKind::LockCreated);
    assert!(event.file_ids.is_empty());
    assert_eq!(event.folder_ids, vec![folder_id]);
    assert!(event.affected_parent_ids.is_empty());
    assert!(event.root_affected);

    // 删除失败
    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/folders/{folder_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status() == 403 || resp.status() == 423);

    // 锁定 collection 也保护其 membership，不能创建子目录。
    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Blocked Child",
            "parent_id": folder_id
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status() == 403 || resp.status() == 423);

    // 重命名失败
    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/folders/{folder_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Nope" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status() == 403 || resp.status() == 423);

    // 解锁 → 删除成功
    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/folders/{folder_id}/lock"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "locked": false }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let event = tokio::time::timeout(Duration::from_secs(1), storage_events.recv())
        .await
        .expect("folder unlock should publish storage change event")
        .expect("storage change channel should stay open");
    assert_eq!(event.kind, StorageChangeKind::LockDeleted);
    assert!(event.file_ids.is_empty());
    assert_eq!(event.folder_ids, vec![folder_id]);
    assert!(event.affected_parent_ids.is_empty());
    assert!(event.root_affected);

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/folders/{folder_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn test_folder_delete_fails_when_descendant_is_locked() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Parent" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let parent_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Locked Child",
            "parent_id": parent_id
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let child_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/folders/{child_id}/lock"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "locked": true }))
        .to_request();
    assert_eq!(test::call_service(&app, req).await.status(), 200);

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/folders/{parent_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status() == 403 || resp.status() == 423);

    for folder_id in [parent_id, child_id] {
        let req = test::TestRequest::get()
            .uri(&format!("/api/v1/folders/{folder_id}"))
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .to_request();
        assert_eq!(test::call_service(&app, req).await.status(), 200);
    }
}

#[actix_web::test]
async fn test_nested_folder_lock_events_include_parent_folder() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());

    let (token, _) = register_and_login!(app);
    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Parent Folder" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let parent_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Child Folder",
            "parent_id": parent_id
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let child_id = body["data"]["id"].as_i64().unwrap();
    let mut storage_events = state.storage_change_bus.subscribe();

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/folders/{child_id}/lock"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "locked": true }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let event = tokio::time::timeout(Duration::from_secs(1), storage_events.recv())
        .await
        .expect("nested folder lock should publish storage change event")
        .expect("storage change channel should stay open");
    assert_eq!(event.kind, StorageChangeKind::LockCreated);
    assert!(event.file_ids.is_empty());
    assert_eq!(event.folder_ids, vec![child_id]);
    assert_eq!(event.affected_parent_ids, vec![parent_id]);
    assert!(!event.root_affected);
}

#[actix_web::test]
async fn test_folder_create_normalizes_nfd_name_to_nfc() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "cafe\u{0301}" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["name"], "caf\u{00e9}");
}

#[actix_web::test]
async fn test_folder_create_rejects_windows_reserved_name() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "CON" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
}

#[actix_web::test]
async fn test_folder_rename_normalizes_nfd_name_and_rejects_windows_reserved_name() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Workspace" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let folder_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/folders/{folder_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "cafe\u{0301}" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["name"], "caf\u{00e9}");

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/folders/{folder_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "LPT1" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
}

#[actix_web::test]
async fn test_folder_name_validation_returns_400() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Valid" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let folder_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/folders/{folder_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "bad/name" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
}

#[actix_web::test]
async fn test_folder_repo_find_ancestors_returns_full_chain() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Projects" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let projects_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "2026", "parent_id": projects_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let year_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Q1", "parent_id": year_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let quarter_id = body["data"]["id"].as_i64().unwrap();

    let user = user_repo::find_by_username(&db, "testuser")
        .await
        .unwrap()
        .expect("registered user should exist");
    let ancestors = folder_repo::find_ancestors(&db, user.id, quarter_id)
        .await
        .unwrap();

    assert_eq!(
        ancestors,
        vec![
            (projects_id, "Projects".to_string()),
            (year_id, "2026".to_string()),
            (quarter_id, "Q1".to_string()),
        ]
    );
}

#[actix_web::test]
async fn test_folder_list_items_are_lightweight_and_info_endpoint_returns_full_details() {
    let state = common::setup().await;
    let app = create_test_app!(state);

    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Projects" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let folder_id = body["data"]["id"].as_i64().unwrap();

    let file_id = upload_test_file!(app, token);

    let req = test::TestRequest::get()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let folder = &body["data"]["folders"][0];
    let file = &body["data"]["files"][0];
    assert_eq!(folder["id"], folder_id);
    assert!(folder["created_at"].is_null());
    assert!(folder["parent_id"].is_null());
    assert!(folder["policy_id"].is_null());
    assert!(folder.get("user_id").is_none());
    assert_eq!(file["id"], file_id);
    assert!(file["blob_id"].is_null());
    assert!(file["created_at"].is_null());
    assert!(file["folder_id"].is_null());
    assert!(file.get("user_id").is_none());

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/folders/{folder_id}/info"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["id"], folder_id);
    assert_eq!(body["data"]["name"], "Projects");
    assert!(body["data"]["created_at"].is_string());
    assert!(body["data"]["parent_id"].is_null());
    assert!(body["data"].get("user_id").is_none());
    assert!(body["data"]["owner_user_id"].as_i64().unwrap() > 0);
}

#[actix_web::test]
async fn test_folder_copy() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    // 创建源文件夹 + 里面放个文件
    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Source" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let src_id = body["data"]["id"].as_i64().unwrap();

    let boundary = "----TestBoundary123";
    let payload = "------TestBoundary123\r\n\
         Content-Disposition: form-data; name=\"file\"; filename=\"inside.txt\"\r\n\
         Content-Type: text/plain\r\n\r\n\
         folder content\r\n\
         ------TestBoundary123--\r\n";
    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/files/upload?folder_id={src_id}"))
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

    // 复制文件夹到根目录（null = root，与根目录同名冲突时应递增）
    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/folders/{src_id}/copy"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "parent_id": null }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["name"], "Source (1)");
    assert!(body["data"]["parent_id"].is_null());
    let copy_id = body["data"]["id"].as_i64().unwrap();

    // 副本文件夹里应该有文件
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/folders/{copy_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["files"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"]["files"][0]["name"], "inside.txt");
}

/// 测试多层嵌套文件夹复制（batch_duplicate_file_records）
#[actix_web::test]
async fn test_nested_folder_copy() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    // 创建 Source/A/B 三层嵌套，每层各一个文件
    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Source" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let source_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "A", "parent_id": source_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let a_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "B", "parent_id": a_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let b_id = body["data"]["id"].as_i64().unwrap();

    upload_test_file_to_folder!(app, token, a_id);
    upload_test_file_to_folder!(app, token, b_id);

    // 复制顶层文件夹 A → 根目录（null = root，应保留原名）
    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/folders/{a_id}/copy"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "parent_id": null }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["name"], "A");
    assert!(body["data"]["parent_id"].is_null());
    let a_copy_id = body["data"]["id"].as_i64().unwrap();

    // A-copy 里应有 1 个文件 + 1 个子文件夹
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/folders/{a_copy_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["data"]["files"].as_array().unwrap().len(),
        1,
        "A copy in root should have 1 file"
    );
    assert_eq!(
        body["data"]["folders"].as_array().unwrap().len(),
        1,
        "A-copy should have 1 subfolder"
    );

    // B-copy 里也应有 1 个文件
    let b_copy_id = body["data"]["folders"][0]["id"].as_i64().unwrap();
    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/folders/{b_copy_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["data"]["files"].as_array().unwrap().len(),
        1,
        "B-copy should have 1 file"
    );

    // 源文件夹和副本独立：删副本不影响源
    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/folders/{a_copy_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/folders/{a_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["data"]["files"].as_array().unwrap().len(),
        1,
        "original A should still have its file"
    );
}

#[actix_web::test]
async fn test_folder_copy_quota_failure_does_not_materialize_nested_descendants() {
    use sea_orm::{ActiveModelTrait, Set};

    let state = common::setup().await;
    let db = state.writer_db().clone();
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "QuotaSource" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let source_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Nested", "parent_id": source_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    upload_test_file_to_folder!(app, token, source_id);
    upload_test_file_to_folder!(app, token, source_id);

    let user = user_repo::find_by_username(&db, "testuser")
        .await
        .unwrap()
        .unwrap();
    let storage_used = user.storage_used;
    let mut user_active: aster_drive_model::entities::user::ActiveModel = user.into();
    user_active.storage_quota = Set(storage_used);
    user_active.update(&db).await.unwrap();

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/folders/{source_id}/copy"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "parent_id": null }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 507);
    let body: Value = test::read_body_json(resp).await;
    assert!(
        body["msg"].as_str().unwrap_or_default().contains("quota"),
        "quota error should be surfaced to the client"
    );

    let req = test::TestRequest::get()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let root_folders = body["data"]["folders"].as_array().unwrap();

    if let Some(copy_folder) = root_folders
        .iter()
        .find(|folder| folder["name"].as_str() == Some("QuotaSource (1)"))
    {
        let copy_folder_id = copy_folder["id"].as_i64().unwrap();
        let req = test::TestRequest::get()
            .uri(&format!("/api/v1/folders/{copy_folder_id}"))
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let body: Value = test::read_body_json(resp).await;
        assert!(
            body["data"]["files"].as_array().unwrap().is_empty(),
            "quota failure should not leave copied files in the exposed copy shell"
        );
        assert!(
            body["data"]["folders"].as_array().unwrap().is_empty(),
            "quota failure should not expose nested descendant folders in the copy shell"
        );
    }

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/folders/{source_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["files"].as_array().unwrap().len(), 2);
    assert_eq!(body["data"]["folders"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"]["folders"][0]["name"], "Nested");
}

#[actix_web::test]
async fn test_folder_patch_can_move_to_root_with_null() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Parent" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let parent_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Child", "parent_id": parent_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let child_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/folders/{child_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "parent_id": null }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert!(body["data"]["parent_id"].is_null());

    let req = test::TestRequest::get()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let root_folders = body["data"]["folders"].as_array().unwrap();
    assert!(
        root_folders
            .iter()
            .any(|folder| folder["id"].as_i64() == Some(child_id)),
        "child folder should be moved back to root"
    );

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/folders/{parent_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["folders"].as_array().unwrap().len(), 0);
}

#[actix_web::test]
async fn test_folder_copy_preserves_policy_ids() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Folder Copy Policy Root",
            "connection": common::local_connection_json("/tmp/test-folder-copy-policy-root"),
            "max_file_size": 0,
            "is_default": false
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let root_policy_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Folder Copy Policy Child",
            "connection": common::local_connection_json("/tmp/test-folder-copy-policy-child"),
            "max_file_size": 0,
            "is_default": false
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let child_policy_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "PolicySource" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let source_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Nested", "parent_id": source_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let nested_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::put()
        .uri(&format!("/api/v1/admin/folders/{source_id}/policy"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "policy_id": root_policy_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::put()
        .uri(&format!("/api/v1/admin/folders/{nested_id}/policy"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "policy_id": child_policy_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/folders/{source_id}/copy"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "parent_id": null }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let copy_id = body["data"]["id"].as_i64().unwrap();

    let copied_root = folder_repo::find_by_id(&db, copy_id).await.unwrap();
    assert_eq!(copied_root.policy_id, Some(root_policy_id));

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/folders/{copy_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let copied_nested_id = body["data"]["folders"][0]["id"].as_i64().unwrap();

    let copied_nested = folder_repo::find_by_id(&db, copied_nested_id)
        .await
        .unwrap();
    assert_eq!(copied_nested.policy_id, Some(child_policy_id));
}

#[actix_web::test]
async fn test_deleted_folder_name_can_be_reused_and_restore_rejects_active_conflict() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "restore-conflict-folder" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let deleted_folder_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/folders/{deleted_folder_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "restore-conflict-folder" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/trash/folder/{deleted_folder_id}/restore"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
}
