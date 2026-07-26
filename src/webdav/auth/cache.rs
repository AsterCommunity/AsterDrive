//! WebDAV 认证缓存。

use crate::runtime::SharedRuntimeState;
use aster_forge_cache::CacheExt;
use aster_forge_crypto as hash;
use hmac::{Hmac, KeyInit, Mac};
use sha2::{Digest, Sha256, Sha512};

use super::CachedWebdavAuth;

const WEBDAV_AUTH_CACHE_TTL: u64 = 60;

pub(super) fn username_cache_component(username: &str) -> String {
    hash::sha256_hex(username.as_bytes())
}

fn credential_cache_component(cache_secret: &str, username: &str, password: &str) -> String {
    // HMAC-SHA-256 的固定 key block 是 64 字节。先把配置 secret 归一化为
    // SHA-512 输出，随后走无失败分支的 KeyInit::new，避免在认证热路径里 panic。
    let cache_key = Sha512::digest(cache_secret.as_bytes());
    let mut mac = <Hmac<Sha256> as KeyInit>::new(&cache_key);
    mac.update(b"asterdrive:webdav-auth-cache:v1\0");
    mac.update(&Sha256::digest(username.as_bytes()));
    mac.update(&Sha256::digest(password.as_bytes()));
    hex::encode(mac.finalize().into_bytes())
}

fn auth_cache_prefix(username: &str) -> String {
    format!("webdav_auth:{}:", username_cache_component(username))
}

fn auth_cache_key(cache_secret: &str, username: &str, password: &str) -> String {
    format!(
        "{}{}",
        auth_cache_prefix(username),
        credential_cache_component(cache_secret, username, password)
    )
}

pub(super) async fn load_auth(
    state: &impl SharedRuntimeState,
    username: &str,
    password: &str,
) -> Option<CachedWebdavAuth> {
    state
        .cache()
        .get::<CachedWebdavAuth>(&auth_cache_key(
            &state.config().auth.webdav_auth_cache_secret,
            username,
            password,
        ))
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
            &auth_cache_key(
                &state.config().auth.webdav_auth_cache_secret,
                username,
                password,
            ),
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

    #[test]
    fn auth_cache_key_hides_credentials_behind_server_secret() {
        let key = auth_cache_key("cache-secret", "webdav-user", "secret-password");
        let leaked_sha256_design = format!(
            "{}{}",
            auth_cache_prefix("webdav-user"),
            hash::sha256_hex(b"secret-password")
        );

        assert!(key.starts_with("webdav_auth:"));
        assert!(!key.contains("webdav-user"));
        assert!(!key.contains("secret-password"));
        assert_ne!(key, leaked_sha256_design);
    }

    #[test]
    fn auth_cache_key_is_scoped_by_secret_and_username() {
        let key = auth_cache_key("cache-secret-a", "alice", "password");

        assert_ne!(key, auth_cache_key("cache-secret-b", "alice", "password"));
        assert_ne!(key, auth_cache_key("cache-secret-a", "bob", "password"));
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
