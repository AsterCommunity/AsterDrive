//! 集成测试：`oidc`。

#[macro_use]
mod common;

mod external_auth;

use actix_web::test;
use aster_drive::api::api_error_code::ApiErrorCode;
use aster_drive::db::repository::{
    external_auth_identity_repo, external_auth_login_flow_repo, external_auth_provider_repo,
};
use aster_drive::entities::{
    audit_log, external_auth_email_verification_flow, external_auth_identity,
    external_auth_login_flow, external_auth_provider, user,
};
use aster_drive::runtime::SharedRuntimeState;
use aster_drive::services::{audit_service, auth::external};
use aster_drive::types::AuditAction;
use chrono::{Duration, Utc};
use external_auth::oidc::*;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, IntoActiveModel, QueryFilter,
    QueryOrder,
};
use serde_json::Value;
use uuid::Uuid;

#[actix_web::test]
async fn admin_provider_api_masks_secret_and_public_list_only_shows_enabled() {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (admin_token, _) = register_and_login!(app);

    let created =
        create_external_auth_provider(&app, &admin_token, &mock_provider.issuer, true, false).await;
    let provider_key = created_provider_key(&created);
    assert!(Uuid::parse_str(&provider_key).is_ok());
    assert_eq!(created["data"]["client_secret"], "***REDACTED***");
    assert_eq!(created["data"]["client_secret_configured"], true);
    assert_eq!(
        created["data"]["icon_url"],
        "/static/external-auth/mock.svg"
    );

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/external-auth/providers")
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let list_body: Value = test::read_body_json(resp).await;
    assert_eq!(list_body["data"]["total"], 1);
    assert_eq!(
        list_body["data"]["items"][0]["client_secret"],
        "***REDACTED***"
    );

    let req = test::TestRequest::get()
        .uri("/api/v1/auth/external-auth/providers")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let public_body: Value = test::read_body_json(resp).await;
    assert_eq!(public_body["data"][0]["key"], provider_key);
    assert_eq!(
        public_body["data"][0]["icon_url"],
        "/static/external-auth/mock.svg"
    );
    assert!(public_body["data"][0].get("client_secret").is_none());

    let provider_id = created["data"]["id"]
        .as_i64()
        .expect("provider id should be returned");
    let req = test::TestRequest::patch()
        .uri(&format!(
            "/api/v1/admin/external-auth/providers/{provider_id}"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({ "enabled": false }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri("/api/v1/auth/external-auth/providers")
        .to_request();
    let resp = test::call_service(&app, req).await;
    let public_body: Value = test::read_body_json(resp).await;
    assert_eq!(public_body["data"].as_array().unwrap().len(), 0);

    server.stop(true).await;
}

#[actix_web::test]
async fn admin_provider_kind_api_drives_create_contract() {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (admin_token, _) = register_and_login!(app);

    let req = test::TestRequest::get()
        .uri("/api/v1/admin/external-auth/provider-kinds")
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let kinds = body["data"]
        .as_array()
        .expect("provider kind list should be an array");
    assert_eq!(kinds.len(), 6);
    let oidc = kinds
        .iter()
        .find(|kind| kind["kind"] == "oidc")
        .expect("OIDC kind should be listed");
    assert_eq!(oidc["protocol"], "oidc");
    assert_eq!(oidc["default_scopes"], "openid email profile");
    assert_eq!(oidc["supports_discovery"], true);
    assert_eq!(oidc["supports_pkce"], true);

    let generic_oauth2 = kinds
        .iter()
        .find(|kind| kind["kind"] == "generic_oauth2")
        .expect("Generic OAuth2 kind should be listed");
    assert_eq!(generic_oauth2["protocol"], "oauth2");
    assert_eq!(generic_oauth2["issuer_url_required"], false);
    assert_eq!(
        generic_oauth2["manual_endpoint_configuration_supported"],
        true
    );
    assert_eq!(generic_oauth2["authorization_url_required"], true);
    assert_eq!(generic_oauth2["token_url_required"], true);
    assert_eq!(generic_oauth2["userinfo_url_required"], true);
    assert_eq!(generic_oauth2["supports_discovery"], false);

    let github = kinds
        .iter()
        .find(|kind| kind["kind"] == "github")
        .expect("GitHub kind should be listed");
    assert_eq!(github["protocol"], "oauth2");
    assert_eq!(github["default_scopes"], "read:user user:email");
    assert_eq!(github["issuer_url_required"], false);
    assert_eq!(github["manual_endpoint_configuration_supported"], false);
    assert_eq!(github["authorization_url_required"], false);
    assert_eq!(github["token_url_required"], false);
    assert_eq!(github["userinfo_url_required"], false);
    assert_eq!(github["supports_discovery"], false);
    assert_eq!(github["supports_pkce"], true);
    assert_eq!(github["supports_email_verified_claim"], false);

    let google = kinds
        .iter()
        .find(|kind| kind["kind"] == "google")
        .expect("Google kind should be listed");
    assert_eq!(google["protocol"], "oidc");
    assert_eq!(google["default_scopes"], "openid profile email");
    assert_eq!(google["issuer_url_required"], false);
    assert_eq!(google["manual_endpoint_configuration_supported"], false);
    assert_eq!(google["authorization_url_required"], false);
    assert_eq!(google["token_url_required"], false);
    assert_eq!(google["userinfo_url_required"], false);
    assert_eq!(google["supports_discovery"], true);
    assert_eq!(google["supports_pkce"], true);
    assert_eq!(google["supports_email_verified_claim"], true);

    let microsoft = kinds
        .iter()
        .find(|kind| kind["kind"] == "microsoft")
        .expect("Microsoft kind should be listed");
    assert_eq!(microsoft["protocol"], "oidc");
    assert_eq!(microsoft["default_scopes"], "openid profile email");
    assert_eq!(microsoft["issuer_url_required"], false);
    assert_eq!(microsoft["manual_endpoint_configuration_supported"], false);
    assert_eq!(microsoft["authorization_url_required"], false);
    assert_eq!(microsoft["token_url_required"], false);
    assert_eq!(microsoft["userinfo_url_required"], false);
    assert_eq!(microsoft["supports_discovery"], true);
    assert_eq!(microsoft["supports_pkce"], true);
    assert_eq!(microsoft["supports_email_verified_claim"], false);

    let qq = kinds
        .iter()
        .find(|kind| kind["kind"] == "qq")
        .expect("QQ kind should be listed");
    assert_eq!(qq["protocol"], "oauth2");
    assert_eq!(qq["default_scopes"], "get_user_info");
    assert_eq!(qq["issuer_url_required"], false);
    assert_eq!(qq["manual_endpoint_configuration_supported"], false);
    assert_eq!(qq["authorization_url_required"], false);
    assert_eq!(qq["token_url_required"], false);
    assert_eq!(qq["userinfo_url_required"], false);
    assert_eq!(qq["supports_discovery"], false);
    assert_eq!(qq["supports_pkce"], true);
    assert_eq!(qq["supports_email_verified_claim"], false);

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/external-auth/providers")
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({
            "provider_kind": "oidc",
            "display_name": "Default Enabled",
            "issuer_url": mock_provider.issuer,
            "client_id": TEST_CLIENT_ID,
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let created: Value = test::read_body_json(resp).await;
    assert_eq!(created["data"]["provider_kind"], "oidc");
    assert_eq!(created["data"]["protocol"], "oidc");
    assert_eq!(created["data"]["enabled"], true);
    assert!(Uuid::parse_str(created["data"]["key"].as_str().unwrap()).is_ok());

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/external-auth/providers")
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({
            "display_name": "Missing Kind",
            "issuer_url": mock_provider.issuer,
            "client_id": TEST_CLIENT_ID,
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);

    server.stop(true).await;
}

#[actix_web::test]
async fn admin_update_external_auth_provider_rejects_identity_fields() {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (admin_token, _) = register_and_login!(app);

    let created =
        create_external_auth_provider(&app, &admin_token, &mock_provider.issuer, true, false).await;
    let provider_id = created["data"]["id"]
        .as_i64()
        .expect("provider id should be returned");

    for payload in [
        serde_json::json!({
            "display_name": "Changed Provider Kind",
            "provider_kind": "github",
        }),
        serde_json::json!({
            "display_name": "Changed Key",
            "key": "github",
        }),
    ] {
        let req = test::TestRequest::patch()
            .uri(&format!(
                "/api/v1/admin/external-auth/providers/{provider_id}"
            ))
            .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
            .insert_header(common::csrf_header_for(&admin_token))
            .set_json(payload)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 400);
    }

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/admin/external-auth/providers/{provider_id}"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let provider: Value = test::read_body_json(resp).await;
    assert_eq!(provider["data"]["provider_kind"], "oidc");
    assert_eq!(provider["data"]["key"], created["data"]["key"]);
    assert_eq!(
        provider["data"]["display_name"],
        created["data"]["display_name"]
    );

    server.stop(true).await;
}

#[actix_web::test]
async fn admin_create_and_test_google_provider_uses_oidc_defaults() {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (admin_token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/external-auth/providers")
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({
            "provider_kind": "google",
            "display_name": "Google",
            "authorization_url": "https://accounts.google.com/o/oauth2/v2/auth",
            "client_id": TEST_CLIENT_ID,
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/external-auth/providers")
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({
            "provider_kind": "google",
            "display_name": "Google",
            "client_id": TEST_CLIENT_ID,
            "client_secret": "super-secret",
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["provider_kind"], "google");
    assert_eq!(body["data"]["protocol"], "oidc");
    assert_eq!(body["data"]["issuer_url"], Value::Null);
    assert_eq!(body["data"]["options"], serde_json::json!({}));
    assert_eq!(body["data"]["authorization_url"], Value::Null);
    assert_eq!(body["data"]["token_url"], Value::Null);
    assert_eq!(body["data"]["userinfo_url"], Value::Null);
    assert_eq!(body["data"]["scopes"], "openid profile email");

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/external-auth/providers/test")
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({
            "provider_kind": "google",
            "issuer_url": mock_provider.issuer,
            "client_id": TEST_CLIENT_ID,
            "client_secret": "super-secret",
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["provider"], "Google");
    assert_eq!(body["data"]["issuer"], mock_provider.issuer);
    assert_eq!(
        body["data"]["authorization_endpoint"],
        format!("{}/authorize", mock_provider.issuer)
    );
    assert_eq!(
        body["data"]["token_endpoint"],
        format!("{}/token", mock_provider.issuer)
    );
    assert_eq!(body["data"]["jwks_key_count"], 1);
    assert_eq!(body["data"]["checks"][0]["name"], "discovery");
    assert_eq!(body["data"]["checks"][1]["name"], "jwks");

    server.stop(true).await;
}

#[actix_web::test]
async fn admin_create_and_test_microsoft_provider_uses_oidc_defaults() {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (admin_token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/external-auth/providers")
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({
            "provider_kind": "microsoft",
            "display_name": "Microsoft",
            "options": { "microsoft": { "tenant": "organizations" } },
            "authorization_url": "https://login.microsoftonline.com/common/oauth2/v2.0/authorize",
            "client_id": TEST_CLIENT_ID,
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/external-auth/providers")
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({
            "provider_kind": "microsoft",
            "display_name": "Microsoft",
            "options": { "microsoft": { "tenant": "organizations" } },
            "client_id": TEST_CLIENT_ID,
            "client_secret": "super-secret",
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["provider_kind"], "microsoft");
    assert_eq!(body["data"]["protocol"], "oidc");
    assert_eq!(body["data"]["issuer_url"], Value::Null);
    assert_eq!(
        body["data"]["options"]["microsoft"]["tenant"],
        "organizations"
    );
    assert_eq!(body["data"]["authorization_url"], Value::Null);
    assert_eq!(body["data"]["token_url"], Value::Null);
    assert_eq!(body["data"]["userinfo_url"], Value::Null);
    assert_eq!(body["data"]["scopes"], "openid profile email");
    assert_eq!(body["data"]["require_email_verified"], false);

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/external-auth/providers/test")
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({
            "provider_kind": "microsoft",
            "issuer_url": mock_provider.issuer,
            "client_id": TEST_CLIENT_ID,
            "client_secret": "super-secret",
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["provider"], "Microsoft");
    assert_eq!(body["data"]["issuer"], mock_provider.issuer);
    assert_eq!(
        body["data"]["authorization_endpoint"],
        format!("{}/authorize", mock_provider.issuer)
    );
    assert_eq!(
        body["data"]["token_endpoint"],
        format!("{}/token", mock_provider.issuer)
    );
    assert_eq!(body["data"]["jwks_key_count"], 1);
    assert_eq!(body["data"]["checks"][0]["name"], "discovery");
    assert_eq!(body["data"]["checks"][1]["name"], "jwks");

    server.stop(true).await;
}

#[actix_web::test]
async fn admin_specialized_providers_reject_configurable_connection_urls() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (admin_token, _) = register_and_login!(app);

    let url_fields = [
        ("issuer_url", "https://idp.example.com"),
        ("authorization_url", "https://idp.example.com/authorize"),
        ("token_url", "https://idp.example.com/token"),
        ("userinfo_url", "https://idp.example.com/userinfo"),
    ];

    for provider_kind in ["github", "google", "microsoft", "qq"] {
        for (field, value) in url_fields {
            let mut payload = serde_json::json!({
                "provider_kind": provider_kind,
                "display_name": provider_kind,
                "client_id": TEST_CLIENT_ID,
                "client_secret": "super-secret",
            });
            payload
                .as_object_mut()
                .unwrap()
                .insert(field.to_string(), Value::String(value.to_string()));
            let req = test::TestRequest::post()
                .uri("/api/v1/admin/external-auth/providers")
                .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
                .insert_header(common::csrf_header_for(&admin_token))
                .set_json(payload)
                .to_request();
            let resp = test::call_service(&app, req).await;
            let status = resp.status();
            let body: Value = test::read_body_json(resp).await;
            assert_eq!(status, 400, "{provider_kind} create {field}: {body:#?}");
            assert!(
                body["msg"].as_str().unwrap().contains(field),
                "{provider_kind} create {field}: {body:#?}"
            );
        }
    }

    for provider_kind in ["github", "google", "microsoft", "qq"] {
        let req = test::TestRequest::post()
            .uri("/api/v1/admin/external-auth/providers")
            .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
            .insert_header(common::csrf_header_for(&admin_token))
            .set_json(serde_json::json!({
                "provider_kind": provider_kind,
                "display_name": provider_kind,
                "client_id": TEST_CLIENT_ID,
                "client_secret": "super-secret",
            }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201, "{provider_kind}");
        let body: Value = test::read_body_json(resp).await;
        let provider_id = body["data"]["id"].as_i64().unwrap();

        for (field, value) in url_fields {
            let mut payload = serde_json::Map::new();
            payload.insert(field.to_string(), Value::String(value.to_string()));
            let req = test::TestRequest::patch()
                .uri(&format!(
                    "/api/v1/admin/external-auth/providers/{provider_id}"
                ))
                .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
                .insert_header(common::csrf_header_for(&admin_token))
                .set_json(Value::Object(payload))
                .to_request();
            let resp = test::call_service(&app, req).await;
            let status = resp.status();
            let body: Value = test::read_body_json(resp).await;
            assert_eq!(status, 400, "{provider_kind} update {field}: {body:#?}");
            assert!(
                body["msg"].as_str().unwrap().contains(field),
                "{provider_kind} update {field}: {body:#?}"
            );
        }
    }
}

#[actix_web::test]
async fn admin_microsoft_provider_rejects_issuer_url_configuration() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (admin_token, _) = register_and_login!(app);

    for issuer_url in [
        "organizations",
        "https://login.microsoftonline.com/organizations/v2.0",
    ] {
        let req = test::TestRequest::post()
            .uri("/api/v1/admin/external-auth/providers")
            .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
            .insert_header(common::csrf_header_for(&admin_token))
            .set_json(serde_json::json!({
                "provider_kind": "microsoft",
                "display_name": "Microsoft",
                "issuer_url": issuer_url,
                "client_id": TEST_CLIENT_ID,
                "client_secret": "super-secret",
            }))
            .to_request();
        let resp = test::call_service(&app, req).await;
        let status = resp.status();
        let body: Value = test::read_body_json(resp).await;
        assert_eq!(status, 400, "{body:#?}");
        assert!(body["msg"].as_str().unwrap().contains("issuer_url"));
    }

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/external-auth/providers")
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({
            "provider_kind": "microsoft",
            "display_name": "Microsoft",
            "options": { "microsoft": { "tenant": "organizations" } },
            "client_id": TEST_CLIENT_ID,
            "client_secret": "super-secret",
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    let provider_id = body["data"]["id"].as_i64().unwrap();

    let req = test::TestRequest::patch()
        .uri(&format!(
            "/api/v1/admin/external-auth/providers/{provider_id}"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({
            "issuer_url": "consumers",
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let status = resp.status();
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(status, 400, "{body:#?}");
    assert!(body["msg"].as_str().unwrap().contains("issuer_url"));
}

#[actix_web::test]
async fn admin_microsoft_legacy_issuer_preserves_unparseable_values() {
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);

    let valid_legacy = microsoft_external_auth_provider_model(
        "microsoft-legacy-valid",
        "https://login.microsoftonline.com/Organizations/v2.0/",
        true,
    )
    .insert(state.writer_db())
    .await
    .expect("valid legacy Microsoft provider should insert");
    let invalid_legacy = microsoft_external_auth_provider_model(
        "microsoft-legacy-invalid",
        "https://idp.example.com/organizations/v2.0",
        true,
    )
    .insert(state.writer_db())
    .await
    .expect("invalid legacy Microsoft provider should insert");

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/admin/external-auth/providers/{}",
            valid_legacy.id
        ))
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["issuer_url"], Value::Null);
    assert_eq!(
        body["data"]["options"]["microsoft"]["tenant"],
        "organizations"
    );

    let req = test::TestRequest::get()
        .uri(&format!(
            "/api/v1/admin/external-auth/providers/{}",
            invalid_legacy.id
        ))
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["data"]["issuer_url"],
        "https://idp.example.com/organizations/v2.0"
    );
    assert_eq!(body["data"]["options"], serde_json::json!({}));
}

#[actix_web::test]
async fn admin_tests_external_auth_provider_draft_params_without_persisting() {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    let state = common::setup().await;
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/external-auth/providers/test")
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({
            "provider_kind": "oidc",
            "issuer_url": mock_provider.issuer,
            "client_id": TEST_CLIENT_ID,
            "client_secret": "super-secret",
            "scopes": "openid email profile",
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["provider"], "OpenID Connect");
    assert_eq!(body["data"]["issuer"], mock_provider.issuer);
    assert_eq!(
        body["data"]["authorization_endpoint"],
        format!("{}/authorize", mock_provider.issuer)
    );
    assert_eq!(
        body["data"]["token_endpoint"],
        format!("{}/token", mock_provider.issuer)
    );
    assert_eq!(body["data"]["jwks_key_count"], 1);
    assert_eq!(body["data"]["checks"][0]["name"], "discovery");
    assert_eq!(body["data"]["checks"][1]["name"], "jwks");

    let providers = external_auth_provider::Entity::find()
        .all(state.writer_db())
        .await
        .expect("providers should query");
    assert!(providers.is_empty());
    audit_service::flush_global_audit_log_manager().await;
    let audit_entry = audit_log::Entity::find()
        .filter(audit_log::Column::Action.eq(AuditAction::AdminTestExternalAuthProvider))
        .order_by_desc(audit_log::Column::Id)
        .one(state.writer_db())
        .await
        .expect("audit log should query")
        .expect("draft test should write an audit log");
    assert_eq!(audit_entry.user_id, 1);
    assert_eq!(audit_entry.entity_type, "external_auth_provider");
    assert_eq!(audit_entry.entity_name.as_deref(), Some("draft"));
    let details: Value = serde_json::from_str(
        audit_entry
            .details
            .as_deref()
            .expect("audit details should exist"),
    )
    .expect("audit details should parse");
    assert_eq!(details["provider_kind"], "oidc");
    assert_eq!(details["key"], "draft");
    assert_eq!(details["success"], true);

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/external-auth/providers/test")
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({
            "provider_kind": "oidc",
            "client_id": TEST_CLIENT_ID,
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    audit_service::flush_global_audit_log_manager().await;
    let audit_entry = audit_log::Entity::find()
        .filter(audit_log::Column::Action.eq(AuditAction::AdminTestExternalAuthProvider))
        .order_by_desc(audit_log::Column::Id)
        .one(state.writer_db())
        .await
        .expect("audit log should query")
        .expect("failed draft test should write an audit log");
    let details: Value = serde_json::from_str(
        audit_entry
            .details
            .as_deref()
            .expect("audit details should exist"),
    )
    .expect("audit details should parse");
    assert_eq!(details["provider_kind"], "oidc");
    assert_eq!(details["success"], false);

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/external-auth/providers")
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({
            "provider_kind": "oidc",
            "display_name": "Saved IDP",
            "issuer_url": mock_provider.issuer,
            "client_id": TEST_CLIENT_ID,
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let created: Value = test::read_body_json(resp).await;
    let provider_id = created["data"]["id"]
        .as_i64()
        .expect("provider id should be returned");

    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/v1/admin/external-auth/providers/{provider_id}/test"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["issuer"], mock_provider.issuer);

    let providers = external_auth_provider::Entity::find()
        .all(state.writer_db())
        .await
        .expect("providers should query");
    assert_eq!(providers.len(), 1);

    server.stop(true).await;
}

#[actix_web::test]
async fn admin_external_auth_provider_test_reports_discovery_failures_as_bad_request() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (admin_token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/external-auth/providers/test")
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({
            "provider_kind": "oidc",
            "issuer_url": "http://127.0.0.1:9",
            "client_id": TEST_CLIENT_ID,
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["code"], "bad_request");
    assert!(
        body["msg"]
            .as_str()
            .unwrap()
            .contains("OIDC discovery failed"),
        "unexpected error message: {}",
        body["msg"]
    );
}

#[actix_web::test]
async fn start_login_requires_public_site_url_for_callback_redirect_uri() {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (admin_token, _) = register_and_login!(app);
    let provider_key =
        create_external_auth_provider_key(&app, &admin_token, &mock_provider.issuer, true, false)
            .await;

    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/v1/auth/external-auth/oidc/{provider_key}/start"
        ))
        .insert_header(("Host", "localhost:8080"))
        .insert_header(("Origin", TEST_BROWSER_ORIGIN))
        .set_json(serde_json::json!({ "return_path": "/files" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(
        body["code"],
        ApiErrorCode::ExternalAuthCallbackRedirectUriRequired.as_str()
    );
    assert!(
        body["msg"].as_str().unwrap().contains("public_site_url"),
        "unexpected error message: {}",
        body["msg"]
    );

    server.stop(true).await;
}

#[actix_web::test]
async fn start_login_persists_pkce_flow_and_rejects_replayed_state() {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    let state = common::setup().await;
    state.runtime_config.apply(common::system_config_model(
        aster_drive::config::site_url::PUBLIC_SITE_URL_KEY,
        r#"["http://localhost:8080"]"#,
    ));
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    let provider_key =
        create_external_auth_provider_key(&app, &admin_token, &mock_provider.issuer, true, false)
            .await;

    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/v1/auth/external-auth/oidc/{provider_key}/start"
        ))
        .insert_header(("Origin", TEST_BROWSER_ORIGIN))
        .set_json(serde_json::json!({ "return_path": "/files?view=grid" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let auth_url = body["data"]["authorization_url"]
        .as_str()
        .expect("authorization url should be returned");
    assert!(auth_url.starts_with(&format!("{}/authorize?", mock_provider.issuer)));

    request_mock_authorize(auth_url).await;
    let authorize_request = mock_provider.last_authorize_request();
    assert_eq!(authorize_request.response_type, "code");
    assert_eq!(authorize_request.client_id, TEST_CLIENT_ID);
    assert_eq!(
        authorize_request.redirect_uri,
        format!("http://localhost:8080/api/v1/auth/external-auth/oidc/{provider_key}/callback")
    );
    assert!(authorize_request.scope.unwrap().contains("openid"));
    assert_eq!(
        authorize_request.code_challenge_method.as_deref(),
        Some("S256")
    );
    assert!(
        authorize_request
            .code_challenge
            .as_deref()
            .is_some_and(|value| !value.is_empty())
    );

    let flows = external_auth_login_flow::Entity::find()
        .all(state.writer_db())
        .await
        .expect("flows should query");
    assert_eq!(flows.len(), 1);
    assert_eq!(flows[0].return_path.as_deref(), Some("/files?view=grid"));
    assert_ne!(flows[0].state_hash, authorize_request.state);

    let consumed = external_auth_login_flow_repo::consume_by_state_hash(
        state.writer_db(),
        &aster_drive::utils::hash::sha256_hex(authorize_request.state.as_bytes()),
        Utc::now(),
    )
    .await
    .expect("flow consume should succeed");
    assert!(consumed.is_some());
    let replay = external_auth_login_flow_repo::consume_by_state_hash(
        state.writer_db(),
        &aster_drive::utils::hash::sha256_hex(authorize_request.state.as_bytes()),
        Utc::now(),
    )
    .await
    .expect("flow replay should query");
    assert!(replay.is_none());

    server.stop(true).await;
}

#[actix_web::test]
async fn finish_callback_verifies_jwks_and_issues_asterdrive_cookies() {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    let state = common::setup().await;
    configure_oidc_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    let provider_key =
        create_external_auth_provider_key(&app, &admin_token, &mock_provider.issuer, true, true)
            .await;

    let state_value =
        start_oidc_login(&app, &mock_provider, &provider_key, "/settings/security").await;
    let resp = finish_oidc_callback(&app, &provider_key, &state_value).await;
    assert_eq!(resp.status(), 302);
    assert_eq!(
        resp.headers()
            .get("Location")
            .and_then(|value| value.to_str().ok()),
        Some("http://localhost:8080/settings/security")
    );
    assert!(common::extract_cookie(&resp, "aster_access").is_some());
    assert!(common::extract_cookie(&resp, "aster_refresh").is_some());
    assert!(common::extract_cookie(&resp, "aster_csrf").is_some());

    let identities = external_auth_identity::Entity::find()
        .all(state.writer_db())
        .await
        .expect("identities should query");
    assert_eq!(identities.len(), 1);
    assert_eq!(identities[0].identity_namespace, mock_provider.issuer);
    assert_eq!(identities[0].subject, "oidc-subject-1");

    server.stop(true).await;
}

#[actix_web::test]
async fn google_callback_uses_oidc_sub_as_stable_identity() {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    mock_provider.set_subject("google-subject-1");
    mock_provider.set_email("google-user@example.com");

    let state = common::setup().await;
    configure_oidc_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let mut provider_model =
        google_external_auth_provider_model("google-test", &mock_provider.issuer, true);
    provider_model.auto_provision_enabled = Set(true);
    let provider = provider_model
        .insert(state.writer_db())
        .await
        .expect("Google provider should insert");

    let state_value = start_google_login(&app, &mock_provider, &provider.key, "/files").await;
    let authorize_request = mock_provider.last_authorize_request();
    assert_eq!(
        authorize_request.redirect_uri,
        format!(
            "http://localhost:8080/api/v1/auth/external-auth/google/{}/callback",
            provider.key
        )
    );
    let scope = authorize_request
        .scope
        .as_deref()
        .expect("Google OIDC authorization request should include scopes");
    assert!(scope.split_whitespace().any(|item| item == "openid"));
    assert!(scope.split_whitespace().any(|item| item == "profile"));
    assert!(scope.split_whitespace().any(|item| item == "email"));

    let resp = finish_google_callback(&app, &provider.key, &state_value).await;
    assert_eq!(resp.status(), 302);
    assert_eq!(
        resp.headers()
            .get("Location")
            .and_then(|value| value.to_str().ok()),
        Some("http://localhost:8080/files")
    );
    assert!(common::extract_cookie(&resp, "aster_access").is_some());

    let identities = external_auth_identity::Entity::find()
        .all(state.writer_db())
        .await
        .expect("identities should query");
    assert_eq!(identities.len(), 1);
    let user_id = identities[0].user_id;
    assert_eq!(identities[0].identity_namespace, mock_provider.issuer);
    assert_eq!(identities[0].subject, "google-subject-1");
    assert_eq!(
        identities[0].email_snapshot.as_deref(),
        Some("google-user@example.com")
    );

    mock_provider.set_email("renamed-google-user@example.com");
    let state_value = start_google_login(&app, &mock_provider, &provider.key, "/files").await;
    let resp = finish_google_callback(&app, &provider.key, &state_value).await;
    assert_eq!(resp.status(), 302);

    let identities = external_auth_identity::Entity::find()
        .all(state.writer_db())
        .await
        .expect("identities should query");
    assert_eq!(identities.len(), 1);
    assert_eq!(identities[0].user_id, user_id);
    assert_eq!(identities[0].subject, "google-subject-1");
    assert_eq!(
        identities[0].email_snapshot.as_deref(),
        Some("renamed-google-user@example.com")
    );

    server.stop(true).await;
}

#[actix_web::test]
async fn google_callback_rejects_unverified_missing_or_non_boolean_email_verified() {
    for (case_name, configure_claim) in [("false", 0_u8), ("missing", 1_u8), ("string", 2_u8)] {
        let (mock_provider, server) = start_mock_external_auth_provider().await;
        mock_provider.set_subject(&format!("google-{case_name}-subject"));
        mock_provider.set_email(&format!("google-{case_name}@example.com"));
        match configure_claim {
            0 => mock_provider.set_email_verified(false),
            1 => mock_provider.clear_email_verified_claim(),
            2 => mock_provider.set_email_verified_claim(serde_json::json!("true")),
            _ => unreachable!("test case should be covered"),
        }

        let state = common::setup().await;
        configure_oidc_public_site_url(&state);
        let app = create_test_app!(state.clone());
        let mut provider_model =
            google_external_auth_provider_model("google-test", &mock_provider.issuer, true);
        provider_model.auto_provision_enabled = Set(true);
        let provider = provider_model
            .insert(state.writer_db())
            .await
            .expect("Google provider should insert");

        let state_value = start_google_login(&app, &mock_provider, &provider.key, "/files").await;
        let resp = finish_google_callback(&app, &provider.key, &state_value).await;
        assert_oidc_error_redirect(&resp);

        let identities = external_auth_identity::Entity::find()
            .all(state.writer_db())
            .await
            .expect("identities should query");
        assert!(
            identities.is_empty(),
            "{case_name} email_verified claim should not create identity"
        );

        server.stop(true).await;
    }
}

#[actix_web::test]
async fn microsoft_callback_missing_email_uses_local_email_verification_flow() {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    mock_provider.set_subject("microsoft-subject-1");
    mock_provider.clear_email();

    let state = common::setup().await;
    configure_oidc_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let mut provider_model =
        microsoft_external_auth_provider_model("microsoft-test", &mock_provider.issuer, true);
    provider_model.auto_provision_enabled = Set(true);
    let provider = provider_model
        .insert(state.writer_db())
        .await
        .expect("Microsoft provider should insert");

    let state_value = start_microsoft_login(&app, &mock_provider, &provider.key, "/files").await;
    let authorize_request = mock_provider.last_authorize_request();
    assert_eq!(
        authorize_request.redirect_uri,
        format!(
            "http://localhost:8080/api/v1/auth/external-auth/microsoft/{}/callback",
            provider.key
        )
    );
    let scope = authorize_request
        .scope
        .as_deref()
        .expect("Microsoft OIDC authorization request should include scopes");
    assert!(scope.split_whitespace().any(|item| item == "openid"));
    assert!(scope.split_whitespace().any(|item| item == "profile"));
    assert!(scope.split_whitespace().any(|item| item == "email"));

    let resp = finish_microsoft_callback(&app, &provider.key, &state_value).await;
    let flow_token = oidc_email_required_flow(&resp);
    assert!(!flow_token.is_empty());

    let identities = external_auth_identity::Entity::find()
        .all(state.writer_db())
        .await
        .expect("identities should query");
    assert!(identities.is_empty());

    server.stop(true).await;
}

#[actix_web::test]
async fn finish_callback_auto_links_verified_email_to_existing_user() {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    mock_provider.set_subject("auto-link-subject");
    mock_provider.set_email("linked@example.com");

    let state = common::setup().await;
    configure_oidc_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    let linked_user_id = admin_create_user!(
        app,
        admin_token,
        "linked-user",
        "linked@example.com",
        "password123"
    );
    let provider_key = create_external_auth_provider_with_key(
        &app,
        &admin_token,
        TestOidcProviderOptions {
            auto_link_verified_email_enabled: true,
            ..TestOidcProviderOptions::mock(&mock_provider.issuer)
        },
    )
    .await;

    let state_value = start_oidc_login(&app, &mock_provider, &provider_key, "/files").await;
    let resp = finish_oidc_callback(&app, &provider_key, &state_value).await;
    assert_eq!(resp.status(), 302);
    assert_eq!(
        resp.headers()
            .get("Location")
            .and_then(|value| value.to_str().ok()),
        Some("http://localhost:8080/files")
    );
    assert!(common::extract_cookie(&resp, "aster_access").is_some());
    assert!(common::extract_cookie(&resp, "aster_refresh").is_some());

    let identities = external_auth_identity::Entity::find()
        .all(state.writer_db())
        .await
        .expect("identities should query");
    assert_eq!(identities.len(), 1);
    assert_eq!(identities[0].user_id, linked_user_id);
    assert_eq!(identities[0].identity_namespace, mock_provider.issuer);
    assert_eq!(identities[0].subject, "auto-link-subject");

    server.stop(true).await;
}

#[actix_web::test]
async fn finish_callback_falls_back_to_manual_binding_for_unverified_auto_link_email() {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    mock_provider.set_subject("unverified-link-subject");
    mock_provider.set_email("unverified@example.com");
    mock_provider.set_email_verified(false);

    let state = common::setup().await;
    configure_oidc_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    let linked_user_id = admin_create_user!(
        app,
        admin_token,
        "unverified-user",
        "unverified@example.com",
        "password123"
    );
    let provider_key = create_external_auth_provider_with_key(
        &app,
        &admin_token,
        TestOidcProviderOptions {
            auto_link_verified_email_enabled: true,
            require_email_verified: false,
            ..TestOidcProviderOptions::mock(&mock_provider.issuer)
        },
    )
    .await;

    let state_value = start_oidc_login(&app, &mock_provider, &provider_key, "/").await;
    let resp = finish_oidc_callback(&app, &provider_key, &state_value).await;
    let flow_token = oidc_email_required_flow(&resp);

    let identities = external_auth_identity::Entity::find()
        .all(state.writer_db())
        .await
        .expect("identities should query");
    assert!(identities.is_empty());

    let resp = link_oidc_with_password(&app, &flow_token, "unverified-user", "password123").await;
    assert_eq!(resp.status(), 200);
    assert!(common::extract_cookie(&resp, "aster_access").is_some());

    let identities = external_auth_identity::Entity::find()
        .all(state.writer_db())
        .await
        .expect("identities should query");
    assert_eq!(identities.len(), 1);
    assert_eq!(identities[0].user_id, linked_user_id);
    assert_eq!(identities[0].subject, "unverified-link-subject");
    assert_eq!(identities[0].email_snapshot.as_deref(), None);

    server.stop(true).await;
}

#[actix_web::test]
async fn finish_callback_rejects_disabled_user_with_existing_identity() {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    mock_provider.set_subject("disabled-subject");
    mock_provider.set_email("disabled@example.com");

    let state = common::setup().await;
    configure_oidc_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    let disabled_user_id = admin_create_user!(
        app,
        admin_token,
        "disabled-user",
        "disabled@example.com",
        "password123"
    );
    let created =
        create_external_auth_provider(&app, &admin_token, &mock_provider.issuer, true, false).await;
    let provider_key = created_provider_key(&created);
    let provider_id = created["data"]["id"]
        .as_i64()
        .expect("provider id should be returned");
    external_auth_identity_repo::create_identity(
        state.writer_db(),
        external_auth_identity_repo::CreateExternalAuthIdentityInput {
            user_id: disabled_user_id,
            provider_id,
            identity_namespace: &mock_provider.issuer,
            subject: "disabled-subject",
            email_snapshot: Some("disabled@example.com"),
            display_name_snapshot: Some("Disabled User"),
            now: Utc::now(),
        },
    )
    .await
    .expect("identity should create");
    disable_user(&state, disabled_user_id).await;

    let state_value = start_oidc_login(&app, &mock_provider, &provider_key, "/").await;
    let resp = finish_oidc_callback(&app, &provider_key, &state_value).await;
    assert_oidc_error_redirect(&resp);

    server.stop(true).await;
}

#[actix_web::test]
async fn finish_callback_allows_existing_identity_without_email_claim() {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    mock_provider.set_subject("linked-no-email-subject");
    mock_provider.clear_email();

    let state = common::setup().await;
    configure_oidc_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    let linked_user_id = admin_create_user!(
        app,
        admin_token,
        "linked-no-email",
        "linked-no-email@example.com",
        "password123"
    );
    let created =
        create_external_auth_provider(&app, &admin_token, &mock_provider.issuer, true, false).await;
    let provider_key = created_provider_key(&created);
    let provider_id = created["data"]["id"]
        .as_i64()
        .expect("provider id should be returned");
    external_auth_identity_repo::create_identity(
        state.writer_db(),
        external_auth_identity_repo::CreateExternalAuthIdentityInput {
            user_id: linked_user_id,
            provider_id,
            identity_namespace: &mock_provider.issuer,
            subject: "linked-no-email-subject",
            email_snapshot: None,
            display_name_snapshot: Some("Linked No Email"),
            now: Utc::now(),
        },
    )
    .await
    .expect("identity should create");

    let state_value = start_oidc_login(&app, &mock_provider, &provider_key, "/files").await;
    let resp = finish_oidc_callback(&app, &provider_key, &state_value).await;
    assert_eq!(resp.status(), 302);
    assert_eq!(
        resp.headers()
            .get("Location")
            .and_then(|value| value.to_str().ok()),
        Some("http://localhost:8080/files")
    );
    assert!(common::extract_cookie(&resp, "aster_access").is_some());

    server.stop(true).await;
}

#[actix_web::test]
async fn finish_callback_falls_back_to_manual_binding_when_auto_provision_disabled() {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    mock_provider.set_subject("unlinked-subject");
    mock_provider.set_email("unlinked@example.com");

    let state = common::setup().await;
    configure_oidc_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    let linked_user_id = admin_create_user!(
        app,
        admin_token,
        "manual-link-user",
        "manual-link@example.com",
        "password123"
    );
    let provider_key = create_external_auth_provider_with_key(
        &app,
        &admin_token,
        TestOidcProviderOptions::mock(&mock_provider.issuer),
    )
    .await;

    let state_value = start_oidc_login(&app, &mock_provider, &provider_key, "/").await;
    let resp = finish_oidc_callback(&app, &provider_key, &state_value).await;
    let flow_token = oidc_email_required_flow(&resp);

    let identities = external_auth_identity::Entity::find()
        .all(state.writer_db())
        .await
        .expect("identities should query");
    assert!(identities.is_empty());
    let users = user::Entity::find()
        .all(state.writer_db())
        .await
        .expect("users should query");
    assert_eq!(users.len(), 2);

    let resp = link_oidc_with_password(&app, &flow_token, "manual-link-user", "password123").await;
    assert_eq!(resp.status(), 200);
    assert!(common::extract_cookie(&resp, "aster_access").is_some());

    let identities = external_auth_identity::Entity::find()
        .all(state.writer_db())
        .await
        .expect("identities should query");
    assert_eq!(identities.len(), 1);
    assert_eq!(identities[0].user_id, linked_user_id);
    assert_eq!(identities[0].subject, "unlinked-subject");
    assert_eq!(identities[0].email_snapshot.as_deref(), None);

    server.stop(true).await;
}

#[actix_web::test]
async fn finish_callback_respects_global_registration_setting_for_auto_provision() {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    mock_provider.set_subject("registration-closed-subject");
    mock_provider.set_email("registration-closed@example.com");

    let state = common::setup().await;
    configure_oidc_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    let existing_user_id = admin_create_user!(
        app,
        admin_token,
        "reg-closed",
        "existing-registration-closed@example.com",
        "password123"
    );
    let provider_key = create_external_auth_provider_with_key(
        &app,
        &admin_token,
        TestOidcProviderOptions {
            auto_provision_enabled: true,
            auto_link_verified_email_enabled: true,
            ..TestOidcProviderOptions::mock(&mock_provider.issuer)
        },
    )
    .await;
    state.runtime_config.apply(common::system_config_model(
        aster_drive::config::auth_runtime::AUTH_ALLOW_USER_REGISTRATION_KEY,
        "false",
    ));

    let state_value = start_oidc_login(&app, &mock_provider, &provider_key, "/").await;
    let resp = finish_oidc_callback(&app, &provider_key, &state_value).await;
    let flow_token = oidc_email_required_flow(&resp);

    let identities = external_auth_identity::Entity::find()
        .all(state.writer_db())
        .await
        .expect("identities should query");
    assert!(identities.is_empty());
    let users = user::Entity::find()
        .all(state.writer_db())
        .await
        .expect("users should query");
    assert_eq!(users.len(), 2);

    let resp = link_oidc_with_password(&app, &flow_token, "reg-closed", "password123").await;
    assert_eq!(resp.status(), 200);
    assert!(common::extract_cookie(&resp, "aster_access").is_some());

    let identities = external_auth_identity::Entity::find()
        .all(state.writer_db())
        .await
        .expect("identities should query");
    assert_eq!(identities.len(), 1);
    assert_eq!(identities[0].user_id, existing_user_id);
    assert_eq!(identities[0].subject, "registration-closed-subject");
    assert_eq!(identities[0].email_snapshot.as_deref(), None);
    let users = user::Entity::find()
        .all(state.writer_db())
        .await
        .expect("users should query");
    assert_eq!(users.len(), 2);

    server.stop(true).await;
}

#[actix_web::test]
async fn manual_binding_respects_global_registration_setting_only_when_email_verification_would_create_user()
 {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    mock_provider.set_subject("manual-registration-closed-subject");
    mock_provider.set_email("manual-registration-closed@example.com");

    let state = common::setup().await;
    configure_oidc_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    let provider_key = create_external_auth_provider_with_key(
        &app,
        &admin_token,
        TestOidcProviderOptions::mock(&mock_provider.issuer),
    )
    .await;
    state.runtime_config.apply(common::system_config_model(
        aster_drive::config::auth_runtime::AUTH_ALLOW_USER_REGISTRATION_KEY,
        "false",
    ));

    let state_value = start_oidc_login(&app, &mock_provider, &provider_key, "/").await;
    let resp = finish_oidc_callback(&app, &provider_key, &state_value).await;
    let flow_token = oidc_email_required_flow(&resp);

    let resp =
        start_oidc_email_verification(&app, &flow_token, "manual-registration-closed@example.com")
            .await;
    assert_eq!(resp.status(), 403);

    let identities = external_auth_identity::Entity::find()
        .all(state.writer_db())
        .await
        .expect("identities should query");
    assert!(identities.is_empty());
    let users = user::Entity::find()
        .all(state.writer_db())
        .await
        .expect("users should query");
    assert_eq!(users.len(), 1);

    server.stop(true).await;
}

#[actix_web::test]
async fn finish_callback_auto_link_by_verified_email_ignores_global_registration_setting() {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    mock_provider.set_subject("registration-closed-auto-link-subject");
    mock_provider.set_email("auto-link-registration-closed@example.com");

    let state = common::setup().await;
    configure_oidc_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    let linked_user_id = admin_create_user!(
        app,
        admin_token,
        "reg-auto-link",
        "auto-link-registration-closed@example.com",
        "password123"
    );
    let provider_key = create_external_auth_provider_with_key(
        &app,
        &admin_token,
        TestOidcProviderOptions {
            auto_provision_enabled: true,
            auto_link_verified_email_enabled: true,
            ..TestOidcProviderOptions::mock(&mock_provider.issuer)
        },
    )
    .await;
    state.runtime_config.apply(common::system_config_model(
        aster_drive::config::auth_runtime::AUTH_ALLOW_USER_REGISTRATION_KEY,
        "false",
    ));

    let state_value = start_oidc_login(&app, &mock_provider, &provider_key, "/files").await;
    let resp = finish_oidc_callback(&app, &provider_key, &state_value).await;
    assert_eq!(resp.status(), 302);
    assert_eq!(
        resp.headers()
            .get("Location")
            .and_then(|value| value.to_str().ok()),
        Some("http://localhost:8080/files")
    );
    assert!(common::extract_cookie(&resp, "aster_access").is_some());

    let identities = external_auth_identity::Entity::find()
        .all(state.writer_db())
        .await
        .expect("identities should query");
    assert_eq!(identities.len(), 1);
    assert_eq!(identities[0].user_id, linked_user_id);
    assert_eq!(
        identities[0].subject,
        "registration-closed-auto-link-subject"
    );

    server.stop(true).await;
}

#[actix_web::test]
async fn no_email_claim_can_register_after_local_email_verification() {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    mock_provider.set_subject("fallback-provision-subject");
    mock_provider.clear_email();

    let state = common::setup().await;
    configure_oidc_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    let provider_key = create_external_auth_provider_with_key(
        &app,
        &admin_token,
        TestOidcProviderOptions::mock(&mock_provider.issuer),
    )
    .await;

    let state_value =
        start_oidc_login(&app, &mock_provider, &provider_key, "/settings/security").await;
    let resp = finish_oidc_callback(&app, &provider_key, &state_value).await;
    let flow_token = oidc_email_required_flow(&resp);

    assert_start_oidc_email_verification_ok(&app, &flow_token, "fallback-provision@example.com")
        .await;
    let token = latest_oidc_email_verification_token(&state).await;
    let resp = confirm_oidc_email_verification(&app, &token).await;
    assert_eq!(resp.status(), 302);
    assert_eq!(
        resp.headers()
            .get("Location")
            .and_then(|value| value.to_str().ok()),
        Some("http://localhost:8080/settings/security")
    );
    assert!(common::extract_cookie(&resp, "aster_access").is_some());
    assert!(common::extract_cookie(&resp, "aster_refresh").is_some());

    let user = user::Entity::find()
        .filter(user::Column::Email.eq("fallback-provision@example.com"))
        .one(state.writer_db())
        .await
        .expect("user should query")
        .expect("OIDC verified email should create user");
    assert!(user.email_verified_at.is_some());
    let identities = external_auth_identity::Entity::find()
        .all(state.writer_db())
        .await
        .expect("identities should query");
    assert_eq!(identities.len(), 1);
    assert_eq!(identities[0].user_id, user.id);
    assert_eq!(identities[0].subject, "fallback-provision-subject");
    assert_eq!(
        identities[0].email_snapshot.as_deref(),
        Some("fallback-provision@example.com")
    );

    server.stop(true).await;
}

#[actix_web::test]
async fn auto_provision_retries_username_collision_with_random_suffix() {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    mock_provider.set_subject("username-collision-subject");
    mock_provider.set_email("username-collision@example.com");

    let state = common::setup().await;
    configure_oidc_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    admin_create_user!(
        app,
        admin_token,
        "oidctest",
        "existing-oidc-name@example.com",
        "password123"
    );
    let provider_key =
        create_external_auth_provider_key(&app, &admin_token, &mock_provider.issuer, true, true)
            .await;

    let state_value = start_oidc_login(&app, &mock_provider, &provider_key, "/files").await;
    let resp = finish_oidc_callback(&app, &provider_key, &state_value).await;
    assert_eq!(resp.status(), 302);
    assert!(common::extract_cookie(&resp, "aster_access").is_some());

    let user = user::Entity::find()
        .filter(user::Column::Email.eq("username-collision@example.com"))
        .one(state.writer_db())
        .await
        .expect("user should query")
        .expect("OIDC auto-provision should create user");
    assert_ne!(user.username, "oidctest");
    assert!(user.username.starts_with("oidctest-"));
    assert!(user.username.len() <= 16);

    let identities = external_auth_identity::Entity::find()
        .filter(external_auth_identity::Column::Subject.eq("username-collision-subject"))
        .all(state.writer_db())
        .await
        .expect("identities should query");
    assert_eq!(identities.len(), 1);
    assert_eq!(identities[0].user_id, user.id);
    assert!(
        state
            .policy_snapshot
            .resolve_default_policy_group_id(user.id)
            .is_some()
    );

    server.stop(true).await;
}

#[actix_web::test]
async fn no_email_claim_falls_back_to_local_email_verification_for_existing_user_link() {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    mock_provider.set_subject("fallback-link-subject");
    mock_provider.clear_email();

    let state = common::setup().await;
    configure_oidc_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    let linked_user_id = admin_create_user!(
        app,
        admin_token,
        "fb-link-user",
        "fallback-link@example.com",
        "password123"
    );
    let provider_key = create_external_auth_provider_with_key(
        &app,
        &admin_token,
        TestOidcProviderOptions {
            auto_link_verified_email_enabled: true,
            ..TestOidcProviderOptions::mock(&mock_provider.issuer)
        },
    )
    .await;

    let state_value = start_oidc_login(&app, &mock_provider, &provider_key, "/files").await;
    let resp = finish_oidc_callback(&app, &provider_key, &state_value).await;
    let flow_token = oidc_email_required_flow(&resp);

    assert_start_oidc_email_verification_ok(&app, &flow_token, "fallback-link@example.com").await;
    let token = latest_oidc_email_verification_token(&state).await;
    let resp = confirm_oidc_email_verification(&app, &token).await;
    assert_eq!(resp.status(), 302);
    assert_eq!(
        resp.headers()
            .get("Location")
            .and_then(|value| value.to_str().ok()),
        Some("http://localhost:8080/files")
    );
    assert!(common::extract_cookie(&resp, "aster_access").is_some());

    let identities = external_auth_identity::Entity::find()
        .all(state.writer_db())
        .await
        .expect("identities should query");
    assert_eq!(identities.len(), 1);
    assert_eq!(identities[0].user_id, linked_user_id);
    assert_eq!(identities[0].subject, "fallback-link-subject");

    server.stop(true).await;
}

#[actix_web::test]
async fn manual_email_verification_can_link_existing_user_without_auto_link_enabled() {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    mock_provider.set_subject("manual-email-link-subject");
    mock_provider.clear_email();

    let state = common::setup().await;
    configure_oidc_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    let linked_user_id = admin_create_user!(
        app,
        admin_token,
        "manual-mail-link",
        "manual-email-link@example.com",
        "password123"
    );
    let provider_key = create_external_auth_provider_with_key(
        &app,
        &admin_token,
        TestOidcProviderOptions::mock(&mock_provider.issuer),
    )
    .await;

    let state_value = start_oidc_login(&app, &mock_provider, &provider_key, "/files").await;
    let resp = finish_oidc_callback(&app, &provider_key, &state_value).await;
    let flow_token = oidc_email_required_flow(&resp);

    assert_start_oidc_email_verification_ok(&app, &flow_token, "manual-email-link@example.com")
        .await;
    let token = latest_oidc_email_verification_token(&state).await;
    let resp = confirm_oidc_email_verification(&app, &token).await;
    assert_eq!(resp.status(), 302);
    assert_eq!(
        resp.headers()
            .get("Location")
            .and_then(|value| value.to_str().ok()),
        Some("http://localhost:8080/files")
    );
    assert!(common::extract_cookie(&resp, "aster_access").is_some());

    let identities = external_auth_identity::Entity::find()
        .all(state.writer_db())
        .await
        .expect("identities should query");
    assert_eq!(identities.len(), 1);
    assert_eq!(identities[0].user_id, linked_user_id);
    assert_eq!(identities[0].subject, "manual-email-link-subject");
    assert_eq!(
        identities[0].email_snapshot.as_deref(),
        Some("manual-email-link@example.com")
    );

    server.stop(true).await;
}

#[actix_web::test]
async fn no_email_claim_can_link_after_local_password_login() {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    mock_provider.set_subject("fallback-password-link-subject");
    mock_provider.clear_email();

    let state = common::setup().await;
    configure_oidc_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    let linked_user_id = admin_create_user!(
        app,
        admin_token,
        "pwd-link-user",
        "password-link@example.com",
        "password123"
    );
    let provider_key = create_external_auth_provider_with_key(
        &app,
        &admin_token,
        TestOidcProviderOptions::mock(&mock_provider.issuer),
    )
    .await;

    let state_value = start_oidc_login(&app, &mock_provider, &provider_key, "/files").await;
    let resp = finish_oidc_callback(&app, &provider_key, &state_value).await;
    let flow_token = oidc_email_required_flow(&resp);

    let resp = link_oidc_with_password(&app, &flow_token, "pwd-link-user", "password123").await;
    assert_eq!(resp.status(), 200);
    assert!(common::extract_cookie(&resp, "aster_access").is_some());
    assert!(common::extract_cookie(&resp, "aster_refresh").is_some());
    assert!(common::extract_cookie(&resp, "aster_csrf").is_some());
    let body: Value = test::read_body_json(resp).await;
    assert!(body["data"]["expires_in"].as_u64().is_some());

    let identities = external_auth_identity::Entity::find()
        .all(state.writer_db())
        .await
        .expect("identities should query");
    assert_eq!(identities.len(), 1);
    assert_eq!(identities[0].user_id, linked_user_id);
    assert_eq!(identities[0].subject, "fallback-password-link-subject");
    assert_eq!(identities[0].email_snapshot.as_deref(), None);

    let resp = link_oidc_with_password(&app, &flow_token, "pwd-link-user", "password123").await;
    assert_eq!(resp.status(), 400);

    server.stop(true).await;
}

#[actix_web::test]
async fn oidc_password_link_rejects_wrong_password_without_sending_email() {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    mock_provider.set_subject("fallback-password-link-wrong-subject");
    mock_provider.clear_email();

    let state = common::setup().await;
    configure_oidc_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    admin_create_user!(
        app,
        admin_token,
        "pwd-link-wrong",
        "password-link-wrong@example.com",
        "password123"
    );
    let provider_key = create_external_auth_provider_with_key(
        &app,
        &admin_token,
        TestOidcProviderOptions::mock(&mock_provider.issuer),
    )
    .await;

    let state_value = start_oidc_login(&app, &mock_provider, &provider_key, "/").await;
    let resp = finish_oidc_callback(&app, &provider_key, &state_value).await;
    let flow_token = oidc_email_required_flow(&resp);

    let resp = link_oidc_with_password(&app, &flow_token, "pwd-link-wrong", "wrong-password").await;
    assert_eq!(resp.status(), 401);
    assert!(common::extract_cookie(&resp, "aster_access").is_none());

    let identities = external_auth_identity::Entity::find()
        .all(state.writer_db())
        .await
        .expect("identities should query");
    assert!(identities.is_empty());
    let email_flows = external_auth_email_verification_flow::Entity::find()
        .all(state.writer_db())
        .await
        .expect("email verification flows should query");
    assert_eq!(email_flows.len(), 1);
    assert!(email_flows[0].verification_token_hash.is_none());
    assert!(email_flows[0].consumed_at.is_none());

    server.stop(true).await;
}

#[actix_web::test]
async fn oidc_email_verification_respects_global_registration_setting() {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    mock_provider.set_subject("fallback-registration-closed-subject");
    mock_provider.clear_email();

    let state = common::setup().await;
    configure_oidc_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    let provider_key = create_external_auth_provider_with_key(
        &app,
        &admin_token,
        TestOidcProviderOptions::mock(&mock_provider.issuer),
    )
    .await;

    let state_value = start_oidc_login(&app, &mock_provider, &provider_key, "/").await;
    let resp = finish_oidc_callback(&app, &provider_key, &state_value).await;
    let flow_token = oidc_email_required_flow(&resp);
    state.runtime_config.apply(common::system_config_model(
        aster_drive::config::auth_runtime::AUTH_ALLOW_USER_REGISTRATION_KEY,
        "false",
    ));

    let resp = start_oidc_email_verification(
        &app,
        &flow_token,
        "registration-closed-fallback@example.com",
    )
    .await;
    assert_eq!(resp.status(), 403);

    let users = user::Entity::find()
        .all(state.writer_db())
        .await
        .expect("users should query");
    assert_eq!(users.len(), 1);
    let identities = external_auth_identity::Entity::find()
        .all(state.writer_db())
        .await
        .expect("identities should query");
    assert!(identities.is_empty());

    server.stop(true).await;
}

#[actix_web::test]
async fn oidc_email_verification_enforces_entered_email_domain() {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    mock_provider.set_subject("fallback-domain-subject");
    mock_provider.clear_email();

    let state = common::setup().await;
    configure_oidc_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    let provider_key = create_external_auth_provider_with_key(
        &app,
        &admin_token,
        TestOidcProviderOptions {
            auto_provision_enabled: true,
            ..TestOidcProviderOptions::mock(&mock_provider.issuer)
        },
    )
    .await;

    let state_value = start_oidc_login(&app, &mock_provider, &provider_key, "/").await;
    let resp = finish_oidc_callback(&app, &provider_key, &state_value).await;
    let flow_token = oidc_email_required_flow(&resp);

    let resp = start_oidc_email_verification(&app, &flow_token, "user@example.org").await;
    assert_eq!(resp.status(), 403);
    let users = user::Entity::find()
        .all(state.writer_db())
        .await
        .expect("users should query");
    assert_eq!(users.len(), 1);

    server.stop(true).await;
}

#[actix_web::test]
async fn oidc_email_verification_rejects_replay() {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    mock_provider.set_subject("fallback-replay-subject");
    mock_provider.clear_email();

    let state = common::setup().await;
    configure_oidc_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    let provider_key = create_external_auth_provider_with_key(
        &app,
        &admin_token,
        TestOidcProviderOptions {
            auto_provision_enabled: true,
            ..TestOidcProviderOptions::mock(&mock_provider.issuer)
        },
    )
    .await;

    let state_value = start_oidc_login(&app, &mock_provider, &provider_key, "/").await;
    let resp = finish_oidc_callback(&app, &provider_key, &state_value).await;
    let flow_token = oidc_email_required_flow(&resp);
    assert_start_oidc_email_verification_ok(&app, &flow_token, "fallback-replay@example.com").await;
    let token = latest_oidc_email_verification_token(&state).await;

    let resp = confirm_oidc_email_verification(&app, &token).await;
    assert_eq!(resp.status(), 302);
    assert!(common::extract_cookie(&resp, "aster_access").is_some());
    let resp = confirm_oidc_email_verification(&app, &token).await;
    assert_eq!(resp.status(), 302);
    let location = resp
        .headers()
        .get("Location")
        .and_then(|value| value.to_str().ok())
        .expect("replay redirect should have location");
    assert_eq!(
        location,
        "http://localhost:8080/login?external_auth=email_verification_invalid"
    );
    assert!(common::extract_cookie(&resp, "aster_access").is_none());
    let identities = external_auth_identity::Entity::find()
        .all(state.writer_db())
        .await
        .expect("identities should query");
    assert_eq!(identities.len(), 1);

    server.stop(true).await;
}

#[actix_web::test]
async fn oidc_email_verification_rejects_expired_pending_flow() {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    mock_provider.set_subject("fallback-expired-subject");
    mock_provider.clear_email();

    let state = common::setup().await;
    configure_oidc_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    let provider_key = create_external_auth_provider_with_key(
        &app,
        &admin_token,
        TestOidcProviderOptions {
            auto_provision_enabled: true,
            ..TestOidcProviderOptions::mock(&mock_provider.issuer)
        },
    )
    .await;

    let state_value = start_oidc_login(&app, &mock_provider, &provider_key, "/").await;
    let resp = finish_oidc_callback(&app, &provider_key, &state_value).await;
    let flow_token = oidc_email_required_flow(&resp);
    let mut flow = external_auth_email_verification_flow::Entity::find()
        .one(state.writer_db())
        .await
        .expect("flow should query")
        .expect("flow should exist")
        .into_active_model();
    flow.expires_at = Set(Utc::now() - Duration::minutes(1));
    flow.update(state.writer_db())
        .await
        .expect("flow should update");

    let resp =
        start_oidc_email_verification(&app, &flow_token, "fallback-expired@example.com").await;
    assert_eq!(resp.status(), 400);

    server.stop(true).await;
}

#[actix_web::test]
async fn finish_callback_rejects_flow_after_provider_disabled() {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    let state = common::setup().await;
    configure_oidc_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    let created =
        create_external_auth_provider(&app, &admin_token, &mock_provider.issuer, true, true).await;
    let provider_key = created_provider_key(&created);
    let provider_id = created["data"]["id"]
        .as_i64()
        .expect("provider id should be returned");
    let state_value = start_oidc_login(&app, &mock_provider, &provider_key, "/").await;

    let req = test::TestRequest::patch()
        .uri(&format!(
            "/api/v1/admin/external-auth/providers/{provider_id}"
        ))
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({ "enabled": false }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let resp = finish_oidc_callback(&app, &provider_key, &state_value).await;
    assert_oidc_error_redirect(&resp);
    let identities = external_auth_identity::Entity::find()
        .all(state.writer_db())
        .await
        .expect("identities should query");
    assert!(identities.is_empty());

    server.stop(true).await;
}

#[actix_web::test]
async fn finish_callback_rejects_audience_mismatch() {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    mock_provider.set_audience("wrong-client-id");

    let state = common::setup().await;
    configure_oidc_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    let provider_key =
        create_external_auth_provider_key(&app, &admin_token, &mock_provider.issuer, true, true)
            .await;

    let state_value = start_oidc_login(&app, &mock_provider, &provider_key, "/").await;
    let resp = finish_oidc_callback(&app, &provider_key, &state_value).await;
    assert_oidc_error_redirect(&resp);

    let identities = external_auth_identity::Entity::find()
        .all(state.writer_db())
        .await
        .expect("identities should query");
    assert!(identities.is_empty());

    server.stop(true).await;
}

#[actix_web::test]
async fn finish_callback_rejects_oversized_subject_before_db_write() {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    mock_provider.set_subject(&"s".repeat(256));

    let state = common::setup().await;
    configure_oidc_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    let provider_key =
        create_external_auth_provider_key(&app, &admin_token, &mock_provider.issuer, true, true)
            .await;

    let state_value = start_oidc_login(&app, &mock_provider, &provider_key, "/").await;
    let resp = finish_oidc_callback(&app, &provider_key, &state_value).await;
    assert_oidc_error_redirect(&resp);

    let identities = external_auth_identity::Entity::find()
        .all(state.writer_db())
        .await
        .expect("identities should query");
    assert!(identities.is_empty());

    server.stop(true).await;
}

#[actix_web::test]
async fn finish_callback_rejects_nonce_mismatch() {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    let state = common::setup().await;
    configure_oidc_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    let provider_key =
        create_external_auth_provider_key(&app, &admin_token, &mock_provider.issuer, true, true)
            .await;

    let state_value = start_oidc_login(&app, &mock_provider, &provider_key, "/").await;
    mock_provider.set_nonce_override(Some("wrong-nonce".to_string()));
    let resp = finish_oidc_callback(&app, &provider_key, &state_value).await;
    assert_oidc_error_redirect(&resp);

    let identities = external_auth_identity::Entity::find()
        .all(state.writer_db())
        .await
        .expect("identities should query");
    assert!(identities.is_empty());

    server.stop(true).await;
}

#[actix_web::test]
async fn finish_callback_rejects_provider_key_mismatch() {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    let state = common::setup().await;
    configure_oidc_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    let provider_key =
        create_external_auth_provider_key(&app, &admin_token, &mock_provider.issuer, true, true)
            .await;

    let state_value = start_oidc_login(&app, &mock_provider, &provider_key, "/").await;
    let resp = finish_oidc_callback(&app, "other", &state_value).await;
    assert_oidc_error_redirect(&resp);

    let identities = external_auth_identity::Entity::find()
        .all(state.writer_db())
        .await
        .expect("identities should query");
    assert!(identities.is_empty());

    server.stop(true).await;
}

#[actix_web::test]
async fn finish_callback_enforces_allowed_domains() {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    mock_provider.set_subject("domain-subject");
    mock_provider.set_email("domain-user@example.com");

    let state = common::setup().await;
    configure_oidc_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    let provider_key = create_external_auth_provider_with_key(
        &app,
        &admin_token,
        TestOidcProviderOptions {
            auto_provision_enabled: true,
            allowed_domains: vec!["example.org".to_string()],
            ..TestOidcProviderOptions::mock(&mock_provider.issuer)
        },
    )
    .await;

    let state_value = start_oidc_login(&app, &mock_provider, &provider_key, "/").await;
    let resp = finish_oidc_callback(&app, &provider_key, &state_value).await;
    assert_oidc_error_redirect(&resp);

    let identities = external_auth_identity::Entity::find()
        .all(state.writer_db())
        .await
        .expect("identities should query");
    assert!(identities.is_empty());
    let users = user::Entity::find()
        .all(state.writer_db())
        .await
        .expect("users should query");
    assert_eq!(users.len(), 1);

    server.stop(true).await;
}

#[actix_web::test]
async fn oidc_links_can_be_listed_and_deleted_after_login() {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    mock_provider.set_subject("links-subject");
    mock_provider.set_email("links-user@example.com");

    let state = common::setup().await;
    configure_oidc_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    let provider_key =
        create_external_auth_provider_key(&app, &admin_token, &mock_provider.issuer, true, true)
            .await;

    let state_value =
        start_oidc_login(&app, &mock_provider, &provider_key, "/settings/security").await;
    let resp = finish_oidc_callback(&app, &provider_key, &state_value).await;
    assert_eq!(resp.status(), 302);
    let access_token =
        common::extract_cookie(&resp, "aster_access").expect("access cookie should be set");

    let req = test::TestRequest::get()
        .uri("/api/v1/auth/external-auth/links")
        .insert_header(("Cookie", common::access_cookie_header(&access_token)))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let links = body["data"]
        .as_array()
        .expect("links response should be an array");
    assert_eq!(links.len(), 1);
    assert_eq!(links[0]["provider_key"], provider_key);
    assert_eq!(links[0]["subject"], "links-subject");
    let link_id = links[0]["id"].as_i64().expect("link id should exist");

    let req = test::TestRequest::delete()
        .uri(&format!("/api/v1/auth/external-auth/links/{link_id}"))
        .insert_header(("Cookie", common::access_cookie_header(&access_token)))
        .insert_header(common::csrf_header_for(&access_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);

    let req = test::TestRequest::get()
        .uri("/api/v1/auth/external-auth/links")
        .insert_header(("Cookie", common::access_cookie_header(&access_token)))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"].as_array().unwrap().len(), 0);

    let identities = external_auth_identity::Entity::find()
        .all(state.writer_db())
        .await
        .expect("identities should query");
    assert!(identities.is_empty());

    server.stop(true).await;
}

#[actix_web::test]
async fn finish_callback_rejects_issuer_mismatch_after_id_token_verification() {
    let (mock_provider, server) = start_mock_external_auth_provider().await;
    let state = common::setup().await;
    configure_oidc_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    let provider_key =
        create_external_auth_provider_key(&app, &admin_token, &mock_provider.issuer, true, true)
            .await;
    mock_provider.set_issuer_override(Some("http://evil.example.test".to_string()));

    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/v1/auth/external-auth/oidc/{provider_key}/start"
        ))
        .insert_header(("Origin", TEST_BROWSER_ORIGIN))
        .set_json(serde_json::json!({ "return_path": "/" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    request_mock_authorize(body["data"]["authorization_url"].as_str().unwrap()).await;
    let state_value = mock_provider.last_authorize_request().state;

    let callback = format!(
        "/api/v1/auth/external-auth/oidc/{provider_key}/callback?code=mock-code&state={state_value}"
    );
    let req = test::TestRequest::get().uri(&callback).to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 302);
    let location = resp
        .headers()
        .get("Location")
        .and_then(|value| value.to_str().ok())
        .unwrap();
    assert!(location.starts_with("http://localhost:8080/login?external_auth=error"));
    assert!(common::extract_cookie(&resp, "aster_access").is_none());

    let identities = external_auth_identity::Entity::find()
        .all(state.writer_db())
        .await
        .expect("identities should query");
    assert!(identities.is_empty());

    server.stop(true).await;
}

#[actix_web::test]
async fn external_auth_identity_lookup_uses_namespace_subject_not_provider_id() {
    let state = common::setup().await;
    let provider_a = external_auth_provider_repo::create(
        state.writer_db(),
        external_auth_provider_model("a", "http://issuer.example.test", true),
    )
    .await
    .expect("provider a should create");
    let provider_b = external_auth_provider_repo::create(
        state.writer_db(),
        external_auth_provider_model("b", "http://issuer.example.test", true),
    )
    .await
    .expect("provider b should create");
    let (admin_token, _) = {
        let app = create_test_app!(state.clone());
        register_and_login!(app)
    };
    let claims = aster_drive::services::auth::local::verify_token(
        &admin_token,
        &state.config.auth.jwt_secret,
    )
    .expect("admin token should verify");

    external_auth_identity_repo::create_identity(
        state.writer_db(),
        external_auth_identity_repo::CreateExternalAuthIdentityInput {
            user_id: claims.user_id,
            provider_id: provider_a.id,
            identity_namespace: provider_a
                .issuer_url
                .as_deref()
                .expect("issuer url should exist"),
            subject: "shared-subject",
            email_snapshot: Some("a@example.com"),
            display_name_snapshot: Some("Provider A"),
            now: Utc::now(),
        },
    )
    .await
    .expect("identity should create");

    let found = external_auth_identity_repo::find_by_identity_namespace_subject(
        state.writer_db(),
        provider_b
            .issuer_url
            .as_deref()
            .expect("issuer url should exist"),
        "shared-subject",
    )
    .await
    .expect("identity lookup should succeed")
    .expect("identity should be found by identity namespace+subject");
    assert_eq!(found.provider_id, provider_a.id);

    let duplicate = external_auth_identity_repo::create_identity(
        state.writer_db(),
        external_auth_identity_repo::CreateExternalAuthIdentityInput {
            user_id: claims.user_id,
            provider_id: provider_b.id,
            identity_namespace: provider_b
                .issuer_url
                .as_deref()
                .expect("issuer url should exist"),
            subject: "shared-subject",
            email_snapshot: Some("b@example.com"),
            display_name_snapshot: Some("Provider B"),
            now: Utc::now(),
        },
    )
    .await;
    assert!(duplicate.is_err());
}

#[actix_web::test]
async fn cleanup_expired_flows_removes_only_expired_rows() {
    let state = common::setup().await;
    let provider = external_auth_provider_repo::create(
        state.writer_db(),
        external_auth_provider_model("cleanup", "http://cleanup.example.test", true),
    )
    .await
    .expect("provider should create");
    let now = Utc::now();
    for (state_hash, expires_at) in [
        ("expired", now - Duration::minutes(1)),
        ("active", now + Duration::minutes(5)),
    ] {
        external_auth_login_flow_repo::create(
            state.writer_db(),
            external_auth_login_flow::ActiveModel {
                provider_id: Set(provider.id),
                state_hash: Set(state_hash.to_string()),
                nonce: Set(Some(format!("{state_hash}-nonce"))),
                pkce_verifier: Set(Some(format!("{state_hash}-verifier"))),
                redirect_uri: Set("http://localhost/callback".to_string()),
                return_path: Set(Some("/".to_string())),
                created_at: Set(now),
                expires_at: Set(expires_at),
                consumed_at: Set(None),
                ..Default::default()
            },
        )
        .await
        .expect("flow should create");
    }
    for (flow_token_hash, expires_at) in [
        ("expired-email-flow", now - Duration::minutes(1)),
        ("active-email-flow", now + Duration::minutes(5)),
    ] {
        external_auth_email_verification_flow::ActiveModel {
            provider_id: Set(provider.id),
            identity_namespace: Set("http://cleanup.example.test".to_string()),
            subject: Set(format!("{flow_token_hash}-subject")),
            target_email: Set(None),
            display_name_snapshot: Set(None),
            preferred_username_snapshot: Set(None),
            return_path: Set(Some("/".to_string())),
            flow_token_hash: Set(flow_token_hash.to_string()),
            verification_token_hash: Set(None),
            email_requested_at: Set(None),
            created_at: Set(now),
            expires_at: Set(expires_at),
            consumed_at: Set(None),
            ..Default::default()
        }
        .insert(state.writer_db())
        .await
        .expect("email verification flow should create");
    }

    let removed = external::cleanup_expired_flows(&state)
        .await
        .expect("cleanup should succeed");
    assert_eq!(removed, 2);
    let flows = external_auth_login_flow::Entity::find()
        .all(state.writer_db())
        .await
        .expect("flows should query");
    assert_eq!(flows.len(), 1);
    assert_eq!(flows[0].state_hash, "active");
    let email_flows = external_auth_email_verification_flow::Entity::find()
        .all(state.writer_db())
        .await
        .expect("email verification flows should query");
    assert_eq!(email_flows.len(), 1);
    assert_eq!(email_flows[0].flow_token_hash, "active-email-flow");
}

/// Dex 容器端到端 smoke：真实 discovery/JWKS/auth-code/token 交换链路。
///
#[actix_web::test]
async fn dex_container_authorization_code_login_e2e() {
    use testcontainers::{
        GenericImage, ImageExt,
        core::{IntoContainerPort, WaitFor},
        runners::AsyncRunner,
    };

    let (dex_port, listener) = reserve_localhost_port();
    let dex_issuer = format!("http://127.0.0.1:{dex_port}");

    let state = common::setup().await;
    configure_oidc_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    let provider_key = create_external_auth_provider_with_key(
        &app,
        &admin_token,
        TestOidcProviderOptions {
            auto_provision_enabled: true,
            ..TestOidcProviderOptions::mock(&dex_issuer)
        },
    )
    .await;
    let config = dex_config(&dex_issuer, &provider_key);
    drop(listener);

    let _container = GenericImage::new("ghcr.io/dexidp/dex", DEX_TEST_IMAGE_TAG)
        .with_wait_for(WaitFor::message_on_either_std("listening on"))
        .with_mapped_port(dex_port, 5556.tcp())
        .with_copy_to("/etc/dex/config.asterdrive-test.yaml", config.into_bytes())
        .with_cmd(["dex", "serve", "/etc/dex/config.asterdrive-test.yaml"])
        .start()
        .await
        .expect("failed to start Dex OIDC container");
    wait_for_dex_discovery(&dex_issuer).await;

    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/v1/auth/external-auth/oidc/{provider_key}/start"
        ))
        .insert_header(("Origin", TEST_BROWSER_ORIGIN))
        .set_json(serde_json::json!({ "return_path": "/settings/security" }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    let authorization_url = body["data"]["authorization_url"]
        .as_str()
        .expect("authorization url should be returned");

    let callback_url =
        complete_dex_password_login(&dex_issuer, &provider_key, authorization_url).await;
    let parsed_callback = reqwest::Url::parse(&callback_url).expect("callback URL should parse");
    let callback_path_and_query = parsed_callback[url::Position::BeforePath..].to_string();
    let req = test::TestRequest::get()
        .uri(&callback_path_and_query)
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 302);
    assert_eq!(
        resp.headers()
            .get("Location")
            .and_then(|value| value.to_str().ok()),
        Some("http://localhost:8080/settings/security")
    );
    assert!(common::extract_cookie(&resp, "aster_access").is_some());
    assert!(common::extract_cookie(&resp, "aster_refresh").is_some());
    assert!(common::extract_cookie(&resp, "aster_csrf").is_some());

    let identities = external_auth_identity::Entity::find()
        .all(state.writer_db())
        .await
        .expect("identities should query");
    assert_eq!(identities.len(), 1);
    assert_eq!(identities[0].identity_namespace, dex_issuer);
    assert_eq!(identities[0].subject, DEX_TEST_USER_SUBJECT);
    assert_eq!(
        identities[0].email_snapshot.as_deref(),
        Some(DEX_TEST_USER_EMAIL)
    );
}
