//! Executable auth-flow boundary.
//!
//! Adapters keep their typed payloads and transactions, while every stateful
//! operation enters through these helpers. This prevents callers from
//! reimplementing terminal, expiry, attempt, and revision semantics locally.

use chrono::{DateTime, Utc};

use super::{
    AuthFlowCommand, AuthFlowKind, AuthFlowSnapshot, AuthFlowState, AuthFlowTransitionError,
    plan_transition,
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
            now + Duration::minutes(5),
            now,
        )
        .unwrap();
        assert_eq!(flow.state, AuthFlowState::Processing);
        assert!(
            plan_transition(
                &flow,
                AuthFlowCommand::Transition {
                    expected_revision: 1,
                    to: AuthFlowState::Completed,
                },
                now,
            )
            .is_err()
        );
        assert!(
            plan_transition(
                &flow,
                AuthFlowCommand::Transition {
                    expected_revision: 1,
                    to: AuthFlowState::Authenticated,
                },
                now,
            )
            .is_ok()
        );
    }

    #[test]
    fn recovery_boundary_is_single_use_and_expiry_aware() {
        let now = Utc::now();
        let flow = AuthFlowSnapshot {
            flow_id: "contact-verification:4".to_string(),
            kind: AuthFlowKind::PasswordReset,
            state: AuthFlowState::Processing,
            revision: 1,
            attempt_count: 0,
            max_attempts: Some(1),
            expires_at: now + Duration::minutes(5),
        };
        assert!(
            plan_transition(
                &flow,
                AuthFlowCommand::Transition {
                    expected_revision: 1,
                    to: AuthFlowState::Completed,
                },
                now,
            )
            .is_ok()
        );

        let mut expired = flow;
        expired.expires_at = now;
        assert_eq!(
            plan_transition(
                &expired,
                AuthFlowCommand::Transition {
                    expected_revision: 1,
                    to: AuthFlowState::Completed,
                },
                now,
            ),
            Err(AuthFlowTransitionError::Expired)
        );
    }

    #[test]
    fn cancel_and_consume_are_distinct_terminal_commands() {
        let now = Utc::now();
        let mut flow = new_primary_flow(
            AuthFlowKind::PasskeyLogin,
            "passkey:1",
            now + Duration::minutes(5),
            now,
        )
        .unwrap();
        flow.state = AuthFlowState::Processing;
        assert_eq!(
            plan_transition(
                &flow,
                AuthFlowCommand::Cancel {
                    expected_revision: 1,
                },
                now,
            )
            .unwrap()
            .state,
            AuthFlowState::Cancelled
        );
        assert_eq!(
            plan_transition(
                &flow,
                AuthFlowCommand::Transition {
                    expected_revision: 1,
                    to: AuthFlowState::Consumed,
                },
                now,
            )
            .unwrap()
            .state,
            AuthFlowState::Consumed
        );
    }
}
