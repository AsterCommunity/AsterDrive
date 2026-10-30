//! Drive refresh adapter: Forge owns the state machine; Drive owns writer transactions, account
//! policy, session persistence, and transactional security audit.

use aster_forge_auth::{
    ClientContext, RefreshDecision, RefreshError, RefreshJti, RefreshOutcome, RefreshPolicy,
    RefreshRejection, RefreshRotationRequest, RefreshSession, RefreshSessionStore,
    RefreshStoreOutcome, RefreshTokenService, Restriction, SessionId, Subject, classify_refresh,
};
use chrono::Utc;
use sea_orm::ConnectionTrait;
use std::sync::Arc;

use super::{auth_policy, map_forge_auth_error};
use crate::api::api_error_code::ApiErrorCode;
use crate::db::repository::{auth_session_repo, user_repo};
use crate::errors::{AsterError, Result, auth_forbidden_with_code};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::ops::audit::AuditEntityType;
use aster_drive_model::entities::auth_session;
use aster_drive_model::types::AuditAction;

/// Refreshes a browser session using the current product TTL snapshot and strict signing namespace.
/// The shared coordinator survives request-scoped signers, dynamic config reloads and cloned state.
///
/// Reuse rejection is returned only after version/purge/audit commit.
/// Authorization reads current writer state and does not depend on cache eviction. A database
/// failure remains a product infrastructure error, never a fabricated authentication success.
///
/// # Errors
/// Returns Drive's mapped authentication rejection or the original store/commit error. No failed
/// outcome releases candidate tokens; confirmed reuse is returned only after security effects commit.
pub async fn refresh_tokens(
    state: &PrimaryAppState,
    refresh: &str,
    ip_address: Option<&str>,
    user_agent: Option<&str>,
) -> Result<(String, String)> {
    let result = async {
        let policy = auth_policy(state)?;
        let key = aster_forge_auth::JwtKey::hmac(state.config().auth.jwt_secret.as_bytes())
            .map_err(map_forge_auth_error)?;
        let service = RefreshTokenService::new(
            aster_forge_auth::JwtSigner::with_system_clock(key.clone(), policy.clone()),
            aster_forge_auth::JwtVerifier::with_system_clock(key, policy),
            Arc::new(DriveRefreshStore {
                state: state.clone(),
            }),
            Arc::new(aster_forge_auth::SystemClock),
            // This is a rejection-classification window, never a second-success grace period.
            RefreshPolicy::SameClientWithin { seconds: 15 },
            state.runtime_config().refresh_coordinator().clone(),
        );
        match service
            .refresh(
                refresh,
                ClientContext {
                    ip_address: ip_address.map(str::to_string),
                    user_agent: user_agent.map(str::to_string),
                },
            )
            .await
        {
            Ok(RefreshOutcome::Rotated(pair)) => Ok((pair.access_token, pair.refresh_token)),
            Ok(RefreshOutcome::Rejected(reason)) => Err(map_refresh_rejection(reason)),
            Err(RefreshError::Auth(error)) => Err(map_forge_auth_error(error)),
            Err(RefreshError::Store(error)) => Err(error),
        }
    }
    .await;
    record_refresh_metric(state, &result);
    result
}

/// Maps mechanism facts into Drive's existing HTTP error codes. Storage errors bypass this mapping.
fn map_refresh_rejection(reason: RefreshRejection) -> AsterError {
    match reason {
        RefreshRejection::Unavailable => AsterError::auth_token_invalid("session revoked"),
        RefreshRejection::Expired => AsterError::auth_token_expired("session expired"),
        RefreshRejection::AccountDisabled => {
            auth_forbidden_with_code(ApiErrorCode::AuthAccountDisabled, "account is disabled")
        }
        RefreshRejection::Restricted => auth_forbidden_with_code(
            ApiErrorCode::AuthPasswordChangeRequired,
            "password change required",
        ),
        RefreshRejection::Stale => AsterError::auth_refresh_token_stale("stale refresh token"),
        RefreshRejection::ReuseDetected => {
            AsterError::auth_refresh_token_reuse_detected("refresh token reuse detected")
        }
    }
}

/// Product store keeps minimal owned runtime resources; it does not create another session table.
struct DriveRefreshStore {
    state: PrimaryAppState,
}

impl DriveRefreshStore {
    /// Rechecks product account state before inspecting current/old JTIs. The account row lock
    /// serializes rotation with password/revocation writes on PostgreSQL/MySQL; SQLite's writer
    /// transaction and the conditional session mutation remain the authority on SQLite.
    async fn decide<C: ConnectionTrait>(
        &self,
        db: &C,
        request: &RefreshRotationRequest,
    ) -> Result<RefreshDecision> {
        let user_id = subject_id(&request.subject)?;
        // Missing subjects are rejected, not reported as a successful security revocation.
        let user = match user_repo::lock_by_id(db, user_id).await {
            Ok(user) => user,
            Err(AsterError::RecordNotFound(_)) => {
                return Ok(RefreshDecision::Reject(RefreshRejection::Unavailable));
            }
            Err(error) => return Err(error),
        };
        // Coordinator admission precedes the database lock; refresh the decision clock after the
        // lock wait, without extending the presented credential's verified expiry or skew.
        let request = RefreshRotationRequest {
            now: request.now.max(Utc::now().timestamp()),
            ..request.clone()
        };
        if !user.status.is_active() {
            return Ok(RefreshDecision::Reject(RefreshRejection::AccountDisabled));
        }
        if u64::try_from(user.session_version).ok() != Some(request.session_version.value()) {
            return Ok(RefreshDecision::Reject(RefreshRejection::Unavailable));
        }
        if crate::services::auth::local::password_change_required_for_login(&self.state, &user)
            || request.restriction != Restriction::None
        {
            return Ok(RefreshDecision::Reject(RefreshRejection::Restricted));
        }
        let session =
            auth_session_repo::find_by_id_for_user(db, user_id, request.session_id.as_str())
                .await?;
        let snapshot = session.as_ref().map(session_snapshot).transpose()?;
        Ok(classify_refresh(&request, snapshot.as_ref()))
    }

    /// Applies confirmed reuse atomically with its required security audit. Returning an `Ok`
    /// rejection allows the caller's transaction to commit instead of rolling back on HTTP 401.
    async fn reject<C: ConnectionTrait>(
        &self,
        db: &C,
        request: &RefreshRotationRequest,
        reason: RefreshRejection,
    ) -> Result<RefreshRejection> {
        if reason == RefreshRejection::ReuseDetected {
            let user_id = subject_id(&request.subject)?;
            user_repo::bump_session_version(db, user_id).await?;
            auth_session_repo::delete_all_for_user(db, user_id).await?;
            // Required security records bypass the optional buffered audit manager. No token or
            // refresh JTI enters details; actor and stable session identity are sufficient.
            let details = serde_json::json!({
                "session_id": request.session_id.as_str(),
                "reason": "refresh_reuse",
            });
            aster_forge_db::create_audit_log_row(
                db,
                aster_forge_db::AuditLogCreate {
                    user_id,
                    action: AuditAction::UserRefreshTokenReuseDetected
                        .as_str()
                        .to_string(),
                    entity_type: AuditEntityType::User.as_str().to_string(),
                    entity_id: Some(user_id),
                    entity_name: None,
                    details: Some(details.to_string()),
                    ip_address: request.client.ip_address.clone(),
                    user_agent: request.client.user_agent.clone(),
                    created_at: timestamp(request.now)?,
                },
            )
            .await?;
        }
        Ok(reason)
    }
}

#[async_trait::async_trait]
impl RefreshSessionStore for DriveRefreshStore {
    type Error = AsterError;

    async fn rotate_refresh(
        &self,
        request: &RefreshRotationRequest,
    ) -> Result<RefreshStoreOutcome> {
        aster_forge_db::transaction::with_transaction::<_, _, _, AsterError>(
            self.state.writer_db(),
            async |txn| match self.decide(txn, request).await? {
                RefreshDecision::Reject(reason) => Ok(RefreshStoreOutcome::Rejected(
                    self.reject(txn, request, reason).await?,
                )),
                RefreshDecision::Rotate => {
                    let rotated = auth_session_repo::rotate_refresh_fenced(
                        txn,
                        auth_session_repo::RefreshRotation {
                            user_id: subject_id(&request.subject)?,
                            session_id: request.session_id.as_str(),
                            current_refresh_jti: request.current_refresh_jti.as_str(),
                            next_refresh_jti: request.next_refresh_jti.as_str(),
                            refresh_expires_at: timestamp(request.next_refresh_expires_at)?,
                            ip_address: request.client.ip_address.as_deref(),
                            user_agent: request.client.user_agent.as_deref(),
                            last_seen_at: timestamp(request.now)?,
                        },
                    )
                    .await?;
                    Ok(if rotated {
                        RefreshStoreOutcome::Rotated
                    } else {
                        RefreshStoreOutcome::Conflict
                    })
                }
            },
        )
        .await
    }

    async fn resolve_conflict(&self, request: &RefreshRotationRequest) -> Result<RefreshRejection> {
        aster_forge_db::transaction::with_transaction::<_, _, _, AsterError>(
            self.state.writer_db(),
            async |txn| {
                let reason = match self.decide(txn, request).await? {
                    RefreshDecision::Reject(reason) => reason,
                    // CAS conflict followed by unchanged current state is unresolved; never rotate the
                    // losing candidate again or misreport a confirmed replay.
                    RefreshDecision::Rotate => {
                        return Err(AsterError::database_operation(
                            "unresolved refresh rotation conflict",
                        ));
                    }
                };
                self.reject(txn, request, reason).await
            },
        )
        .await
    }
}

/// Maps Drive entity fields into the product-neutral transaction snapshot; persisted invalid IDs
/// are infrastructure corruption, not client-authentication errors.
fn session_snapshot(session: &auth_session::Model) -> Result<RefreshSession> {
    let invalid = |_| AsterError::internal_error("invalid persisted authentication session");
    Ok(RefreshSession {
        subject: Subject::new(session.user_id.to_string()).map_err(invalid)?,
        session_id: SessionId::new(session.id.clone()).map_err(invalid)?,
        current_refresh_jti: RefreshJti::new(session.current_refresh_jti.clone())
            .map_err(invalid)?,
        previous_refresh_jti: session
            .previous_refresh_jti
            .clone()
            .map(RefreshJti::new)
            .transpose()
            .map_err(invalid)?,
        last_seen_at: session.last_seen_at.timestamp(),
        refresh_expires_at: session.refresh_expires_at.timestamp(),
        revoked: session.revoked_at.is_some(),
        client: ClientContext {
            ip_address: session.ip_address.clone(),
            user_agent: session.user_agent.clone(),
        },
    })
}

fn subject_id(subject: &Subject) -> Result<i64> {
    subject
        .as_str()
        .parse()
        .map_err(|_| AsterError::auth_token_invalid("invalid token subject"))
}

fn timestamp(seconds: i64) -> Result<chrono::DateTime<Utc>> {
    chrono::DateTime::from_timestamp(seconds, 0)
        .ok_or_else(|| AsterError::internal_error("invalid authentication timestamp"))
}

fn record_refresh_metric(state: &impl SharedRuntimeState, result: &Result<(String, String)>) {
    let (status, reason) = match result {
        Ok(_) => ("success", "ok"),
        Err(AsterError::AuthTokenExpired(_)) => ("failure", "expired"),
        Err(AsterError::AuthRefreshTokenStale(_)) => ("failure", "stale"),
        Err(AsterError::AuthRefreshTokenReuseDetected(_)) => ("failure", "reuse_detected"),
        Err(AsterError::AuthTokenInvalid(_)) => ("failure", "invalid"),
        Err(AsterError::AuthForbidden(_)) => ("failure", "forbidden"),
        Err(AsterError::RateLimited(_)) => ("failure", "rate_limited"),
        Err(_) => ("failure", "error"),
    };
    state.metrics().record_auth_event("refresh", status, reason);
}
