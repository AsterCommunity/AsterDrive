//! 认证服务子模块：`session`。

use aster_forge_db::transaction;
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ConnectionTrait, IntoActiveModel, Set};

use crate::api::api_error_code::ApiErrorCode;
use crate::db::repository::{auth_session_repo, user_repo};
use crate::errors::{AsterError, MapAsterErr, Result, auth_forbidden_with_code};
use crate::runtime::SharedRuntimeState;

use super::{AuthSessionInfo, AuthSnapshot, UserAuditInfo, cache, user_audit_info};

/// Loads current account authorization from the writer. Use this for security decisions,
/// including JWT generations, WOPI session validity and administrator permissions. Unlike the
/// cached presentation snapshot, this read cannot authorize from an old cache fill or replica.
///
/// # Errors
/// Returns the product missing-account or database error without substituting cached state.
pub async fn get_current_auth_snapshot(
    state: &impl SharedRuntimeState,
    user_id: i64,
) -> Result<AuthSnapshot> {
    let user = user_repo::find_by_id(state.writer_db(), user_id).await?;
    Ok(AuthSnapshot::from_user(&user))
}

/// Loads a cached account projection for presentation after authorization. Never use it to
/// decide account status, token validity or permissions; use `get_current_auth_snapshot` instead.
pub async fn get_auth_snapshot(
    state: &impl SharedRuntimeState,
    user_id: i64,
) -> Result<AuthSnapshot> {
    if let Some(snapshot) = cache::load_auth_snapshot(state, user_id).await {
        tracing::debug!(user_id, "auth snapshot cache hit");
        return Ok(snapshot);
    }

    let user = user_repo::find_by_id(state.reader_db(), user_id).await?;
    let snapshot = AuthSnapshot::from_user(&user);
    cache::store_auth_snapshot(state, user_id, &snapshot).await;
    tracing::debug!(user_id, "auth snapshot cache miss");
    Ok(snapshot)
}

pub async fn invalidate_auth_snapshot_cache(state: &impl SharedRuntimeState, user_id: i64) {
    cache::invalidate_auth_snapshot(state, user_id).await;
}

pub(crate) async fn purge_all_auth_sessions_in_connection<C: ConnectionTrait>(
    db: &C,
    user_id: i64,
) -> Result<()> {
    auth_session_repo::delete_all_for_user(db, user_id).await?;
    Ok(())
}

pub async fn list_auth_sessions(
    state: &impl SharedRuntimeState,
    user_id: i64,
    current_refresh_jti: Option<&str>,
) -> Result<Vec<AuthSessionInfo>> {
    auth_session_repo::list_active_for_user(state.writer_db(), user_id)
        .await
        .map(|sessions| {
            sessions
                .into_iter()
                .map(|session| AuthSessionInfo {
                    id: session.id,
                    is_current: current_refresh_jti
                        .is_some_and(|refresh_jti| refresh_jti == session.current_refresh_jti),
                    ip_address: session.ip_address,
                    user_agent: session.user_agent,
                    created_at: session.created_at,
                    last_seen_at: session.last_seen_at,
                    expires_at: session.refresh_expires_at,
                })
                .collect()
        })
}

pub async fn revoke_auth_session(
    state: &impl SharedRuntimeState,
    user_id: i64,
    session_id: &str,
    current_refresh_jti: Option<&str>,
) -> Result<bool> {
    let txn = transaction::begin(state.writer_db()).await?;
    let result = async {
        let session = auth_session_repo::find_by_id_for_user(&txn, user_id, session_id)
            .await?
            .ok_or_else(|| AsterError::record_not_found(format!("auth session '{session_id}'")))?;
        auth_session_repo::revoke_by_id_for_user(&txn, user_id, session_id, Utc::now()).await?;
        Ok::<bool, AsterError>(
            current_refresh_jti
                .is_some_and(|refresh_jti| refresh_jti == session.current_refresh_jti),
        )
    }
    .await;

    match result {
        Ok(revoked_current) => {
            transaction::commit(txn).await?;
            Ok(revoked_current)
        }
        Err(error) => {
            transaction::rollback(txn).await?;
            Err(error)
        }
    }
}

pub async fn revoke_other_auth_sessions(
    state: &impl SharedRuntimeState,
    user_id: i64,
    current_refresh_jti: &str,
) -> Result<u64> {
    let txn = transaction::begin(state.writer_db()).await?;
    let result = async {
        let current_session = auth_session_repo::find_by_refresh_jti(&txn, current_refresh_jti)
            .await?
            .ok_or_else(|| AsterError::auth_token_invalid("missing current session"))?;
        if current_session.user_id != user_id {
            return Err(auth_forbidden_with_code(
                ApiErrorCode::AuthSessionUserMismatch,
                "current session does not belong to user",
            ));
        }
        if current_session.revoked_at.is_some() {
            return Err(AsterError::auth_token_invalid("missing current session"));
        }
        let removed = auth_session_repo::revoke_all_for_user_except_id(
            &txn,
            user_id,
            &current_session.id,
            Utc::now(),
        )
        .await?;
        Ok::<u64, AsterError>(removed)
    }
    .await;

    match result {
        Ok(removed) => {
            transaction::commit(txn).await?;
            Ok(removed)
        }
        Err(error) => {
            transaction::rollback(txn).await?;
            Err(error)
        }
    }
}

pub async fn cleanup_expired_auth_sessions(state: &impl SharedRuntimeState) -> Result<u64> {
    auth_session_repo::delete_expired(state.writer_db()).await
}

/// Atomically advances the account generation and purges all refresh sessions, then evicts cached
/// presentation state. Access authorization checks the writer generation and cannot be restored by
/// a delayed cache fill.
///
/// # Errors
/// Returns writer/commit errors or generation exhaustion; failed transactions leave sessions intact.
pub async fn revoke_user_sessions(
    state: &impl SharedRuntimeState,
    user_id: i64,
) -> Result<UserAuditInfo> {
    tracing::debug!(user_id, "revoking user sessions");
    let txn = transaction::begin(state.writer_db()).await?;
    let result = async {
        let user = user_repo::find_by_id(&txn, user_id).await?;
        // Atomic advancement shares the refresh/password/MFA invalidation boundary.
        user_repo::bump_session_version(&txn, user_id).await?;
        let mut active = user.into_active_model();
        active.updated_at = Set(Utc::now());
        let updated = active
            .update(&txn)
            .await
            .map_aster_err(AsterError::database_operation)?;
        purge_all_auth_sessions_in_connection(&txn, updated.id).await?;
        Ok::<_, AsterError>(updated)
    }
    .await;

    let updated = match result {
        Ok(updated) => {
            transaction::commit(txn).await?;
            updated
        }
        Err(error) => {
            transaction::rollback(txn).await?;
            return Err(error);
        }
    };

    invalidate_auth_snapshot_cache(state, updated.id).await;
    tracing::debug!(
        user_id = updated.id,
        session_version = updated.session_version,
        "revoked user sessions"
    );
    Ok(user_audit_info(&updated))
}
