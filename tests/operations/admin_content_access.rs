//! Administrator inspection exercises real authentication, persistence and transport boundaries.
use crate::common;
use actix_web::{http::StatusCode, test};
use aster_drive::db::repository::{config_repo, revision_repo, user_repo};
use aster_drive_model::{
    entities::{admin_content_access, audit_log, file},
    types::UserStatus,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, IntoActiveModel, PaginatorTrait,
    QueryFilter, Set,
};
use serde_json::{Value, json};

async fn enable(state: &aster_drive::runtime::PrimaryAppState) {
    config_repo::upsert(
        state.writer_db(),
        "admin_workspace_content_access_enabled",
        "true",
        0,
    )
    .await
    .unwrap();
}
fn get(token: &str, uri: &str) -> test::TestRequest {
    test::TestRequest::get()
        .uri(uri)
        .insert_header(("Cookie", common::access_cookie_header(token)))
}
fn post(token: &str, uri: &str, body: Value) -> test::TestRequest {
    test::TestRequest::post()
        .uri(uri)
        .insert_header(("Cookie", common::access_cookie_header(token)))
        .insert_header(common::csrf_header_for(token))
        .set_json(body)
}
macro_rules! grant {
    ($app:expr, $token:expr, $id:expr) => {{
        let response = test::call_service(&$app, post(&$token, "/api/v1/admin/content-access", json!({"scope_kind":"personal","target_id":$id,"reason":"user_support","remark":"private case note"})).to_request()).await;
        assert_eq!(response.status(), 200);
        let body: Value = test::read_body_json(response).await;
        body["data"]["id"].as_str().unwrap().to_owned()
    }};
}

#[actix_web::test]
async fn access_defaults_off_requires_reason_and_rejects_invalid_input() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let response = test::call_service(&app, get(&token, "/api/v1/auth/me").to_request()).await;
    let me: Value = test::read_body_json(response).await;
    let uid = me["data"]["id"].as_i64().unwrap();
    let body = json!({"scope_kind":"personal","target_id":uid,"reason":"user_support"});
    let response = test::call_service(
        &app,
        post(&token, "/api/v1/admin/content-access", body.clone()).to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let result: Value = test::read_body_json(response).await;
    assert_eq!(result["code"], "admin.content_access_disabled");
    enable(&state).await;
    for (request, code) in [
        (
            json!({"scope_kind":"personal","target_id":uid}),
            "admin.content_reason_required",
        ),
        (
            json!({"scope_kind":"personal","target_id":uid,"reason":"user_support","remark":"x".repeat(501)}),
            "bad_request",
        ),
        (
            json!({"scope_kind":"personal","target_id":uid,"reason":"other","remark":"private\nnewline"}),
            "bad_request",
        ),
    ] {
        let response = test::call_service(
            &app,
            post(&token, "/api/v1/admin/content-access", request).to_request(),
        )
        .await;
        assert!(response.status().is_client_error());
        let result: Value = test::read_body_json(response).await;
        assert_eq!(result["code"], code);
    }
    assert_eq!(
        admin_content_access::Entity::find()
            .count(state.writer_db())
            .await
            .unwrap(),
        0
    );
    let access = grant!(app, token, uid);
    assert!(!access.contains("private"));
    let response = test::call_service(
        &app,
        get(
            &token,
            &format!("/api/v1/admin/content-access/workspace?access_id={access}"),
        )
        .to_request(),
    )
    .await;
    assert_eq!(response.status(), 200);
    let records = audit_log::Entity::find()
        .filter(audit_log::Column::Action.eq("admin_workspace_inspect"))
        .all(state.writer_db())
        .await
        .unwrap();
    assert_eq!(records.len(), 1);
    assert!(
        records[0]
            .details
            .as_ref()
            .unwrap()
            .contains("private case note")
    );
}

#[actix_web::test]
async fn access_cannot_cross_sessions_targets_or_revocation_and_audit_is_mandatory() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let response = test::call_service(&app, get(&token, "/api/v1/auth/me").to_request()).await;
    let me: Value = test::read_body_json(response).await;
    let uid = me["data"]["id"].as_i64().unwrap();
    enable(&state).await;
    // Optional system logging may be off; administrator access evidence remains compulsory.
    config_repo::upsert(state.writer_db(), "audit_log_enabled", "false", 0)
        .await
        .unwrap();
    state
        .runtime_config()
        .reload(state.writer_db())
        .await
        .unwrap();
    let access = grant!(app, token, uid);
    let (other_session, _) = login_user!(app, "testuser", "password123");
    let response = test::call_service(
        &app,
        get(
            &other_session,
            &format!("/api/v1/admin/content-access/workspace?access_id={access}"),
        )
        .to_request(),
    )
    .await;
    assert_eq!(response.status(), 403);
    let mut target = user_repo::find_by_id(state.writer_db(), uid)
        .await
        .unwrap()
        .into_active_model();
    target.status = Set(UserStatus::Disabled);
    target.update(state.writer_db()).await.unwrap();
    // Actor and target are the same here; both account and scope checks must deny.
    let response = test::try_call_service(
        &app,
        get(
            &token,
            &format!("/api/v1/admin/content-access/workspace?access_id={access}"),
        )
        .to_request(),
    )
    .await;
    assert!(response.is_err() || response.unwrap().status().is_client_error());
    assert_eq!(
        audit_log::Entity::find()
            .filter(audit_log::Column::Action.eq("admin_workspace_inspect"))
            .count(state.writer_db())
            .await
            .unwrap(),
        1
    );
}

#[actix_web::test]
async fn failed_audit_rolls_back_access_grant_and_returns_no_content() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let response = test::call_service(&app, get(&token, "/api/v1/auth/me").to_request()).await;
    let me: Value = test::read_body_json(response).await;
    let uid = me["data"]["id"].as_i64().unwrap();
    enable(&state).await;
    // A database failure at the required audit write must roll back the matching authorization.
    state
        .writer_db()
        .execute_unprepared("DROP TABLE audit_logs")
        .await
        .unwrap();
    let response = test::call_service(
        &app,
        post(
            &token,
            "/api/v1/admin/content-access",
            json!({"scope_kind":"personal","target_id":uid,"reason":"data_recovery"}),
        )
        .to_request(),
    )
    .await;
    assert_eq!(response.status(), 403);
    let body: Value = test::read_body_json(response).await;
    assert_eq!(body["code"], "admin.content_audit_unavailable");
    assert_eq!(
        admin_content_access::Entity::find()
            .count(state.writer_db())
            .await
            .unwrap(),
        0
    );
}

macro_rules! init_download {
    ($app:expr, $token:expr, $access:expr, $file:expr, $revision:expr) => {{
        let response = test::call_service(&$app, post(&$token, &format!("/api/v1/admin/files/{}/download-init", $file), json!({"access_id":$access,"revision_id":$revision})).to_request()).await;
        assert_eq!(response.status(), 200);
        let body: Value = test::read_body_json(response).await;
        body["data"].clone()
    }};
}
fn stream_uri(info: &Value) -> String {
    format!(
        "/api/v1{}",
        info["resource"]["request"]["url"].as_str().unwrap()
    )
}

#[actix_web::test]
async fn download_current_revision_range_etag_and_init_snapshot_are_consistent() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let uid = user_repo::find_by_username(state.writer_db(), "testuser")
        .await
        .unwrap()
        .unwrap()
        .id;
    enable(&state).await;
    let file_id = upload_test_file_named!(app, token, "恢复 report.txt");
    let access = grant!(app, token, uid);
    let init = init_download!(app, token, access, file_id, None::<i64>);
    let uri = stream_uri(&init);
    let revision_id = init["revision_id"].as_i64().unwrap();
    let etag = init["resource"]["identity"]["etag"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(init["resource"]["request"]["credentials"], "include");
    assert_eq!(
        init["resource"]["request"]["conditional_headers"],
        "allowed"
    );
    // A later overwrite must not change the content authorized by init.
    aster_drive::services::files::file::update_content(
        &state,
        file_id,
        uid,
        actix_web::web::Bytes::from_static(b"new content"),
        None,
    )
    .await
    .unwrap();
    let response = test::call_service(&app, get(&token, &uri).to_request()).await;
    assert_eq!(response.status(), 200);
    assert_eq!(
        response.headers().get("ETag").unwrap().to_str().unwrap(),
        etag
    );
    assert_eq!(response.headers().get("Content-Length").unwrap(), "12");
    assert!(
        response
            .headers()
            .get("Content-Disposition")
            .unwrap()
            .to_str()
            .unwrap()
            .contains("%E6%81%A2%E5%A4%8D")
    );
    assert_eq!(test::read_body(response).await.as_ref(), b"test content");
    for (range, bytes, bounds) in [
        ("bytes=0-3", &b"test"[..], "bytes 0-3/12"),
        ("bytes=-3", &b"ent"[..], "bytes 9-11/12"),
        ("bytes=5-", &b"content"[..], "bytes 5-11/12"),
    ] {
        let response = test::call_service(
            &app,
            get(&token, &uri)
                .insert_header(("Range", range))
                .to_request(),
        )
        .await;
        assert_eq!(response.status(), 206);
        assert_eq!(response.headers().get("Content-Range").unwrap(), bounds);
        assert_eq!(test::read_body(response).await.as_ref(), bytes);
    }
    let response = test::call_service(
        &app,
        get(&token, &uri)
            .insert_header(("If-None-Match", etag.clone()))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), 304);
    assert!(test::read_body(response).await.is_empty());
    let response = test::call_service(
        &app,
        get(&token, &uri)
            .insert_header(("Range", "bytes=99-100"))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), 400);
    let historical = init_download!(app, token, access, file_id, Some(revision_id));
    assert_eq!(historical["revision_id"], revision_id);
    let current = init_download!(app, token, access, file_id, None::<i64>);
    assert_ne!(current["revision_id"], revision_id);
    let response = test::call_service(&app, get(&token, &stream_uri(&current)).to_request()).await;
    assert_eq!(test::read_body(response).await.as_ref(), b"new content");
    let audits = audit_log::Entity::find()
        .filter(audit_log::Column::Action.is_in(["admin_revision_download", "admin_file_download"]))
        .all(state.writer_db())
        .await
        .unwrap();
    assert!(
        audits
            .iter()
            .any(|row| row.details.as_ref().unwrap().contains("\"has_range\":true"))
    );
    assert!(audits.iter().any(|row| {
        row.details
            .as_ref()
            .unwrap()
            .contains("\"result\":\"failed\"")
    }));
    // Ordinary user download auditing keeps its independent action.
    let response = test::call_service(
        &app,
        get(&token, &format!("/api/v1/files/{file_id}/download")).to_request(),
    )
    .await;
    assert_eq!(response.status(), 200);
    let _ = test::read_body(response).await;
    aster_drive::services::ops::audit::flush(&state)
        .await
        .unwrap();
    assert!(
        audit_log::Entity::find()
            .filter(audit_log::Column::Action.eq("file_download"))
            .count(state.writer_db())
            .await
            .unwrap()
            > 0
    );
}

#[actix_web::test]
async fn deleted_files_empty_content_foreign_revisions_and_missing_blob_are_bounded() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let uid = user_repo::find_by_username(state.writer_db(), "testuser")
        .await
        .unwrap()
        .unwrap()
        .id;
    enable(&state).await;
    let file_id = upload_test_file!(app, token);
    let other_file = upload_test_file_named!(app, token, "other.txt");
    let other_revision = revision_repo::find_current_by_file_id(state.writer_db(), other_file)
        .await
        .unwrap();
    let access = grant!(app, token, uid);
    let response = test::call_service(
        &app,
        post(
            &token,
            &format!("/api/v1/admin/files/{file_id}/download-init"),
            json!({"access_id":access,"revision_id":other_revision.id}),
        )
        .to_request(),
    )
    .await;
    assert_eq!(response.status(), 404);
    let mut deleted = file::Entity::find_by_id(file_id)
        .one(state.writer_db())
        .await
        .unwrap()
        .unwrap()
        .into_active_model();
    deleted.deleted_at = Set(Some(chrono::Utc::now()));
    deleted.update(state.writer_db()).await.unwrap();
    let init = init_download!(app, token, access, file_id, None::<i64>);
    let response = test::call_service(&app, get(&token, &stream_uri(&init)).to_request()).await;
    assert_eq!(response.status(), 200);
    assert_eq!(test::read_body(response).await.as_ref(), b"test content");
    let empty_id = common::create_empty_file_via_api(&app, &token, "empty.txt", None).await;
    let empty_init = init_download!(app, token, access, empty_id, None::<i64>);
    let response =
        test::call_service(&app, get(&token, &stream_uri(&empty_init)).to_request()).await;
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers().get("Content-Length").unwrap(), "0");
    assert!(test::read_body(response).await.is_empty());
    let response = test::call_service(
        &app,
        get(&token, &stream_uri(&empty_init))
            .insert_header(("Range", "bytes=0-0"))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), 400);
    // Purged revision content is never recovered through a blob identifier.
    let current = revision_repo::find_current_by_file_id(state.writer_db(), other_file)
        .await
        .unwrap();
    let mut purged = current.into_active_model();
    purged.purged_at = Set(Some(chrono::Utc::now()));
    purged.blob_id = Set(None);
    purged.update(state.writer_db()).await.unwrap();
    let response = test::call_service(
        &app,
        post(
            &token,
            &format!("/api/v1/admin/files/{other_file}/download-init"),
            json!({"access_id":access}),
        )
        .to_request(),
    )
    .await;
    assert!(response.status().is_client_error() || response.status().is_server_error());
}

#[actix_web::test]
async fn browsing_filters_scope_paginates_and_validates_ancestry_and_target_state() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let uid = user_repo::find_by_username(state.writer_db(), "testuser")
        .await
        .unwrap()
        .unwrap()
        .id;
    let other_id = admin_create_user!(app, token, "other", "other@example.com", "password123");
    let (other_token, _) = login_user!(app, "other", "password123");
    enable(&state).await;
    let first = upload_test_file_named!(app, other_token, "a.txt");
    let second = upload_test_file_named!(app, other_token, "b.txt");
    upload_test_file_named!(app, token, "admin-only.txt");
    let folder_resp = test::call_service(
        &app,
        post(&other_token, "/api/v1/folders", json!({"name":"nested"})).to_request(),
    )
    .await;
    let folder_body: Value = test::read_body_json(folder_resp).await;
    let folder_id = folder_body["data"]["id"].as_i64().unwrap();
    let access = grant!(app, token, other_id);
    let root = format!("/api/v1/admin/content-access/workspace?access_id={access}");
    let response = test::call_service(
        &app,
        get(&token, &format!("{root}&file_limit=1&folder_limit=1")).to_request(),
    )
    .await;
    assert_eq!(response.status(), 200);
    let page: Value = test::read_body_json(response).await;
    assert_eq!(page["data"]["contents"]["files_total"], 2);
    assert_eq!(page["data"]["contents"]["files"][0]["id"], first);
    let cursor = &page["data"]["contents"]["next_file_cursor"];
    let response = test::call_service(
        &app,
        get(
            &token,
            &format!(
                "{root}&file_limit=1&file_after_value={}&file_after_id={}",
                cursor["value"].as_str().unwrap(),
                cursor["id"]
            ),
        )
        .to_request(),
    )
    .await;
    let page: Value = test::read_body_json(response).await;
    assert_eq!(page["data"]["contents"]["files"][0]["id"], second);
    let response = test::call_service(
        &app,
        get(&token, &format!("{root}&folder_id={folder_id}")).to_request(),
    )
    .await;
    let page: Value = test::read_body_json(response).await;
    assert_eq!(page["data"]["ancestors"][0]["id"], folder_id);
    assert_eq!(page["data"]["contents"]["files_total"], 0);
    let own_access = grant!(app, token, uid);
    let response = test::call_service(&app, get(&token, &format!("/api/v1/admin/content-access/workspace?access_id={own_access}&folder_id={folder_id}")).to_request()).await;
    assert_eq!(response.status(), 403);
    let mut target = user_repo::find_by_id(state.writer_db(), other_id)
        .await
        .unwrap()
        .into_active_model();
    target.status = Set(UserStatus::Disabled);
    target.update(state.writer_db()).await.unwrap();
    let response = test::call_service(&app, get(&token, &root).to_request()).await;
    assert_eq!(response.status(), 403);
    let result: Value = test::read_body_json(response).await;
    assert_eq!(result["code"], "admin.content_target_unavailable");
}

#[actix_web::test]
async fn team_access_needs_no_membership_but_cannot_cross_personal_scope_and_is_audited_to_team() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let uid = user_repo::find_by_username(state.writer_db(), "testuser")
        .await
        .unwrap()
        .unwrap()
        .id;
    admin_create_user!(
        app,
        token,
        "teamowner",
        "teamowner@example.com",
        "password123"
    );
    let (owner_token, _) = login_user!(app, "teamowner", "password123");
    let response = test::call_service(
        &app,
        post(
            &token,
            "/api/v1/admin/teams",
            json!({"name":"Support team", "admin_identifier":"teamowner"}),
        )
        .to_request(),
    )
    .await;
    assert_eq!(response.status(), 201);
    let team: Value = test::read_body_json(response).await;
    let team_id = team["data"]["id"].as_i64().unwrap();
    let (status, uploaded) = common::upload_via_server_session(
        &app,
        &owner_token,
        &format!("/api/v1/teams/{team_id}/files"),
        "team.txt",
        "text/plain",
        b"team content",
    )
    .await;
    assert_eq!(status, 201);
    let file_id = uploaded["data"]["id"].as_i64().unwrap();
    let response = test::try_call_service(
        &app,
        get(
            &token,
            &format!("/api/v1/teams/{team_id}/files/{file_id}/download"),
        )
        .to_request(),
    )
    .await;
    match response {
        Ok(response) => assert_eq!(response.status(), 403),
        Err(error) => assert_eq!(error.as_response_error().status_code(), 403),
    }
    enable(&state).await;
    let response = test::call_service(
        &app,
        post(
            &token,
            "/api/v1/admin/content-access",
            json!({"scope_kind":"team","target_id":team_id,"reason":"security_investigation"}),
        )
        .to_request(),
    )
    .await;
    assert_eq!(response.status(), 200);
    let grant: Value = test::read_body_json(response).await;
    let access = grant["data"]["id"].as_str().unwrap();
    let response = test::call_service(
        &app,
        get(
            &token,
            &format!("/api/v1/admin/content-access/workspace?access_id={access}"),
        )
        .to_request(),
    )
    .await;
    assert_eq!(response.status(), 200);
    let contents: Value = test::read_body_json(response).await;
    assert_eq!(contents["data"]["contents"]["files"][0]["id"], file_id);
    let init = init_download!(app, token, access, file_id, None::<i64>);
    let response = test::call_service(
        &app,
        get(&token, &stream_uri(&init))
            .insert_header(("Range", "bytes=0-3"))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), 206);
    assert_eq!(test::read_body(response).await.as_ref(), b"team");
    let personal = grant!(app, token, uid);
    let response = test::call_service(
        &app,
        post(
            &token,
            &format!("/api/v1/admin/files/{file_id}/download-init"),
            json!({"access_id":personal}),
        )
        .to_request(),
    )
    .await;
    assert_eq!(response.status(), 403);
    let response = test::call_service(
        &app,
        get(&token, &format!("/api/v1/admin/teams/{team_id}/audit-logs")).to_request(),
    )
    .await;
    assert_eq!(response.status(), 200);
    let logs: Value = test::read_body_json(response).await;
    assert!(logs.to_string().contains("admin_file_download"));
    let response = test::call_service(
        &app,
        get(
            &token,
            &format!("/api/v1/admin/teams/{team_id}/audit-logs/export"),
        )
        .to_request(),
    )
    .await;
    assert_eq!(response.status(), 200);
    assert!(
        String::from_utf8_lossy(&test::read_body(response).await).contains("admin_file_download")
    );
    let mut team = aster_drive::db::repository::team_repo::find_by_id(state.writer_db(), team_id)
        .await
        .unwrap()
        .into_active_model();
    team.archived_at = Set(Some(chrono::Utc::now()));
    team.update(state.writer_db()).await.unwrap();
    let response = test::call_service(&app, get(&token, &stream_uri(&init)).to_request()).await;
    assert_eq!(response.status(), 403);
}

#[actix_web::test]
async fn expiration_disabled_switch_and_revoked_session_invalidate_existing_contexts() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let uid = user_repo::find_by_username(state.writer_db(), "testuser")
        .await
        .unwrap()
        .unwrap()
        .id;
    enable(&state).await;
    let access = grant!(app, token, uid);
    let uri = format!("/api/v1/admin/content-access/workspace?access_id={access}");
    config_repo::upsert(
        state.writer_db(),
        "admin_workspace_content_access_enabled",
        "false",
        uid,
    )
    .await
    .unwrap();
    let response = test::call_service(&app, get(&token, &uri).to_request()).await;
    assert_eq!(response.status(), 403);
    enable(&state).await;
    let grant = admin_content_access::Entity::find_by_id(access.clone())
        .one(state.writer_db())
        .await
        .unwrap()
        .unwrap();
    let sid = grant.session_id.clone();
    let mut expired = grant.into_active_model();
    expired.expires_at = Set(chrono::Utc::now() - chrono::Duration::seconds(1));
    expired.update(state.writer_db()).await.unwrap();
    let response = test::call_service(&app, get(&token, &uri).to_request()).await;
    assert_eq!(response.status(), 403);
    assert_eq!(
        aster_drive::db::repository::admin_content_access_repo::delete_expired(state.writer_db())
            .await
            .unwrap(),
        1
    );
    let access = grant!(app, token, uid);
    aster_drive::db::repository::auth_session_repo::revoke_by_id_for_user(
        state.writer_db(),
        uid,
        &sid,
        chrono::Utc::now(),
    )
    .await
    .unwrap();
    let response = test::try_call_service(
        &app,
        get(
            &token,
            &format!("/api/v1/admin/content-access/workspace?access_id={access}"),
        )
        .to_request(),
    )
    .await;
    match response {
        Ok(response) => assert!(response.status().is_client_error()),
        Err(error) => assert!(error.as_response_error().status_code().is_client_error()),
    }
}

#[actix_web::test]
async fn download_audit_failure_withholds_init_urls_and_stream_body() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let uid = user_repo::find_by_username(state.writer_db(), "testuser")
        .await
        .unwrap()
        .unwrap()
        .id;
    enable(&state).await;
    let file_id = upload_test_file!(app, token);
    let access = grant!(app, token, uid);
    let init = init_download!(app, token, access, file_id, None::<i64>);
    let grant_count = admin_content_access::Entity::find()
        .count(state.writer_db())
        .await
        .unwrap();
    state
        .writer_db()
        .execute_unprepared("DROP TABLE audit_logs")
        .await
        .unwrap();
    let response = test::call_service(
        &app,
        post(
            &token,
            &format!("/api/v1/admin/files/{file_id}/download-init"),
            json!({"access_id":access}),
        )
        .to_request(),
    )
    .await;
    assert_eq!(response.status(), 403);
    let body: Value = test::read_body_json(response).await;
    assert!(body.get("data").is_none_or(Value::is_null));
    assert_eq!(
        admin_content_access::Entity::find()
            .count(state.writer_db())
            .await
            .unwrap(),
        grant_count
    );
    let response = test::call_service(&app, get(&token, &stream_uri(&init)).to_request()).await;
    assert_eq!(response.status(), 403);
    let body = test::read_body(response).await;
    assert!(
        !body
            .windows(b"test content".len())
            .any(|window| window == b"test content")
    );
}

#[actix_web::test]
async fn invalid_navigation_cycles_deleted_views_and_missing_contexts_do_not_leak_content() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let uid = user_repo::find_by_username(state.writer_db(), "testuser")
        .await
        .unwrap()
        .unwrap()
        .id;
    enable(&state).await;
    let file_id = upload_test_file!(app, token);
    let access = grant!(app, token, uid);
    let root = format!("/api/v1/admin/content-access/workspace?access_id={access}");
    for query in [
        "&file_after_id=1",
        "&file_after_value=a",
        "&folder_id=-1",
        "&file_after_id=-1&file_after_value=a",
        "&folder_offset=-1",
    ] {
        let response =
            test::call_service(&app, get(&token, &format!("{root}{query}")).to_request()).await;
        assert_eq!(response.status(), 400, "{query}");
    }
    let mut file = file::Entity::find_by_id(file_id)
        .one(state.writer_db())
        .await
        .unwrap()
        .unwrap()
        .into_active_model();
    file.deleted_at = Set(Some(chrono::Utc::now()));
    file.update(state.writer_db()).await.unwrap();
    for (deleted, expected) in [(false, 0), (true, 1)] {
        let response = test::call_service(
            &app,
            get(&token, &format!("{root}&deleted={deleted}")).to_request(),
        )
        .await;
        assert_eq!(response.status(), 200);
        let body: Value = test::read_body_json(response).await;
        assert_eq!(body["data"]["contents"]["files_total"], expected);
    }
    let response = test::call_service(
        &app,
        post(&token, "/api/v1/folders", json!({"name":"cycle"})).to_request(),
    )
    .await;
    let folder: Value = test::read_body_json(response).await;
    let folder_id = folder["data"]["id"].as_i64().unwrap();
    let mut folder = aster_drive_model::entities::folder::Entity::find_by_id(folder_id)
        .one(state.writer_db())
        .await
        .unwrap()
        .unwrap()
        .into_active_model();
    folder.parent_id = Set(Some(folder_id));
    folder.update(state.writer_db()).await.unwrap();
    let response = test::call_service(
        &app,
        get(&token, &format!("{root}&folder_id={folder_id}")).to_request(),
    )
    .await;
    assert_eq!(response.status(), 500);
    for uri in [
        "/api/v1/admin/content-access/workspace?access_id=",
        "/api/v1/admin/content-access/workspace?access_id=unknown",
        "/api/v1/admin/content-access/unknown/download",
    ] {
        let response = test::call_service(&app, get(&token, uri).to_request()).await;
        assert_eq!(response.status(), 403);
    }
    let response = test::call_service(
        &app,
        get(&token, "/api/v1/admin/file-blobs/1/download").to_request(),
    )
    .await;
    assert_eq!(response.status(), 404);
}

#[actix_web::test]
async fn every_content_endpoint_requires_administrator_authentication_and_post_requires_csrf() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (admin, _) = register_and_login!(app);
    admin_create_user!(app, admin, "reader", "reader@example.com", "password123");
    let (reader, _) = login_user!(app, "reader", "password123");
    for (method, uri) in [
        (
            actix_web::http::Method::GET,
            "/api/v1/admin/content-access/status",
        ),
        (
            actix_web::http::Method::POST,
            "/api/v1/admin/content-access",
        ),
        (
            actix_web::http::Method::GET,
            "/api/v1/admin/content-access/workspace?access_id=unknown",
        ),
        (
            actix_web::http::Method::POST,
            "/api/v1/admin/files/1/download-init",
        ),
        (
            actix_web::http::Method::GET,
            "/api/v1/admin/content-access/unknown/download",
        ),
    ] {
        for (token, expected) in [(None, 401), (Some(&reader), 403)] {
            let mut request = test::TestRequest::default()
                .method(method.clone())
                .uri(uri)
                .set_json(json!({}));
            if let Some(token) = token {
                request = request
                    .insert_header(("Cookie", common::access_cookie_header(token)))
                    .insert_header(common::csrf_header_for(token));
            }
            let response = test::try_call_service(&app, request.to_request()).await;
            let status = match response {
                Ok(response) => response.status(),
                Err(error) => error.as_response_error().status_code(),
            };
            assert_eq!(status.as_u16(), expected, "{method} {uri}");
        }
    }
    let response = test::try_call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/admin/content-access")
            .insert_header(("Cookie", common::access_cookie_header(&admin)))
            .set_json(json!({}))
            .to_request(),
    )
    .await;
    let status = match response {
        Ok(response) => response.status(),
        Err(error) => error.as_response_error().status_code(),
    };
    assert_eq!(status, 403);
}

#[actix_web::test]
async fn missing_storage_object_fails_without_returning_a_content_body_and_is_audited() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);
    let uid = user_repo::find_by_username(state.writer_db(), "testuser")
        .await
        .unwrap()
        .unwrap()
        .id;
    enable(&state).await;
    let file_id = upload_test_file!(app, token);
    let access = grant!(app, token, uid);
    let record = aster_drive::db::repository::file_repo::find_by_id(state.writer_db(), file_id)
        .await
        .unwrap();
    let blob =
        aster_drive::db::repository::file_repo::find_blob_by_id(state.writer_db(), record.blob_id)
            .await
            .unwrap();
    let policy =
        aster_drive::db::repository::policy_repo::find_by_id(state.writer_db(), blob.policy_id)
            .await
            .unwrap();
    state
        .driver_registry
        .get_driver(&policy)
        .unwrap()
        .delete(blob.storage_path_for_connector().unwrap())
        .await
        .unwrap();
    let init = init_download!(app, token, access, file_id, None::<i64>);
    let response = test::call_service(&app, get(&token, &stream_uri(&init)).to_request()).await;
    assert_eq!(response.status(), 500);
    let error: Value = test::read_body_json(response).await;
    assert_eq!(error["code"], "storage.not_found");
    assert!(error.get("data").is_none_or(Value::is_null));
    let audits = audit_log::Entity::find()
        .filter(audit_log::Column::Action.eq("admin_file_download"))
        .all(state.writer_db())
        .await
        .unwrap();
    assert!(audits.iter().any(|row| {
        row.details
            .as_deref()
            .unwrap_or_default()
            .contains("\"result\":\"failed\"")
    }));
}
