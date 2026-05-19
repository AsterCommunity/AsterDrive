//! WOPI launch session 与 access token 解析。
//!
//! 这里的 token 不是普通登录 JWT，而是“某个用户、某个 app、某个 file、某个 session_version”
//! 的一次性 WOPI 访问授权。所有后续 WOPI 操作都会先在这里解出作用域和目标文件。

use chrono::{DateTime, Duration, Utc};
use sea_orm::Set;
use serde::{Deserialize, Serialize};

use crate::api::subcode::ApiSubcode;
use crate::cache::CacheExt;
use crate::config::site_url;
use crate::db::repository::wopi_session_repo;
use crate::entities::{file, wopi_session};
use crate::errors::{
    AsterError, Result, auth_forbidden_with_subcode, validation_error_with_subcode,
};
use crate::runtime::PrimaryAppState;
use crate::services::{
    auth_service, preview_app_service, workspace_storage_service,
    workspace_storage_service::WorkspaceStorageScope,
};

use super::discovery::{
    ensure_request_proof_valid, ensure_request_source_allowed, parse_wopi_app_config,
    resolve_action_url,
};
use super::types::{WopiLaunchSession, WopiRequestSource};

const WOPI_SESSION_CACHE_TTL: u64 = 300;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedWopiSession {
    id: i64,
    actor_user_id: i64,
    session_version: i64,
    team_id: Option<i64>,
    file_id: i64,
    app_key: String,
    expires_at: DateTime<Utc>,
}

impl From<&wopi_session::Model> for CachedWopiSession {
    fn from(session: &wopi_session::Model) -> Self {
        Self {
            id: session.id,
            actor_user_id: session.actor_user_id,
            session_version: session.session_version,
            team_id: session.team_id,
            file_id: session.file_id,
            app_key: session.app_key.clone(),
            expires_at: session.expires_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct WopiAccessTokenPayload {
    pub(crate) actor_user_id: i64,
    pub(crate) session_version: i64,
    pub(crate) team_id: Option<i64>,
    pub(crate) file_id: i64,
    pub(crate) app_key: String,
    pub(crate) exp: i64,
}

#[derive(Debug, Clone)]
pub(crate) struct ResolvedWopiAccess {
    pub(crate) file: file::Model,
    pub(crate) payload: WopiAccessTokenPayload,
}

fn wopi_session_cache_key(token_hash: &str) -> String {
    format!("wopi_session:{token_hash}")
}

fn wopi_session_cache_ttl(session: &wopi_session::Model) -> Option<u64> {
    let ttl = (session.expires_at - Utc::now()).num_seconds();
    if ttl <= 0 {
        None
    } else {
        let ttl = u64::try_from(ttl).ok()?;
        Some(std::cmp::min(ttl, WOPI_SESSION_CACHE_TTL))
    }
}

async fn cache_wopi_session(state: &PrimaryAppState, session: &wopi_session::Model) {
    let Some(ttl) = wopi_session_cache_ttl(session) else {
        return;
    };
    state
        .cache
        .set(
            &wopi_session_cache_key(&session.token_hash),
            &CachedWopiSession::from(session),
            Some(ttl),
        )
        .await;
}

async fn load_wopi_session_by_hash(
    state: &PrimaryAppState,
    token_hash: &str,
) -> Result<Option<CachedWopiSession>> {
    let cache_key = wopi_session_cache_key(token_hash);
    if let Some(cached) = state.cache.get::<CachedWopiSession>(&cache_key).await {
        tracing::debug!(token_hash = %token_hash, "wopi session cache hit");
        return Ok(Some(cached));
    }

    let session = wopi_session_repo::find_by_token_hash(&state.db, token_hash).await?;
    if let Some(session) = &session {
        cache_wopi_session(state, session).await;
    }
    tracing::debug!(token_hash = %token_hash, "wopi session cache miss");
    Ok(session.as_ref().map(CachedWopiSession::from))
}

async fn delete_wopi_session(
    state: &PrimaryAppState,
    token_hash: &str,
    session_id: i64,
) -> Result<()> {
    wopi_session_repo::delete_by_id(&state.db, session_id).await?;
    state
        .cache
        .delete(&wopi_session_cache_key(token_hash))
        .await;
    Ok(())
}

pub(crate) async fn create_launch_session_in_scope(
    state: &PrimaryAppState,
    scope: WorkspaceStorageScope,
    file_id: i64,
    app_key: &str,
    request_origin: Option<RequestOrigin<'_>>,
) -> Result<WopiLaunchSession> {
    // launch 阶段做的是“把内部 file 访问权翻译成外部 WOPI app 可用的 action_url +
    // access_token”，并不是立刻打开文档。
    let file = workspace_storage_service::verify_file_access(state, scope, file_id).await?;
    let auth_snapshot = auth_service::get_auth_snapshot(state, scope.actor_user_id()).await?;
    let app = preview_app_service::get_public_preview_apps(state)
        .apps
        .into_iter()
        .find(|candidate| candidate.key == app_key)
        .ok_or_else(|| AsterError::record_not_found(format!("preview app '{app_key}'")))?;
    let app_config = parse_wopi_app_config(&app)?;

    let wopi_src = build_public_wopi_src(state, file.id, request_origin)?;
    let action_url = resolve_action_url(state, &app_config, &file, &wopi_src).await?;
    let expires_at = Utc::now()
        + Duration::seconds(crate::config::wopi::access_token_ttl_secs(
            &state.runtime_config,
        ));
    let access_token = create_access_token_session(
        state,
        &WopiAccessTokenPayload {
            actor_user_id: scope.actor_user_id(),
            session_version: auth_snapshot.session_version,
            team_id: scope.team_id(),
            file_id: file.id,
            app_key: app.key.clone(),
            exp: expires_at.timestamp(),
        },
    )
    .await?;

    Ok(WopiLaunchSession {
        access_token,
        access_token_ttl: expires_at.timestamp_millis(),
        action_url,
        form_fields: app_config.form_fields,
        mode: Some(app_config.mode),
    })
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct RequestOrigin<'a> {
    pub(crate) scheme: &'a str,
    pub(crate) host: &'a str,
}

fn require_public_origin(public_origin: Option<String>) -> Result<String> {
    public_origin.ok_or_else(|| {
        validation_error_with_subcode(
            ApiSubcode::WopiPublicSiteUrlRequired,
            "public_site_url is required for WOPI integration",
        )
    })
}

pub(crate) fn select_public_origin(
    state: &PrimaryAppState,
    request_origin: Option<RequestOrigin<'_>>,
) -> Result<String> {
    require_public_origin(
        request_origin
            .and_then(|origin| {
                site_url::public_site_url_for_request(
                    &state.runtime_config,
                    origin.scheme,
                    origin.host,
                )
            })
            .or_else(|| site_url::public_site_url(&state.runtime_config)),
    )
}

pub(crate) fn select_public_origin_from_preselected(
    state: &PrimaryAppState,
    request_public_origin: Option<&str>,
) -> Result<String> {
    require_public_origin(
        request_public_origin
            .map(str::to_owned)
            .or_else(|| site_url::public_site_url(&state.runtime_config)),
    )
}

pub(crate) fn build_public_wopi_src(
    state: &PrimaryAppState,
    file_id: i64,
    request_origin: Option<RequestOrigin<'_>>,
) -> Result<String> {
    // WOPISrc 指向的是 CheckFileInfo 端点，而不是 `/contents`。
    // 官方路径定义见：
    // - https://learn.microsoft.com/en-us/microsoft-365/cloud-storage-partner-program/rest/files/checkfileinfo
    // - https://learn.microsoft.com/en-us/microsoft-365/cloud-storage-partner-program/rest/files/getfile
    // 这里与 `src/api/routes/wopi.rs`、PUT_RELATIVE 返回的新文件 URL 强耦合，改动时必须同步。
    let base = select_public_origin(state, request_origin)?;

    Ok(format!("{base}/api/v1/wopi/files/{file_id}"))
}

pub(crate) async fn create_access_token_for_file(
    state: &PrimaryAppState,
    payload: &WopiAccessTokenPayload,
    file_id: i64,
) -> Result<String> {
    let expires_at = Utc::now()
        + Duration::seconds(crate::config::wopi::access_token_ttl_secs(
            &state.runtime_config,
        ));
    create_access_token_session(
        state,
        &WopiAccessTokenPayload {
            file_id,
            exp: expires_at.timestamp(),
            ..payload.clone()
        },
    )
    .await
}

pub(crate) async fn resolve_access_token(
    state: &PrimaryAppState,
    file_id: i64,
    access_token: &str,
    request_source: WopiRequestSource<'_>,
) -> Result<ResolvedWopiAccess> {
    let token_hash = access_token_hash(access_token);
    let session = load_wopi_session_by_hash(state, &token_hash)
        .await?
        .ok_or_else(|| AsterError::auth_token_invalid("WOPI access token not found or expired"))?;
    let payload = payload_from_session(&session)?;
    let expires_at = session.expires_at;
    if expires_at < Utc::now() {
        delete_wopi_session(state, &token_hash, session.id).await?;
        return Err(AsterError::auth_token_expired("WOPI access token expired"));
    }
    if payload.file_id != file_id {
        return Err(AsterError::file_not_found(format!(
            "WOPI token does not match file #{file_id}",
        )));
    }
    // session_version 绑定的是“登录态快照”而不是 WOPI session 自身。
    // 用户登出、改密或被强制刷新会话后，旧的 WOPI token 会一起失效。
    let auth_snapshot = auth_service::get_auth_snapshot(state, payload.actor_user_id).await?;
    if !auth_snapshot.status.is_active() {
        delete_wopi_session(state, &token_hash, session.id).await?;
        return Err(auth_forbidden_with_subcode(
            ApiSubcode::AuthAccountDisabled,
            "account is disabled",
        ));
    }
    if auth_snapshot.session_version != payload.session_version {
        delete_wopi_session(state, &token_hash, session.id).await?;
        return Err(AsterError::auth_token_invalid("WOPI session revoked"));
    }
    let Some(app) = preview_app_service::get_public_preview_apps(state)
        .apps
        .into_iter()
        .find(|candidate| candidate.key == payload.app_key)
    else {
        delete_wopi_session(state, &token_hash, session.id).await?;
        return Err(auth_forbidden_with_subcode(
            ApiSubcode::WopiAppDisabled,
            "WOPI app is no longer available",
        ));
    };
    if !app.enabled {
        delete_wopi_session(state, &token_hash, session.id).await?;
        return Err(auth_forbidden_with_subcode(
            ApiSubcode::WopiAppDisabled,
            "WOPI app is disabled",
        ));
    }
    let app_config = match parse_wopi_app_config(&app) {
        Ok(config) => config,
        Err(error) => {
            delete_wopi_session(state, &token_hash, session.id).await?;
            return Err(error);
        }
    };
    ensure_request_source_allowed(&app, &request_source)?;
    if let Err(error) =
        ensure_request_proof_valid(state, &app_config, access_token, &request_source).await
    {
        delete_wopi_session(state, &token_hash, session.id).await?;
        return Err(error);
    }

    // 最终仍回到统一文件 scope 校验：WOPI 只是另一个接入层，不应绕开个人/团队边界。
    let file =
        workspace_storage_service::verify_file_access(state, scope_from_payload(&payload), file_id)
            .await?;

    Ok(ResolvedWopiAccess { file, payload })
}

pub(crate) fn scope_from_payload(payload: &WopiAccessTokenPayload) -> WorkspaceStorageScope {
    match payload.team_id {
        Some(team_id) => WorkspaceStorageScope::Team {
            team_id,
            actor_user_id: payload.actor_user_id,
        },
        None => WorkspaceStorageScope::Personal {
            user_id: payload.actor_user_id,
        },
    }
}

async fn create_access_token_session(
    state: &PrimaryAppState,
    payload: &WopiAccessTokenPayload,
) -> Result<String> {
    let token = format!("wopi_{}", crate::utils::id::new_short_token());
    let token_hash = access_token_hash(&token);
    let expires_at = DateTime::from_timestamp(payload.exp, 0)
        .ok_or_else(|| AsterError::internal_error("invalid WOPI access token expiry"))?;
    let now = Utc::now();
    let session = wopi_session_repo::create(
        &state.db,
        wopi_session::ActiveModel {
            token_hash: Set(token_hash),
            actor_user_id: Set(payload.actor_user_id),
            session_version: Set(payload.session_version),
            team_id: Set(payload.team_id),
            file_id: Set(payload.file_id),
            app_key: Set(payload.app_key.clone()),
            expires_at: Set(expires_at),
            created_at: Set(now),
            ..Default::default()
        },
    )
    .await?;
    cache_wopi_session(state, &session).await;
    Ok(token)
}

pub(crate) fn access_token_hash(token: &str) -> String {
    crate::utils::hash::sha256_hex(token.as_bytes())
}

fn payload_from_session(session: &CachedWopiSession) -> Result<WopiAccessTokenPayload> {
    Ok(WopiAccessTokenPayload {
        actor_user_id: session.actor_user_id,
        session_version: session.session_version,
        team_id: session.team_id,
        file_id: session.file_id,
        app_key: session.app_key.clone(),
        exp: session.expires_at.timestamp(),
    })
}

pub async fn cleanup_expired(state: &PrimaryAppState) -> Result<u64> {
    wopi_session_repo::delete_expired(&state.db).await
}
