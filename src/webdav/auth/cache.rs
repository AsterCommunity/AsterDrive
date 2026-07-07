//! WebDAV 认证缓存。

use aster_forge_cache::CacheExt;
use crate::runtime::SharedRuntimeState;
use crate::utils::hash;

use super::CachedWebdavAuth;

const WEBDAV_AUTH_CACHE_TTL: u64 = 60;

pub(super) fn username_cache_component(username: &str) -> String {
    hash::sha256_hex(username.as_bytes())
}

fn password_cache_component(password: &str) -> String {
    hash::sha256_hex(password.as_bytes())
}

fn auth_cache_prefix(username: &str) -> String {
    format!("webdav_auth:{}:", username_cache_component(username))
}

fn auth_cache_key(username: &str, password: &str) -> String {
    format!(
        "{}{}",
        auth_cache_prefix(username),
        password_cache_component(password)
    )
}

pub(super) async fn load_auth(
    state: &impl SharedRuntimeState,
    username: &str,
    password: &str,
) -> Option<CachedWebdavAuth> {
    state
        .cache()
        .get::<CachedWebdavAuth>(&auth_cache_key(username, password))
        .await
}

pub(super) async fn store_auth(
    state: &impl SharedRuntimeState,
    username: &str,
    password: &str,
    cached: &CachedWebdavAuth,
) {
    state
        .cache()
        .set(
            &auth_cache_key(username, password),
            cached,
            Some(WEBDAV_AUTH_CACHE_TTL),
        )
        .await;
}

pub(super) async fn invalidate_for_username(state: &impl SharedRuntimeState, username: &str) {
    state
        .cache()
        .invalidate_prefix(&auth_cache_prefix(username))
        .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::test_support::CacheOnlyState;

    fn cached(account_id: i64) -> CachedWebdavAuth {
        CachedWebdavAuth {
            account_id,
            user_id: 10,
            team_id: None,
            root_folder_id: Some(20),
        }
    }

    #[tokio::test]
    async fn auth_cache_key_hashes_username_and_password() {
        let key = auth_cache_key("webdav-user", "secret-password");

        assert!(key.starts_with("webdav_auth:"));
        assert!(!key.contains("webdav-user"));
        assert!(!key.contains("secret-password"));
    }

    #[tokio::test]
    async fn auth_cache_is_scoped_by_username_and_password() {
        let state = CacheOnlyState::new().await;

        store_auth(&state, "alice", "password-a", &cached(1)).await;
        store_auth(&state, "alice", "password-b", &cached(2)).await;
        store_auth(&state, "bob", "password-a", &cached(3)).await;

        assert_eq!(
            load_auth(&state, "alice", "password-a")
                .await
                .map(|value| value.account_id),
            Some(1)
        );
        assert_eq!(
            load_auth(&state, "alice", "password-b")
                .await
                .map(|value| value.account_id),
            Some(2)
        );
        assert_eq!(
            load_auth(&state, "bob", "password-a")
                .await
                .map(|value| value.account_id),
            Some(3)
        );
    }

    #[tokio::test]
    async fn username_invalidation_keeps_other_user_cache_entries() {
        let state = CacheOnlyState::new().await;

        store_auth(&state, "alice", "password-a", &cached(1)).await;
        store_auth(&state, "bob", "password-a", &cached(2)).await;

        invalidate_for_username(&state, "alice").await;

        assert!(load_auth(&state, "alice", "password-a").await.is_none());
        assert_eq!(
            load_auth(&state, "bob", "password-a")
                .await
                .map(|value| value.account_id),
            Some(2)
        );
    }
}
