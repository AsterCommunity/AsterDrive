//! Refresh token rotation and reuse handling.
//!
//! The security rule here is deliberately strict: a refresh token is single-use.
//! Once it has been rotated, seeing the old JTI again is treated as a possible
//! token theft unless we have concrete evidence that it is a harmless stale
//! retry. The two evidence sources we currently accept are:
//!
//! 1. The request waited on this process's per-JTI rotation lock, so it is a
//!    true same-process loser of an in-flight refresh race.
//! 2. The old JTI matches `previous_refresh_jti`, the rotation is recent, and
//!    the stored client fingerprint matches the current request.
//!
//! Do not widen the "recent" window into a generic grace period. A sequential
//! replay with no client evidence must still revoke all sessions by bumping
//! `session_version`; otherwise a stolen refresh token can probe silently.
//!
//! End-to-end flow:
//!
//! 1. Decode and validate the incoming refresh JWT, including token type and
//!    JTI presence.
//! 2. Acquire the process-local mutex keyed by that JTI. This serializes
//!    same-process requests before any database mutation and records whether
//!    the request actually waited behind another refresh.
//! 3. In a transaction, load the auth session and user, verify the token's
//!    `session_version`, issue the next token pair, then conditionally update
//!    `auth_sessions.current_refresh_jti = incoming_jti`.
//! 4. If the current JTI is already gone, classify the incoming token as stale
//!    or reuse by looking at `previous_refresh_jti` plus the evidence listed
//!    above.
//! 5. A stale retry returns E012 only. A confirmed reuse bumps
//!    `users.session_version`, purges auth sessions, invalidates cached auth
//!    snapshots, and records an audit log.

use chrono::Utc;
use dashmap::{DashMap, mapref::entry::Entry};
use sea_orm::ConnectionTrait;
use serde::Serialize;
#[cfg(debug_assertions)]
use std::sync::Mutex as StdMutex;
use std::sync::{Arc, LazyLock, Weak};
use tokio::sync::{Mutex, OwnedMutexGuard};

use crate::api::subcode::ApiSubcode;
use crate::db::repository::{auth_session_repo, user_repo};
use crate::entities::auth_session;
use crate::errors::{
    AsterError, Result, auth_forbidden_with_subcode,
};
use crate::runtime::PrimaryAppState;
use crate::services::audit_service::{self, AuditContext};
use crate::types::TokenType;

use super::super::Claims;
use super::super::session::{
    invalidate_auth_snapshot_cache, purge_all_auth_sessions_in_connection,
};
use super::{ensure_token_type, issue_tokens_for_session_id, verify_token};

// This window is only for classifying an already-rotated previous JTI. It does
// not allow a second successful refresh; stale callers still receive E012.
const REFRESH_REUSE_GRACE_SECS: i64 = 15;

// Process-local serialization for a single refresh JTI.
//
// The database conditional update remains the source of truth. This lock only
// lets us distinguish a real same-process race loser from a later replay with
// no client evidence. Values are Weak so completed refreshes do not leak one
// mutex per historical JTI.
static REFRESH_ROTATION_LOCKS: LazyLock<DashMap<String, Weak<Mutex<()>>>> =
    LazyLock::new(DashMap::new);

#[cfg(debug_assertions)]
static REFRESH_ROTATION_TEST_HOOK: LazyLock<
    StdMutex<Option<test_support::RefreshRotationTestHook>>,
> = LazyLock::new(|| StdMutex::new(None));

struct RefreshRotationLockGuard {
    refresh_jti: String,
    lock: Option<Arc<Mutex<()>>>,
    guard: Option<OwnedMutexGuard<()>>,
}

struct RefreshRotationLock {
    _guard: RefreshRotationLockGuard,
    was_contended: bool,
}

impl Drop for RefreshRotationLockGuard {
    fn drop(&mut self) {
        // Drop the mutex guard before dropping our strong Arc, then remove the
        // DashMap entry only when the Weak can no longer be upgraded. A live
        // Weak entry is not treated as security evidence elsewhere because it
        // may simply be waiting for this cleanup path to run.
        drop(self.guard.take());
        drop(self.lock.take());
        REFRESH_ROTATION_LOCKS.remove_if(&self.refresh_jti, |_, lock| lock.upgrade().is_none());
    }
}

#[derive(Debug)]
enum RefreshRotationOutcome {
    // The normal winner path: the auth_session row was atomically moved from
    // incoming JTI to next JTI, and the newly issued tokens can be returned.
    Rotated {
        access_token: String,
        refresh_token: String,
        session_version: i64,
    },
    // The incoming JTI was already recorded as previous_refresh_jti. The
    // request is rejected either as stale or as reuse; handling is deferred so
    // side effects such as audit logging happen outside the mutation helper.
    Rejected(RefreshRejection),
    // The row existed when read, but the conditional update matched zero rows.
    // That means another actor rotated or revoked the row between read and
    // update. We intentionally reclassify after this transaction ends.
    RotateConflict {
        ip_address: Option<String>,
        user_agent: Option<String>,
    },
}

#[derive(Debug)]
enum RefreshRejection {
    // A stale caller never receives new tokens and never changes session state.
    // This is expected for cross-tab races and same-client retry edges.
    StaleRefresh { user_id: i64, reused_jti: String },
    // A reuse caller is treated as possible token theft. This path revokes the
    // whole login family by bumping session_version and deleting sessions.
    ReuseDetected { user_id: i64, reused_jti: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RefreshReuseEvidence {
    // Evidence comes from stored request metadata (user agent/IP) only.
    ClientFingerprint,
    // The request actually waited behind another refresh in this process.
    SameProcessContention,
}

struct RefreshReuseClassification<'a> {
    // Session found by previous_refresh_jti. None means the incoming JTI is not
    // tied to any current session and should remain invalid.
    reused_auth_session: Option<&'a auth_session::Model>,
    user_id: i64,
    refresh_jti: &'a str,
    now: chrono::DateTime<Utc>,
    ip_address: Option<&'a str>,
    user_agent: Option<&'a str>,
    evidence: RefreshReuseEvidence,
}

#[derive(Serialize)]
struct RefreshTokenReuseAuditDetails<'a> {
    reused_jti: &'a str,
}

fn is_recent_refresh_rotation(session: &auth_session::Model, now: chrono::DateTime<Utc>) -> bool {
    now.signed_duration_since(session.last_seen_at)
        .num_seconds()
        .abs()
        <= REFRESH_REUSE_GRACE_SECS
}

fn refresh_client_matches(
    session: &auth_session::Model,
    ip_address: Option<&str>,
    user_agent: Option<&str>,
) -> bool {
    // User agent is the required fingerprint because it is stable across the
    // normal browser retry path. IP is a refinement: trusted-proxy handling can
    // legitimately leave it absent, so absence on either side does not turn a
    // matching UA into a compromise signal.
    let Some(stored_user_agent) = session.user_agent.as_deref() else {
        return false;
    };
    let Some(current_user_agent) = user_agent else {
        return false;
    };
    if stored_user_agent != current_user_agent {
        return false;
    }

    session.ip_address.is_none()
        || ip_address.is_none()
        || session.ip_address.as_deref() == ip_address
}

fn is_stale_refresh_from_same_client(
    session: &auth_session::Model,
    user_id: i64,
    now: chrono::DateTime<Utc>,
    ip_address: Option<&str>,
    user_agent: Option<&str>,
) -> bool {
    // Same-client stale requires all three facts: same user, live session, and
    // a recent rotation. The caller still gets E012; this only decides whether
    // to avoid global session revocation.
    session.user_id == user_id
        && session.revoked_at.is_none()
        && is_recent_refresh_rotation(session, now)
        && refresh_client_matches(session, ip_address, user_agent)
}

fn classify_refresh_reuse_session(input: RefreshReuseClassification<'_>) -> RefreshRejection {
    let RefreshReuseClassification {
        reused_auth_session,
        user_id,
        refresh_jti,
        now,
        ip_address,
        user_agent,
        evidence,
    } = input;

    // A same-process lock loser still needs the same-client fingerprint before
    // it can be treated as stale. The lock proves overlap on this server
    // process, but it does not prove the old JTI came from the same client.
    if evidence == RefreshReuseEvidence::SameProcessContention
        && reused_auth_session.is_some_and(|session| {
            session.user_id == user_id
                && session.revoked_at.is_none()
                && is_recent_refresh_rotation(session, now)
                && is_stale_refresh_from_same_client(session, user_id, now, ip_address, user_agent)
        })
    {
        return RefreshRejection::StaleRefresh {
            user_id,
            reused_jti: refresh_jti.to_string(),
        };
    }

    // Without same-process contention, require a same-client fingerprint. This
    // keeps sequential no-evidence replay in the compromise path.
    if !matches!(evidence, RefreshReuseEvidence::SameProcessContention)
        && reused_auth_session.is_some_and(|session| {
            is_stale_refresh_from_same_client(session, user_id, now, ip_address, user_agent)
        })
    {
        return RefreshRejection::StaleRefresh {
            user_id,
            reused_jti: refresh_jti.to_string(),
        };
    }

    RefreshRejection::ReuseDetected {
        user_id,
        reused_jti: refresh_jti.to_string(),
    }
}

fn refresh_rotation_lock(refresh_jti: &str) -> Arc<Mutex<()>> {
    // This map is best-effort process-local coordination, not distributed
    // locking. If a Weak entry has expired we replace it; if it upgrades we use
    // the same mutex so only one task in this process can rotate a given JTI at
    // a time.
    match REFRESH_ROTATION_LOCKS.entry(refresh_jti.to_string()) {
        Entry::Occupied(mut entry) => {
            if let Some(lock) = entry.get().upgrade() {
                lock
            } else {
                let lock = Arc::new(Mutex::new(()));
                entry.insert(Arc::downgrade(&lock));
                lock
            }
        }
        Entry::Vacant(entry) => {
            let lock = Arc::new(Mutex::new(()));
            entry.insert(Arc::downgrade(&lock));
            lock
        }
    }
}

async fn acquire_refresh_rotation_lock(refresh_jti: &str) -> RefreshRotationLock {
    let lock = refresh_rotation_lock(refresh_jti);
    match lock.clone().try_lock_owned() {
        Ok(guard) => {
            // A direct acquisition means this task did not wait behind an
            // already-running same-JTI refresh in this process. Even if a Weak
            // entry existed, we classify future old-token observations using
            // client fingerprint evidence only.
            #[cfg(debug_assertions)]
            maybe_pause_refresh_rotation_after_lock_acquired(refresh_jti).await;
            RefreshRotationLock {
                _guard: RefreshRotationLockGuard {
                    refresh_jti: refresh_jti.to_string(),
                    lock: Some(lock),
                    guard: Some(guard),
                },
                was_contended: false,
            }
        }
        Err(_) => {
            // Only this branch is "same-process contention". Seeing an old
            // Weak entry after the mutex has become available is not enough:
            // that could be a later sequential replay racing with cleanup.
            #[cfg(debug_assertions)]
            maybe_notify_refresh_rotation_lock_contended(refresh_jti).await;
            let guard = lock.clone().lock_owned().await;
            RefreshRotationLock {
                _guard: RefreshRotationLockGuard {
                    refresh_jti: refresh_jti.to_string(),
                    lock: Some(lock),
                    guard: Some(guard),
                },
                was_contended: true,
            }
        }
    }
}

#[cfg(debug_assertions)]
async fn maybe_pause_refresh_rotation_after_lock_acquired(refresh_jti: &str) {
    let hook = REFRESH_ROTATION_TEST_HOOK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    let Some(hook) = hook else {
        return;
    };
    if hook.refresh_jti != refresh_jti {
        return;
    }
    hook.lock_acquired.notify_one();
    hook.release_lock.notified().await;
}

#[cfg(debug_assertions)]
async fn maybe_notify_refresh_rotation_lock_contended(refresh_jti: &str) {
    let hook = REFRESH_ROTATION_TEST_HOOK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    let Some(hook) = hook else {
        return;
    };
    if hook.refresh_jti != refresh_jti {
        return;
    }
    hook.lock_contended.notify_one();
}

async fn classify_failed_refresh_rotation<C: ConnectionTrait>(
    db: &C,
    claims: &Claims,
    refresh_jti: &str,
    now: chrono::DateTime<Utc>,
    ip_address: Option<&str>,
    user_agent: Option<&str>,
) -> Result<RefreshRejection> {
    // This path is for DB-level conflicts, usually another process/connection
    // rotating the same JTI between our read and conditional update. There is
    // no process-local lock evidence here, so classification must fall back to
    // client fingerprint rules.
    let reused_auth_session =
        auth_session_repo::find_by_previous_refresh_jti(db, refresh_jti).await?;
    Ok(classify_refresh_reuse_session(RefreshReuseClassification {
        reused_auth_session: reused_auth_session.as_ref(),
        user_id: claims.user_id,
        refresh_jti,
        now,
        ip_address,
        user_agent,
        evidence: RefreshReuseEvidence::ClientFingerprint,
    }))
}

async fn revoke_sessions_in_connection<C: ConnectionTrait>(db: &C, user_id: i64) -> Result<()> {
    // Keep the version bump and auth_session purge in the caller's transaction
    // so a reuse detection cannot leave half-revoked state behind.
    user_repo::bump_session_version(db, user_id).await?;
    purge_all_auth_sessions_in_connection(db, user_id).await
}

async fn classify_refresh_reuse_in_transaction<C: ConnectionTrait>(
    db: &C,
    input: RefreshReuseClassification<'_>,
) -> Result<RefreshRejection> {
    // Classification is pure except for confirmed reuse. The side effect is
    // intentionally here, inside the same transaction that observed the reused
    // JTI, so concurrent access tokens are invalidated atomically with session
    // cleanup.
    let rejection = classify_refresh_reuse_session(input);
    if let RefreshRejection::ReuseDetected { user_id, .. } = &rejection {
        revoke_sessions_in_connection(db, *user_id).await?;
    }
    Ok(rejection)
}

async fn rotate_refresh_in_transaction<C: ConnectionTrait>(
    db: &C,
    state: &PrimaryAppState,
    claims: &Claims,
    refresh_jti: &str,
    ip_address: Option<&str>,
    user_agent: Option<&str>,
    reuse_evidence: RefreshReuseEvidence,
) -> Result<RefreshRotationOutcome> {
    // This helper runs under the per-JTI lock and a DB transaction. It returns
    // an outcome instead of writing cookies/audit logs directly so transaction
    // boundaries stay explicit: DB mutations happen here, external side effects
    // happen in finish_* after commit.
    let now = Utc::now();

    // First read both the current and previous-JTI views of the session. These
    // two lookups partition the important states:
    //
    // - current exists: this request may be the rotation winner.
    // - current missing, previous exists: this is an old-token observation.
    // - neither exists: the token is invalid or already purged.
    let existing_auth_session = auth_session_repo::find_by_refresh_jti(db, refresh_jti).await?;
    let reused_auth_session = if existing_auth_session.is_none() {
        auth_session_repo::find_by_previous_refresh_jti(db, refresh_jti).await?
    } else {
        None
    };
    let user = user_repo::find_by_id(db, claims.user_id).await?;
    // Validate account and token snapshot before issuing new tokens. A stale
    // session_version means a previous password/session/security event already
    // invalidated this refresh token, even if the auth_session row still exists.
    if !user.status.is_active() {
        return Err(auth_forbidden_with_subcode(
            ApiSubcode::AuthAccountDisabled,
            "account is disabled",
        ));
    }
    if claims.session_version != user.session_version {
        return Err(AsterError::auth_token_invalid("session revoked"));
    }

    let Some(existing_auth_session) = existing_auth_session else {
        // The current JTI is gone. If it is recorded as the previous JTI of a
        // live session, the request is using a rotated token; classify it as a
        // harmless stale retry only when the evidence rules above allow that.
        if reused_auth_session.as_ref().is_some_and(|session| {
            session.user_id == claims.user_id && session.revoked_at.is_none()
        }) {
            let rejection = classify_refresh_reuse_in_transaction(
                db,
                RefreshReuseClassification {
                    reused_auth_session: reused_auth_session.as_ref(),
                    user_id: claims.user_id,
                    refresh_jti,
                    now,
                    ip_address,
                    user_agent,
                    evidence: reuse_evidence,
                },
            )
            .await?;
            return Ok(RefreshRotationOutcome::Rejected(rejection));
        }
        return Err(AsterError::auth_token_invalid("session revoked"));
    };

    if existing_auth_session.user_id != claims.user_id {
        return Err(AsterError::auth_token_invalid("invalid token"));
    }
    if existing_auth_session.revoked_at.is_some() {
        return Err(AsterError::auth_token_invalid("session revoked"));
    }

    let next_ip_address = ip_address.or(existing_auth_session.ip_address.as_deref());
    let next_user_agent = user_agent.or(existing_auth_session.user_agent.as_deref());
    // The refresh JTI is generated before the conditional update, but it is not
    // usable unless the update below wins. Losers never return these tokens.
    let tokens = issue_tokens_for_session_id(
        state,
        user.id,
        user.session_version,
        Some(existing_auth_session.id.as_str()),
    )?;

    if !auth_session_repo::rotate_refresh(
        db,
        refresh_jti,
        &tokens.refresh_jti,
        tokens.refresh_expires_at,
        next_ip_address,
        next_user_agent,
        now,
    )
    .await?
    {
        // We held the process-local lock, but the database row no longer
        // matches. That means a cross-process rotation or an unexpected DB race
        // won first. Re-read `previous_refresh_jti` in a new transaction after
        // this one ends, then classify with the stricter fingerprint evidence.
        return Ok(RefreshRotationOutcome::RotateConflict {
            ip_address: next_ip_address.map(str::to_string),
            user_agent: next_user_agent.map(str::to_string),
        });
    }

    Ok(RefreshRotationOutcome::Rotated {
        access_token: tokens.access_token,
        refresh_token: tokens.refresh_token,
        session_version: user.session_version,
    })
}

async fn record_refresh_reuse_detection(
    state: &PrimaryAppState,
    user_id: i64,
    reused_jti: &str,
    log_message: &'static str,
) -> Result<()> {
    // `revoke_sessions_in_connection` mutates the DB. This function handles the
    // non-transactional follow-up after commit: evict cached auth snapshots and
    // leave an audit trail. Do not call this for stale retries; those are noisy
    // normal browser races, not confirmed compromise signals.
    invalidate_auth_snapshot_cache(state, user_id).await;
    tracing::warn!(user_id, reused_jti, "{log_message}");
    audit_service::log(
        state,
        &AuditContext {
            user_id,
            ip_address: None,
            user_agent: None,
        },
        audit_service::AuditAction::UserRefreshTokenReuseDetected,
        crate::services::audit_service::AuditEntityType::User,
        Some(user_id),
        None,
        audit_service::details(RefreshTokenReuseAuditDetails { reused_jti }),
    )
    .await;
    Ok(())
}

async fn finish_refresh_rejection(
    state: &PrimaryAppState,
    rejection: RefreshRejection,
    reuse_log_message: &'static str,
) -> Result<(String, String)> {
    // Both stale and reuse map to E012 at the API boundary, but only reuse has
    // already changed session state and receives security audit logging.
    match rejection {
        RefreshRejection::StaleRefresh {
            user_id,
            reused_jti,
        } => {
            tracing::debug!(
                user_id,
                reused_jti,
                "stale refresh token reused within rotation grace window"
            );
            Err(AsterError::auth_refresh_token_stale("stale refresh token"))
        }
        RefreshRejection::ReuseDetected {
            user_id,
            reused_jti,
        } => {
            record_refresh_reuse_detection(state, user_id, &reused_jti, reuse_log_message).await?;
            Err(AsterError::auth_refresh_token_reuse_detected(
                "refresh token reuse detected",
            ))
        }
    }
}

async fn finish_refresh_outcome(
    state: &PrimaryAppState,
    claims: &Claims,
    refresh_jti: &str,
    outcome: RefreshRotationOutcome,
) -> Result<(String, String)> {
    // The transaction that produced `outcome` has committed by the time this
    // runs. That matters for RotateConflict: reclassification must observe the
    // winner's committed previous_refresh_jti, not the pre-commit view that
    // caused earlier race bugs.
    match outcome {
        RefreshRotationOutcome::Rotated {
            access_token,
            refresh_token,
            session_version,
        } => {
            tracing::debug!(
                user_id = claims.user_id,
                session_version,
                "refreshed auth tokens"
            );
            Ok((access_token, refresh_token))
        }
        RefreshRotationOutcome::Rejected(rejection) => {
            finish_refresh_rejection(
                state,
                rejection,
                "refresh token reuse detected; revoked all sessions",
            )
            .await
        }
        RefreshRotationOutcome::RotateConflict {
            ip_address,
            user_agent,
        } => {
            let conflict_outcome =
                crate::db::transaction::with_transaction(state.writer_db(), async |txn| {
                    let outcome = classify_failed_refresh_rotation(
                        txn,
                        claims,
                        refresh_jti,
                        Utc::now(),
                        ip_address.as_deref(),
                        user_agent.as_deref(),
                    )
                    .await?;
                    if let RefreshRejection::ReuseDetected { user_id, .. } = &outcome {
                        revoke_sessions_in_connection(txn, *user_id).await?;
                    }
                    Ok(outcome)
                })
                .await?;
            finish_refresh_rejection(
                state,
                conflict_outcome,
                "refresh token reuse detected after refresh rotation conflict; revoked all sessions",
            )
            .await
        }
    }
}

pub async fn refresh_tokens(
    state: &PrimaryAppState,
    refresh: &str,
    ip_address: Option<&str>,
    user_agent: Option<&str>,
) -> Result<(String, String)> {
    let result = async {
        // Public service entrypoint used by both HTTP handlers and tests. Keep the
        // order intact: verify JWT cheaply first, acquire the per-JTI lock second,
        // then do all authoritative state checks in the transaction.
        tracing::debug!("refreshing auth tokens");
        let claims = verify_token(refresh, &state.config.auth.jwt_secret)?;
        ensure_token_type(&claims, TokenType::Refresh)?;
        let refresh_jti = claims
            .jti
            .clone()
            .ok_or_else(|| AsterError::auth_token_invalid("refresh token missing jti"))?;

        let rotation_lock = acquire_refresh_rotation_lock(&refresh_jti).await;
        // This boolean is the only bridge between the lock layer and reuse
        // classification. Do not infer SameProcessContention from timing, map
        // entries, or recent timestamps alone.
        let reuse_evidence = if rotation_lock.was_contended {
            RefreshReuseEvidence::SameProcessContention
        } else {
            RefreshReuseEvidence::ClientFingerprint
        };
        let outcome = crate::db::transaction::with_transaction(state.writer_db(), async |txn| {
            rotate_refresh_in_transaction(
                txn,
                state,
                &claims,
                &refresh_jti,
                ip_address,
                user_agent,
                reuse_evidence,
            )
            .await
        })
        .await?;

        finish_refresh_outcome(state, &claims, &refresh_jti, outcome).await
    }
    .await;

    record_refresh_metric(state, &result);
    result
}

fn record_refresh_metric(state: &PrimaryAppState, result: &Result<(String, String)>) {
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
    state.metrics.record_auth_event("refresh", status, reason);
}

#[cfg(debug_assertions)]
pub mod test_support {
    //! Debug-only hooks for integration tests.
    //!
    //! `cfg(test)` is not visible to integration tests that depend on the lib
    //! crate, so this module is guarded by `debug_assertions` instead. Release
    //! builds do not compile it. Tests use this hook to prove that a request
    //! actually waits on the server-side rotation lock; a plain async barrier is
    //! not enough because the first refresh may complete before the second
    //! reaches the lock, making the request indistinguishable from a sequential
    //! no-evidence replay.

    use std::sync::Arc;

    use tokio::sync::Notify;

    use crate::errors::{AsterError, Result};
    use crate::types::TokenType;

    use super::{REFRESH_ROTATION_TEST_HOOK, ensure_token_type, verify_token};

    #[derive(Clone)]
    pub struct RefreshRotationTestHook {
        pub(super) refresh_jti: String,
        pub(super) lock_acquired: Arc<Notify>,
        pub(super) lock_contended: Arc<Notify>,
        pub(super) release_lock: Arc<Notify>,
    }

    impl RefreshRotationTestHook {
        /// Wait until the first request holds the per-JTI lock and is paused.
        pub async fn wait_until_lock_acquired(&self) {
            self.lock_acquired.notified().await;
        }

        /// Wait until another request has failed `try_lock_owned` and is queued.
        pub async fn wait_until_lock_contended(&self) {
            self.lock_contended.notified().await;
        }

        /// Let the paused lock holder continue through rotation.
        pub fn release_lock(&self) {
            self.release_lock.notify_one();
        }
    }

    impl Drop for RefreshRotationTestHook {
        fn drop(&mut self) {
            self.release_lock.notify_waiters();
        }
    }

    pub async fn install_refresh_rotation_test_hook(
        refresh_token: &str,
        jwt_secret: &str,
    ) -> Result<RefreshRotationTestHook> {
        let claims = verify_token(refresh_token, jwt_secret)?;
        ensure_token_type(&claims, TokenType::Refresh)?;
        let refresh_jti = claims
            .jti
            .ok_or_else(|| AsterError::auth_token_invalid("refresh token missing jti"))?;
        let hook = RefreshRotationTestHook {
            refresh_jti,
            lock_acquired: Arc::new(Notify::new()),
            lock_contended: Arc::new(Notify::new()),
            release_lock: Arc::new(Notify::new()),
        };
        *REFRESH_ROTATION_TEST_HOOK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(hook.clone());
        Ok(hook)
    }

    pub async fn clear_refresh_rotation_test_hook() {
        *REFRESH_ROTATION_TEST_HOOK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration as ChronoDuration;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    use tokio::sync::Notify;
    use tokio::time::{Duration, timeout};

    fn make_auth_session(now: chrono::DateTime<Utc>) -> auth_session::Model {
        auth_session::Model {
            id: "session-id".to_string(),
            user_id: 1,
            current_refresh_jti: "next-jti".to_string(),
            previous_refresh_jti: Some("refresh-jti".to_string()),
            refresh_expires_at: now + ChronoDuration::days(1),
            ip_address: Some("203.0.113.10".to_string()),
            user_agent: Some("browser".to_string()),
            created_at: now,
            last_seen_at: now,
            revoked_at: None,
        }
    }

    fn assert_stale_refresh(rejection: RefreshRejection, user_id: i64, reused_jti: &str) {
        match rejection {
            RefreshRejection::StaleRefresh {
                user_id: actual_user_id,
                reused_jti: actual_reused_jti,
            } => {
                assert_eq!(actual_user_id, user_id);
                assert_eq!(actual_reused_jti, reused_jti);
            }
            other => panic!("expected stale refresh, got {other:?}"),
        }
    }

    fn assert_reuse_detected(rejection: RefreshRejection, user_id: i64, reused_jti: &str) {
        match rejection {
            RefreshRejection::ReuseDetected {
                user_id: actual_user_id,
                reused_jti: actual_reused_jti,
            } => {
                assert_eq!(actual_user_id, user_id);
                assert_eq!(actual_reused_jti, reused_jti);
            }
            other => panic!("expected refresh reuse, got {other:?}"),
        }
    }

    fn classify_refresh_reuse_for_test(
        session: &auth_session::Model,
        now: chrono::DateTime<Utc>,
        ip_address: Option<&str>,
        user_agent: Option<&str>,
        evidence: RefreshReuseEvidence,
    ) -> RefreshRejection {
        classify_refresh_reuse_session(RefreshReuseClassification {
            reused_auth_session: Some(session),
            user_id: 1,
            refresh_jti: "refresh-jti",
            now,
            ip_address,
            user_agent,
            evidence,
        })
    }

    #[test]
    fn classify_refresh_reuse_session_treats_same_client_grace_as_stale() {
        let now = Utc::now();
        let session = make_auth_session(now);

        let outcome = classify_refresh_reuse_for_test(
            &session,
            now,
            Some("203.0.113.10"),
            Some("browser"),
            RefreshReuseEvidence::ClientFingerprint,
        );

        assert_stale_refresh(outcome, 1, "refresh-jti");
    }

    #[test]
    fn classify_refresh_reuse_session_treats_different_client_as_reuse() {
        let now = Utc::now();
        let session = make_auth_session(now);

        let outcome = classify_refresh_reuse_for_test(
            &session,
            now,
            Some("203.0.113.11"),
            Some("other-browser"),
            RefreshReuseEvidence::ClientFingerprint,
        );

        assert_reuse_detected(outcome, 1, "refresh-jti");
    }

    #[test]
    fn classify_refresh_reuse_session_treats_exact_grace_boundary_as_stale() {
        let now = Utc::now();
        let mut session = make_auth_session(now);
        session.last_seen_at = now - ChronoDuration::seconds(REFRESH_REUSE_GRACE_SECS);

        let outcome = classify_refresh_reuse_for_test(
            &session,
            now,
            Some("203.0.113.10"),
            Some("browser"),
            RefreshReuseEvidence::ClientFingerprint,
        );

        assert_stale_refresh(outcome, 1, "refresh-jti");
    }

    #[test]
    fn classify_refresh_reuse_session_treats_just_outside_grace_as_reuse() {
        let now = Utc::now();
        let mut session = make_auth_session(now);
        session.last_seen_at = now - ChronoDuration::seconds(REFRESH_REUSE_GRACE_SECS + 1);

        let outcome = classify_refresh_reuse_for_test(
            &session,
            now,
            Some("203.0.113.10"),
            Some("browser"),
            RefreshReuseEvidence::ClientFingerprint,
        );

        assert_reuse_detected(outcome, 1, "refresh-jti");
    }

    #[test]
    fn classify_refresh_reuse_session_treats_missing_client_evidence_as_reuse() {
        let now = Utc::now();
        let session = make_auth_session(now);

        let outcome = classify_refresh_reuse_for_test(
            &session,
            now,
            None,
            None,
            RefreshReuseEvidence::ClientFingerprint,
        );

        assert_reuse_detected(outcome, 1, "refresh-jti");
    }

    #[test]
    fn same_process_contention_without_client_evidence_is_reuse() {
        let now = Utc::now();
        let session = make_auth_session(now);

        let outcome = classify_refresh_reuse_for_test(
            &session,
            now,
            None,
            None,
            RefreshReuseEvidence::SameProcessContention,
        );

        assert_reuse_detected(outcome, 1, "refresh-jti");
    }

    #[test]
    fn same_process_contention_with_mismatched_client_is_reuse() {
        let now = Utc::now();
        let session = make_auth_session(now);

        let outcome = classify_refresh_reuse_for_test(
            &session,
            now,
            Some("203.0.113.11"),
            Some("other-browser"),
            RefreshReuseEvidence::SameProcessContention,
        );

        assert_reuse_detected(outcome, 1, "refresh-jti");
    }

    #[test]
    fn same_process_contention_from_same_client_is_stale() {
        let now = Utc::now();
        let session = make_auth_session(now);

        let outcome = classify_refresh_reuse_for_test(
            &session,
            now,
            Some("203.0.113.10"),
            Some("browser"),
            RefreshReuseEvidence::SameProcessContention,
        );

        assert_stale_refresh(outcome, 1, "refresh-jti");
    }

    #[test]
    fn classify_refresh_reuse_session_treats_same_process_contention_for_other_user_as_reuse() {
        let now = Utc::now();
        let mut session = make_auth_session(now);
        session.user_id = 2;

        let outcome = classify_refresh_reuse_for_test(
            &session,
            now,
            None,
            None,
            RefreshReuseEvidence::SameProcessContention,
        );

        assert_reuse_detected(outcome, 1, "refresh-jti");
    }

    #[test]
    fn classify_refresh_reuse_session_treats_same_process_contention_for_revoked_session_as_reuse()
    {
        let now = Utc::now();
        let mut session = make_auth_session(now);
        session.revoked_at = Some(now);

        let outcome = classify_refresh_reuse_for_test(
            &session,
            now,
            None,
            None,
            RefreshReuseEvidence::SameProcessContention,
        );

        assert_reuse_detected(outcome, 1, "refresh-jti");
    }

    #[tokio::test]
    async fn acquire_refresh_rotation_lock_serializes_same_jti_and_cleans_up_after_release() {
        let refresh_jti = format!("refresh-jti-{}", uuid::Uuid::new_v4());
        let first = acquire_refresh_rotation_lock(&refresh_jti).await;

        assert!(REFRESH_ROTATION_LOCKS.get(&refresh_jti).is_some());

        let acquired = Arc::new(AtomicBool::new(false));
        let notify = Arc::new(Notify::new());
        let acquired_clone = Arc::clone(&acquired);
        let notify_clone = Arc::clone(&notify);
        let refresh_jti_clone = refresh_jti.clone();
        let second_task = tokio::spawn(async move {
            let second = acquire_refresh_rotation_lock(&refresh_jti_clone).await;
            acquired_clone.store(true, Ordering::SeqCst);
            notify_clone.notify_one();
            second
        });

        assert!(
            timeout(Duration::from_millis(200), notify.notified())
                .await
                .is_err(),
            "second acquisition should stay blocked while the first lock is held"
        );

        drop(first);

        timeout(Duration::from_secs(1), notify.notified())
            .await
            .expect("second acquisition should complete after the first lock is released");
        assert!(acquired.load(Ordering::SeqCst));

        let second = second_task.await.unwrap();
        drop(second);

        assert!(
            REFRESH_ROTATION_LOCKS.get(&refresh_jti).is_none(),
            "lock entry should be cleaned up after the last guard is dropped"
        );
    }

    #[test]
    fn classify_refresh_reuse_session_treats_same_process_contention_outside_grace_as_reuse() {
        let now = Utc::now();
        let mut session = make_auth_session(now);
        session.last_seen_at = now - ChronoDuration::seconds(REFRESH_REUSE_GRACE_SECS + 1);

        let outcome = classify_refresh_reuse_for_test(
            &session,
            now,
            None,
            None,
            RefreshReuseEvidence::SameProcessContention,
        );

        assert_reuse_detected(outcome, 1, "refresh-jti");
    }
}
