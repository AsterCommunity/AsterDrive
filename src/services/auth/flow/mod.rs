//! Shared lifecycle rules for typed authentication flows.
//!
//! Security payloads remain in their owning MFA, external-auth, recovery, or
//! Passkey records. This module only owns lifecycle vocabulary and transition
//! validation so every adapter applies expiry, cancellation, replay, attempt,
//! and optimistic-concurrency rules in the same order.

use chrono::{DateTime, Utc};

mod coordinator;
mod snapshots;

pub use coordinator::new_primary_flow;
pub use snapshots::{
    AuthSessionLifecycleState, AuthSessionSnapshot, auth_session_snapshot,
    contact_verification_snapshot, external_login_snapshot, external_recovery_snapshot,
    invitation_snapshot,
};

/// Typed category of an authentication flow; selects the legal state machine in
/// [`plan_transition`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AuthFlowKind {
    /// First-factor username/email + password login.
    PasswordLogin,
    /// Browser WebAuthn credential creation flow.
    PasskeyRegistration,
    /// Discoverable-credential WebAuthn login flow.
    PasskeyLogin,
    /// Login completed through an external identity provider.
    ExternalLogin,
    /// Second-factor challenge issued after a validated first factor.
    MfaLogin,
    /// Email token that activates a freshly registered account.
    RegistrationActivation,
    /// Email token that lets an invitee join and activate their account.
    InvitationAcceptance,
    /// Email token granting a one-shot password change for a locked-out account.
    PasswordReset,
    /// Email token confirming a pending email address change.
    EmailChange,
    /// Email token verifying an address claimed through external sign-in.
    ExternalEmailVerification,
}

/// Lifecycle state of a flow. Terminal states reject every further transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AuthFlowState {
    /// First factor (identifier + password, passkey, external identity) still owes verification.
    FirstFactorPending,
    /// First factor verified; an MFA challenge must still pass.
    SecondFactorPending,
    /// Recovery-style token issued and waiting to be exchanged (activation, reset, invitation).
    RecoveryPending,
    /// Provider challenge is in flight; only provider completion may leave this state.
    Processing,
    /// Second factor passed but the account still owes an administrator-forced password change.
    PasswordChangeRequired,
    /// Terminal: login fully succeeded and session issuance is authorized.
    Authenticated,
    /// Terminal: single-use token or registration-style flow finished successfully.
    Completed,
    /// Terminal: attempt budget exhausted.
    Failed,
    /// Terminal: expiry deadline passed.
    Expired,
    /// Terminal: explicitly abandoned by the caller.
    Cancelled,
    /// Terminal: single-use token exchanged; replay attempts land here.
    Consumed,
}

impl AuthFlowState {
    /// Whether no further command may ever be applied to a flow in this state.
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

/// Immutable projection of a flow's persisted state at the moment a command is planned.
///
/// Adapters derive this from their owning record (consumed flag, attempt counter, expiry) and
/// feed it back into [`plan_transition`]; the result must be persisted with the new revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthFlowSnapshot {
    /// Stable identifier used for audit trails; must never contain token material.
    pub flow_id: String,
    /// Flow category driving the legal transition table.
    pub kind: AuthFlowKind,
    /// Current lifecycle state.
    pub state: AuthFlowState,
    /// Optimistic-concurrency counter; commands carry the revision they were planned against.
    pub revision: u64,
    /// Failed verification attempts recorded so far.
    pub attempt_count: u32,
    /// Upper bound on failed attempts before the flow fails; `None` means unlimited.
    pub max_attempts: Option<u32>,
    /// Inclusive deadline; every non-expiry command is rejected once `now` passes it.
    pub expires_at: DateTime<Utc>,
}

/// Command applied to a flow snapshot. Every variant carries the revision the caller observed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthFlowCommand {
    /// Move the flow to a new state; validated against the per-kind transition table.
    Transition {
        /// Revision the caller read; must equal the snapshot's current revision.
        expected_revision: u64,
        /// Requested next state.
        to: AuthFlowState,
    },
    /// Record a failed verification attempt; consumes the attempt budget and may fail the flow.
    RecordFailure {
        /// Revision the caller read; must equal the snapshot's current revision.
        expected_revision: u64,
    },
    /// Abandon the flow; legal from any non-terminal state and subject to expiry.
    Cancel {
        /// Revision the caller read; must equal the snapshot's current revision.
        expected_revision: u64,
    },
    /// Force the expired terminal state; the only command allowed on an already-expired flow.
    Expire {
        /// Revision the caller read; must equal the snapshot's current revision.
        expected_revision: u64,
    },
}

/// Why a command was rejected. Order of checks: revision, terminal, expiry, then per-command rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthFlowTransitionError {
    /// A `Transition` was attempted after the attempt budget was already exhausted.
    AttemptBudgetExhausted,
    /// The flow's `expires_at` deadline passed before the command was applied.
    Expired,
    /// The requested state is not reachable from the current state for this flow kind.
    IllegalTransition {
        /// State the flow was in when the illegal command arrived.
        from: AuthFlowState,
        /// State the command tried to move to.
        to: AuthFlowState,
    },
    /// Optimistic concurrency check failed; re-read the flow and retry.
    RevisionConflict {
        /// Revision the command expected.
        expected: u64,
        /// Revision actually present on the snapshot.
        actual: u64,
    },
    /// The flow already sits in this terminal state.
    Terminal(AuthFlowState),
}

/// Successful result of planning a command: the values the adapter must persist atomically.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuthFlowTransition {
    /// Next lifecycle state after the command.
    pub state: AuthFlowState,
    /// Revision to persist (previous revision + 1).
    pub revision: u64,
    /// Attempt count to persist; only `RecordFailure` changes it.
    pub attempt_count: u32,
}

/// Validates a lifecycle command against a flow snapshot without touching any record.
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

/// Per-kind transition table. `Failed` and `Consumed` are reachable from any non-terminal state.
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
