//! Team and membership management tests

#[macro_use]
mod common;
use aster_drive::runtime::SharedRuntimeState;

use actix_web::test;
use serde_json::Value;

macro_rules! register_user {
    ($app:expr, $db:expr, $mail_sender:expr, $username:expr, $email:expr, $password:expr) => {{
        common::create_test_account_via_api(
            &$app,
            &$db,
            &$mail_sender,
            $username,
            $email,
            $password,
        )
        .await
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

#[actix_web::test]
async fn test_team_crud_and_member_lifecycle() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let mail_sender = state.mail_sender.clone();
    let app = create_test_app!(state);

    let owner_id = register_user!(
        app,
        db,
        mail_sender,
        "owner1",
        "owner1@example.com",
        "password123"
    );
    let member_id = register_user!(
        app,
        db,
        mail_sender,
        "member1",
        "member1@example.com",
        "password123"
    );
    let owner_token = login_user!(app, "owner1", "password123");
    let member_token = login_user!(app, "member1", "password123");

    let req = test::TestRequest::post()
        .uri("/api/v1/teams")
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({
            "name": "Design",
            "description": "Core design team"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let team_id = body["data"]["id"].as_i64().unwrap();
    assert_eq!(body["data"]["created_by"]["id"], owner_id);
    assert_eq!(body["data"]["created_by"]["username"], "owner1");
    assert_eq!(body["data"]["my_role"], "owner");
    assert_eq!(body["data"]["member_count"], 1);

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/members"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({
            "identifier": "member1"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["user_id"], member_id);
    assert_eq!(body["data"]["role"], "member");

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/teams/{team_id}/members?limit=1&offset=0&keyword=member1"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["total"], 1);
    assert_eq!(body["data"]["limit"], 1);
    assert_eq!(body["data"]["offset"], 0);
    assert_eq!(body["data"]["owner_count"], 1);
    assert_eq!(body["data"]["manager_count"], 1);
    assert_eq!(body["data"]["items"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"]["items"][0]["user"]["username"], "member1");

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/members?keyword=emb"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["total"], 1);
    assert_eq!(body["data"]["items"][0]["user"]["username"], "member1");

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/members?keyword=be"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["total"], 1);
    assert_eq!(body["data"]["items"][0]["user"]["username"], "member1");

    let req = test::TestRequest::get()
        .uri("/api/v1/teams")
        .insert_header(("Cookie", common::access_cookie_header(&member_token)))
        .insert_header(common::csrf_header_for(&member_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"][0]["my_role"], "member");

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/teams/{team_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({
            "name": "Design Ops",
            "description": "Updated"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["name"], "Design Ops");

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/teams/{team_id}/members/{member_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({
            "role": "admin"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["role"], "admin");

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/teams/{team_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&member_token)))
        .insert_header(common::csrf_header_for(&member_token))
        .set_json(serde_json::json!({
            "description": "Admin updated"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["description"], "Admin updated");
}

#[actix_web::test]
async fn test_team_list_keyword_filters_visible_teams_and_archived_state() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let mail_sender = state.mail_sender.clone();
    let app = create_test_app!(state);

    let owner_id = register_user!(
        app,
        db,
        mail_sender,
        "teamsearchowner",
        "teamsearchowner@example.com",
        "password123"
    );
    let member_id = register_user!(
        app,
        db,
        mail_sender,
        "teamsearchmember",
        "teamsearchmember@example.com",
        "password123"
    );
    let owner_token = login_user!(app, "teamsearchowner", "password123");
    let member_token = login_user!(app, "teamsearchmember", "password123");

    macro_rules! create_team {
        ($name:expr, $description:expr) => {{
        let req = test::TestRequest::post()
            .uri("/api/v1/teams")
            .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
            .insert_header(common::csrf_header_for(&owner_token))
            .set_json(serde_json::json!({
                "name": $name,
                "description": $description
            }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201);
        let body: Value = test::read_body_json(resp).await;
        body["data"]["id"].as_i64().unwrap()
    }};
    }

    let alpha_id = create_team!("Alpha Ops", "Operations workspace");
    let beta_id = create_team!("Beta Studio", "Backend platform squad");
    let archived_id = create_team!("Archived Ops", "Old operations");
    let hidden_id = create_team!("Hidden Ops", "Owner-only workspace");

    for team_id in [alpha_id, beta_id, archived_id] {
        let req = test::TestRequest::post()
            .uri(&format!("/api/v1/teams/{team_id}/members"))
            .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
            .insert_header(common::csrf_header_for(&owner_token))
            .set_json(serde_json::json!({ "user_id": member_id }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201);
    }

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/teams/{archived_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri("/api/v1/teams?keyword=Ops")
        .insert_header(("Cookie", common::access_cookie_header(&member_token)))
        .insert_header(common::csrf_header_for(&member_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let teams = body["data"].as_array().unwrap();
    assert_eq!(teams.len(), 1);
    assert_eq!(teams[0]["id"], alpha_id);
    assert_eq!(teams[0]["my_role"], "member");
    assert_ne!(teams[0]["id"], hidden_id);
    assert_eq!(teams[0]["created_by"]["id"], owner_id);

    let req = test::TestRequest::get()
        .uri("/api/v1/teams?keyword=backend")
        .insert_header(("Cookie", common::access_cookie_header(&member_token)))
        .insert_header(common::csrf_header_for(&member_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let teams = body["data"].as_array().unwrap();
    assert_eq!(teams.len(), 1);
    assert_eq!(teams[0]["id"], beta_id);

    let req = test::TestRequest::get()
        .uri("/api/v1/teams?archived=true&keyword=Ops")
        .insert_header(("Cookie", common::access_cookie_header(&member_token)))
        .insert_header(common::csrf_header_for(&member_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let teams = body["data"].as_array().unwrap();
    assert_eq!(teams.len(), 1);
    assert_eq!(teams[0]["id"], archived_id);
    assert!(teams[0]["archived_at"].is_string());

    let req = test::TestRequest::get()
        .uri("/api/v1/teams?keyword=%20%20&limit=1&offset=1")
        .insert_header(("Cookie", common::access_cookie_header(&member_token)))
        .insert_header(common::csrf_header_for(&member_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let teams = body["data"].as_array().unwrap();
    assert_eq!(teams.len(), 1);
    assert!(
        teams[0]["id"] == alpha_id || teams[0]["id"] == beta_id,
        "limited active list should return one visible active team"
    );
    assert_ne!(teams[0]["id"], archived_id);
    assert_ne!(teams[0]["id"], hidden_id);
}

#[actix_web::test]
async fn test_team_permissions_for_member_and_admin() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let mail_sender = state.mail_sender.clone();
    let app = create_test_app!(state);

    let owner_id = register_user!(
        app,
        db,
        mail_sender,
        "owner2",
        "owner2@example.com",
        "password123"
    );
    let member_id = register_user!(
        app,
        db,
        mail_sender,
        "member2",
        "member2@example.com",
        "password123"
    );
    let extra_id = register_user!(
        app,
        db,
        mail_sender,
        "extra2",
        "extra2@example.com",
        "password123"
    );
    let owner_token = login_user!(app, "owner2", "password123");
    let member_token = login_user!(app, "member2", "password123");

    let req = test::TestRequest::post()
        .uri("/api/v1/teams")
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({ "name": "Platform" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let team_id = body["data"]["id"].as_i64().unwrap();

    for user_id in [member_id, extra_id] {
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
        .uri(&format!("/api/v1/teams/{team_id}/members"))
        .insert_header(("Cookie", common::access_cookie_header(&member_token)))
        .insert_header(common::csrf_header_for(&member_token))
        .set_json(serde_json::json!({ "user_id": owner_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/teams/{team_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&member_token)))
        .insert_header(common::csrf_header_for(&member_token))
        .set_json(serde_json::json!({ "description": "nope" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/teams/{team_id}/members/{member_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({ "role": "admin" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/teams/{team_id}/members/{extra_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&member_token)))
        .insert_header(common::csrf_header_for(&member_token))
        .set_json(serde_json::json!({ "role": "owner" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/teams/{team_id}/members/{owner_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&member_token)))
        .insert_header(common::csrf_header_for(&member_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);
}

#[actix_web::test]
async fn test_only_system_admin_can_create_team() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let mail_sender = state.mail_sender.clone();
    let app = create_test_app!(state);

    let _admin_id = register_user!(
        app,
        db,
        mail_sender,
        "teamadminroot",
        "teamadminroot@example.com",
        "password123"
    );
    let _user_id = register_user!(
        app,
        db,
        mail_sender,
        "plainteamuser",
        "plainteamuser@example.com",
        "password123"
    );
    let user_token = login_user!(app, "plainteamuser", "password123");

    let req = test::TestRequest::post()
        .uri("/api/v1/teams")
        .insert_header(("Cookie", common::access_cookie_header(&user_token)))
        .insert_header(common::csrf_header_for(&user_token))
        .set_json(serde_json::json!({ "name": "Should Fail" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["msg"], "team creation is restricted to system admins");
}

#[actix_web::test]
async fn test_team_owner_protection_and_archive() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let mail_sender = state.mail_sender.clone();
    let app = create_test_app!(state);

    let owner_id = register_user!(
        app,
        db,
        mail_sender,
        "owner3",
        "owner3@example.com",
        "password123"
    );
    let co_owner_id = register_user!(
        app,
        db,
        mail_sender,
        "owner4",
        "owner4@example.com",
        "password123"
    );
    let owner_token = login_user!(app, "owner3", "password123");
    let co_owner_token = login_user!(app, "owner4", "password123");

    let req = test::TestRequest::post()
        .uri("/api/v1/teams")
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({ "name": "Ops" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let team_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/teams/{team_id}/members/{owner_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({ "role": "member" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/teams/{team_id}/members/{owner_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/members"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({
            "user_id": co_owner_id,
            "role": "owner"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/teams/{team_id}/members/{owner_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri("/api/v1/teams")
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"].as_array().unwrap().len(), 0);

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/teams/{team_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&co_owner_token)))
        .insert_header(common::csrf_header_for(&co_owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri("/api/v1/teams")
        .insert_header(("Cookie", common::access_cookie_header(&co_owner_token)))
        .insert_header(common::csrf_header_for(&co_owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"].as_array().unwrap().len(), 0);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&co_owner_token)))
        .insert_header(common::csrf_header_for(&co_owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);
}

#[actix_web::test]
async fn test_team_admin_can_restore_archived_team() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let mail_sender = state.mail_sender.clone();
    let app = create_test_app!(state);

    let owner_id = register_user!(
        app,
        db,
        mail_sender,
        "restore-owner",
        "restore-owner@example.com",
        "password123"
    );
    let admin_id = register_user!(
        app,
        db,
        mail_sender,
        "restore-admin",
        "restore-admin@example.com",
        "password123"
    );
    let member_id = register_user!(
        app,
        db,
        mail_sender,
        "restore-member",
        "restore-member@example.com",
        "password123"
    );
    let owner_token = login_user!(app, "restore-owner", "password123");
    let admin_token = login_user!(app, "restore-admin", "password123");
    let member_token = login_user!(app, "restore-member", "password123");

    let req = test::TestRequest::post()
        .uri("/api/v1/teams")
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({ "name": "Restore Team" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let team_id = body["data"]["id"].as_i64().unwrap();
    assert_eq!(body["data"]["created_by"]["id"], owner_id);
    assert_eq!(body["data"]["created_by"]["username"], "restore-owner");

    for (user_id, role) in [(admin_id, "admin"), (member_id, "member")] {
        let req = test::TestRequest::post()
            .uri(&format!("/api/v1/teams/{team_id}/members"))
            .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
            .insert_header(common::csrf_header_for(&owner_token))
            .set_json(serde_json::json!({
                "user_id": user_id,
                "role": role
            }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201);
    }

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/teams/{team_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri("/api/v1/teams")
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"].as_array().unwrap().len(), 0);

    let req = test::TestRequest::get()
        .uri("/api/v1/teams?archived=true")
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"][0]["id"], team_id);
    assert!(body["data"][0]["archived_at"].is_string());

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/restore"))
        .insert_header(("Cookie", common::access_cookie_header(&member_token)))
        .insert_header(common::csrf_header_for(&member_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/restore"))
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["id"], team_id);
    assert_eq!(body["data"]["my_role"], "admin");
    assert!(body["data"]["archived_at"].is_null());

    let req = test::TestRequest::get()
        .uri("/api/v1/teams")
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"][0]["id"], team_id);
}

#[actix_web::test]
async fn test_team_add_member_rejects_missing_target() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/teams")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Validation Team" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let team_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/members"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({}))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["msg"], "user_id or identifier is required");
}
