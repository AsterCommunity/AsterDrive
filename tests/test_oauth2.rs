//! 集成测试：`generic_oauth2` 外部认证 provider。

#[macro_use]
mod common;

mod external_auth;

use actix_web::http::StatusCode;
use actix_web::test;
use aster_drive::db::repository::external_auth_provider_repo;
use aster_drive::entities::{external_auth_identity, user};
use aster_drive::runtime::SharedRuntimeState;
use external_auth::oauth2::*;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait, IntoActiveModel};
use serde_json::Value;

#[actix_web::test]
async fn admin_provider_kind_api_includes_generic_oauth2_contract() {
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
    let oauth2 = kinds
        .iter()
        .find(|kind| kind["kind"] == "generic_oauth2")
        .expect("generic OAuth2 kind should be listed");
    assert_eq!(oauth2["protocol"], "oauth2");
    assert_eq!(oauth2["create_defaults"]["display_name"], "");
    assert_eq!(oauth2["create_defaults"]["options"], serde_json::json!({}));
    assert_eq!(oauth2["create_defaults"]["require_email_verified"], true);
    assert_eq!(oauth2["default_scopes"], "openid email profile");
    assert_eq!(oauth2["issuer_url_supported"], true);
    assert_eq!(oauth2["issuer_url_required"], false);
    assert_eq!(oauth2["manual_endpoint_configuration_supported"], true);
    assert_eq!(oauth2["authorization_url_required"], true);
    assert_eq!(oauth2["token_url_required"], true);
    assert_eq!(oauth2["userinfo_url_required"], true);
    assert_eq!(oauth2["supports_discovery"], false);
    assert_eq!(oauth2["supports_pkce"], true);

    let github = kinds
        .iter()
        .find(|kind| kind["kind"] == "github")
        .expect("GitHub kind should be listed");
    assert_eq!(github["protocol"], "oauth2");
    assert_eq!(github["create_defaults"]["display_name"], "GitHub");
    assert_eq!(github["create_defaults"]["require_email_verified"], true);
    assert_eq!(github["issuer_url_supported"], false);
    assert_eq!(github["default_scopes"], "read:user user:email");
    assert_eq!(github["issuer_url_required"], false);
    assert_eq!(github["manual_endpoint_configuration_supported"], false);
    assert_eq!(github["authorization_url_required"], false);
    assert_eq!(github["token_url_required"], false);
    assert_eq!(github["userinfo_url_required"], false);
    assert_eq!(github["supports_discovery"], false);
    assert_eq!(github["supports_pkce"], true);
    assert_eq!(github["supports_email_verified_claim"], false);

    let qq = kinds
        .iter()
        .find(|kind| kind["kind"] == "qq")
        .expect("QQ kind should be listed");
    assert_eq!(qq["protocol"], "oauth2");
    assert_eq!(qq["create_defaults"]["display_name"], "QQ");
    assert_eq!(qq["create_defaults"]["require_email_verified"], false);
    assert_eq!(qq["issuer_url_supported"], false);
    assert_eq!(qq["default_scopes"], "get_user_info");
    assert_eq!(qq["issuer_url_required"], false);
    assert_eq!(qq["manual_endpoint_configuration_supported"], false);
    assert_eq!(qq["authorization_url_required"], false);
    assert_eq!(qq["token_url_required"], false);
    assert_eq!(qq["userinfo_url_required"], false);
    assert_eq!(qq["supports_discovery"], false);
    assert_eq!(qq["supports_pkce"], true);
    assert_eq!(qq["supports_email_verified_claim"], false);
}

#[actix_web::test]
async fn admin_create_and_test_generic_oauth2_provider_requires_manual_endpoints() {
    let (mock_provider, server) = start_mock_oauth2_provider().await;
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (admin_token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/external-auth/providers")
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({
            "provider_kind": "generic_oauth2",
            "display_name": "Broken OAuth2",
            "client_id": TEST_CLIENT_ID
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);

    let created = create_oauth2_provider_with(
        &app,
        &admin_token,
        TestOAuth2ProviderOptions::mock(&mock_provider.base_url),
    )
    .await;
    assert_eq!(created["data"]["provider_kind"], "generic_oauth2");
    assert_eq!(created["data"]["protocol"], "oauth2");
    assert_eq!(created["data"]["issuer_url"], Value::Null);
    assert_eq!(created["data"]["client_secret"], "***REDACTED***");
    assert_eq!(created["data"]["client_secret_configured"], true);
    assert_eq!(created["data"]["scopes"], "read:user user:email");

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/external-auth/providers/test")
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({
            "provider_kind": "generic_oauth2",
            "authorization_url": format!("{}/authorize", mock_provider.base_url),
            "token_url": format!("{}/token", mock_provider.base_url),
            "userinfo_url": format!("{}/userinfo", mock_provider.base_url),
            "client_id": TEST_CLIENT_ID,
            "client_secret": TEST_CLIENT_SECRET,
            "scopes": "read:user user:email"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["provider"], "Generic OAuth2");
    assert_eq!(body["data"]["issuer"], Value::Null);
    assert_eq!(
        body["data"]["authorization_endpoint"],
        format!("{}/authorize", mock_provider.base_url)
    );
    assert_eq!(body["data"]["jwks_key_count"], Value::Null);
    assert_eq!(body["data"]["checks"][0]["name"], "manual_endpoints");
    assert_eq!(body["data"]["checks"][1]["name"], "authorization_code");

    server.stop(true).await;
}

#[actix_web::test]
async fn admin_create_and_test_github_provider_uses_fixed_endpoints() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (admin_token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/external-auth/providers")
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({
            "provider_kind": "github",
            "display_name": "GitHub",
            "authorization_url": "https://github.example.test/login/oauth/authorize",
            "client_id": TEST_CLIENT_ID
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/external-auth/providers")
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({
            "provider_kind": "github",
            "display_name": "GitHub",
            "client_id": TEST_CLIENT_ID,
            "client_secret": TEST_CLIENT_SECRET
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["provider_kind"], "github");
    assert_eq!(body["data"]["protocol"], "oauth2");
    assert_eq!(body["data"]["authorization_url"], Value::Null);
    assert_eq!(body["data"]["token_url"], Value::Null);
    assert_eq!(body["data"]["userinfo_url"], Value::Null);
    assert_eq!(body["data"]["scopes"], "read:user user:email");

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/external-auth/providers/test")
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({
            "provider_kind": "github",
            "client_id": TEST_CLIENT_ID,
            "client_secret": TEST_CLIENT_SECRET
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["provider"], "GitHub");
    assert_eq!(
        body["data"]["authorization_endpoint"],
        "https://github.com/login/oauth/authorize"
    );
    assert_eq!(
        body["data"]["token_endpoint"],
        "https://github.com/login/oauth/access_token"
    );
    assert_eq!(
        body["data"]["userinfo_endpoint"],
        "https://api.github.com/user"
    );
    assert_eq!(body["data"]["checks"][0]["name"], "github_endpoints");
    assert_eq!(body["data"]["checks"][1]["name"], "verified_primary_email");
}

#[actix_web::test]
async fn admin_create_and_test_qq_provider_uses_fixed_endpoints_and_defaults() {
    let state = common::setup().await;
    let app = create_test_app!(state);
    let (admin_token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/external-auth/providers")
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({
            "provider_kind": "qq",
            "display_name": "QQ",
            "authorization_url": "https://qq.example.test/oauth2.0/authorize",
            "client_id": TEST_CLIENT_ID
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/external-auth/providers")
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({
            "provider_kind": "qq",
            "display_name": "QQ",
            "client_id": TEST_CLIENT_ID,
            "client_secret": TEST_CLIENT_SECRET
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["provider_kind"], "qq");
    assert_eq!(body["data"]["protocol"], "oauth2");
    assert_eq!(body["data"]["authorization_url"], Value::Null);
    assert_eq!(body["data"]["token_url"], Value::Null);
    assert_eq!(body["data"]["userinfo_url"], Value::Null);
    assert_eq!(body["data"]["scopes"], "get_user_info");
    assert_eq!(body["data"]["require_email_verified"], false);
    assert_eq!(body["data"]["auto_link_verified_email_enabled"], false);

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/external-auth/providers/test")
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({
            "provider_kind": "qq",
            "client_id": TEST_CLIENT_ID,
            "client_secret": TEST_CLIENT_SECRET
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 200);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["provider"], "QQ");
    assert_eq!(body["data"]["issuer"], format!("qq:{TEST_CLIENT_ID}"));
    assert_eq!(
        body["data"]["authorization_endpoint"],
        "https://graph.qq.com/oauth2.0/authorize"
    );
    assert_eq!(
        body["data"]["token_endpoint"],
        "https://graph.qq.com/oauth2.0/token"
    );
    assert_eq!(
        body["data"]["userinfo_endpoint"],
        "https://graph.qq.com/user/get_user_info"
    );
    assert_eq!(body["data"]["checks"][0]["name"], "qq_endpoints");
    assert_eq!(body["data"]["checks"][1]["name"], "qq_openid");
}

#[actix_web::test]
async fn start_login_builds_oauth2_authorization_url_with_default_scopes_without_nonce() {
    let (mock_provider, server) = start_mock_oauth2_provider().await;
    let state = common::setup().await;
    configure_oauth2_public_site_url(&state);
    let app = create_test_app!(state);
    let (admin_token, _) = register_and_login!(app);
    let created = create_oauth2_provider_with(
        &app,
        &admin_token,
        TestOAuth2ProviderOptions::mock(&mock_provider.base_url),
    )
    .await;
    let provider_key = created_provider_key(&created);

    let state_value = start_oauth2_login(&app, &mock_provider, &provider_key, "/files").await;
    let authorize_request = mock_provider.last_authorize_request();
    assert_eq!(authorize_request.response_type, "code");
    assert_eq!(authorize_request.client_id, TEST_CLIENT_ID);
    assert_eq!(
        authorize_request.redirect_uri,
        format!(
            "http://localhost:8080/api/v1/auth/external-auth/generic_oauth2/{provider_key}/callback"
        )
    );
    assert_eq!(
        authorize_request.scope.as_deref(),
        Some("read:user user:email")
    );
    assert_eq!(authorize_request.state, state_value);
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
    assert_eq!(authorize_request.nonce, None);

    server.stop(true).await;
}

#[actix_web::test]
async fn create_provider_without_scopes_uses_logto_compatible_oauth2_default() {
    let (mock_provider, server) = start_mock_oauth2_provider().await;
    let state = common::setup().await;
    configure_oauth2_public_site_url(&state);
    let app = create_test_app!(state);
    let (admin_token, _) = register_and_login!(app);

    let req = test::TestRequest::post()
        .uri("/api/v1/admin/external-auth/providers")
        .insert_header(("Cookie", common::access_cookie_header(&admin_token)))
        .insert_header(common::csrf_header_for(&admin_token))
        .set_json(serde_json::json!({
            "provider_kind": "generic_oauth2",
            "display_name": "Default Scope OAuth2",
            "authorization_url": format!("{}/authorize", mock_provider.base_url),
            "token_url": format!("{}/token", mock_provider.base_url),
            "userinfo_url": format!("{}/userinfo", mock_provider.base_url),
            "client_id": TEST_CLIENT_ID,
            "client_secret": TEST_CLIENT_SECRET
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 201);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["data"]["scopes"], "openid email profile");
    let provider_key = created_provider_key(&body);

    start_oauth2_login(&app, &mock_provider, &provider_key, "/").await;
    let authorize_request = mock_provider.last_authorize_request();
    assert_eq!(
        authorize_request.scope.as_deref(),
        Some("openid email profile")
    );
    assert_eq!(authorize_request.nonce, None);

    server.stop(true).await;
}

#[actix_web::test]
async fn finish_callback_exchanges_code_fetches_userinfo_and_issues_cookies() {
    let (mock_provider, server) = start_mock_oauth2_provider().await;
    let state = common::setup().await;
    configure_oauth2_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    let created = create_oauth2_provider_with(
        &app,
        &admin_token,
        TestOAuth2ProviderOptions {
            auto_provision_enabled: true,
            ..TestOAuth2ProviderOptions::mock(&mock_provider.base_url)
        },
    )
    .await;
    let provider_key = created_provider_key(&created);

    let state_value =
        start_oauth2_login(&app, &mock_provider, &provider_key, "/settings/security").await;
    let resp = finish_oauth2_callback(&app, &provider_key, &state_value).await;
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
    assert_eq!(identities[0].identity_namespace, mock_provider.base_url);
    assert_eq!(identities[0].subject, "oauth2-subject-1");
    assert_eq!(
        identities[0].email_snapshot.as_deref(),
        Some("oauth2-user@example.com")
    );
    assert_eq!(
        mock_provider.token_auth_observations(),
        vec![TokenAuthObservation::Post]
    );

    server.stop(true).await;
}

#[actix_web::test]
async fn local_email_policy_does_not_apply_to_oauth2_auto_provision() {
    let (mock_provider, server) = start_mock_oauth2_provider().await;
    let state = common::setup().await;
    configure_oauth2_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    state.runtime_config.apply(common::system_config_model(
        aster_drive::config::local_email_policy::AUTH_LOCAL_EMAIL_ALLOWLIST_KEY,
        r#"["other.test"]"#,
    ));
    state.runtime_config.apply(common::system_config_model(
        aster_drive::config::local_email_policy::AUTH_LOCAL_EMAIL_BLOCKLIST_KEY,
        r#"["example.com"]"#,
    ));
    let created = create_oauth2_provider_with(
        &app,
        &admin_token,
        TestOAuth2ProviderOptions {
            auto_provision_enabled: true,
            allowed_domains: vec!["example.com".to_string()],
            ..TestOAuth2ProviderOptions::mock(&mock_provider.base_url)
        },
    )
    .await;
    let provider_key = created_provider_key(&created);

    let state_value = start_oauth2_login(&app, &mock_provider, &provider_key, "/").await;
    let resp = finish_oauth2_callback(&app, &provider_key, &state_value).await;
    assert_eq!(resp.status(), 302);
    assert!(common::extract_cookie(&resp, "aster_access").is_some());

    let provisioned = user::Entity::find()
        .all(state.writer_db())
        .await
        .expect("users should query");
    assert!(
        provisioned
            .iter()
            .any(|user| user.email == "oauth2-user@example.com"),
        "OAuth2 auto-provision should not be denied by local email policy"
    );

    server.stop(true).await;
}

#[actix_web::test]
async fn github_callback_uses_verified_primary_email_from_email_list() {
    let (mock_provider, server) = start_mock_oauth2_provider().await;
    mock_provider.set_email(None);
    mock_provider.set_email_verified(None);
    mock_provider.set_github_emails(vec![
        GitHubEmailEntry {
            email: "secondary@example.com".to_string(),
            primary: false,
            verified: true,
        },
        GitHubEmailEntry {
            email: "primary-unverified@example.com".to_string(),
            primary: true,
            verified: false,
        },
        GitHubEmailEntry {
            email: "github-primary@example.com".to_string(),
            primary: true,
            verified: true,
        },
    ]);

    let state = common::setup().await;
    configure_oauth2_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let provider_model =
        github_external_auth_provider_model("github-mock", &mock_provider.base_url, true)
            .insert(state.writer_db())
            .await
            .expect("GitHub provider should insert");

    let state_value = start_github_login(&app, &mock_provider, &provider_model.key, "/files").await;
    let authorize_request = mock_provider.last_authorize_request();
    assert_eq!(
        authorize_request.redirect_uri,
        format!(
            "http://localhost:8080/api/v1/auth/external-auth/github/{}/callback",
            provider_model.key
        )
    );
    assert_eq!(
        authorize_request.scope.as_deref(),
        Some("read:user user:email")
    );

    let resp = finish_github_callback(&app, &provider_model.key, &state_value).await;
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
    assert_eq!(identities[0].identity_namespace, mock_provider.base_url);
    assert_eq!(identities[0].subject, "oauth2-subject-1");
    assert_eq!(
        identities[0].email_snapshot.as_deref(),
        Some("github-primary@example.com")
    );

    server.stop(true).await;
}

#[actix_web::test]
async fn github_require_email_verified_rejects_without_verified_primary_email() {
    let (mock_provider, server) = start_mock_oauth2_provider().await;
    mock_provider.set_email(Some("public@example.com"));
    mock_provider.set_email_verified(Some(true));
    mock_provider.set_github_emails(vec![
        GitHubEmailEntry {
            email: "secondary@example.com".to_string(),
            primary: false,
            verified: true,
        },
        GitHubEmailEntry {
            email: "primary-unverified@example.com".to_string(),
            primary: true,
            verified: false,
        },
    ]);

    let state = common::setup().await;
    configure_oauth2_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let provider_model =
        github_external_auth_provider_model("github-no-email", &mock_provider.base_url, true)
            .insert(state.writer_db())
            .await
            .expect("GitHub provider should insert");

    let state_value = start_github_login(&app, &mock_provider, &provider_model.key, "/").await;
    let resp = finish_github_callback(&app, &provider_model.key, &state_value).await;
    assert_oauth2_error_redirect(&resp);
    let identities = external_auth_identity::Entity::find()
        .all(state.writer_db())
        .await
        .expect("identities should query");
    assert!(identities.is_empty());

    server.stop(true).await;
}

#[actix_web::test]
async fn github_existing_identity_can_login_without_verified_primary_email() {
    let (mock_provider, server) = start_mock_oauth2_provider().await;
    mock_provider.set_email(Some("public@example.com"));
    mock_provider.set_email_verified(Some(true));
    mock_provider.set_github_emails(Vec::new());

    let state = common::setup().await;
    configure_oauth2_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    let linked_user_id = admin_create_user!(
        app,
        admin_token,
        "gh-linked",
        "github-linked@example.com",
        "password123"
    );
    let provider_model =
        github_external_auth_provider_model("github-bound-no-email", &mock_provider.base_url, true)
            .insert(state.writer_db())
            .await
            .expect("GitHub provider should insert");
    external_auth_identity::ActiveModel {
        user_id: Set(linked_user_id),
        provider_id: Set(provider_model.id),
        identity_namespace: Set(mock_provider.base_url.clone()),
        subject: Set("oauth2-subject-1".to_string()),
        email_snapshot: Set(Some("github-linked@example.com".to_string())),
        display_name_snapshot: Set(Some("Linked GitHub User".to_string())),
        created_at: Set(chrono::Utc::now()),
        updated_at: Set(chrono::Utc::now()),
        last_login_at: Set(None),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .expect("identity should insert");

    let state_value = start_github_login(&app, &mock_provider, &provider_model.key, "/files").await;
    let resp = finish_github_callback(&app, &provider_model.key, &state_value).await;
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
async fn github_ignores_public_user_email_when_verified_primary_email_is_missing() {
    let (mock_provider, server) = start_mock_oauth2_provider().await;
    mock_provider.set_email(Some("public@example.com"));
    mock_provider.set_email_verified(Some(true));
    mock_provider.set_github_emails(Vec::new());

    let state = common::setup().await;
    configure_oauth2_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let provider_model =
        github_external_auth_provider_model("github-public-email", &mock_provider.base_url, true)
            .insert(state.writer_db())
            .await
            .expect("GitHub provider should insert");

    let state_value = start_github_login(&app, &mock_provider, &provider_model.key, "/").await;
    let resp = finish_github_callback(&app, &provider_model.key, &state_value).await;
    assert_oauth2_error_redirect(&resp);
    let identities = external_auth_identity::Entity::find()
        .all(state.writer_db())
        .await
        .expect("identities should query");
    assert!(identities.is_empty());

    server.stop(true).await;
}

#[actix_web::test]
async fn github_rejects_invalid_verified_primary_email() {
    let (mock_provider, server) = start_mock_oauth2_provider().await;
    mock_provider.set_email(None);
    mock_provider.set_email_verified(None);
    mock_provider.set_github_emails(vec![GitHubEmailEntry {
        email: "not-an-email".to_string(),
        primary: true,
        verified: true,
    }]);

    let state = common::setup().await;
    configure_oauth2_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let provider_model =
        github_external_auth_provider_model("github-invalid-email", &mock_provider.base_url, true)
            .insert(state.writer_db())
            .await
            .expect("GitHub provider should insert");

    let state_value = start_github_login(&app, &mock_provider, &provider_model.key, "/").await;
    let resp = finish_github_callback(&app, &provider_model.key, &state_value).await;
    assert_oauth2_error_redirect(&resp);
    let identities = external_auth_identity::Entity::find()
        .all(state.writer_db())
        .await
        .expect("identities should query");
    assert!(identities.is_empty());

    server.stop(true).await;
}

#[actix_web::test]
async fn github_rejects_user_emails_api_failure() {
    let (mock_provider, server) = start_mock_oauth2_provider().await;
    mock_provider.set_github_emails_status(StatusCode::FORBIDDEN);

    let state = common::setup().await;
    configure_oauth2_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let provider_model =
        github_external_auth_provider_model("github-email-api-fail", &mock_provider.base_url, true)
            .insert(state.writer_db())
            .await
            .expect("GitHub provider should insert");

    let state_value = start_github_login(&app, &mock_provider, &provider_model.key, "/").await;
    let resp = finish_github_callback(&app, &provider_model.key, &state_value).await;
    assert_oauth2_error_redirect(&resp);
    let identities = external_auth_identity::Entity::find()
        .all(state.writer_db())
        .await
        .expect("identities should query");
    assert!(identities.is_empty());

    server.stop(true).await;
}

#[actix_web::test]
async fn qq_callback_fetches_openid_and_userinfo_then_requires_email_verification() {
    let (mock_provider, server) = start_mock_oauth2_provider().await;
    let state = common::setup().await;
    configure_oauth2_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let provider_model =
        qq_external_auth_provider_model("qq-mock", &mock_provider.base_url, true, false)
            .insert(state.writer_db())
            .await
            .expect("QQ provider should insert");

    let state_value = start_qq_login(&app, &mock_provider, &provider_model.key, "/files").await;
    let authorize_request = mock_provider.last_authorize_request();
    assert_eq!(authorize_request.response_type, "code");
    assert_eq!(authorize_request.client_id, TEST_CLIENT_ID);
    assert_eq!(
        authorize_request.redirect_uri,
        format!(
            "http://localhost:8080/api/v1/auth/external-auth/qq/{}/callback",
            provider_model.key
        )
    );
    assert_eq!(authorize_request.scope.as_deref(), Some("get_user_info"));
    assert_eq!(authorize_request.state, state_value);
    assert_eq!(
        authorize_request.code_challenge_method.as_deref(),
        Some("S256")
    );

    let resp = finish_qq_callback(&app, &provider_model.key, &state_value).await;
    let flow_token = oauth2_email_required_flow(&resp);
    assert!(!flow_token.is_empty());
    let identities = external_auth_identity::Entity::find()
        .all(state.writer_db())
        .await
        .expect("identities should query");
    assert!(identities.is_empty());
    assert_eq!(
        mock_provider.token_auth_observations(),
        vec![TokenAuthObservation::Post]
    );

    server.stop(true).await;
}

#[actix_web::test]
async fn qq_existing_identity_can_login_without_email() {
    let (mock_provider, server) = start_mock_oauth2_provider().await;
    let state = common::setup().await;
    configure_oauth2_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    let linked_user_id = admin_create_user!(
        app,
        admin_token,
        "qq-linked",
        "qq-linked@example.com",
        "password123"
    );
    let provider_model =
        qq_external_auth_provider_model("qq-bound-no-email", &mock_provider.base_url, true, true)
            .insert(state.writer_db())
            .await
            .expect("QQ provider should insert");
    external_auth_identity::ActiveModel {
        user_id: Set(linked_user_id),
        provider_id: Set(provider_model.id),
        identity_namespace: Set(format!("qq:{TEST_CLIENT_ID}")),
        subject: Set("qq-openid-1".to_string()),
        email_snapshot: Set(None),
        display_name_snapshot: Set(Some("Linked QQ User".to_string())),
        created_at: Set(chrono::Utc::now()),
        updated_at: Set(chrono::Utc::now()),
        last_login_at: Set(None),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .expect("identity should insert");

    let state_value = start_qq_login(&app, &mock_provider, &provider_model.key, "/files").await;
    let resp = finish_qq_callback(&app, &provider_model.key, &state_value).await;
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
async fn qq_rejects_openid_client_id_mismatch() {
    let (mock_provider, server) = start_mock_oauth2_provider().await;
    mock_provider.set_qq_openid_client_id("different-client");
    let state = common::setup().await;
    configure_oauth2_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let provider_model =
        qq_external_auth_provider_model("qq-client-mismatch", &mock_provider.base_url, true, false)
            .insert(state.writer_db())
            .await
            .expect("QQ provider should insert");

    let state_value = start_qq_login(&app, &mock_provider, &provider_model.key, "/").await;
    let resp = finish_qq_callback(&app, &provider_model.key, &state_value).await;
    assert_oauth2_error_redirect(&resp);

    server.stop(true).await;
}

#[actix_web::test]
async fn qq_rejects_missing_openid_and_userinfo_errors() {
    let (mock_provider, server) = start_mock_oauth2_provider().await;
    mock_provider.set_qq_openid("");
    let state = common::setup().await;
    configure_oauth2_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let provider_model =
        qq_external_auth_provider_model("qq-missing-openid", &mock_provider.base_url, true, false)
            .insert(state.writer_db())
            .await
            .expect("QQ provider should insert");

    let state_value = start_qq_login(&app, &mock_provider, &provider_model.key, "/").await;
    let resp = finish_qq_callback(&app, &provider_model.key, &state_value).await;
    assert_oauth2_error_redirect(&resp);

    server.stop(true).await;

    let (mock_provider, server) = start_mock_oauth2_provider().await;
    mock_provider.set_qq_userinfo_error(100030, "mock userinfo error");
    let state = common::setup().await;
    configure_oauth2_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let provider_model =
        qq_external_auth_provider_model("qq-userinfo-error", &mock_provider.base_url, true, false)
            .insert(state.writer_db())
            .await
            .expect("QQ provider should insert");

    let state_value = start_qq_login(&app, &mock_provider, &provider_model.key, "/").await;
    let resp = finish_qq_callback(&app, &provider_model.key, &state_value).await;
    assert_oauth2_error_redirect(&resp);

    server.stop(true).await;
}

#[actix_web::test]
async fn finish_callback_uses_single_client_secret_post_token_request() {
    let (mock_provider, server) = start_mock_oauth2_provider().await;
    let state = common::setup().await;
    configure_oauth2_public_site_url(&state);
    let app = create_test_app!(state);
    let (admin_token, _) = register_and_login!(app);
    let created = create_oauth2_provider_with(
        &app,
        &admin_token,
        TestOAuth2ProviderOptions {
            auto_provision_enabled: true,
            ..TestOAuth2ProviderOptions::mock(&mock_provider.base_url)
        },
    )
    .await;
    let provider_key = created_provider_key(&created);

    let state_value = start_oauth2_login(&app, &mock_provider, &provider_key, "/").await;
    let resp = finish_oauth2_callback(&app, &provider_key, &state_value).await;
    assert_eq!(resp.status(), 302);
    assert!(common::extract_cookie(&resp, "aster_access").is_some());
    assert_eq!(
        mock_provider.token_auth_observations(),
        vec![TokenAuthObservation::Post]
    );

    server.stop(true).await;
}

#[actix_web::test]
async fn finish_callback_supports_public_client_token_request() {
    let (mock_provider, server) = start_mock_oauth2_provider().await;
    mock_provider.set_expected_token_auth(TokenAuthObservation::None);
    let state = common::setup().await;
    configure_oauth2_public_site_url(&state);
    let app = create_test_app!(state);
    let (admin_token, _) = register_and_login!(app);
    let created = create_oauth2_provider_with(
        &app,
        &admin_token,
        TestOAuth2ProviderOptions {
            auto_provision_enabled: true,
            client_secret: None,
            ..TestOAuth2ProviderOptions::mock(&mock_provider.base_url)
        },
    )
    .await;
    assert_eq!(created["data"]["client_secret"], Value::Null);
    assert_eq!(created["data"]["client_secret_configured"], false);
    let provider_key = created_provider_key(&created);

    let state_value = start_oauth2_login(&app, &mock_provider, &provider_key, "/").await;
    let resp = finish_oauth2_callback(&app, &provider_key, &state_value).await;
    assert_eq!(resp.status(), 302);
    assert!(common::extract_cookie(&resp, "aster_access").is_some());
    assert_eq!(
        mock_provider.token_auth_observations(),
        vec![TokenAuthObservation::None]
    );

    server.stop(true).await;
}

#[actix_web::test]
async fn finish_callback_rejects_state_replay() {
    let (mock_provider, server) = start_mock_oauth2_provider().await;
    let state = common::setup().await;
    configure_oauth2_public_site_url(&state);
    let app = create_test_app!(state);
    let (admin_token, _) = register_and_login!(app);
    let created = create_oauth2_provider_with(
        &app,
        &admin_token,
        TestOAuth2ProviderOptions {
            auto_provision_enabled: true,
            ..TestOAuth2ProviderOptions::mock(&mock_provider.base_url)
        },
    )
    .await;
    let provider_key = created_provider_key(&created);

    let state_value = start_oauth2_login(&app, &mock_provider, &provider_key, "/").await;
    let resp = finish_oauth2_callback(&app, &provider_key, &state_value).await;
    assert_eq!(resp.status(), 302);
    assert!(common::extract_cookie(&resp, "aster_access").is_some());

    let resp = finish_oauth2_callback(&app, &provider_key, &state_value).await;
    assert_oauth2_error_redirect(&resp);
    external_auth::assert_external_auth_binding_cookie_cleared(&resp);

    server.stop(true).await;
}

#[actix_web::test]
async fn parallel_login_flows_keep_independent_browser_bindings() {
    let (mock_provider, server) = start_mock_oauth2_provider().await;
    let state = common::setup().await;
    configure_oauth2_public_site_url(&state);
    let app = create_test_app!(state);
    let (admin_token, _) = register_and_login!(app);
    let created = create_oauth2_provider_with(
        &app,
        &admin_token,
        TestOAuth2ProviderOptions {
            auto_provision_enabled: true,
            ..TestOAuth2ProviderOptions::mock(&mock_provider.base_url)
        },
    )
    .await;
    let provider_key = created_provider_key(&created);

    let first_state = start_oauth2_login(&app, &mock_provider, &provider_key, "/files").await;
    let second_state =
        start_oauth2_login(&app, &mock_provider, &provider_key, "/settings/security").await;
    let first_cookie = external_auth::external_auth_binding_cookie_header(&first_state);
    let second_cookie = external_auth::external_auth_binding_cookie_header(&second_state);
    assert_ne!(
        external_auth::external_auth_binding_cookie_name(&first_cookie),
        external_auth::external_auth_binding_cookie_name(&second_cookie)
    );

    let first_resp = finish_oauth2_callback(&app, &provider_key, &first_state).await;
    assert_eq!(first_resp.status(), 302);
    assert_eq!(
        first_resp
            .headers()
            .get("Location")
            .and_then(|value| value.to_str().ok()),
        Some("http://localhost:8080/files")
    );
    let second_resp = finish_oauth2_callback(&app, &provider_key, &second_state).await;
    assert_eq!(second_resp.status(), 302);
    assert_eq!(
        second_resp
            .headers()
            .get("Location")
            .and_then(|value| value.to_str().ok()),
        Some("http://localhost:8080/settings/security")
    );

    server.stop(true).await;
}

#[actix_web::test]
async fn finish_callback_rejects_userinfo_missing_subject() {
    let (mock_provider, server) = start_mock_oauth2_provider().await;
    mock_provider.set_subject(None);
    let state = common::setup().await;
    configure_oauth2_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    let created = create_oauth2_provider_with(
        &app,
        &admin_token,
        TestOAuth2ProviderOptions {
            auto_provision_enabled: true,
            ..TestOAuth2ProviderOptions::mock(&mock_provider.base_url)
        },
    )
    .await;
    let provider_key = created_provider_key(&created);

    let state_value = start_oauth2_login(&app, &mock_provider, &provider_key, "/").await;
    let resp = finish_oauth2_callback(&app, &provider_key, &state_value).await;
    assert_oauth2_error_redirect(&resp);
    let identities = external_auth_identity::Entity::find()
        .all(state.writer_db())
        .await
        .expect("identities should query");
    assert!(identities.is_empty());

    server.stop(true).await;
}

#[actix_web::test]
async fn unverified_email_does_not_auto_link_existing_user() {
    let (mock_provider, server) = start_mock_oauth2_provider().await;
    mock_provider.set_subject(Some("unverified-auto-link"));
    mock_provider.set_email(Some("linked@example.com"));
    mock_provider.set_email_verified(Some(false));

    let state = common::setup().await;
    configure_oauth2_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    let _linked_user_id = admin_create_user!(
        app,
        admin_token,
        "linked-user",
        "linked@example.com",
        "password123"
    );
    let created = create_oauth2_provider_with(
        &app,
        &admin_token,
        TestOAuth2ProviderOptions {
            auto_link_verified_email_enabled: true,
            require_email_verified: false,
            ..TestOAuth2ProviderOptions::mock(&mock_provider.base_url)
        },
    )
    .await;
    let provider_key = created_provider_key(&created);

    let state_value = start_oauth2_login(&app, &mock_provider, &provider_key, "/").await;
    let resp = finish_oauth2_callback(&app, &provider_key, &state_value).await;
    let flow_token = oauth2_email_required_flow(&resp);
    assert!(!flow_token.is_empty());
    let identities = external_auth_identity::Entity::find()
        .all(state.writer_db())
        .await
        .expect("identities should query");
    assert!(identities.is_empty());

    server.stop(true).await;
}

#[actix_web::test]
async fn finish_callback_rejects_flow_after_provider_disabled() {
    let (mock_provider, server) = start_mock_oauth2_provider().await;
    let state = common::setup().await;
    configure_oauth2_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    let created = create_oauth2_provider_with(
        &app,
        &admin_token,
        TestOAuth2ProviderOptions::mock(&mock_provider.base_url),
    )
    .await;
    let provider_key = created_provider_key(&created);
    let provider_id = created["data"]["id"].as_i64().unwrap();

    let state_value = start_oauth2_login(&app, &mock_provider, &provider_key, "/").await;
    let mut provider = external_auth_provider_repo::find_by_id(state.writer_db(), provider_id)
        .await
        .expect("provider should query")
        .into_active_model();
    provider.enabled = Set(false);
    provider
        .update(state.writer_db())
        .await
        .expect("provider should update");

    let resp = finish_oauth2_callback(&app, &provider_key, &state_value).await;
    assert_oauth2_error_redirect(&resp);

    server.stop(true).await;
}

#[actix_web::test]
async fn finish_callback_rejects_disabled_user_with_existing_identity() {
    let (mock_provider, server) = start_mock_oauth2_provider().await;
    mock_provider.set_subject(Some("disabled-user-subject"));

    let state = common::setup().await;
    configure_oauth2_public_site_url(&state);
    let app = create_test_app!(state.clone());
    let (admin_token, _) = register_and_login!(app);
    let disabled_user_id = admin_create_user!(
        app,
        admin_token,
        "disabled-user",
        "disabled-oauth2@example.com",
        "password123"
    );
    disable_user(&state, disabled_user_id).await;

    let provider_model =
        external_auth_provider_model("oauth2-disabled", &mock_provider.base_url, true)
            .insert(state.writer_db())
            .await
            .expect("provider should insert");
    external_auth_identity::ActiveModel {
        user_id: Set(disabled_user_id),
        provider_id: Set(provider_model.id),
        identity_namespace: Set(mock_provider.base_url.clone()),
        subject: Set("disabled-user-subject".to_string()),
        email_snapshot: Set(Some("disabled-oauth2@example.com".to_string())),
        display_name_snapshot: Set(None),
        created_at: Set(chrono::Utc::now()),
        updated_at: Set(chrono::Utc::now()),
        last_login_at: Set(None),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .expect("identity should insert");

    let state_value = start_oauth2_login(&app, &mock_provider, "oauth2-disabled", "/").await;
    let resp = finish_oauth2_callback(&app, "oauth2-disabled", &state_value).await;
    assert_oauth2_error_redirect(&resp);

    server.stop(true).await;
}
