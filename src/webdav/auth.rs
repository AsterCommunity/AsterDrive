//! WebDAV 子模块：`auth`。

#[path = "auth/cache.rs"]
mod cache;

use base64::Engine;
use serde::{Deserialize, Serialize};

use crate::api::api_error_code::ApiErrorCode;
use crate::db::repository::{user_repo, webdav_account_repo};
use crate::errors::{AsterError, MapAsterErr, auth_forbidden_with_code};
use crate::runtime::SharedRuntimeState;
use crate::services::workspace_storage_service::WorkspaceStorageScope;
use crate::utils::hash;

/// WebDAV 认证结果
#[derive(Debug)]
pub(crate) struct WebdavAuthResult {
    pub(crate) account_id: i64,
    pub(crate) scope: WorkspaceStorageScope,
    /// 限制访问范围：None = 全部，Some(folder_id) = 只能访问该文件夹及子目录
    pub(crate) root_folder_id: Option<i64>,
}

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
    headers: &actix_web::http::header::HeaderMap,
    state: &impl SharedRuntimeState,
) -> Result<WebdavAuthResult, AsterError> {
    let auth_header = headers
        .get(actix_web::http::header::AUTHORIZATION)
        .and_then(|v: &actix_web::http::header::HeaderValue| v.to_str().ok())
        .ok_or_else(|| AsterError::auth_token_invalid("missing Authorization header"))?;

    if let Some(basic) = auth_header.strip_prefix("Basic ") {
        authenticate_basic(basic.trim(), state).await
    } else {
        Err(AsterError::auth_token_invalid("unsupported auth scheme"))
    }
}

/// Basic Auth: 查 webdav_accounts 表（独立于登录密码）
async fn authenticate_basic(
    encoded: &str,
    state: &impl SharedRuntimeState,
) -> Result<WebdavAuthResult, AsterError> {
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

    // 查 WebDAV 专用账号
    let account = webdav_account_repo::find_by_username(state.writer_db(), username)
        .await?
        .ok_or_else(|| AsterError::auth_invalid_credentials("WebDAV account not found"))?;

    if !account.is_active {
        return Err(auth_forbidden_with_code(
            ApiErrorCode::AuthAccountDisabled,
            "WebDAV account is disabled",
        ));
    }

    if !hash::verify_password(password, &account.password_hash)? {
        return Err(AsterError::auth_invalid_credentials("wrong password"));
    }

    // 确认关联用户仍然活跃
    let user = user_repo::find_by_id(state.writer_db(), account.user_id).await?;
    if !user.status.is_active() {
        return Err(auth_forbidden_with_code(
            ApiErrorCode::AuthAccountDisabled,
            "user account is disabled",
        ));
    }

    let scope = match account.team_id {
        Some(team_id) => {
            crate::services::workspace_storage_service::require_team_access(
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
    tracing::debug!(username_hash = %cache::username_cache_component(username), "webdav auth cache miss");
    Ok(WebdavAuthResult {
        account_id: account.id,
        scope,
        root_folder_id: account.root_folder_id,
    })
}

#[cfg(test)]
mod tests {
    use super::{authenticate_webdav, invalidate_webdav_auth_for_username};
    use crate::cache;
    use crate::config::{CacheConfig, Config, DatabaseConfig, RuntimeConfig};
    use crate::entities::{user, webdav_account};
    use crate::errors::AsterError;
    use crate::runtime::{PrimaryAppState, SharedRuntimeState};
    use crate::services::mail_service;
    use crate::storage::{DriverRegistry, PolicySnapshot};
    use crate::types::{UserRole, UserStatus};
    use crate::utils::hash;
    use actix_web::http::header::{self, HeaderMap, HeaderValue};
    use base64::Engine;
    use chrono::Utc;
    use migration::Migrator;
    use sea_orm::{ActiveModelTrait, IntoActiveModel, Set};
    use std::sync::Arc;

    async fn build_auth_test_state() -> PrimaryAppState {
        let db = crate::db::connect_with_metrics(
            &DatabaseConfig {
                url: "sqlite::memory:".to_string(),
                pool_size: 1,
                retry_count: 0,
            },
            crate::metrics_core::NoopMetrics::arc(),
        )
        .await
        .expect("webdav auth test database should connect");
        Migrator::up(&db, None)
            .await
            .expect("webdav auth test migrations should succeed");

        let runtime_config = Arc::new(RuntimeConfig::new());
        let cache = cache::create_cache(&CacheConfig::default()).await;
        let (storage_change_tx, _) = tokio::sync::broadcast::channel(
            crate::services::storage_change_service::STORAGE_CHANGE_CHANNEL_CAPACITY,
        );
        let share_download_rollback =
            crate::services::share_service::spawn_detached_share_download_rollback_queue(
                db.clone(),
                crate::config::operations::share_download_rollback_queue_capacity(&runtime_config),
            );

        PrimaryAppState {
            db_handles: crate::db::DbHandles::single(db),
            driver_registry: Arc::new(DriverRegistry::noop()),
            runtime_config: runtime_config.clone(),
            policy_snapshot: Arc::new(PolicySnapshot::new()),
            config: Arc::new(Config::default()),
            cache,
            metrics: crate::metrics_core::NoopMetrics::arc(),
            mail_sender: mail_service::runtime_sender(runtime_config),
            storage_change_tx,
            share_download_rollback,
            background_task_dispatch_wakeup:
                crate::runtime::PrimaryAppState::new_background_task_dispatch_wakeup(),
            remote_protocol: crate::runtime::PrimaryAppState::new_remote_protocol(),
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

    fn basic_headers(username: &str, password: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        let encoded =
            base64::engine::general_purpose::STANDARD.encode(format!("{username}:{password}"));
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_str(&format!("Basic {encoded}"))
                .expect("basic auth header should be valid"),
        );
        headers
    }

    fn bearer_headers(token: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {token}"))
                .expect("bearer auth header should be valid"),
        );
        headers
    }

    #[actix_web::test]
    async fn basic_auth_succeeds() {
        let state = build_auth_test_state().await;
        let (username, password, user_id, account_id, root_folder_id) =
            seed_webdav_account(&state).await;

        let result = authenticate_webdav(&basic_headers(&username, &password), &state)
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

        let err = authenticate_webdav(&basic_headers(&username, "wrong-password"), &state)
            .await
            .expect_err("wrong password should fail");

        assert!(matches!(
            err,
            AsterError::AuthInvalidCredentials(message) if message == "wrong password"
        ));
    }

    #[actix_web::test]
    async fn bearer_auth_returns_unsupported_auth_scheme() {
        let state = build_auth_test_state().await;

        let err = authenticate_webdav(&bearer_headers("jwt-token"), &state)
            .await
            .expect_err("bearer auth should be rejected");

        assert!(matches!(
            err,
            AsterError::AuthTokenInvalid(message) if message == "unsupported auth scheme"
        ));
    }

    #[actix_web::test]
    async fn cached_basic_auth_rejects_disabled_account_after_invalidation() {
        let state = build_auth_test_state().await;
        let (username, password, _, _, _) = seed_webdav_account(&state).await;

        authenticate_webdav(&basic_headers(&username, &password), &state)
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

        let err = authenticate_webdav(&basic_headers(&username, &password), &state)
            .await
            .expect_err("invalidated auth should reject a disabled account immediately");

        assert!(matches!(err, AsterError::AuthForbidden(_)));
    }

    #[actix_web::test]
    async fn cached_basic_auth_rejects_changed_password_after_invalidation() {
        let state = build_auth_test_state().await;
        let (username, password, _, _, _) = seed_webdav_account(&state).await;

        authenticate_webdav(&basic_headers(&username, &password), &state)
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

        let err = authenticate_webdav(&basic_headers(&username, &password), &state)
            .await
            .expect_err("invalidated auth should reject the old password immediately");

        assert!(matches!(err, AsterError::AuthInvalidCredentials(_)));
    }

    #[actix_web::test]
    async fn missing_authorization_header_returns_token_invalid() {
        let state = build_auth_test_state().await;

        let err = authenticate_webdav(&HeaderMap::new(), &state)
            .await
            .expect_err("missing Authorization header should fail");

        assert!(matches!(
            err,
            AsterError::AuthTokenInvalid(message) if message == "missing Authorization header"
        ));
    }
}
