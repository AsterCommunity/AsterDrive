//! WOPI 服务子模块：`locks`。

use chrono::{Duration, Utc};
use sea_orm::{ActiveModelTrait, Set};

use crate::config::wopi;
use crate::db::repository::lock_repo;
use crate::entities::{file, resource_lock};
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::runtime::SharedRuntimeState;
use crate::services::{
    files::file as file_ops,
    files::lock,
    ops::audit::{self, AuditRequestInfo},
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

#[derive(Debug, Clone)]
pub(crate) enum ActiveWopiLockState {
    None,
    Single(ActiveWopiLock),
    Conflict(WopiConflict),
}

pub async fn get_lock(
    state: &impl SharedRuntimeState,
    file_id: i64,
    access_token: &str,
    request_source: WopiRequestSource<'_>,
) -> Result<WopiGetLockResult> {
    let resolved = resolve_access_token(state, file_id, access_token, request_source).await?;
    match load_active_lock(state, resolved.file.id).await? {
        ActiveWopiLockState::None => Ok(WopiGetLockResult::Success {
            current_lock: String::new(),
        }),
        ActiveWopiLockState::Single(active_lock) => match active_lock.payload {
            Some(payload) => Ok(WopiGetLockResult::Success {
                current_lock: payload.lock,
            }),
            None => Ok(WopiGetLockResult::Conflict(outside_wopi_lock_conflict())),
        },
        ActiveWopiLockState::Conflict(conflict) => Ok(WopiGetLockResult::Conflict(conflict)),
    }
}

pub async fn lock_file(
    state: &impl SharedRuntimeState,
    file_id: i64,
    access_token: &str,
    requested_lock: &str,
    request_info: &AuditRequestInfo,
    request_source: WopiRequestSource<'_>,
) -> Result<WopiLockOperationResult> {
    let resolved = resolve_access_token(state, file_id, access_token, request_source).await?;
    let lock_value = normalize_wopi_lock_header("X-WOPI-Lock", requested_lock)?;
    match load_active_lock(state, resolved.file.id).await? {
        ActiveWopiLockState::None => {}
        ActiveWopiLockState::Single(active_lock) => {
            if let Some(payload) = active_lock.payload {
                // WOPI lock 是“文件 + opaque lock ID”，不是“文件 + 用户/app”。
                // 官方 key concepts 明确要求 lock 不能绑定到特定用户。
                if payload.lock == lock_value {
                    refresh_lock_model(state, active_lock.lock).await?;
                    log_wopi_lock_action(
                        state,
                        request_info,
                        &resolved.payload,
                        audit::AuditAction::FileLock,
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

            return Ok(WopiLockOperationResult::Conflict(
                outside_wopi_lock_conflict(),
            ));
        }
        ActiveWopiLockState::Conflict(conflict) => {
            return Ok(WopiLockOperationResult::Conflict(conflict));
        }
    }

    match create_wopi_lock(state, &resolved.payload, &resolved.file, &lock_value).await {
        Ok(()) => {}
        Err(error) => {
            return map_create_wopi_lock_error(error, || {
                concurrent_wopi_lock_conflict(state, resolved.file.id)
            })
            .await;
        }
    }

    log_wopi_lock_action(
        state,
        request_info,
        &resolved.payload,
        audit::AuditAction::FileLock,
        &resolved.file,
    )
    .await;
    Ok(WopiLockOperationResult::Success)
}

pub async fn unlock_and_relock_file(
    state: &impl SharedRuntimeState,
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
    let active_lock = match load_active_lock(state, resolved.file.id).await? {
        ActiveWopiLockState::None => {
            return Ok(WopiLockOperationResult::Conflict(WopiConflict {
                current_lock: Some(String::new()),
                reason: "file is not locked".to_string(),
            }));
        }
        ActiveWopiLockState::Single(active_lock) => active_lock,
        ActiveWopiLockState::Conflict(conflict) => {
            return Ok(WopiLockOperationResult::Conflict(conflict));
        }
    };

    match active_lock.payload {
        Some(payload) if payload.lock == old_lock => {
            replace_wopi_lock_model(state, active_lock.lock, &resolved.payload, &new_lock).await?;
            log_wopi_lock_action(
                state,
                request_info,
                &resolved.payload,
                audit::AuditAction::FileLock,
                &resolved.file,
            )
            .await;
            Ok(WopiLockOperationResult::Success)
        }
        Some(payload) => Ok(WopiLockOperationResult::Conflict(WopiConflict {
            current_lock: Some(payload.lock),
            reason: "WOPI lock mismatch".to_string(),
        })),
        None => Ok(WopiLockOperationResult::Conflict(
            outside_wopi_lock_conflict(),
        )),
    }
}

pub async fn refresh_lock(
    state: &impl SharedRuntimeState,
    file_id: i64,
    access_token: &str,
    requested_lock: &str,
    request_info: &AuditRequestInfo,
    request_source: WopiRequestSource<'_>,
) -> Result<WopiLockOperationResult> {
    let resolved = resolve_access_token(state, file_id, access_token, request_source).await?;
    let lock_value = normalize_wopi_lock_header("X-WOPI-Lock", requested_lock)?;
    let active_lock = match load_active_lock(state, resolved.file.id).await? {
        ActiveWopiLockState::None => {
            return Ok(WopiLockOperationResult::Conflict(WopiConflict {
                current_lock: Some(String::new()),
                reason: "file is not locked".to_string(),
            }));
        }
        ActiveWopiLockState::Single(active_lock) => active_lock,
        ActiveWopiLockState::Conflict(conflict) => {
            return Ok(WopiLockOperationResult::Conflict(conflict));
        }
    };

    match active_lock.payload {
        Some(payload) if payload.lock == lock_value => {
            refresh_lock_model(state, active_lock.lock).await?;
            log_wopi_lock_action(
                state,
                request_info,
                &resolved.payload,
                audit::AuditAction::FileLock,
                &resolved.file,
            )
            .await;
            Ok(WopiLockOperationResult::Success)
        }
        Some(payload) => Ok(WopiLockOperationResult::Conflict(WopiConflict {
            current_lock: Some(payload.lock),
            reason: "WOPI lock mismatch".to_string(),
        })),
        None => Ok(WopiLockOperationResult::Conflict(
            outside_wopi_lock_conflict(),
        )),
    }
}

pub async fn unlock_file(
    state: &impl SharedRuntimeState,
    file_id: i64,
    access_token: &str,
    requested_lock: &str,
    request_info: &AuditRequestInfo,
    request_source: WopiRequestSource<'_>,
) -> Result<WopiLockOperationResult> {
    let resolved = resolve_access_token(state, file_id, access_token, request_source).await?;
    let lock_value = normalize_wopi_lock_header("X-WOPI-Lock", requested_lock)?;
    let active_lock = match load_active_lock(state, resolved.file.id).await? {
        ActiveWopiLockState::None => {
            return Ok(WopiLockOperationResult::Conflict(WopiConflict {
                current_lock: Some(String::new()),
                reason: "file is not locked".to_string(),
            }));
        }
        ActiveWopiLockState::Single(active_lock) => active_lock,
        ActiveWopiLockState::Conflict(conflict) => {
            return Ok(WopiLockOperationResult::Conflict(conflict));
        }
    };

    match active_lock.payload {
        Some(payload) if payload.lock == lock_value => {
            lock_repo::delete_by_id(state.writer_db(), active_lock.lock.id).await?;
            lock::clear_entity_locked_if_unlocked(
                state.writer_db(),
                EntityType::File,
                resolved.file.id,
            )
            .await?;
            log_wopi_lock_action(
                state,
                request_info,
                &resolved.payload,
                audit::AuditAction::FileUnlock,
                &resolved.file,
            )
            .await;
            Ok(WopiLockOperationResult::Success)
        }
        Some(payload) => Ok(WopiLockOperationResult::Conflict(WopiConflict {
            current_lock: Some(payload.lock),
            reason: "WOPI lock mismatch".to_string(),
        })),
        None => Ok(WopiLockOperationResult::Conflict(
            outside_wopi_lock_conflict(),
        )),
    }
}

async fn log_wopi_lock_action(
    state: &impl SharedRuntimeState,
    request_info: &AuditRequestInfo,
    payload: &WopiAccessTokenPayload,
    action: audit::AuditAction,
    file: &file::Model,
) {
    let audit_ctx = request_info.to_context(payload.actor_user_id);
    let details = file_ops::audit_location_details_for_model(
        state,
        super::session::scope_from_payload(payload),
        file,
    )
    .await;
    audit::log_with_details(
        state,
        &audit_ctx,
        action,
        crate::services::ops::audit::AuditEntityType::File,
        Some(file.id),
        Some(&file.name),
        || details.clone(),
    )
    .await;
}

pub(crate) async fn ensure_wopi_lock_matches(
    state: &impl SharedRuntimeState,
    file_id: i64,
    requested_lock: Option<&str>,
) -> Result<Option<WopiConflict>> {
    match load_active_lock(state, file_id).await? {
        ActiveWopiLockState::None => Ok(None),
        ActiveWopiLockState::Single(active_lock) => {
            ensure_active_wopi_lock_matches(&active_lock, requested_lock)
        }
        ActiveWopiLockState::Conflict(conflict) => Ok(Some(conflict)),
    }
}

pub(crate) async fn ensure_wopi_putfile_lock_matches(
    state: &impl SharedRuntimeState,
    file: &file::Model,
    requested_lock: Option<&str>,
) -> Result<Option<WopiConflict>> {
    match load_active_lock(state, file.id).await? {
        ActiveWopiLockState::None => {
            if file.size == 0 {
                return Ok(None);
            }

            Ok(Some(WopiConflict {
                current_lock: Some(String::new()),
                reason: "existing file requires a WOPI lock".to_string(),
            }))
        }
        ActiveWopiLockState::Single(active_lock) => {
            ensure_active_wopi_lock_matches(&active_lock, requested_lock)
        }
        ActiveWopiLockState::Conflict(conflict) => Ok(Some(conflict)),
    }
}

fn ensure_active_wopi_lock_matches(
    active_lock: &ActiveWopiLock,
    requested_lock: Option<&str>,
) -> Result<Option<WopiConflict>> {
    let Some(lock_payload) = active_lock.payload.as_ref() else {
        return Ok(Some(outside_wopi_lock_conflict()));
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
    state: &impl SharedRuntimeState,
    file_id: i64,
) -> Result<ActiveWopiLockState> {
    let mut active_locks = Vec::new();
    let now = Utc::now();

    for lock in lock_repo::find_all_by_entity(state.writer_db(), EntityType::File, file_id).await? {
        if lock.timeout_at.is_some_and(|timeout_at| timeout_at < now) {
            lock_repo::delete_by_id(state.writer_db(), lock.id).await?;
            continue;
        }

        active_locks.push(ActiveWopiLock {
            payload: parse_wopi_lock_payload(&lock)?,
            lock,
        });
    }

    if active_locks.is_empty() {
        lock::clear_entity_locked_if_unlocked(state.writer_db(), EntityType::File, file_id).await?;
        return Ok(ActiveWopiLockState::None);
    }

    if active_locks.len() > 1 || active_locks.iter().any(|active| active.payload.is_none()) {
        return Ok(ActiveWopiLockState::Conflict(outside_wopi_lock_conflict()));
    }

    Ok(ActiveWopiLockState::Single(
        active_locks
            .into_iter()
            .next()
            .ok_or_else(|| AsterError::internal_error("active WOPI lock disappeared"))?,
    ))
}

pub(crate) fn active_wopi_lock_value(active_lock: &ActiveWopiLock) -> Option<String> {
    active_lock
        .payload
        .as_ref()
        .map(|payload| payload.lock.clone())
}

async fn create_wopi_lock(
    state: &impl SharedRuntimeState,
    payload: &WopiAccessTokenPayload,
    file: &file::Model,
    requested_lock: &str,
) -> Result<()> {
    let owner_info = lock::ResourceLockOwnerInfo::Wopi(lock::WopiLockOwnerInfo {
        app_key: payload.app_key.clone(),
        lock: requested_lock.to_string(),
    });

    lock::lock(
        state,
        EntityType::File,
        file.id,
        Some(payload.actor_user_id),
        Some(owner_info),
        Some(Duration::seconds(wopi::lock_ttl_secs(
            state.runtime_config(),
        ))),
    )
    .await?;
    Ok(())
}

async fn concurrent_wopi_lock_conflict(
    state: &impl SharedRuntimeState,
    file_id: i64,
) -> Result<WopiConflict> {
    match load_active_lock(state, file_id).await? {
        ActiveWopiLockState::Single(active_lock) => Ok(WopiConflict {
            current_lock: Some(active_wopi_lock_value(&active_lock).unwrap_or_default()),
            reason: "file is locked by another WOPI session".to_string(),
        }),
        ActiveWopiLockState::Conflict(conflict) => Ok(conflict),
        ActiveWopiLockState::None => Ok(WopiConflict {
            current_lock: Some(String::new()),
            reason: "file lock conflicted with a concurrent request".to_string(),
        }),
    }
}

fn outside_wopi_lock_conflict() -> WopiConflict {
    WopiConflict {
        current_lock: Some(String::new()),
        reason: "file is locked outside WOPI".to_string(),
    }
}

async fn map_create_wopi_lock_error<F, Fut>(
    error: AsterError,
    conflict_loader: F,
) -> Result<WopiLockOperationResult>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<WopiConflict>>,
{
    if matches!(error, AsterError::ResourceLocked(_)) {
        return Ok(WopiLockOperationResult::Conflict(conflict_loader().await?));
    }

    Err(error)
}

async fn refresh_lock_model(
    state: &impl SharedRuntimeState,
    lock: resource_lock::Model,
) -> Result<()> {
    let mut active: resource_lock::ActiveModel = lock.into();
    active.timeout_at = Set(Some(
        Utc::now() + Duration::seconds(wopi::lock_ttl_secs(state.runtime_config())),
    ));
    active
        .update(state.writer_db())
        .await
        .map_aster_err(AsterError::database_operation)?;
    Ok(())
}

async fn replace_wopi_lock_model(
    state: &impl SharedRuntimeState,
    lock: resource_lock::Model,
    payload: &WopiAccessTokenPayload,
    requested_lock: &str,
) -> Result<()> {
    let mut active: resource_lock::ActiveModel = lock.into();
    // app_key 仍保留在 owner_info 里做审计/排障，但 lock 是否匹配只比较 opaque lock ID。
    let owner_info = lock::ResourceLockOwnerInfo::Wopi(lock::WopiLockOwnerInfo {
        app_key: payload.app_key.clone(),
        lock: requested_lock.to_string(),
    });
    active.owner_info = Set(lock::serialize_resource_lock_owner_info(Some(&owner_info))?);
    active.timeout_at = Set(Some(
        Utc::now() + Duration::seconds(wopi::lock_ttl_secs(state.runtime_config())),
    ));
    active
        .update(state.writer_db())
        .await
        .map_aster_err(AsterError::database_operation)?;
    Ok(())
}

fn parse_wopi_lock_payload(lock: &resource_lock::Model) -> Result<Option<WopiLockPayload>> {
    match lock::deserialize_resource_lock_owner_info(lock)? {
        Some(lock::ResourceLockOwnerInfo::Wopi(payload)) => Ok(Some(WopiLockPayload {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn create_lock_resource_locked_error_maps_to_wopi_conflict() {
        let result = map_create_wopi_lock_error(
            AsterError::resource_locked("resource is already locked"),
            || async {
                Ok(WopiConflict {
                    current_lock: Some("lock-a".to_string()),
                    reason: "file is locked by another WOPI session".to_string(),
                })
            },
        )
        .await
        .expect("resource locked should map to WOPI conflict");

        match result {
            WopiLockOperationResult::Conflict(conflict) => {
                assert_eq!(conflict.current_lock.as_deref(), Some("lock-a"));
                assert!(conflict.reason.contains("another WOPI session"));
            }
            WopiLockOperationResult::Success => panic!("expected WOPI conflict"),
        }
    }

    #[tokio::test]
    async fn create_lock_non_resource_locked_error_is_propagated() {
        let error =
            map_create_wopi_lock_error(AsterError::database_operation("insert failed"), || async {
                Ok(WopiConflict {
                    current_lock: Some("lock-a".to_string()),
                    reason: "unused".to_string(),
                })
            })
            .await
            .expect_err("non-lock errors should propagate");

        assert!(matches!(error, AsterError::DatabaseOperation(_)));
    }
}
