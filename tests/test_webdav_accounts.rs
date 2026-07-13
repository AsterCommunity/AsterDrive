//! WebDAV 账号管理测试

#[macro_use]
mod common;

use actix_web::test;
use aster_drive::db::repository::webdav_account_repo;
use aster_drive::entities::{audit_log, team, team_member, user, webdav_account};
use aster_drive::runtime::SharedRuntimeState;
use aster_drive::types::{AuditAction, TeamMemberRole, UserRole, UserStatus};
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, IntoActiveModel, PaginatorTrait, QueryFilter, Set,
};
use serde_json::Value;

fn webdav_test_password(label: &str) -> String {
    format!("TEST_PASSWORD_{label}")
}

async fn seed_team_for_webdav_account_test(
    state: &aster_drive::runtime::PrimaryAppState,
    user_id: i64,
) -> i64 {
    let now = Utc::now();
    let default_policy_group =
        aster_drive::db::repository::policy_group_repo::find_default_group(state.writer_db())
            .await
            .expect("default policy group lookup should succeed")
            .expect("default policy group should exist");
    let team = team::ActiveModel {
        name: Set("WebDAV Account Team".to_string()),
        description: Set("Team WebDAV account management".to_string()),
        created_by: Set(user_id),
        storage_used: Set(0),
        storage_quota: Set(0),
        policy_group_id: Set(Some(default_policy_group.id)),
        created_at: Set(now),
        updated_at: Set(now),
        archived_at: Set(None),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .expect("team should be inserted");
    team_member::ActiveModel {
        team_id: Set(team.id),
        user_id: Set(user_id),
        role: Set(TeamMemberRole::Owner),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .expect("team owner membership should be inserted");
    team.id
}

async fn add_team_member_for_webdav_account_test(
    state: &aster_drive::runtime::PrimaryAppState,
    team_id: i64,
    user_id: i64,
    role: TeamMemberRole,
) {
    let now = Utc::now();
    team_member::ActiveModel {
        team_id: Set(team_id),
        user_id: Set(user_id),
        role: Set(role),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .expect("team membership should be inserted");
}

async fn seed_active_user_for_webdav_account_test(
    state: &aster_drive::runtime::PrimaryAppState,
    username: &str,
) -> user::Model {
    let now = Utc::now();
    user::ActiveModel {
        username: Set(username.to_string()),
        email: Set(format!("{username}@example.com")),
        password_hash: Set(aster_forge_crypto::hash_password("password123")
            .expect("test user password should hash")),
        role: Set(UserRole::User),
        status: Set(UserStatus::Active),
        session_version: Set(0),
        email_verified_at: Set(Some(now)),
        pending_email: Set(None),
        storage_used: Set(0),
        storage_quota: Set(0),
        policy_group_id: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
        config: Set(None),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .expect("active user should be inserted")
}

async fn webdav_audit_action_count(
    state: &aster_drive::runtime::PrimaryAppState,
    action: AuditAction,
) -> u64 {
    aster_forge_audit::flush_global_audit_log_manager().await;
    audit_log::Entity::find()
        .filter(audit_log::Column::Action.eq(action))
        .count(state.writer_db())
        .await
        .expect("audit log query should succeed")
}

#[actix_web::test]
async fn test_webdav_account_crud() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);

    // 创建 WebDAV 账号
    let req = test::TestRequest::post()
        .uri("/api/v1/webdav-accounts")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "username": "webdav_user",
            "password": webdav_test_password("PERSONAL_CRUD")
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let status = resp.status();
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(status, 201, "personal account create failed: {body}");
    assert_eq!(body["data"]["username"], "webdav_user");
    let account_id = body["data"]["id"].as_i64().unwrap();
    assert_eq!(
        webdav_audit_action_count(&state, AuditAction::WebdavAccountCreate).await,
        1
    );
    assert_eq!(
        webdav_audit_action_count(&state, AuditAction::TeamWebdavAccountCreate).await,
        0
    );

    // 分页列出账号
    let req = test::TestRequest::get()
        .uri("/api/v1/webdav-accounts?limit=1&offset=0")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["total"], 1);
    assert_eq!(body["data"]["limit"], 1);
    assert_eq!(body["data"]["offset"], 0);
    assert_eq!(body["data"]["items"].as_array().unwrap().len(), 1);

    // 禁用账号
    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/webdav-accounts/{account_id}/toggle"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["is_active"], false);

    // 再次 toggle 启用
    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/webdav-accounts/{account_id}/toggle"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["is_active"], true);

    // 删除账号
    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/webdav-accounts/{account_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    // 列表应为空
    let req = test::TestRequest::get()
        .uri("/api/v1/webdav-accounts")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["items"].as_array().unwrap().len(), 0);
    assert_eq!(body["data"]["total"], 0);
}

#[actix_web::test]
async fn test_team_webdav_account_crud_is_separate_from_personal_accounts() {
    let state = common::setup().await;
    let team_state = state.clone();
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let me_req = test::TestRequest::get()
        .uri("/api/v1/auth/me")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let me_resp = test::call_service(&app, me_req).await;
    assert_eq!(me_resp.status(), 200);
    let me_body: Value = test::read_body_json(me_resp).await;
    let user_id = me_body["data"]["id"].as_i64().unwrap();
    let team_id = seed_team_for_webdav_account_test(&team_state, user_id).await;

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/webdav-accounts"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
                "username": "team_webdav_user",
                "password": webdav_test_password("TEAM_CRUD")
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["username"], "team_webdav_user");
    assert_eq!(body["data"]["team_id"], team_id);
    let account_id = body["data"]["id"].as_i64().unwrap();
    assert_eq!(
        webdav_audit_action_count(&team_state, AuditAction::TeamWebdavAccountCreate).await,
        1
    );
    assert_eq!(
        webdav_audit_action_count(&team_state, AuditAction::WebdavAccountCreate).await,
        0
    );

    let req = test::TestRequest::get()
        .uri("/api/v1/webdav-accounts")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["data"]["total"], 0,
        "personal account list must not include team WebDAV accounts"
    );

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/webdav-accounts"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["total"], 1);
    assert_eq!(body["data"]["items"][0]["team_id"], team_id);

    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/v1/teams/{team_id}/webdav-accounts/{account_id}/toggle"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["is_active"], false);
    assert_eq!(body["data"]["team_id"], team_id);
    assert_eq!(
        webdav_audit_action_count(&team_state, AuditAction::TeamWebdavAccountToggle).await,
        1
    );

    let req = test::TestRequest::delete()
        .uri(&format!(
            "/api/v1/teams/{team_id}/webdav-accounts/{account_id}"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        webdav_audit_action_count(&team_state, AuditAction::TeamWebdavAccountDelete).await,
        1
    );

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/webdav-accounts"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["total"], 0);
}

#[actix_web::test]
async fn test_team_webdav_account_visibility_and_management_boundaries() {
    let state = common::setup().await;
    let team_state = state.clone();
    let app = create_test_app!(state);
    let (owner_token, _) = register_and_login!(app);

    let me_req = test::TestRequest::get()
        .uri("/api/v1/auth/me")
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let me_resp = test::call_service(&app, me_req).await;
    assert_eq!(me_resp.status(), 200);
    let me_body: Value = test::read_body_json(me_resp).await;
    let owner_id = me_body["data"]["id"].as_i64().unwrap();
    let team_id = seed_team_for_webdav_account_test(&team_state, owner_id).await;

    let member = seed_active_user_for_webdav_account_test(&team_state, "team-webdav-member").await;
    add_team_member_for_webdav_account_test(
        &team_state,
        team_id,
        member.id,
        TeamMemberRole::Member,
    )
    .await;
    let (member_token, _) = login_user!(app, "team-webdav-member", "password123");

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/webdav-accounts"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .set_json(serde_json::json!({
            "username": "owner_team_webdav",
            "password": webdav_test_password("OWNER")
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let owner_account_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/webdav-accounts"))
        .insert_header(("Cookie", common::access_cookie_header(&member_token)))
        .insert_header(common::csrf_header_for(&member_token))
        .set_json(serde_json::json!({
            "username": "member_team_webdav",
            "password": webdav_test_password("MEMBER")
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        201,
        "plain team members should create their own team WebDAV accounts"
    );
    let body: Value = test::read_body_json(resp).await;
    let member_account_id = body["data"]["id"].as_i64().unwrap();
    assert!(body["data"].get("user_id").is_none());

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/webdav-accounts"))
        .insert_header(("Cookie", common::access_cookie_header(&member_token)))
        .insert_header(common::csrf_header_for(&member_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let member_items = body["data"]["items"].as_array().unwrap();
    assert_eq!(body["data"]["total"], 1);
    assert_eq!(member_items[0]["id"], member_account_id);
    assert_eq!(member_items[0]["user_id"], member.id);

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/teams/{team_id}/webdav-accounts"))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["data"]["total"], 2,
        "team owners/admins should see all team WebDAV accounts"
    );

    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/v1/teams/{team_id}/webdav-accounts/{owner_account_id}/toggle"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&member_token)))
        .insert_header(common::csrf_header_for(&member_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        403,
        "plain team members must not toggle another member's team WebDAV account"
    );

    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/v1/teams/{team_id}/webdav-accounts/{member_account_id}/toggle"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&member_token)))
        .insert_header(common::csrf_header_for(&member_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["is_active"], false);

    let req = test::TestRequest::delete()
        .uri(&format!(
            "/api/v1/teams/{team_id}/webdav-accounts/{owner_account_id}"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&member_token)))
        .insert_header(common::csrf_header_for(&member_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        403,
        "plain team members must not delete another member's team WebDAV account"
    );

    let req = test::TestRequest::delete()
        .uri(&format!(
            "/api/v1/teams/{team_id}/webdav-accounts/{member_account_id}"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&member_token)))
        .insert_header(common::csrf_header_for(&member_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::delete()
        .uri(&format!(
            "/api/v1/teams/{team_id}/webdav-accounts/{owner_account_id}"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&owner_token)))
        .insert_header(common::csrf_header_for(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
}

#[actix_web::test]
async fn test_team_webdav_accounts_are_deleted_when_archived_team_is_purged() {
    let state = common::setup().await;
    let now = Utc::now();
    let owner = seed_active_user_for_webdav_account_test(&state, "team-webdav-cleanup-owner").await;
    let team_id = seed_team_for_webdav_account_test(&state, owner.id).await;

    webdav_account::ActiveModel {
        user_id: Set(owner.id),
        team_id: Set(Some(team_id)),
        username: Set("team-cleanup-webdav".to_string()),
        password_hash: Set(aster_forge_crypto::hash_password("team-cleanup-pass")
            .expect("team cleanup WebDAV password should hash")),
        root_folder_id: Set(None),
        is_active: Set(true),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .expect("team WebDAV cleanup account should be inserted");

    let mut active_team =
        aster_drive::db::repository::team_repo::find_by_id(state.writer_db(), team_id)
            .await
            .expect("team should exist")
            .into_active_model();
    active_team.archived_at = Set(Some(now - chrono::Duration::days(2)));
    active_team
        .update(state.writer_db())
        .await
        .expect("team should be archived");
    state.runtime_config.apply(common::system_config_model(
        "team_archive_retention_days",
        "0",
    ));

    let deleted = aster_drive::services::workspace::team::cleanup_expired_archived_teams(&state)
        .await
        .expect("archived team cleanup should succeed");
    assert_eq!(deleted, 1);

    let remaining = webdav_account_repo::find_by_team(state.writer_db(), team_id)
        .await
        .expect("remaining team WebDAV account query should succeed");
    assert!(
        remaining.is_empty(),
        "purging an archived team must delete its WebDAV accounts"
    );
}

#[actix_web::test]
async fn test_team_webdav_account_rejects_personal_root_folder() {
    let state = common::setup().await;
    let team_state = state.clone();
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let me_req = test::TestRequest::get()
        .uri("/api/v1/auth/me")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let me_resp = test::call_service(&app, me_req).await;
    assert_eq!(me_resp.status(), 200);
    let me_body: Value = test::read_body_json(me_resp).await;
    let owner_id = me_body["data"]["id"].as_i64().unwrap();
    let team_id = seed_team_for_webdav_account_test(&team_state, owner_id).await;

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "personal-root"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let personal_root_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/webdav-accounts"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "username": "team_root_guard",
            "password": webdav_test_password("TEAM_ROOT_GUARD"),
            "root_folder_id": personal_root_id
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);
}

#[actix_web::test]
async fn test_webdav_settings_returns_public_endpoint_when_configured() {
    let state = common::setup().await;
    state.runtime_config.apply(common::system_config_model(
        aster_drive::config::site_url::PUBLIC_SITE_URL_KEY,
        r#"["https://drive.example.com"]"#,
    ));
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::get()
        .uri("/api/v1/webdav-accounts/settings")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;

    assert_eq!(body["data"]["prefix"], "/webdav");
    assert_eq!(
        body["data"]["endpoint"],
        "https://drive.example.com/webdav/"
    );
}

#[actix_web::test]
async fn test_webdav_settings_uses_matching_public_site_url_from_host() {
    let state = common::setup().await;
    state.runtime_config.apply(common::system_config_model(
        aster_drive::config::site_url::PUBLIC_SITE_URL_KEY,
        r#"["http://drive.example.com","http://panel.example.com"]"#,
    ));
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::get()
        .uri("/api/v1/webdav-accounts/settings")
        .insert_header(("Host", "panel.example.com"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;

    assert_eq!(body["data"]["endpoint"], "http://panel.example.com/webdav/");
}

#[actix_web::test]
async fn test_webdav_settings_falls_back_to_relative_endpoint_without_public_site_url() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::get()
        .uri("/api/v1/webdav-accounts/settings")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;

    assert_eq!(body["data"]["prefix"], "/webdav");
    assert_eq!(body["data"]["endpoint"], "/webdav/");
}

#[actix_web::test]
async fn test_webdav_account_list_resolves_nested_paths() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "A" }))
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

    for (username, folder_id) in [("path_user_a", a_id), ("path_user_b", b_id)] {
        let req = test::TestRequest::post()
            .uri("/api/v1/webdav-accounts")
            .insert_header(("Cookie", common::access_cookie_header(&token)))
            .insert_header(common::csrf_header_for(&token))
            .set_json(serde_json::json!({
                "username": username,
                "password": webdav_test_password("ROOT_PATH"),
                "root_folder_id": folder_id,
            }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201);
    }

    let req = test::TestRequest::get()
        .uri("/api/v1/webdav-accounts?limit=10&offset=0")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let items = body["data"]["items"].as_array().unwrap();
    let path_a = items
        .iter()
        .find(|i| i["username"] == "path_user_a")
        .unwrap()["root_folder_path"]
        .as_str()
        .unwrap();
    let path_b = items
        .iter()
        .find(|i| i["username"] == "path_user_b")
        .unwrap()["root_folder_path"]
        .as_str()
        .unwrap();
    assert_eq!(path_a, "/A");
    assert_eq!(path_b, "/A/B");
}

#[actix_web::test]
async fn test_webdav_account_rejects_duplicate_username_and_foreign_root_folder() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let mail_sender = state.mail_sender.clone();
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/webdav-accounts")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "username": "dup_user",
            "password": webdav_test_password("DUPLICATE")
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::post()
        .uri("/api/v1/webdav-accounts")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "username": "dup_user",
            "password": webdav_test_password("DUPLICATE")
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/register")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "username": "other-user",
            "email": "other-user@example.com",
            "password": "password123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let _ = confirm_latest_contact_verification!(app, db, mail_sender);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/login")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "identifier": "other-user",
            "password": "password123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let other_token = common::extract_cookie(&resp, "aster_access").unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&other_token)))
        .insert_header(common::csrf_header_for(&other_token))
        .set_json(serde_json::json!({ "name": "Other Root" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let body: Value = test::read_body_json(resp).await;
    let foreign_folder_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/webdav-accounts")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "username": "foreign-root",
            "password": webdav_test_password("FOREIGN_ROOT"),
            "root_folder_id": foreign_folder_id,
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_client_error());
}

#[actix_web::test]
async fn test_webdav_account_auto_generated_password_and_disabled_test_rejected() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/webdav-accounts")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "username": "auto-pass-user"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let password = body["data"]["password"].as_str().unwrap().to_string();
    assert_eq!(password.len(), 16);
    let account_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/webdav-accounts/test")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "username": "auto-pass-user",
            "password": password
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/webdav-accounts/{account_id}/toggle"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::post()
        .uri("/api/v1/webdav-accounts/test")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "username": "auto-pass-user",
            "password": body["data"]["password"]
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_client_error());
}

#[actix_web::test]
async fn test_webdav_account_test_connection() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    // 创建账号
    let req = test::TestRequest::post()
        .uri("/api/v1/webdav-accounts")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "username": "test_conn",
            "password": webdav_test_password("TEST_CONNECTION")
        }))
        .to_request();
    test::call_service(&app, req).await;

    // 测试连接（正确密码）
    let req = test::TestRequest::post()
        .uri("/api/v1/webdav-accounts/test")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "username": "test_conn",
            "password": webdav_test_password("TEST_CONNECTION")
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    // 测试连接（错误密码）
    let req = test::TestRequest::post()
        .uri("/api/v1/webdav-accounts/test")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "username": "test_conn",
            "password": "wrong"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status() == 401 || resp.status() == 400);
}

#[actix_web::test]
async fn test_webdav_account_rejects_blank_username() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/webdav-accounts")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "username": "   ",
            "password": webdav_test_password("BLANK_USERNAME")
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["msg"], "value cannot be empty");
}
