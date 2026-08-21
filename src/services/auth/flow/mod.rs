//! Shared lifecycle rules for typed authentication flows.
//!
//! Security payloads remain in their owning MFA, external-auth, recovery, or
//! Passkey records. This module only owns lifecycle vocabulary and transition
//! validation so every adapter applies expiry, cancellation, replay, attempt,
//! and optimistic-concurrency rules in the same order.

use chrono::{DateTime, Utc};

mod coordinator;
mod snapshots;

pub use coordinator::{
    authenticate, cancelled, complete, consume, default_expiry, new_primary_flow, new_recovery_flow,
};
pub use snapshots::{
    AuthSessionLifecycleState, AuthSessionSnapshot, auth_session_snapshot,
    contact_verification_snapshot, external_login_snapshot, external_recovery_snapshot,
    invitation_snapshot,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AuthFlowKind {
    PasswordLogin,
    PasskeyRegistration,
    PasskeyLogin,
    ExternalLogin,
    MfaLogin,
    RegistrationActivation,
    InvitationAcceptance,
    PasswordReset,
    EmailChange,
    ExternalEmailVerification,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AuthFlowState {
    FirstFactorPending,
    SecondFactorPending,
    RecoveryPending,
    Processing,
    PasswordChangeRequired,
    Authenticated,
    Completed,
    Failed,
    Expired,
    Cancelled,
    Consumed,
}

impl AuthFlowState {
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Authenticated
                | Self::Completed
                | Self::Failed
                | Self::Expired
                | Self::Cancelled
                | Self::Consumed
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthFlowSnapshot {
    pub flow_id: String,
    pub kind: AuthFlowKind,
    pub state: AuthFlowState,
    pub revision: u64,
    pub attempt_count: u32,
    pub max_attempts: Option<u32>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthFlowCommand {
    Transition {
        expected_revision: u64,
        to: AuthFlowState,
    },
    RecordFailure {
        expected_revision: u64,
    },
    Cancel {
        expected_revision: u64,
    },
    Expire {
        expected_revision: u64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthFlowTransitionError {
    AttemptBudgetExhausted,
    Expired,
    IllegalTransition {
        from: AuthFlowState,
        to: AuthFlowState,
    },
    RevisionConflict {
        expected: u64,
        actual: u64,
    },
    Terminal(AuthFlowState),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuthFlowTransition {
    pub state: AuthFlowState,
    pub revision: u64,
    pub attempt_count: u32,
}

pub fn plan_transition(
    snapshot: &AuthFlowSnapshot,
    command: AuthFlowCommand,
    now: DateTime<Utc>,
) -> Result<AuthFlowTransition, AuthFlowTransitionError> {
    let expected_revision = match command {
        AuthFlowCommand::Transition {
            expected_revision, ..
        }
        | AuthFlowCommand::RecordFailure { expected_revision }
        | AuthFlowCommand::Cancel { expected_revision }
        | AuthFlowCommand::Expire { expected_revision } => expected_revision,
    };
    if expected_revision != snapshot.revision {
        return Err(AuthFlowTransitionError::RevisionConflict {
            expected: expected_revision,
            actual: snapshot.revision,
        });
    }
    if snapshot.state.is_terminal() {
        return Err(AuthFlowTransitionError::Terminal(snapshot.state));
    }

    let next_revision = snapshot.revision.saturating_add(1);
    match command {
        AuthFlowCommand::Expire { .. } => Ok(AuthFlowTransition {
            state: AuthFlowState::Expired,
            revision: next_revision,
            attempt_count: snapshot.attempt_count,
        }),
        _ if snapshot.expires_at <= now => Err(AuthFlowTransitionError::Expired),
        AuthFlowCommand::Cancel { .. } => Ok(AuthFlowTransition {
            state: AuthFlowState::Cancelled,
            revision: next_revision,
            attempt_count: snapshot.attempt_count,
        }),
        AuthFlowCommand::RecordFailure { .. } => {
            let attempt_count = snapshot.attempt_count.saturating_add(1);
            let exhausted = snapshot
                .max_attempts
                .is_some_and(|max_attempts| attempt_count >= max_attempts);
            Ok(AuthFlowTransition {
                state: if exhausted {
                    AuthFlowState::Failed
                } else {
                    snapshot.state
                },
                revision: next_revision,
                attempt_count,
            })
        }
        AuthFlowCommand::Transition { to, .. } => {
            if snapshot
                .max_attempts
                .is_some_and(|max_attempts| snapshot.attempt_count >= max_attempts)
            {
                return Err(AuthFlowTransitionError::AttemptBudgetExhausted);
            }
            if !transition_allowed(snapshot.kind, snapshot.state, to) {
                return Err(AuthFlowTransitionError::IllegalTransition {
                    from: snapshot.state,
                    to,
                });
            }
            Ok(AuthFlowTransition {
                state: to,
                revision: next_revision,
                attempt_count: snapshot.attempt_count,
            })
        }
    }
}

const fn transition_allowed(kind: AuthFlowKind, from: AuthFlowState, to: AuthFlowState) -> bool {
    if matches!(to, AuthFlowState::Failed) {
        return true;
    }
    if matches!(to, AuthFlowState::Consumed) {
        return true;
    }
    match kind {
        AuthFlowKind::PasswordLogin => matches!(
            (from, to),
            (
                AuthFlowState::FirstFactorPending,
                AuthFlowState::SecondFactorPending
                    | AuthFlowState::PasswordChangeRequired
                    | AuthFlowState::Authenticated
                    | AuthFlowState::Processing
            ) | (
                AuthFlowState::Processing,
                AuthFlowState::SecondFactorPending
            ) | (
                AuthFlowState::SecondFactorPending,
                AuthFlowState::PasswordChangeRequired | AuthFlowState::Authenticated
            ) | (
                AuthFlowState::PasswordChangeRequired,
                AuthFlowState::Authenticated
            )
        ),
        AuthFlowKind::PasskeyRegistration => matches!(
            (from, to),
            (AuthFlowState::FirstFactorPending, AuthFlowState::Processing)
                | (AuthFlowState::Processing, AuthFlowState::Completed)
        ),
        AuthFlowKind::PasskeyLogin | AuthFlowKind::ExternalLogin => matches!(
            (from, to),
            (AuthFlowState::FirstFactorPending, AuthFlowState::Processing)
                | (
                    AuthFlowState::Processing,
                    AuthFlowState::SecondFactorPending | AuthFlowState::Authenticated
                )
        ),
        AuthFlowKind::MfaLogin => matches!(
            (from, to),
            (
                AuthFlowState::SecondFactorPending,
                AuthFlowState::PasswordChangeRequired | AuthFlowState::Authenticated
            )
        ),
        AuthFlowKind::RegistrationActivation
        | AuthFlowKind::InvitationAcceptance
        | AuthFlowKind::PasswordReset
        | AuthFlowKind::EmailChange
        | AuthFlowKind::ExternalEmailVerification => matches!(
            (from, to),
            (AuthFlowState::RecoveryPending, AuthFlowState::Processing)
                | (AuthFlowState::Processing, AuthFlowState::Completed)
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn snapshot(kind: AuthFlowKind, state: AuthFlowState) -> AuthFlowSnapshot {
        AuthFlowSnapshot {
            flow_id: "test-flow".to_string(),
            kind,
            state,
            revision: 7,
            attempt_count: 0,
            max_attempts: Some(3),
            expires_at: Utc::now() + Duration::minutes(5),
        }
    }

    #[test]
    fn allows_each_primary_login_completion_path() {
        let now = Utc::now();
        for (kind, from, targets) in [
            (
                AuthFlowKind::PasswordLogin,
                AuthFlowState::FirstFactorPending,
                vec![
                    AuthFlowState::SecondFactorPending,
                    AuthFlowState::PasswordChangeRequired,
                    AuthFlowState::Authenticated,
                ],
            ),
            (
                AuthFlowKind::PasskeyLogin,
                AuthFlowState::Processing,
                vec![
                    AuthFlowState::SecondFactorPending,
                    AuthFlowState::Authenticated,
                ],
            ),
            (
                AuthFlowKind::ExternalLogin,
                AuthFlowState::Processing,
                vec![
                    AuthFlowState::SecondFactorPending,
                    AuthFlowState::Authenticated,
                ],
            ),
        ] {
            for to in targets {
                let result = plan_transition(
                    &snapshot(kind, from),
                    AuthFlowCommand::Transition {
                        expected_revision: 7,
                        to,
                    },
                    now,
                )
                .unwrap();
                assert_eq!(result.state, to);
                assert_eq!(result.revision, 8);
            }
        }
    }

    #[test]
    fn rejects_cross_context_and_skipped_recovery_transitions() {
        let now = Utc::now();
        for (kind, from, to) in [
            (
                AuthFlowKind::MfaLogin,
                AuthFlowState::SecondFactorPending,
                AuthFlowState::Completed,
            ),
            (
                AuthFlowKind::PasswordReset,
                AuthFlowState::RecoveryPending,
                AuthFlowState::Authenticated,
            ),
            (
                AuthFlowKind::ExternalLogin,
                AuthFlowState::FirstFactorPending,
                AuthFlowState::Authenticated,
            ),
        ] {
            assert_eq!(
                plan_transition(
                    &snapshot(kind, from),
                    AuthFlowCommand::Transition {
                        expected_revision: 7,
                        to,
                    },
                    now,
                ),
                Err(AuthFlowTransitionError::IllegalTransition { from, to })
            );
        }
    }

    #[test]
    fn failure_exhausts_attempt_budget_without_overflow() {
        let now = Utc::now();
        let mut flow = snapshot(AuthFlowKind::MfaLogin, AuthFlowState::SecondFactorPending);
        flow.attempt_count = 2;
        let result = plan_transition(
            &flow,
            AuthFlowCommand::RecordFailure {
                expected_revision: 7,
            },
            now,
        )
        .unwrap();
        assert_eq!(result.attempt_count, 3);
        assert_eq!(result.state, AuthFlowState::Failed);

        flow.attempt_count = u32::MAX;
        flow.max_attempts = None;
        assert_eq!(
            plan_transition(
                &flow,
                AuthFlowCommand::RecordFailure {
                    expected_revision: 7,
                },
                now,
            )
            .unwrap()
            .attempt_count,
            u32::MAX
        );
    }

    #[test]
    fn expiry_precedes_cancel_and_normal_transition() {
        let now = Utc::now();
        let mut flow = snapshot(AuthFlowKind::PasswordReset, AuthFlowState::RecoveryPending);
        flow.expires_at = now;
        for command in [
            AuthFlowCommand::Cancel {
                expected_revision: 7,
            },
            AuthFlowCommand::Transition {
                expected_revision: 7,
                to: AuthFlowState::Processing,
            },
        ] {
            assert_eq!(
                plan_transition(&flow, command, now),
                Err(AuthFlowTransitionError::Expired)
            );
        }
        assert_eq!(
            plan_transition(
                &flow,
                AuthFlowCommand::Expire {
                    expected_revision: 7,
                },
                now,
            )
            .unwrap()
            .state,
            AuthFlowState::Expired
        );
    }

    #[test]
    fn revision_and_terminal_guards_are_stable() {
        let now = Utc::now();
        let flow = snapshot(
            AuthFlowKind::ExternalEmailVerification,
            AuthFlowState::RecoveryPending,
        );
        assert_eq!(
            plan_transition(
                &flow,
                AuthFlowCommand::Cancel {
                    expected_revision: 6,
                },
                now,
            ),
            Err(AuthFlowTransitionError::RevisionConflict {
                expected: 6,
                actual: 7,
            })
        );

        for terminal in [
            AuthFlowState::Authenticated,
            AuthFlowState::Completed,
            AuthFlowState::Failed,
            AuthFlowState::Expired,
            AuthFlowState::Cancelled,
            AuthFlowState::Consumed,
        ] {
            let flow = snapshot(AuthFlowKind::PasswordReset, terminal);
            assert_eq!(
                plan_transition(
                    &flow,
                    AuthFlowCommand::Cancel {
                        expected_revision: 7,
                    },
                    now,
                ),
                Err(AuthFlowTransitionError::Terminal(terminal))
            );
        }
    }
}
