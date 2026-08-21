use chrono::{DateTime, Utc};

use aster_drive_model::entities::{
    auth_session, contact_verification_token, external_auth_email_verification_flow,
    external_auth_login_flow, user_invitation,
};
use aster_drive_model::types::{UserInvitationStatus, VerificationPurpose};

use super::{AuthFlowKind, AuthFlowSnapshot, AuthFlowState};

fn state_from_single_use(
    consumed_at: Option<DateTime<Utc>>,
    expires_at: DateTime<Utc>,
    active: AuthFlowState,
    now: DateTime<Utc>,
) -> AuthFlowState {
    if consumed_at.is_some() {
        AuthFlowState::Consumed
    } else if expires_at <= now {
        AuthFlowState::Expired
    } else {
        active
    }
}

pub fn external_login_snapshot(
    flow: &external_auth_login_flow::Model,
    now: DateTime<Utc>,
) -> AuthFlowSnapshot {
    AuthFlowSnapshot {
        flow_id: format!("external-login:{}", flow.id),
        kind: AuthFlowKind::ExternalLogin,
        state: state_from_single_use(
            flow.consumed_at,
            flow.expires_at,
            AuthFlowState::FirstFactorPending,
            now,
        ),
        revision: u64::from(flow.consumed_at.is_some()),
        attempt_count: 0,
        max_attempts: Some(1),
        expires_at: flow.expires_at,
    }
}

pub fn external_recovery_snapshot(
    flow: &external_auth_email_verification_flow::Model,
    now: DateTime<Utc>,
) -> AuthFlowSnapshot {
    AuthFlowSnapshot {
        flow_id: format!("external-recovery:{}", flow.id),
        kind: AuthFlowKind::ExternalEmailVerification,
        state: state_from_single_use(
            flow.consumed_at,
            flow.expires_at,
            AuthFlowState::RecoveryPending,
            now,
        ),
        revision: u64::from(flow.verification_token_hash.is_some())
            + u64::from(flow.consumed_at.is_some()),
        attempt_count: 0,
        max_attempts: None,
        expires_at: flow.expires_at,
    }
}

pub fn contact_verification_snapshot(
    token: &contact_verification_token::Model,
    now: DateTime<Utc>,
) -> AuthFlowSnapshot {
    let kind = match token.purpose {
        VerificationPurpose::RegisterActivation => AuthFlowKind::RegistrationActivation,
        VerificationPurpose::ContactChange => AuthFlowKind::EmailChange,
        VerificationPurpose::PasswordReset => AuthFlowKind::PasswordReset,
    };
    AuthFlowSnapshot {
        flow_id: format!("contact-verification:{}", token.id),
        kind,
        state: state_from_single_use(
            token.consumed_at,
            token.expires_at,
            AuthFlowState::RecoveryPending,
            now,
        ),
        revision: u64::from(token.consumed_at.is_some()),
        attempt_count: 0,
        max_attempts: Some(1),
        expires_at: token.expires_at,
    }
}

pub fn invitation_snapshot(
    invitation: &user_invitation::Model,
    now: DateTime<Utc>,
) -> AuthFlowSnapshot {
    let state = match invitation.status {
        UserInvitationStatus::Pending if invitation.expires_at <= now => AuthFlowState::Expired,
        UserInvitationStatus::Pending => AuthFlowState::RecoveryPending,
        UserInvitationStatus::Accepted => AuthFlowState::Completed,
        UserInvitationStatus::Expired => AuthFlowState::Expired,
        UserInvitationStatus::Revoked => AuthFlowState::Cancelled,
    };
    AuthFlowSnapshot {
        flow_id: format!("invitation:{}", invitation.id),
        kind: AuthFlowKind::InvitationAcceptance,
        state,
        revision: u64::from(!invitation.status.is_pending()),
        attempt_count: 0,
        max_attempts: Some(1),
        expires_at: invitation.expires_at,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthSessionLifecycleState {
    Active,
    Expired,
    Revoked,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthSessionSnapshot {
    pub session_id: String,
    pub state: AuthSessionLifecycleState,
    pub refresh_expires_at: DateTime<Utc>,
}

pub fn auth_session_snapshot(
    session: &auth_session::Model,
    now: DateTime<Utc>,
) -> AuthSessionSnapshot {
    let state = if session.revoked_at.is_some() {
        AuthSessionLifecycleState::Revoked
    } else if session.refresh_expires_at <= now {
        AuthSessionLifecycleState::Expired
    } else {
        AuthSessionLifecycleState::Active
    };
    AuthSessionSnapshot {
        session_id: session.id.clone(),
        state,
        refresh_expires_at: session.refresh_expires_at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aster_drive_model::types::{UserInvitationStatus, VerificationChannel};
    use chrono::Duration;

    #[test]
    fn single_use_state_precedence_is_consumed_then_expired() {
        let now = Utc::now();
        assert_eq!(
            state_from_single_use(
                Some(now),
                now - Duration::seconds(1),
                AuthFlowState::RecoveryPending,
                now
            ),
            AuthFlowState::Consumed
        );
        assert_eq!(
            state_from_single_use(None, now, AuthFlowState::RecoveryPending, now),
            AuthFlowState::Expired
        );
    }

    #[test]
    fn contact_verification_purpose_maps_to_typed_kind_without_token_material() {
        let now = Utc::now();
        let token = contact_verification_token::Model {
            id: 9,
            user_id: 3,
            channel: VerificationChannel::Email,
            purpose: VerificationPurpose::PasswordReset,
            target: "alice@example.com".to_string(),
            token_hash: "secret-hash".to_string(),
            expires_at: now + Duration::minutes(5),
            consumed_at: None,
            created_at: now,
        };
        let snapshot = contact_verification_snapshot(&token, now);
        assert_eq!(snapshot.kind, AuthFlowKind::PasswordReset);
        assert_eq!(snapshot.flow_id, "contact-verification:9");
        assert!(!snapshot.flow_id.contains("secret"));
    }

    #[test]
    fn invitation_terminal_states_are_distinct() {
        let now = Utc::now();
        for (status, expected) in [
            (UserInvitationStatus::Accepted, AuthFlowState::Completed),
            (UserInvitationStatus::Expired, AuthFlowState::Expired),
            (UserInvitationStatus::Revoked, AuthFlowState::Cancelled),
        ] {
            let invitation = user_invitation::Model {
                id: 7,
                email: "alice@example.com".to_string(),
                token_hash: "hash".to_string(),
                status,
                invited_by: 1,
                accepted_user_id: None,
                expires_at: now + Duration::minutes(5),
                created_at: now,
                updated_at: now,
                accepted_at: None,
                revoked_at: None,
            };
            assert_eq!(invitation_snapshot(&invitation, now).state, expected);
        }
    }

    #[test]
    fn revoked_session_precedes_expiry() {
        let now = Utc::now();
        let session = auth_session::Model {
            id: "session".to_string(),
            user_id: 4,
            current_refresh_jti: "current".to_string(),
            previous_refresh_jti: None,
            refresh_expires_at: now - Duration::minutes(1),
            ip_address: None,
            user_agent: None,
            created_at: now - Duration::hours(1),
            last_seen_at: now - Duration::hours(1),
            revoked_at: Some(now),
        };
        assert_eq!(
            auth_session_snapshot(&session, now).state,
            AuthSessionLifecycleState::Revoked
        );
    }
}
