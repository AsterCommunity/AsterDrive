//! 认证服务子模块：`tokens`。

use chrono::{Duration as ChronoDuration, Utc};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use sea_orm::{ActiveValue::Set, ConnectionTrait};
use serde::Serialize;

use crate::config::auth_runtime::RuntimeAuthPolicy;
use crate::db::repository::{auth_session_repo, user_repo};
use crate::entities::{auth_session, user};
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::runtime::PrimaryAppState;
use crate::services::audit_service::{self, AuditContext};
use crate::types::TokenType;
use crate::utils::numbers::{i64_to_u64, u64_to_i64, u64_to_usize};

use super::session::{
    get_auth_snapshot, invalidate_auth_snapshot_cache, purge_all_auth_sessions_in_connection,
};
use super::{AuthSnapshot, Claims};

const REFRESH_REUSE_GRACE_SECS: i64 = 15;

#[derive(Debug)]
struct IssuedTokens {
    access_token: String,
    refresh_token: String,
    session_id: String,
    refresh_jti: String,
    refresh_expires_at: chrono::DateTime<Utc>,
}

#[derive(Debug)]
enum RefreshRotationError {
    Aster(AsterError),
    StaleRefresh { user_id: i64, reused_jti: String },
    ReuseDetected { user_id: i64, reused_jti: String },
}

#[derive(Serialize)]
struct RefreshTokenReuseAuditDetails<'a> {
    reused_jti: &'a str,
}

impl From<AsterError> for RefreshRotationError {
    fn from(value: AsterError) -> Self {
        Self::Aster(value)
    }
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

fn is_recent_refresh_rotation(session: &auth_session::Model, now: chrono::DateTime<Utc>) -> bool {
    now.signed_duration_since(session.last_seen_at)
        .num_seconds()
        .abs()
        <= REFRESH_REUSE_GRACE_SECS
}

async fn authenticate_token(
    state: &PrimaryAppState,
    token: &str,
    expected_type: TokenType,
) -> Result<(Claims, AuthSnapshot)> {
    tracing::debug!(
        expected_type = expected_type.as_str(),
        "authenticating token"
    );
    let claims = verify_token(token, &state.config.auth.jwt_secret)?;
    ensure_token_type(&claims, expected_type)?;

    let snapshot = get_auth_snapshot(state, claims.user_id).await?;
    if !snapshot.status.is_active() {
        return Err(AsterError::auth_forbidden("account is disabled"));
    }
    ensure_session_current(&claims, snapshot)?;

    tracing::debug!(
        user_id = claims.user_id,
        expected_type = expected_type.as_str(),
        session_version = snapshot.session_version,
        "authenticated token"
    );

    Ok((claims, snapshot))
}

pub async fn authenticate_access_token(
    state: &PrimaryAppState,
    token: &str,
) -> Result<(Claims, AuthSnapshot)> {
    authenticate_token(state, token, TokenType::Access).await
}

pub async fn authenticate_refresh_token(
    state: &PrimaryAppState,
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
) -> Result<IssuedTokens> {
    let access = create_token(
        user_id,
        session_version,
        TokenType::Access,
        auth_policy.access_token_ttl_secs,
        jwt_secret,
        None,
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

fn issue_tokens_for_session_id(
    state: &PrimaryAppState,
    user_id: i64,
    session_version: i64,
    session_id: Option<&str>,
) -> Result<IssuedTokens> {
    let auth_policy = RuntimeAuthPolicy::from_runtime_config(&state.runtime_config);
    issue_tokens(
        user_id,
        session_version,
        &state.config.auth.jwt_secret,
        auth_policy,
        session_id,
    )
}

pub async fn issue_tokens_for_session(
    state: &PrimaryAppState,
    user_id: i64,
    session_version: i64,
    ip_address: Option<&str>,
    user_agent: Option<&str>,
) -> Result<(String, String)> {
    let tokens = issue_tokens_for_session_id(state, user_id, session_version, None)?;
    persist_auth_session(&state.db, user_id, &tokens, ip_address, user_agent).await?;
    Ok((tokens.access_token, tokens.refresh_token))
}

pub async fn issue_tokens_for_user(
    state: &PrimaryAppState,
    user: &user::Model,
    ip_address: Option<&str>,
    user_agent: Option<&str>,
) -> Result<(String, String)> {
    issue_tokens_for_session(state, user.id, user.session_version, ip_address, user_agent).await
}

pub async fn refresh_tokens(
    state: &PrimaryAppState,
    refresh: &str,
    ip_address: Option<&str>,
    user_agent: Option<&str>,
) -> Result<(String, String)> {
    tracing::debug!("refreshing auth tokens");
    let claims = verify_token(refresh, &state.config.auth.jwt_secret)?;
    ensure_token_type(&claims, TokenType::Refresh)?;
    let refresh_jti = claims
        .jti
        .clone()
        .ok_or_else(|| AsterError::auth_token_invalid("refresh token missing jti"))?;

    let txn = crate::db::transaction::begin(&state.db).await?;
    let rotation = async {
        let now = Utc::now();
        let existing_auth_session = auth_session_repo::find_by_refresh_jti(&txn, &refresh_jti)
            .await
            .map_err(RefreshRotationError::from)?;
        let reused_auth_session = if existing_auth_session.is_none() {
            auth_session_repo::find_by_previous_refresh_jti(&txn, &refresh_jti)
                .await
                .map_err(RefreshRotationError::from)?
        } else {
            None
        };
        let user = user_repo::find_by_id(&txn, claims.user_id)
            .await
            .map_err(RefreshRotationError::from)?;
        if !user.status.is_active() {
            return Err(RefreshRotationError::from(AsterError::auth_forbidden(
                "account is disabled",
            )));
        }
        if claims.session_version != user.session_version {
            return Err(RefreshRotationError::from(AsterError::auth_token_invalid(
                "session revoked",
            )));
        }

        let Some(existing_auth_session) = existing_auth_session else {
            if reused_auth_session.as_ref().is_some_and(|session| {
                session.user_id == claims.user_id && session.revoked_at.is_none()
            }) {
                if reused_auth_session
                    .as_ref()
                    .is_some_and(|session| is_recent_refresh_rotation(session, now))
                {
                    return Err(RefreshRotationError::StaleRefresh {
                        user_id: claims.user_id,
                        reused_jti: refresh_jti,
                    });
                }
                user_repo::bump_session_version(&txn, claims.user_id)
                    .await
                    .map_err(RefreshRotationError::from)?;
                purge_all_auth_sessions_in_connection(&txn, claims.user_id)
                    .await
                    .map_err(RefreshRotationError::from)?;
                return Err(RefreshRotationError::ReuseDetected {
                    user_id: claims.user_id,
                    reused_jti: refresh_jti,
                });
            }
            return Err(RefreshRotationError::from(AsterError::auth_token_invalid(
                "session revoked",
            )));
        };

        if existing_auth_session.user_id != claims.user_id {
            return Err(RefreshRotationError::from(AsterError::auth_token_invalid(
                "invalid token",
            )));
        }
        if existing_auth_session.revoked_at.is_some() {
            return Err(RefreshRotationError::from(AsterError::auth_token_invalid(
                "session revoked",
            )));
        }

        let next_ip_address = ip_address.or(existing_auth_session.ip_address.as_deref());
        let next_user_agent = user_agent.or(existing_auth_session.user_agent.as_deref());
        let tokens = issue_tokens_for_session_id(
            state,
            user.id,
            user.session_version,
            Some(existing_auth_session.id.as_str()),
        )
        .map_err(RefreshRotationError::from)?;

        if !auth_session_repo::rotate_refresh(
            &txn,
            &refresh_jti,
            &tokens.refresh_jti,
            tokens.refresh_expires_at,
            next_ip_address,
            next_user_agent,
            now,
        )
        .await
        .map_err(RefreshRotationError::from)?
        {
            return Err(RefreshRotationError::StaleRefresh {
                user_id: claims.user_id,
                reused_jti: refresh_jti,
            });
        }

        Ok::<_, RefreshRotationError>((
            (tokens.access_token, tokens.refresh_token),
            user.session_version,
        ))
    }
    .await;

    match rotation {
        Ok((tokens, session_version)) => {
            crate::db::transaction::commit(txn).await?;
            tracing::debug!(
                user_id = claims.user_id,
                session_version,
                "refreshed auth tokens"
            );
            Ok(tokens)
        }
        Err(RefreshRotationError::StaleRefresh {
            user_id,
            reused_jti,
        }) => {
            crate::db::transaction::rollback(txn).await?;
            tracing::debug!(
                user_id,
                reused_jti,
                "stale refresh token reused within rotation grace window"
            );
            Err(AsterError::auth_token_invalid("stale refresh token"))
        }
        Err(RefreshRotationError::ReuseDetected {
            user_id,
            reused_jti,
        }) => {
            crate::db::transaction::commit(txn).await?;
            invalidate_auth_snapshot_cache(state, user_id).await;
            tracing::warn!(
                user_id,
                reused_jti,
                "refresh token reuse detected; revoked all sessions"
            );
            audit_service::log(
                state,
                &AuditContext {
                    user_id,
                    ip_address: None,
                    user_agent: None,
                },
                audit_service::AuditAction::UserRefreshTokenReuseDetected,
                crate::services::audit_service::AuditEntityType::User,
                Some(user_id),
                None,
                audit_service::details(RefreshTokenReuseAuditDetails {
                    reused_jti: &reused_jti,
                }),
            )
            .await;
            Err(AsterError::auth_token_invalid(
                "refresh token reuse detected",
            ))
        }
        Err(RefreshRotationError::Aster(error)) => {
            crate::db::transaction::rollback(txn).await?;
            Err(error)
        }
    }
}

pub async fn revoke_refresh_token(state: &PrimaryAppState, token: &str) -> Result<bool> {
    let claims = match verify_token(token, &state.config.auth.jwt_secret) {
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
    auth_session_repo::revoke_by_refresh_jti(&state.db, &jti, Utc::now()).await
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
        create_token(1, 1, token_type, ttl_secs, secret, jti)
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
