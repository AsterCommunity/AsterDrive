//! 集成测试：`user_invitations`。

#[macro_use]
mod common;

use actix_web::test;
use aster_drive::config::{auth_runtime, local_email_policy, site_url};
use aster_drive::entities::{audit_log, mail_outbox, user_invitation};
use aster_drive::runtime::SharedRuntimeState;
use aster_drive::services::ops::audit;
use aster_drive::types::{AuditAction, MailTemplateCode, UserInvitationStatus};
use chrono::{DateTime, Duration, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, IntoActiveModel, QueryFilter, QueryOrder, Set,
};
use serde_json::Value;

macro_rules! create_invitation {
    ($app:expr, $admin_token:expr, $email:expr) => {{
        let req = test::TestRequest::post()
            .uri("/api/v1/admin/users/invitations")
            .insert_header(("Cookie", common::access_cookie_header(&$admin_token)))
            .insert_header(common::csrf_header_for(&$admin_token))
            .set_json(serde_json::json!({ "email": $email }))
            .to_request();
        let resp = test::call_service(&$app, req).await;
        assert_eq!(resp.status(), 201, "create invitation should return 201");
        let body: Value = test::read_body_json(resp).await;
        body["data"].clone()
    }};
}

macro_rules! create_invitation_with_status {
    ($app:expr, $admin_token:expr, $email:expr) => {{
        let req = test::TestRequest::post()
            .uri("/api/v1/admin/users/invitations")
            .insert_header(("Cookie", common::access_cookie_header(&$admin_token)))
            .insert_header(common::csrf_header_for(&$admin_token))
            .set_json(serde_json::json!({ "email": $email }))
            .to_request();
        let resp = test::call_service(&$app, req).await;
        let status = resp.status();
        let body: Value = test::read_body_json(resp).await;
        (status, body)
    }};
}

macro_rules! accept_invitation_with_status {
    ($app:expr, $token:expr, $username:expr, $password:expr) => {{
        let req = test::TestRequest::post()
            .uri(&format!(
                "/api/v1/auth/invitations/{}/accept",
                urlencoding::encode($token)
            ))
            .set_json(serde_json::json!({
                "username": $username,
                "password": $password
            }))
            .to_request();
        let resp = test::call_service(&$app, req).await;
        let status = resp.status();
        let body: Value = test::read_body_json(resp).await;
        (status, body)
    }};
}

macro_rules! list_invitations {
    ($app:expr, $admin_token:expr) => {{
        let req = test::TestRequest::get()
            .uri("/api/v1/admin/users/invitations?limit=20&offset=0")
            .insert_header(("Cookie", common::access_cookie_header(&$admin_token)))
            .insert_header(common::csrf_header_for(&$admin_token))
            .to_request();
        let resp = test::call_service(&$app, req).await;
        assert_eq!(resp.status(), 200, "list invitations should return 200");
        let body: Value = test::read_body_json(resp).await;
        body["data"].clone()
    }};
}

fn extract_invitation_token(invitation: &Value) -> String {
    let url = invitation["invitation_url"]
        .as_str()
        .expect("create response should include invitation_url");
    let (_, token) = url
        .rsplit_once("/invite/")
        .expect("invitation URL should contain /invite/");
    token.to_string()
}

async fn latest_invitation_row(
    db: &sea_orm::DatabaseConnection,
    email: &str,
) -> user_invitation::Model {
    user_invitation::Entity::find()
        .filter(user_invitation::Column::Email.eq(email))
        .order_by_desc(user_invitation::Column::Id)
        .one(db)
        .await
        .expect("invitation lookup should succeed")
        .expect("invitation row should exist")
}

async fn count_invitations_by_status(
    db: &sea_orm::DatabaseConnection,
    email: &str,
    status: UserInvitationStatus,
) -> u64 {
    use sea_orm::PaginatorTrait;

    user_invitation::Entity::find()
        .filter(user_invitation::Column::Email.eq(email))
        .filter(user_invitation::Column::Status.eq(status))
        .count(db)
        .await
        .expect("invitation count should succeed")
}

async fn latest_invitation_outbox(db: &sea_orm::DatabaseConnection) -> mail_outbox::Model {
    mail_outbox::Entity::find()
        .filter(mail_outbox::Column::TemplateCode.eq(MailTemplateCode::UserInvitation))
        .order_by_desc(mail_outbox::Column::Id)
        .one(db)
        .await
        .expect("mail outbox lookup should succeed")
        .expect("user invitation outbox row should exist")
}

async fn expire_invitation(db: &sea_orm::DatabaseConnection, id: i64) {
    let mut active = user_invitation::Entity::find_by_id(id)
        .one(db)
        .await
        .expect("invitation lookup should succeed")
        .expect("invitation row should exist")
        .into_active_model();
    active.expires_at = Set(Utc::now() - Duration::minutes(1));
    active
        .update(db)
        .await
        .expect("invitation expiry update should succeed");
}

#[actix_web::test]
async fn test_invitation_lifecycle_accepts_and_marks_user_email_verified() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let runtime_config = state.runtime_config.clone();
    let mail_sender = state.mail_sender.clone();
    state.runtime_config.apply(common::system_config_model(
        site_url::PUBLIC_SITE_URL_KEY,
        r#"["https://drive.example.test"]"#,
    ));
    let app = create_test_app!(state);
    let (admin_token, _) = register_and_login!(app);

    let invitation = create_invitation!(app, admin_token, " Invited@Example.COM ");
    assert_eq!(invitation["email"], "invited@example.com");
    assert_eq!(invitation["status"], "pending");
    assert_eq!(invitation["mail_queued"], true);
    let invitation_url = invitation["invitation_url"].as_str().unwrap();
    assert!(invitation_url.starts_with("https://drive.example.test/invite/"));
    let token = extract_invitation_token(&invitation);

    let list = list_invitations!(app, admin_token);
    let listed = list["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"].as_i64() == invitation["id"].as_i64())
        .expect("new pending invitation should be listed");
    assert!(listed.get("invitation_url").is_none());

    let outbox = latest_invitation_outbox(&db).await;
    assert_eq!(outbox.to_address, "invited@example.com");
    assert_eq!(outbox.template_code, MailTemplateCode::UserInvitation);
    assert!(
        outbox.payload_json.as_ref().contains(&token),
        "stored payload should contain the plaintext token only for mail delivery"
    );
    let pending_row = latest_invitation_row(&db, "invited@example.com").await;
    assert_ne!(pending_row.token_hash, token);

    common::flush_mail_outbox_with(&db, &runtime_config, &mail_sender).await;
    let memory_sender = aster_drive::services::mail::sender::memory_sender_ref(&mail_sender)
        .expect("memory mail sender should be available in tests");
    let message = memory_sender
        .last_message()
        .expect("invitation email should be sent");
    assert_eq!(message.to.address, "invited@example.com");
    assert!(message.text_body.contains(invitation_url));

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/auth/invitations/{}",
            urlencoding::encode(&token)
        ))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["email"], "invited@example.com");

    let (status, body) = accept_invitation_with_status!(app, &token, "invited_user", "password123");
    assert_eq!(status, 201);
    assert_eq!(body["data"]["username"], "invited_user");
    assert_eq!(body["data"]["email"], "invited@example.com");
    assert_eq!(body["data"]["email_verified"], true);

    let row = latest_invitation_row(&db, "invited@example.com").await;
    assert_eq!(row.status, UserInvitationStatus::Accepted);
    assert_eq!(row.accepted_user_id, body["data"]["id"].as_i64());
    assert!(row.accepted_at.is_some());
    assert!(
        row.token_hash.len() == 64 && row.token_hash != token,
        "database should still keep the SHA-256 hash for token lookup"
    );

    let (status, body) =
        accept_invitation_with_status!(app, &token, "invited_user_2", "password123");
    assert_eq!(status, 400);
    assert_eq!(body["code"], "auth.invitation_accepted");
}

#[actix_web::test]
async fn test_invitation_uses_configured_ttl_for_expiry_and_mail_text() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    state.runtime_config.apply(common::system_config_model(
        auth_runtime::AUTH_USER_INVITATION_TTL_SECS_KEY,
        "3600",
    ));
    let app = create_test_app!(state);
    let (admin_token, _) = register_and_login!(app);

    let invitation = create_invitation!(app, admin_token, "ttl@example.com");
    let created_at = DateTime::parse_from_rfc3339(invitation["created_at"].as_str().unwrap())
        .unwrap()
        .with_timezone(&Utc);
    let expires_at = DateTime::parse_from_rfc3339(invitation["expires_at"].as_str().unwrap())
        .unwrap()
        .with_timezone(&Utc);
    assert_eq!(expires_at - created_at, Duration::seconds(3600));

    let outbox = latest_invitation_outbox(&db).await;
    let payload: Value =
        serde_json::from_str(outbox.payload_json.as_ref()).expect("payload should be JSON");
    assert_eq!(payload["expires_in"], "1 hour");
}

#[actix_web::test]
async fn test_duplicate_invitation_revokes_previous_pending_and_only_new_token_works() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let app = create_test_app!(state);
    let (admin_token, _) = register_and_login!(app);

    let first = create_invitation!(app, admin_token, "Duplicate@Example.COM");
    let first_id = first["id"].as_i64().unwrap();
    let first_token = extract_invitation_token(&first);
    let second = create_invitation!(app, admin_token, "duplicate@example.com");
    let second_id = second["id"].as_i64().unwrap();
    let second_token = extract_invitation_token(&second);
    assert_ne!(first_id, second_id);
    assert_ne!(first_token, second_token);
    assert_eq!(first["email"], "duplicate@example.com");
    assert_eq!(second["email"], "duplicate@example.com");

    assert_eq!(
        count_invitations_by_status(&db, "duplicate@example.com", UserInvitationStatus::Revoked)
            .await,
        1
    );
    assert_eq!(
        count_invitations_by_status(&db, "duplicate@example.com", UserInvitationStatus::Pending)
            .await,
        1
    );

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/auth/invitations/{}",
            urlencoding::encode(&first_token)
        ))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "auth.invitation_revoked");

    let (status, body) =
        accept_invitation_with_status!(app, &second_token, "duplicate_user", "password123");
    assert_eq!(status, 201);
    assert_eq!(body["data"]["email"], "duplicate@example.com");
}

#[actix_web::test]
async fn test_invitation_revoke_blocks_accept_and_rejects_non_pending_revoke() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (admin_token, _) = register_and_login!(app);

    let invitation = create_invitation!(app, admin_token, "revoked@example.com");
    let invitation_id = invitation["id"].as_i64().unwrap();
    let token = extract_invitation_token(&invitation);

    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/v1/admin/users/invitations/{invitation_id}/revoke"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["status"], "revoked");
    assert!(body["data"]["revoked_at"].is_string());
    assert!(body["data"].get("invitation_url").is_none());

    let (status, body) = accept_invitation_with_status!(app, &token, "revoked_user", "password123");
    assert_eq!(status, 400);
    assert_eq!(body["code"], "auth.invitation_revoked");

    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/v1/admin/users/invitations/{invitation_id}/revoke"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "auth.invitation_revoked");
}

#[actix_web::test]
async fn test_invitation_create_and_revoke_audit_invitation_entity() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let app = create_test_app!(state);
    let (admin_token, _) = register_and_login!(app);

    let invitation = create_invitation!(app, admin_token, "audit-invite@example.com");
    let invitation_id = invitation["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/v1/admin/users/invitations/{invitation_id}/revoke"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    audit::flush_global_audit_log_manager().await;
    let entries = audit_log::Entity::find()
        .filter(audit_log::Column::Action.is_in([
            AuditAction::AdminCreateInvitation,
            AuditAction::AdminRevokeInvitation,
        ]))
        .order_by_asc(audit_log::Column::Id)
        .all(&db)
        .await
        .expect("audit log query should succeed");

    let create = entries
        .iter()
        .find(|entry| entry.action == AuditAction::AdminCreateInvitation)
        .expect("create invitation audit should be recorded");
    assert_eq!(create.entity_type, "invitation");
    assert_eq!(create.entity_id, Some(invitation_id));
    assert_eq!(
        create.entity_name.as_deref(),
        Some("audit-invite@example.com")
    );

    let revoke = entries
        .iter()
        .find(|entry| entry.action == AuditAction::AdminRevokeInvitation)
        .expect("revoke invitation audit should be recorded");
    assert_eq!(revoke.entity_type, "invitation");
    assert_eq!(revoke.entity_id, Some(invitation_id));
    assert_eq!(
        revoke.entity_name.as_deref(),
        Some("audit-invite@example.com")
    );
}

#[actix_web::test]
async fn test_invitation_expiry_is_refreshed_by_verify_list_and_accept() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let app = create_test_app!(state);
    let (admin_token, _) = register_and_login!(app);

    let invitation = create_invitation!(app, admin_token, "expired@example.com");
    let invitation_id = invitation["id"].as_i64().unwrap();
    let token = extract_invitation_token(&invitation);
    expire_invitation(&db, invitation_id).await;

    let list = list_invitations!(app, admin_token);
    let item = list["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"].as_i64() == Some(invitation_id))
        .expect("expired invitation should be listed");
    assert_eq!(item["status"], "expired");
    assert!(item.get("invitation_url").is_none());

    let row = latest_invitation_row(&db, "expired@example.com").await;
    assert_eq!(row.status, UserInvitationStatus::Pending);

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/auth/invitations/{}",
            urlencoding::encode(&token)
        ))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "auth.invitation_expired");

    let list = list_invitations!(app, admin_token);
    let item = list["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"].as_i64() == Some(invitation_id))
        .expect("expired invitation should be listed");
    assert_eq!(item["status"], "expired");
    assert!(item.get("invitation_url").is_none());

    let row = latest_invitation_row(&db, "expired@example.com").await;
    assert_eq!(row.status, UserInvitationStatus::Expired);

    let (status, body) = accept_invitation_with_status!(app, &token, "expired_user", "password123");
    assert_eq!(status, 400);
    assert_eq!(body["code"], "auth.invitation_expired");
}

#[actix_web::test]
async fn test_invitation_accept_rejects_conflicts_invalid_token_and_bad_credentials() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (admin_token, _) = register_and_login!(app);
    admin_create_user!(
        app,
        admin_token,
        "existinguser",
        "existing@example.com",
        "password123"
    );

    let (status, body) = create_invitation_with_status!(app, admin_token, "existing@example.com");
    assert_eq!(status, 400);
    assert_eq!(body["code"], "auth.email_exists");

    let invitation = create_invitation!(app, admin_token, "conflict@example.com");
    let token = extract_invitation_token(&invitation);

    let (status, body) = accept_invitation_with_status!(app, &token, "existinguser", "password123");
    assert_eq!(status, 400);
    assert_eq!(body["code"], "auth.username_exists");

    let (status, body) = accept_invitation_with_status!(app, &token, "ab", "password123");
    assert_eq!(status, 400);
    assert_eq!(body["code"], "bad_request");

    let (status, body) = accept_invitation_with_status!(app, &token, "valid_new_user", "short");
    assert_eq!(status, 400);
    assert_eq!(body["code"], "bad_request");

    let (status, body) =
        accept_invitation_with_status!(app, "not-a-real-token", "valid_new_user", "password123");
    assert_eq!(status, 400);
    assert_eq!(body["code"], "auth.invitation_invalid");
}

#[actix_web::test]
async fn test_invitations_require_admin_but_accept_when_public_registration_disabled() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    state.runtime_config.apply(common::system_config_model(
        auth_runtime::AUTH_ALLOW_USER_REGISTRATION_KEY,
        "false",
    ));
    admin_create_user!(
        app,
        admin_token,
        "plainuser",
        "plain@example.com",
        "password123"
    );
    let (plain_token, _) = login_user!(app, "plainuser", "password123");

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/users/invitations")
        .insert_header(("Cookie", common::access_cookie_header(&plain_token)))
        .insert_header(common::csrf_header_for(&plain_token))
        .set_json(serde_json::json!({ "email": "blocked-admin@example.com" }))
        .to_request();
    let err = test::try_call_service(&app, req).await.unwrap_err();
    assert_eq!(err.error_response().status(), 403);

    let invitation = create_invitation!(app, admin_token, "closed-registration@example.com");
    let token = extract_invitation_token(&invitation);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/register")
        .set_json(serde_json::json!({
            "username": "public_closed",
            "email": "public-closed@example.com",
            "password": "password123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 403);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "auth.registration_disabled");

    let (status, body) =
        accept_invitation_with_status!(app, &token, "closedreguser", "password123");
    assert_eq!(status, 201);
    assert_eq!(body["data"]["email"], "closed-registration@example.com");
}

#[actix_web::test]
async fn test_invitation_respects_local_email_policy_on_create_and_accept() {
    let state = common::setup().await;
    let db = state.writer_db().clone();
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);

    state.runtime_config.apply(common::system_config_model(
        local_email_policy::AUTH_LOCAL_EMAIL_ALLOWLIST_KEY,
        r#"["allowed.test"]"#,
    ));

    let (status, body) = create_invitation_with_status!(app, admin_token, "blocked@example.com");
    assert_eq!(status, 400);
    assert_eq!(body["code"], "auth.email_not_allowlisted");

    let invitation = create_invitation!(app, admin_token, "policy@allowed.test");
    let token = extract_invitation_token(&invitation);

    state.runtime_config.apply(common::system_config_model(
        local_email_policy::AUTH_LOCAL_EMAIL_BLOCKLIST_KEY,
        r#"["policy@allowed.test"]"#,
    ));

    let (status, body) = accept_invitation_with_status!(app, &token, "policy_user", "password123");
    assert_eq!(status, 400);
    assert_eq!(body["code"], "auth.email_blocked");

    let row = latest_invitation_row(&db, "policy@allowed.test").await;
    assert_eq!(row.status, UserInvitationStatus::Pending);
    assert!(row.accepted_user_id.is_none());
}
