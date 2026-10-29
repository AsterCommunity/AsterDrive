use crate::config::OUTBOUND_HTTP_USER_AGENT;
use crate::errors::{AsterError, MapAsterErr, Result};
use aster_drive_storage::StorageErrorKind;
use base64::Engine as _;
use rand::RngExt;
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use sha2::{Digest, Sha256};

#[derive(Debug, Deserialize)]
pub(super) struct GoogleTokenResponse {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub expires_in: Option<i64>,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub token_type: Option<String>,
}

pub(super) fn flow_client_secret_aad(policy_id: i64, state_hash: &str) -> String {
    format!("storage_policy_authorization_flow:{policy_id}:{state_hash}:client_secret")
}
pub(super) fn build_pkce_verifier() -> String {
    let mut bytes = [0_u8; 32];
    rand::rng().fill(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}
pub(super) fn build_pkce_challenge(verifier: &str) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

pub(super) fn authorization_url(
    client_id: &str,
    redirect_uri: &str,
    scopes: &[String],
    state: &str,
    challenge: &str,
) -> Result<String> {
    let mut url = url::Url::parse("https://accounts.google.com/o/oauth2/v2/auth")
        .map_aster_err_ctx(
            "invalid Google authorization endpoint",
            AsterError::config_error,
        )?;
    {
        let mut q = url.query_pairs_mut();
        q.append_pair("response_type", "code");
        q.append_pair("client_id", client_id);
        q.append_pair("redirect_uri", redirect_uri);
        q.append_pair("scope", &scopes.join(" "));
        q.append_pair("access_type", "offline");
        q.append_pair("prompt", "consent");
        q.append_pair("state", state);
        q.append_pair("code_challenge", challenge);
        q.append_pair("code_challenge_method", "S256");
    }
    Ok(url.to_string())
}

pub(super) async fn exchange_code(
    client_id: &str,
    client_secret: Option<&SecretString>,
    code: &str,
    redirect_uri: &str,
    verifier: &str,
) -> Result<GoogleTokenResponse> {
    let client = reqwest::ClientBuilder::new()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(15))
        .user_agent(OUTBOUND_HTTP_USER_AGENT)
        .build()
        .map_aster_err_ctx(
            "failed to build Google OAuth HTTP client",
            AsterError::internal_error,
        )?;
    let body = {
        let mut form = url::form_urlencoded::Serializer::new(String::new());
        form.append_pair("code", code);
        form.append_pair("client_id", client_id);
        form.append_pair("redirect_uri", redirect_uri);
        form.append_pair("grant_type", "authorization_code");
        form.append_pair("code_verifier", verifier);
        if let Some(secret) = client_secret {
            form.append_pair("client_secret", secret.expose_secret());
        }
        form.finish()
    };
    let response = client
        .post("https://oauth2.googleapis.com/token")
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .body(body)
        .send()
        .await
        .map_aster_err_ctx(
            "Google OAuth token exchange failed",
            AsterError::auth_invalid_credentials,
        )?;
    if !response.status().is_success() {
        return Err(crate::errors::storage_driver_error(
            if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
                StorageErrorKind::RateLimited
            } else if response.status().is_server_error() {
                StorageErrorKind::Transient
            } else {
                StorageErrorKind::Auth
            },
            format!(
                "Google OAuth token exchange returned HTTP {}",
                response.status()
            ),
        ));
    }
    let token = response
        .json::<GoogleTokenResponse>()
        .await
        .map_aster_err_ctx(
            "Google OAuth token response is invalid",
            AsterError::auth_invalid_credentials,
        )?;
    if token.access_token.trim().is_empty()
        || token
            .token_type
            .as_deref()
            .is_some_and(|kind| !kind.eq_ignore_ascii_case("bearer"))
    {
        return Err(AsterError::auth_invalid_credentials(
            "Google OAuth token response is invalid",
        ));
    }
    Ok(token)
}

pub(super) async fn refresh_token(
    client_id: &str,
    client_secret: Option<&SecretString>,
    refresh_token: &str,
) -> Result<GoogleTokenResponse> {
    let client = reqwest::ClientBuilder::new()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent(OUTBOUND_HTTP_USER_AGENT)
        .build()
        .map_aster_err_ctx(
            "failed to build Google OAuth HTTP client",
            AsterError::internal_error,
        )?;
    let body = {
        let mut form = url::form_urlencoded::Serializer::new(String::new());
        form.append_pair("client_id", client_id);
        form.append_pair("refresh_token", refresh_token);
        form.append_pair("grant_type", "refresh_token");
        if let Some(secret) = client_secret {
            form.append_pair("client_secret", secret.expose_secret());
        }
        form.finish()
    };
    let response = client
        .post("https://oauth2.googleapis.com/token")
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .body(body)
        .send()
        .await
        .map_aster_err_ctx(
            "Google OAuth token refresh failed",
            AsterError::auth_invalid_credentials,
        )?;
    if !response.status().is_success() {
        return Err(crate::errors::storage_driver_error(
            if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
                StorageErrorKind::RateLimited
            } else if response.status().is_server_error() {
                StorageErrorKind::Transient
            } else {
                StorageErrorKind::Auth
            },
            format!(
                "Google OAuth token refresh returned HTTP {}",
                response.status()
            ),
        ));
    }
    let token = response
        .json::<GoogleTokenResponse>()
        .await
        .map_aster_err_ctx(
            "Google OAuth refresh response is invalid",
            AsterError::auth_invalid_credentials,
        )?;
    if token.access_token.trim().is_empty() {
        return Err(AsterError::auth_invalid_credentials(
            "Google OAuth refresh response missing access_token",
        ));
    }
    Ok(token)
}
