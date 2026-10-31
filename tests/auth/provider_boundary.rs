//! Drive owns identity/session policy; Forge owns provider endpoints and wire normalization.
//! Test doubles return the normalized provider boundary, never reimplement vendor protocols.

use crate::{common, external_auth};
use actix_web::{body::MessageBody, dev::ServiceResponse, test};
use aster_drive::runtime::PrimaryAppState;
use aster_drive_model::{
    entities::{external_auth_identity, external_auth_provider, user},
    types::external_auth_provider::ExternalAuthCallbackMode,
};
use aster_forge_external_auth::{
    ExternalAuthAuthorizationStart, ExternalAuthCallback, ExternalAuthError, ExternalAuthProfile,
    ExternalAuthProviderConfig, ExternalAuthProviderDescriptor, ExternalAuthProviderDriver,
    ExternalAuthProviderKind as Kind, ExternalAuthProviderRegistry, ExternalAuthProviderTestResult,
    Result, default_registry,
};
use sea_orm::{ActiveModelTrait, EntityTrait, Set};
use serde_json::Value;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

#[derive(Clone)]
struct ProfileDriver {
    kind: Kind,
    reply: Arc<Mutex<std::result::Result<ExternalAuthProfile, &'static str>>>,
    exchanges: Arc<AtomicUsize>,
    probes: Arc<AtomicUsize>,
}

impl ProfileDriver {
    fn new(kind: Kind) -> Self {
        Self {
            kind,
            reply: Arc::new(Mutex::new(Ok(ExternalAuthProfile {
                identity_namespace: format!("provider://{}", kind.as_str()),
                subject: "stable-subject".into(),
                email: Some("provider-user@example.com".into()),
                email_verified: true,
                display_name: Some("Provider User".into()),
                preferred_username: Some("provider-user".into()),
            }))),
            exchanges: Arc::new(AtomicUsize::new(0)),
            probes: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn registry(&self) -> ExternalAuthProviderRegistry {
        let mut registry = ExternalAuthProviderRegistry::default();
        registry.register(self.clone()).unwrap();
        registry
    }

    fn profile(&self, change: impl FnOnce(&mut ExternalAuthProfile)) {
        change(self.reply.lock().unwrap().as_mut().unwrap());
    }
}

#[async_trait::async_trait]
impl ExternalAuthProviderDriver for ProfileDriver {
    fn kind(&self) -> Kind {
        self.kind
    }

    fn descriptor(&self) -> ExternalAuthProviderDescriptor {
        default_registry().descriptor_for(self.kind).unwrap()
    }

    async fn start_authorization(
        &self,
        provider: &ExternalAuthProviderConfig,
        redirect_uri: &str,
    ) -> Result<ExternalAuthAuthorizationStart> {
        assert_eq!(provider.provider_kind, self.kind);
        let state = uuid::Uuid::new_v4().to_string();
        let mut url = reqwest::Url::parse("https://provider.invalid/authorize").unwrap();
        url.query_pairs_mut()
            .append_pair("state", &state)
            .append_pair("redirect_uri", redirect_uri);
        Ok(ExternalAuthAuthorizationStart {
            authorization_url: url.into(),
            state,
            nonce: Some("boundary-nonce".into()),
            pkce_verifier: Some("boundary-pkce".into()),
        })
    }

    async fn exchange_callback(
        &self,
        provider: &ExternalAuthProviderConfig,
        callback: ExternalAuthCallback,
    ) -> Result<ExternalAuthProfile> {
        assert_eq!(provider.provider_kind, self.kind);
        assert_eq!(callback.code, "mock-code");
        assert_eq!(callback.nonce.as_deref(), Some("boundary-nonce"));
        assert_eq!(callback.pkce_verifier.as_deref(), Some("boundary-pkce"));
        assert_eq!(
            callback.redirect_uri,
            "http://localhost:8080/api/v1/auth/external-auth/callback"
        );
        self.exchanges.fetch_add(1, Ordering::SeqCst);
        self.reply
            .lock()
            .unwrap()
            .clone()
            .map_err(ExternalAuthError::auth_invalid_credentials)
    }

    async fn test_provider(
        &self,
        provider: &ExternalAuthProviderConfig,
    ) -> Result<ExternalAuthProviderTestResult> {
        assert_eq!(provider.provider_kind, self.kind);
        self.probes.fetch_add(1, Ordering::SeqCst);
        Ok(ExternalAuthProviderTestResult {
            provider: self.descriptor().display_name.into(),
            issuer: Some("provider://test".into()),
            authorization_endpoint: None,
            token_endpoint: None,
            userinfo_endpoint: None,
            jwks_key_count: None,
            checks: vec![],
        })
    }
}

async fn provider(
    state: &PrimaryAppState,
    kind: Kind,
    key: &str,
    require_email: bool,
) -> external_auth_provider::Model {
    let mut model =
        external_auth::oauth2::external_auth_provider_model(key, "https://unused.invalid", true);
    model.provider_kind = Set(kind);
    model.protocol = Set(default_registry().descriptor_for(kind).unwrap().protocol);
    model.issuer_url = Set(None);
    model.authorization_url = Set(None);
    model.token_url = Set(None);
    model.userinfo_url = Set(None);
    model.callback_mode = Set(ExternalAuthCallbackMode::Unified);
    model.auto_provision_enabled = Set(true);
    model.require_email_verified = Set(require_email);
    model.insert(state.writer_db()).await.unwrap()
}

async fn begin<S, B, E>(app: &S, kind: Kind, key: &str) -> String
where
    S: actix_web::dev::Service<actix_http::Request, Response = ServiceResponse<B>, Error = E>,
    B: MessageBody,
    E: std::fmt::Debug,
{
    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/v1/auth/external-auth/{}/{key}/start",
            kind.as_str()
        ))
        .insert_header(("Origin", "http://localhost:8080"))
        .set_json(serde_json::json!({"return_path":"/files"}))
        .to_request();
    let resp = test::call_service(app, req).await;
    assert_eq!(resp.status(), 200);
    let binding = external_auth::external_auth_binding_cookie_from_response(&resp);
    let body: Value = test::read_body_json(resp).await;
    let url = reqwest::Url::parse(body["data"]["authorization_url"].as_str().unwrap()).unwrap();
    let state = url
        .query_pairs()
        .find(|(key, _)| key == "state")
        .unwrap()
        .1
        .into_owned();
    external_auth::remember_external_auth_binding(&state, binding);
    state
}

#[actix_web::test]
async fn builtins_ignore_persisted_endpoint_overrides_without_fetching_them() {
    for (kind, expected) in [
        (Kind::GitHub, "https://github.com/login/oauth/authorize"),
        (Kind::Qq, "https://graph.qq.com/oauth2.0/authorize"),
    ] {
        let state = common::setup().await;
        external_auth::oauth2::configure_oauth2_public_site_url(&state);
        let mut stored = external_auth::oauth2::external_auth_provider_model(
            "fixed",
            "http://127.0.0.1:1",
            true,
        );
        stored.provider_kind = Set(kind);
        stored.insert(state.writer_db()).await.unwrap();
        // No injected driver: exercise Forge's production endpoint policy.
        let app = create_test_app!(state);
        let req = test::TestRequest::post()
            .uri(&format!(
                "/api/v1/auth/external-auth/{}/fixed/start",
                kind.as_str()
            ))
            .insert_header(("Origin", "http://localhost:8080"))
            .set_json(serde_json::json!({}))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let body: Value = test::read_body_json(resp).await;
        let url = reqwest::Url::parse(body["data"]["authorization_url"].as_str().unwrap()).unwrap();
        assert_eq!(
            format!(
                "{}://{}{}",
                url.scheme(),
                url.host_str().unwrap(),
                url.path()
            ),
            expected
        );
        assert!(
            url.query_pairs()
                .any(|(key, value)| key == "state" && !value.is_empty())
        );
        assert!(
            url.query_pairs()
                .any(|(key, value)| key == "code_challenge" && !value.is_empty())
        );
    }
}

#[actix_web::test]
async fn normalized_profiles_provision_identity_and_reject_state_replay() {
    for kind in [Kind::GitHub, Kind::Google] {
        let state = common::setup().await;
        external_auth::oauth2::configure_oauth2_public_site_url(&state);
        let driver = ProfileDriver::new(kind);
        let app = create_test_app!(state.clone(), driver.registry());
        let _ = register_and_login!(app);
        provider(&state, kind, "profile", true).await;
        let flow = begin(&app, kind, "profile").await;
        let resp = external_auth::finish_unified_external_auth_callback(&app, &flow).await;
        assert!(common::extract_cookie(&resp, "aster_access").is_some());
        let rows = external_auth_identity::Entity::find()
            .all(state.writer_db())
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].identity_namespace,
            format!("provider://{}", kind.as_str())
        );
        assert_eq!(rows[0].subject, "stable-subject");
        assert_eq!(
            rows[0].email_snapshot.as_deref(),
            Some("provider-user@example.com")
        );
        let resp = external_auth::finish_unified_external_auth_callback(&app, &flow).await;
        external_auth::oauth2::assert_oauth2_error_redirect(&resp);
        assert_eq!(driver.exchanges.load(Ordering::SeqCst), 1);
    }
}

#[actix_web::test]
async fn new_identity_enforces_missing_and_unverified_email_policy() {
    for kind in [Kind::GitHub, Kind::Google] {
        for (email, verified) in [(None, false), (Some("unverified@example.com"), false)] {
            let state = common::setup().await;
            external_auth::oauth2::configure_oauth2_public_site_url(&state);
            let driver = ProfileDriver::new(kind);
            driver.profile(|profile| {
                profile.email = email.map(str::to_owned);
                profile.email_verified = verified;
            });
            let app = create_test_app!(state.clone(), driver.registry());
            provider(&state, kind, "unverified", true).await;
            let flow = begin(&app, kind, "unverified").await;
            let resp = external_auth::finish_unified_external_auth_callback(&app, &flow).await;
            if kind == Kind::Google && email.is_none() {
                assert!(!external_auth::oauth2::oauth2_email_required_flow(&resp).is_empty());
            } else {
                external_auth::oauth2::assert_oauth2_error_redirect(&resp);
            }
            assert!(
                external_auth_identity::Entity::find()
                    .all(state.writer_db())
                    .await
                    .unwrap()
                    .is_empty()
            );
        }
    }
}

#[actix_web::test]
async fn existing_identity_survives_missing_provider_email() {
    for kind in [Kind::GitHub, Kind::Qq] {
        let state = common::setup().await;
        external_auth::oauth2::configure_oauth2_public_site_url(&state);
        let driver = ProfileDriver::new(kind);
        driver.profile(|profile| {
            profile.email = None;
            profile.email_verified = false;
        });
        let app = create_test_app!(state.clone(), driver.registry());
        let (admin, _) = register_and_login!(app);
        let user_id = admin_create_user!(app, admin, "linked", "linked@example.com", "password123");
        let stored = provider(&state, kind, "existing", true).await;
        external_auth_identity::ActiveModel {
            user_id: Set(user_id),
            provider_id: Set(stored.id),
            identity_namespace: Set(format!("provider://{}", kind.as_str())),
            subject: Set("stable-subject".into()),
            email_snapshot: Set(Some("linked@example.com".into())),
            created_at: Set(chrono::Utc::now()),
            updated_at: Set(chrono::Utc::now()),
            ..Default::default()
        }
        .insert(state.writer_db())
        .await
        .unwrap();
        let flow = begin(&app, kind, "existing").await;
        let resp = external_auth::finish_unified_external_auth_callback(&app, &flow).await;
        assert!(common::extract_cookie(&resp, "aster_access").is_some());
        assert_eq!(
            external_auth_identity::Entity::find()
                .all(state.writer_db())
                .await
                .unwrap()
                .len(),
            1
        );
    }
}

#[actix_web::test]
async fn qq_profile_without_email_requires_local_verification() {
    let state = common::setup().await;
    external_auth::oauth2::configure_oauth2_public_site_url(&state);
    let driver = ProfileDriver::new(Kind::Qq);
    driver.profile(|profile| {
        profile.email = None;
        profile.email_verified = false;
    });
    let app = create_test_app!(state.clone(), driver.registry());
    provider(&state, Kind::Qq, "qq", false).await;
    let flow = begin(&app, Kind::Qq, "qq").await;
    let resp = external_auth::finish_unified_external_auth_callback(&app, &flow).await;
    assert!(!external_auth::oauth2::oauth2_email_required_flow(&resp).is_empty());
    assert!(
        external_auth_identity::Entity::find()
            .all(state.writer_db())
            .await
            .unwrap()
            .is_empty()
    );
}

#[actix_web::test]
async fn provider_exchange_errors_issue_no_session_and_create_no_identity() {
    for kind in [Kind::GitHub, Kind::Google, Kind::Qq] {
        let state = common::setup().await;
        external_auth::oauth2::configure_oauth2_public_site_url(&state);
        let driver = ProfileDriver::new(kind);
        *driver.reply.lock().unwrap() = Err("provider rejected the callback");
        let app = create_test_app!(state.clone(), driver.registry());
        provider(&state, kind, "error", true).await;
        let flow = begin(&app, kind, "error").await;
        let resp = external_auth::finish_unified_external_auth_callback(&app, &flow).await;
        external_auth::oauth2::assert_oauth2_error_redirect(&resp);
        assert_eq!(driver.exchanges.load(Ordering::SeqCst), 1);
        assert!(
            external_auth_identity::Entity::find()
                .all(state.writer_db())
                .await
                .unwrap()
                .is_empty()
        );
    }
}

#[actix_web::test]
async fn google_subject_stays_stable_across_email_changes() {
    let state = common::setup().await;
    external_auth::oauth2::configure_oauth2_public_site_url(&state);
    let driver = ProfileDriver::new(Kind::Google);
    let app = create_test_app!(state.clone(), driver.registry());
    let _ = register_and_login!(app);
    provider(&state, Kind::Google, "google", true).await;
    let flow = begin(&app, Kind::Google, "google").await;
    let resp = external_auth::finish_unified_external_auth_callback(&app, &flow).await;
    assert!(common::extract_cookie(&resp, "aster_access").is_some());
    let original = external_auth_identity::Entity::find()
        .one(state.writer_db())
        .await
        .unwrap()
        .unwrap();
    driver.profile(|profile| profile.email = Some("changed@example.com".into()));
    let flow = begin(&app, Kind::Google, "google").await;
    let resp = external_auth::finish_unified_external_auth_callback(&app, &flow).await;
    assert!(common::extract_cookie(&resp, "aster_access").is_some());
    let rows = external_auth_identity::Entity::find()
        .all(state.writer_db())
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, original.id);
    assert_eq!(rows[0].user_id, original.user_id);
    assert_eq!(
        rows[0].email_snapshot.as_deref(),
        Some("changed@example.com")
    );
    assert!(
        user::Entity::find_by_id(original.user_id)
            .one(state.writer_db())
            .await
            .unwrap()
            .is_some()
    );
}

#[actix_web::test]
async fn connection_tests_use_the_same_instance_registry_as_login() {
    let state = common::setup().await;
    external_auth::oauth2::configure_oauth2_public_site_url(&state);
    let driver = ProfileDriver::new(Kind::Google);
    let app = create_test_app!(state.clone(), driver.registry());
    let (admin, _) = register_and_login!(app);
    let stored = provider(&state, Kind::Google, "probe", true).await;
    for uri in [
        "/api/v1/admin/external-auth/providers/test".to_string(),
        format!("/api/v1/admin/external-auth/providers/{}/test", stored.id),
    ] {
        let req = test::TestRequest::post().uri(&uri)
            .insert_header(("Cookie", common::access_cookie_header(&admin)))
            .insert_header(common::csrf_header_for(&admin))
            .set_json(serde_json::json!({"provider_kind":"google", "client_id":"client", "client_secret":"secret"})).to_request();
        let resp = test::call_service(&app, req).await;
        let status = resp.status();
        let body: Value = test::read_body_json(resp).await;
        assert_eq!(status, 200, "{uri}: {body}");
    }
    assert_eq!(driver.probes.load(Ordering::SeqCst), 2);
    let req = test::TestRequest::post().uri("/api/v1/admin/external-auth/providers/test")
        .insert_header(("Cookie", common::access_cookie_header(&admin)))
        .insert_header(common::csrf_header_for(&admin))
        .set_json(serde_json::json!({"provider_kind":"google", "issuer_url":"http://127.0.0.1:1", "client_id":"client"})).to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
    assert_eq!(driver.probes.load(Ordering::SeqCst), 2);
}

#[actix_web::test]
async fn registries_are_isolated_between_app_instances() {
    let first = ProfileDriver::new(Kind::Google);
    let second = ProfileDriver::new(Kind::Google);
    let first_app = create_test_app!(common::setup().await, first.registry());
    let second_app = create_test_app!(common::setup().await, second.registry());
    let (first_admin, _) = register_and_login!(first_app);
    let (second_admin, _) = register_and_login!(second_app);
    let first_req = test::TestRequest::post()
        .uri("/api/v1/admin/external-auth/providers/test")
        .insert_header(("Cookie", common::access_cookie_header(&first_admin)))
        .insert_header(common::csrf_header_for(&first_admin))
        .set_json(serde_json::json!({"provider_kind":"google", "client_id":"client"}))
        .to_request();
    assert_eq!(
        test::call_service(&first_app, first_req).await.status(),
        200
    );
    assert_eq!(first.probes.load(Ordering::SeqCst), 1);
    assert_eq!(second.probes.load(Ordering::SeqCst), 0);
    let second_req = test::TestRequest::post()
        .uri("/api/v1/admin/external-auth/providers/test")
        .insert_header(("Cookie", common::access_cookie_header(&second_admin)))
        .insert_header(common::csrf_header_for(&second_admin))
        .set_json(serde_json::json!({"provider_kind":"google", "client_id":"client"}))
        .to_request();
    assert_eq!(
        test::call_service(&second_app, second_req).await.status(),
        200
    );
    assert_eq!(first.probes.load(Ordering::SeqCst), 1);
    assert_eq!(second.probes.load(Ordering::SeqCst), 1);
}
