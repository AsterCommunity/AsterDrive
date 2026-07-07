//! Team workspace file-space tests

#[macro_use]
mod common;
use aster_drive::runtime::SharedRuntimeState;

use actix_web::test;
use serde_json::Value;

const OVER_LIMIT_BODY_SIZE: usize = 10 * 1024 * 1024 + 1;

fn assert_upload_error_contract(body: &Value, expected_code: &str) {
    assert_eq!(body["code"], expected_code);
    assert_eq!(body["error"]["retryable"], false);
    assert!(body["error"].get("internal_code").is_none());
    assert!(body["error"].get("subcode").is_none());
}

macro_rules! register_user {
    ($app:expr, $db:expr, $mail_sender:expr, $username:expr, $email:expr, $password:expr) => {{
        let req = test::TestRequest::post()
            .uri("/api/v1/auth/register")
            .peer_addr("127.0.0.1:12345".parse().unwrap())
            .set_json(serde_json::json!({
                "username": $username,
                "email": $email,
                "password": $password
            }))
            .to_request();
        let resp = test::call_service(&$app, req).await;
        assert_eq!(resp.status(), 201);
        let body: Value = test::read_body_json(resp).await;
        let user_id = body["data"]["id"].as_i64().unwrap();
        let _ = confirm_latest_contact_verification!($app, $db, $mail_sender);
        user_id
    }};
}

macro_rules! login_user {
    ($app:expr, $identifier:expr, $password:expr) => {{
        let req = test::TestRequest::post()
            .uri("/api/v1/auth/login")
            .peer_addr("127.0.0.1:12345".parse().unwrap())
            .set_json(serde_json::json!({
                "identifier": $identifier,
                "password": $password
            }))
            .to_request();
        let resp = test::call_service(&$app, req).await;
        assert_eq!(resp.status(), 200);
        common::extract_cookie(&resp, "aster_access").unwrap()
    }};
}

macro_rules! multipart_request {
    ($uri:expr, $token:expr, $filename:expr, $content:expr $(,)?) => {{
        let boundary = "----TeamBoundary123";
        let payload = format!(
            "------TeamBoundary123\r\n\
             Content-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\n\
             Content-Type: text/plain\r\n\r\n\
             {content}\r\n\
             ------TeamBoundary123--\r\n",
            filename = $filename,
            content = $content,
        );

        test::TestRequest::post()
            .uri($uri)
            .insert_header(("Cookie", common::access_cookie_header(&$token)))
            .insert_header(common::csrf_header_for(&$token))
            .insert_header((
                "Content-Type",
                format!("multipart/form-data; boundary={boundary}"),
            ))
            .set_payload(payload)
            .to_request()
    }};
}

macro_rules! multipart_request_with_mime {
    ($uri:expr, $token:expr, $filename:expr, $content:expr, $mime:expr $(,)?) => {{
        let boundary = "----TeamMimeBoundary123";
        let payload = format!(
            "------TeamMimeBoundary123\r\n\
             Content-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\n\
             Content-Type: {mime}\r\n\r\n\
             {content}\r\n\
             ------TeamMimeBoundary123--\r\n",
            filename = $filename,
            mime = $mime,
            content = $content,
        );

        test::TestRequest::post()
            .uri($uri)
            .insert_header(("Cookie", common::access_cookie_header(&$token)))
            .insert_header(common::csrf_header_for(&$token))
            .insert_header((
                "Content-Type",
                format!("multipart/form-data; boundary={boundary}"),
            ))
            .set_payload(payload)
            .to_request()
    }};
}

fn build_binary_multipart_payload(filename: &str, data: &[u8]) -> (String, Vec<u8>) {
    let boundary = format!("----AsterTeamBoundary{}", uuid::Uuid::new_v4().simple());
    let mut payload = Vec::new();
    payload.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    payload.extend_from_slice(
        format!("Content-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\n")
            .as_bytes(),
    );
    payload.extend_from_slice(b"Content-Type: application/octet-stream\r\n\r\n");
    payload.extend_from_slice(data);
    payload.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    (boundary, payload)
}

async fn set_default_policy_chunk_size(
    state: &aster_drive::runtime::PrimaryAppState,
    chunk_size: i64,
) {
    use sea_orm::{ActiveModelTrait, Set};

    let policy = aster_drive::db::repository::policy_repo::find_default(state.writer_db())
        .await
        .unwrap()
        .expect("default policy should exist");
    let mut active: aster_drive::entities::storage_policy::ActiveModel = policy.into();
    active.chunk_size = Set(chunk_size);
    active.update(state.writer_db()).await.unwrap();
    state
        .policy_snapshot
        .reload(state.writer_db())
        .await
        .unwrap();
}

#[actix_web::test]
async fn test_team_file_resource_handle_resolves_team_scoped_paths_and_rejects_personal_file() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let mail_sender = state.mail_sender.clone();
    let app = create_test_app!(state);

    let _owner_id = register_user!(
        app,
        db,
        mail_sender,
        "teamresown",
        "teamresown@example.com",
        "password123"
    );
    let owner_token = login_user!(app, "teamresown", "password123");

    let req = test::TestRequest::post()
        .uri("/api/v1/teams")
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({ "name": "Team Resource Handles" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let team_id = body["data"]["id"].as_i64().unwrap();

    let req = multipart_request_with_mime!(
        &format!("/api/v1/teams/{team_id}/files/upload"),
        &owner_token,
        "team-resource.png",
        "png-ish",
        "image/png",
    );
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let team_file_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/v1/teams/{team_id}/files/{team_file_id}/resource-handle"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({
            "purpose": "preview",
            "delivery_mode": "blob_url",
            "representation": "thumbnail"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "success");
    assert_eq!(
        body["data"]["identity"]["cache_key"],
        format!("/teams/{team_id}/files/{team_file_id}/thumbnail")
    );
    assert_eq!(body["data"]["identity"]["scope"], "team");
    assert_eq!(
        body["data"]["request"]["url"],
        format!("/teams/{team_id}/files/{team_file_id}/thumbnail")
    );
    assert_eq!(body["data"]["request"]["credentials"], "include");
    assert_eq!(body["data"]["request"]["conditional_headers"], "allowed");
    assert_eq!(
        body["data"]["request"]["redirect_policy"],
        "same_origin_only"
    );
    assert_eq!(body["data"]["delivery"]["mode"], "blob_url");
    assert_eq!(body["data"]["delivery"]["mime_type"], "image/webp");

    let req = multipart_request!(
        "/api/v1/files/upload",
        &owner_token,
        "personal-resource.txt",
        "personal"
    );
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let personal_file_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/v1/teams/{team_id}/files/{personal_file_id}/resource-handle"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({
            "purpose": "preview",
            "delivery_mode": "blob_url"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;

    assert_eq!(resp.status(), 403);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "workspace.scope_denied");
}

#[actix_web::test]
async fn test_team_space_upload_browse_download_and_personal_separation() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let mail_sender = state.mail_sender.clone();
    let app = create_test_app!(state);

    let _owner_id = register_user!(
        app,
        db,
        mail_sender,
        "spaceown",
        "spaceown@example.com",
        "password123"
    );
    let member_id = register_user!(
        app,
        db,
        mail_sender,
        "spacemem",
        "spacemem@example.com",
        "password123"
    );
    let owner_token = login_user!(app, "spaceown", "password123");
    let member_token = login_user!(app, "spacemem", "password123");

    let req = test::TestRequest::post()
        .uri("/api/v1/teams")
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({ "name": "Docs Team" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let team_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/members"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({ "user_id": member_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/folders"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({ "name": "Docs" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let docs_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/files/new")
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({
            "name": "should-not-work.txt",
            "folder_id": docs_id
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);

    let req = multipart_request!(
        &format!("/api/v1/teams/{team_id}/files/upload?folder_id={docs_id}"),
        &owner_token,
        "team.txt",
        "hello team",
    );
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let file_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/folders/{docs_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&member_token)))
        .insert_header(common::csrf_header_for(&member_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["files"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"]["files"][0]["name"], "team.txt");
    assert!(body["data"]["files"][0]["blob_id"].is_null());
    assert!(body["data"]["files"][0]["created_at"].is_null());
    assert!(body["data"]["folders"].is_array());

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/folders/{docs_id}/info"))
        .insert_header(("Cookie", common::access_cookie_header(&member_token)))
        .insert_header(common::csrf_header_for(&member_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["id"], docs_id);
    assert_eq!(body["data"]["name"], "Docs");
    assert!(body["data"]["created_at"].is_string());
    assert_eq!(body["data"]["team_id"], team_id);
    assert_eq!(body["data"]["storage_used"], "hello team".len() as i64);

    let req = test::TestRequest::get()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert!(body["data"]["folders"].as_array().unwrap().is_empty());
    assert!(body["data"]["files"].as_array().unwrap().is_empty());

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/files/{file_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/files/{file_id}/versions"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);

    let req = test::TestRequest::put()
        .uri(&format!("/api/v1/properties/file/{file_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({
            "namespace": "custom",
            "name": "note",
            "value": "x"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);

    let req = test::TestRequest::post()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({
            "target": { "type": "file", "id": file_id }
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/files/{file_id}/download"))
        .insert_header(("Cookie", common::access_cookie_header(&member_token)))
        .insert_header(common::csrf_header_for(&member_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let status = resp.status();
    let body = test::read_body(resp).await;
    assert_eq!(status, 200, "{}", String::from_utf8_lossy(&body));
    assert_eq!(&body[..], b"hello team");

    let req = test::TestRequest::post()
        .uri("/api/v1/batch/delete")
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({ "file_ids": [file_id] }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["succeeded"], 0);
    assert_eq!(body["data"]["failed"], 1);
}

#[actix_web::test]
async fn test_team_space_delete_folder_and_non_member_forbidden() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let mail_sender = state.mail_sender.clone();
    let app = create_test_app!(state);

    let _owner_id = register_user!(
        app,
        db,
        mail_sender,
        "spaceown2",
        "spaceown2@example.com",
        "password123"
    );
    let outsider_id = register_user!(
        app,
        db,
        mail_sender,
        "outsider2",
        "outsider2@example.com",
        "password123"
    );
    let owner_token = login_user!(app, "spaceown2", "password123");
    let outsider_token = login_user!(app, "outsider2", "password123");

    let req = test::TestRequest::post()
        .uri("/api/v1/teams")
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({ "name": "Ops Team" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let team_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/folders"))
        .insert_header(("Cookie", common::access_cookie_header(&outsider_token)))
        .insert_header(common::csrf_header_for(&outsider_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/folders"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({ "name": "Nested" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let nested_id = body["data"]["id"].as_i64().unwrap();

    let req = multipart_request!(
        &format!("/api/v1/teams/{team_id}/files/upload?folder_id={nested_id}"),
        &owner_token,
        "ops.txt",
        "ops notes",
    );
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let file_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/teams/{team_id}/folders/{nested_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/folders"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    assert!(body["data"]["folders"].as_array().unwrap().is_empty());

    let req = test::TestRequest::get()
        .uri("/api/v1/trash")
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert!(body["data"]["folders"].as_array().unwrap().is_empty());
    assert!(body["data"]["files"].as_array().unwrap().is_empty());

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/files/{file_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/members"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({ "user_id": outsider_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/files/new"))
        .insert_header(("Cookie", common::access_cookie_header(&outsider_token)))
        .insert_header(common::csrf_header_for(&outsider_token))
        .set_json(serde_json::json!({ "name": "empty.md" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
}

#[actix_web::test]
async fn test_team_space_patch_file_and_folder() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let mail_sender = state.mail_sender.clone();
    let app = create_test_app!(state);

    let _owner_id = register_user!(
        app,
        db,
        mail_sender,
        "spaceown3",
        "spaceown3@example.com",
        "password123"
    );
    let member_id = register_user!(
        app,
        db,
        mail_sender,
        "spacemem3",
        "spacemem3@example.com",
        "password123"
    );
    let owner_token = login_user!(app, "spaceown3", "password123");
    let member_token = login_user!(app, "spacemem3", "password123");

    let req = test::TestRequest::post()
        .uri("/api/v1/teams")
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({ "name": "Collab Team" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let team_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/members"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({ "user_id": member_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/folders"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({ "name": "Docs" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let docs_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/folders"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({ "name": "Archive" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let archive_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/folders"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({
            "name": "Drafts",
            "parent_id": docs_id
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let drafts_id = body["data"]["id"].as_i64().unwrap();

    let req = multipart_request!(
        &format!("/api/v1/teams/{team_id}/files/upload?folder_id={docs_id}"),
        &owner_token,
        "draft.txt",
        "draft body",
    );
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let file_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/teams/{team_id}/folders/{docs_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&member_token)))
        .insert_header(common::csrf_header_for(&member_token))
        .set_json(serde_json::json!({ "parent_id": drafts_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/teams/{team_id}/files/{file_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&member_token)))
        .insert_header(common::csrf_header_for(&member_token))
        .set_json(serde_json::json!({
            "name": "final.txt",
            "folder_id": archive_id
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["name"], "final.txt");
    assert_eq!(body["data"]["folder_id"], archive_id);

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/teams/{team_id}/folders/{drafts_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&member_token)))
        .insert_header(common::csrf_header_for(&member_token))
        .set_json(serde_json::json!({
            "name": "Shared",
            "parent_id": null
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["name"], "Shared");
    assert!(body["data"]["parent_id"].is_null());

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/folders/{archive_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["files"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"]["files"][0]["name"], "final.txt");

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/folders"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let folder_names: Vec<_> = body["data"]["folders"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["name"].as_str().unwrap())
        .collect();
    assert!(folder_names.contains(&"Docs"));
    assert!(folder_names.contains(&"Archive"));
    assert!(folder_names.contains(&"Shared"));

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/teams/{team_id}/folders/{drafts_id}/ancestors"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"][0]["name"], "Shared");

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/files/{file_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({ "name": "should-not-work.txt" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/folders/{drafts_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({ "name": "should-not-work" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);
}

#[actix_web::test]
async fn test_team_file_direct_link_supports_public_access_and_team_deactivation() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let mail_sender = state.mail_sender.clone();
    let app = create_test_app!(state);

    let _owner_id = register_user!(
        app,
        db,
        mail_sender,
        "directteam",
        "directteam@example.com",
        "password123"
    );
    let owner_token = login_user!(app, "directteam", "password123");

    let req = test::TestRequest::post()
        .uri("/api/v1/teams")
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({ "name": "Direct Team" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let team_id = body["data"]["id"].as_i64().unwrap();

    let req = multipart_request!(
        &format!("/api/v1/teams/{team_id}/files/upload"),
        &owner_token,
        "stream.m3u8",
        "team direct body",
    );
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let file_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/teams/{team_id}/files/{file_id}/direct-link"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let direct_token = body["data"]["token"]
        .as_str()
        .expect("direct link token should exist")
        .to_string();

    let req = test::TestRequest::get()
        .uri(&format!("/d/{direct_token}/stream.m3u8"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get("Content-Disposition").unwrap(),
        "inline; filename*=UTF-8''stream.m3u8"
    );

    let req = test::TestRequest::get()
        .uri(&format!("/d/{direct_token}/stream.m3u8?download=1"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get("Content-Disposition").unwrap(),
        "attachment; filename*=UTF-8''stream.m3u8"
    );

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/teams/{team_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri(&format!("/d/{direct_token}/stream.m3u8"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);
}

#[actix_web::test]
async fn test_team_space_copy_file_and_folder() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let mail_sender = state.mail_sender.clone();
    let app = create_test_app!(state);

    let _owner_id = register_user!(
        app,
        db,
        mail_sender,
        "spaceown5",
        "spaceown5@example.com",
        "password123"
    );
    let member_id = register_user!(
        app,
        db,
        mail_sender,
        "spacemem5",
        "spacemem5@example.com",
        "password123"
    );
    let owner_token = login_user!(app, "spaceown5", "password123");
    let member_token = login_user!(app, "spacemem5", "password123");

    let req = test::TestRequest::post()
        .uri("/api/v1/teams")
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({ "name": "Copy Team" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let team_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/members"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({ "user_id": member_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/folders"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({ "name": "Source" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let source_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/folders"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({
            "name": "Nested",
            "parent_id": source_id
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let nested_id = body["data"]["id"].as_i64().unwrap();

    let req = multipart_request!(
        &format!("/api/v1/teams/{team_id}/files/upload?folder_id={source_id}"),
        &owner_token,
        "plan.txt",
        "rootcopy",
    );
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let file_id = body["data"]["id"].as_i64().unwrap();

    let req = multipart_request!(
        &format!("/api/v1/teams/{team_id}/files/upload?folder_id={nested_id}"),
        &owner_token,
        "nested.txt",
        "nestedcopy",
    );
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/folders/{source_id}/copy"))
        .insert_header(("Cookie", common::access_cookie_header(&member_token)))
        .insert_header(common::csrf_header_for(&member_token))
        .set_json(serde_json::json!({ "parent_id": nested_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/files/{file_id}/copy"))
        .insert_header(("Cookie", common::access_cookie_header(&member_token)))
        .insert_header(common::csrf_header_for(&member_token))
        .set_json(serde_json::json!({ "folder_id": null }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["name"], "plan.txt");
    assert!(body["data"]["folder_id"].is_null());

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/files/{file_id}/copy"))
        .insert_header(("Cookie", common::access_cookie_header(&member_token)))
        .insert_header(common::csrf_header_for(&member_token))
        .set_json(serde_json::json!({ "folder_id": null }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["name"], "plan (1).txt");
    assert!(body["data"]["folder_id"].is_null());

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/files/{file_id}/copy"))
        .insert_header(("Cookie", common::access_cookie_header(&member_token)))
        .insert_header(common::csrf_header_for(&member_token))
        .set_json(serde_json::json!({ "folder_id": null }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/folders/{source_id}/copy"))
        .insert_header(("Cookie", common::access_cookie_header(&member_token)))
        .insert_header(common::csrf_header_for(&member_token))
        .set_json(serde_json::json!({ "parent_id": null }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["name"], "Source (1)");
    assert!(body["data"]["parent_id"].is_null());
    let copy_folder_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/folders/{source_id}/copy"))
        .insert_header(("Cookie", common::access_cookie_header(&member_token)))
        .insert_header(common::csrf_header_for(&member_token))
        .set_json(serde_json::json!({ "parent_id": null }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/folders/{copy_folder_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&member_token)))
        .insert_header(common::csrf_header_for(&member_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["files"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"]["files"][0]["name"], "plan.txt");
    assert_eq!(body["data"]["folders"].as_array().unwrap().len(), 1);
    let nested_copy_id = body["data"]["folders"][0]["id"].as_i64().unwrap();

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/folders/{nested_copy_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&member_token)))
        .insert_header(common::csrf_header_for(&member_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["files"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"]["files"][0]["name"], "nested.txt");

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&member_token)))
        .insert_header(common::csrf_header_for(&member_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["storage_used"], 52);
}

#[actix_web::test]
async fn test_team_space_content_versions_and_locks() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let mail_sender = state.mail_sender.clone();
    let app = create_test_app!(state);

    let _owner_id = register_user!(
        app,
        db,
        mail_sender,
        "spaceown6",
        "spaceown6@example.com",
        "password123"
    );
    let member_id = register_user!(
        app,
        db,
        mail_sender,
        "spacemem6",
        "spacemem6@example.com",
        "password123"
    );
    let owner_token = login_user!(app, "spaceown6", "password123");
    let member_token = login_user!(app, "spacemem6", "password123");

    let req = test::TestRequest::post()
        .uri("/api/v1/teams")
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({ "name": "Editor Team" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let team_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/members"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({ "user_id": member_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/folders"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({ "name": "Docs" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let docs_id = body["data"]["id"].as_i64().unwrap();

    let req = multipart_request!(
        &format!("/api/v1/teams/{team_id}/files/upload?folder_id={docs_id}"),
        &member_token,
        "editor.txt",
        "v1",
    );
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let file_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::put()
        .uri(&format!("/api/v1/files/{file_id}/content"))
        .insert_header(("Cookie", common::access_cookie_header(&member_token)))
        .insert_header(common::csrf_header_for(&member_token))
        .insert_header(("Content-Type", "application/octet-stream"))
        .set_payload("should-not-work")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/files/{file_id}/lock"))
        .insert_header(("Cookie", common::access_cookie_header(&member_token)))
        .insert_header(common::csrf_header_for(&member_token))
        .set_json(serde_json::json!({ "locked": true }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/files/{file_id}/download"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let etag = resp
        .headers()
        .get("ETag")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();

    let req = test::TestRequest::put()
        .uri(&format!("/api/v1/teams/{team_id}/files/{file_id}/content"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .insert_header(("Content-Type", "application/octet-stream"))
        .insert_header(("If-Match", "\"wrong-etag\""))
        .set_payload("bad")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 412);

    let req = test::TestRequest::put()
        .uri(&format!("/api/v1/teams/{team_id}/files/{file_id}/content"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .insert_header(("Content-Type", "application/octet-stream"))
        .insert_header(("If-Match", etag.as_str()))
        .set_payload("v1-owner")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/files/{file_id}/lock"))
        .insert_header(("Cookie", common::access_cookie_header(&member_token)))
        .insert_header(common::csrf_header_for(&member_token))
        .set_json(serde_json::json!({ "locked": true }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::put()
        .uri(&format!("/api/v1/teams/{team_id}/files/{file_id}/content"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .insert_header(("Content-Type", "application/octet-stream"))
        .set_payload("blocked-by-lock")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 423);

    let req = test::TestRequest::put()
        .uri(&format!("/api/v1/teams/{team_id}/files/{file_id}/content"))
        .insert_header(("Cookie", common::access_cookie_header(&member_token)))
        .insert_header(common::csrf_header_for(&member_token))
        .insert_header(("Content-Type", "application/octet-stream"))
        .set_payload("v2-member")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    assert!(resp.headers().contains_key("ETag"));

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/files/{file_id}/versions"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"].as_array().unwrap().len(), 2);
    let restore_version_id = body["data"][1]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/files/{file_id}/lock"))
        .insert_header(("Cookie", common::access_cookie_header(&member_token)))
        .insert_header(common::csrf_header_for(&member_token))
        .set_json(serde_json::json!({ "locked": false }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/v1/teams/{team_id}/files/{file_id}/versions/{restore_version_id}/restore"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/files/{file_id}/download"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body = test::read_body(resp).await;
    assert_eq!(&body[..], b"v1");

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/files/{file_id}/versions"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert!(body["data"].as_array().unwrap().is_empty());

    let req = test::TestRequest::put()
        .uri(&format!("/api/v1/teams/{team_id}/files/{file_id}/content"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .insert_header(("Content-Type", "application/octet-stream"))
        .set_payload("v3-owner")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/files/{file_id}/versions"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"].as_array().unwrap().len(), 1);
    let delete_version_id = body["data"][0]["id"].as_i64().unwrap();

    let req = test::TestRequest::delete()
        .uri(&format!(
            "/api/v1/teams/{team_id}/files/{file_id}/versions/{delete_version_id}"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/files/{file_id}/versions"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert!(body["data"].as_array().unwrap().is_empty());

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/folders/{docs_id}/lock"))
        .insert_header(("Cookie", common::access_cookie_header(&member_token)))
        .insert_header(common::csrf_header_for(&member_token))
        .set_json(serde_json::json!({ "locked": true }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/teams/{team_id}/folders/{docs_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({ "name": "Blocked Rename" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 423);

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/folders/{docs_id}/lock"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({ "locked": true }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/folders/{docs_id}/lock"))
        .insert_header(("Cookie", common::access_cookie_header(&member_token)))
        .insert_header(common::csrf_header_for(&member_token))
        .set_json(serde_json::json!({ "locked": false }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/teams/{team_id}/folders/{docs_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({ "name": "Renamed Docs" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["name"], "Renamed Docs");
}

#[actix_web::test]
async fn test_team_update_content_allows_body_larger_than_global_payload_limit() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let mail_sender = state.mail_sender.clone();
    let app = create_test_app!(state);

    let _owner_id = register_user!(
        app,
        db,
        mail_sender,
        "spaceownlarge",
        "spaceownlarge@example.com",
        "password123"
    );
    let owner_token = login_user!(app, "spaceownlarge", "password123");

    let req = test::TestRequest::post()
        .uri("/api/v1/teams")
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({ "name": "Large Update Team" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let team_id = body["data"]["id"].as_i64().unwrap();

    let req = multipart_request!(
        &format!("/api/v1/teams/{team_id}/files/upload"),
        &owner_token,
        "large-team.txt",
        "seed",
    );
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let file_id = body["data"]["id"].as_i64().unwrap();

    let payload = vec![b't'; OVER_LIMIT_BODY_SIZE];
    let req = test::TestRequest::put()
        .uri(&format!("/api/v1/teams/{team_id}/files/{file_id}/content"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .insert_header(("Content-Type", "application/octet-stream"))
        .set_payload(payload.clone())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["size"], payload.len() as i64);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/files/{file_id}/download"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body = test::read_body(resp).await;
    assert_eq!(&body[..], payload.as_slice());
}

#[actix_web::test]
async fn test_team_versions_enforce_scope_and_membership() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let mail_sender = state.mail_sender.clone();
    let app = create_test_app!(state);

    let _owner_id = register_user!(
        app,
        db,
        mail_sender,
        "teamversionown2",
        "teamversionown2@example.com",
        "password123"
    );
    let _outsider_id = register_user!(
        app,
        db,
        mail_sender,
        "teamversionout2",
        "teamversionout2@example.com",
        "password123"
    );
    let owner_token = login_user!(app, "teamversionown2", "password123");
    let outsider_token = login_user!(app, "teamversionout2", "password123");

    let req = test::TestRequest::post()
        .uri("/api/v1/teams")
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({ "name": "Version Guard Team" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let team_id = body["data"]["id"].as_i64().unwrap();

    let req = multipart_request!(
        &format!("/api/v1/teams/{team_id}/files/upload"),
        &owner_token,
        "team-version-guard.txt",
        "team-v1",
    );
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let file_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::put()
        .uri(&format!("/api/v1/teams/{team_id}/files/{file_id}/content"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .insert_header(("Content-Type", "application/octet-stream"))
        .set_payload("team-v2")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/files/{file_id}/versions"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"].as_array().unwrap().len(), 1);
    let version_id = body["data"][0]["id"].as_i64().unwrap();

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/files/{file_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["size"], "team-v2".len() as i64);
    assert_eq!(
        body["data"]["storage_used"],
        ("team-v1".len() + "team-v2".len()) as i64
    );

    for req in [
        test::TestRequest::get()
            .uri(&format!("/api/v1/files/{file_id}"))
            .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
            .insert_header(common::csrf_header_for(&owner_token))
            .to_request(),
        test::TestRequest::get()
            .uri(&format!("/api/v1/files/{file_id}/versions"))
            .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
            .insert_header(common::csrf_header_for(&owner_token))
            .to_request(),
        test::TestRequest::post()
            .uri(&format!(
                "/api/v1/files/{file_id}/versions/{version_id}/restore"
            ))
            .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
            .insert_header(common::csrf_header_for(&owner_token))
            .to_request(),
        test::TestRequest::delete()
            .uri(&format!("/api/v1/files/{file_id}/versions/{version_id}"))
            .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
            .insert_header(common::csrf_header_for(&owner_token))
            .to_request(),
    ] {
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 403);
    }

    for req in [
        test::TestRequest::get()
            .uri(&format!("/api/v1/teams/{team_id}/files/{file_id}"))
            .insert_header(("Cookie", common::access_cookie_header(&outsider_token)))
            .insert_header(common::csrf_header_for(&outsider_token))
            .to_request(),
        test::TestRequest::get()
            .uri(&format!("/api/v1/teams/{team_id}/files/{file_id}/versions"))
            .insert_header(("Cookie", common::access_cookie_header(&outsider_token)))
            .insert_header(common::csrf_header_for(&outsider_token))
            .to_request(),
        test::TestRequest::post()
            .uri(&format!(
                "/api/v1/teams/{team_id}/files/{file_id}/versions/{version_id}/restore"
            ))
            .insert_header(("Cookie", common::access_cookie_header(&outsider_token)))
            .insert_header(common::csrf_header_for(&outsider_token))
            .to_request(),
        test::TestRequest::delete()
            .uri(&format!(
                "/api/v1/teams/{team_id}/files/{file_id}/versions/{version_id}"
            ))
            .insert_header(("Cookie", common::access_cookie_header(&outsider_token)))
            .insert_header(common::csrf_header_for(&outsider_token))
            .to_request(),
    ] {
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 403);
    }

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/files/{file_id}/versions"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"][0]["id"], version_id);
}

#[actix_web::test]
async fn test_team_shares_support_public_folder_access_and_team_management() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let mail_sender = state.mail_sender.clone();
    let app = create_test_app!(state);

    let _owner_id = register_user!(
        app,
        db,
        mail_sender,
        "spaceown7",
        "spaceown7@example.com",
        "password123"
    );
    let member_id = register_user!(
        app,
        db,
        mail_sender,
        "spacemem7",
        "spacemem7@example.com",
        "password123"
    );
    let owner_token = login_user!(app, "spaceown7", "password123");
    let member_token = login_user!(app, "spacemem7", "password123");

    let req = test::TestRequest::post()
        .uri("/api/v1/teams")
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({ "name": "Share Team" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let team_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/members"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({ "user_id": member_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/folders"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({ "name": "Docs" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let docs_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/folders"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({
            "name": "Nested",
            "parent_id": docs_id
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let nested_id = body["data"]["id"].as_i64().unwrap();

    let req = multipart_request!(
        &format!("/api/v1/teams/{team_id}/files/upload?folder_id={docs_id}"),
        &owner_token,
        "team-share.txt",
        "team-share-body",
    );
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = multipart_request!(
        &format!("/api/v1/teams/{team_id}/files/upload?folder_id={nested_id}"),
        &owner_token,
        "nested-share.txt",
        "nested-share-body",
    );
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/shares"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({
            "target": { "type": "folder", "id": docs_id },
            "password": "secret123",
            "max_downloads": 2
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let share_id = body["data"]["id"].as_i64().unwrap();
    let share_token = body["data"]["token"].as_str().unwrap().to_string();
    assert_eq!(body["data"]["team_id"], team_id);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/folders"))
        .insert_header(("Cookie", common::access_cookie_header(&member_token)))
        .insert_header(common::csrf_header_for(&member_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["folders"][0]["name"], "Docs");
    assert_eq!(body["data"]["folders"][0]["is_shared"], true);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/shares?limit=20&offset=0"))
        .insert_header(("Cookie", common::access_cookie_header(&member_token)))
        .insert_header(common::csrf_header_for(&member_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["total"], 1);
    assert_eq!(body["data"]["items"][0]["resource_name"], "Docs");
    assert_eq!(body["data"]["items"][0]["resource_type"], "folder");
    assert_eq!(body["data"]["items"][0]["status"], "active");

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/s/{share_token}"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["name"], "Docs");
    assert_eq!(body["data"]["share_type"], "folder");
    assert_eq!(body["data"]["has_password"], true);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/s/{share_token}/content"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/s/{share_token}/verify"))
        .set_json(serde_json::json!({ "password": "secret123" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let share_cookie = common::extract_cookie(&resp, &format!("aster_share_{share_token}"))
        .expect("share verification cookie should exist");

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/s/{share_token}/content"))
        .insert_header((
            "Cookie",
            format!("aster_share_{share_token}={share_cookie}"),
        ))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["folders"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"]["folders"][0]["name"], "Nested");
    assert_eq!(body["data"]["files"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"]["files"][0]["name"], "team-share.txt");

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/s/{share_token}/folders/{nested_id}/content"
        ))
        .insert_header((
            "Cookie",
            format!("aster_share_{share_token}={share_cookie}"),
        ))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["files"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"]["files"][0]["name"], "nested-share.txt");

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/teams/{team_id}/shares/{share_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&member_token)))
        .insert_header(common::csrf_header_for(&member_token))
        .set_json(serde_json::json!({
            "password": "",
            "expires_at": null,
            "max_downloads": 0
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/teams/{team_id}/shares/{share_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&member_token)))
        .insert_header(common::csrf_header_for(&member_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/shares?limit=20&offset=0"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert!(body["data"]["items"].as_array().unwrap().is_empty());

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/folders"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["folders"][0]["is_shared"], false);
}

#[actix_web::test]
async fn test_team_trash_restore_file_to_root_and_purge_deleted_folder_tree() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let mail_sender = state.mail_sender.clone();
    let app = create_test_app!(state);

    let _owner_id = register_user!(
        app,
        db,
        mail_sender,
        "spaceown8",
        "spaceown8@example.com",
        "password123"
    );
    let owner_token = login_user!(app, "spaceown8", "password123");

    let req = test::TestRequest::post()
        .uri("/api/v1/teams")
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({ "name": "Trash Team" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let team_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/folders"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({ "name": "Parent" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let parent_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/folders"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({
            "name": "Child",
            "parent_id": parent_id
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let child_id = body["data"]["id"].as_i64().unwrap();

    let req = multipart_request!(
        &format!("/api/v1/teams/{team_id}/files/upload?folder_id={parent_id}"),
        &owner_token,
        "restore-me.txt",
        "restore-me-body",
    );
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let restored_file_id = body["data"]["id"].as_i64().unwrap();

    let req = multipart_request!(
        &format!("/api/v1/teams/{team_id}/files/upload?folder_id={child_id}"),
        &owner_token,
        "purge-me.txt",
        "purge-me-body",
    );
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let purged_file_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/teams/{team_id}/folders/{parent_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/trash"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["folders"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"]["folders"][0]["name"], "Parent");
    assert!(body["data"]["files"].as_array().unwrap().is_empty());

    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/v1/teams/{team_id}/trash/file/{restored_file_id}/restore"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/folders"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["files"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"]["files"][0]["id"], restored_file_id);
    assert_eq!(body["data"]["files"][0]["name"], "restore-me.txt");

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/teams/{team_id}/trash/folder/{parent_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/trash"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert!(body["data"]["folders"].as_array().unwrap().is_empty());
    assert!(body["data"]["files"].as_array().unwrap().is_empty());

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/files/{purged_file_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/teams/{team_id}/files/{restored_file_id}/download"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body = test::read_body(resp).await;
    assert_eq!(&body[..], b"restore-me-body");
}

#[actix_web::test]
async fn test_team_trash_purge_all_schedules_background_task() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let mail_sender = state.mail_sender.clone();
    let app = create_test_app!(state.clone());

    let _owner_id = register_user!(
        app,
        db,
        mail_sender,
        "ttpurgeall",
        "ttpurgeall@example.com",
        "password123"
    );
    let owner_token = login_user!(app, "ttpurgeall", "password123");

    let req = test::TestRequest::post()
        .uri("/api/v1/teams")
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({ "name": "Team Trash Purge All" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let team_id = body["data"]["id"].as_i64().unwrap();

    let req = multipart_request!(
        &format!("/api/v1/teams/{team_id}/files/upload"),
        &owner_token,
        "team-trash-purge-all.txt",
        "team trash purge all body",
    );
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let file_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/teams/{team_id}/files/{file_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/teams/{team_id}/trash"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let task_id = body["data"]["id"].as_i64().unwrap();
    assert_eq!(body["data"]["kind"], "trash_purge_all");
    assert_eq!(body["data"]["status"], "pending");
    assert_eq!(body["data"]["team_id"], team_id);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/trash"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["files"].as_array().unwrap().len(), 1);

    let stats = aster_drive::services::task::drain(&state)
        .await
        .expect("team trash purge task drain should succeed");
    assert_eq!(stats.succeeded, 1);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/tasks/{task_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["status"], "succeeded");
    assert_eq!(body["data"]["result"]["kind"], "trash_purge_all");
    assert_eq!(body["data"]["result"]["purged"], 1);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/trash"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert!(body["data"]["files"].as_array().unwrap().is_empty());
    assert!(body["data"]["folders"].as_array().unwrap().is_empty());
}

#[actix_web::test]
async fn test_team_trash_purge_all_rejects_non_member_without_creating_task() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let mail_sender = state.mail_sender.clone();
    let app = create_test_app!(state);

    let _owner_id = register_user!(
        app,
        db.clone(),
        mail_sender.clone(),
        "ttpurgeown",
        "ttpurgeown@example.com",
        "password123"
    );
    let owner_token = login_user!(app, "ttpurgeown", "password123");
    let _outsider_id = register_user!(
        app,
        db,
        mail_sender,
        "ttpurgeout",
        "ttpurgeout@example.com",
        "password123"
    );
    let outsider_token = login_user!(app, "ttpurgeout", "password123");

    let req = test::TestRequest::post()
        .uri("/api/v1/teams")
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({ "name": "Team Trash Purge Guard" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let team_id = body["data"]["id"].as_i64().unwrap();

    let req = multipart_request!(
        &format!("/api/v1/teams/{team_id}/files/upload"),
        &owner_token,
        "guard-trash.txt",
        "guard trash body",
    );
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let file_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/teams/{team_id}/files/{file_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/teams/{team_id}/trash"))
        .insert_header(("Cookie", common::access_cookie_header(&outsider_token)))
        .insert_header(common::csrf_header_for(&outsider_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/tasks"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["total"], 0);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/trash"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["files"].as_array().unwrap().len(), 1);
}

#[actix_web::test]
async fn test_team_trash_rejects_active_and_out_of_scope_items() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let mail_sender = state.mail_sender.clone();
    let app = create_test_app!(state);

    let _owner_id = register_user!(
        app,
        db,
        mail_sender,
        "spaceowntrash2",
        "spaceowntrash2@example.com",
        "password123"
    );
    let owner_token = login_user!(app, "spaceowntrash2", "password123");

    let req = test::TestRequest::post()
        .uri("/api/v1/teams")
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({ "name": "Trash Guard Team" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let team_id = body["data"]["id"].as_i64().unwrap();

    let req = multipart_request!(
        &format!("/api/v1/teams/{team_id}/files/upload"),
        &owner_token,
        "active-team.txt",
        "active team body",
    );
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let active_team_file_id = body["data"]["id"].as_i64().unwrap();

    let req = multipart_request!(
        &format!("/api/v1/teams/{team_id}/files/upload"),
        &owner_token,
        "trashed-team.txt",
        "trashed team body",
    );
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let trashed_team_file_id = body["data"]["id"].as_i64().unwrap();

    let personal_file_id = upload_test_file_named!(app, owner_token, "personal-trash.txt");

    let req = test::TestRequest::delete()
        .uri(&format!(
            "/api/v1/teams/{team_id}/files/{trashed_team_file_id}"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/v1/teams/{team_id}/trash/file/{active_team_file_id}/restore"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);

    let req = test::TestRequest::delete()
        .uri(&format!(
            "/api/v1/teams/{team_id}/trash/file/{personal_file_id}"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/trash"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["files"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"]["files"][0]["id"], trashed_team_file_id);

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/teams/{team_id}/files/{active_team_file_id}"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/files/{personal_file_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn test_team_trash_pagination_preserves_totals_and_membership() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let mail_sender = state.mail_sender.clone();
    let app = create_test_app!(state);

    let _owner_id = register_user!(
        app,
        db,
        mail_sender,
        "teamtrashpageown",
        "teamtrashpageown@example.com",
        "password123"
    );
    let _outsider_id = register_user!(
        app,
        db,
        mail_sender,
        "teamtrashpageout",
        "teamtrashpageout@example.com",
        "password123"
    );
    let owner_token = login_user!(app, "teamtrashpageown", "password123");
    let outsider_token = login_user!(app, "teamtrashpageout", "password123");

    let req = test::TestRequest::post()
        .uri("/api/v1/teams")
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({ "name": "Trash Pagination Team" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let team_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/trash"))
        .insert_header(("Cookie", common::access_cookie_header(&outsider_token)))
        .insert_header(common::csrf_header_for(&outsider_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);

    for i in 0..3 {
        let req = test::TestRequest::post()
            .uri(&format!("/api/v1/teams/{team_id}/folders"))
            .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
            .insert_header(common::csrf_header_for(&owner_token))
            .set_json(serde_json::json!({ "name": format!("trash-folder-{i}") }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201);
        let body: Value = test::read_body_json(resp).await;
        let folder_id = body["data"]["id"].as_i64().unwrap();

        let req = test::TestRequest::delete()
            .uri(&format!("/api/v1/teams/{team_id}/folders/{folder_id}"))
            .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
            .insert_header(common::csrf_header_for(&owner_token))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
    }

    for i in 0..4 {
        let req = multipart_request!(
            &format!("/api/v1/teams/{team_id}/files/upload"),
            &owner_token,
            &format!("trash-file-{i}.txt"),
            "team trash body",
        );
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201);
        let body: Value = test::read_body_json(resp).await;
        let file_id = body["data"]["id"].as_i64().unwrap();

        let req = test::TestRequest::delete()
            .uri(&format!("/api/v1/teams/{team_id}/files/{file_id}"))
            .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
            .insert_header(common::csrf_header_for(&owner_token))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
    }

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/trash"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["folders_total"], 3);
    assert_eq!(body["data"]["files_total"], 4);

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/teams/{team_id}/trash?folder_limit=2&file_limit=3"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["folders"].as_array().unwrap().len(), 2);
    assert_eq!(body["data"]["files"].as_array().unwrap().len(), 3);
    assert_eq!(body["data"]["folders_total"], 3);
    assert_eq!(body["data"]["files_total"], 4);
    let next_file_cursor = &body["data"]["next_file_cursor"];
    assert!(
        next_file_cursor.is_object(),
        "should have next_file_cursor after first page"
    );
    let after_expires_at = next_file_cursor["expires_at"].as_str().unwrap();
    let after_id = next_file_cursor["id"].as_i64().unwrap();

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/teams/{team_id}/trash?folder_limit=0&file_limit=3&file_after_expires_at={after_expires_at}&file_after_id={after_id}"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert!(body["data"]["folders"].as_array().unwrap().is_empty());
    assert_eq!(body["data"]["files"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"]["folders_total"], 3);
    assert_eq!(body["data"]["files_total"], 4);
    assert!(body["data"]["next_file_cursor"].is_null());

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/teams/{team_id}/trash?folder_limit=0&file_limit=0"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert!(body["data"]["folders"].as_array().unwrap().is_empty());
    assert!(body["data"]["files"].as_array().unwrap().is_empty());
    assert_eq!(body["data"]["folders_total"], 3);
    assert_eq!(body["data"]["files_total"], 4);
}

#[actix_web::test]
async fn test_team_space_chunked_upload_flow_and_personal_route_rejection() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    set_default_policy_chunk_size(&state, 4).await;
    let mail_sender = state.mail_sender.clone();
    let app = create_test_app!(state);

    let _owner_id = register_user!(
        app,
        db,
        mail_sender,
        "spaceown4",
        "spaceown4@example.com",
        "password123"
    );
    let owner_token = login_user!(app, "spaceown4", "password123");

    let req = test::TestRequest::post()
        .uri("/api/v1/teams")
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({ "name": "Chunk Team" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let team_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/files/upload/init"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({
            "filename": "chunked-team.txt",
            "total_size": 10
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["mode"], "chunked");
    assert_eq!(body["data"]["chunk_size"], 4);
    assert_eq!(body["data"]["total_chunks"], 3);
    let upload_id = body["data"]["upload_id"].as_str().unwrap().to_string();

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/files/upload/{upload_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);

    for (chunk_number, bytes) in [
        (0, b"ABCD".as_slice()),
        (1, b"EFGH".as_slice()),
        (2, b"IJ".as_slice()),
    ] {
        let req = test::TestRequest::put()
            .uri(&format!(
                "/api/v1/teams/{team_id}/files/upload/{upload_id}/{chunk_number}"
            ))
            .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
            .insert_header(common::csrf_header_for(&owner_token))
            .insert_header(("Content-Type", "application/octet-stream"))
            .set_payload(bytes.to_vec())
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
    }

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/files/upload/{upload_id}/complete"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/files/upload/{upload_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["received_count"], 3);
    assert_eq!(body["data"]["chunks_on_disk"].as_array().unwrap().len(), 3);

    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/v1/teams/{team_id}/files/upload/{upload_id}/complete"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let file_id = body["data"]["id"].as_i64().unwrap();
    assert_eq!(body["data"]["name"], "chunked-team.txt");

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/files/{file_id}/download"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body = test::read_body(resp).await;
    assert_eq!(&body[..], b"ABCDEFGHIJ");

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/files/{file_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);
}

#[actix_web::test]
async fn test_team_chunk_upload_endpoint_rejects_oversized_chunk_with_413() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    set_default_policy_chunk_size(&state, 4).await;
    let mail_sender = state.mail_sender.clone();
    let app = create_test_app!(state);

    let _owner_id = register_user!(
        app,
        db,
        mail_sender,
        "teamchunklimit",
        "teamchunklimit@example.com",
        "password123"
    );
    let owner_token = login_user!(app, "teamchunklimit", "password123");

    let req = test::TestRequest::post()
        .uri("/api/v1/teams")
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({ "name": "Team Chunk Limit" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let team_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/files/upload/init"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({
            "filename": "team-oversized-chunk.txt",
            "total_size": 5
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["mode"], "chunked");
    assert_eq!(body["data"]["chunk_size"], 4);
    let upload_id = body["data"]["upload_id"].as_str().unwrap();

    let req = test::TestRequest::put()
        .uri(&format!(
            "/api/v1/teams/{team_id}/files/upload/{upload_id}/0"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .insert_header(("Content-Type", "application/octet-stream"))
        .set_payload(b"ABCDE".to_vec())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        actix_web::http::StatusCode::PAYLOAD_TOO_LARGE
    );
    let body: Value = test::read_body_json(resp).await;
    assert_upload_error_contract(&body, "upload.chunk_too_large");
}

#[actix_web::test]
async fn test_team_chunk_upload_endpoint_keeps_duplicate_size_validation() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    set_default_policy_chunk_size(&state, 4).await;
    let mail_sender = state.mail_sender.clone();
    let app = create_test_app!(state);

    let _owner_id = register_user!(
        app,
        db,
        mail_sender,
        "teamchunkdup",
        "teamchunkdup@example.com",
        "password123"
    );
    let owner_token = login_user!(app, "teamchunkdup", "password123");

    let req = test::TestRequest::post()
        .uri("/api/v1/teams")
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({ "name": "Team Chunk Duplicate" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let team_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/files/upload/init"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({
            "filename": "team-duplicate-chunk.txt",
            "total_size": 5
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let upload_id = body["data"]["upload_id"].as_str().unwrap();

    let req = test::TestRequest::put()
        .uri(&format!(
            "/api/v1/teams/{team_id}/files/upload/{upload_id}/0"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .insert_header(("Content-Type", "application/octet-stream"))
        .set_payload(b"ABCD".to_vec())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::put()
        .uri(&format!(
            "/api/v1/teams/{team_id}/files/upload/{upload_id}/0"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .insert_header(("Content-Type", "application/octet-stream"))
        .set_payload(b"ABC".to_vec())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_client_error());
    let body: Value = test::read_body_json(resp).await;
    assert_upload_error_contract(&body, "upload.chunk_size_mismatch");
}

#[actix_web::test]
async fn test_team_empty_upload_flow_uses_direct_and_creates_file() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let mail_sender = state.mail_sender.clone();
    let app = create_test_app!(state);

    let _owner_id = register_user!(
        app,
        db,
        mail_sender,
        "spaceempty",
        "spaceempty@example.com",
        "password123"
    );
    let owner_token = login_user!(app, "spaceempty", "password123");

    let req = test::TestRequest::post()
        .uri("/api/v1/teams")
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({ "name": "Empty Upload Team" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let team_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/files/upload/init"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({
            "filename": "empty-team.txt",
            "total_size": 0
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["mode"], "direct");
    assert!(body["data"]["upload_id"].is_null());

    let (boundary, payload) = build_binary_multipart_payload("empty-team.txt", b"");
    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/v1/teams/{team_id}/files/upload?declared_size=0"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .insert_header((
            "Content-Type",
            format!("multipart/form-data; boundary={boundary}"),
        ))
        .set_payload(payload)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["name"], "empty-team.txt");
    assert_eq!(body["data"]["team_id"].as_i64().unwrap(), team_id);
    assert_eq!(body["data"]["size"], 0);
}

#[actix_web::test]
async fn test_team_upload_session_enforces_owner_even_for_members() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    set_default_policy_chunk_size(&state, 4).await;
    let mail_sender = state.mail_sender.clone();
    let app = create_test_app!(state);

    let _owner_id = register_user!(
        app,
        db,
        mail_sender,
        "spaceownupload2",
        "spaceownupload2@example.com",
        "password123"
    );
    let member_id = register_user!(
        app,
        db,
        mail_sender,
        "spacememupload2",
        "spacememupload2@example.com",
        "password123"
    );
    let outsider_id = register_user!(
        app,
        db,
        mail_sender,
        "spaceoutupload2",
        "spaceoutupload2@example.com",
        "password123"
    );
    let owner_token = login_user!(app, "spaceownupload2", "password123");
    let member_token = login_user!(app, "spacememupload2", "password123");
    let outsider_token = login_user!(app, "spaceoutupload2", "password123");

    let req = test::TestRequest::post()
        .uri("/api/v1/teams")
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({ "name": "Upload Guard Team" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let team_id = body["data"]["id"].as_i64().unwrap();

    for user_id in [member_id, outsider_id] {
        let req = test::TestRequest::post()
            .uri(&format!("/api/v1/teams/{team_id}/members"))
            .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
            .insert_header(common::csrf_header_for(&owner_token))
            .set_json(serde_json::json!({ "user_id": user_id }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201);
    }

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/files/upload/init"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({
            "filename": "owner-only.bin",
            "total_size": 10
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["mode"], "chunked");
    let upload_id = body["data"]["upload_id"].as_str().unwrap().to_string();

    for token in [&member_token, &outsider_token] {
        let req = test::TestRequest::get()
            .uri(&format!("/api/v1/teams/{team_id}/files/upload/{upload_id}"))
            .insert_header(("Cookie", common::access_cookie_header(token)))
            .insert_header(common::csrf_header_for(token))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 403);

        let req = test::TestRequest::delete()
            .uri(&format!("/api/v1/teams/{team_id}/files/upload/{upload_id}"))
            .insert_header(("Cookie", common::access_cookie_header(token)))
            .insert_header(common::csrf_header_for(token))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 403);
    }

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/teams/{team_id}/files/upload/{upload_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/files/upload/{upload_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status() == 404 || resp.status() == 410);
}

#[actix_web::test]
async fn test_team_search_scopes_results_to_workspace_and_enforces_membership() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let mail_sender = state.mail_sender.clone();
    let app = create_test_app!(state);

    let _owner_id = register_user!(
        app,
        db,
        mail_sender,
        "teamsearch",
        "teamsearch@example.com",
        "password123"
    );
    let _outsider_id = register_user!(
        app,
        db,
        mail_sender,
        "teamsearchout",
        "teamsearchout@example.com",
        "password123"
    );
    let owner_token = login_user!(app, "teamsearch", "password123");
    let outsider_token = login_user!(app, "teamsearchout", "password123");

    let req = test::TestRequest::post()
        .uri("/api/v1/teams")
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({ "name": "Search Team" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let team_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/folders"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({ "name": "Docs" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let docs_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/folders"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({
            "name": "Research Notes",
            "parent_id": docs_id
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = multipart_request!(
        &format!("/api/v1/teams/{team_id}/files/upload?folder_id={docs_id}"),
        &owner_token,
        "roadmap-team.txt",
        "team roadmap",
    );
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let boundary = "----PersonalBoundary123";
    let payload = "------PersonalBoundary123\r\n\
         Content-Disposition: form-data; name=\"file\"; filename=\"roadmap-personal.txt\"\r\n\
         Content-Type: text/plain\r\n\r\n\
         personal roadmap\r\n\
         ------PersonalBoundary123--\r\n"
        .to_string();
    let req = test::TestRequest::post()
        .uri("/api/v1/files/upload")
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .insert_header((
            "Content-Type",
            format!("multipart/form-data; boundary={boundary}"),
        ))
        .set_payload(payload)
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/search?q=roadmap"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["total_files"], 1);
    assert_eq!(body["data"]["files"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"]["files"][0]["name"], "roadmap-team.txt");

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/teams/{team_id}/search?type=folder&q=research"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["total_folders"], 1);
    assert_eq!(body["data"]["folders"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"]["folders"][0]["name"], "Research Notes");

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/search?q=roadmap"))
        .insert_header(("Cookie", common::access_cookie_header(&outsider_token)))
        .insert_header(common::csrf_header_for(&outsider_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);
}

#[actix_web::test]
async fn test_team_search_rejects_invalid_params_via_shared_validation() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let mail_sender = state.mail_sender.clone();
    let app = create_test_app!(state);

    let _owner_id = register_user!(
        app,
        db,
        mail_sender,
        "teamsearchvalid",
        "teamsearchvalid@example.com",
        "password123"
    );
    let owner_token = login_user!(app, "teamsearchvalid", "password123");

    let req = test::TestRequest::post()
        .uri("/api/v1/teams")
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({ "name": "Search Validation Team" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let team_id = body["data"]["id"].as_i64().unwrap();

    for uri in [
        format!("/api/v1/teams/{team_id}/search?type=bogus"),
        format!("/api/v1/teams/{team_id}/search?created_after=not-a-date"),
        format!("/api/v1/teams/{team_id}/search?q=%20%20"),
    ] {
        let req = test::TestRequest::get()
            .uri(&uri)
            .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
            .insert_header(common::csrf_header_for(&owner_token))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400, "unexpected status for {uri}");
    }
}

#[actix_web::test]
async fn test_team_search_supports_category_and_extension_filters() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let mail_sender = state.mail_sender.clone();
    let app = create_test_app!(state);

    let _owner_id = register_user!(
        app,
        db,
        mail_sender,
        "teamfiletype",
        "teamfiletype@example.com",
        "password123"
    );
    let owner_token = login_user!(app, "teamfiletype", "password123");

    let req = test::TestRequest::post()
        .uri("/api/v1/teams")
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({ "name": "Type Search Team" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let team_id = body["data"]["id"].as_i64().unwrap();

    let req = multipart_request_with_mime!(
        &format!("/api/v1/teams/{team_id}/files/upload"),
        &owner_token,
        "team-photo.jpg",
        "image",
        "image/jpeg"
    );
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = multipart_request_with_mime!(
        &format!("/api/v1/teams/{team_id}/files/upload"),
        &owner_token,
        "team-report.pdf",
        "pdf",
        "application/pdf"
    );
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = multipart_request_with_mime!(
        &format!("/api/v1/teams/{team_id}/files/upload"),
        &owner_token,
        "team-pdf-notes.txt",
        "text",
        "text/plain"
    );
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let image_req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/search?category=image"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let image_resp = test::call_service(&app, image_req).await;
    assert_eq!(image_resp.status(), 200);
    let image_body: Value = test::read_body_json(image_resp).await;
    assert_eq!(image_body["data"]["total_files"], 1);
    assert_eq!(image_body["data"]["total_folders"], 0);
    assert_eq!(image_body["data"]["files"][0]["name"], "team-photo.jpg");
    assert_eq!(image_body["data"]["files"][0]["extension"], "jpg");
    assert_eq!(image_body["data"]["files"][0]["file_category"], "image");

    let pdf_req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/search?extensions=pdf"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let pdf_resp = test::call_service(&app, pdf_req).await;
    assert_eq!(pdf_resp.status(), 200);
    let pdf_body: Value = test::read_body_json(pdf_resp).await;
    assert_eq!(pdf_body["data"]["total_files"], 1);
    assert_eq!(pdf_body["data"]["files"][0]["name"], "team-report.pdf");

    let invalid_req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/teams/{team_id}/search?type=folder&category=image"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let invalid_resp = test::call_service(&app, invalid_req).await;
    assert_eq!(invalid_resp.status(), 400);
}

#[actix_web::test]
async fn test_team_batch_routes_support_copy_move_and_delete() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let mail_sender = state.mail_sender.clone();
    let app = create_test_app!(state);

    let _owner_id = register_user!(
        app,
        db,
        mail_sender,
        "teambatch",
        "teambatch@example.com",
        "password123"
    );
    let owner_token = login_user!(app, "teambatch", "password123");

    let req = test::TestRequest::post()
        .uri("/api/v1/teams")
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({ "name": "Batch Team" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let team_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/folders"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({ "name": "Source" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let source_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/folders"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({ "name": "Target" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let target_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/folders"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({
            "name": "Nested",
            "parent_id": source_id
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = multipart_request!(
        &format!("/api/v1/teams/{team_id}/files/upload?folder_id={source_id}"),
        &owner_token,
        "batch-team.txt",
        "batch team body",
    );
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let file_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/batch/copy"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({
            "file_ids": [],
            "folder_ids": [source_id],
            "target_folder_id": target_id
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["succeeded"], 1);
    assert_eq!(body["data"]["failed"], 0);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/folders/{target_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["folders"].as_array().unwrap().len(), 1);

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/batch/move"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({
            "file_ids": [file_id],
            "folder_ids": [],
            "target_folder_id": target_id
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["succeeded"], 1);
    assert_eq!(body["data"]["failed"], 0);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/folders/{source_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert!(body["data"]["files"].as_array().unwrap().is_empty());

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/folders/{target_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["files"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"]["files"][0]["id"], file_id);

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/batch/delete"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({
            "file_ids": [file_id],
            "folder_ids": []
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["succeeded"], 1);
    assert_eq!(body["data"]["failed"], 0);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/trash"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["files"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"]["files"][0]["id"], file_id);
}

#[actix_web::test]
async fn test_team_batch_delete_preserves_scope_and_locked_failures() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let mail_sender = state.mail_sender.clone();
    let app = create_test_app!(state);

    let _owner_id = register_user!(
        app,
        db,
        mail_sender,
        "teambatchpartial",
        "teambatchpartial@example.com",
        "password123"
    );
    let owner_token = login_user!(app, "teambatchpartial", "password123");

    let req = test::TestRequest::post()
        .uri("/api/v1/teams")
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({ "name": "Partial Batch Team" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let team_id = body["data"]["id"].as_i64().unwrap();

    let req = multipart_request!(
        &format!("/api/v1/teams/{team_id}/files/upload"),
        &owner_token,
        "team-ok.txt",
        "team ok body",
    );
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let team_file_id = body["data"]["id"].as_i64().unwrap();

    let req = multipart_request!(
        &format!("/api/v1/teams/{team_id}/files/upload"),
        &owner_token,
        "team-locked.txt",
        "team locked body",
    );
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let locked_team_file_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/v1/teams/{team_id}/files/{locked_team_file_id}/lock"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({ "locked": true }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let personal_file_id = upload_test_file_named!(app, owner_token, "personal-batch.txt");

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/batch/delete"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({
            "file_ids": [team_file_id, locked_team_file_id, personal_file_id],
            "folder_ids": []
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["succeeded"], 1);
    assert_eq!(body["data"]["failed"], 2);
    let errors = body["data"]["errors"].as_array().unwrap();
    assert_eq!(errors.len(), 2);
    assert!(
        errors
            .iter()
            .any(|item| item["entity_id"] == locked_team_file_id)
    );
    assert!(
        errors
            .iter()
            .any(|item| item["entity_id"] == personal_file_id)
    );

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/trash"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["files"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"]["files"][0]["id"], team_file_id);

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/teams/{team_id}/files/{locked_team_file_id}"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/files/{personal_file_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn test_team_share_batch_delete_preserves_partial_failures() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let mail_sender = state.mail_sender.clone();
    let app = create_test_app!(state);

    let _owner_id = register_user!(
        app,
        db,
        mail_sender,
        "teamsharebatch",
        "teamsharebatch@example.com",
        "password123"
    );
    let owner_token = login_user!(app, "teamsharebatch", "password123");

    let req = test::TestRequest::post()
        .uri("/api/v1/teams")
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({ "name": "Team Share Batch" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let team_id = body["data"]["id"].as_i64().unwrap();

    let req = multipart_request!(
        &format!("/api/v1/teams/{team_id}/files/upload"),
        &owner_token,
        "team-share-batch.txt",
        "share batch body",
    );
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let file_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/shares"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({
            "target": { "type": "file", "id": file_id }
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let share_id = body["data"]["id"].as_i64().unwrap();
    let share_token = body["data"]["token"].as_str().unwrap().to_string();

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/shares/batch-delete"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({
            "share_ids": [share_id, 999999]
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["succeeded"], 1);
    assert_eq!(body["data"]["failed"], 1);
    let errors = body["data"]["errors"].as_array().unwrap();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0]["entity_id"], 999999);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/shares"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["total"], 0);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/s/{share_token}"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);
}
