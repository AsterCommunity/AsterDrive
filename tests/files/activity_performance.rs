//! Query-budget regressions and a repeatable operation-level performance probe.
use crate::common;
use actix_web::test;
use aster_drive::db::repository::file_repo;
use aster_drive_model::entities::{file, workspace_activity};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, Set};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

#[actix_web::test]
async fn batch_move_activity_query_budget() {
    let mut state = common::setup().await;
    let statements = Arc::new(Mutex::new(Vec::<String>::new()));
    let measured = statements.clone();
    let mut db = state.writer_db().clone();
    db.set_metric_callback(move |info| {
        // Capture SQL shape only; never collect bound values or credentials.
        measured.lock().unwrap().push(info.statement.sql.clone());
    });
    state.db_handles = aster_forge_db::DbHandles::single(db);
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let template_id = common::create_empty_file_via_api(&app, &token, "template.txt", None).await;
    let template = file_repo::find_by_id(state.writer_db(), template_id)
        .await
        .unwrap();
    let mut parent = None;
    for depth in 0..8 {
        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/folders")
                .insert_header(("Cookie", common::access_cookie_header(&token)))
                .insert_header(common::csrf_header_for(&token))
                .set_json(json!({"name":format!("level-{depth}"),"parent_id":parent}))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), 201);
        let body: Value = test::read_body_json(response).await;
        parent = body["data"]["id"].as_i64();
    }
    let count = 200;
    for chunk in (0..count).collect::<Vec<_>>().chunks(25) {
        let rows = chunk
            .iter()
            .map(|n| {
                let mut model: file::ActiveModel = template.clone().into();
                model.id = sea_orm::NotSet;
                model.name = Set(format!("budget-{n}.txt"));
                model
            })
            .collect::<Vec<_>>();
        file::Entity::insert_many(rows)
            .exec(state.writer_db())
            .await
            .unwrap();
    }
    let ids = file::Entity::find()
        .filter(file::Column::Name.starts_with("budget-"))
        .all(state.writer_db())
        .await
        .unwrap()
        .into_iter()
        .map(|f| f.id)
        .collect::<Vec<_>>();
    let mut max_inserts = 0;
    for round in 0..3 {
        let target = if round % 2 == 0 { parent } else { None };
        statements.lock().unwrap().clear();
        let start = std::time::Instant::now();
        let response = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/batch/move")
                .insert_header(("Cookie", common::access_cookie_header(&token)))
                .insert_header(common::csrf_header_for(&token))
                .set_json(json!({"file_ids":ids,"folder_ids":[],"target_folder_id":target}))
                .to_request(),
        )
        .await;
        let elapsed = start.elapsed();
        let status = response.status();
        let body: Value = test::read_body_json(response).await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(body["data"]["succeeded"], count);
        let sql = statements.lock().unwrap().clone();
        let inserts = sql
            .iter()
            .filter(|s| s.starts_with("INSERT") && s.contains("workspace_activities"))
            .count();
        let updates = sql
            .iter()
            .filter(|s| s.starts_with("UPDATE") && s.contains("modified_by_username"))
            .count();
        max_inserts = max_inserts.max(inserts);
        eprintln!(
            "activity-move round={round} files={count} depth=8 queries={} activity_inserts={inserts} attribution_updates={updates} elapsed_ms={:.3}",
            sql.len(),
            elapsed.as_secs_f64() * 1000.0
        );
    }
    let rows = workspace_activity::Entity::find()
        .filter(workspace_activity::Column::Action.eq("file_move"))
        .all(state.writer_db())
        .await
        .unwrap();
    assert_eq!(rows.len(), count * 3);
    assert!(
        max_inserts <= count.div_ceil(50),
        "activity inserts must be chunked, got {max_inserts}"
    );
    statements.lock().unwrap().clear();
    let response = test::call_service(
        &app,
        test::TestRequest::delete()
            .uri(&format!("/api/v1/folders/{}", parent.unwrap()))
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), 200);
    let sql = statements.lock().unwrap();
    let inserts = sql
        .iter()
        .filter(|s| s.starts_with("INSERT") && s.contains("workspace_activities"))
        .count();
    let updates = sql
        .iter()
        .filter(|s| s.starts_with("UPDATE") && s.contains("modified_by_username"))
        .count();
    assert!(
        inserts <= count.div_ceil(50) + 1,
        "tree deletion should use chunked inserts, got {inserts}"
    );
    assert!(
        updates <= 2,
        "tree deletion should bulk-update attribution, got {updates}"
    );
}

#[actix_web::test]
async fn no_op_rename_skips_disabled_legacy_audit_path_queries() {
    let mut state = common::setup().await;
    let statements = Arc::new(Mutex::new(Vec::<String>::new()));
    let captured = statements.clone();
    let mut db = state.writer_db().clone();
    db.set_metric_callback(move |info| captured.lock().unwrap().push(info.statement.sql.clone()));
    state.db_handles = aster_forge_db::DbHandles::single(db);
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/folders")
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .set_json(json!({"name":"no-op-audit-root"}))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), 201);
    let body: Value = test::read_body_json(response).await;
    let folder_id = body["data"]["id"].as_i64().unwrap();
    let file_id =
        common::create_empty_file_via_api(&app, &token, "no-op-audit.txt", Some(folder_id)).await;
    for (uri, name) in [
        (format!("/api/v1/folders/{folder_id}"), "no-op-audit-root"),
        (format!("/api/v1/files/{file_id}"), "no-op-audit.txt"),
    ] {
        statements.lock().unwrap().clear();
        let response = test::call_service(
            &app,
            test::TestRequest::patch()
                .uri(&uri)
                .insert_header(("Cookie", common::access_cookie_header(&token)))
                .insert_header(common::csrf_header_for(&token))
                .set_json(json!({"name":name}))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), 200);
        let sql = statements.lock().unwrap();
        let paths = sql
            .iter()
            .filter(|sql| {
                sql.starts_with("SELECT")
                    && (sql.contains("FROM \"folders\" WHERE \"folders\".\"id\" IN (")
                        || sql.contains("FROM `folders` WHERE `folders`.`id` IN ("))
            })
            .collect::<Vec<_>>();
        eprintln!(
            "legacy-audit-noop uri={uri} total={} folder_path_queries={}",
            sql.len(),
            paths.len()
        );
        assert!(
            paths.is_empty(),
            "no-op mutation should not query legacy audit paths: {paths:?}"
        );
    }
}
