//! 服务模块：`stream_ticket_service`。

use chrono::{DateTime, Duration, Utc};
use moka::future::Cache;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;
use std::time::Duration as StdDuration;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::cache::CacheExt;
use crate::config::site_url;
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::runtime::SharedRuntimeState;
use crate::services::{
    task_service,
    workspace_storage_service::{self, WorkspaceStorageScope},
};

const STREAM_TICKET_TTL_SECS: i64 = 5 * 60;
const STREAM_TICKET_CACHE_PREFIX: &str = "stream_ticket:";
static FALLBACK_STREAM_TICKETS: LazyLock<Cache<String, StreamTicketPayload>> =
    LazyLock::new(|| {
        Cache::builder()
            .max_capacity(10_000)
            .time_to_live(StdDuration::from_secs(
                u64::try_from(STREAM_TICKET_TTL_SECS).unwrap_or(300),
            ))
            .build()
    });

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StreamTicketInfo {
    pub token: String,
    pub download_path: String,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum StreamTicketKind {
    ArchiveDownload {
        file_ids: Vec<i64>,
        folder_ids: Vec<i64>,
        archive_name: String,
    },
    SharedArchiveDownload {
        share_token: String,
        file_ids: Vec<i64>,
        folder_ids: Vec<i64>,
        archive_name: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StreamTicketPayload {
    actor_user_id: i64,
    team_id: Option<i64>,
    exp: i64,
    kind: StreamTicketKind,
}

pub(crate) async fn create_archive_download_ticket_in_scope(
    state: &impl SharedRuntimeState,
    scope: WorkspaceStorageScope,
    params: &task_service::types::CreateArchiveTaskParams,
) -> Result<StreamTicketInfo> {
    let prepared =
        task_service::archive::prepare_archive_download_in_scope(state, scope, params).await?;
    let expires_at = Utc::now() + Duration::seconds(STREAM_TICKET_TTL_SECS);
    let token = format!("st_{}", crate::utils::id::new_short_token());
    let payload = StreamTicketPayload {
        actor_user_id: scope.actor_user_id(),
        team_id: scope.team_id(),
        exp: expires_at.timestamp(),
        kind: StreamTicketKind::ArchiveDownload {
            file_ids: prepared.file_ids,
            folder_ids: prepared.folder_ids,
            archive_name: prepared.archive_name,
        },
    };

    let cache_key = cache_key(&token);
    store_ticket(state, &cache_key, &payload, ttl_secs_until(expires_at)?).await?;

    Ok(StreamTicketInfo {
        download_path: stream_download_path(state.runtime_config(), scope, &token),
        token,
        expires_at,
    })
}

pub(crate) async fn create_shared_archive_download_ticket(
    state: &impl SharedRuntimeState,
    share_token: &str,
    params: &task_service::types::CreateArchiveTaskParams,
) -> Result<StreamTicketInfo> {
    let prepared =
        task_service::archive::prepare_shared_archive_download(state, share_token, params).await?;
    let expires_at = Utc::now() + Duration::seconds(STREAM_TICKET_TTL_SECS);
    let token = format!("st_{}", crate::utils::id::new_short_token());
    let payload = StreamTicketPayload {
        actor_user_id: 0,
        team_id: None,
        exp: expires_at.timestamp(),
        kind: StreamTicketKind::SharedArchiveDownload {
            share_token: share_token.to_string(),
            file_ids: prepared.file_ids,
            folder_ids: prepared.folder_ids,
            archive_name: prepared.archive_name,
        },
    };

    let cache_key = cache_key(&token);
    store_ticket(state, &cache_key, &payload, ttl_secs_until(expires_at)?).await?;

    Ok(StreamTicketInfo {
        download_path: shared_stream_download_path(state.runtime_config(), share_token, &token),
        token,
        expires_at,
    })
}

pub(crate) async fn resolve_archive_download_ticket_in_scope(
    state: &impl SharedRuntimeState,
    scope: WorkspaceStorageScope,
    token: &str,
) -> Result<task_service::types::CreateArchiveTaskParams> {
    let cache_key = cache_key(token);
    let payload = load_ticket(state, &cache_key)
        .await
        .ok_or_else(|| AsterError::validation_error("stream ticket not found or expired"))?;

    let expires_at = decode_expiry(payload.exp)?;
    if expires_at < Utc::now() {
        delete_ticket(state, &cache_key).await;
        return Err(AsterError::validation_error("stream ticket expired"));
    }

    ensure_scope_matches_ticket(scope, &payload)?;
    workspace_storage_service::require_scope_access(state, scope).await?;

    match payload.kind {
        StreamTicketKind::ArchiveDownload {
            file_ids,
            folder_ids,
            archive_name,
        } => Ok(task_service::types::CreateArchiveTaskParams {
            file_ids,
            folder_ids,
            archive_name: Some(archive_name),
        }),
        StreamTicketKind::SharedArchiveDownload { .. } => Err(AsterError::auth_forbidden(
            "stream ticket belongs to a shared archive download",
        )),
    }
}

pub(crate) async fn resolve_shared_archive_download_ticket(
    state: &impl SharedRuntimeState,
    share_token: &str,
    token: &str,
) -> Result<task_service::types::CreateArchiveTaskParams> {
    let cache_key = cache_key(token);
    let payload = load_ticket(state, &cache_key)
        .await
        .ok_or_else(|| AsterError::validation_error("stream ticket not found or expired"))?;

    let expires_at = decode_expiry(payload.exp)?;
    if expires_at < Utc::now() {
        delete_ticket(state, &cache_key).await;
        return Err(AsterError::validation_error("stream ticket expired"));
    }

    match payload.kind {
        StreamTicketKind::SharedArchiveDownload {
            share_token: ticket_share_token,
            file_ids,
            folder_ids,
            archive_name,
        } if ticket_share_token == share_token => {
            Ok(task_service::types::CreateArchiveTaskParams {
                file_ids,
                folder_ids,
                archive_name: Some(archive_name),
            })
        }
        StreamTicketKind::SharedArchiveDownload { .. } => Err(AsterError::auth_forbidden(
            "stream ticket belongs to a different share",
        )),
        StreamTicketKind::ArchiveDownload { .. } => Err(AsterError::auth_forbidden(
            "stream ticket belongs to a workspace archive download",
        )),
    }
}

fn cache_key(token: &str) -> String {
    format!("{STREAM_TICKET_CACHE_PREFIX}{token}")
}

async fn store_ticket(
    state: &impl SharedRuntimeState,
    cache_key: &str,
    payload: &StreamTicketPayload,
    ttl_secs: u64,
) -> Result<()> {
    if state.config().cache.enabled {
        state.cache().set(cache_key, payload, Some(ttl_secs)).await;
        if state
            .cache()
            .get::<StreamTicketPayload>(cache_key)
            .await
            .is_some()
        {
            return Ok(());
        }

        tracing::warn!(
            key = %cache_key,
            "stream ticket cache backend did not persist entry; falling back to local cache"
        );
    }

    FALLBACK_STREAM_TICKETS
        .insert(cache_key.to_string(), payload.clone())
        .await;
    Ok(())
}

async fn load_ticket(
    state: &impl SharedRuntimeState,
    cache_key: &str,
) -> Option<StreamTicketPayload> {
    if state.config().cache.enabled
        && let Some(payload) = state.cache().get::<StreamTicketPayload>(cache_key).await
    {
        return Some(payload);
    }

    FALLBACK_STREAM_TICKETS.get(cache_key).await
}

async fn delete_ticket(state: &impl SharedRuntimeState, cache_key: &str) {
    if state.config().cache.enabled {
        state.cache().delete(cache_key).await;
    }
    FALLBACK_STREAM_TICKETS.remove(cache_key).await;
}

fn ttl_secs_until(expires_at: DateTime<Utc>) -> Result<u64> {
    let ttl_secs = (expires_at - Utc::now()).num_seconds().max(1);
    u64::try_from(ttl_secs)
        .map_aster_err_with(|| AsterError::internal_error("stream ticket ttl overflow"))
}

fn decode_expiry(exp: i64) -> Result<DateTime<Utc>> {
    DateTime::from_timestamp(exp, 0)
        .ok_or_else(|| AsterError::validation_error("invalid stream ticket expiry"))
}

fn ensure_scope_matches_ticket(
    scope: WorkspaceStorageScope,
    payload: &StreamTicketPayload,
) -> Result<()> {
    match scope {
        WorkspaceStorageScope::Personal { user_id } => {
            if payload.team_id.is_some() {
                return Err(AsterError::auth_forbidden(
                    "stream ticket belongs to a team workspace",
                ));
            }
            if payload.actor_user_id != user_id {
                return Err(AsterError::auth_forbidden(
                    "stream ticket belongs to a different user",
                ));
            }
        }
        WorkspaceStorageScope::Team {
            team_id,
            actor_user_id,
        } => {
            if payload.team_id != Some(team_id) {
                return Err(AsterError::auth_forbidden(
                    "stream ticket is outside team workspace",
                ));
            }
            if payload.actor_user_id != actor_user_id {
                return Err(AsterError::auth_forbidden(
                    "stream ticket belongs to a different user",
                ));
            }
        }
    }

    Ok(())
}

fn stream_download_path(
    runtime_config: &crate::config::RuntimeConfig,
    scope: WorkspaceStorageScope,
    token: &str,
) -> String {
    let path = match scope {
        WorkspaceStorageScope::Personal { .. } => {
            format!("/api/v1/batch/archive-download/{token}")
        }
        WorkspaceStorageScope::Team { team_id, .. } => {
            format!("/api/v1/teams/{team_id}/batch/archive-download/{token}")
        }
    };

    site_url::public_app_url_or_path(runtime_config, &path)
}

fn shared_stream_download_path(
    runtime_config: &crate::config::RuntimeConfig,
    share_token: &str,
    token: &str,
) -> String {
    let path = format!("/api/v1/s/{share_token}/archive-download/{token}");
    site_url::public_app_url_or_path(runtime_config, &path)
}
