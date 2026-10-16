//! 认证服务子模块：`tokens`。

use aster_forge_auth::{
    AuthError as ForgeAuthError, AuthTokenType as ForgeTokenType, JwtKey, JwtPolicy, JwtSigner,
    JwtVerifier, Restriction as ForgeRestriction, SessionId as ForgeSessionId,
    SessionVersion as ForgeSessionVersion, Subject as ForgeSubject, TokenClaims,
};
use chrono::Utc;
use sea_orm::{ActiveValue::Set, ConnectionTrait};

use crate::config::auth_runtime::RuntimeAuthPolicy;
use crate::db::repository::auth_session_repo;
use crate::errors::{AsterError, Result};
use crate::runtime::SharedRuntimeState;
use aster_drive_model::entities::{auth_session, user};
use aster_drive_model::types::TokenType;

use super::{AuthSnapshot, Claims};

mod refresh;

pub use refresh::refresh_tokens;

struct IssuedTokens {
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

fn ensure_password_change_scope(claims: &Claims, password_change_required: bool) -> Result<()> {
    if password_change_required && !claims.password_change {
        return Err(AsterError::auth_token_invalid("password change required"));
    }
    if !password_change_required && claims.password_change {
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
    let expected = forge_token_type(expected_type);
    let claims = auth_verifier(
        state.config().auth.jwt_secret.as_bytes(),
        auth_policy(state)?,
    )?
    .verify(token, expected)
    .map_err(map_forge_auth_error)?
    .into_claims();
    let claims = product_claims(claims)?;

    // Authorization uses writer state, never an unfenced cache/replica snapshot. Cache eviction
    // may fail or an old cache fill may arrive after revocation; neither can restore permission.
    let snapshot = match super::get_current_auth_snapshot(state, claims.user_id).await {
        Ok(snapshot) => snapshot,
        Err(AsterError::RecordNotFound(_)) => {
            return Err(AsterError::auth_token_invalid("account unavailable"));
        }
        Err(error) => return Err(error),
    };
    if !snapshot.status.is_active() {
        return Err(AsterError::auth_forbidden("account is disabled"));
    }
    ensure_session_current(&claims, snapshot)?;
    ensure_password_change_scope(
        &claims,
        super::password_change_required_for_snapshot(state, snapshot),
    )?;

    tracing::debug!(
        user_id = claims.user_id,
        expected_type = expected_type.as_str(),
        session_version = snapshot.session_version,
        "authenticated token"
    );

    Ok((claims, snapshot))
}

/// Verifies an access JWT and current writer-side account state, generation and login restriction.
/// Drive retains the access-expiry window for individual session revocation.
///
/// # Errors
/// Returns the product authentication rejection or original writer query failure.
pub async fn authenticate_access_token(
    state: &impl SharedRuntimeState,
    token: &str,
) -> Result<(Claims, AuthSnapshot)> {
    authenticate_token(state, token, TokenType::Access).await
}

/// Verifies refresh purpose and account state for identification. This does not consume the JTI;
/// actual rotation and server-side session checks are performed by `refresh_tokens`.
///
/// # Errors
/// Returns the product authentication rejection or original writer query failure.
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
    let session_id = session_id
        .map(str::to_string)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let policy = signing_policy(auth_policy)?;
    let signer = auth_signer(jwt_secret.as_bytes(), policy)?;
    let pair = signer
        .issue(
            ForgeSubject::new(user_id.to_string())
                .map_err(|_| AsterError::internal_error("invalid user subject"))?,
            ForgeSessionId::new(session_id.clone())
                .map_err(|_| AsterError::internal_error("invalid session id"))?,
            ForgeSessionVersion::new(
                u64::try_from(session_version)
                    .map_err(|_| AsterError::internal_error("invalid session version"))?,
            ),
            if password_change {
                ForgeRestriction::MustChangePassword
            } else {
                ForgeRestriction::None
            },
        )
        .map_err(|_| AsterError::internal_error("failed to issue auth token pair"))?;
    Ok(IssuedTokens {
        access_token: pair.access_token,
        refresh_token: pair.refresh_token,
        session_id,
        refresh_jti: pair.refresh_jti.as_str().to_string(),
        refresh_expires_at: chrono::DateTime::<Utc>::from_timestamp(pair.refresh_expires_at, 0)
            .ok_or_else(|| AsterError::internal_error("invalid refresh expiry"))?,
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

/// Issues and persists a session in the caller's writer transaction using current runtime TTLs.
/// The caller owns account-policy validation and must commit before sending either credential.
///
/// # Errors
/// Returns signing/configuration or persistence errors; an enclosing transaction must roll back.
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

/// Creates an unrestricted session through the writer connection. The caller supplies the current
/// validated account generation; credentials are returned only after session persistence succeeds.
///
/// # Errors
/// Returns signing/configuration or persistence errors without releasing replacement credentials.
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

/// Persists a session for a validated account, projecting its password-change restriction.
///
/// # Errors
/// Returns signing/configuration or writer persistence errors.
pub async fn issue_tokens_for_user(
    state: &impl SharedRuntimeState,
    user: &user::Model,
    ip_address: Option<&str>,
    user_agent: Option<&str>,
) -> Result<(String, String)> {
    issue_tokens_for_user_in_connection(state.writer_db(), state, user, ip_address, user_agent)
        .await
}

/// Persists a restricted login whose allowed actions remain a Drive route-policy responsibility.
///
/// # Errors
/// Returns signing/configuration or writer persistence errors.
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

/// Revokes the current persisted refresh identity. Invalid/expired or already rotated credentials
/// return `false`; this operation does not change the account generation or revoke access early.
///
/// # Errors
/// Returns product signing-policy or writer persistence errors.
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

/// Uses the product's current runtime TTLs for both login and refresh issuance.
fn auth_policy(state: &impl SharedRuntimeState) -> Result<JwtPolicy> {
    signing_policy(RuntimeAuthPolicy::from_runtime_config(
        state.runtime_config(),
    ))
}

/// One strict signing domain shared by all local-web-session entry points.
fn signing_policy(policy: RuntimeAuthPolicy) -> Result<JwtPolicy> {
    JwtPolicy::new(
        "aster-drive",
        "aster-drive-web",
        policy.access_token_ttl_secs,
        policy.refresh_token_ttl_secs,
    )
    .map_err(|_| AsterError::internal_error("invalid auth signing policy"))
}

fn auth_signer(secret: &[u8], policy: JwtPolicy) -> Result<JwtSigner> {
    let key =
        JwtKey::hmac(secret).map_err(|_| AsterError::internal_error("invalid auth signing key"))?;
    Ok(JwtSigner::with_system_clock(key, policy))
}

fn auth_verifier(secret: &[u8], policy: JwtPolicy) -> Result<JwtVerifier> {
    let key =
        JwtKey::hmac(secret).map_err(|_| AsterError::internal_error("invalid auth signing key"))?;
    Ok(JwtVerifier::with_system_clock(key, policy))
}

fn forge_token_type(kind: TokenType) -> ForgeTokenType {
    match kind {
        TokenType::Access => ForgeTokenType::Access,
        TokenType::Refresh => ForgeTokenType::Refresh,
    }
}

/// Validates either local token purpose for logout/session-identification callers. Protected-route
/// authentication instead passes its exact expected purpose directly to the Forge verifier.
///
/// # Errors
/// Rejects old formats, invalid signatures, namespace, identifiers, or time boundaries. No account
/// or permission check is performed here; those remain the caller's product responsibility.
pub fn verify_token(token: &str, secret: &str) -> Result<Claims> {
    let policy = JwtPolicy::new("aster-drive", "aster-drive-web", 1, 1)
        .map_err(|_| AsterError::internal_error("invalid auth verification policy"))?;
    let verifier = auth_verifier(secret.as_bytes(), policy)?;
    let verified = match verifier.verify(token, ForgeTokenType::Access) {
        Err(ForgeAuthError::WrongTokenType) => verifier.verify(token, ForgeTokenType::Refresh),
        other => other,
    }
    .map_err(map_forge_auth_error)?;
    product_claims(verified.into_claims())
}

/// Converts generic verified claims into Drive's typed account projection. There is only one
/// subject source and one restriction source, so duplicate legacy fields cannot disagree.
fn product_claims(claims: TokenClaims) -> Result<Claims> {
    let user_id = claims
        .sub
        .as_str()
        .parse::<i64>()
        .map_err(|_| AsterError::auth_token_invalid("invalid token subject"))?;
    Ok(Claims {
        sid: claims.sid.as_str().to_string(),
        sub: claims.sub.as_str().to_string(),
        user_id,
        session_version: i64::try_from(claims.session_version.value())
            .map_err(|_| AsterError::auth_token_invalid("invalid session version"))?,
        password_change: claims.restriction == ForgeRestriction::MustChangePassword,
        jti: claims.jti.map(|id| id.as_str().to_string()),
        token_type: match claims.token_type {
            ForgeTokenType::Access => TokenType::Access,
            ForgeTokenType::Refresh => TokenType::Refresh,
        },
        exp: usize::try_from(claims.exp)
            .map_err(|_| AsterError::auth_token_invalid("invalid token expiry"))?,
    })
}

/// Stable Drive presentation boundary; mechanism diagnostics never become public error text.
fn map_forge_auth_error(error: ForgeAuthError) -> AsterError {
    match error {
        ForgeAuthError::Expired => AsterError::auth_token_expired("token expired"),
        ForgeAuthError::InvalidPolicy | ForgeAuthError::Encoding => {
            AsterError::internal_error("authentication policy failure")
        }
        ForgeAuthError::ResourceLimit => {
            AsterError::rate_limited("refresh request budget exhausted")
        }
        ForgeAuthError::Cancelled => {
            AsterError::internal_error("authentication runtime is closing")
        }
        _ => AsterError::auth_token_invalid("invalid token"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "test_secret_32bytes_xxxxxxxxxxxxxxxxx";
    fn pair(password_change: bool) -> IssuedTokens {
        issue_tokens(
            1,
            7,
            SECRET,
            RuntimeAuthPolicy {
                cookie_secure: true,
                allow_user_registration: true,
                passkey_login_enabled: true,
                password_login_enabled: true,
                register_activation_enabled: false,
                access_token_ttl_secs: 900,
                refresh_token_ttl_secs: 604800,
            },
            Some("session-1"),
            password_change,
        )
        .unwrap()
    }

    #[test]
    fn issued_pair_uses_runtime_lifetimes_and_one_session_identity() {
        let tokens = pair(false);
        let verifier = auth_verifier(
            SECRET.as_bytes(),
            JwtPolicy::new("aster-drive", "aster-drive-web", 900, 604800).unwrap(),
        )
        .unwrap();
        let access = verifier
            .verify(&tokens.access_token, ForgeTokenType::Access)
            .unwrap()
            .into_claims();
        let refresh = verifier
            .verify(&tokens.refresh_token, ForgeTokenType::Refresh)
            .unwrap()
            .into_claims();
        assert_eq!(access.exp - access.iat, 900);
        assert_eq!(refresh.exp - refresh.iat, 604800);
        assert_eq!(access.sid, refresh.sid);
        assert_eq!(access.sid.as_str(), "session-1");
        assert_eq!(refresh.jti.unwrap().as_str(), tokens.refresh_jti);
        assert_eq!(
            verify_token(&tokens.access_token, SECRET).unwrap().user_id,
            1
        );
    }

    #[test]
    fn restriction_maps_from_one_verified_source_and_purpose_cannot_cross() {
        let tokens = pair(true);
        let claims = verify_token(&tokens.access_token, SECRET).unwrap();
        assert!(claims.password_change);
        assert!(ensure_token_type(&claims, TokenType::Refresh).is_err());
        let snapshot = AuthSnapshot {
            status: aster_drive_model::types::UserStatus::Active,
            role: aster_drive_model::types::UserRole::User,
            session_version: 7,
            must_change_password: true,
        };
        assert!(ensure_session_current(&claims, snapshot).is_ok());
        assert!(
            ensure_session_current(
                &claims,
                AuthSnapshot {
                    session_version: 8,
                    ..snapshot
                }
            )
            .is_err()
        );
        assert!(ensure_password_change_scope(&claims, true).is_ok());
        assert!(ensure_password_change_scope(&claims, false).is_err());
    }

    #[test]
    fn legacy_format_and_wrong_signature_are_rejected() {
        let old = serde_json::json!({"sub":"1","user_id":1,"session_version":7,
            "password_change":false,"token_type":"access","exp":4102444800_u64});
        let old = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &old,
            &jsonwebtoken::EncodingKey::from_secret(SECRET.as_bytes()),
        )
        .unwrap();
        assert!(verify_token(&old, SECRET).is_err());
        assert!(
            verify_token(
                &pair(false).access_token,
                "a_different_secret_32bytes_xxxxxxxxxxx"
            )
            .is_err()
        );
    }
}
