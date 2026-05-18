//! WOPI 服务子模块：`locks`。

use chrono::{Duration, Utc};
use sea_orm::{ActiveModelTrait, Set};

use crate::config::wopi;
use crate::db::repository::lock_repo;
use crate::entities::{file, resource_lock};
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::runtime::PrimaryAppState;
use crate::services::{
    audit_service::{self, AuditRequestInfo},
    lock_service,
};
use crate::types::EntityType;

use super::session::{WopiAccessTokenPayload, resolve_access_token};
use super::types::{
    MAX_WOPI_LOCK_LEN, WopiConflict, WopiGetLockResult, WopiLockOperationResult, WopiLockPayload,
    WopiRequestSource,
};

#[derive(Debug, Clone)]
pub(crate) struct ActiveWopiLock {
    pub(crate) lock: resource_lock::Model,
    pub(crate) payload: Option<WopiLockPayload>,
}

pub async fn get_lock(
    state: &PrimaryAppState,
    file_id: i64,
    access_token: &str,
    request_source: WopiRequestSource<'_>,
) -> Result<WopiGetLockResult> {
    let resolved = resolve_access_token(state, file_id, access_token, request_source).await?;
    let Some(active_lock) = load_active_lock(state, resolved.file.id).await? else {
        return Ok(WopiGetLockResult::Success {
            current_lock: String::new(),
        });
    };

    match active_lock.payload {
        Some(payload) => Ok(WopiGetLockResult::Success {
            current_lock: payload.lock,
        }),
        None => Ok(WopiGetLockResult::Conflict(WopiConflict {
            current_lock: Some(String::new()),
            reason: "file is locked outside WOPI".to_string(),
        })),
    }
}

pub async fn lock_file(
    state: &PrimaryAppState,
    file_id: i64,
    access_token: &str,
    requested_lock: &str,
    request_info: &AuditRequestInfo,
    request_source: WopiRequestSource<'_>,
) -> Result<WopiLockOperationResult> {
    let resolved = resolve_access_token(state, file_id, access_token, request_source).await?;
    let lock_value = normalize_wopi_lock_header("X-WOPI-Lock", requested_lock)?;
    let active_lock = load_active_lock(state, resolved.file.id).await?;

    if let Some(active_lock) = active_lock {
        if let Some(payload) = active_lock.payload {
            // WOPI lock 是“文件 + opaque lock ID”，不是“文件 + 用户/app”。
            // 官方 key concepts 明确要求 lock 不能绑定到特定用户。
            if payload.lock == lock_value {
                refresh_lock_model(state, active_lock.lock).await?;
                log_wopi_lock_action(
                    state,
                    request_info,
                    resolved.payload.actor_user_id,
                    audit_service::AuditAction::FileLock,
                    &resolved.file,
                )
                .await;
                return Ok(WopiLockOperationResult::Success);
            }

            return Ok(WopiLockOperationResult::Conflict(WopiConflict {
                current_lock: Some(payload.lock),
                reason: "file is locked by another WOPI session".to_string(),
            }));
        }

        return Ok(WopiLockOperationResult::Conflict(WopiConflict {
            current_lock: Some(String::new()),
            reason: "file is locked outside WOPI".to_string(),
        }));
    }

    create_wopi_lock(state, &resolved.payload, &resolved.file, &lock_value).await?;
    log_wopi_lock_action(
        state,
        request_info,
        resolved.payload.actor_user_id,
        audit_service::AuditAction::FileLock,
        &resolved.file,
    )
    .await;
    Ok(WopiLockOperationResult::Success)
}

pub async fn unlock_and_relock_file(
    state: &PrimaryAppState,
    file_id: i64,
    access_token: &str,
    requested_lock: &str,
    old_lock: &str,
    request_info: &AuditRequestInfo,
    request_source: WopiRequestSource<'_>,
) -> Result<WopiLockOperationResult> {
    let resolved = resolve_access_token(state, file_id, access_token, request_source).await?;
    let new_lock = normalize_wopi_lock_header("X-WOPI-Lock", requested_lock)?;
    let old_lock = normalize_wopi_lock_header("X-WOPI-OldLock", old_lock)?;
    let Some(active_lock) = load_active_lock(state, resolved.file.id).await? else {
        return Ok(WopiLockOperationResult::Conflict(WopiConflict {
            current_lock: Some(String::new()),
            reason: "file is not locked".to_string(),
        }));
    };

    match active_lock.payload {
        Some(payload) if payload.lock == old_lock => {
            replace_wopi_lock_model(state, active_lock.lock, &resolved.payload, &new_lock).await?;
            log_wopi_lock_action(
                state,
                request_info,
                resolved.payload.actor_user_id,
                audit_service::AuditAction::FileLock,
                &resolved.file,
            )
            .await;
            Ok(WopiLockOperationResult::Success)
        }
        Some(payload) => Ok(WopiLockOperationResult::Conflict(WopiConflict {
            current_lock: Some(payload.lock),
            reason: "WOPI lock mismatch".to_string(),
        })),
        None => Ok(WopiLockOperationResult::Conflict(WopiConflict {
            current_lock: Some(String::new()),
            reason: "file is locked outside WOPI".to_string(),
        })),
    }
}

pub async fn refresh_lock(
    state: &PrimaryAppState,
    file_id: i64,
    access_token: &str,
    requested_lock: &str,
    request_info: &AuditRequestInfo,
    request_source: WopiRequestSource<'_>,
) -> Result<WopiLockOperationResult> {
    let resolved = resolve_access_token(state, file_id, access_token, request_source).await?;
    let lock_value = normalize_wopi_lock_header("X-WOPI-Lock", requested_lock)?;
    let Some(active_lock) = load_active_lock(state, resolved.file.id).await? else {
        return Ok(WopiLockOperationResult::Conflict(WopiConflict {
            current_lock: Some(String::new()),
            reason: "file is not locked".to_string(),
        }));
    };

    match active_lock.payload {
        Some(payload) if payload.lock == lock_value => {
            refresh_lock_model(state, active_lock.lock).await?;
            log_wopi_lock_action(
                state,
                request_info,
                resolved.payload.actor_user_id,
                audit_service::AuditAction::FileLock,
                &resolved.file,
            )
            .await;
            Ok(WopiLockOperationResult::Success)
        }
        Some(payload) => Ok(WopiLockOperationResult::Conflict(WopiConflict {
            current_lock: Some(payload.lock),
            reason: "WOPI lock mismatch".to_string(),
        })),
        None => Ok(WopiLockOperationResult::Conflict(WopiConflict {
            current_lock: Some(String::new()),
            reason: "file is locked outside WOPI".to_string(),
        })),
    }
}

pub async fn unlock_file(
    state: &PrimaryAppState,
    file_id: i64,
    access_token: &str,
    requested_lock: &str,
    request_info: &AuditRequestInfo,
    request_source: WopiRequestSource<'_>,
) -> Result<WopiLockOperationResult> {
    let resolved = resolve_access_token(state, file_id, access_token, request_source).await?;
    let lock_value = normalize_wopi_lock_header("X-WOPI-Lock", requested_lock)?;
    let Some(active_lock) = load_active_lock(state, resolved.file.id).await? else {
        return Ok(WopiLockOperationResult::Conflict(WopiConflict {
            current_lock: Some(String::new()),
            reason: "file is not locked".to_string(),
        }));
    };

    match active_lock.payload {
        Some(payload) if payload.lock == lock_value => {
            lock_service::set_entity_locked(&state.db, EntityType::File, resolved.file.id, false)
                .await?;
            lock_repo::delete_by_id(&state.db, active_lock.lock.id).await?;
            log_wopi_lock_action(
                state,
                request_info,
                resolved.payload.actor_user_id,
                audit_service::AuditAction::FileUnlock,
                &resolved.file,
            )
            .await;
            Ok(WopiLockOperationResult::Success)
        }
        Some(payload) => Ok(WopiLockOperationResult::Conflict(WopiConflict {
            current_lock: Some(payload.lock),
            reason: "WOPI lock mismatch".to_string(),
        })),
        None => Ok(WopiLockOperationResult::Conflict(WopiConflict {
            current_lock: Some(String::new()),
            reason: "file is locked outside WOPI".to_string(),
        })),
    }
}

async fn log_wopi_lock_action(
    state: &PrimaryAppState,
    request_info: &AuditRequestInfo,
    actor_user_id: i64,
    action: audit_service::AuditAction,
    file: &file::Model,
) {
    let audit_ctx = request_info.to_context(actor_user_id);
    audit_service::log(
        state,
        &audit_ctx,
        action,
        crate::services::audit_service::AuditEntityType::File,
        Some(file.id),
        Some(&file.name),
        Some(serde_json::json!({ "source": "wopi" })),
    )
    .await;
}

pub(crate) async fn ensure_wopi_lock_matches(
    state: &PrimaryAppState,
    file_id: i64,
    requested_lock: Option<&str>,
) -> Result<Option<WopiConflict>> {
    let Some(active_lock) = load_active_lock(state, file_id).await? else {
        return Ok(None);
    };

    ensure_active_wopi_lock_matches(&active_lock, requested_lock)
}

pub(crate) async fn ensure_wopi_putfile_lock_matches(
    state: &PrimaryAppState,
    file: &file::Model,
    requested_lock: Option<&str>,
) -> Result<Option<WopiConflict>> {
    let Some(active_lock) = load_active_lock(state, file.id).await? else {
        if file.size == 0 {
            return Ok(None);
        }

        return Ok(Some(WopiConflict {
            current_lock: Some(String::new()),
            reason: "existing file requires a WOPI lock".to_string(),
        }));
    };

    ensure_active_wopi_lock_matches(&active_lock, requested_lock)
}

fn ensure_active_wopi_lock_matches(
    active_lock: &ActiveWopiLock,
    requested_lock: Option<&str>,
) -> Result<Option<WopiConflict>> {
    let Some(lock_payload) = active_lock.payload.as_ref() else {
        return Ok(Some(WopiConflict {
            current_lock: Some(String::new()),
            reason: "file is locked outside WOPI".to_string(),
        }));
    };

    let requested_lock = requested_lock
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AsterError::validation_error("X-WOPI-Lock header is required"))?;

    if lock_payload.lock == requested_lock {
        return Ok(None);
    }

    Ok(Some(WopiConflict {
        current_lock: Some(lock_payload.lock.clone()),
        reason: "WOPI lock mismatch".to_string(),
    }))
}

pub(crate) async fn load_active_lock(
    state: &PrimaryAppState,
    file_id: i64,
) -> Result<Option<ActiveWopiLock>> {
    let Some(lock) = lock_repo::find_by_entity(&state.db, EntityType::File, file_id).await? else {
        return Ok(None);
    };

    if let Some(timeout_at) = lock.timeout_at
        && timeout_at < Utc::now()
    {
        lock_repo::delete_by_id(&state.db, lock.id).await?;
        lock_service::set_entity_locked(&state.db, EntityType::File, file_id, false).await?;
        return Ok(None);
    }

    Ok(Some(ActiveWopiLock {
        payload: parse_wopi_lock_payload(&lock)?,
        lock,
    }))
}

pub(crate) fn active_wopi_lock_value(active_lock: &ActiveWopiLock) -> Option<String> {
    active_lock
        .payload
        .as_ref()
        .map(|payload| payload.lock.clone())
}

async fn create_wopi_lock(
    state: &PrimaryAppState,
    payload: &WopiAccessTokenPayload,
    file: &file::Model,
    requested_lock: &str,
) -> Result<()> {
    let path = lock_service::resolve_entity_path(&state.db, EntityType::File, file.id).await?;
    let now = Utc::now();
    let timeout_at = now + Duration::seconds(wopi::lock_ttl_secs(&state.runtime_config));
    let owner_info = lock_service::ResourceLockOwnerInfo::Wopi(lock_service::WopiLockOwnerInfo {
        app_key: payload.app_key.clone(),
        lock: requested_lock.to_string(),
    });

    let model = resource_lock::ActiveModel {
        token: Set(format!("wopi:{}", uuid::Uuid::new_v4())),
        entity_type: Set(EntityType::File),
        entity_id: Set(file.id),
        path: Set(path),
        owner_id: Set(Some(payload.actor_user_id)),
        owner_info: Set(lock_service::serialize_resource_lock_owner_info(Some(
            &owner_info,
        ))?),
        timeout_at: Set(Some(timeout_at)),
        shared: Set(false),
        deep: Set(false),
        created_at: Set(now),
        ..Default::default()
    };

    lock_repo::create(&state.db, model).await?;
    lock_service::set_entity_locked(&state.db, EntityType::File, file.id, true).await?;
    Ok(())
}

async fn refresh_lock_model(state: &PrimaryAppState, lock: resource_lock::Model) -> Result<()> {
    let mut active: resource_lock::ActiveModel = lock.into();
    active.timeout_at = Set(Some(
        Utc::now() + Duration::seconds(wopi::lock_ttl_secs(&state.runtime_config)),
    ));
    active
        .update(&state.db)
        .await
        .map_aster_err(AsterError::database_operation)?;
    Ok(())
}

async fn replace_wopi_lock_model(
    state: &PrimaryAppState,
    lock: resource_lock::Model,
    payload: &WopiAccessTokenPayload,
    requested_lock: &str,
) -> Result<()> {
    let mut active: resource_lock::ActiveModel = lock.into();
    // app_key 仍保留在 owner_info 里做审计/排障，但 lock 是否匹配只比较 opaque lock ID。
    let owner_info = lock_service::ResourceLockOwnerInfo::Wopi(lock_service::WopiLockOwnerInfo {
        app_key: payload.app_key.clone(),
        lock: requested_lock.to_string(),
    });
    active.owner_info = Set(lock_service::serialize_resource_lock_owner_info(Some(
        &owner_info,
    ))?);
    active.timeout_at = Set(Some(
        Utc::now() + Duration::seconds(wopi::lock_ttl_secs(&state.runtime_config)),
    ));
    active
        .update(&state.db)
        .await
        .map_aster_err(AsterError::database_operation)?;
    Ok(())
}

fn parse_wopi_lock_payload(lock: &resource_lock::Model) -> Result<Option<WopiLockPayload>> {
    match lock_service::deserialize_resource_lock_owner_info(lock)? {
        Some(lock_service::ResourceLockOwnerInfo::Wopi(payload)) => Ok(Some(WopiLockPayload {
            kind: "wopi".to_string(),
            app_key: payload.app_key,
            lock: payload.lock,
        })),
        Some(_) => Ok(None),
        None => Ok(None),
    }
}

fn normalize_wopi_lock_header(header_name: &str, value: &str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(AsterError::validation_error(format!(
            "{header_name} header must not be empty"
        )));
    }
    if !trimmed.is_ascii() {
        return Err(AsterError::validation_error(format!(
            "{header_name} header must contain ASCII characters only"
        )));
    }
    if trimmed.len() > MAX_WOPI_LOCK_LEN {
        return Err(AsterError::validation_error(format!(
            "{header_name} header must be {MAX_WOPI_LOCK_LEN} bytes or fewer"
        )));
    }
    Ok(trimmed.to_string())
}
