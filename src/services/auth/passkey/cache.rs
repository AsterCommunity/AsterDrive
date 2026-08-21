//! Passkey/WebAuthn flow challenge 缓存。

use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::runtime::SharedRuntimeState;
use crate::services::auth::flow::{AuthFlowKind, AuthFlowSnapshot, AuthFlowState};
use aster_forge_cache::CacheExt;

use super::{PasskeyAuthenticationChallenge, PasskeyRegistrationChallenge};

const PASSKEY_CHALLENGE_TTL_SECS: u64 = 300;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedPasskeyRegistrationFlow {
    flow_id: String,
    revision: u64,
    created_at: chrono::DateTime<Utc>,
    expires_at: chrono::DateTime<Utc>,
    challenge: PasskeyRegistrationChallenge,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedPasskeyLoginFlow {
    flow_id: String,
    revision: u64,
    created_at: chrono::DateTime<Utc>,
    expires_at: chrono::DateTime<Utc>,
    challenge: PasskeyAuthenticationChallenge,
}

impl CachedPasskeyRegistrationFlow {
    fn snapshot(&self) -> AuthFlowSnapshot {
        AuthFlowSnapshot {
            flow_id: self.flow_id.clone(),
            kind: AuthFlowKind::PasskeyRegistration,
            state: AuthFlowState::FirstFactorPending,
            revision: self.revision,
            attempt_count: 0,
            max_attempts: Some(1),
            expires_at: self.expires_at,
        }
    }
}

impl CachedPasskeyLoginFlow {
    fn snapshot(&self) -> AuthFlowSnapshot {
        AuthFlowSnapshot {
            flow_id: self.flow_id.clone(),
            kind: AuthFlowKind::PasskeyLogin,
            state: AuthFlowState::FirstFactorPending,
            revision: self.revision,
            attempt_count: 0,
            max_attempts: Some(1),
            expires_at: self.expires_at,
        }
    }
}

fn registration_cache_key(flow_id: &str) -> String {
    format!("external_auth:passkey:registration:{flow_id}")
}

fn login_cache_key(flow_id: &str) -> String {
    format!("external_auth:passkey:login:{flow_id}")
}

pub(super) async fn store_registration_challenge(
    state: &impl SharedRuntimeState,
    flow_id: &str,
    challenge: &PasskeyRegistrationChallenge,
) {
    let created_at = Utc::now();
    state
        .cache()
        .set(
            &registration_cache_key(flow_id),
            &CachedPasskeyRegistrationFlow {
                flow_id: flow_id.to_string(),
                revision: 0,
                created_at,
                expires_at: created_at + Duration::seconds(PASSKEY_CHALLENGE_TTL_SECS as i64),
                challenge: challenge.clone(),
            },
            Some(PASSKEY_CHALLENGE_TTL_SECS),
        )
        .await;
}

pub(super) async fn take_registration_challenge(
    state: &impl SharedRuntimeState,
    flow_id: &str,
) -> Option<PasskeyRegistrationChallenge> {
    let cached: CachedPasskeyRegistrationFlow =
        state.cache().take(&registration_cache_key(flow_id)).await?;
    let active = cached.flow_id == flow_id && cached.snapshot().expires_at > Utc::now();
    active.then_some(cached.challenge)
}

pub(super) async fn store_login_challenge(
    state: &impl SharedRuntimeState,
    flow_id: &str,
    challenge: &PasskeyAuthenticationChallenge,
) {
    let created_at = Utc::now();
    state
        .cache()
        .set(
            &login_cache_key(flow_id),
            &CachedPasskeyLoginFlow {
                flow_id: flow_id.to_string(),
                revision: 0,
                created_at,
                expires_at: created_at + Duration::seconds(PASSKEY_CHALLENGE_TTL_SECS as i64),
                challenge: challenge.clone(),
            },
            Some(PASSKEY_CHALLENGE_TTL_SECS),
        )
        .await;
}

pub(super) async fn take_login_challenge(
    state: &impl SharedRuntimeState,
    flow_id: &str,
) -> Option<PasskeyAuthenticationChallenge> {
    let cached: CachedPasskeyLoginFlow = state.cache().take(&login_cache_key(flow_id)).await?;
    let active = cached.flow_id == flow_id && cached.snapshot().expires_at > Utc::now();
    active.then_some(cached.challenge)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::test_support::CacheOnlyState;
    use webauthn_rs::prelude::{Uuid, Webauthn, WebauthnBuilder};

    fn webauthn() -> Webauthn {
        WebauthnBuilder::new(
            "localhost",
            &url::Url::parse("http://localhost").expect("test origin should parse"),
        )
        .expect("test webauthn builder should initialize")
        .rp_name("AsterDrive Test")
        .build()
        .expect("test webauthn should build")
    }

    fn registration_challenge(user_id: i64) -> PasskeyRegistrationChallenge {
        let user_handle = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
        let (_, state) = webauthn()
            .start_passkey_registration(user_handle, "alice", "Alice", None)
            .expect("test registration challenge should start");
        PasskeyRegistrationChallenge {
            user_id,
            user_handle,
            default_name: format!("Passkey {user_id}"),
            state,
        }
    }

    fn login_challenge(identifier: Option<&str>) -> PasskeyAuthenticationChallenge {
        let (_, state) = webauthn()
            .start_discoverable_authentication()
            .expect("test login challenge should start");
        PasskeyAuthenticationChallenge {
            identifier: identifier.map(str::to_string),
            state,
        }
    }

    #[tokio::test]
    async fn registration_challenge_is_consumed_once() {
        let state = CacheOnlyState::new().await;
        let challenge = registration_challenge(42);

        store_registration_challenge(&state, "flow", &challenge).await;

        assert_eq!(
            take_registration_challenge(&state, "flow")
                .await
                .map(|cached| cached.user_id),
            Some(42)
        );
        assert!(take_registration_challenge(&state, "flow").await.is_none());
    }

    #[tokio::test]
    async fn login_challenge_is_consumed_once_and_scoped_by_flow() {
        let state = CacheOnlyState::new().await;

        store_login_challenge(&state, "flow-a", &login_challenge(Some("alice"))).await;
        store_login_challenge(&state, "flow-b", &login_challenge(None)).await;

        assert_eq!(
            take_login_challenge(&state, "flow-a")
                .await
                .and_then(|cached| cached.identifier),
            Some("alice".to_string())
        );
        assert!(take_login_challenge(&state, "flow-a").await.is_none());
        assert_eq!(
            take_login_challenge(&state, "flow-b")
                .await
                .and_then(|cached| cached.identifier),
            None
        );
    }

    #[test]
    fn cached_flows_expose_typed_lifecycle_snapshots() {
        let created_at = Utc::now();
        let registration = CachedPasskeyRegistrationFlow {
            flow_id: "register-flow".to_string(),
            revision: 4,
            created_at,
            expires_at: created_at + Duration::minutes(5),
            challenge: registration_challenge(42),
        };
        assert_eq!(
            registration.snapshot().kind,
            AuthFlowKind::PasskeyRegistration
        );
        assert_eq!(registration.snapshot().revision, 4);

        let login = CachedPasskeyLoginFlow {
            flow_id: "login-flow".to_string(),
            revision: 2,
            created_at,
            expires_at: created_at + Duration::minutes(5),
            challenge: login_challenge(None),
        };
        assert_eq!(login.snapshot().kind, AuthFlowKind::PasskeyLogin);
        assert_eq!(login.snapshot().max_attempts, Some(1));
    }
}
