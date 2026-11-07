//! Writer-owned Passkey/WebAuthn challenge consumption.

use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::db::repository::auth_once_state_repo;
use crate::errors::{AsterError, Result};
use crate::runtime::SharedRuntimeState;
use crate::services::auth::flow::{AuthFlowKind, AuthFlowSnapshot, AuthFlowState};

use super::{PasskeyAuthenticationChallenge, PasskeyRegistrationChallenge};

const PASSKEY_CHALLENGE_TTL_SECS: u64 = 300;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedPasskeyRegistrationFlow {
    flow_id: String,
    expires_at: chrono::DateTime<Utc>,
    challenge: PasskeyRegistrationChallenge,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedPasskeyLoginFlow {
    flow_id: String,
    expires_at: chrono::DateTime<Utc>,
    challenge: PasskeyAuthenticationChallenge,
}

impl PersistedPasskeyRegistrationFlow {
    fn snapshot(&self) -> AuthFlowSnapshot {
        AuthFlowSnapshot {
            flow_id: self.flow_id.clone(),
            kind: AuthFlowKind::PasskeyRegistration,
            state: AuthFlowState::FirstFactorPending,
            revision: 0,
            attempt_count: 0,
            max_attempts: Some(1),
            expires_at: self.expires_at,
        }
    }
}

impl PersistedPasskeyLoginFlow {
    fn snapshot(&self) -> AuthFlowSnapshot {
        AuthFlowSnapshot {
            flow_id: self.flow_id.clone(),
            kind: AuthFlowKind::PasskeyLogin,
            state: AuthFlowState::FirstFactorPending,
            revision: 0,
            attempt_count: 0,
            max_attempts: Some(1),
            expires_at: self.expires_at,
        }
    }
}

fn registration_state_key(flow_id: &str) -> String {
    format!("external_auth:passkey:registration:{flow_id}")
}

fn login_state_key(flow_id: &str) -> String {
    format!("external_auth:passkey:login:{flow_id}")
}

pub(super) async fn store_registration_challenge(
    state: &impl SharedRuntimeState,
    flow_id: &str,
    challenge: &PasskeyRegistrationChallenge,
) -> Result<()> {
    let flow = PersistedPasskeyRegistrationFlow {
        flow_id: flow_id.to_string(),
        expires_at: Utc::now() + Duration::seconds(PASSKEY_CHALLENGE_TTL_SECS as i64),
        challenge: challenge.clone(),
    };
    let payload = serde_json::to_string(&flow)
        .map_err(|_| AsterError::internal_error("serialize registration challenge"))?;
    match auth_once_state_repo::reserve(
        state.writer_db(),
        &registration_state_key(flow_id),
        &payload,
        flow.expires_at,
    )
    .await?
    {
        true => Ok(()),
        false => Err(AsterError::conflict(
            "passkey registration challenge already exists",
        )),
    }
}

pub(super) async fn take_registration_challenge(
    state: &impl SharedRuntimeState,
    flow_id: &str,
) -> Result<PasskeyRegistrationChallenge> {
    let invalid =
        || AsterError::auth_token_invalid("passkey registration challenge expired or consumed");
    let payload =
        auth_once_state_repo::consume(state.writer_db(), &registration_state_key(flow_id))
            .await?
            .ok_or_else(invalid)?;
    let flow: PersistedPasskeyRegistrationFlow =
        serde_json::from_str(&payload).map_err(|_| invalid())?;
    match flow.flow_id == flow_id && flow.snapshot().expires_at > Utc::now() {
        true => Ok(flow.challenge),
        false => Err(invalid()),
    }
}

pub(super) async fn store_login_challenge(
    state: &impl SharedRuntimeState,
    flow_id: &str,
    challenge: &PasskeyAuthenticationChallenge,
) -> Result<()> {
    let flow = PersistedPasskeyLoginFlow {
        flow_id: flow_id.to_string(),
        expires_at: Utc::now() + Duration::seconds(PASSKEY_CHALLENGE_TTL_SECS as i64),
        challenge: challenge.clone(),
    };
    let payload = serde_json::to_string(&flow)
        .map_err(|_| AsterError::internal_error("serialize login challenge"))?;
    match auth_once_state_repo::reserve(
        state.writer_db(),
        &login_state_key(flow_id),
        &payload,
        flow.expires_at,
    )
    .await?
    {
        true => Ok(()),
        false => Err(AsterError::conflict(
            "passkey login challenge already exists",
        )),
    }
}

pub(super) async fn take_login_challenge(
    state: &impl SharedRuntimeState,
    flow_id: &str,
) -> Result<PasskeyAuthenticationChallenge> {
    let invalid = || AsterError::auth_token_invalid("passkey login challenge expired or consumed");
    let payload = auth_once_state_repo::consume(state.writer_db(), &login_state_key(flow_id))
        .await?
        .ok_or_else(invalid)?;
    let flow: PersistedPasskeyLoginFlow = serde_json::from_str(&payload).map_err(|_| invalid())?;
    match flow.flow_id == flow_id && flow.snapshot().expires_at > Utc::now() {
        true => Ok(flow.challenge),
        false => Err(invalid()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::tasks::test_support::setup_primary_state;
    use webauthn_rs::prelude::WebauthnBuilder;

    #[tokio::test]
    async fn passkey_challenges_are_writer_owned_scoped_and_consumed_once() {
        let state = setup_primary_state().await;
        let webauthn =
            WebauthnBuilder::new("localhost", &url::Url::parse("http://localhost").unwrap())
                .unwrap()
                .build()
                .unwrap();
        let (_, challenge) = webauthn.start_discoverable_authentication().unwrap();
        let challenge = PasskeyAuthenticationChallenge {
            identifier: Some("alice".into()),
            state: challenge,
        };
        store_login_challenge(state.get_ref(), "login", &challenge)
            .await
            .unwrap();
        assert!(
            store_login_challenge(state.get_ref(), "login", &challenge)
                .await
                .is_err()
        );
        assert!(
            take_registration_challenge(state.get_ref(), "login")
                .await
                .is_err()
        );
        assert_eq!(
            take_login_challenge(state.get_ref(), "login")
                .await
                .unwrap()
                .identifier
                .as_deref(),
            Some("alice")
        );
        assert!(
            take_login_challenge(state.get_ref(), "login")
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn recovered_cache_copy_cannot_resurrect_a_consumed_passkey_challenge() {
        let state = setup_primary_state().await;
        let webauthn =
            WebauthnBuilder::new("localhost", &url::Url::parse("http://localhost").unwrap())
                .unwrap()
                .build()
                .unwrap();
        let (_, challenge) = webauthn.start_discoverable_authentication().unwrap();
        let challenge = PasskeyAuthenticationChallenge {
            identifier: None,
            state: challenge,
        };
        store_login_challenge(state.get_ref(), "recovery", &challenge)
            .await
            .unwrap();
        assert!(
            take_login_challenge(state.get_ref(), "recovery")
                .await
                .is_ok()
        );
        // A Redis timeout/fallback/recovery can leave any old cache payload;
        // no cache copy is authoritative after writer consumption.
        state
            .cache()
            .set_bytes(
                &login_state_key("recovery"),
                serde_json::to_vec(&challenge).unwrap(),
                Some(300),
            )
            .await;
        assert!(
            take_login_challenge(state.get_ref(), "recovery")
                .await
                .is_err()
        );
    }
}
