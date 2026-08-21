//! Executable auth-flow boundary.
//!
//! Adapters keep their typed payloads and transactions, while every stateful
//! operation enters through these helpers. This prevents callers from
//! reimplementing terminal, expiry, attempt, and revision semantics locally.

use chrono::{DateTime, Duration, Utc};

use super::{
    AuthFlowCommand, AuthFlowKind, AuthFlowSnapshot, AuthFlowState, AuthFlowTransition,
    AuthFlowTransitionError, plan_transition,
};

pub fn new_primary_flow(
    kind: AuthFlowKind,
    flow_id: impl Into<String>,
    expires_at: DateTime<Utc>,
    now: DateTime<Utc>,
) -> Result<AuthFlowSnapshot, AuthFlowTransitionError> {
    let snapshot = AuthFlowSnapshot {
        flow_id: flow_id.into(),
        kind,
        state: AuthFlowState::FirstFactorPending,
        revision: 0,
        attempt_count: 0,
        max_attempts: None,
        expires_at,
    };
    let transition = plan_transition(
        &snapshot,
        AuthFlowCommand::Transition {
            expected_revision: 0,
            to: AuthFlowState::Processing,
        },
        now,
    )?;
    Ok(AuthFlowSnapshot {
        state: transition.state,
        revision: transition.revision,
        ..snapshot
    })
}

pub fn new_recovery_flow(
    kind: AuthFlowKind,
    flow_id: impl Into<String>,
    expires_at: DateTime<Utc>,
    now: DateTime<Utc>,
) -> Result<AuthFlowSnapshot, AuthFlowTransitionError> {
    let snapshot = AuthFlowSnapshot {
        flow_id: flow_id.into(),
        kind,
        state: AuthFlowState::RecoveryPending,
        revision: 0,
        attempt_count: 0,
        max_attempts: Some(1),
        expires_at,
    };
    let transition = plan_transition(
        &snapshot,
        AuthFlowCommand::Transition {
            expected_revision: 0,
            to: AuthFlowState::Processing,
        },
        now,
    )?;
    Ok(AuthFlowSnapshot {
        state: transition.state,
        revision: transition.revision,
        ..snapshot
    })
}

pub fn complete(
    snapshot: &AuthFlowSnapshot,
    expected_revision: u64,
    now: DateTime<Utc>,
) -> Result<AuthFlowTransition, AuthFlowTransitionError> {
    plan_transition(
        snapshot,
        AuthFlowCommand::Transition {
            expected_revision,
            to: AuthFlowState::Completed,
        },
        now,
    )
}

pub fn authenticate(
    snapshot: &AuthFlowSnapshot,
    expected_revision: u64,
    now: DateTime<Utc>,
) -> Result<AuthFlowTransition, AuthFlowTransitionError> {
    plan_transition(
        snapshot,
        AuthFlowCommand::Transition {
            expected_revision,
            to: AuthFlowState::Authenticated,
        },
        now,
    )
}

pub fn consume(
    snapshot: &AuthFlowSnapshot,
    expected_revision: u64,
    now: DateTime<Utc>,
) -> Result<AuthFlowTransition, AuthFlowTransitionError> {
    plan_transition(
        snapshot,
        AuthFlowCommand::Transition {
            expected_revision,
            to: AuthFlowState::Consumed,
        },
        now,
    )
}

pub fn cancelled(
    snapshot: &AuthFlowSnapshot,
    expected_revision: u64,
    now: DateTime<Utc>,
) -> Result<AuthFlowTransition, AuthFlowTransitionError> {
    plan_transition(snapshot, AuthFlowCommand::Cancel { expected_revision }, now)
}

pub fn default_expiry(ttl: Duration, now: DateTime<Utc>) -> DateTime<Utc> {
    now + ttl
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn executable_primary_boundary_requires_processing_before_completion() {
        let now = Utc::now();
        let flow = new_primary_flow(
            AuthFlowKind::ExternalLogin,
            "external-login:1",
            default_expiry(Duration::minutes(5), now),
            now,
        )
        .unwrap();
        assert_eq!(flow.state, AuthFlowState::Processing);
        assert!(complete(&flow, 1, now).is_err());
        assert!(authenticate(&flow, 1, now).is_ok());
    }

    #[test]
    fn recovery_boundary_is_single_use_and_expiry_aware() {
        let now = Utc::now();
        let flow = new_recovery_flow(
            AuthFlowKind::PasswordReset,
            "contact-verification:4",
            default_expiry(Duration::minutes(5), now),
            now,
        )
        .unwrap();
        assert!(complete(&flow, 1, now).is_ok());

        let mut expired = flow;
        expired.expires_at = now;
        assert_eq!(
            complete(&expired, 1, now),
            Err(AuthFlowTransitionError::Expired)
        );
    }

    #[test]
    fn cancel_and_consume_are_distinct_terminal_commands() {
        let now = Utc::now();
        let mut flow = new_primary_flow(
            AuthFlowKind::PasskeyLogin,
            "passkey:1",
            default_expiry(Duration::minutes(5), now),
            now,
        )
        .unwrap();
        flow.state = AuthFlowState::Processing;
        assert_eq!(
            cancelled(&flow, 1, now).unwrap().state,
            AuthFlowState::Cancelled
        );
        assert_eq!(
            consume(&flow, 1, now).unwrap().state,
            AuthFlowState::Consumed
        );
    }
}
