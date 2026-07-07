//! 分享服务子模块：`access`。

use chrono::Utc;
use std::net::{IpAddr, Ipv6Addr};

use crate::db::repository::{share_repo, user_profile_repo, user_repo};
use crate::entities::share;
use crate::errors::{AsterError, Result};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::user::profile;
use crate::utils::hash;

use super::cache::invalidate_share_token_record_cache_for_share;
use super::models::{SharePublicInfo, SharePublicOwnerInfo};
use super::shared::{
    load_usable_share_ignoring_download_limit, load_valid_share, resolve_share_name,
};

pub async fn get_share_info(
    state: &impl SharedRuntimeState,
    token: &str,
) -> Result<SharePublicInfo> {
    let db = state.writer_db();
    let share = load_valid_share(state, token).await?;
    tracing::debug!(share_id = share.id, "loading public share info");

    match share_repo::increment_view_count(db, share.id).await {
        Ok(()) => invalidate_share_token_record_cache_for_share(state, &share).await,
        Err(error) => {
            tracing::warn!(
                share_id = share.id,
                "failed to increment view count: {error}"
            );
        }
    }

    let (name, share_type, mime_type, size) = resolve_share_name(db, &share).await?;
    let shared_by = resolve_share_owner_info(state, &share).await?;

    let is_expired = share.expires_at.is_some_and(|exp| exp < Utc::now());

    let info = SharePublicInfo {
        token: share.token,
        name,
        share_type,
        has_password: share.password.is_some(),
        expires_at: share.expires_at.map(|e| e.to_rfc3339()),
        is_expired,
        download_count: share.download_count,
        view_count: share.view_count,
        max_downloads: share.max_downloads,
        mime_type,
        size,
        shared_by,
    };
    tracing::debug!(
        share_id = share.id,
        has_password = info.has_password,
        is_expired = info.is_expired,
        download_count = info.download_count,
        view_count = info.view_count,
        "loaded public share info"
    );
    Ok(info)
}

fn resolve_share_owner_name(
    user: &crate::entities::user::Model,
    profile: Option<&crate::entities::user_profile::Model>,
) -> String {
    profile
        .and_then(|p| p.display_name.as_deref())
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| user.username.clone())
}

async fn resolve_share_owner_info(
    state: &impl SharedRuntimeState,
    share: &share::Model,
) -> Result<SharePublicOwnerInfo> {
    let user = user_repo::find_by_id(state.reader_db(), share.user_id).await?;
    let profile = user_profile_repo::find_by_user_id(state.reader_db(), share.user_id).await?;
    let gravatar_base_url = profile::resolve_gravatar_base_url(state);

    Ok(SharePublicOwnerInfo {
        name: resolve_share_owner_name(&user, profile.as_ref()),
        avatar: profile::build_share_public_avatar_info(
            &user,
            profile.as_ref(),
            &share.token,
            &gravatar_base_url,
        ),
    })
}

pub async fn get_share_avatar_bytes(
    state: &PrimaryAppState,
    token: &str,
    size: u32,
) -> Result<Vec<u8>> {
    let share = load_valid_share(state, token).await?;
    profile::get_avatar_bytes(state, share.user_id, size).await
}

pub async fn verify_password(
    state: &impl SharedRuntimeState,
    token: &str,
    password: &str,
) -> Result<()> {
    let share = load_valid_share(state, token).await?;
    tracing::debug!("verifying share password");

    let pw_hash = share
        .password
        .as_deref()
        .ok_or_else(|| AsterError::validation_error("share has no password"))?;

    if !hash::verify_password(password, pw_hash)? {
        return Err(AsterError::auth_invalid_credentials("wrong share password"));
    }

    tracing::debug!("verified share password");
    Ok(())
}

/// 用 HMAC-SHA256 对分享 token 签名作为密码验证 cookie。
///
/// 之前是 `SHA256("share_verified:" + secret + ":" + token)` 的手写拼接，虽然把
/// secret 放在前缀缓解了 length-extension，但这是自己 roll 的 KDF 结构。
/// 换用 `hmac` crate 的 HMAC-SHA256 后语义干净：
/// - 抗 length-extension（HMAC 内置 ipad/opad 双轮）
/// - 验证用 `Mac::verify_slice` 的恒等时间比较，避免侧信道
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShareCookieBinding {
    user_agent_hash: String,
    ip_subnet: String,
}

impl ShareCookieBinding {
    pub fn from_request_parts(user_agent: Option<&str>, ip_address: Option<&str>) -> Self {
        Self {
            user_agent_hash: user_agent
                .map(|value| hash::sha256_hex(value.as_bytes()))
                .unwrap_or_else(|| "none".to_string()),
            ip_subnet: ip_address
                .and_then(|value| value.parse::<IpAddr>().ok())
                .map(ip_subnet)
                .unwrap_or_else(|| "none".to_string()),
        }
    }
}

fn ip_subnet(ip: IpAddr) -> String {
    match ip {
        IpAddr::V4(ip) => {
            let [a, b, c, _] = ip.octets();
            format!("{a}.{b}.{c}.0/24")
        }
        IpAddr::V6(ip) => {
            let mut octets = ip.octets();
            octets[8..].fill(0);
            format!("{}/64", Ipv6Addr::from(octets))
        }
    }
}

#[allow(clippy::expect_used)]
fn share_cookie_mac(
    token: &str,
    binding: &ShareCookieBinding,
    secret: &str,
) -> hmac::Hmac<sha2::Sha256> {
    use hmac::{Hmac, KeyInit, Mac};
    let mut mac = <Hmac<sha2::Sha256> as KeyInit>::new_from_slice(secret.as_bytes())
        .expect("HMAC accepts any key length");
    mac.update(b"share_verified:");
    mac.update(token.as_bytes());
    mac.update(b":ua:");
    mac.update(binding.user_agent_hash.as_bytes());
    mac.update(b":ip:");
    mac.update(binding.ip_subnet.as_bytes());
    mac
}

pub fn sign_share_cookie(token: &str, binding: &ShareCookieBinding, secret: &str) -> String {
    use hmac::Mac;
    let bytes = share_cookie_mac(token, binding, secret)
        .finalize()
        .into_bytes();
    hex::encode(bytes)
}

pub fn verify_share_cookie(
    token: &str,
    cookie_value: &str,
    binding: &ShareCookieBinding,
    secret: &str,
) -> bool {
    use hmac::Mac;
    // hex 解码失败（长度不对、非法字符）一律视为不匹配，
    // 解码成功后用 HMAC 自带的恒等时间 verify_slice 比较。
    let mut decoded = [0u8; 32];
    if hex::decode_to_slice(cookie_value, &mut decoded).is_err() {
        return false;
    }
    share_cookie_mac(token, binding, secret)
        .verify_slice(&decoded)
        .is_ok()
}

pub async fn check_share_password_cookie(
    state: &impl SharedRuntimeState,
    token: &str,
    cookie_value: Option<&str>,
    binding: &ShareCookieBinding,
) -> Result<()> {
    // 使用 load_valid_share 而非 load_share_record，确保验证过期时间和下载次数限制
    let share = load_valid_share(state, token).await?;

    if share.password.is_some() {
        let value = cookie_value
            .ok_or_else(|| AsterError::share_password_required("password verification required"))?;

        if !verify_share_cookie(
            token,
            value,
            binding,
            &state.config().auth.share_cookie_secret,
        ) {
            return Err(AsterError::share_password_required(
                "invalid verification cookie",
            ));
        }
    }
    Ok(())
}

pub(crate) async fn check_share_password_cookie_ignoring_download_limit(
    state: &impl SharedRuntimeState,
    token: &str,
    cookie_value: Option<&str>,
    binding: &ShareCookieBinding,
) -> Result<()> {
    let share = load_usable_share_ignoring_download_limit(state, token).await?;

    if share.password.is_some() {
        let value = cookie_value
            .ok_or_else(|| AsterError::share_password_required("password verification required"))?;

        if !verify_share_cookie(
            token,
            value,
            binding,
            &state.config().auth.share_cookie_secret,
        ) {
            return Err(AsterError::share_password_required(
                "invalid verification cookie",
            ));
        }
    }
    Ok(())
}

pub struct PasswordVerified {
    pub cookie_signature: String,
}

pub async fn verify_password_and_sign(
    state: &impl SharedRuntimeState,
    token: &str,
    password: &str,
    binding: &ShareCookieBinding,
) -> Result<PasswordVerified> {
    verify_password(state, token, password).await?;
    Ok(PasswordVerified {
        cookie_signature: sign_share_cookie(
            token,
            binding,
            &state.config().auth.share_cookie_secret,
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "share_secret_12345";

    fn binding() -> ShareCookieBinding {
        ShareCookieBinding::from_request_parts(Some("AsterDrive Test/1.0"), Some("203.0.113.42"))
    }

    #[test]
    fn sign_verify_share_cookie_roundtrip() {
        let token = "abc123xyz";
        let binding = binding();
        let cookie = sign_share_cookie(token, &binding, SECRET);
        assert!(!cookie.is_empty());
        assert!(verify_share_cookie(token, &cookie, &binding, SECRET));
    }

    #[test]
    fn verify_share_cookie_rejects_wrong_token() {
        let token_a = "token_a";
        let token_b = "token_b";
        let binding = binding();
        let cookie = sign_share_cookie(token_a, &binding, SECRET);
        assert!(!verify_share_cookie(token_b, &cookie, &binding, SECRET));
    }

    #[test]
    fn verify_share_cookie_rejects_wrong_secret() {
        let token = "token";
        let binding = binding();
        let cookie = sign_share_cookie(token, &binding, SECRET);
        assert!(!verify_share_cookie(
            token,
            &cookie,
            &binding,
            "wrong_secret"
        ));
    }

    #[test]
    fn verify_share_cookie_rejects_jwt_secret_when_share_secret_differs() {
        let token = "token";
        let binding = binding();
        let cookie = sign_share_cookie(token, &binding, "dedicated-share-cookie-secret");

        assert!(!verify_share_cookie(token, &cookie, &binding, "jwt-secret"));
    }

    #[test]
    fn verify_share_cookie_rejects_short_value() {
        let token = "token";
        // wrong length
        assert!(!verify_share_cookie(token, "short", &binding(), SECRET));
    }

    #[test]
    fn verify_share_cookie_rejects_non_hex_input() {
        let token = "token";
        // 长度对（64 字符）但含非 hex 字符
        let bad = "z".repeat(64);
        assert!(!verify_share_cookie(token, &bad, &binding(), SECRET));
    }

    #[test]
    fn sign_share_cookie_output_is_64_hex_chars() {
        let cookie = sign_share_cookie("anytoken", &binding(), SECRET);
        assert_eq!(
            cookie.len(),
            64,
            "HMAC-SHA256 hex output is always 64 chars"
        );
        assert!(
            cookie.chars().all(|c| c.is_ascii_hexdigit()),
            "expected pure hex, got '{cookie}'"
        );
    }

    #[test]
    fn verify_share_cookie_rejects_different_client_binding() {
        let token = "token";
        let original =
            ShareCookieBinding::from_request_parts(Some("Browser A"), Some("203.0.113.42"));
        let different_user_agent =
            ShareCookieBinding::from_request_parts(Some("Browser B"), Some("203.0.113.42"));
        let different_ipv4_subnet =
            ShareCookieBinding::from_request_parts(Some("Browser A"), Some("203.0.114.1"));
        let same_ipv4_subnet =
            ShareCookieBinding::from_request_parts(Some("Browser A"), Some("203.0.113.99"));
        let cookie = sign_share_cookie(token, &original, SECRET);

        assert!(!verify_share_cookie(
            token,
            &cookie,
            &different_user_agent,
            SECRET
        ));
        assert!(!verify_share_cookie(
            token,
            &cookie,
            &different_ipv4_subnet,
            SECRET
        ));
        assert!(verify_share_cookie(
            token,
            &cookie,
            &same_ipv4_subnet,
            SECRET
        ));
    }

    #[test]
    fn share_cookie_binding_groups_ipv6_by_64() {
        let original =
            ShareCookieBinding::from_request_parts(Some("Browser"), Some("2001:db8:1:2::1234"));
        let same_subnet =
            ShareCookieBinding::from_request_parts(Some("Browser"), Some("2001:db8:1:2::abcd"));
        let different_subnet =
            ShareCookieBinding::from_request_parts(Some("Browser"), Some("2001:db8:1:3::1"));

        assert_eq!(original, same_subnet);
        assert_ne!(original, different_subnet);
    }

    #[test]
    fn resolve_share_owner_name_prefers_display_name() {
        let user = crate::entities::user::Model {
            id: 1,
            username: "alice".to_string(),
            email: "alice@test.com".to_string(),
            password_hash: String::new(),
            role: crate::types::UserRole::User,
            status: crate::types::UserStatus::Active,
            session_version: 0,
            must_change_password: false,
            email_verified_at: None,
            pending_email: None,
            storage_used: 0,
            storage_quota: 0,
            policy_group_id: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            config: None,
        };
        let profile = crate::entities::user_profile::Model {
            user_id: 1,
            display_name: Some("Alicia".to_string()),
            wopi_user_info: None,
            avatar_source: crate::types::AvatarSource::None,
            avatar_key: None,
            avatar_version: 0,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        let name = resolve_share_owner_name(&user, Some(&profile));
        assert_eq!(name, "Alicia");
    }

    #[test]
    fn resolve_share_owner_name_falls_back_to_username() {
        let user = crate::entities::user::Model {
            id: 1,
            username: "bob".to_string(),
            email: "bob@test.com".to_string(),
            password_hash: String::new(),
            role: crate::types::UserRole::User,
            status: crate::types::UserStatus::Active,
            session_version: 0,
            must_change_password: false,
            email_verified_at: None,
            pending_email: None,
            storage_used: 0,
            storage_quota: 0,
            policy_group_id: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            config: None,
        };
        let name = resolve_share_owner_name(&user, None);
        assert_eq!(name, "bob");
    }

    #[test]
    fn resolve_share_owner_name_skips_empty_display_name() {
        let user = crate::entities::user::Model {
            id: 1,
            username: "charlie".to_string(),
            email: "charlie@test.com".to_string(),
            password_hash: String::new(),
            role: crate::types::UserRole::User,
            status: crate::types::UserStatus::Active,
            session_version: 0,
            must_change_password: false,
            email_verified_at: None,
            pending_email: None,
            storage_used: 0,
            storage_quota: 0,
            policy_group_id: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            config: None,
        };
        let profile = crate::entities::user_profile::Model {
            user_id: 1,
            display_name: Some("   ".to_string()),
            wopi_user_info: None,
            avatar_source: crate::types::AvatarSource::None,
            avatar_key: None,
            avatar_version: 0,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        let name = resolve_share_owner_name(&user, Some(&profile));
        assert_eq!(name, "charlie");
    }
}
