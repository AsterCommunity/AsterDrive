//! 集成测试：`audit`。

#[macro_use]
mod common;

use actix_web::test;
use aster_drive::runtime::SharedRuntimeState;
use aster_drive::{entities::audit_log, services::ops::audit, types::AuditAction};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, Set};
use serde_json::Value;

macro_rules! fetch_audit_items {
    ($app:expr, $token:expr) => {{
        let req = test::TestRequest::get()
            .uri("/api/v1/admin/audit-logs")
            .insert_header(("Cookie", common::access_cookie_header(&$token)))
            .insert_header(common::csrf_header_for(&$token))
            .to_request();
        let resp = test::call_service(&$app, req).await;
        assert_eq!(resp.status(), 200);
        let body: Value = test::read_body_json(resp).await;
        body["data"]["items"]
            .as_array()
            .expect("audit log response should contain items")
            .clone()
    }};
}

fn assert_action_present<'a>(items: &'a [Value], action: &str) -> &'a Value {
    items
        .iter()
        .find(|item| item["action"] == action)
        .unwrap_or_else(|| {
            panic!(
                "audit log should contain {action}, got {:?}",
                items
                    .iter()
                    .map(|item| item["action"].as_str().unwrap_or("<non-string>"))
                    .collect::<Vec<_>>()
            )
        })
}

#[actix_web::test]
async fn test_audit_log_persists_long_entity_type_values() {
    let state = common::setup().await;

    audit::log(
        &state,
        &audit::AuditContext {
            user_id: 42,
            ip_address: None,
            user_agent: None,
        },
        AuditAction::AdminTestExternalAuthProvider,
        audit::AuditEntityType::ExternalAuthProvider,
        None,
        Some("draft"),
        Some(serde_json::json!({
            "provider_kind": "oidc",
            "key": "draft",
            "success": true,
        })),
    )
    .await;
    aster_forge_audit::flush_global_audit_log_manager().await;

    let entry = audit_log::Entity::find()
        .filter(audit_log::Column::Action.eq(AuditAction::AdminTestExternalAuthProvider))
        .one(state.writer_db())
        .await
        .expect("audit log query should succeed")
        .expect("long entity_type audit log should persist");

    assert_eq!(entry.entity_type, "external_auth_provider");
    assert_eq!(entry.entity_name.as_deref(), Some("draft"));
}

#[actix_web::test]
async fn test_audit_log_recorded_on_upload() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    // Upload a file — this triggers a "file_upload" audit log entry
    let _file_id = upload_test_file!(app, token);

    // Admin queries audit logs
    let req = test::TestRequest::get()
        .uri("/api/v1/admin/audit-logs")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "success");

    let items = body["data"]["items"].as_array().unwrap();
    let has_upload = items.iter().any(|item| item["action"] == "file_upload");
    assert!(
        has_upload,
        "audit log should contain a file_upload entry, got: {:?}",
        items.iter().map(|i| &i["action"]).collect::<Vec<_>>()
    );
}

#[actix_web::test]
async fn test_audit_log_recorded_on_file_access_token_creation() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let file_id = upload_test_file_named!(app, token, "token-audit.txt");

    let req = test::TestRequest::get()
        .uri(&format!("/api/v1/files/{file_id}/direct-link"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/files/{file_id}/preview-link"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let items = fetch_audit_items!(app, token);
    let direct_entry = assert_action_present(&items, "file_direct_link_create");
    assert_eq!(direct_entry["entity_type"], "file");
    assert_eq!(direct_entry["entity_id"], file_id);

    let preview_entry = assert_action_present(&items, "file_preview_link_create");
    assert_eq!(preview_entry["entity_type"], "file");
    assert_eq!(preview_entry["entity_id"], file_id);
}

#[actix_web::test]
async fn test_audit_log_recorded_on_admin_create_user() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/users")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "username": "audituser",
            "email": "audituser@example.com",
            "password": "password123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/audit-logs")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let items = body["data"]["items"].as_array().unwrap();
    let entry = items
        .iter()
        .find(|item| item["action"] == "admin_create_user");
    assert!(
        entry.is_some(),
        "audit log should contain admin_create_user"
    );
    let entry = entry.unwrap();
    assert_eq!(entry["entity_type"], "user");
    assert_eq!(entry["entity_name"], "audituser");
    let raw_details = entry["details"].as_str().unwrap();
    assert!(
        !raw_details.contains("\"password\""),
        "audit details must not include password as a raw field"
    );
    assert!(
        !raw_details.contains("\"generated_password\""),
        "audit details must not include generated_password as a raw field"
    );
    assert!(
        !raw_details.contains("password123"),
        "audit details must not include password plaintext"
    );
    let details: Value = serde_json::from_str(raw_details).unwrap();
    assert_eq!(details["temporary_password_generated"], false);
    assert!(details.get("password").is_none());
    assert!(details.get("generated_password").is_none());
}

#[actix_web::test]
async fn test_audit_log_redacts_generated_admin_create_user_password() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/users")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "username": "generatedaudit",
            "email": "generatedaudit@example.com"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let generated_password = body["data"]["generated_password"]
        .as_str()
        .expect("generated password should be returned once")
        .to_string();

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/audit-logs")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let items = body["data"]["items"].as_array().unwrap();
    let entry = items
        .iter()
        .find(|item| {
            item["action"] == "admin_create_user" && item["entity_name"] == "generatedaudit"
        })
        .expect("audit log should contain generated admin_create_user");
    let raw_details = entry["details"].as_str().unwrap();
    assert!(
        !raw_details.contains(&generated_password),
        "audit details must not include generated password plaintext"
    );
    assert!(
        !raw_details.contains("\"password\""),
        "audit details must not include password as a raw field"
    );
    assert!(
        !raw_details.contains("\"generated_password\""),
        "audit details must not include generated_password as a raw field"
    );
    let details: Value = serde_json::from_str(raw_details).unwrap();
    assert_eq!(details["temporary_password_generated"], true);
    assert_eq!(details["must_change_password"], true);
    assert!(details.get("password").is_none());
    assert!(details.get("generated_password").is_none());
}

#[actix_web::test]
async fn test_audit_log_recorded_on_admin_task_cleanup() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/tasks/cleanup")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "finished_before": chrono::Utc::now().to_rfc3339()
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let items = fetch_audit_items!(app, token);
    let cleanup_entry = assert_action_present(&items, "admin_cleanup_tasks");
    assert_eq!(cleanup_entry["entity_type"], "task");
    let details: Value = serde_json::from_str(cleanup_entry["details"].as_str().unwrap()).unwrap();
    assert_eq!(details["removed"], 0);
    assert!(details["finished_before"].is_string());
}

#[actix_web::test]
async fn test_audit_cleanup_retention_zero_keeps_logs() {
    let state = common::setup().await;
    state
        .runtime_config
        .apply(common::system_config_model("audit_log_retention_days", "0"));

    let old_created_at = chrono::Utc::now() - chrono::Duration::days(365);
    audit_log::ActiveModel {
        id: Default::default(),
        user_id: Set(0),
        action: Set(AuditAction::FileUpload),
        entity_type: Set("file".to_string()),
        entity_id: Set(Some(1)),
        entity_name: Set(Some("retained-audit-entry.txt".to_string())),
        details: Set(None),
        ip_address: Set(None),
        user_agent: Set(None),
        created_at: Set(old_created_at),
    }
    .insert(state.writer_db())
    .await
    .unwrap();

    let deleted = audit::cleanup_expired(&state).await.unwrap();
    assert_eq!(deleted, 0);

    let remaining = audit_log::Entity::find()
        .filter(audit_log::Column::Action.eq(AuditAction::FileUpload))
        .count(state.writer_db())
        .await
        .unwrap();
    assert_eq!(remaining, 1);
}

#[actix_web::test]
async fn test_audit_log_pagination_fields_and_offset() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    for _ in 0..3 {
        let _file_id = upload_test_file!(app, token);
    }

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/audit-logs?limit=1&offset=1")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;

    assert_eq!(body["data"]["limit"], 1);
    assert_eq!(body["data"]["offset"], 1);
    assert!(body["data"]["total"].as_u64().unwrap() >= 3);
    assert_eq!(body["data"]["items"].as_array().unwrap().len(), 1);
}

#[actix_web::test]
async fn test_audit_log_limit_is_clamped() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let _file_id = upload_test_file!(app, token);

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/audit-logs?limit=9999")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;

    assert_eq!(body["data"]["limit"], 200);
    assert_eq!(body["data"]["offset"], 0);
}

#[actix_web::test]
async fn test_audit_log_admin_only() {
    let state = common::setup().await;
    let app = create_test_app!(state);

    let (admin_token, _) = register_and_login!(app);
    admin_create_user!(
        app,
        admin_token,
        "user2",
        "user2@example.com",
        "password123"
    );
    let (token2, _) = login_user!(app, "user2", "password123");

    // Non-admin tries to access audit logs — should get 403
    let req = test::TestRequest::get()
        .uri("/api/v1/admin/audit-logs")
        .insert_header(("Cookie", common::access_cookie_header(&token2)))
        .insert_header(common::csrf_header_for(&token2))
        .to_request();
    let err = test::try_call_service(&app, req).await.unwrap_err();
    let resp = err.error_response();
    assert_eq!(resp.status(), 403);
}

#[actix_web::test]
async fn test_audit_log_recorded_on_setup_register_and_login_after_refactor() {
    let state = common::setup().await;
    let app = create_test_app!(state);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/setup")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "username": "setupadmin",
            "email": "setupadmin@example.com",
            "password": "password123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/login")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "identifier": "setupadmin",
            "password": "password123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let token = common::extract_cookie(&resp, "aster_access").unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/auth/register")
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .set_json(serde_json::json!({
            "username": "member1",
            "email": "member1@example.com",
            "password": "password123"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);

    let items = fetch_audit_items!(app, token);

    let setup_entry = assert_action_present(&items, "system_setup");
    assert_eq!(setup_entry["entity_name"], "setupadmin");

    let login_entry = assert_action_present(&items, "user_login");
    assert_eq!(login_entry["entity_name"], "setupadmin");

    let register_entry = assert_action_present(&items, "user_register");
    assert_eq!(register_entry["entity_name"], "member1");
}

#[actix_web::test]
async fn test_audit_log_recorded_on_file_and_folder_patch_variants_after_refactor() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Source Folder" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let source_folder_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Target Folder" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let target_folder_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/folders/{source_folder_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Renamed Folder" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/folders/{source_folder_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "parent_id": target_folder_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let file_id = upload_test_file_named!(app, token, "audit-file.txt");

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/files/{file_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "renamed-file.txt" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/files/{file_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "folder_id": target_folder_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let items = fetch_audit_items!(app, token);

    assert_eq!(
        assert_action_present(&items, "folder_rename")["entity_type"],
        "folder"
    );
    assert_eq!(
        assert_action_present(&items, "folder_move")["entity_type"],
        "folder"
    );
    assert_eq!(
        assert_action_present(&items, "file_rename")["entity_type"],
        "file"
    );
    assert_eq!(
        assert_action_present(&items, "file_move")["entity_type"],
        "file"
    );
}

#[actix_web::test]
async fn test_audit_log_records_admin_folder_policy_change_details() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Policy Audit Folder" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let folder_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/policies")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Folder Policy Audit Target",
            "driver_type": "local",
            "base_path": format!("/tmp/asterdrive-folder-policy-audit-{}", uuid::Uuid::new_v4()),
            "max_file_size": 0,
            "is_default": false
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let policy_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::put()
        .uri(&format!("/api/v1/admin/folders/{folder_id}/policy"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "policy_id": policy_id }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let items = fetch_audit_items!(app, token);
    let entry = assert_action_present(&items, "folder_policy_change");
    assert_eq!(entry["entity_type"], "folder");
    assert_eq!(entry["entity_id"], folder_id);
    assert_eq!(entry["entity_name"], "Policy Audit Folder");

    let details: Value = serde_json::from_str(entry["details"].as_str().unwrap()).unwrap();
    assert!(details["previous_policy_id"].is_null());
    assert_eq!(details["policy_id"], policy_id);
}

#[actix_web::test]
async fn test_audit_log_recorded_on_batch_actions_after_refactor() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Batch Target" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let target_folder_id = body["data"]["id"].as_i64().unwrap();

    let file_to_copy = upload_test_file_named!(app, token, "copy-me.txt");
    let file_to_copy_again = upload_test_file_named!(app, token, "copy-me-again.txt");
    let file_to_move = upload_test_file_named!(app, token, "move-me.txt");
    let file_to_move_again = upload_test_file_named!(app, token, "move-me-again.txt");
    let file_to_delete = upload_test_file_named!(app, token, "delete-me.txt");

    let req = test::TestRequest::post()
        .uri("/api/v1/batch/copy")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "file_ids": [file_to_copy, file_to_copy_again],
            "folder_ids": [],
            "target_folder_id": target_folder_id
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::post()
        .uri("/api/v1/batch/move")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "file_ids": [file_to_move, file_to_move_again],
            "folder_ids": [],
            "target_folder_id": target_folder_id
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::post()
        .uri("/api/v1/batch/delete")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "file_ids": [file_to_delete],
            "folder_ids": []
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let items = fetch_audit_items!(app, token);
    assert_action_present(&items, "batch_copy");
    assert_action_present(&items, "batch_move");
    assert_action_present(&items, "batch_delete");
}

#[actix_web::test]
async fn test_audit_log_records_workspace_transfer_copy_scopes() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let file_id = upload_test_file_named!(app, token, "workspace-transfer-audit.txt");

    let req = test::TestRequest::post()
        .uri("/api/v1/teams")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "name": "Workspace Transfer Audit Team" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let team_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::post()
        .uri("/api/v1/workspace-transfer/copy")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "source_workspace": { "kind": "personal" },
            "file_ids": [file_id],
            "folder_ids": [],
            "destination_workspace": { "kind": "team", "team_id": team_id },
            "target_folder_id": null
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let items = fetch_audit_items!(app, token);
    let entry = assert_action_present(&items, "batch_copy");
    let details: Value = serde_json::from_str(entry["details"].as_str().unwrap()).unwrap();
    assert_eq!(details["source_workspace"]["kind"], "personal");
    assert!(details["source_workspace"].get("team_id").is_none());
    assert_eq!(details["destination_workspace"]["kind"], "team");
    assert_eq!(details["destination_workspace"]["team_id"], team_id);
    assert_eq!(details["file_ids"], serde_json::json!([file_id]));
    assert_eq!(details["folder_ids"], serde_json::json!([]));
    assert!(details["target_folder_id"].is_null());
    assert_eq!(details["succeeded"], 1);
    assert_eq!(details["failed"], 0);
}

#[actix_web::test]
async fn test_audit_log_recorded_on_share_config_and_admin_user_actions_after_refactor() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let file_id = upload_test_file!(app, token);

    let req = test::TestRequest::post()
        .uri("/api/v1/shares")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "target": { "type": "file", "id": file_id }
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let share_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/shares/{share_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "expires_at": common::TEST_FUTURE_SHARE_EXPIRY_RFC3339,
            "max_downloads": 3
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let status = resp.status();
    if status != 200 {
        let body = test::read_body(resp).await;
        panic!(
            "share update returned {status}: {}",
            String::from_utf8_lossy(&body)
        );
    }

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/shares/{share_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::put()
        .uri("/api/v1/admin/config/max_versions_per_file")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({ "value": "25" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let managed_user_id = admin_create_user!(
        app,
        token,
        "managed-user",
        "managed-user@example.com",
        "password123"
    );

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/admin/users/{managed_user_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "status": "disabled",
            "storage_quota": 1024
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let items = fetch_audit_items!(app, token);

    assert_action_present(&items, "share_create");
    assert_action_present(&items, "share_update");
    assert_action_present(&items, "share_delete");

    let config_entry = assert_action_present(&items, "config_update");
    assert_eq!(config_entry["entity_name"], "max_versions_per_file");

    let create_entry = assert_action_present(&items, "admin_create_user");
    assert_eq!(create_entry["entity_type"], "user");
    assert_eq!(create_entry["entity_name"], "managed-user");

    let update_entry = assert_action_present(&items, "admin_update_user");
    assert_eq!(update_entry["entity_type"], "user");
    assert_eq!(update_entry["entity_name"], "managed-user");
}

#[actix_web::test]
async fn test_audit_log_recorded_on_admin_force_delete_user() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let managed_user_id = admin_create_user!(
        app,
        token,
        "forcedeleteuser",
        "forcedeleteuser@example.com",
        "password123"
    );

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/admin/users/{managed_user_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let items = fetch_audit_items!(app, token);
    let delete_entry = assert_action_present(&items, "admin_force_delete_user");
    assert_eq!(delete_entry["entity_type"], "user");
    assert_eq!(delete_entry["entity_name"], "forcedeleteuser");
    let details = delete_entry["details"]
        .as_str()
        .expect("force delete audit details should be serialized JSON");
    assert!(details.contains("\"file_count\":0"));
    assert!(details.contains("\"folder_count\":0"));
    assert!(details.contains("\"share_count\":0"));
}

#[actix_web::test]
async fn test_audit_log_recorded_on_admin_team_lifecycle() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let team_admin_user_id = admin_create_user!(
        app,
        token,
        "team-admin-user",
        "team-admin@example.com",
        "password123"
    );

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/teams")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Admin Audit Team",
            "description": "created by admin route",
            "admin_user_id": team_admin_user_id
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let team_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/admin/teams/{team_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Admin Audit Team Updated",
            "description": "updated by admin route"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/admin/teams/{team_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/admin/teams/{team_id}/restore"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let items = fetch_audit_items!(app, token);

    let create_entry = assert_action_present(&items, "admin_create_team");
    assert_eq!(create_entry["entity_type"], "team");
    assert_eq!(create_entry["entity_name"], "Admin Audit Team");

    let update_entry = assert_action_present(&items, "admin_update_team");
    assert_eq!(update_entry["entity_type"], "team");
    assert_eq!(update_entry["entity_name"], "Admin Audit Team Updated");

    let archive_entry = assert_action_present(&items, "admin_archive_team");
    assert_eq!(archive_entry["entity_type"], "team");
    assert_eq!(archive_entry["entity_name"], "Admin Audit Team Updated");

    let restore_entry = assert_action_present(&items, "admin_restore_team");
    assert_eq!(restore_entry["entity_type"], "team");
    assert_eq!(restore_entry["entity_name"], "Admin Audit Team Updated");
}

#[actix_web::test]
async fn test_audit_log_recorded_on_config_action_execute() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/config/mail/action")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "action": "send_test_email",
            "target_email": "audit-mail@example.com"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let items = fetch_audit_items!(app, token);
    let entry = assert_action_present(&items, "config_action_execute");
    assert_eq!(entry["entity_name"], "mail");
    assert!(
        entry["details"]
            .as_str()
            .unwrap_or_default()
            .contains("\"target_email\":\"audit-mail@example.com\"")
    );
}

#[actix_web::test]
async fn test_audit_log_recorded_on_team_lifecycle() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/teams")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Member Audit Team",
            "description": "created from team route"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let team_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::patch()
        .uri(&format!("/api/v1/teams/{team_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Member Audit Team Updated",
            "description": "updated from team route"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/teams/{team_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::post()
        .uri(&format!("/api/v1/teams/{team_id}/restore"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let items = fetch_audit_items!(app, token);

    let create_entry = assert_action_present(&items, "team_create");
    assert_eq!(create_entry["entity_type"], "team");
    assert_eq!(create_entry["entity_name"], "Member Audit Team");

    let update_entry = assert_action_present(&items, "team_update");
    assert_eq!(update_entry["entity_type"], "team");
    assert_eq!(update_entry["entity_name"], "Member Audit Team Updated");

    let archive_entry = assert_action_present(&items, "team_archive");
    assert_eq!(archive_entry["entity_type"], "team");
    assert_eq!(archive_entry["entity_name"], "Member Audit Team Updated");

    let restore_entry = assert_action_present(&items, "team_restore");
    assert_eq!(restore_entry["entity_type"], "team");
    assert_eq!(restore_entry["entity_name"], "Member Audit Team Updated");
}

#[actix_web::test]
async fn test_audit_log_recorded_on_team_archive_cleanup() {
    use actix_web::{App, web};
    use chrono::{Duration, Utc};
    use sea_orm::{IntoActiveModel, Set};

    let state = common::setup().await;
    let db = state.writer_db().clone();
    let state = web::Data::new(state);
    let app = test::init_service(
        App::new()
            .app_data(web::PayloadConfig::new(10 * 1024 * 1024))
            .app_data(web::JsonConfig::default().limit(1024 * 1024))
            .app_data(web::Data::clone(&state))
            .configure(move |cfg| aster_drive::api::configure_primary(cfg, &db)),
    )
    .await;
    let (token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/teams")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Cleanup Audit Team",
            "description": "cleanup target"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let team_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/teams/{team_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let mut archived_team =
        aster_drive::db::repository::team_repo::find_by_id(state.writer_db(), team_id)
            .await
            .unwrap()
            .into_active_model();
    let archived_at = Utc::now() - Duration::days(10);
    archived_team.archived_at = Set(Some(archived_at));
    archived_team.updated_at = Set(archived_at);
    aster_drive::db::repository::team_repo::update(state.writer_db(), archived_team)
        .await
        .unwrap();

    let deleted = aster_drive::services::workspace::team::cleanup_expired_archived_teams(&state)
        .await
        .unwrap();
    assert_eq!(deleted, 1);

    let items = fetch_audit_items!(app, token);
    let cleanup_entry = assert_action_present(&items, "team_cleanup_expired");
    assert_eq!(cleanup_entry["entity_type"], "team");
    assert_eq!(cleanup_entry["entity_name"], "Cleanup Audit Team");
    assert!(cleanup_entry["user"].is_null());

    let details: Value = serde_json::from_str(cleanup_entry["details"].as_str().unwrap()).unwrap();
    assert_eq!(details["retention_days"], 7);
    assert!(details["archived_at"].is_string());
}
