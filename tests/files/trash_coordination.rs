//! Trash admission uses writer-database state and the same scope lock as purge enqueue.
use crate::common;
use actix_web::test;
use aster_drive::db::repository::{file_repo, folder_repo, user_repo};
use aster_drive_model::{
    entities::{background_task, file, folder},
    types::BackgroundTaskStatus,
};
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set, sea_query::Expr};
use serde_json::Value;

macro_rules! request {
    ($app:expr, $token:expr, $method:ident, $uri:expr) => {
        test::call_service(
            &$app,
            test::TestRequest::$method()
                .uri(&$uri)
                .insert_header(("Cookie", common::access_cookie_header(&$token)))
                .insert_header(common::csrf_header_for(&$token))
                .to_request(),
        )
    };
}

async fn set_status(
    state: &aster_drive::runtime::PrimaryAppState,
    id: i64,
    status: BackgroundTaskStatus,
) {
    background_task::Entity::update_many()
        .col_expr(background_task::Column::Status, Expr::value(status))
        // An expired lease must not admit restores: another worker can reclaim the task.
        .col_expr(
            background_task::Column::LeaseExpiresAt,
            Expr::value(Some(Utc::now() - chrono::Duration::hours(1))),
        )
        .filter(background_task::Column::Id.eq(id))
        .exec(state.writer_db())
        .await
        .unwrap();
}

async fn lifecycle(team: bool) {
    let state = common::setup_with_pool_size(4).await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let user = user_repo::find_by_username(state.writer_db(), "testuser")
        .await
        .unwrap()
        .unwrap();
    let team_id = if team {
        let resp = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/teams")
                .insert_header(("Cookie", common::access_cookie_header(&token)))
                .insert_header(common::csrf_header_for(&token))
                .set_json(serde_json::json!({"name":"Trash race team"}))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), 201);
        let body: Value = test::read_body_json(resp).await;
        Some(body["data"]["id"].as_i64().unwrap())
    } else {
        None
    };
    let base = team_id.map_or("/api/v1".to_string(), |id| format!("/api/v1/teams/{id}"));
    let (status, body) = common::upload_via_server_session(
        &app,
        &token,
        &format!("{base}/files"),
        "restore.txt",
        "text/plain",
        b"retain me",
    )
    .await;
    assert_eq!(status, 201);
    let file_id = body["data"]["id"].as_i64().unwrap();
    let now = Utc::now();
    let root = folder::ActiveModel {
        name: Set("small".into()),
        parent_id: Set(None),
        team_id: Set(team_id),
        owner_user_id: Set(if team { None } else { Some(user.id) }),
        created_by_user_id: Set(Some(user.id)),
        created_by_username: Set(user.username.clone()),
        created_at: Set(now),
        updated_at: Set(now),
        deleted_at: Set(Some(now)),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .unwrap();
    let mut large = root.clone();
    let mut active: folder::ActiveModel = large.clone().into();
    active.id = sea_orm::NotSet;
    active.name = Set("large".into());
    large = active.insert(state.writer_db()).await.unwrap();
    let mut parent = large.id;
    for depth in 0..130 {
        let mut child: folder::ActiveModel = large.clone().into();
        child.id = sea_orm::NotSet;
        child.name = Set(format!("depth-{depth}"));
        child.parent_id = Set(Some(parent));
        parent = child.insert(state.writer_db()).await.unwrap().id;
    }
    file::Entity::update_many()
        .col_expr(file::Column::DeletedAt, Expr::value(Some(now)))
        .filter(file::Column::Id.eq(file_id))
        .exec(state.writer_db())
        .await
        .unwrap();

    let resp = request!(app, token, delete, format!("{base}/trash")).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let task_id = body["data"]["id"].as_i64().unwrap();
    for status in [
        BackgroundTaskStatus::Pending,
        BackgroundTaskStatus::Processing,
        BackgroundTaskStatus::Retry,
        BackgroundTaskStatus::Canceling,
    ] {
        set_status(&state, task_id, status).await;
        for (kind, id) in [("file", file_id), ("folder", root.id), ("folder", large.id)] {
            let resp = request!(
                app,
                token,
                post,
                format!("{base}/trash/{kind}/{id}/restore")
            )
            .await;
            assert_eq!(resp.status(), 409, "{status:?}: {kind} {id}");
            let body: Value = test::read_body_json(resp).await;
            assert_eq!(body["code"], "trash.purge_in_progress");
        }
        assert!(
            file_repo::find_by_id(state.writer_db(), file_id)
                .await
                .unwrap()
                .deleted_at
                .is_some()
        );
        assert!(
            folder_repo::find_by_id(state.writer_db(), root.id)
                .await
                .unwrap()
                .deleted_at
                .is_some()
        );
    }
    // No in-memory unlock operation is involved in observing terminal state.
    for status in [
        BackgroundTaskStatus::Succeeded,
        BackgroundTaskStatus::Failed,
        BackgroundTaskStatus::Canceled,
    ] {
        set_status(&state, task_id, status).await;
        for (kind, id) in [("file", file_id), ("folder", root.id)] {
            let resp = request!(
                app,
                token,
                post,
                format!("{base}/trash/{kind}/{id}/restore")
            )
            .await;
            assert_eq!(resp.status(), 200, "{status:?}: {kind}");
        }
        file::Entity::update_many()
            .col_expr(file::Column::DeletedAt, Expr::value(Some(now)))
            .filter(file::Column::Id.eq(file_id))
            .exec(state.writer_db())
            .await
            .unwrap();
        folder::Entity::update_many()
            .col_expr(folder::Column::DeletedAt, Expr::value(Some(now)))
            .filter(folder::Column::Id.eq(root.id))
            .exec(state.writer_db())
            .await
            .unwrap();
    }
    // Crossing the synchronous depth budget can queue a restore once the purge is terminal.
    let resp = request!(
        app,
        token,
        post,
        format!("{base}/trash/folder/{}/restore", large.id)
    )
    .await;
    assert_eq!(resp.status(), 202);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["kind"], "folder_tree_mutation");
    let queued_restore_id = body["data"]["id"].as_i64().unwrap();
    let stats = common::drain_task_to_terminal(&state, queued_restore_id).await;
    assert_eq!(stats.failed, 0, "{stats:?}");
    let restored_task = background_task::Entity::find_by_id(queued_restore_id)
        .one(state.writer_db())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        restored_task.status,
        BackgroundTaskStatus::Succeeded,
        "{restored_task:?}"
    );
    assert!(
        folder_repo::find_by_id(state.writer_db(), large.id)
            .await
            .unwrap()
            .deleted_at
            .is_none()
    );
}

#[actix_web::test]
async fn personal_trash_restore_lifecycle() {
    lifecycle(false).await;
}
#[actix_web::test]
async fn team_trash_restore_lifecycle() {
    lifecycle(true).await;
}

async fn race(team: bool, folder_mode: bool) {
    let state = common::setup_with_pool_size(4).await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let base = if team {
        let resp = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/teams")
                .insert_header(("Cookie", common::access_cookie_header(&token)))
                .insert_header(common::csrf_header_for(&token))
                .set_json(serde_json::json!({"name":"Concurrent trash"}))
                .to_request(),
        )
        .await;
        let body: Value = test::read_body_json(resp).await;
        format!("/api/v1/teams/{}", body["data"]["id"].as_i64().unwrap())
    } else {
        "/api/v1".to_string()
    };
    for iteration in 0..4 {
        let folder_id = if folder_mode {
            let response = test::call_service(
                &app,
                test::TestRequest::post()
                    .uri(&format!("{base}/folders"))
                    .insert_header(("Cookie", common::access_cookie_header(&token)))
                    .insert_header(common::csrf_header_for(&token))
                    .set_json(serde_json::json!({"name":format!("race-folder-{iteration}")}))
                    .to_request(),
            )
            .await;
            let body: Value = test::read_body_json(response).await;
            Some(body["data"]["id"].as_i64().unwrap())
        } else {
            None
        };
        let upload_path = folder_id.map_or(format!("{base}/files"), |id| {
            format!("{base}/files?folder_id={id}")
        });
        let (status, body) = common::upload_via_server_session(
            &app,
            &token,
            &upload_path,
            &format!("race-{iteration}.txt"),
            "text/plain",
            b"restore wins or is rejected",
        )
        .await;
        assert_eq!(status, 201);
        let id = body["data"]["id"].as_i64().unwrap();
        let (resource, entity, target) =
            folder_id.map_or(("files", "file", id), |id| ("folders", "folder", id));
        assert_eq!(
            request!(app, token, delete, format!("{base}/{resource}/{target}"))
                .await
                .status(),
            200
        );
        let restore = request!(
            app,
            token,
            post,
            format!("{base}/trash/{entity}/{target}/restore")
        );
        let purge = request!(app, token, delete, format!("{base}/trash"));
        let (restored, queued) = if iteration % 2 == 0 {
            tokio::join!(restore, purge)
        } else {
            let (queued, restored) = tokio::join!(purge, restore);
            (restored, queued)
        };
        assert_eq!(queued.status(), 200);
        assert!(matches!(restored.status().as_u16(), 200 | 409));
        let succeeded = restored.status() == 200;
        let task: Value = test::read_body_json(queued).await;
        let task_id = task["data"]["id"].as_i64().unwrap();
        let stats = common::drain_task_to_terminal(&state, task_id).await;
        assert_eq!(
            stats.failed,
            0,
            "purge failed: {stats:?}, tasks: {:?}",
            background_task::Entity::find()
                .all(state.writer_db())
                .await
                .unwrap()
        );
        assert_eq!(
            stats.succeeded,
            1,
            "iteration={iteration}, now={}, stats={stats:?}, tasks={:?}",
            Utc::now(),
            background_task::Entity::find()
                .all(state.writer_db())
                .await
                .unwrap()
        );
        let after = file_repo::find_by_id(state.writer_db(), id).await;
        if let Some(folder_id) = folder_id {
            let after = folder_repo::find_by_id(state.writer_db(), folder_id).await;
            assert_eq!(
                after.is_ok(),
                succeeded,
                "restored folder must survive the purge"
            );
        }
        if succeeded {
            assert!(
                after.unwrap().deleted_at.is_none(),
                "a successful restore must survive this purge"
            );
        } else {
            assert!(
                after.is_err(),
                "a rejected restore remains eligible for purge"
            );
        }
    }
}
#[actix_web::test]
async fn personal_trash_restore_enqueue_race() {
    race(false, false).await;
}
#[actix_web::test]
async fn team_trash_restore_enqueue_race() {
    race(true, false).await;
}

#[actix_web::test]
async fn team_scope_blocks_every_member_but_not_other_workspaces() {
    use aster_drive_model::{entities::team_member, types::TeamMemberRole};
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let mut teams = Vec::new();
    for name in ["Locked team", "Unaffected team"] {
        let resp = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/teams")
                .insert_header(("Cookie", common::access_cookie_header(&token)))
                .insert_header(common::csrf_header_for(&token))
                .set_json(serde_json::json!({"name":name}))
                .to_request(),
        )
        .await;
        let body: Value = test::read_body_json(resp).await;
        teams.push(body["data"]["id"].as_i64().unwrap());
    }
    let bases = [
        "/api/v1".to_string(),
        format!("/api/v1/teams/{}", teams[0]),
        format!("/api/v1/teams/{}", teams[1]),
    ];
    let mut files = Vec::new();
    for base in &bases {
        let (status, body) = common::upload_via_server_session(
            &app,
            &token,
            &format!("{base}/files"),
            "scope.txt",
            "text/plain",
            b"scope isolation",
        )
        .await;
        assert_eq!(status, 201);
        let id = body["data"]["id"].as_i64().unwrap();
        assert_eq!(
            request!(app, token, delete, format!("{base}/files/{id}"))
                .await
                .status(),
            200
        );
        files.push(id);
    }
    let resp = request!(app, token, delete, format!("{}/trash", bases[1])).await;
    let body: Value = test::read_body_json(resp).await;
    let purge = body["data"]["id"].as_i64().unwrap();
    common::create_test_account_via_api(
        &app,
        state.writer_db(),
        &state.mail_sender,
        "othermember",
        "othermember@example.com",
        "password123",
    )
    .await;
    let member = user_repo::find_by_username(state.writer_db(), "othermember")
        .await
        .unwrap()
        .unwrap();
    let now = Utc::now();
    team_member::ActiveModel {
        team_id: Set(teams[0]),
        user_id: Set(member.id),
        role: Set(TeamMemberRole::Member),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .unwrap();
    let error = aster_drive::services::files::trash::restore_team_file(
        &state, teams[0], files[1], member.id,
    )
    .await
    .unwrap_err();
    assert_eq!(error.api_error_code().as_str(), "trash.purge_in_progress");
    for index in [0, 2] {
        assert_eq!(
            request!(
                app,
                token,
                post,
                format!("{}/trash/file/{}/restore", bases[index], files[index])
            )
            .await
            .status(),
            200
        );
    }
    // Canceling just one of two tasks must not release the scope.
    let resp = request!(app, token, delete, format!("{}/trash", bases[1])).await;
    let body: Value = test::read_body_json(resp).await;
    let second = body["data"]["id"].as_i64().unwrap();
    assert_eq!(
        request!(
            app,
            token,
            post,
            format!("{}/tasks/{purge}/cancel", bases[1])
        )
        .await
        .status(),
        200
    );
    assert_eq!(
        request!(
            app,
            token,
            post,
            format!("{}/trash/file/{}/restore", bases[1], files[1])
        )
        .await
        .status(),
        409
    );
    assert_eq!(
        request!(
            app,
            token,
            post,
            format!("{}/tasks/{second}/cancel", bases[1])
        )
        .await
        .status(),
        200
    );
    assert_eq!(
        request!(
            app,
            token,
            post,
            format!("{}/trash/file/{}/restore", bases[1], files[1])
        )
        .await
        .status(),
        200
    );
}

#[actix_web::test]
async fn fresh_instance_reads_persisted_trash_task_state() {
    let path = std::env::temp_dir().join(format!(
        "asterdrive-trash-restart-{}.db",
        uuid::Uuid::new_v4()
    ));
    let url = format!("sqlite://{}?mode=rwc", path.display());
    let state = common::setup_with_database_url(&url).await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let id = upload_test_file!(app, token);
    assert_eq!(
        request!(app, token, delete, format!("/api/v1/files/{id}"))
            .await
            .status(),
        200
    );
    let resp = request!(app, token, delete, "/api/v1/trash").await;
    let body: Value = test::read_body_json(resp).await;
    let task_id = body["data"]["id"].as_i64().unwrap();
    let user = user_repo::find_by_username(state.writer_db(), "testuser")
        .await
        .unwrap()
        .unwrap();
    // A different pool and cache model a restarted/new API instance; no guard lives in memory.
    let mut restarted = state.clone();
    restarted.db_handles =
        aster_forge_db::DbHandles::single(sea_orm::Database::connect(&url).await.unwrap());
    restarted.cache =
        aster_forge_cache::create_cache(&aster_forge_cache::CacheConfig::default()).await;
    for status in [
        BackgroundTaskStatus::Pending,
        BackgroundTaskStatus::Processing,
        BackgroundTaskStatus::Retry,
        BackgroundTaskStatus::Canceling,
    ] {
        set_status(&state, task_id, status).await;
        let error = aster_drive::services::files::trash::restore_file(&restarted, id, user.id)
            .await
            .unwrap_err();
        assert_eq!(error.api_error_code().as_str(), "trash.purge_in_progress");
    }
    set_status(&state, task_id, BackgroundTaskStatus::Failed).await;
    aster_drive::services::files::trash::restore_file(&restarted, id, user.id)
        .await
        .unwrap();
    assert!(
        file_repo::find_by_id(state.writer_db(), id)
            .await
            .unwrap()
            .deleted_at
            .is_none()
    );
}

#[actix_web::test]
async fn processing_purge_cancellation_retains_remaining_items_and_unlocks_restores() {
    use sea_orm::{ConnectionTrait, DbBackend};
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    // Folders are separate purge batches without needing 101 upload sessions.
    let user = user_repo::find_by_username(state.writer_db(), "testuser")
        .await
        .unwrap()
        .unwrap();
    let now = Utc::now();
    folder_repo::create_many(
        state.writer_db(),
        (0..101)
            .map(|index| folder::ActiveModel {
                name: Set(format!("cancel-root-{index}")),
                owner_user_id: Set(Some(user.id)),
                created_by_user_id: Set(Some(user.id)),
                created_by_username: Set(user.username.clone()),
                created_at: Set(now),
                updated_at: Set(now),
                deleted_at: Set(Some(now)),
                ..Default::default()
            })
            .collect(),
    )
    .await
    .unwrap();
    let resp = request!(app, token, delete, "/api/v1/trash").await;
    let body: Value = test::read_body_json(resp).await;
    let task_id = body["data"]["id"].as_i64().unwrap();
    let trigger = match state.writer_db().get_database_backend() {
        DbBackend::Sqlite => format!(
            "CREATE TRIGGER cancel_trash_batch AFTER DELETE ON folders BEGIN UPDATE background_tasks SET status='canceling' WHERE id={task_id}; END"
        ),
        DbBackend::Postgres => format!(
            "CREATE FUNCTION cancel_trash_batch_fn() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE background_tasks SET status='canceling' WHERE id={task_id}; RETURN OLD; END $$; CREATE TRIGGER cancel_trash_batch AFTER DELETE ON folders FOR EACH ROW EXECUTE FUNCTION cancel_trash_batch_fn();"
        ),
        DbBackend::MySql => format!(
            "CREATE TRIGGER cancel_trash_batch AFTER DELETE ON folders FOR EACH ROW UPDATE background_tasks SET status='canceling' WHERE id={task_id}"
        ),
        backend => panic!("unsupported test backend: {backend:?}"),
    };
    state
        .writer_db()
        .execute_unprepared(&trigger)
        .await
        .unwrap();
    let stats = common::drain_task_to_terminal(&state, task_id).await;
    assert_eq!(stats.canceled, 1, "{stats:?}");
    let task = background_task::Entity::find_by_id(task_id)
        .one(state.writer_db())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(task.status, BackgroundTaskStatus::Canceled);
    let remaining = folder::Entity::find()
        .filter(folder::Column::OwnerUserId.eq(user.id))
        .all(state.writer_db())
        .await
        .unwrap();
    assert_eq!(
        remaining.len(),
        1,
        "one batch commits; the next must stop before deletion"
    );
    let id = remaining[0].id;

    assert_eq!(
        request!(
            app,
            token,
            post,
            format!("/api/v1/trash/folder/{id}/restore")
        )
        .await
        .status(),
        200
    );
}

#[actix_web::test]
async fn personal_folder_restore_enqueue_race() {
    race(false, true).await;
}
#[actix_web::test]
async fn team_folder_restore_enqueue_race() {
    race(true, true).await;
}

#[actix_web::test]
async fn failed_purge_rolls_back_deletion_and_releases_restore_restriction() {
    use sea_orm::{ConnectionTrait, DbBackend};
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let id = upload_test_file!(app, token);
    assert_eq!(
        request!(app, token, delete, format!("/api/v1/files/{id}"))
            .await
            .status(),
        200
    );
    let resp = request!(app, token, delete, "/api/v1/trash").await;
    let body: Value = test::read_body_json(resp).await;
    let task_id = body["data"]["id"].as_i64().unwrap();
    let trigger = match state.writer_db().get_database_backend() {
        DbBackend::Sqlite => {
            "CREATE TRIGGER fail_trash_delete BEFORE DELETE ON files BEGIN SELECT RAISE(ABORT, 'injected trash delete failure'); END"
        }
        DbBackend::Postgres => {
            "CREATE FUNCTION fail_trash_delete_fn() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected trash delete failure'; END $$; CREATE TRIGGER fail_trash_delete BEFORE DELETE ON files FOR EACH ROW EXECUTE FUNCTION fail_trash_delete_fn();"
        }
        DbBackend::MySql => {
            "CREATE TRIGGER fail_trash_delete BEFORE DELETE ON files FOR EACH ROW SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT='injected trash delete failure'"
        }
        backend => panic!("unsupported test backend: {backend:?}"),
    };
    state.writer_db().execute_unprepared(trigger).await.unwrap();
    let before = file_repo::find_by_id(state.writer_db(), id).await.unwrap();
    let user = user_repo::find_by_username(state.writer_db(), "testuser")
        .await
        .unwrap()
        .unwrap();
    let stats = common::drain_task_to_terminal(&state, task_id).await;
    assert_eq!(stats.failed, 1, "{stats:?}");
    let task = background_task::Entity::find_by_id(task_id)
        .one(state.writer_db())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(task.status, BackgroundTaskStatus::Failed);
    assert_eq!(
        file_repo::find_by_id(state.writer_db(), id).await.unwrap(),
        before
    );
    assert_eq!(
        user_repo::find_by_username(state.writer_db(), "testuser")
            .await
            .unwrap()
            .unwrap()
            .storage_used,
        user.storage_used
    );
    assert_eq!(
        request!(app, token, post, format!("/api/v1/trash/file/{id}/restore"))
            .await
            .status(),
        200
    );
}

#[actix_web::test]
async fn lease_takeover_keeps_restores_blocked_until_the_current_owner_finishes() {
    use aster_drive::db::repository::background_task_repo;
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let id = upload_test_file!(app, token);
    assert_eq!(
        request!(app, token, delete, format!("/api/v1/files/{id}"))
            .await
            .status(),
        200
    );
    let response = request!(app, token, delete, "/api/v1/trash").await;
    let body: Value = test::read_body_json(response).await;
    let task_id = body["data"]["id"].as_i64().unwrap();
    let now = Utc::now();
    assert!(
        background_task_repo::try_claim(
            state.writer_db(),
            task_id,
            0,
            now,
            now - chrono::Duration::minutes(10),
            1,
            now - chrono::Duration::seconds(1)
        )
        .await
        .unwrap()
    );
    // The expired owner still blocks admission while another instance reclaims it.
    assert_eq!(
        request!(app, token, post, format!("/api/v1/trash/file/{id}/restore"))
            .await
            .status(),
        409
    );
    assert!(
        background_task_repo::try_claim(
            state.writer_db(),
            task_id,
            1,
            now,
            now - chrono::Duration::minutes(10),
            2,
            now + chrono::Duration::minutes(1)
        )
        .await
        .unwrap()
    );
    assert!(
        !background_task_repo::touch_heartbeat(
            state.writer_db(),
            task_id,
            1,
            now,
            now + chrono::Duration::minutes(1)
        )
        .await
        .unwrap()
    );
    assert_eq!(
        request!(app, token, post, format!("/api/v1/trash/file/{id}/restore"))
            .await
            .status(),
        409
    );
    for (processing_token, expected) in [(1, false), (2, true)] {
        assert_eq!(
            background_task_repo::mark_failed(
                state.writer_db(),
                background_task_repo::TaskFailureUpdate {
                    id: task_id,
                    processing_token,
                    attempt_count: 1,
                    last_error: "fixture worker failure",
                    finished_at: now,
                    expires_at: now + chrono::Duration::hours(1),
                    steps_json: None,
                    failure_can_retry: true,
                }
            )
            .await
            .unwrap(),
            expected
        );
    }
    assert_eq!(
        request!(app, token, post, format!("/api/v1/trash/file/{id}/restore"))
            .await
            .status(),
        200
    );
}

#[actix_web::test]
async fn manual_purge_retry_and_restore_are_serialized() {
    let state = common::setup_with_pool_size(4).await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    for status in [BackgroundTaskStatus::Failed, BackgroundTaskStatus::Canceled] {
        let (uploaded, body) = common::upload_via_server_session(
            &app,
            &token,
            "/api/v1/files",
            &format!("retry-{}.txt", status.as_str()),
            "text/plain",
            b"retry race",
        )
        .await;
        assert_eq!(uploaded, 201);
        let id = body["data"]["id"].as_i64().unwrap();
        assert_eq!(
            request!(app, token, delete, format!("/api/v1/files/{id}"))
                .await
                .status(),
            200
        );
        let response = request!(app, token, delete, "/api/v1/trash").await;
        let body: Value = test::read_body_json(response).await;
        let task_id = body["data"]["id"].as_i64().unwrap();
        set_status(&state, task_id, status).await;
        let retry = request!(app, token, post, format!("/api/v1/tasks/{task_id}/retry"));
        let restore = request!(app, token, post, format!("/api/v1/trash/file/{id}/restore"));
        let (retry, restore) = tokio::join!(retry, restore);
        assert_eq!(retry.status(), 200);
        assert!(matches!(restore.status().as_u16(), 200 | 409));
        let restored = restore.status() == 200;
        let stats = common::drain_task_to_terminal(&state, task_id).await;
        assert_eq!(stats.failed, 0, "{stats:?}");
        let after = file_repo::find_by_id(state.writer_db(), id).await;
        assert_eq!(after.is_ok(), restored);
        if let Ok(file) = after {
            assert!(file.deleted_at.is_none());
        }
    }
}

async fn deletion_lifecycle(team: bool) {
    use aster_drive_model::entities::file_blob;
    let state = common::setup_with_pool_size(4).await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let owner = user_repo::find_by_username(state.writer_db(), "testuser")
        .await
        .unwrap()
        .unwrap();
    let team_id = if team {
        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/teams")
                .insert_header(("Cookie", common::access_cookie_header(&token)))
                .insert_header(common::csrf_header_for(&token))
                .set_json(serde_json::json!({"name":"Deletion coordination"}))
                .to_request(),
        )
        .await;
        let body: Value = test::read_body_json(response).await;
        Some(body["data"]["id"].as_i64().unwrap())
    } else {
        None
    };
    let base = team_id.map_or("/api/v1".into(), |id| format!("/api/v1/teams/{id}"));
    let now = Utc::now();
    let mut folders = Vec::new();
    for (name, deleted) in [
        ("active", false),
        ("queued-delete", false),
        ("trashed", true),
    ] {
        let root = folder::ActiveModel {
            name: Set(name.into()),
            parent_id: Set(None),
            team_id: Set(team_id),
            owner_user_id: Set(if team { None } else { Some(owner.id) }),
            created_by_user_id: Set(Some(owner.id)),
            created_by_username: Set(owner.username.clone()),
            created_at: Set(now),
            updated_at: Set(now),
            deleted_at: Set(deleted.then_some(now)),
            ..Default::default()
        }
        .insert(state.writer_db())
        .await
        .unwrap();
        folders.push(root);
    }
    let mut parent = folders[1].id;
    for depth in 0..130 {
        let mut child: folder::ActiveModel = folders[1].clone().into();
        child.id = sea_orm::NotSet;
        child.parent_id = Set(Some(parent));
        child.name = Set(format!("child-{depth}"));
        parent = child.insert(state.writer_db()).await.unwrap().id;
    }
    let mut files = Vec::new();
    for (name, deleted) in [("active.txt", false), ("trashed.txt", true)] {
        let (status, body) = common::upload_via_server_session(
            &app,
            &token,
            &format!("{base}/files"),
            name,
            "text/plain",
            b"deletion protection",
        )
        .await;
        assert_eq!(status, 201);
        let id = body["data"]["id"].as_i64().unwrap();
        if deleted {
            assert_eq!(
                request!(app, token, delete, format!("{base}/files/{id}"))
                    .await
                    .status(),
                200
            );
        }
        files.push(file_repo::find_by_id(state.writer_db(), id).await.unwrap());
    }
    let blobs_before = file_blob::Entity::find()
        .all(state.writer_db())
        .await
        .unwrap();
    let response = request!(app, token, delete, format!("{base}/trash")).await;
    let body: Value = test::read_body_json(response).await;
    let task_id = body["data"]["id"].as_i64().unwrap();
    for status in [
        BackgroundTaskStatus::Pending,
        BackgroundTaskStatus::Processing,
        BackgroundTaskStatus::Retry,
        BackgroundTaskStatus::Canceling,
    ] {
        set_status(&state, task_id, status).await;
        for url in [
            format!("{base}/files/{}", files[0].id),
            format!("{base}/folders/{}", folders[0].id),
            format!("{base}/folders/{}", folders[1].id),
            format!("{base}/trash/file/{}", files[1].id),
            format!("{base}/trash/folder/{}", folders[2].id),
        ] {
            let response = request!(app, token, delete, url).await;
            assert_eq!(response.status(), 409, "{status:?}");
            let body: Value = test::read_body_json(response).await;
            assert_eq!(body["code"], "trash.purge_in_progress");
        }
        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri(&format!("{base}/batch/delete"))
                .insert_header(("Cookie", common::access_cookie_header(&token)))
                .insert_header(common::csrf_header_for(&token))
                .set_json(
                    serde_json::json!({"file_ids":[files[0].id], "folder_ids":[folders[0].id]}),
                )
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), 409, "batch {status:?}");
        let body: Value = test::read_body_json(response).await;
        assert_eq!(body["code"], "trash.purge_in_progress");
        // The trash UI submits bulk permanent deletes as concurrent single-item requests.
        let (file_response, folder_response) = tokio::join!(
            request!(
                app,
                token,
                delete,
                format!("{base}/trash/file/{}", files[1].id)
            ),
            request!(
                app,
                token,
                delete,
                format!("{base}/trash/folder/{}", folders[2].id)
            ),
        );
        assert_eq!(file_response.status(), 409);
        assert_eq!(folder_response.status(), 409);
        for before in &files {
            assert_eq!(
                file_repo::find_by_id(state.writer_db(), before.id)
                    .await
                    .unwrap(),
                *before
            );
        }
        for before in &folders {
            assert_eq!(
                folder_repo::find_by_id(state.writer_db(), before.id)
                    .await
                    .unwrap(),
                *before
            );
        }
        assert_eq!(
            file_blob::Entity::find()
                .all(state.writer_db())
                .await
                .unwrap(),
            blobs_before
        );
        assert_eq!(
            background_task::Entity::find()
                .all(state.writer_db())
                .await
                .unwrap()
                .len(),
            1,
            "rejected large deletion cannot enqueue work"
        );
    }
    // Both classes of deletion become available again for every terminal state.
    for status in [
        BackgroundTaskStatus::Succeeded,
        BackgroundTaskStatus::Failed,
        BackgroundTaskStatus::Canceled,
    ] {
        set_status(&state, task_id, status).await;
        assert_eq!(
            request!(app, token, delete, format!("{base}/files/{}", files[0].id))
                .await
                .status(),
            200
        );
        assert_eq!(
            request!(
                app,
                token,
                delete,
                format!("{base}/folders/{}", folders[0].id)
            )
            .await
            .status(),
            200
        );
        assert_eq!(
            request!(
                app,
                token,
                post,
                format!("{base}/trash/file/{}/restore", files[0].id)
            )
            .await
            .status(),
            200
        );
        assert_eq!(
            request!(
                app,
                token,
                post,
                format!("{base}/trash/folder/{}/restore", folders[0].id)
            )
            .await
            .status(),
            200
        );
    }
    assert_eq!(
        request!(
            app,
            token,
            delete,
            format!("{base}/trash/file/{}", files[1].id)
        )
        .await
        .status(),
        200
    );
    assert_eq!(
        request!(
            app,
            token,
            delete,
            format!("{base}/trash/folder/{}", folders[2].id)
        )
        .await
        .status(),
        200
    );
}

#[actix_web::test]
async fn personal_trash_blocks_soft_and_permanent_deletion() {
    deletion_lifecycle(false).await;
}
#[actix_web::test]
async fn team_trash_blocks_soft_and_permanent_deletion() {
    deletion_lifecycle(true).await;
}

#[actix_web::test]
async fn mysql_prior_snapshot_cannot_hide_a_committed_purge_admission() {
    use aster_drive::db::repository::{background_task_repo, lock_namespace_repo};
    use aster_drive_model::types::{BackgroundTaskKind, LockWorkspaceType};
    use sea_orm::TransactionTrait;
    let state = common::setup_with_pool_size(4).await;
    if state.writer_db().get_database_backend() != sea_orm::DbBackend::MySql {
        return;
    }
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let user = user_repo::find_by_username(state.writer_db(), "testuser")
        .await
        .unwrap()
        .unwrap();
    let txn = state.writer_db().begin().await.unwrap();
    // A protocol adapter can resolve its resource before reaching the shared delete service.
    assert!(
        background_task::Entity::find()
            .all(&txn)
            .await
            .unwrap()
            .is_empty()
    );
    let response = request!(app, token, delete, "/api/v1/trash").await;
    assert_eq!(response.status(), 200);
    lock_namespace_repo::ensure_and_lock(&txn, LockWorkspaceType::Personal, user.id)
        .await
        .unwrap();
    assert!(
        background_task_repo::find_active_by_kind_in_workspace(
            &txn,
            BackgroundTaskKind::TrashPurgeAll,
            LockWorkspaceType::Personal,
            user.id
        )
        .await
        .unwrap()
        .is_some()
    );
    txn.rollback().await.unwrap();
}

#[actix_web::test]
async fn permanent_folder_delete_rolls_back_files_versions_blobs_and_quota() {
    use aster_drive_model::entities::{file_blob, file_revision, file_revision_history};
    use sea_orm::{ConnectionTrait, DbBackend};
    let state = common::setup().await;
    let mut events = state.storage_change_bus.subscribe();
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/folders")
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .set_json(serde_json::json!({"name":"Atomic permanent deletion"}))
            .to_request(),
    )
    .await;
    let body: Value = test::read_body_json(response).await;
    let folder_id = body["data"]["id"].as_i64().unwrap();
    let (status, body) = common::upload_via_server_session(
        &app,
        &token,
        &format!("/api/v1/files?folder_id={folder_id}"),
        "retained.txt",
        "text/plain",
        b"must survive failed folder purge",
    )
    .await;
    assert_eq!(status, 201);
    let file_id = body["data"]["id"].as_i64().unwrap();
    assert_eq!(
        request!(app, token, delete, format!("/api/v1/folders/{folder_id}"))
            .await
            .status(),
        200
    );
    let before_file = file_repo::find_by_id(state.writer_db(), file_id)
        .await
        .unwrap();
    let before_folder = folder_repo::find_by_id(state.writer_db(), folder_id)
        .await
        .unwrap();
    let before_blobs = file_blob::Entity::find()
        .all(state.writer_db())
        .await
        .unwrap();
    let before_revisions = file_revision::Entity::find()
        .all(state.writer_db())
        .await
        .unwrap();
    let before_histories = file_revision_history::Entity::find()
        .all(state.writer_db())
        .await
        .unwrap();
    let before_user = user_repo::find_by_username(state.writer_db(), "testuser")
        .await
        .unwrap()
        .unwrap();
    while events.try_recv().is_ok() {}
    let trigger = match state.writer_db().get_database_backend() {
        DbBackend::Sqlite => {
            "CREATE TRIGGER fail_folder_purge BEFORE DELETE ON folders BEGIN SELECT RAISE(ABORT, 'injected folder purge failure'); END"
        }
        DbBackend::Postgres => {
            "CREATE FUNCTION fail_folder_purge_fn() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected folder purge failure'; END $$; CREATE TRIGGER fail_folder_purge BEFORE DELETE ON folders FOR EACH ROW EXECUTE FUNCTION fail_folder_purge_fn();"
        }
        DbBackend::MySql => {
            "CREATE TRIGGER fail_folder_purge BEFORE DELETE ON folders FOR EACH ROW SIGNAL SQLSTATE '45000' SET MESSAGE_TEXT='injected folder purge failure'"
        }
        backend => panic!("unsupported test backend: {backend:?}"),
    };
    state.writer_db().execute_unprepared(trigger).await.unwrap();
    let response = request!(
        app,
        token,
        delete,
        format!("/api/v1/trash/folder/{folder_id}")
    )
    .await;
    assert_eq!(response.status(), 500);
    assert_eq!(
        file_repo::find_by_id(state.writer_db(), file_id)
            .await
            .unwrap(),
        before_file
    );
    assert_eq!(
        folder_repo::find_by_id(state.writer_db(), folder_id)
            .await
            .unwrap(),
        before_folder
    );
    assert_eq!(
        file_blob::Entity::find()
            .all(state.writer_db())
            .await
            .unwrap(),
        before_blobs
    );
    assert_eq!(
        file_revision::Entity::find()
            .all(state.writer_db())
            .await
            .unwrap(),
        before_revisions
    );
    assert_eq!(
        file_revision_history::Entity::find()
            .all(state.writer_db())
            .await
            .unwrap(),
        before_histories
    );
    assert_eq!(
        user_repo::find_by_username(state.writer_db(), "testuser")
            .await
            .unwrap()
            .unwrap()
            .storage_used,
        before_user.storage_used
    );
    assert!(
        events.try_recv().is_err(),
        "rolled back deletion must not publish changes"
    );
    assert_eq!(
        request!(
            app,
            token,
            post,
            format!("/api/v1/trash/folder/{folder_id}/restore")
        )
        .await
        .status(),
        200
    );
    let response = request!(app, token, get, format!("/api/v1/files/{file_id}/download")).await;
    assert_eq!(response.status(), 200);
    assert_eq!(
        test::read_body(response).await.as_ref(),
        b"must survive failed folder purge"
    );
}

async fn deletion_enqueue_race(team: bool, permanent: bool) {
    let state = common::setup_with_pool_size(4).await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let base = if team {
        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/teams")
                .insert_header(("Cookie", common::access_cookie_header(&token)))
                .insert_header(common::csrf_header_for(&token))
                .set_json(serde_json::json!({"name":"Concurrent deletion"}))
                .to_request(),
        )
        .await;
        let body: Value = test::read_body_json(response).await;
        format!("/api/v1/teams/{}", body["data"]["id"].as_i64().unwrap())
    } else {
        "/api/v1".into()
    };
    for iteration in 0..4 {
        let (status, body) = common::upload_via_server_session(
            &app,
            &token,
            &format!("{base}/files"),
            &format!("delete-race-{iteration}.txt"),
            "text/plain",
            b"one quota decrement",
        )
        .await;
        assert_eq!(status, 201);
        let id = body["data"]["id"].as_i64().unwrap();
        if permanent {
            assert_eq!(
                request!(app, token, delete, format!("{base}/files/{id}"))
                    .await
                    .status(),
                200
            );
        }
        let url = if permanent {
            format!("{base}/trash/file/{id}")
        } else {
            format!("{base}/files/{id}")
        };
        let deletion = request!(app, token, delete, url);
        let empty = request!(app, token, delete, format!("{base}/trash"));
        let (deleted, queued) = if iteration % 2 == 0 {
            tokio::join!(deletion, empty)
        } else {
            let (queued, deleted) = tokio::join!(empty, deletion);
            (deleted, queued)
        };
        assert!(matches!(deleted.status().as_u16(), 200 | 409));
        assert_eq!(queued.status(), 200);
        let rejected = deleted.status() == 409;
        let body: Value = test::read_body_json(queued).await;
        let stats =
            common::drain_task_to_terminal(&state, body["data"]["id"].as_i64().unwrap()).await;
        assert_eq!(stats.failed, 0, "{stats:?}");
        let after = file_repo::find_by_id(state.writer_db(), id).await;
        if !permanent && rejected {
            assert!(
                after.unwrap().deleted_at.is_none(),
                "a rejected soft delete must stay outside this purge"
            );
        } else {
            assert!(
                after.is_err(),
                "accepted/previously trashed resource must be removed once"
            );
        }
        if !team {
            let owner = user_repo::find_by_username(state.writer_db(), "testuser")
                .await
                .unwrap()
                .unwrap();
            let active_bytes: i64 = file::Entity::find()
                .filter(file::Column::DeletedAt.is_null())
                .all(state.writer_db())
                .await
                .unwrap()
                .iter()
                .map(|file| file.size)
                .sum();
            assert_eq!(owner.storage_used, active_bytes);
        }
    }
}

#[actix_web::test]
async fn personal_soft_delete_enqueue_race() {
    deletion_enqueue_race(false, false).await;
}
#[actix_web::test]
async fn team_soft_delete_enqueue_race() {
    deletion_enqueue_race(true, false).await;
}
#[actix_web::test]
async fn personal_permanent_delete_enqueue_race() {
    deletion_enqueue_race(false, true).await;
}
#[actix_web::test]
async fn team_permanent_delete_enqueue_race() {
    deletion_enqueue_race(true, true).await;
}
