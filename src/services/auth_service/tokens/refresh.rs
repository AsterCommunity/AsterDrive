use chrono::Utc;
use dashmap::{DashMap, mapref::entry::Entry};
use sea_orm::ConnectionTrait;
use serde::Serialize;
use std::sync::{Arc, LazyLock, Weak};
use tokio::sync::{Mutex, OwnedMutexGuard};

use crate::api::subcode::ApiSubcode;
use crate::db::repository::{auth_session_repo, user_repo};
use crate::entities::auth_session;
use crate::errors::{AsterError, Result, auth_forbidden_with_subcode};
use crate::runtime::PrimaryAppState;
use crate::services::audit_service::{self, AuditContext};
use crate::types::TokenType;

use super::super::Claims;
use super::super::session::{
    invalidate_auth_snapshot_cache, purge_all_auth_sessions_in_connection,
};
use super::{ensure_token_type, issue_tokens_for_session_id, verify_token};

const REFRESH_REUSE_GRACE_SECS: i64 = 15;

static REFRESH_ROTATION_LOCKS: LazyLock<DashMap<String, Weak<Mutex<()>>>> =
    LazyLock::new(DashMap::new);

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
        drop(self.guard.take());
        drop(self.lock.take());
        REFRESH_ROTATION_LOCKS.remove_if(&self.refresh_jti, |_, lock| lock.upgrade().is_none());
    }
}

#[derive(Debug)]
enum RefreshRotationOutcome {
    Rotated {
        access_token: String,
        refresh_token: String,
        session_version: i64,
    },
    Rejected(RefreshRejection),
    RotateConflict {
        ip_address: Option<String>,
        user_agent: Option<String>,
    },
}

#[derive(Debug)]
enum RefreshRejection {
    StaleRefresh { user_id: i64, reused_jti: String },
    ReuseDetected { user_id: i64, reused_jti: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RefreshReuseEvidence {
    ClientFingerprint,
    SameProcessContention,
}

struct RefreshReuseClassification<'a> {
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

    if evidence == RefreshReuseEvidence::SameProcessContention
        && reused_auth_session.is_some_and(|session| {
            session.user_id == user_id
                && session.revoked_at.is_none()
                && is_recent_refresh_rotation(session, now)
        })
    {
        return RefreshRejection::StaleRefresh {
            user_id,
            reused_jti: refresh_jti.to_string(),
        };
    }

    if reused_auth_session.is_some_and(|session| {
        is_stale_refresh_from_same_client(session, user_id, now, ip_address, user_agent)
    }) {
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
        Ok(guard) => RefreshRotationLock {
            _guard: RefreshRotationLockGuard {
                refresh_jti: refresh_jti.to_string(),
                lock: Some(lock),
                guard: Some(guard),
            },
            was_contended: false,
        },
        Err(_) => {
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

async fn classify_failed_refresh_rotation<C: ConnectionTrait>(
    db: &C,
    claims: &Claims,
    refresh_jti: &str,
    now: chrono::DateTime<Utc>,
    ip_address: Option<&str>,
    user_agent: Option<&str>,
) -> Result<RefreshRejection> {
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
    user_repo::bump_session_version(db, user_id).await?;
    purge_all_auth_sessions_in_connection(db, user_id).await
}

async fn classify_refresh_reuse_in_transaction<C: ConnectionTrait>(
    db: &C,
    input: RefreshReuseClassification<'_>,
) -> Result<RefreshRejection> {
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
    let now = Utc::now();
    let existing_auth_session = auth_session_repo::find_by_refresh_jti(db, refresh_jti).await?;
    let reused_auth_session = if existing_auth_session.is_none() {
        auth_session_repo::find_by_previous_refresh_jti(db, refresh_jti).await?
    } else {
        None
    };
    let user = user_repo::find_by_id(db, claims.user_id).await?;
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
            Err(AsterError::auth_token_invalid("stale refresh token"))
        }
        RefreshRejection::ReuseDetected {
            user_id,
            reused_jti,
        } => {
            record_refresh_reuse_detection(state, user_id, &reused_jti, reuse_log_message).await?;
            Err(AsterError::auth_token_invalid(
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
                crate::db::transaction::with_transaction(&state.db, async |txn| {
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
    tracing::debug!("refreshing auth tokens");
    let claims = verify_token(refresh, &state.config.auth.jwt_secret)?;
    ensure_token_type(&claims, TokenType::Refresh)?;
    let refresh_jti = claims
        .jti
        .clone()
        .ok_or_else(|| AsterError::auth_token_invalid("refresh token missing jti"))?;

    let rotation_lock = acquire_refresh_rotation_lock(&refresh_jti).await;
    let reuse_evidence = if rotation_lock.was_contended {
        RefreshReuseEvidence::SameProcessContention
    } else {
        RefreshReuseEvidence::ClientFingerprint
    };
    let outcome = crate::db::transaction::with_transaction(&state.db, async |txn| {
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
    fn classify_refresh_reuse_session_treats_same_process_contention_as_stale_without_client_evidence()
     {
        let now = Utc::now();
        let session = make_auth_session(now);

        let outcome = classify_refresh_reuse_for_test(
            &session,
            now,
            None,
            None,
            RefreshReuseEvidence::SameProcessContention,
        );

        assert_stale_refresh(outcome, 1, "refresh-jti");
    }

    #[test]
    fn classify_refresh_reuse_session_treats_same_process_contention_as_stale_even_with_mismatched_client()
     {
        let now = Utc::now();
        let session = make_auth_session(now);

        let outcome = classify_refresh_reuse_for_test(
            &session,
            now,
            Some("203.0.113.11"),
            Some("other-browser"),
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
