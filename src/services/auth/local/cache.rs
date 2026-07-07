//! 认证领域缓存。

use aster_forge_cache::CacheExt;
use crate::runtime::SharedRuntimeState;

use super::AuthSnapshot;

const AUTH_SNAPSHOT_TTL: u64 = 30;

fn auth_snapshot_cache_key(user_id: i64) -> String {
    format!("auth_snapshot:{user_id}")
}

pub(super) async fn load_auth_snapshot(
    state: &impl SharedRuntimeState,
    user_id: i64,
) -> Option<AuthSnapshot> {
    state.cache().get(&auth_snapshot_cache_key(user_id)).await
}

pub(super) async fn store_auth_snapshot(
    state: &impl SharedRuntimeState,
    user_id: i64,
    snapshot: &AuthSnapshot,
) {
    state
        .cache()
        .set(
            &auth_snapshot_cache_key(user_id),
            snapshot,
            Some(AUTH_SNAPSHOT_TTL),
        )
        .await;
}

pub(super) async fn invalidate_auth_snapshot(state: &impl SharedRuntimeState, user_id: i64) {
    state
        .cache()
        .delete(&auth_snapshot_cache_key(user_id))
        .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::test_support::CacheOnlyState;
    use crate::types::{UserRole, UserStatus};

    fn snapshot(session_version: i64) -> AuthSnapshot {
        AuthSnapshot {
            status: UserStatus::Active,
            role: UserRole::User,
            session_version,
            must_change_password: false,
        }
    }

    #[tokio::test]
    async fn auth_snapshot_is_scoped_by_user_and_can_be_invalidated() {
        let state = CacheOnlyState::new().await;

        store_auth_snapshot(&state, 1, &snapshot(10)).await;
        store_auth_snapshot(&state, 2, &snapshot(20)).await;

        assert_eq!(
            load_auth_snapshot(&state, 1)
                .await
                .map(|value| value.session_version),
            Some(10)
        );
        assert_eq!(
            load_auth_snapshot(&state, 2)
                .await
                .map(|value| value.session_version),
            Some(20)
        );

        invalidate_auth_snapshot(&state, 1).await;

        assert!(load_auth_snapshot(&state, 1).await.is_none());
        assert_eq!(
            load_auth_snapshot(&state, 2)
                .await
                .map(|value| value.session_version),
            Some(20)
        );
    }
}
