//! 认证服务子模块：`tokens`。

use chrono::{Duration as ChronoDuration, Utc};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use sea_orm::{ActiveValue::Set, ConnectionTrait};

use crate::config::auth_runtime::RuntimeAuthPolicy;
use crate::db::repository::auth_session_repo;
use crate::entities::{auth_session, user};
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::runtime::SharedRuntimeState;
use crate::types::TokenType;
use crate::utils::numbers::{i64_to_u64, u64_to_i64, u64_to_usize};

use super::session::get_auth_snapshot;
use super::{AuthSnapshot, Claims};

mod refresh;

pub use refresh::refresh_tokens;
#[cfg(debug_assertions)]
pub use refresh::test_support;

#[derive(Debug)]
pub struct IssuedTokens {
    access_token: String,
    refresh_token: String,
    session_id: String,
    refresh_jti: String,
    refresh_expires_at: chrono::DateTime<Utc>,
}

fn ensure_token_type(claims: &Claims, expected: TokenType) -> Result<()> {
    if claims.token_type != expected {
        return Err(AsterError::auth_token_invalid(format!(
            "not an {} token",
            expected.as_str()
        )));
    }

    Ok(())
}

fn ensure_session_current(claims: &Claims, snapshot: AuthSnapshot) -> Result<()> {
    if claims.session_version != snapshot.session_version {
        return Err(AsterError::auth_token_invalid("session revoked"));
    }

    Ok(())
}

fn ensure_password_change_scope(claims: &Claims, snapshot: AuthSnapshot) -> Result<()> {
    if snapshot.must_change_password && !claims.password_change {
        return Err(AsterError::auth_token_invalid("password change required"));
    }
    if !snapshot.must_change_password && claims.password_change {
        return Err(AsterError::auth_token_invalid(
            "password change session is no longer valid",
        ));
    }

    Ok(())
}

async fn authenticate_token(
    state: &impl SharedRuntimeState,
    token: &str,
    expected_type: TokenType,
) -> Result<(Claims, AuthSnapshot)> {
    tracing::debug!(
        expected_type = expected_type.as_str(),
        "authenticating token"
    );
    let claims = verify_token(token, &state.config().auth.jwt_secret)?;
    ensure_token_type(&claims, expected_type)?;

    let snapshot = get_auth_snapshot(state, claims.user_id).await?;
    if !snapshot.status.is_active() {
        return Err(AsterError::auth_forbidden("account is disabled"));
    }
    ensure_session_current(&claims, snapshot)?;
    ensure_password_change_scope(&claims, snapshot)?;

    tracing::debug!(
        user_id = claims.user_id,
        expected_type = expected_type.as_str(),
        session_version = snapshot.session_version,
        "authenticated token"
    );

    Ok((claims, snapshot))
}

pub async fn authenticate_access_token(
    state: &impl SharedRuntimeState,
    token: &str,
) -> Result<(Claims, AuthSnapshot)> {
    authenticate_token(state, token, TokenType::Access).await
}

pub async fn authenticate_refresh_token(
    state: &impl SharedRuntimeState,
    token: &str,
) -> Result<(Claims, AuthSnapshot)> {
    authenticate_token(state, token, TokenType::Refresh).await
}

fn issue_tokens(
    user_id: i64,
    session_version: i64,
    jwt_secret: &str,
    auth_policy: RuntimeAuthPolicy,
    session_id: Option<&str>,
    password_change: bool,
) -> Result<IssuedTokens> {
    let access = create_token(
        user_id,
        session_version,
        TokenType::Access,
        auth_policy.access_token_ttl_secs,
        jwt_secret,
        None,
        password_change,
    )?;
    let session_id = session_id
        .map(str::to_string)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let refresh_jti = uuid::Uuid::new_v4().to_string();
    let refresh = create_token(
        user_id,
        session_version,
        TokenType::Refresh,
        auth_policy.refresh_token_ttl_secs,
        jwt_secret,
        Some(refresh_jti.clone()),
        password_change,
    )?;
    Ok(IssuedTokens {
        access_token: access.token,
        refresh_token: refresh.token,
        session_id,
        refresh_jti,
        refresh_expires_at: refresh.expires_at,
    })
}

async fn persist_auth_session<C: ConnectionTrait>(
    db: &C,
    user_id: i64,
    tokens: &IssuedTokens,
    ip_address: Option<&str>,
    user_agent: Option<&str>,
) -> Result<()> {
    let now = Utc::now();
    auth_session_repo::create(
        db,
        auth_session::ActiveModel {
            id: Set(tokens.session_id.clone()),
            user_id: Set(user_id),
            current_refresh_jti: Set(tokens.refresh_jti.clone()),
            previous_refresh_jti: Set(None),
            refresh_expires_at: Set(tokens.refresh_expires_at),
            ip_address: Set(ip_address.map(str::to_string)),
            user_agent: Set(user_agent.map(str::to_string)),
            created_at: Set(now),
            last_seen_at: Set(now),
            revoked_at: Set(None),
        },
    )
    .await?;
    Ok(())
}

pub async fn issue_tokens_for_user_in_connection<C: ConnectionTrait>(
    db: &C,
    state: &impl SharedRuntimeState,
    user: &user::Model,
    ip_address: Option<&str>,
    user_agent: Option<&str>,
) -> Result<(String, String)> {
    let tokens = issue_tokens_for_session_id(
        state,
        user.id,
        user.session_version,
        None,
        user.must_change_password,
    )?;
    persist_auth_session(db, user.id, &tokens, ip_address, user_agent).await?;
    Ok((tokens.access_token, tokens.refresh_token))
}

fn issue_tokens_for_session_id(
    state: &impl SharedRuntimeState,
    user_id: i64,
    session_version: i64,
    session_id: Option<&str>,
    password_change: bool,
) -> Result<IssuedTokens> {
    let auth_policy = RuntimeAuthPolicy::from_runtime_config(state.runtime_config());
    issue_tokens(
        user_id,
        session_version,
        &state.config().auth.jwt_secret,
        auth_policy,
        session_id,
        password_change,
    )
}

pub async fn issue_tokens_for_session(
    state: &impl SharedRuntimeState,
    user_id: i64,
    session_version: i64,
    ip_address: Option<&str>,
    user_agent: Option<&str>,
) -> Result<(String, String)> {
    let tokens = issue_tokens_for_session_id(state, user_id, session_version, None, false)?;
    persist_auth_session(state.writer_db(), user_id, &tokens, ip_address, user_agent).await?;
    Ok((tokens.access_token, tokens.refresh_token))
}

pub async fn issue_tokens_for_user(
    state: &impl SharedRuntimeState,
    user: &user::Model,
    ip_address: Option<&str>,
    user_agent: Option<&str>,
) -> Result<(String, String)> {
    issue_tokens_for_user_in_connection(state.writer_db(), state, user, ip_address, user_agent)
        .await
}

pub async fn issue_password_change_tokens_for_user(
    state: &impl SharedRuntimeState,
    user: &user::Model,
    ip_address: Option<&str>,
    user_agent: Option<&str>,
) -> Result<(String, String)> {
    let tokens = issue_tokens_for_session_id(state, user.id, user.session_version, None, true)?;
    persist_auth_session(state.writer_db(), user.id, &tokens, ip_address, user_agent).await?;
    Ok((tokens.access_token, tokens.refresh_token))
}

pub async fn revoke_refresh_token(state: &impl SharedRuntimeState, token: &str) -> Result<bool> {
    let claims = match verify_token(token, &state.config().auth.jwt_secret) {
        Ok(claims) => claims,
        Err(AsterError::AuthTokenExpired(_) | AsterError::AuthTokenInvalid(_)) => return Ok(false),
        Err(error) => return Err(error),
    };
    if let Err(error) = ensure_token_type(&claims, TokenType::Refresh) {
        if matches!(error, AsterError::AuthTokenInvalid(_)) {
            return Ok(false);
        }
        return Err(error);
    }
    let Some(jti) = claims.jti else {
        return Ok(false);
    };
    auth_session_repo::revoke_by_refresh_jti(state.writer_db(), &jti, Utc::now()).await
}

struct CreatedToken {
    token: String,
    expires_at: chrono::DateTime<Utc>,
}

fn create_token(
    user_id: i64,
    session_version: i64,
    token_type: TokenType,
    ttl_secs: u64,
    secret: &str,
    jti: Option<String>,
    password_change: bool,
) -> Result<CreatedToken> {
    let now = Utc::now();
    let now_secs = i64_to_u64(now.timestamp(), "jwt issued_at unix timestamp")?;
    let exp_secs = now_secs.checked_add(ttl_secs).ok_or_else(|| {
        AsterError::internal_error(format!("jwt exp overflow: {now_secs} + {ttl_secs}"))
    })?;
    let exp = u64_to_usize(exp_secs, "jwt exp")?;
    let expires_at = now
        .checked_add_signed(ChronoDuration::seconds(u64_to_i64(
            ttl_secs,
            "jwt ttl secs",
        )?))
        .ok_or_else(|| AsterError::internal_error("jwt expires_at overflow"))?;
    let claims = Claims {
        sub: user_id.to_string(),
        user_id,
        session_version,
        password_change,
        jti,
        token_type,
        exp,
    };
    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_aster_err(AsterError::internal_error)?;
    Ok(CreatedToken { token, expires_at })
}

pub fn verify_token(token: &str, secret: &str) -> Result<Claims> {
    let data = match decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    ) {
        Ok(data) => data,
        Err(error) => {
            return Err(match error.kind() {
                jsonwebtoken::errors::ErrorKind::ExpiredSignature => {
                    AsterError::auth_token_expired("token expired")
                }
                _ => AsterError::auth_token_invalid("invalid token"),
            });
        }
    };
    Ok(data.claims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::TokenType;

    const SECRET: &str = "test_secret_32bytes_xxxxxxxxxxxxxxxxx";

    fn make_token(
        token_type: TokenType,
        ttl_secs: u64,
        secret: &str,
        jti: Option<String>,
    ) -> String {
        create_token(1, 1, token_type, ttl_secs, secret, jti, false)
            .unwrap()
            .token
    }

    #[test]
    fn verify_access_token_roundtrip() {
        let token = make_token(TokenType::Access, 3600, SECRET, None);
        let claims = verify_token(&token, SECRET).unwrap();
        assert_eq!(claims.user_id, 1);
        assert_eq!(claims.session_version, 1);
        assert_eq!(claims.token_type, TokenType::Access);
        assert!(claims.jti.is_none());
    }

    #[test]
    fn verify_refresh_token_roundtrip() {
        let token = make_token(
            TokenType::Refresh,
            86400,
            SECRET,
            Some("refresh-jti".to_string()),
        );
        let claims = verify_token(&token, SECRET).unwrap();
        assert_eq!(claims.token_type, TokenType::Refresh);
        assert_eq!(claims.jti.as_deref(), Some("refresh-jti"));
    }

    #[test]
    fn verify_token_rejects_wrong_secret() {
        let token = make_token(TokenType::Access, 3600, SECRET, None);
        let err = verify_token(&token, "wrong_secret").unwrap_err();
        // jsonwebtoken 的 InvalidSignature 归类到 "invalid token"
        assert_eq!(err.code(), "E012"); // AuthTokenInvalid
    }

    #[test]
    fn ensure_token_type_access_rejects_refresh() {
        let token = make_token(
            TokenType::Refresh,
            3600,
            SECRET,
            Some("refresh-jti".to_string()),
        );
        let claims = verify_token(&token, SECRET).unwrap();
        let err = ensure_token_type(&claims, TokenType::Access).unwrap_err();
        assert_eq!(err.code(), "E012");
    }

    #[test]
    fn ensure_session_current_rejects_stale_version() {
        let claims = Claims {
            sub: "1".to_string(),
            user_id: 1,
            session_version: 1,
            jti: None,
            token_type: TokenType::Access,
            exp: usize::MAX, // 永不过期，只测 version
        };
        let snapshot = crate::services::auth_service::AuthSnapshot {
            session_version: 2,
            status: crate::types::UserStatus::Active,
            role: crate::types::UserRole::User,
        };
        let err = ensure_session_current(&claims, snapshot).unwrap_err();
        assert_eq!(err.code(), "E012");
    }

    #[test]
    fn ensure_session_current_accepts_matching_version() {
        let claims = Claims {
            sub: "1".to_string(),
            user_id: 1,
            session_version: 1,
            jti: None,
            token_type: TokenType::Access,
            exp: usize::MAX,
        };
        let snapshot = crate::services::auth_service::AuthSnapshot {
            session_version: 1,
            status: crate::types::UserStatus::Active,
            role: crate::types::UserRole::User,
        };
        assert!(ensure_session_current(&claims, snapshot).is_ok());
    }
}
