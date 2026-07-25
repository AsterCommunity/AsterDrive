//! PostgreSQL / MySQL 生产数据库 smoke tests（使用 testcontainers）

#[macro_use]
mod common;

use actix_web::test;
use sea_orm::{ActiveValue::Set, ConnectionTrait, DatabaseConnection, DbBackend, Statement};
use serde_json::Value;
use tokio::time::{Duration, timeout};

use aster_drive::db::repository::background_task_repo;
use aster_drive::entities::background_task;
use aster_drive::runtime::SharedRuntimeState;
use aster_drive::types::{
    BackgroundTaskKind, BackgroundTaskStatus, StoredTaskPayload, StoredTaskResult,
};

const OLD_BACKGROUND_TASK_DISPLAY_NAME_LIMIT: usize = 255;
const EXPANDED_BACKGROUND_TASK_DISPLAY_NAME_LIMIT: usize = 512;

fn upload_named_file(name: &str, content: &str, mime: &str, boundary: &str) -> String {
    format!(
        "--{boundary}\r\n\
         Content-Disposition: form-data; name=\"file\"; filename=\"{name}\"\r\n\
         Content-Type: {mime}\r\n\r\n\
         {content}\r\n\
         --{boundary}--\r\n"
    )
}

async fn wait_for_database(database_url: &str) {
    let mut last_err: Option<String> = None;
    let ready = tokio::time::timeout(std::time::Duration::from_secs(60), async {
        loop {
            let cfg = aster_drive::config::DatabaseConfig {
                url: database_url.to_string(),
                pool_size: 1,
                retry_count: 0,
            };
            match aster_drive::db::connect_with_metrics(
                &cfg,
                aster_drive::metrics::NoopMetrics::arc(),
            )
            .await
            {
                Ok(_) => break,
                Err(err) => {
                    last_err = Some(err.to_string());
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                }
            }
        }
    })
    .await;

    if ready.is_err() {
        panic!(
            "timed out waiting for database {database_url}: {}",
            last_err.unwrap_or_else(|| "unknown error".to_string())
        );
    }
}

async fn assert_postgres_search_objects(db: &DatabaseConnection) {
    let extension = db
        .query_one_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT extname FROM pg_extension WHERE extname = 'pg_trgm'",
        ))
        .await
        .unwrap();
    assert!(extension.is_some(), "pg_trgm extension should exist");

    let indexes = db
        .query_all_raw(Statement::from_string(
            DbBackend::Postgres,
            "SELECT indexname FROM pg_indexes \
             WHERE schemaname = 'public' \
               AND indexname IN (\
                   'idx_files_live_name_trgm', \
                   'idx_folders_live_name_trgm', \
                   'idx_teams_name_trgm', \
                   'idx_teams_description_trgm', \
                   'idx_users_username_trgm', \
                   'idx_users_email_trgm'\
               )",
        ))
        .await
        .unwrap();
    let names: Vec<String> = indexes
        .into_iter()
        .map(|row| row.try_get_by_index(0).unwrap())
        .collect();
    assert!(names.iter().any(|name| name == "idx_files_live_name_trgm"));
    assert!(
        names
            .iter()
            .any(|name| name == "idx_folders_live_name_trgm")
    );
    assert!(names.iter().any(|name| name == "idx_teams_name_trgm"));
    assert!(
        names
            .iter()
            .any(|name| name == "idx_teams_description_trgm")
    );
    assert!(names.iter().any(|name| name == "idx_users_username_trgm"));
    assert!(names.iter().any(|name| name == "idx_users_email_trgm"));
}

async fn assert_mysql_search_objects(db: &DatabaseConnection) {
    let file_index = db
        .query_one_raw(Statement::from_string(
            DbBackend::MySql,
            "SHOW INDEX FROM files WHERE Key_name = 'idx_files_name_fulltext'",
        ))
        .await
        .unwrap();
    assert!(file_index.is_some(), "files fulltext index should exist");

    let folder_index = db
        .query_one_raw(Statement::from_string(
            DbBackend::MySql,
            "SHOW INDEX FROM folders WHERE Key_name = 'idx_folders_name_fulltext'",
        ))
        .await
        .unwrap();
    assert!(
        folder_index.is_some(),
        "folders fulltext index should exist"
    );

    let user_index = db
        .query_one_raw(Statement::from_string(
            DbBackend::MySql,
            "SHOW INDEX FROM users WHERE Key_name = 'idx_users_search_fulltext'",
        ))
        .await
        .unwrap();
    assert!(user_index.is_some(), "users fulltext index should exist");

    let team_index = db
        .query_one_raw(Statement::from_string(
            DbBackend::MySql,
            "SHOW INDEX FROM teams WHERE Key_name = 'idx_teams_search_fulltext'",
        ))
        .await
        .unwrap();
    assert!(team_index.is_some(), "teams fulltext index should exist");

    let timestamp_count = db
        .query_one_raw(Statement::from_string(
            DbBackend::MySql,
            "SELECT COUNT(*) \
             FROM INFORMATION_SCHEMA.COLUMNS \
             WHERE TABLE_SCHEMA = DATABASE() \
               AND TABLE_NAME <> 'seaql_migrations' \
               AND DATA_TYPE = 'timestamp'",
        ))
        .await
        .unwrap()
        .expect("timestamp count query should return one row");
    let timestamp_count: i64 = timestamp_count.try_get_by_index(0).unwrap();
    assert_eq!(
        timestamp_count, 0,
        "application tables should not retain MySQL TIMESTAMP columns after the 2038 fix"
    );

    let shares_expires_at = db
        .query_one_raw(Statement::from_string(
            DbBackend::MySql,
            "SELECT DATA_TYPE, DATETIME_PRECISION \
             FROM INFORMATION_SCHEMA.COLUMNS \
             WHERE TABLE_SCHEMA = DATABASE() \
               AND TABLE_NAME = 'shares' \
               AND COLUMN_NAME = 'expires_at'",
        ))
        .await
        .unwrap()
        .expect("shares.expires_at column should exist");
    let data_type: String = shares_expires_at.try_get_by_index(0).unwrap();
    let precision: Option<u64> = shares_expires_at.try_get_by_index(1).unwrap();
    assert_eq!(data_type, "datetime");
    assert_eq!(precision, Some(6));
}

async fn assert_upload_session_kind_column(db: &DatabaseConnection, backend: DbBackend) {
    let statement = match backend {
        DbBackend::Postgres => Statement::from_string(
            DbBackend::Postgres,
            "SELECT is_nullable FROM information_schema.columns \
             WHERE table_schema = current_schema() \
               AND table_name = 'upload_sessions' \
               AND column_name = 'session_kind'",
        ),
        DbBackend::MySql => Statement::from_string(
            DbBackend::MySql,
            "SELECT is_nullable FROM information_schema.columns \
             WHERE table_schema = DATABASE() \
               AND table_name = 'upload_sessions' \
               AND column_name = 'session_kind'",
        ),
        _ => unreachable!("only postgres/mysql smoke tests use this helper"),
    };
    let row = db
        .query_one_raw(statement)
        .await
        .expect("upload session kind column lookup should succeed")
        .expect("upload_sessions.session_kind should exist");
    let is_nullable: String = row
        .try_get_by_index(0)
        .expect("session_kind metadata should include is_nullable");
    assert_eq!(is_nullable, "NO");
}

async fn assert_background_task_display_name_column_len(
    db: &DatabaseConnection,
    backend: DbBackend,
) {
    let sql = match backend {
        DbBackend::Postgres => {
            "SELECT character_maximum_length::bigint \
             FROM information_schema.columns \
             WHERE table_schema = 'public' \
               AND table_name = 'background_tasks' \
               AND column_name = 'display_name'"
        }
        DbBackend::MySql => {
            "SELECT CAST(CHARACTER_MAXIMUM_LENGTH AS SIGNED) \
             FROM INFORMATION_SCHEMA.COLUMNS \
             WHERE TABLE_SCHEMA = DATABASE() \
               AND TABLE_NAME = 'background_tasks' \
               AND COLUMN_NAME = 'display_name'"
        }
        backend => panic!("unsupported test database backend: {backend:?}"),
    };

    let row = db
        .query_one_raw(Statement::from_string(backend, sql))
        .await
        .unwrap()
        .expect("background_tasks.display_name column should exist");
    let max_len: i64 = row.try_get_by_index(0).unwrap();
    assert_eq!(
        max_len,
        i64::try_from(EXPANDED_BACKGROUND_TASK_DISPLAY_NAME_LIMIT).unwrap()
    );
}

async fn assert_background_task_display_name_accepts_expanded_len(db: &DatabaseConnection) {
    let now = chrono::Utc::now();
    let display_name = "x".repeat(OLD_BACKGROUND_TASK_DISPLAY_NAME_LIMIT + 1);
    assert!(display_name.len() <= EXPANDED_BACKGROUND_TASK_DISPLAY_NAME_LIMIT);

    let task = background_task_repo::create(
        db,
        background_task::ActiveModel {
            kind: Set(BackgroundTaskKind::SystemRuntime),
            status: Set(BackgroundTaskStatus::Succeeded),
            creator_user_id: Set(None),
            team_id: Set(None),
            share_id: Set(None),
            display_name: Set(display_name.clone()),
            payload_json: Set(StoredTaskPayload(
                r#"{"task_name":"expanded-display-name-smoke"}"#.to_string(),
            )),
            result_json: Set(Some(StoredTaskResult(
                r#"{"duration_ms":0,"summary":"expanded display name accepted"}"#.to_string(),
            ))),
            runtime_json: Set(None),
            steps_json: Set(None),
            progress_current: Set(1),
            progress_total: Set(1),
            status_text: Set(Some("expanded display name accepted".to_string())),
            attempt_count: Set(0),
            max_attempts: Set(1),
            next_run_at: Set(now),
            processing_token: Set(0),
            processing_started_at: Set(None),
            last_heartbeat_at: Set(None),
            lease_expires_at: Set(None),
            started_at: Set(Some(now)),
            finished_at: Set(Some(now)),
            last_error: Set(None),
            failure_can_retry: Set(None),
            expires_at: Set(now + chrono::Duration::hours(1)),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        },
    )
    .await
    .expect("expanded background task display_name should insert");

    assert_eq!(task.display_name, display_name);
}

#[actix_web::test]
async fn test_sqlite_transactions_are_serialized_by_single_connection_pool() {
    use sea_orm::TransactionTrait;

    let database_path = format!("/tmp/asterdrive-sqlite-lock-{}.db", uuid::Uuid::new_v4());
    let database_url = format!("sqlite://{database_path}");
    let cfg = aster_drive::config::DatabaseConfig {
        url: database_url,
        pool_size: 8,
        retry_count: 0,
    };
    let db = aster_drive::db::connect_with_metrics(&cfg, aster_drive::metrics::NoopMetrics::arc())
        .await
        .unwrap();

    let txn = db.begin().await.unwrap();
    let second_begin = timeout(Duration::from_millis(100), db.begin()).await;
    assert!(
        second_begin.is_err(),
        "SQLite should serialize transactions by exposing only one pooled connection"
    );

    txn.commit().await.unwrap();

    let second_txn = timeout(Duration::from_secs(1), db.begin())
        .await
        .expect("second transaction should start after the first commit")
        .unwrap();
    second_txn.commit().await.unwrap();

    let _ = tokio::fs::remove_file(database_path).await;
}

async fn exercise_backend_smoke(database_url: &str, backend: DbBackend) {
    wait_for_database(database_url).await;

    let state = common::setup_with_database_url(database_url).await;
    match backend {
        DbBackend::Postgres => assert_postgres_search_objects(state.writer_db()).await,
        DbBackend::MySql => assert_mysql_search_objects(state.writer_db()).await,
        _ => unreachable!("only postgres/mysql smoke tests use this helper"),
    }
    assert_background_task_display_name_column_len(state.writer_db(), backend).await;
    assert_background_task_display_name_accepts_expanded_len(state.writer_db()).await;
    assert_upload_session_kind_column(state.writer_db(), backend).await;

    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let share_file_boundary = "----BackendShareBoundary123";
    let share_payload = upload_named_file(
        "shared.txt",
        "shared content",
        "text/plain",
        share_file_boundary,
    );
    let share_upload_req = test::TestRequest::post()
        .uri("/api/v1/files/upload")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .insert_header((
            "Content-Type",
            format!("multipart/form-data; boundary={share_file_boundary}"),
        ))
        .set_payload(share_payload)
        .to_request();
    let share_upload_resp = test::call_service(&app, share_upload_req).await;
    let share_upload_status = share_upload_resp.status();
    if share_upload_status != 201 {
        let body = test::read_body(share_upload_resp).await;
        panic!(
            "share upload returned {share_upload_status}: {}",
            String::from_utf8_lossy(&body)
        );
    }
    let share_upload_body: Value = test::read_body_json(share_upload_resp).await;
    let share_file_id = share_upload_body["data"]["id"]
        .as_i64()
        .expect("share upload should return file id");

    let create_share_req = test::TestRequest::post()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "target": { "type": "file", "id": share_file_id }
        }))
        .to_request();
    let create_share_resp = test::call_service(&app, create_share_req).await;
    let create_share_status = create_share_resp.status();
    if create_share_status != 201 {
        let body = test::read_body(create_share_resp).await;
        panic!(
            "create share returned {create_share_status}: {}",
            String::from_utf8_lossy(&body)
        );
    }
    let create_share_body: Value = test::read_body_json(create_share_resp).await;
    let share_id = create_share_body["data"]["id"]
        .as_i64()
        .expect("create share should return id");

    let update_share_req = test::TestRequest::patch()
        .uri(&format!("/api/v1/shares/{share_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "expires_at": common::TEST_FUTURE_SHARE_EXPIRY_RFC3339,
            "max_downloads": 2
        }))
        .to_request();
    let update_share_resp = test::call_service(&app, update_share_req).await;
    let update_share_status = update_share_resp.status();
    if update_share_status != 200 {
        let body = test::read_body(update_share_resp).await;
        panic!(
            "update share with far-future expiry returned {update_share_status}: {}",
            String::from_utf8_lossy(&body)
        );
    }
    let update_share_body: Value = test::read_body_json(update_share_resp).await;
    assert_eq!(
        update_share_body["data"]["expires_at"],
        common::TEST_FUTURE_SHARE_EXPIRY_RFC3339
    );

    let register_req = test::TestRequest::post()
        .uri("/api/v1/auth/register")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "username": "backend-user",
            "email": "backend-user@example.com",
            "password": "password123"
        }))
        .to_request();
    let register_resp = test::call_service(&app, register_req).await;
    assert_eq!(register_resp.status(), 201);

    let create_team_req = test::TestRequest::post()
        .uri("/api/v1/admin/teams")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Operations",
            "description": "Shared operations workspace",
            "admin_identifier": "backend-user"
        }))
        .to_request();
    let create_team_resp = test::call_service(&app, create_team_req).await;
    let create_team_status = create_team_resp.status();
    if create_team_status != 201 {
        let body = test::read_body(create_team_resp).await;
        panic!(
            "create team returned {create_team_status}: {}",
            String::from_utf8_lossy(&body)
        );
    }
    let create_team_body: Value = test::read_body_json(create_team_resp).await;
    let team_id = create_team_body["data"]["id"]
        .as_i64()
        .expect("created team should return id");

    let admin_team_search_req = test::TestRequest::get()
        .uri("/api/v1/admin/teams?keyword=erat")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let admin_team_search_resp = test::call_service(&app, admin_team_search_req).await;
    let admin_team_search_status = admin_team_search_resp.status();
    if admin_team_search_status != 200 {
        let body = test::read_body(admin_team_search_resp).await;
        panic!(
            "admin team search returned {admin_team_search_status}: {}",
            String::from_utf8_lossy(&body)
        );
    }
    let admin_team_search_body: Value = test::read_body_json(admin_team_search_resp).await;
    assert_eq!(admin_team_search_body["data"]["total"], 1);
    assert_eq!(admin_team_search_body["data"]["items"][0]["id"], team_id);

    let admin_team_member_search_req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/admin/teams/{team_id}/members?keyword=end-u"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let admin_team_member_search_resp =
        test::call_service(&app, admin_team_member_search_req).await;
    let admin_team_member_search_status = admin_team_member_search_resp.status();
    if admin_team_member_search_status != 200 {
        let body = test::read_body(admin_team_member_search_resp).await;
        panic!(
            "admin team member search returned {admin_team_member_search_status}: {}",
            String::from_utf8_lossy(&body)
        );
    }
    let admin_team_member_search_body: Value =
        test::read_body_json(admin_team_member_search_resp).await;
    assert_eq!(admin_team_member_search_body["data"]["total"], 1);
    assert_eq!(
        admin_team_member_search_body["data"]["items"][0]["user"]["username"],
        "backend-user"
    );

    let boundary = "----BackendBoundary123";
    let mut report_file_id = None;
    for (name, mime, content) in [
        ("report.pdf", "application/pdf", "pdf content"),
        ("notes.txt", "text/plain", "notes content"),
    ] {
        let payload = upload_named_file(name, content, mime, boundary);
        let req = test::TestRequest::post()
            .uri("/api/v1/files/upload")
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .insert_header((
                "Content-Type",
                format!("multipart/form-data; boundary={boundary}"),
            ))
            .set_payload(payload)
            .to_request();
        let resp = test::call_service(&app, req).await;
        let status = resp.status();
        if status != 201 {
            let body = test::read_body(resp).await;
            panic!(
                "upload {name} returned {status}: {}",
                String::from_utf8_lossy(&body)
            );
        }
        let body: Value = test::read_body_json(resp).await;
        if name == "report.pdf" {
            report_file_id = body["data"]["id"].as_i64();
        }
    }

    let report_file_id = report_file_id.expect("report upload should return file id");
    let delete_req = test::TestRequest::delete()
        .uri(&format!("/api/v1/files/{report_file_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let delete_resp = test::call_service(&app, delete_req).await;
    assert_eq!(delete_resp.status(), 200);

    let payload = upload_named_file(
        "report.pdf",
        "pdf content again",
        "application/pdf",
        boundary,
    );
    let recreate_req = test::TestRequest::post()
        .uri("/api/v1/files/upload")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .insert_header((
            "Content-Type",
            format!("multipart/form-data; boundary={boundary}"),
        ))
        .set_payload(payload)
        .to_request();
    let recreate_resp = test::call_service(&app, recreate_req).await;
    let recreate_status = recreate_resp.status();
    if recreate_status != 201 {
        let body = test::read_body(recreate_resp).await;
        panic!(
            "recreate report.pdf returned {recreate_status}: {}",
            String::from_utf8_lossy(&body)
        );
    }

    let mut documents_folder_id = None;
    for folder_name in ["Documents", "Photos"] {
        let req = test::TestRequest::post()
            .uri("/api/v1/folders")
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .set_json(serde_json::json!({ "name": folder_name, "parent_id": null }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201);
        let body: Value = test::read_body_json(resp).await;
        if folder_name == "Documents" {
            documents_folder_id = body["data"]["id"].as_i64();
        }
    }

    let documents_folder_id = documents_folder_id.expect("Documents folder id should exist");
    let delete_folder_req = test::TestRequest::delete()
        .uri(&format!("/api/v1/folders/{documents_folder_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let delete_folder_resp = test::call_service(&app, delete_folder_req).await;
    assert_eq!(delete_folder_resp.status(), 200);

    let recreate_folder_req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Documents", "parent_id": null }))
        .to_request();
    let recreate_folder_resp = test::call_service(&app, recreate_folder_req).await;
    let recreate_folder_status = recreate_folder_resp.status();
    if recreate_folder_status != 201 {
        let body = test::read_body(recreate_folder_resp).await;
        panic!(
            "recreate Documents folder returned {recreate_folder_status}: {}",
            String::from_utf8_lossy(&body)
        );
    }

    let search_req = test::TestRequest::get()
        .uri("/api/v1/search?q=rep")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let search_resp = test::call_service(&app, search_req).await;
    let search_status = search_resp.status();
    if search_status != 200 {
        let body = test::read_body(search_resp).await;
        panic!(
            "search returned {search_status}: {}",
            String::from_utf8_lossy(&body)
        );
    }
    let search_body: Value = test::read_body_json(search_resp).await;
    assert_eq!(search_body["data"]["total_files"], 1);
    assert_eq!(search_body["data"]["files"][0]["name"], "report.pdf");

    let short_search_req = test::TestRequest::get()
        .uri("/api/v1/search?q=r")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let short_search_resp = test::call_service(&app, short_search_req).await;
    let short_search_status = short_search_resp.status();
    if short_search_status != 200 {
        let body = test::read_body(short_search_resp).await;
        panic!(
            "short search returned {short_search_status}: {}",
            String::from_utf8_lossy(&body)
        );
    }
    let short_search_body: Value = test::read_body_json(short_search_resp).await;
    let short_search_files = short_search_body["data"]["files"]
        .as_array()
        .expect("short search files should be an array");
    assert!(
        short_search_body["data"]["total_files"]
            .as_u64()
            .expect("short search total should be numeric")
            >= 1
    );
    assert!(
        short_search_files
            .iter()
            .any(|file| file["name"] == "report.pdf"),
        "short search should include report.pdf: {short_search_body}"
    );

    let admin_user_search_req = test::TestRequest::get()
        .uri("/api/v1/admin/users?keyword=end-u")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let admin_user_search_resp = test::call_service(&app, admin_user_search_req).await;
    let admin_user_search_status = admin_user_search_resp.status();
    if admin_user_search_status != 200 {
        let body = test::read_body(admin_user_search_resp).await;
        panic!(
            "admin user search returned {admin_user_search_status}: {}",
            String::from_utf8_lossy(&body)
        );
    }
    let admin_user_search_body: Value = test::read_body_json(admin_user_search_resp).await;
    assert_eq!(admin_user_search_body["data"]["total"], 1);
    assert_eq!(
        admin_user_search_body["data"]["items"][0]["username"],
        "backend-user"
    );

    let folder_search_req = test::TestRequest::get()
        .uri("/api/v1/search?type=folder&q=doc")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let folder_search_resp = test::call_service(&app, folder_search_req).await;
    let folder_search_status = folder_search_resp.status();
    if folder_search_status != 200 {
        let body = test::read_body(folder_search_resp).await;
        panic!(
            "folder search returned {folder_search_status}: {}",
            String::from_utf8_lossy(&body)
        );
    }
    let folder_search_body: Value = test::read_body_json(folder_search_resp).await;
    assert_eq!(folder_search_body["data"]["total_folders"], 1);
    assert_eq!(
        folder_search_body["data"]["folders"][0]["name"],
        "Documents"
    );

    let overview_req = test::TestRequest::get()
        .uri("/api/v1/admin/overview?days=3&timezone=UTC&event_limit=1")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let overview_resp = test::call_service(&app, overview_req).await;
    let overview_status = overview_resp.status();
    if overview_status != 200 {
        let body = test::read_body(overview_resp).await;
        panic!(
            "admin overview returned {overview_status}: {}",
            String::from_utf8_lossy(&body)
        );
    }
    let overview_body: Value = test::read_body_json(overview_resp).await;
    assert_eq!(overview_body["data"]["days"], 3);
    assert_eq!(overview_body["data"]["stats"]["total_users"], 2);
    assert_eq!(overview_body["data"]["stats"]["total_files"], 3);
    assert_eq!(overview_body["data"]["stats"]["uploads_today"], 4);
}

#[actix_web::test]
async fn test_postgres_smoke_search_and_admin_overview() {
    let database_url = common::postgres_test_database_url().await;

    exercise_backend_smoke(&database_url, DbBackend::Postgres).await;
}

#[actix_web::test]
async fn test_mysql_smoke_search_and_admin_overview() {
    let database_url = common::mysql_test_database_url().await;

    exercise_backend_smoke(&database_url, DbBackend::MySql).await;
}
