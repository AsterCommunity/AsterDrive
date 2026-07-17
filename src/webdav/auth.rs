//! WebDAV 子模块：`auth`。

#[path = "auth/cache.rs"]
mod cache;
#[path = "auth/rate_limit.rs"]
mod rate_limit;

use base64::Engine;
use serde::{Deserialize, Serialize};

use crate::api::api_error_code::ApiErrorCode;
use crate::db::repository::{user_repo, webdav_account_repo};
use crate::errors::{AsterError, MapAsterErr, auth_forbidden_with_code};
use crate::runtime::SharedRuntimeState;
use crate::services::workspace::storage::WorkspaceStorageScope;
use aster_forge_crypto as hash;

/// WebDAV 认证结果
#[derive(Debug)]
pub(crate) struct WebdavAuthResult {
    pub(crate) account_id: i64,
    pub(crate) scope: WorkspaceStorageScope,
    /// 限制访问范围：None = 全部，Some(folder_id) = 只能访问该文件夹及子目录
    pub(crate) root_folder_id: Option<i64>,
}

#[derive(Debug)]
pub(crate) enum WebdavAuthError {
    Rejected,
    RateLimited { retry_after: u64 },
}

impl From<AsterError> for WebdavAuthError {
    fn from(_error: AsterError) -> Self {
        Self::Rejected
    }
}

pub(crate) use rate_limit::WebdavAuthProtection;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedWebdavAuth {
    account_id: i64,
    user_id: i64,
    team_id: Option<i64>,
    root_folder_id: Option<i64>,
}

impl CachedWebdavAuth {
    fn scope(&self) -> WorkspaceStorageScope {
        match self.team_id {
            Some(team_id) => WorkspaceStorageScope::Team {
                team_id,
                actor_user_id: self.user_id,
            },
            None => WorkspaceStorageScope::Personal {
                user_id: self.user_id,
            },
        }
    }
}

pub(crate) async fn invalidate_webdav_auth_for_username(
    state: &impl SharedRuntimeState,
    username: &str,
) {
    cache::invalidate_for_username(state, username).await;
}

pub(crate) async fn invalidate_webdav_auth_for_user(
    state: &impl SharedRuntimeState,
    user_id: i64,
) -> Result<(), AsterError> {
    let accounts = webdav_account_repo::find_all_by_user(state.writer_db(), user_id).await?;
    for account in accounts {
        invalidate_webdav_auth_for_username(state, &account.username).await;
    }
    Ok(())
}

pub(crate) async fn invalidate_webdav_auth_for_team(
    state: &impl SharedRuntimeState,
    team_id: i64,
) -> Result<(), AsterError> {
    let accounts = webdav_account_repo::find_by_team(state.writer_db(), team_id).await?;
    for account in accounts {
        invalidate_webdav_auth_for_username(state, &account.username).await;
    }
    Ok(())
}

pub(crate) async fn invalidate_webdav_auth_for_team_member(
    state: &impl SharedRuntimeState,
    team_id: i64,
    user_id: i64,
) -> Result<(), AsterError> {
    let accounts =
        webdav_account_repo::find_by_team_and_user(state.writer_db(), team_id, user_id).await?;
    for account in accounts {
        invalidate_webdav_auth_for_username(state, &account.username).await;
    }
    Ok(())
}

/// 从 WebDAV 请求头提取并认证用户
///
/// 支持：
/// 1. `Authorization: Basic base64(username:password)` — 查 webdav_accounts 表
pub(crate) async fn authenticate_webdav(
    request: &actix_web::HttpRequest,
    state: &impl SharedRuntimeState,
) -> Result<WebdavAuthResult, WebdavAuthError> {
    let auth_header = request
        .headers()
        .get(actix_web::http::header::AUTHORIZATION)
        .and_then(|v: &actix_web::http::header::HeaderValue| v.to_str().ok())
        .ok_or_else(|| AsterError::auth_token_invalid("missing Authorization header"))?;

    if let Some(basic) = auth_header.strip_prefix("Basic ") {
        authenticate_basic(basic.trim(), request, state).await
    } else {
        Err(AsterError::auth_token_invalid("unsupported auth scheme").into())
    }
}

/// Basic Auth: 查 webdav_accounts 表（独立于登录密码）
async fn authenticate_basic(
    encoded: &str,
    request: &actix_web::HttpRequest,
    state: &impl SharedRuntimeState,
) -> Result<WebdavAuthResult, WebdavAuthError> {
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_aster_err_with(|| AsterError::auth_invalid_credentials("invalid base64"))?;

    let credentials = String::from_utf8(decoded)
        .map_aster_err_with(|| AsterError::auth_invalid_credentials("invalid utf8"))?;

    let (username, password) = credentials
        .split_once(':')
        .ok_or_else(|| AsterError::auth_invalid_credentials("invalid basic auth format"))?;

    if let Some(cached) = cache::load_auth(state, username, password).await {
        tracing::debug!(username_hash = %cache::username_cache_component(username), "webdav auth cache hit");
        return Ok(WebdavAuthResult {
            account_id: cached.account_id,
            scope: cached.scope(),
            root_folder_id: cached.root_folder_id,
        });
    }

    let protection = request
        .app_data::<actix_web::web::Data<WebdavAuthProtection>>()
        .ok_or_else(|| AsterError::internal_error("WebDAV auth protection is not configured"))?;
    protection
        .check_ip(request)
        .map_err(|retry_after| WebdavAuthError::RateLimited { retry_after })?;
    rate_limit::check_username_backoff(state, protection.enabled(), username)
        .await
        .map_err(|retry_after| WebdavAuthError::RateLimited { retry_after })?;

    // 查 WebDAV 专用账号
    let account = match webdav_account_repo::find_by_username(state.writer_db(), username).await? {
        Some(account) => account,
        None => {
            rate_limit::record_username_failure(state, protection.enabled(), username).await;
            return Err(AsterError::auth_invalid_credentials("invalid WebDAV credentials").into());
        }
    };

    if !account.is_active {
        rate_limit::record_username_failure(state, protection.enabled(), username).await;
        return Err(auth_forbidden_with_code(
            ApiErrorCode::AuthAccountDisabled,
            "WebDAV account is disabled",
        )
        .into());
    }

    if !hash::verify_password(password, &account.password_hash).map_err(AsterError::from)? {
        rate_limit::record_username_failure(state, protection.enabled(), username).await;
        return Err(AsterError::auth_invalid_credentials("invalid WebDAV credentials").into());
    }

    // 确认关联用户仍然活跃
    let user = user_repo::find_by_id(state.writer_db(), account.user_id).await?;
    if !user.status.is_active() {
        rate_limit::record_username_failure(state, protection.enabled(), username).await;
        return Err(auth_forbidden_with_code(
            ApiErrorCode::AuthAccountDisabled,
            "user account is disabled",
        )
        .into());
    }

    let scope = match account.team_id {
        Some(team_id) => {
            crate::services::workspace::storage::require_team_access(
                state,
                team_id,
                account.user_id,
            )
            .await?;
            WorkspaceStorageScope::Team {
                team_id,
                actor_user_id: account.user_id,
            }
        }
        None => WorkspaceStorageScope::Personal {
            user_id: account.user_id,
        },
    };

    cache::store_auth(
        state,
        username,
        password,
        &CachedWebdavAuth {
            account_id: account.id,
            user_id: account.user_id,
            team_id: account.team_id,
            root_folder_id: account.root_folder_id,
        },
    )
    .await;
    rate_limit::clear_username_failures(state, protection.enabled(), username).await;
    tracing::debug!(username_hash = %cache::username_cache_component(username), "webdav auth cache miss");
    Ok(WebdavAuthResult {
        account_id: account.id,
        scope,
        root_folder_id: account.root_folder_id,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        WebdavAuthError, WebdavAuthProtection, authenticate_webdav,
        invalidate_webdav_auth_for_username,
    };
    use crate::config::{Config, DatabaseConfig, RateLimitTier, RuntimeConfig};
    use crate::entities::{user, webdav_account};
    use crate::runtime::{PrimaryAppState, SharedRuntimeState};
    use crate::services::mail::sender;
    use crate::storage::{DriverRegistry, PolicySnapshot};
    use crate::types::{UserRole, UserStatus};
    use actix_web::http::header::{self, HeaderValue};
    use actix_web::{HttpRequest, test, web};
    use aster_forge_cache as cache;
    use aster_forge_cache::CacheConfig;
    use aster_forge_crypto as hash;
    use base64::Engine;
    use chrono::Utc;
    use migration::Migrator;
    use sea_orm::{ActiveModelTrait, IntoActiveModel, Set};
    use std::num::{NonZeroU32, NonZeroU64};
    use std::sync::Arc;

    async fn build_auth_test_state() -> PrimaryAppState {
        let db = crate::db::connect_with_metrics(
            &DatabaseConfig {
                url: "sqlite::memory:".to_string(),
                pool_size: 1,
                retry_count: 0,
            },
            crate::metrics::NoopMetrics::arc(),
        )
        .await
        .expect("webdav auth test database should connect");
        Migrator::up(&db, None)
            .await
            .expect("webdav auth test migrations should succeed");

        let runtime_config = Arc::new(RuntimeConfig::new());
        let cache = cache::create_cache(&CacheConfig::default()).await;
        let (storage_change_tx, _) = tokio::sync::broadcast::channel(
            crate::services::events::storage_change::STORAGE_CHANGE_CHANNEL_CAPACITY,
        );
        let share_download_rollback =
            crate::services::share::spawn_detached_share_download_rollback_queue(
                db.clone(),
                crate::config::operations::share_download_rollback_queue_capacity(&runtime_config),
            );

        PrimaryAppState {
            db_handles: aster_forge_db::DbHandles::single(db),
            driver_registry: Arc::new(DriverRegistry::noop()),
            runtime_config: runtime_config.clone(),
            policy_snapshot: Arc::new(PolicySnapshot::new()),
            config: Arc::new(Config::default()),
            cache,
            config_sync: aster_forge_config::ConfigSyncRuntime::disabled_for_test("aster_drive"),
            metrics: crate::metrics::NoopMetrics::arc(),
            mail_sender: sender::runtime_sender(runtime_config),
            storage_change_tx,
            share_download_rollback,
            background_task_dispatch_wakeup:
                crate::runtime::PrimaryAppState::new_background_task_dispatch_wakeup(),
            remote_protocol: crate::runtime::PrimaryAppState::new_remote_protocol(),
            upload_runtime: crate::runtime::PrimaryAppState::new_upload_runtime(),
        }
    }

    async fn seed_webdav_account(
        state: &PrimaryAppState,
    ) -> (String, String, i64, i64, Option<i64>) {
        let now = Utc::now();
        let user = user::ActiveModel {
            username: Set("webdav-auth-user".to_string()),
            email: Set("webdav-auth-user@example.com".to_string()),
            password_hash: Set("unused".to_string()),
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
        .expect("webdav auth test user should be inserted");

        let username = "webdav-auth".to_string();
        let password = "webdav-pass".to_string();
        let root_folder_id = Some(123);

        let account = webdav_account::ActiveModel {
            user_id: Set(user.id),
            username: Set(username.clone()),
            password_hash: Set(
                hash::hash_password(&password).expect("webdav auth test password hash should work")
            ),
            root_folder_id: Set(root_folder_id),
            is_active: Set(true),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(state.writer_db())
        .await
        .expect("webdav auth test account should be inserted");

        (username, password, user.id, account.id, root_folder_id)
    }

    fn request_with_auth(
        value: HeaderValue,
        protection: WebdavAuthProtection,
        peer: &str,
        forwarded_for: Option<&str>,
    ) -> HttpRequest {
        let mut request = test::TestRequest::default();
        request = request
            .peer_addr(peer.parse().expect("test peer address should parse"))
            .insert_header((header::AUTHORIZATION, value))
            .app_data(web::Data::new(protection));
        if let Some(forwarded_for) = forwarded_for {
            request = request.insert_header(("X-Forwarded-For", forwarded_for));
        }
        request.to_http_request()
    }

    fn default_protection() -> WebdavAuthProtection {
        WebdavAuthProtection::new(true, &RateLimitTier::default(), &[])
    }

    fn strict_protection(trusted_proxies: &[String]) -> WebdavAuthProtection {
        WebdavAuthProtection::new(
            true,
            &RateLimitTier {
                seconds_per_request: NonZeroU64::new(60).unwrap(),
                burst_size: NonZeroU32::new(1).unwrap(),
            },
            trusted_proxies,
        )
    }

    fn wide_protection() -> WebdavAuthProtection {
        WebdavAuthProtection::new(
            true,
            &RateLimitTier {
                seconds_per_request: NonZeroU64::new(1).unwrap(),
                burst_size: NonZeroU32::new(100).unwrap(),
            },
            &[],
        )
    }

    fn basic_request(username: &str, password: &str) -> HttpRequest {
        basic_request_with(username, password, default_protection(), "127.0.0.1:12345")
    }

    fn basic_request_with(
        username: &str,
        password: &str,
        protection: WebdavAuthProtection,
        peer: &str,
    ) -> HttpRequest {
        let encoded =
            base64::engine::general_purpose::STANDARD.encode(format!("{username}:{password}"));
        request_with_auth(
            HeaderValue::from_str(&format!("Basic {encoded}"))
                .expect("basic auth header should be valid"),
            protection,
            peer,
            None,
        )
    }

    fn bearer_request(token: &str) -> HttpRequest {
        request_with_auth(
            HeaderValue::from_str(&format!("Bearer {token}"))
                .expect("bearer auth header should be valid"),
            default_protection(),
            "127.0.0.1:12345",
            None,
        )
    }

    #[actix_web::test]
    async fn basic_auth_succeeds() {
        let state = build_auth_test_state().await;
        let (username, password, user_id, account_id, root_folder_id) =
            seed_webdav_account(&state).await;

        let result = authenticate_webdav(&basic_request(&username, &password), &state)
            .await
            .expect("basic auth should succeed");

        assert_eq!(result.account_id, account_id);
        assert_eq!(result.scope.actor_user_id(), user_id);
        assert_eq!(result.root_folder_id, root_folder_id);
    }

    #[actix_web::test]
    async fn basic_auth_wrong_password_returns_invalid_credentials() {
        let state = build_auth_test_state().await;
        let (username, _, _, _, _) = seed_webdav_account(&state).await;

        let err = authenticate_webdav(&basic_request(&username, "wrong-password"), &state)
            .await
            .expect_err("wrong password should fail");

        assert!(matches!(err, WebdavAuthError::Rejected));
    }

    #[actix_web::test]
    async fn bearer_auth_returns_unsupported_auth_scheme() {
        let state = build_auth_test_state().await;

        let err = authenticate_webdav(&bearer_request("jwt-token"), &state)
            .await
            .expect_err("bearer auth should be rejected");

        assert!(matches!(err, WebdavAuthError::Rejected));
    }

    #[actix_web::test]
    async fn cached_basic_auth_rejects_disabled_account_after_invalidation() {
        let state = build_auth_test_state().await;
        let (username, password, _, _, _) = seed_webdav_account(&state).await;

        authenticate_webdav(&basic_request(&username, &password), &state)
            .await
            .expect("first auth should populate cache");

        let account = crate::db::repository::webdav_account_repo::find_by_username(
            state.writer_db(),
            &username,
        )
        .await
        .expect("account lookup should work")
        .expect("account should exist");
        let mut active = account.into_active_model();
        active.is_active = Set(false);
        active
            .update(state.writer_db())
            .await
            .expect("account disable should work");
        invalidate_webdav_auth_for_username(&state, &username).await;

        let err = authenticate_webdav(&basic_request(&username, &password), &state)
            .await
            .expect_err("invalidated auth should reject a disabled account immediately");

        assert!(matches!(err, WebdavAuthError::Rejected));
    }

    #[actix_web::test]
    async fn cached_basic_auth_rejects_changed_password_after_invalidation() {
        let state = build_auth_test_state().await;
        let (username, password, _, _, _) = seed_webdav_account(&state).await;

        authenticate_webdav(&basic_request(&username, &password), &state)
            .await
            .expect("first auth should populate cache");

        let account = crate::db::repository::webdav_account_repo::find_by_username(
            state.writer_db(),
            &username,
        )
        .await
        .expect("account lookup should work")
        .expect("account should exist");
        let mut active = account.into_active_model();
        active.password_hash = Set(hash::hash_password("new-webdav-password")
            .expect("webdav auth test password hash should work"));
        active
            .update(state.writer_db())
            .await
            .expect("password update should work");
        invalidate_webdav_auth_for_username(&state, &username).await;

        let err = authenticate_webdav(&basic_request(&username, &password), &state)
            .await
            .expect_err("invalidated auth should reject the old password immediately");

        assert!(matches!(err, WebdavAuthError::Rejected));
    }

    #[actix_web::test]
    async fn missing_authorization_header_returns_token_invalid() {
        let state = build_auth_test_state().await;

        let request = test::TestRequest::default().to_http_request();
        let err = authenticate_webdav(&request, &state)
            .await
            .expect_err("missing Authorization header should fail");

        assert!(matches!(err, WebdavAuthError::Rejected));
    }

    #[actix_web::test]
    async fn cached_success_bypasses_ip_limiter() {
        let state = build_auth_test_state().await;
        let (username, password, _, _, _) = seed_webdav_account(&state).await;
        let protection = strict_protection(&[]);

        authenticate_webdav(
            &basic_request_with(&username, &password, protection.clone(), "127.0.0.1:12345"),
            &state,
        )
        .await
        .expect("first auth should populate cache");

        authenticate_webdav(
            &basic_request_with(&username, &password, protection, "127.0.0.1:12345"),
            &state,
        )
        .await
        .expect("cached auth should not consume the IP limiter");
    }

    #[actix_web::test]
    async fn ip_limiter_uses_forwarded_ip_only_for_trusted_proxy() {
        let state = build_auth_test_state().await;
        let (username, _, _, _, _) = seed_webdav_account(&state).await;
        let protection = strict_protection(&["10.0.0.0/8".to_string()]);

        let first = request_with_auth(
            HeaderValue::from_str(&format!(
                "Basic {}",
                base64::engine::general_purpose::STANDARD.encode(format!("{username}:wrong-one"))
            ))
            .unwrap(),
            protection.clone(),
            "10.0.0.5:12345",
            Some("203.0.113.9"),
        );
        assert!(matches!(
            authenticate_webdav(&first, &state).await,
            Err(WebdavAuthError::Rejected)
        ));

        let second = request_with_auth(
            HeaderValue::from_str(&format!(
                "Basic {}",
                base64::engine::general_purpose::STANDARD.encode(format!("{username}:wrong-two"))
            ))
            .unwrap(),
            protection,
            "10.0.0.6:12345",
            Some("203.0.113.9"),
        );
        assert!(matches!(
            authenticate_webdav(&second, &state).await,
            Err(WebdavAuthError::RateLimited { .. })
        ));
    }

    #[actix_web::test]
    async fn malformed_basic_auth_variants_are_rejected() {
        let state = build_auth_test_state().await;
        let variants = [
            HeaderValue::from_static("Basic !!!"),
            HeaderValue::from_static("Basic /w=="),
            HeaderValue::from_str(&format!(
                "Basic {}",
                base64::engine::general_purpose::STANDARD.encode("missing-separator")
            ))
            .unwrap(),
        ];

        for value in variants {
            let request = request_with_auth(value, wide_protection(), "127.0.0.1:12345", None);
            assert!(matches!(
                authenticate_webdav(&request, &state).await,
                Err(WebdavAuthError::Rejected)
            ));
        }
    }

    #[actix_web::test]
    async fn missing_peer_falls_back_to_localhost_for_ip_limit() {
        let state = build_auth_test_state().await;
        let (username, _, _, _, _) = seed_webdav_account(&state).await;
        let protection = strict_protection(&[]);

        for (password, expected_rate_limit) in [("wrong-one", false), ("wrong-two", true)] {
            let encoded =
                base64::engine::general_purpose::STANDARD.encode(format!("{username}:{password}"));
            let request = test::TestRequest::default()
                .insert_header((header::AUTHORIZATION, format!("Basic {encoded}")))
                .app_data(web::Data::new(protection.clone()))
                .to_http_request();
            assert_eq!(
                matches!(
                    authenticate_webdav(&request, &state).await,
                    Err(WebdavAuthError::RateLimited { .. })
                ),
                expected_rate_limit
            );
        }
    }

    #[actix_web::test]
    async fn untrusted_peer_cannot_merge_ip_buckets_with_forwarded_header() {
        let state = build_auth_test_state().await;
        let (username, _, _, _, _) = seed_webdav_account(&state).await;
        let protection = strict_protection(&["10.0.0.0/8".to_string()]);

        for (password, peer) in [
            ("wrong-one", "198.51.100.10:12345"),
            ("wrong-two", "198.51.100.11:12345"),
        ] {
            let encoded =
                base64::engine::general_purpose::STANDARD.encode(format!("{username}:{password}"));
            let request = request_with_auth(
                HeaderValue::from_str(&format!("Basic {encoded}")).unwrap(),
                protection.clone(),
                peer,
                Some("203.0.113.9"),
            );
            assert!(matches!(
                authenticate_webdav(&request, &state).await,
                Err(WebdavAuthError::Rejected)
            ));
        }
    }

    #[actix_web::test]
    async fn username_backoff_applies_across_rotating_ips_before_password_verify() {
        let state = build_auth_test_state().await;
        let (username, _, _, _, _) = seed_webdav_account(&state).await;
        let protection = wide_protection();

        for (index, password) in ["wrong-one", "wrong-two", "wrong-three"]
            .into_iter()
            .enumerate()
        {
            let peer = format!("198.51.100.{}:12345", index + 10);
            assert!(matches!(
                authenticate_webdav(
                    &basic_request_with(&username, password, protection.clone(), &peer),
                    &state,
                )
                .await,
                Err(WebdavAuthError::Rejected)
            ));
        }

        let account = crate::db::repository::webdav_account_repo::find_by_username(
            state.writer_db(),
            &username,
        )
        .await
        .unwrap()
        .unwrap();
        let mut active = account.into_active_model();
        active.password_hash = Set("broken-password-hash".to_string());
        active.update(state.writer_db()).await.unwrap();

        assert!(matches!(
            authenticate_webdav(
                &basic_request_with(
                    &username,
                    "would-trigger-hash-error",
                    protection,
                    "198.51.100.99:12345",
                ),
                &state,
            )
            .await,
            Err(WebdavAuthError::RateLimited { .. })
        ));
    }

    #[actix_web::test]
    async fn successful_authentication_clears_username_failure_state() {
        let state = build_auth_test_state().await;
        let (username, password, _, _, _) = seed_webdav_account(&state).await;
        let protection = wide_protection();

        for wrong_password in ["wrong-one", "wrong-two"] {
            assert!(matches!(
                authenticate_webdav(
                    &basic_request_with(
                        &username,
                        wrong_password,
                        protection.clone(),
                        "198.51.100.20:12345",
                    ),
                    &state,
                )
                .await,
                Err(WebdavAuthError::Rejected)
            ));
        }
        authenticate_webdav(
            &basic_request_with(
                &username,
                &password,
                protection.clone(),
                "198.51.100.21:12345",
            ),
            &state,
        )
        .await
        .expect("correct password should clear failure state");
        invalidate_webdav_auth_for_username(&state, &username).await;

        assert!(matches!(
            authenticate_webdav(
                &basic_request_with(
                    &username,
                    "wrong-after-success",
                    protection,
                    "198.51.100.22:12345",
                ),
                &state,
            )
            .await,
            Err(WebdavAuthError::Rejected)
        ));
    }

    #[actix_web::test]
    async fn disabled_rate_limit_does_not_accumulate_ip_or_username_state() {
        let state = build_auth_test_state().await;
        let (username, _, _, _, _) = seed_webdav_account(&state).await;
        let protection = WebdavAuthProtection::new(
            false,
            &RateLimitTier {
                seconds_per_request: NonZeroU64::new(60).unwrap(),
                burst_size: NonZeroU32::new(1).unwrap(),
            },
            &[],
        );

        for index in 0..10 {
            assert!(matches!(
                authenticate_webdav(
                    &basic_request_with(
                        &username,
                        &format!("wrong-{index}"),
                        protection.clone(),
                        "127.0.0.1:12345",
                    ),
                    &state,
                )
                .await,
                Err(WebdavAuthError::Rejected)
            ));
        }
    }
}
