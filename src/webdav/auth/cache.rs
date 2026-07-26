//! WebDAV 认证缓存。

use crate::runtime::SharedRuntimeState;
use aster_forge_cache::CacheExt;
use aster_forge_crypto as hash;
use sha2::{Digest, Sha256};

use super::CachedWebdavAuth;

const WEBDAV_AUTH_CACHE_TTL: u64 = 60;

pub(super) fn username_cache_component(username: &str) -> String {
    hash::sha256_hex(username.as_bytes())
}

fn credential_cache_component(
    cache_secret: &str,
    username: &str,
    password: &str,
) -> hash::Result<String> {
    let username_digest = Sha256::digest(username.as_bytes());
    let password_digest = Sha256::digest(password.as_bytes());
    let mut credential_material = Vec::with_capacity(
        b"asterdrive:webdav-auth-cache:v1\0".len() + username_digest.len() + password_digest.len(),
    );
    credential_material.extend_from_slice(b"asterdrive:webdav-auth-cache:v1\0");
    credential_material.extend_from_slice(&username_digest);
    credential_material.extend_from_slice(&password_digest);

    hash::hmac_sha256_hex(cache_secret.as_bytes(), &credential_material)
}

fn auth_cache_prefix(username: &str) -> String {
    format!("webdav_auth:{}:", username_cache_component(username))
}

fn auth_cache_key(cache_secret: &str, username: &str, password: &str) -> hash::Result<String> {
    Ok(format!(
        "{}{}",
        auth_cache_prefix(username),
        credential_cache_component(cache_secret, username, password)?
    ))
}

pub(super) async fn load_auth(
    state: &impl SharedRuntimeState,
    username: &str,
    password: &str,
) -> hash::Result<Option<CachedWebdavAuth>> {
    Ok(state
        .cache()
        .get::<CachedWebdavAuth>(&auth_cache_key(
            &state.config().auth.webdav_auth_cache_secret,
            username,
            password,
        )?)
        .await)
}

pub(super) async fn store_auth(
    state: &impl SharedRuntimeState,
    username: &str,
    password: &str,
    cached: &CachedWebdavAuth,
) -> hash::Result<()> {
    state
        .cache()
        .set(
            &auth_cache_key(
                &state.config().auth.webdav_auth_cache_secret,
                username,
                password,
            )?,
            cached,
            Some(WEBDAV_AUTH_CACHE_TTL),
        )
        .await;
    Ok(())
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
        let key = auth_cache_key("cache-secret", "webdav-user", "secret-password")
            .expect("WebDAV auth cache key should be generated");
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
        let key = auth_cache_key("cache-secret-a", "alice", "password")
            .expect("WebDAV auth cache key should be generated");

        assert_ne!(
            key,
            auth_cache_key("cache-secret-b", "alice", "password")
                .expect("WebDAV auth cache key should be generated")
        );
        assert_ne!(
            key,
            auth_cache_key("cache-secret-a", "bob", "password")
                .expect("WebDAV auth cache key should be generated")
        );
    }

    #[tokio::test]
    async fn auth_cache_is_scoped_by_username_and_password() {
        let state = CacheOnlyState::new().await;

        store_auth(&state, "alice", "password-a", &cached(1))
            .await
            .expect("WebDAV auth cache entry should be stored");
        store_auth(&state, "alice", "password-b", &cached(2))
            .await
            .expect("WebDAV auth cache entry should be stored");
        store_auth(&state, "bob", "password-a", &cached(3))
            .await
            .expect("WebDAV auth cache entry should be stored");

        assert_eq!(
            load_auth(&state, "alice", "password-a")
                .await
                .expect("WebDAV auth cache key should be generated")
                .map(|value| value.account_id),
            Some(1)
        );
        assert_eq!(
            load_auth(&state, "alice", "password-b")
                .await
                .expect("WebDAV auth cache key should be generated")
                .map(|value| value.account_id),
            Some(2)
        );
        assert_eq!(
            load_auth(&state, "bob", "password-a")
                .await
                .expect("WebDAV auth cache key should be generated")
                .map(|value| value.account_id),
            Some(3)
        );
    }

    #[tokio::test]
    async fn username_invalidation_keeps_other_user_cache_entries() {
        let state = CacheOnlyState::new().await;

        store_auth(&state, "alice", "password-a", &cached(1))
            .await
            .expect("WebDAV auth cache entry should be stored");
        store_auth(&state, "bob", "password-a", &cached(2))
            .await
            .expect("WebDAV auth cache entry should be stored");

        invalidate_for_username(&state, "alice").await;

        assert!(
            load_auth(&state, "alice", "password-a")
                .await
                .expect("WebDAV auth cache key should be generated")
                .is_none()
        );
        assert_eq!(
            load_auth(&state, "bob", "password-a")
                .await
                .expect("WebDAV auth cache key should be generated")
                .map(|value| value.account_id),
            Some(2)
        );
    }
}
