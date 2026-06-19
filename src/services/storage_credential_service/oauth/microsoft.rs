use base64::Engine as _;
use rand::RngExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::entities::storage_policy_authorization_flow;
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::storage::error::{StorageErrorKind, storage_driver_error};
use crate::types::{MicrosoftGraphCloud, StorageCredentialProvider};
use crate::utils::OUTBOUND_HTTP_USER_AGENT;

use super::super::{REDACTED_SECRET, crypto};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct MicrosoftGraphFlowContext {
    pub(super) cloud: MicrosoftGraphCloud,
    pub(super) tenant: String,
    pub(super) client_id: String,
    pub(super) client_secret_ciphertext: Option<String>,
    pub(super) scopes: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct MicrosoftTokenResponse {
    pub(super) access_token: String,
    #[serde(default)]
    pub(super) refresh_token: Option<String>,
    #[serde(default)]
    pub(super) token_type: Option<String>,
    #[serde(default)]
    pub(super) expires_in: Option<i64>,
    #[serde(default)]
    pub(super) scope: Option<String>,
    #[serde(default)]
    pub(super) id_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MicrosoftTokenError {
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    error_description: Option<String>,
}

pub(crate) struct StorageCredentialMetadataInput<'a> {
    pub(crate) encryption_key: &'a str,
    pub(crate) policy_id: i64,
    pub(crate) cloud: MicrosoftGraphCloud,
    pub(crate) drive_id: &'a str,
    pub(crate) root_item_id: &'a str,
    pub(crate) root_item_name: Option<&'a str>,
    pub(crate) id_token: Option<&'a str>,
}

pub(crate) fn storage_credential_metadata(
    input: StorageCredentialMetadataInput<'_>,
) -> Result<String> {
    let mut metadata = serde_json::json!({
        "cloud": input.cloud,
        "graph_base_url": input.cloud.graph_base_url(),
        "drive_id": input.drive_id,
        "root_item_id": input.root_item_id,
    });
    let _ = (input.encryption_key, input.policy_id);
    if let Some(root_item_name) = input.root_item_name {
        metadata["root_item_name"] = serde_json::Value::String(root_item_name.to_string());
    }
    if input.id_token.is_some() {
        metadata["id_token"] = serde_json::Value::String(REDACTED_SECRET.to_string());
    }
    serde_json::to_string(&metadata).map_aster_err_ctx(
        "failed to serialize storage credential metadata",
        AsterError::internal_error,
    )
}

pub(super) fn flow_client_secret_aad(policy_id: i64, state_hash: &str) -> String {
    format!("storage_policy_authorization_flow:{policy_id}:{state_hash}:client_secret")
}

fn application_client_secret_aad(policy_id: i64) -> String {
    format!("storage_connector_application_config:{policy_id}:microsoft_graph:client_secret")
}

pub(super) fn encrypt_application_client_secret(
    encryption_key: &str,
    policy_id: i64,
    client_secret: &str,
) -> Result<String> {
    crypto::encrypt_token(
        encryption_key,
        application_client_secret_aad(policy_id).as_bytes(),
        client_secret,
    )
}

pub(super) fn decrypt_application_client_secret(
    encryption_key: &str,
    policy_id: i64,
    ciphertext: &str,
) -> Result<String> {
    crypto::decrypt_token(
        encryption_key,
        application_client_secret_aad(policy_id).as_bytes(),
        ciphertext,
    )
}

pub(super) fn parse_metadata(value: &str) -> Option<serde_json::Value> {
    serde_json::from_str(value).ok()
}

pub(super) fn metadata_cloud(metadata: &serde_json::Value) -> Option<MicrosoftGraphCloud> {
    serde_json::from_value(metadata.get("cloud")?.clone()).ok()
}

pub(super) fn microsoft_graph_flow_cloud(
    flow: &storage_policy_authorization_flow::Model,
) -> Option<MicrosoftGraphCloud> {
    if flow.provider != StorageCredentialProvider::MicrosoftGraph {
        return None;
    }
    serde_json::from_str::<MicrosoftGraphFlowContext>(&flow.context)
        .ok()
        .map(|context| context.cloud)
}

pub(super) fn microsoft_graph_flow_tenant(
    flow: &storage_policy_authorization_flow::Model,
) -> Option<String> {
    if flow.provider != StorageCredentialProvider::MicrosoftGraph {
        return None;
    }
    serde_json::from_str::<MicrosoftGraphFlowContext>(&flow.context)
        .ok()
        .map(|context| context.tenant)
}

pub(super) async fn exchange_microsoft_graph_code(
    context: &MicrosoftGraphFlowContext,
    client_secret: Option<&str>,
    code: &str,
    redirect_uri: &str,
    pkce_verifier: &str,
) -> Result<MicrosoftTokenResponse> {
    let client = reqwest::ClientBuilder::new()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(15))
        .user_agent(OUTBOUND_HTTP_USER_AGENT)
        .build()
        .map_aster_err_ctx(
            "failed to build Microsoft Graph OAuth HTTP client",
            AsterError::internal_error,
        )?;
    let token_endpoint = context.cloud.token_endpoint(&context.tenant);
    let mut form = url::form_urlencoded::Serializer::new(String::new());
    form.append_pair("grant_type", "authorization_code");
    form.append_pair("client_id", &context.client_id);
    form.append_pair("code", code);
    form.append_pair("redirect_uri", redirect_uri);
    form.append_pair("code_verifier", pkce_verifier);
    if let Some(client_secret) = client_secret {
        form.append_pair("client_secret", client_secret);
    }
    let body = form.finish();
    let response = client
        .post(&token_endpoint)
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .body(body)
        .send()
        .await
        .map_aster_err_ctx(
            "Microsoft Graph OAuth token exchange failed",
            AsterError::auth_invalid_credentials,
        )?;
    if !response.status().is_success() {
        return Err(microsoft_token_endpoint_error(response).await);
    }
    let token = response
        .json::<MicrosoftTokenResponse>()
        .await
        .map_aster_err_ctx(
            "Microsoft Graph OAuth token response is invalid",
            AsterError::auth_invalid_credentials,
        )?;
    validate_microsoft_token_response(&token)?;
    Ok(token)
}

pub(super) async fn refresh_microsoft_graph_token(
    cloud: MicrosoftGraphCloud,
    tenant: &str,
    client_id: &str,
    client_secret: Option<&str>,
    refresh_token: &str,
) -> Result<MicrosoftTokenResponse> {
    let client = reqwest::ClientBuilder::new()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(15))
        .user_agent(OUTBOUND_HTTP_USER_AGENT)
        .build()
        .map_aster_err_ctx(
            "failed to build Microsoft Graph OAuth HTTP client",
            AsterError::internal_error,
        )?;
    let token_endpoint = cloud.token_endpoint(tenant);
    let body = {
        let mut form = url::form_urlencoded::Serializer::new(String::new());
        form.append_pair("grant_type", "refresh_token");
        form.append_pair("client_id", client_id);
        form.append_pair("refresh_token", refresh_token);
        if let Some(client_secret) = client_secret {
            form.append_pair("client_secret", client_secret);
        }
        form.finish()
    };
    let response = client
        .post(&token_endpoint)
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .body(body)
        .send()
        .await
        .map_err(|err| {
            storage_driver_error(
                StorageErrorKind::Transient,
                format!("Microsoft Graph OAuth token refresh failed: {err}"),
            )
        })?;
    if !response.status().is_success() {
        return Err(microsoft_refresh_token_endpoint_error(response).await);
    }
    let token = response
        .json::<MicrosoftTokenResponse>()
        .await
        .map_aster_err_ctx(
            "Microsoft Graph OAuth token refresh response is invalid",
            AsterError::auth_invalid_credentials,
        )?;
    validate_microsoft_token_response(&token)?;
    Ok(token)
}

pub(super) fn validate_microsoft_token_response(token: &MicrosoftTokenResponse) -> Result<()> {
    if token.access_token.trim().is_empty() {
        return Err(AsterError::auth_invalid_credentials(
            "Microsoft Graph OAuth token response missing access_token",
        ));
    }
    if let Some(token_type) = token.token_type.as_deref()
        && !token_type.eq_ignore_ascii_case("bearer")
    {
        return Err(AsterError::auth_invalid_credentials(
            "Microsoft Graph OAuth token response returned unsupported token_type",
        ));
    }
    Ok(())
}

async fn microsoft_token_endpoint_error(response: reqwest::Response) -> AsterError {
    let status = response.status();
    let parsed = response.json::<MicrosoftTokenError>().await.ok();
    let message = parsed
        .and_then(|body| body.error_description.or(body.error))
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| format!("HTTP {status}"));
    AsterError::auth_invalid_credentials(format!(
        "Microsoft Graph OAuth token exchange failed: {message}"
    ))
}

async fn microsoft_refresh_token_endpoint_error(response: reqwest::Response) -> AsterError {
    let status = response.status();
    let parsed = response.json::<MicrosoftTokenError>().await.ok();
    let message = parsed
        .and_then(|body| body.error_description.or(body.error))
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| format!("HTTP {status}"));
    let kind = if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        StorageErrorKind::RateLimited
    } else if status == reqwest::StatusCode::UNAUTHORIZED {
        StorageErrorKind::Auth
    } else if status == reqwest::StatusCode::FORBIDDEN {
        StorageErrorKind::Permission
    } else if status.is_server_error() {
        StorageErrorKind::Transient
    } else {
        StorageErrorKind::Auth
    };
    storage_driver_error(
        kind,
        format!("Microsoft Graph OAuth token refresh failed: {message}"),
    )
}

pub(super) fn microsoft_authorization_url(
    cloud: MicrosoftGraphCloud,
    tenant: &str,
    client_id: &str,
    redirect_uri: &str,
    scopes: &[String],
    state: &str,
    pkce_challenge: &str,
) -> Result<String> {
    let mut url = url::Url::parse(&cloud.authorization_endpoint(tenant)).map_aster_err_ctx(
        "invalid Microsoft Graph authorization endpoint",
        AsterError::config_error,
    )?;
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("response_type", "code");
        query.append_pair("client_id", client_id);
        query.append_pair("redirect_uri", redirect_uri);
        query.append_pair("scope", &scopes.join(" "));
        query.append_pair("state", state);
        query.append_pair("code_challenge", pkce_challenge);
        query.append_pair("code_challenge_method", "S256");
    }
    Ok(url.to_string())
}

pub(super) fn build_pkce_verifier() -> String {
    let mut bytes = [0_u8; 32];
    let mut rng = rand::rng();
    rng.fill(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

pub(super) fn build_pkce_challenge(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest)
}
