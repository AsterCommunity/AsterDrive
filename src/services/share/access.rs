//! 分享服务子模块：`access`。

use chrono::Utc;
use std::net::{IpAddr, Ipv6Addr};

use crate::db::repository::{folder_repo, share_repo, user_profile_repo, user_repo};
use crate::errors::{AsterError, Result};
use crate::runtime::{PrimaryAppState, SharedRuntimeState};
use crate::services::user::profile;
use aster_drive_model::entities::share;
use aster_forge_crypto as hash;

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
    let folder_icon = match share.folder_id {
        Some(folder_id) => Some(crate::services::files::folder::FolderIcon::from_model(
            &folder_repo::find_by_id(db, folder_id).await?,
        )),
        None => None,
    };
    let shared_by = resolve_share_owner_info(state, &share).await?;

    let is_expired = share.expires_at.is_some_and(|exp| exp < Utc::now());

    let info = SharePublicInfo {
        token: share.token,
        name,
        share_type,
        folder_icon,
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
    user: &aster_drive_model::entities::user::Model,
    profile: Option<&aster_drive_model::entities::user_profile::Model>,
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

/// 分享密码 cookie 的服务端有效时长；签发侧 Cookie max-age 必须与此一致，
/// 服务端校验是权威（客户端 cookie 过期只是兜底）。
pub const SHARE_COOKIE_MAX_AGE_SECS: i64 = 3600;

/// 容忍多实例间时钟小幅不同步：签发时间戳允许领先本地时钟该数值。
const SHARE_COOKIE_CLOCK_SKEW_SECS: i64 = 60;

/// 用 HMAC-SHA256 对分享 token 签名作为密码验证 cookie。
///
/// 之前是 `SHA256("share_verified:" + secret + ":" + token)` 的手写拼接，虽然把
/// secret 放在前缀缓解了 length-extension，但这是自己 roll 的 KDF 结构。
/// 换用 `hmac` crate 的 HMAC-SHA256 后语义干净：
/// - 抗 length-extension（HMAC 内置 ipad/opad 双轮）
/// - 验证用 `Mac::verify_slice` 的恒等时间比较，避免侧信道
///
/// MAC 输入还绑定两个时效/版本因子：
/// - 签发时间戳 `issued_at`：截获的 cookie 值无法永久重放；
/// - 当前分享密码 hash：密码变更后所有已发 cookie 立即失效。
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

#[expect(
    clippy::expect_used,
    reason = "HMAC-SHA256 accepts keys of every length, so construction with secret bytes is infallible"
)]
fn share_cookie_mac(
    token: &str,
    binding: &ShareCookieBinding,
    password_hash: &str,
    issued_at: i64,
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
    mac.update(b":pw:");
    mac.update(password_hash.as_bytes());
    mac.update(b":at:");
    mac.update(issued_at.to_string().as_bytes());
    mac
}

/// 签发分享密码 cookie 值：`<issued_at_unix_secs>.<hex hmac>`。
///
/// `password_hash` 传入分享当前的密码 hash（无密码分享传 None），使密码
/// 变更后已发 cookie 立即失效。
pub fn sign_share_cookie(
    token: &str,
    binding: &ShareCookieBinding,
    password_hash: Option<&str>,
    secret: &str,
) -> String {
    use hmac::Mac;
    let issued_at = Utc::now().timestamp();
    let bytes = share_cookie_mac(
        token,
        binding,
        password_hash.unwrap_or_default(),
        issued_at,
        secret,
    )
    .finalize()
    .into_bytes();
    format!("{issued_at}.{}", hex::encode(bytes))
}

/// 校验分享密码 cookie 值；`now` 由调用方传入以便测试。
pub fn verify_share_cookie(
    token: &str,
    cookie_value: &str,
    binding: &ShareCookieBinding,
    password_hash: Option<&str>,
    now: i64,
    secret: &str,
) -> bool {
    use hmac::Mac;
    // 值格式为 `<issued_at>.<hex hmac>`；旧格式（纯 hex）在此直接解析失败，
    // 升级后存量 cookie 一律要求重新输密码，可接受。
    let Some((issued_at_raw, signature)) = cookie_value.rsplit_once('.') else {
        return false;
    };
    let Ok(issued_at) = issued_at_raw.parse::<i64>() else {
        return false;
    };
    let Some(age) = now.checked_sub(issued_at) else {
        return false;
    };
    if !(-SHARE_COOKIE_CLOCK_SKEW_SECS..SHARE_COOKIE_MAX_AGE_SECS).contains(&age) {
        return false;
    }
    // hex 解码失败（长度不对、非法字符）一律视为不匹配，
    // 解码成功后用 HMAC 自带的恒等时间 verify_slice 比较。
    let mut decoded = [0u8; 32];
    if hex::decode_to_slice(signature, &mut decoded).is_err() {
        return false;
    }
    share_cookie_mac(
        token,
        binding,
        password_hash.unwrap_or_default(),
        issued_at,
        secret,
    )
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
            share.password.as_deref(),
            Utc::now().timestamp(),
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
            share.password.as_deref(),
            Utc::now().timestamp(),
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

/// 验证分享密码；命中 rehash 时升级存储。
/// 返回最终生效的密码 hash（rehash 成功为新 hash，否则为原 hash），
/// 供签发 cookie 时绑定与数据库一致的密码版本。
async fn verify_share_password_effective_hash(
    state: &impl SharedRuntimeState,
    share: &share::Model,
    password: &str,
) -> Result<String> {
    let pw_hash = share
        .password
        .as_deref()
        .ok_or_else(|| AsterError::validation_error("share has no password"))?;

    let verification = state
        .runtime_config()
        .password_hash_runtime()
        .verify_password(password, pw_hash)
        .await?;
    if !verification.is_valid {
        return Err(AsterError::auth_invalid_credentials("wrong share password"));
    }

    if verification.needs_rehash {
        if let Some(new_hash) = upgrade_share_password_hash(state, share, pw_hash, password).await {
            return Ok(new_hash);
        }
    }

    Ok(pw_hash.to_string())
}

pub async fn verify_password(
    state: &impl SharedRuntimeState,
    token: &str,
    password: &str,
) -> Result<()> {
    let share = load_valid_share(state, token).await?;
    tracing::debug!("verifying share password");

    verify_share_password_effective_hash(state, &share, password).await?;

    tracing::debug!("verified share password");
    Ok(())
}

async fn upgrade_share_password_hash(
    state: &impl SharedRuntimeState,
    share: &share::Model,
    current_hash: &str,
    password: &str,
) -> Option<String> {
    let new_hash = match state
        .runtime_config()
        .password_hash_runtime()
        .hash_password(password)
        .await
    {
        Ok(hash) => hash,
        Err(error) => {
            tracing::warn!(
                share_id = share.id,
                "failed to generate upgraded share password hash: {error}"
            );
            return None;
        }
    };

    match share_repo::update_password_hash_if_current(
        state.writer_db(),
        share.id,
        current_hash,
        &new_hash,
    )
    .await
    {
        Ok(true) => {
            invalidate_share_token_record_cache_for_share(state, share).await;
            tracing::info!(share_id = share.id, "upgraded share password hash policy");
            Some(new_hash)
        }
        Ok(false) => {
            tracing::debug!(
                share_id = share.id,
                "skipped share password hash upgrade after concurrent credential change"
            );
            None
        }
        Err(error) => {
            tracing::warn!(
                share_id = share.id,
                "failed to persist upgraded share password hash: {error}"
            );
            None
        }
    }
}

pub async fn verify_password_and_sign(
    state: &impl SharedRuntimeState,
    token: &str,
    password: &str,
    binding: &ShareCookieBinding,
) -> Result<PasswordVerified> {
    let share = load_valid_share(state, token).await?;
    let effective_hash = verify_share_password_effective_hash(state, &share, password).await?;
    Ok(PasswordVerified {
        cookie_signature: sign_share_cookie(
            token,
            binding,
            Some(&effective_hash),
            &state.config().auth.share_cookie_secret,
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "share_secret_12345";
    const PASSWORD_HASH: &str = "$argon2id$v=19$m=65536,t=3,p=4$test$saltvalue";

    fn binding() -> ShareCookieBinding {
        ShareCookieBinding::from_request_parts(Some("AsterDrive Test/1.0"), Some("203.0.113.42"))
    }

    fn sign(token: &str, binding: &ShareCookieBinding) -> String {
        sign_share_cookie(token, binding, Some(PASSWORD_HASH), SECRET)
    }

    fn verify(token: &str, value: &str, binding: &ShareCookieBinding, now: i64) -> bool {
        verify_share_cookie(token, value, binding, Some(PASSWORD_HASH), now, SECRET)
    }

    #[test]
    fn sign_verify_share_cookie_roundtrip() {
        let token = "abc123xyz";
        let binding = binding();
        let cookie = sign(token, &binding);
        assert!(!cookie.is_empty());
        assert!(verify(token, &cookie, &binding, Utc::now().timestamp()));
    }

    #[test]
    fn share_cookie_value_is_timestamp_dot_hex() {
        let cookie = sign("anytoken", &binding());
        let (issued_at, signature) = cookie.split_once('.').expect("value must be ts.sig");
        assert!(
            issued_at.parse::<i64>().is_ok(),
            "issued_at must be unix seconds, got '{issued_at}'"
        );
        assert_eq!(signature.len(), 64, "HMAC-SHA256 hex output is 64 chars");
        assert!(signature.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn verify_share_cookie_rejects_wrong_token() {
        let binding = binding();
        let cookie = sign("token_a", &binding);
        assert!(!verify(
            "token_b",
            &cookie,
            &binding,
            Utc::now().timestamp()
        ));
    }

    #[test]
    fn verify_share_cookie_rejects_wrong_secret() {
        let token = "token";
        let binding = binding();
        let cookie = sign(token, &binding);
        assert!(!verify_share_cookie(
            token,
            &cookie,
            &binding,
            Some(PASSWORD_HASH),
            Utc::now().timestamp(),
            "wrong_secret"
        ));
    }

    #[test]
    fn verify_share_cookie_rejects_jwt_secret_when_share_secret_differs() {
        let token = "token";
        let binding = binding();
        let cookie = sign_share_cookie(
            token,
            &binding,
            Some(PASSWORD_HASH),
            "dedicated-share-cookie-secret",
        );

        assert!(!verify_share_cookie(
            token,
            &cookie,
            &binding,
            Some(PASSWORD_HASH),
            Utc::now().timestamp(),
            "jwt-secret"
        ));
    }

    #[test]
    fn verify_share_cookie_rejects_legacy_plain_hex_value() {
        let token = "token";
        // 旧格式（纯 hex、无时间戳前缀）升级后一律失效
        let legacy = "a".repeat(64);
        assert!(!verify(token, &legacy, &binding(), Utc::now().timestamp()));
        assert!(!verify(token, "short", &binding(), Utc::now().timestamp()));
    }

    #[test]
    fn verify_share_cookie_rejects_non_hex_signature() {
        let token = "token";
        let bad = format!("{}.{}", Utc::now().timestamp(), "z".repeat(64));
        assert!(!verify(token, &bad, &binding(), Utc::now().timestamp()));
    }

    #[test]
    fn verify_share_cookie_rejects_expired_value_and_accepts_boundary() {
        let token = "token";
        let binding = binding();
        let cookie = sign(token, &binding);
        let issued_at: i64 = cookie
            .split_once('.')
            .expect("ts.sig format")
            .0
            .parse()
            .expect("issued_at is unix seconds");

        assert!(verify(
            token,
            &cookie,
            &binding,
            issued_at + SHARE_COOKIE_MAX_AGE_SECS - 1
        ));
        assert!(!verify(
            token,
            &cookie,
            &binding,
            issued_at + SHARE_COOKIE_MAX_AGE_SECS
        ));
        assert!(!verify(token, &cookie, &binding, issued_at + 86_400));
    }

    #[test]
    fn verify_share_cookie_rejects_extreme_timestamps_without_overflow() {
        let token = "token";
        let binding = binding();
        let now = Utc::now().timestamp();
        for timestamp in [i64::MIN, i64::MAX] {
            let malformed = format!("{timestamp}.{}", "0".repeat(64));
            assert!(!verify(token, &malformed, &binding, now));
        }
    }

    #[test]
    fn verify_share_cookie_tolerates_small_clock_skew_but_not_beyond() {
        let token = "token";
        let binding = binding();
        let cookie = sign(token, &binding);
        let issued_at: i64 = cookie.split_once('.').unwrap().0.parse().unwrap();

        assert!(verify(token, &cookie, &binding, issued_at - 30));
        assert!(!verify(
            token,
            &cookie,
            &binding,
            issued_at - SHARE_COOKIE_CLOCK_SKEW_SECS - 1
        ));
    }

    #[test]
    fn verify_share_cookie_rejects_value_signed_with_different_password_hash() {
        let token = "token";
        let binding = binding();
        let cookie = sign_share_cookie(token, &binding, Some("old-password-hash"), SECRET);

        // 分享者改密码后，旧 cookie 必须立即失效
        assert!(!verify(token, &cookie, &binding, Utc::now().timestamp()));
        let new_hash_cookie = sign_share_cookie(token, &binding, Some("new-password-hash"), SECRET);
        assert!(!verify_share_cookie(
            token,
            &new_hash_cookie,
            &binding,
            Some("old-password-hash"),
            Utc::now().timestamp(),
            SECRET
        ));
    }

    #[test]
    fn verify_share_cookie_rejects_tampered_timestamp() {
        let token = "token";
        let binding = binding();
        let cookie = sign(token, &binding);
        let (_, signature) = cookie.split_once('.').unwrap();
        let forged = format!("{}.{signature}", Utc::now().timestamp() - 120);

        // 时间戳改了但签名还是原签名：MAC 覆盖 issued_at，必须拒绝
        assert!(!verify(token, &forged, &binding, Utc::now().timestamp()));
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
        let cookie = sign(token, &original);
        let now = Utc::now().timestamp();

        assert!(!verify(token, &cookie, &different_user_agent, now));
        assert!(!verify(token, &cookie, &different_ipv4_subnet, now));
        assert!(verify(token, &cookie, &same_ipv4_subnet, now));
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
        let user = aster_drive_model::entities::user::Model {
            id: 1,
            username: "alice".to_string(),
            email: "alice@test.com".to_string(),
            password_hash: String::new(),
            role: aster_drive_model::types::UserRole::User,
            status: aster_drive_model::types::UserStatus::Active,
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
        let profile = aster_drive_model::entities::user_profile::Model {
            user_id: 1,
            display_name: Some("Alicia".to_string()),
            wopi_user_info: None,
            avatar_source: aster_drive_model::types::AvatarSource::None,
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
        let user = aster_drive_model::entities::user::Model {
            id: 1,
            username: "bob".to_string(),
            email: "bob@test.com".to_string(),
            password_hash: String::new(),
            role: aster_drive_model::types::UserRole::User,
            status: aster_drive_model::types::UserStatus::Active,
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
        let user = aster_drive_model::entities::user::Model {
            id: 1,
            username: "charlie".to_string(),
            email: "charlie@test.com".to_string(),
            password_hash: String::new(),
            role: aster_drive_model::types::UserRole::User,
            status: aster_drive_model::types::UserStatus::Active,
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
        let profile = aster_drive_model::entities::user_profile::Model {
            user_id: 1,
            display_name: Some("   ".to_string()),
            wopi_user_info: None,
            avatar_source: aster_drive_model::types::AvatarSource::None,
            avatar_key: None,
            avatar_version: 0,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        let name = resolve_share_owner_name(&user, Some(&profile));
        assert_eq!(name, "charlie");
    }
}
