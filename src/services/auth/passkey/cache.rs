//! Passkey/WebAuthn flow challenge 缓存。

use crate::runtime::SharedRuntimeState;
use aster_forge_cache::CacheExt;

use super::{PasskeyAuthenticationChallenge, PasskeyRegistrationChallenge};

const PASSKEY_CHALLENGE_TTL_SECS: u64 = 300;

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
    state
        .cache()
        .set(
            &registration_cache_key(flow_id),
            challenge,
            Some(PASSKEY_CHALLENGE_TTL_SECS),
        )
        .await;
}

pub(super) async fn take_registration_challenge(
    state: &impl SharedRuntimeState,
    flow_id: &str,
) -> Option<PasskeyRegistrationChallenge> {
    state.cache().take(&registration_cache_key(flow_id)).await
}

pub(super) async fn store_login_challenge(
    state: &impl SharedRuntimeState,
    flow_id: &str,
    challenge: &PasskeyAuthenticationChallenge,
) {
    state
        .cache()
        .set(
            &login_cache_key(flow_id),
            challenge,
            Some(PASSKEY_CHALLENGE_TTL_SECS),
        )
        .await;
}

pub(super) async fn take_login_challenge(
    state: &impl SharedRuntimeState,
    flow_id: &str,
) -> Option<PasskeyAuthenticationChallenge> {
    state.cache().take(&login_cache_key(flow_id)).await
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
}
