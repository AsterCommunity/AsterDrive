//! Forge adapter regressions with the real Drive writer transaction, schema, and HTTP router.
use crate::common;

use actix_web::test;
use aster_drive::{
    db::repository::{auth_session_repo, user_repo},
    runtime::PrimaryAppState,
    services::auth::local,
};
use aster_drive_model::entities::auth_session;
use aster_drive_model::types::{AuditAction, UserStatus};
use aster_forge_auth::{AuthTokenType, JwtKey, JwtPolicy, JwtVerifier, TokenClaims};
use aster_forge_cache::{CacheBackend, CacheExt};
use aster_forge_db::audit_log;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, IntoActiveModel, PaginatorTrait,
    QueryFilter, Set,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Rejects reads/deletes of the obsolete auth cache namespace; new authorization must not touch it.
struct UnavailableLegacyAuthCache {
    inner: Arc<dyn CacheBackend>,
    accesses: AtomicUsize,
}
#[async_trait::async_trait]
impl CacheBackend for UnavailableLegacyAuthCache {
    fn backend_name(&self) -> &'static str {
        self.inner.backend_name()
    }
    async fn health_check(&self) -> aster_forge_cache::Result<()> {
        self.inner.health_check().await
    }
    async fn get_bytes(&self, key: &str) -> Option<Vec<u8>> {
        if key.starts_with("auth_snapshot:") {
            self.accesses.fetch_add(1, Ordering::SeqCst);
            None
        } else {
            self.inner.get_bytes(key).await
        }
    }
    async fn take_bytes(&self, key: &str) -> Option<Vec<u8>> {
        self.inner.take_bytes(key).await
    }
    async fn set_bytes(&self, key: &str, value: Vec<u8>, ttl_secs: Option<u64>) {
        self.inner.set_bytes(key, value, ttl_secs).await;
    }
    async fn set_bytes_if_absent(&self, key: &str, value: Vec<u8>, ttl_secs: Option<u64>) -> bool {
        self.inner.set_bytes_if_absent(key, value, ttl_secs).await
    }
    async fn delete(&self, key: &str) {
        if key.starts_with("auth_snapshot:") {
            self.accesses.fetch_add(1, Ordering::SeqCst);
        } else {
            self.inner.delete(key).await;
        }
    }
    async fn invalidate_prefix(&self, prefix: &str) {
        self.inner.invalidate_prefix(prefix).await;
    }
}

/// Creates an isolated writer/cache/router fixture and one committed browser session.
async fn fixture() -> (PrimaryAppState, String, String, i64) {
    let state = common::setup_with_memory_cache().await;
    let admin = local::setup(&state, "forge-user", "forge@example.com", "password123")
        .await
        .unwrap();
    let (access, refresh) = local::issue_tokens_for_session(
        &state,
        admin.id,
        admin.session_version,
        Some("127.0.0.1"),
        Some("browser"),
    )
    .await
    .unwrap();
    common::seed_csrf_token(&access);
    common::seed_csrf_token(&refresh);
    (state, access, refresh, admin.id)
}
/// Inspects tokens with the real strict verifier, without relying on unchecked JWT JSON.
fn claims(state: &PrimaryAppState, token: &str, kind: AuthTokenType) -> TokenClaims {
    JwtVerifier::with_system_clock(
        JwtKey::hmac(state.config.auth.jwt_secret.as_bytes()).unwrap(),
        JwtPolicy::new("aster-drive", "aster-drive-web", 900, 604800).unwrap(),
    )
    .verify(token, kind)
    .unwrap()
    .into_claims()
}
/// Reuses the fixture's explicit browser fingerprint for deterministic stale classification.
async fn refresh(
    state: &PrimaryAppState,
    token: &str,
) -> aster_drive::errors::Result<(String, String)> {
    local::refresh_tokens(state, token, Some("127.0.0.1"), Some("browser")).await
}
/// Counts the security facts persisted by the same transaction as reuse revocation.
async fn audit_count(state: &PrimaryAppState, id: i64) -> u64 {
    audit_log::Entity::find()
        .filter(audit_log::Column::UserId.eq(id))
        .filter(audit_log::Column::Action.eq(AuditAction::UserRefreshTokenReuseDetected.as_str()))
        .count(state.writer_db())
        .await
        .unwrap()
}

/// Verifies the actual refresh route returns only committed, session-bound credentials and
/// preserves Drive's cookie paths and CSRF transport contract.
#[actix_web::test]
async fn refresh_router_keeps_runtime_ttls_session_binding_cookie_and_csrf_contract() {
    let (state, _, refresh_token, _) = fixture().await;
    let app = create_test_app!(state.clone());
    let original = claims(&state, &refresh_token, AuthTokenType::Refresh);
    let req = test::TestRequest::post()
        .uri("/api/v1/auth/refresh")
        .insert_header(("Cookie", common::refresh_cookie_header(&refresh_token)))
        .insert_header(common::csrf_header_for(&refresh_token))
        .insert_header(("User-Agent", "browser"))
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .to_request();
    let response = test::call_service(&app, req).await;
    assert_eq!(response.status(), 200);
    let access = common::extract_cookie(&response, "aster_access").unwrap();
    let rotated = common::extract_cookie(&response, "aster_refresh").unwrap();
    let next_access = claims(&state, &access, AuthTokenType::Access);
    let next_refresh = claims(&state, &rotated, AuthTokenType::Refresh);
    assert_eq!(next_access.exp - next_access.iat, 900);
    assert_eq!(next_refresh.exp - next_refresh.iat, 604800);
    assert_eq!(next_access.sid, original.sid);
    assert_eq!(next_refresh.sid, original.sid);
    assert_ne!(next_refresh.jti, original.jti);
    for cookie in response.response().cookies() {
        if matches!(cookie.name(), "aster_access" | "aster_refresh") {
            assert!(cookie.http_only().unwrap());
        }
        if cookie.name() == "aster_refresh" {
            assert_eq!(cookie.path(), Some("/api/v1/auth"));
        }
    }
    assert!(common::extract_cookie(&response, "aster_csrf").is_some());
}

/// A TTL reload changes newly issued expiry while keeping the instance admission/lock state.
#[actix_web::test]
async fn runtime_ttl_reload_changes_issuance_without_replacing_the_instance_coordinator() {
    let (state, _, token, _) = fixture().await;
    for (key, value) in [
        (
            aster_drive::config::auth_runtime::AUTH_ACCESS_TOKEN_TTL_SECS_KEY,
            "123",
        ),
        (
            aster_drive::config::auth_runtime::AUTH_REFRESH_TOKEN_TTL_SECS_KEY,
            "4567",
        ),
    ] {
        let now = chrono::Utc::now();
        state
            .runtime_config
            .apply(aster_forge_db::system_config::Model {
                id: 0,
                key: key.into(),
                value: value.into(),
                value_type: aster_forge_config::ConfigValueType::Number,
                is_sensitive: false,
                source: aster_forge_config::ConfigSource::System,
                visibility: aster_forge_config::ConfigVisibility::Private,
                namespace: String::new(),
                category: "auth".into(),
                requires_restart: false,
                description: String::new(),
                updated_at: now,
                updated_by: None,
            });
    }
    let (access, rotated) = refresh(&state, &token).await.unwrap();
    let a = claims(&state, &access, AuthTokenType::Access);
    let r = claims(&state, &rotated, AuthTokenType::Refresh);
    assert_eq!(a.exp - a.iat, 123);
    assert_eq!(r.exp - r.iat, 4567);
    assert_eq!(state.runtime_config.refresh_coordinator().tracked_ids(), 0);
}

/// A stale retry is always rejected without side effects; replay after another rotation is
/// still distinguishable and must commit the required security revocation.
#[actix_web::test]
async fn stale_retry_has_no_mutation_or_audit_and_older_replay_remains_identifiable() {
    let (state, _, token, id) = fixture().await;
    let (_, next) = refresh(&state, &token).await.unwrap();
    assert_eq!(refresh(&state, &token).await.unwrap_err().code(), "E019");
    assert_eq!(
        user_repo::find_by_id(state.writer_db(), id)
            .await
            .unwrap()
            .session_version,
        1
    );
    assert_eq!(audit_count(&state, id).await, 0);
    refresh(&state, &next).await.unwrap();
    assert_eq!(refresh(&state, &token).await.unwrap_err().code(), "E062");
    assert_eq!(
        user_repo::find_by_id(state.writer_db(), id)
            .await
            .unwrap()
            .session_version,
        2
    );
    assert_eq!(audit_count(&state, id).await, 1);
    assert!(
        auth_session_repo::list_active_for_user(state.writer_db(), id)
            .await
            .unwrap()
            .is_empty()
    );
}

/// Missing request metadata cannot erase persisted hints or manufacture matching-client proof.
#[actix_web::test]
async fn missing_client_metadata_is_preserved_but_never_substituted_as_replay_evidence() {
    for (ip, ua, expected_ip, expected_ua) in [
        (None, None, "127.0.0.1", "browser"),
        (Some("127.0.0.2"), None, "127.0.0.2", "browser"),
        (None, Some("new-browser"), "127.0.0.1", "new-browser"),
    ] {
        let (state, _, token, id) = fixture().await;
        let sid = claims(&state, &token, AuthTokenType::Refresh).sid;
        local::refresh_tokens(&state, &token, ip, ua).await.unwrap();
        let stored = auth_session_repo::find_by_id_for_user(state.writer_db(), id, sid.as_str())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.ip_address.as_deref(), Some(expected_ip));
        assert_eq!(stored.user_agent.as_deref(), Some(expected_ua));
        // Stored metadata is display/history data; a request with no evidence is still reuse.
        assert_eq!(
            local::refresh_tokens(&state, &token, None, None)
                .await
                .unwrap_err()
                .code(),
            "E062"
        );
        assert_eq!(audit_count(&state, id).await, 1);
    }
}

/// Exercises each SQL predicate independently and guards timestamp precision against moving
/// last_seen backwards when JWT timestamps have only whole-second precision.
#[actix_web::test]
async fn database_cas_checks_every_fence_and_keeps_subsecond_last_seen_monotonic() {
    for case in 0..6 {
        let (state, _, token, id) = fixture().await;
        let wire = claims(&state, &token, AuthTokenType::Refresh);
        let mut stored =
            auth_session_repo::find_by_id_for_user(state.writer_db(), id, wire.sid.as_str())
                .await
                .unwrap()
                .unwrap();
        let now = chrono::Utc::now();
        if case == 3 || case == 4 {
            let mut active = stored.into_active_model();
            if case == 3 {
                active.revoked_at = Set(Some(now));
            } else {
                active.refresh_expires_at = Set(now);
            }
            stored = active.update(state.writer_db()).await.unwrap();
        }
        let rotation = auth_session_repo::RefreshRotation {
            user_id: if case == 0 { id + 1 } else { id },
            session_id: if case == 1 {
                "foreign-session"
            } else {
                wire.sid.as_str()
            },
            current_refresh_jti: if case == 2 {
                "foreign-jti"
            } else {
                wire.jti.as_ref().unwrap().as_str()
            },
            next_refresh_jti: "candidate-cas-identity",
            refresh_expires_at: now + chrono::Duration::hours(1),
            ip_address: None,
            user_agent: None,
            // Force the same precision boundary without sleeps or racing the wall clock.
            last_seen_at: if case == 5 {
                chrono::DateTime::from_timestamp(stored.last_seen_at.timestamp(), 0).unwrap()
            } else {
                now
            },
        };
        assert_eq!(
            auth_session_repo::rotate_refresh_fenced(state.writer_db(), rotation)
                .await
                .unwrap(),
            case == 5
        );
        let after =
            auth_session_repo::find_by_id_for_user(state.writer_db(), id, wire.sid.as_str())
                .await
                .unwrap()
                .unwrap();
        if case == 5 {
            assert_eq!(after.last_seen_at, stored.last_seen_at);
            assert!(after.last_seen_at >= after.created_at);
            assert_eq!(after.current_refresh_jti, "candidate-cas-identity");
        } else {
            assert_eq!(after, stored);
        }
    }
}

/// Exhausted generations fail closed; reuse must not partially purge sessions or write audit.
#[actix_web::test]
async fn exhausted_account_generation_fails_without_partial_reuse_effects() {
    let (state, _, _, id) = fixture().await;
    let mut user = user_repo::find_by_id(state.writer_db(), id)
        .await
        .unwrap()
        .into_active_model();
    user.session_version = Set(i64::MAX);
    user.update(state.writer_db()).await.unwrap();
    let (access, token) =
        local::issue_tokens_for_session(&state, id, i64::MAX, Some("127.0.0.1"), Some("browser"))
            .await
            .unwrap();
    refresh(&state, &token).await.unwrap();
    let before = auth_session_repo::list_active_for_user(state.writer_db(), id)
        .await
        .unwrap();
    assert!(matches!(
        local::refresh_tokens(&state, &token, None, None).await,
        Err(aster_drive::errors::AsterError::InternalError(_))
    ));
    assert_eq!(
        user_repo::find_by_id(state.writer_db(), id)
            .await
            .unwrap()
            .session_version,
        i64::MAX
    );
    assert_eq!(
        auth_session_repo::list_active_for_user(state.writer_db(), id)
            .await
            .unwrap(),
        before
    );
    assert_eq!(audit_count(&state, id).await, 0);
    assert!(
        local::authenticate_access_token(&state, &access)
            .await
            .is_ok()
    );
}

/// Overflow refusal applies to password and explicit revoke paths, preserving their old state.
#[actix_web::test]
async fn exhausted_generation_rejects_account_revoke_and_password_changes_atomically() {
    let (state, _, _, id) = fixture().await;
    let mut user = user_repo::find_by_id(state.writer_db(), id)
        .await
        .unwrap()
        .into_active_model();
    user.session_version = Set(i64::MAX);
    let original = user.update(state.writer_db()).await.unwrap();
    let sessions = auth_session_repo::list_active_for_user(state.writer_db(), id)
        .await
        .unwrap();
    assert!(matches!(
        local::revoke_user_sessions(&state, id).await,
        Err(aster_drive::errors::AsterError::InternalError(_))
    ));
    assert!(matches!(
        local::set_password(&state, id, "replacement-password123").await,
        Err(aster_drive::errors::AsterError::InternalError(_))
    ));
    assert_eq!(
        user_repo::find_by_id(state.writer_db(), id).await.unwrap(),
        original
    );
    assert_eq!(
        auth_session_repo::list_active_for_user(state.writer_db(), id)
            .await
            .unwrap(),
        sessions
    );
}

/// Competing revocations use an atomic SQL increment, so neither overwrites the other epoch.
#[actix_web::test]
async fn concurrent_account_revoke_and_refresh_reuse_never_lose_generation_updates() {
    let (state, access, token, id) = fixture().await;
    refresh(&state, &token).await.unwrap();
    let (revoke, replay) = tokio::join!(
        local::revoke_user_sessions(&state, id),
        local::refresh_tokens(&state, &token, None, None)
    );
    let updated = revoke.unwrap();
    assert_eq!(updated.id, id);
    let code = replay.unwrap_err().code();
    assert!(matches!(code, "E012" | "E062"));
    let version = user_repo::find_by_id(state.writer_db(), id)
        .await
        .unwrap()
        .session_version;
    assert_eq!(version, if code == "E062" { 3 } else { 2 });
    assert_eq!(audit_count(&state, id).await, u64::from(code == "E062"));
    assert!(
        auth_session_repo::list_active_for_user(state.writer_db(), id)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        local::authenticate_access_token(&state, &access)
            .await
            .is_err()
    );
}

/// A legacy cache write arriving after commit cannot restore JWT/WOPI authorization.
#[actix_web::test]
async fn committed_reuse_invalidates_access_even_if_an_old_cache_fill_arrives_late() {
    let (state, access, token, id) = fixture().await;
    let stale = local::get_current_auth_snapshot(&state, id).await.unwrap();
    refresh(&state, &token).await.unwrap();
    assert_eq!(
        local::refresh_tokens(&state, &token, Some("127.0.0.1"), Some("different-browser"))
            .await
            .unwrap_err()
            .code(),
        "E062"
    );
    state
        .cache
        .set(&format!("auth_snapshot:{id}"), &stale, Some(60))
        .await;
    // WOPI validity and product permission checks share this authoritative read, so the old
    // presentation snapshot cannot restore their generation or administrator authorization.
    assert_eq!(
        local::get_current_auth_snapshot(&state, id)
            .await
            .unwrap()
            .session_version,
        stale.session_version + 1,
    );
    assert_eq!(
        local::authenticate_access_token(&state, &access)
            .await
            .unwrap_err()
            .code(),
        "E012"
    );
    let record = audit_log::Entity::find()
        .filter(audit_log::Column::Action.eq(AuditAction::UserRefreshTokenReuseDetected.as_str()))
        .one(state.writer_db())
        .await
        .unwrap()
        .unwrap();
    let old_jti = claims(&state, &token, AuthTokenType::Refresh).jti.unwrap();
    let details = record.details.unwrap();
    assert!(!details.contains(old_jti.as_str()));
    assert!(!details.contains(&token));
}

/// Exercises an already-open router stream, including a deliberately late cache fill, so the
/// regression cannot pass merely because the request's initial JWT guard reads the writer.
#[actix_web::test]
async fn storage_event_stream_rejects_stale_cache_after_live_account_security_changes() {
    use actix_web::body::MessageBody;
    use aster_drive::services::events::storage_change::{self, StorageChangeEvent};
    use std::{future::poll_fn, pin::Pin, time::Duration};

    for change in ["revoke", "disable", "require_password_change"] {
        let (state, access, _, id) = fixture().await;
        let stale = local::get_current_auth_snapshot(&state, id).await.unwrap();
        let app = create_test_app!(state.clone());
        let request = test::TestRequest::get()
            .uri("/api/v1/auth/events/storage")
            .insert_header(("Cookie", common::access_cookie_header(&access)))
            .to_request();
        let response = test::call_service(&app, request).await;
        assert_eq!(response.status(), 200);
        let mut body = response.into_body();
        // Start the stream and consume the interval's immediate first heartbeat. Subsequent
        // revalidation is driven by the explicit broadcast below; there is no wall-clock sleep.
        let first = tokio::time::timeout(
            Duration::from_secs(2),
            poll_fn(|cx| Pin::new(&mut body).poll_next(cx)),
        )
        .await
        .unwrap()
        .unwrap()
        .unwrap();
        assert_eq!(first.as_ref(), b": keep-alive\n\n");

        if change == "revoke" {
            local::revoke_user_sessions(&state, id).await.unwrap();
        } else {
            let mut user = user_repo::find_by_id(state.writer_db(), id)
                .await
                .unwrap()
                .into_active_model();
            if change == "disable" {
                user.status = Set(UserStatus::Disabled);
            } else {
                user.must_change_password = Set(true);
            }
            user.update(state.writer_db()).await.unwrap();
        }
        state
            .cache
            .set(&format!("auth_snapshot:{id}"), &stale, Some(60))
            .await;
        assert_eq!(
            state
                .cache
                .get::<local::AuthSnapshot>(&format!("auth_snapshot:{id}"))
                .await
                .unwrap(),
            stale
        );
        storage_change::publish(&state, StorageChangeEvent::sync_required());
        let next = tokio::time::timeout(
            Duration::from_secs(2),
            poll_fn(|cx| Pin::new(&mut body).poll_next(cx)),
        )
        .await
        .expect("security change must close SSE before emitting the queued event");
        assert!(
            next.is_none(),
            "stale cache must not authorize {change}: {next:?}"
        );
    }
}

/// Makes the retired cache namespace unavailable and proves refresh/revocation/authentication
/// do not read or invalidate it, while stale data may remain harmlessly in the cache backend.
#[actix_web::test]
async fn refresh_and_authorization_never_touch_the_obsolete_auth_cache() {
    let (mut state, access, token, id) = fixture().await;
    let old = local::get_current_auth_snapshot(&state, id).await.unwrap();
    let key = format!("auth_snapshot:{id}");
    state.cache.set(&key, &old, Some(60)).await;
    let cache = Arc::new(UnavailableLegacyAuthCache {
        inner: state.cache.clone(),
        accesses: AtomicUsize::new(0),
    });
    state.cache = cache.clone();
    refresh(&state, &token).await.unwrap();
    assert_eq!(
        local::refresh_tokens(&state, &token, None, None)
            .await
            .unwrap_err()
            .code(),
        "E062"
    );
    assert_eq!(cache.accesses.load(Ordering::SeqCst), 0);
    assert_eq!(
        cache
            .inner
            .get::<local::AuthSnapshot>(&key)
            .await
            .unwrap()
            .session_version,
        old.session_version
    );
    assert!(
        local::authenticate_access_token(&state, &access)
            .await
            .is_err()
    );
    assert_eq!(
        user_repo::find_by_id(state.writer_db(), id)
            .await
            .unwrap()
            .session_version,
        old.session_version + 1
    );
    assert_eq!(audit_count(&state, id).await, 1);
    assert_eq!(cache.accesses.load(Ordering::SeqCst), 0);
    assert!(
        auth_session_repo::list_active_for_user(state.writer_db(), id)
            .await
            .unwrap()
            .is_empty()
    );
}

/// Disabled and restricted accounts are checked before replay side effects are considered.
#[actix_web::test]
async fn current_account_and_restriction_checks_also_apply_to_replayed_tokens() {
    for disabled in [false, true] {
        let (state, _, token, id) = fixture().await;
        refresh(&state, &token).await.unwrap();
        let mut account = user_repo::find_by_id(state.writer_db(), id)
            .await
            .unwrap()
            .into_active_model();
        if disabled {
            account.status = Set(UserStatus::Disabled);
        } else {
            account.must_change_password = Set(true);
        }
        account.update(state.writer_db()).await.unwrap();
        let error = refresh(&state, &token).await.unwrap_err();
        assert!(matches!(
            error,
            aster_drive::errors::AsterError::AuthForbidden(_)
        ));
        assert_eq!(audit_count(&state, id).await, 0);
        assert_eq!(
            user_repo::find_by_id(state.writer_db(), id)
                .await
                .unwrap()
                .session_version,
            1
        );
    }
}

/// Both initial issuance and refresh use the effective password-login policy, rather than
/// interpreting the stored must-change flag as an unconditional restriction.
#[actix_web::test]
async fn password_login_toggle_keeps_the_current_password_change_policy() {
    let (state, access, token, id) = fixture().await;
    let mut account = user_repo::find_by_id(state.writer_db(), id)
        .await
        .unwrap()
        .into_active_model();
    account.must_change_password = Set(true);
    account.update(state.writer_db()).await.unwrap();
    let mut toggle = aster_drive::db::repository::config_repo::find_by_key(
        state.writer_db(),
        aster_drive::config::auth_runtime::AUTH_PASSWORD_LOGIN_ENABLED_KEY,
    )
    .await
    .unwrap()
    .unwrap();
    toggle.value = "false".to_string();
    state.runtime_config.apply(toggle.clone());
    let account = user_repo::find_by_id(state.writer_db(), id).await.unwrap();
    let (issued, _) = local::issue_tokens_for_user(state.writer_db(), &state, &account, None, None)
        .await
        .unwrap();
    assert_eq!(
        claims(&state, &issued, AuthTokenType::Access).restriction,
        aster_forge_auth::Restriction::None
    );
    assert!(
        local::authenticate_access_token(&state, &access)
            .await
            .is_ok()
    );
    let (_, rotated) = refresh(&state, &token).await.unwrap();
    assert_eq!(
        claims(&state, &rotated, AuthTokenType::Refresh).restriction,
        aster_forge_auth::Restriction::None
    );
    toggle.value = "true".to_string();
    state.runtime_config.apply(toggle);
    assert!(
        local::authenticate_access_token(&state, &access)
            .await
            .is_err()
    );
    assert!(matches!(
        refresh(&state, &rotated).await,
        Err(aster_drive::errors::AsterError::AuthForbidden(_))
    ));
    assert_eq!(audit_count(&state, id).await, 0);
}

/// Correctly signed credentials with foreign ownership or expired persistence cannot mutate
/// the victim account or rotate an unrelated session.
#[actix_web::test]
async fn foreign_account_session_and_expired_persistence_do_not_rotate_or_revoke() {
    let (state, _, token, id) = fixture().await;
    let mut wire = claims(&state, &token, AuthTokenType::Refresh);
    wire.sid = aster_forge_auth::SessionId::new("another-session").unwrap();
    let foreign = jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &wire,
        &jsonwebtoken::EncodingKey::from_secret(state.config.auth.jwt_secret.as_bytes()),
    )
    .unwrap();
    assert_eq!(refresh(&state, &foreign).await.unwrap_err().code(), "E012");
    let mut wire = claims(&state, &token, AuthTokenType::Refresh);
    wire.sub = aster_forge_auth::Subject::new("9999999").unwrap();
    let foreign = jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &wire,
        &jsonwebtoken::EncodingKey::from_secret(state.config.auth.jwt_secret.as_bytes()),
    )
    .unwrap();
    assert_eq!(refresh(&state, &foreign).await.unwrap_err().code(), "E012");
    let wire = claims(&state, &token, AuthTokenType::Refresh);
    let mut session =
        auth_session_repo::find_by_id_for_user(state.writer_db(), id, wire.sid.as_str())
            .await
            .unwrap()
            .unwrap()
            .into_active_model();
    session.refresh_expires_at = Set(chrono::Utc::now() - chrono::Duration::seconds(1));
    session.update(state.writer_db()).await.unwrap();
    assert!(matches!(
        refresh(&state, &token).await,
        Err(aster_drive::errors::AsterError::AuthTokenExpired(_))
    ));
    assert_eq!(audit_count(&state, id).await, 0);
}

/// Documents Drive's chosen single-session boundary: refresh stops immediately, while already
/// issued access credentials keep their expiry window unless the account epoch changes.
#[actix_web::test]
async fn revoked_session_retains_explicit_access_expiry_window_but_refresh_is_rejected() {
    let (state, access, token, id) = fixture().await;
    let wire = claims(&state, &token, AuthTokenType::Refresh);
    local::revoke_auth_session(
        &state,
        id,
        wire.sid.as_str(),
        wire.jti.as_ref().map(aster_forge_auth::RefreshJti::as_str),
    )
    .await
    .unwrap();
    assert_eq!(refresh(&state, &token).await.unwrap_err().code(), "E012");
    // Drive's product policy retains short-lived access for single-session revocation. Account
    // generation changes are authoritative immediately; sid presence alone is not a live check.
    assert!(
        local::authenticate_access_token(&state, &access)
            .await
            .is_ok()
    );
}

/// Independent in-memory coordinators still have one SQL winner; the loser gets no tokens.
#[actix_web::test]
async fn independent_instance_coordinators_have_one_real_database_cas_winner() {
    let (state, _, token, id) = fixture().await;
    let mut other = state.clone();
    other.runtime_config = Arc::new(aster_drive::config::RuntimeConfig::new());
    let (one, two) = tokio::join!(refresh(&state, &token), refresh(&other, &token));
    assert_eq!(usize::from(one.is_ok()) + usize::from(two.is_ok()), 1);
    let loser = one.as_ref().err().or_else(|| two.as_ref().err()).unwrap();
    assert_eq!(loser.code(), "E019");
    assert_eq!(
        user_repo::find_by_id(state.writer_db(), id)
            .await
            .unwrap()
            .session_version,
        1
    );
}

/// Closing one runtime admission gate must not close another product instance's coordinator.
#[actix_web::test]
async fn shutdown_rejects_refresh_and_isolated_instances_keep_working() {
    let (state, _, token, _) = fixture().await;
    let (other, _, other_token, _) = fixture().await;
    state.runtime_config.refresh_coordinator().close();
    assert!(refresh(&state, &token).await.is_err());
    assert!(refresh(&other, &other_token).await.is_ok());
}

// SQLite failure injection exercises actual product rollback; Forge's portable tests inject
// begin/CAS/revoke/audit/commit failures without backend-specific sleep or global hooks.
/// A required security-audit insert failure rolls back generation and purge writes together.
#[actix_web::test]
async fn sqlite_audit_failure_rolls_back_reuse_version_and_session_purge() {
    let (state, access, token, id) = fixture().await;
    if state.writer_db().get_database_backend() != sea_orm::DbBackend::Sqlite {
        return;
    }
    refresh(&state, &token).await.unwrap();
    state.writer_db().execute_unprepared("CREATE TRIGGER fail_security_audit BEFORE INSERT ON audit_logs BEGIN SELECT RAISE(ABORT, 'injected audit failure'); END;").await.unwrap();
    let error = local::refresh_tokens(&state, &token, None, None)
        .await
        .unwrap_err();
    assert_eq!(error.code(), "E002");
    assert_eq!(
        user_repo::find_by_id(state.writer_db(), id)
            .await
            .unwrap()
            .session_version,
        1
    );
    assert_eq!(
        auth_session_repo::list_active_for_user(state.writer_db(), id)
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(
        local::authenticate_access_token(&state, &access)
            .await
            .is_ok()
    );
}

/// A deferred constraint fails COMMIT after the writes, proving rejection cannot escape before
/// the security transaction is durably complete.
#[actix_web::test]
async fn sqlite_commit_failure_rolls_back_completed_reuse_writes_and_preserves_access() {
    let (state, access, token, id) = fixture().await;
    if state.writer_db().get_database_backend() != sea_orm::DbBackend::Sqlite {
        return;
    }
    refresh(&state, &token).await.unwrap();
    let sessions = auth_session_repo::list_active_for_user(state.writer_db(), id)
        .await
        .unwrap();
    state
        .writer_db()
        .execute_unprepared("PRAGMA foreign_keys=ON;")
        .await
        .unwrap();
    state.writer_db().execute_unprepared(
        "CREATE TABLE deferred_auth_failure (user_id INTEGER REFERENCES users(id) DEFERRABLE INITIALLY DEFERRED);"
    ).await.unwrap();
    // The audit insert succeeds. Only COMMIT checks this deliberately invalid deferred reference.
    state.writer_db().execute_unprepared(
        "CREATE TRIGGER fail_security_commit AFTER INSERT ON audit_logs BEGIN INSERT INTO deferred_auth_failure VALUES (-1); END;"
    ).await.unwrap();
    assert_eq!(
        local::refresh_tokens(&state, &token, None, None)
            .await
            .unwrap_err()
            .code(),
        "E002"
    );
    assert_eq!(
        user_repo::find_by_id(state.writer_db(), id)
            .await
            .unwrap()
            .session_version,
        1
    );
    assert_eq!(
        auth_session_repo::list_active_for_user(state.writer_db(), id)
            .await
            .unwrap(),
        sessions
    );
    assert_eq!(audit_count(&state, id).await, 0);
    assert!(
        local::authenticate_access_token(&state, &access)
            .await
            .is_ok()
    );
}

/// A real SQL update failure keeps the old JTI usable and sends no uncommitted cookies.
#[actix_web::test]
async fn sqlite_cas_failure_preserves_old_credential_and_returns_no_replacement_cookies() {
    let (state, _, token, id) = fixture().await;
    if state.writer_db().get_database_backend() != sea_orm::DbBackend::Sqlite {
        return;
    }
    let original = claims(&state, &token, AuthTokenType::Refresh).jti.unwrap();
    state.writer_db().execute_unprepared("CREATE TRIGGER fail_refresh_cas BEFORE UPDATE OF current_refresh_jti ON auth_sessions BEGIN SELECT RAISE(ABORT, 'injected CAS failure'); END;").await.unwrap();
    let app = create_test_app!(state.clone());
    let req = test::TestRequest::post()
        .uri("/api/v1/auth/refresh")
        .insert_header(("Cookie", common::refresh_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .to_request();
    let response = test::call_service(&app, req).await;
    assert_eq!(response.status(), 500);
    assert!(common::extract_cookie(&response, "aster_access").is_none());
    assert!(common::extract_cookie(&response, "aster_refresh").is_none());
    let session = auth_session::Entity::find()
        .filter(auth_session::Column::UserId.eq(id))
        .one(state.writer_db())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(session.current_refresh_jti, original.as_str());
    assert!(session.previous_refresh_jti.is_none());
}
