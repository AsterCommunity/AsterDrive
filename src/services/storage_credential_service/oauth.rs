use base64::Engine as _;
use chrono::{Duration, Utc};
use rand::RngExt;
use sea_orm::{ActiveValue::Set, TransactionTrait};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::db::repository::{
    policy_repo, storage_policy_authorization_flow_repo, storage_policy_credential_repo,
};
use crate::entities::{storage_policy_authorization_flow, storage_policy_credential};
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::runtime::SharedRuntimeState;
use crate::storage::drivers::onedrive::{MicrosoftGraphClient, MicrosoftGraphClientConfig};
use crate::types::{
    DriverType, MicrosoftGraphCloud, StorageAuthorizationFlowStatus, StorageCredentialKind,
    StorageCredentialProvider, StorageCredentialStatus, parse_storage_policy_options,
};
use crate::utils::{OUTBOUND_HTTP_USER_AGENT, id};

use super::{
    FLOW_TTL_SECS, MicrosoftGraphAuthorizationContext, MicrosoftGraphAuthorizationInput,
    REDACTED_SECRET, StoragePolicyCredentialInfo, crypto, normalize_optional_string,
    normalize_required_string, normalize_scopes, scopes_to_json,
};

#[derive(Clone, Debug, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct StorageAuthorizationStartInput {
    pub provider: StorageCredentialProvider,
    pub microsoft_graph: Option<MicrosoftGraphAuthorizationInput>,
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct StorageAuthorizationStartResponse {
    pub authorization_url: String,
    pub expires_in: u64,
    pub provider: StorageCredentialProvider,
    pub microsoft_graph: Option<MicrosoftGraphAuthorizationContext>,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    derive(utoipa::IntoParams, utoipa::ToSchema)
)]
pub struct StorageAuthorizationCallbackQuery {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
    pub error_description: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct StorageAuthorizationCallbackOutcome {
    pub credential: StoragePolicyCredentialInfo,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct MicrosoftGraphFlowContext {
    cloud: MicrosoftGraphCloud,
    tenant: String,
    client_id: String,
    client_secret_ciphertext: Option<String>,
    scopes: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct MicrosoftTokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    token_type: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    id_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MicrosoftTokenError {
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    error_description: Option<String>,
}

pub async fn start_authorization(
    state: &impl SharedRuntimeState,
    req: &actix_web::HttpRequest,
    policy_id: i64,
    created_by_user_id: i64,
    input: StorageAuthorizationStartInput,
) -> Result<StorageAuthorizationStartResponse> {
    let policy = policy_repo::find_by_id(state.writer_db(), policy_id).await?;
    if policy.driver_type != DriverType::OneDrive {
        return Err(AsterError::validation_error(
            "storage credential authorization is only supported for OneDrive policies",
        ));
    }
    match input.provider {
        StorageCredentialProvider::MicrosoftGraph => {
            start_microsoft_graph_authorization(
                state,
                req,
                policy_id,
                created_by_user_id,
                input.microsoft_graph,
            )
            .await
        }
        StorageCredentialProvider::GoogleDrive => Err(AsterError::unsupported_driver(
            "Google Drive storage credential authorization is not implemented yet",
        )),
    }
}

async fn start_microsoft_graph_authorization(
    state: &impl SharedRuntimeState,
    req: &actix_web::HttpRequest,
    policy_id: i64,
    created_by_user_id: i64,
    input: Option<MicrosoftGraphAuthorizationInput>,
) -> Result<StorageAuthorizationStartResponse> {
    let input = input.ok_or_else(|| {
        AsterError::validation_error("microsoft_graph authorization parameters are required")
    })?;
    let cloud = input.cloud.unwrap_or_default();
    let tenant = normalize_optional_string(input.tenant).unwrap_or_else(|| "common".to_string());
    let client_id = normalize_required_string(&input.client_id, "client_id", 512)?;
    let client_secret = normalize_optional_string(input.client_secret);
    let scopes = normalize_scopes(input.scopes);
    let redirect_uri = callback_redirect_uri(state, req)?;
    let state_value = format!("storage_oauth_{}", id::new_short_token());
    let pkce_verifier = build_pkce_verifier();
    let pkce_challenge = build_pkce_challenge(&pkce_verifier);
    let authorization_url = microsoft_authorization_url(
        cloud,
        &tenant,
        &client_id,
        &redirect_uri,
        &scopes,
        &state_value,
        &pkce_challenge,
    )?;
    let state_hash = crypto::token_hash(&state_value);
    let client_secret_ciphertext = match client_secret.as_deref() {
        Some(secret) => Some(crypto::encrypt_token(
            &state.config().auth.storage_credential_secret_key,
            flow_client_secret_aad(policy_id, &state_hash).as_bytes(),
            secret,
        )?),
        None => None,
    };
    let context = MicrosoftGraphFlowContext {
        cloud,
        tenant: tenant.clone(),
        client_id: client_id.clone(),
        client_secret_ciphertext,
        scopes: scopes.clone(),
    };
    let now = Utc::now();
    let ttl = crate::utils::numbers::u64_to_i64(FLOW_TTL_SECS, "storage authorization flow ttl")?;
    storage_policy_authorization_flow_repo::cancel_pending_for_policy(
        state.writer_db(),
        policy_id,
        now,
    )
    .await?;
    storage_policy_authorization_flow_repo::create(
        state.writer_db(),
        storage_policy_authorization_flow::ActiveModel {
            provider: Set(StorageCredentialProvider::MicrosoftGraph),
            policy_id: Set(Some(policy_id)),
            created_by_user_id: Set(created_by_user_id),
            state_hash: Set(state_hash),
            pkce_verifier: Set(Some(pkce_verifier)),
            redirect_uri: Set(redirect_uri),
            scopes: Set(scopes_to_json(&scopes)?),
            context: Set(serde_json::to_string(&context).map_aster_err_ctx(
                "failed to serialize Microsoft Graph authorization context",
                AsterError::internal_error,
            )?),
            status: Set(StorageAuthorizationFlowStatus::Pending),
            created_at: Set(now),
            expires_at: Set(now + Duration::seconds(ttl)),
            consumed_at: Set(None),
            ..Default::default()
        },
    )
    .await?;

    Ok(StorageAuthorizationStartResponse {
        authorization_url,
        expires_in: FLOW_TTL_SECS,
        provider: StorageCredentialProvider::MicrosoftGraph,
        microsoft_graph: Some(MicrosoftGraphAuthorizationContext {
            cloud,
            tenant,
            client_id,
            client_secret_configured: client_secret.is_some(),
            scopes,
        }),
    })
}

pub async fn finish_authorization_callback(
    state: &impl SharedRuntimeState,
    query: &StorageAuthorizationCallbackQuery,
) -> Result<StorageAuthorizationCallbackOutcome> {
    if let Some(error) = query.error.as_deref() {
        let description = query
            .error_description
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(error);
        return Err(AsterError::auth_invalid_credentials(format!(
            "storage credential provider returned error: {description}"
        )));
    }
    let code = query.code.as_deref().ok_or_else(|| {
        AsterError::auth_invalid_credentials("storage credential callback missing code")
    })?;
    let state_value = query.state.as_deref().ok_or_else(|| {
        AsterError::auth_invalid_credentials("storage credential callback missing state")
    })?;

    let txn = state.writer_db().begin().await.map_err(AsterError::from)?;
    let now = Utc::now();
    let flow = storage_policy_authorization_flow_repo::consume_by_state_hash(
        &txn,
        &crypto::token_hash(state_value),
        now,
    )
    .await?
    .ok_or_else(|| {
        AsterError::auth_invalid_credentials("storage credential state is invalid or expired")
    })?;
    let credential = match flow.provider {
        StorageCredentialProvider::MicrosoftGraph => {
            finish_microsoft_graph_callback(
                &txn,
                &state.config().auth.storage_credential_secret_key,
                &flow,
                code,
                now,
            )
            .await?
        }
        StorageCredentialProvider::GoogleDrive => {
            return Err(AsterError::unsupported_driver(
                "Google Drive storage credential authorization is not implemented yet",
            ));
        }
    };
    txn.commit().await.map_err(AsterError::from)?;
    state
        .driver_registry()
        .reload_storage_policy_credentials(state.writer_db(), state.config().as_ref())
        .await?;
    Ok(StorageAuthorizationCallbackOutcome {
        credential: credential.into(),
    })
}

async fn finish_microsoft_graph_callback<C: sea_orm::ConnectionTrait>(
    db: &C,
    encryption_key: &str,
    flow: &storage_policy_authorization_flow::Model,
    code: &str,
    now: chrono::DateTime<Utc>,
) -> Result<storage_policy_credential::Model> {
    let policy_id = flow.policy_id.ok_or_else(|| {
        AsterError::database_operation("storage authorization flow missing policy_id")
    })?;
    let context =
        serde_json::from_str::<MicrosoftGraphFlowContext>(&flow.context).map_err(|err| {
            AsterError::database_operation(format!(
                "invalid Microsoft Graph authorization context: {err}"
            ))
        })?;
    let pkce_verifier = flow.pkce_verifier.as_deref().ok_or_else(|| {
        AsterError::database_operation("storage authorization flow missing PKCE verifier")
    })?;
    let client_secret = match context.client_secret_ciphertext.as_deref() {
        Some(ciphertext) => Some(crypto::decrypt_token(
            encryption_key,
            flow_client_secret_aad(policy_id, &flow.state_hash).as_bytes(),
            ciphertext,
        )?),
        None => None,
    };
    let token = exchange_microsoft_graph_code(
        &context,
        client_secret.as_deref(),
        code,
        &flow.redirect_uri,
        pkce_verifier,
    )
    .await?;
    let policy = policy_repo::find_by_id(db, policy_id).await?;
    let options = parse_storage_policy_options(policy.options.as_ref());
    let drive_id = options.onedrive_drive_id.as_deref().ok_or_else(|| {
        AsterError::database_operation("OneDrive storage policy missing onedrive_drive_id")
    })?;
    let root_item_id = options.onedrive_root_item_id.as_deref().ok_or_else(|| {
        AsterError::database_operation("OneDrive storage policy missing onedrive_root_item_id")
    })?;
    let graph_client = MicrosoftGraphClient::new(MicrosoftGraphClientConfig::new(
        context.cloud.graph_base_url(),
        token.access_token.clone(),
    ))?;
    let root_item = graph_client
        .get_drive_item_by_id(drive_id, root_item_id)
        .await?;
    let expires_at = token
        .expires_in
        .and_then(|seconds| (seconds > 0).then(|| now + Duration::seconds(seconds)));
    let granted_scopes = token
        .scope
        .as_deref()
        .map(|scope| {
            normalize_scopes(Some(
                scope.split_whitespace().map(ToOwned::to_owned).collect(),
            ))
        })
        .filter(|scopes| !scopes.is_empty())
        .unwrap_or_else(|| context.scopes.clone());
    let access_aad = crypto::token_aad(
        policy_id,
        StorageCredentialProvider::MicrosoftGraph.as_str(),
        "access",
    );
    let refresh_aad = crypto::token_aad(
        policy_id,
        StorageCredentialProvider::MicrosoftGraph.as_str(),
        "refresh",
    );
    let access_token_ciphertext =
        crypto::encrypt_token(encryption_key, access_aad.as_bytes(), &token.access_token)?;
    let refresh_token_ciphertext = match token.refresh_token.as_deref() {
        Some(refresh_token) if !refresh_token.trim().is_empty() => Some(crypto::encrypt_token(
            encryption_key,
            refresh_aad.as_bytes(),
            refresh_token,
        )?),
        _ => None,
    };
    storage_policy_credential_repo::upsert_by_policy_provider_kind(
        db,
        storage_policy_credential::ActiveModel {
            policy_id: Set(policy_id),
            provider: Set(StorageCredentialProvider::MicrosoftGraph),
            credential_kind: Set(StorageCredentialKind::OauthDelegated),
            account_label: Set(root_item.name.clone()),
            subject: Set(Some(root_item.id.clone())),
            tenant_id: Set(Some(context.tenant.clone())),
            scopes: Set(scopes_to_json(&granted_scopes)?),
            access_token_ciphertext: Set(Some(access_token_ciphertext)),
            refresh_token_ciphertext: Set(refresh_token_ciphertext),
            metadata: Set(storage_credential_metadata(
                &context,
                drive_id,
                root_item_id,
                root_item.name.as_deref(),
                token.id_token.as_deref(),
            )?),
            status: Set(StorageCredentialStatus::Authorized),
            status_reason: Set(None),
            expires_at: Set(expires_at),
            authorized_at: Set(Some(now)),
            last_refreshed_at: Set(None),
            last_validated_at: Set(None),
            ..Default::default()
        },
        now,
    )
    .await
}

fn storage_credential_metadata(
    context: &MicrosoftGraphFlowContext,
    drive_id: &str,
    root_item_id: &str,
    root_item_name: Option<&str>,
    id_token: Option<&str>,
) -> Result<String> {
    let mut metadata = serde_json::json!({
        "cloud": context.cloud,
        "graph_base_url": context.cloud.graph_base_url(),
        "client_id": context.client_id,
        "client_secret_configured": context.client_secret_ciphertext.is_some(),
        "drive_id": drive_id,
        "root_item_id": root_item_id,
    });
    if let Some(root_item_name) = root_item_name {
        metadata["root_item_name"] = serde_json::Value::String(root_item_name.to_string());
    }
    if id_token.is_some() {
        metadata["id_token"] = serde_json::Value::String(REDACTED_SECRET.to_string());
    }
    serde_json::to_string(&metadata).map_aster_err_ctx(
        "failed to serialize storage credential metadata",
        AsterError::internal_error,
    )
}

fn flow_client_secret_aad(policy_id: i64, state_hash: &str) -> String {
    format!("storage_policy_authorization_flow:{policy_id}:{state_hash}:client_secret")
}

async fn exchange_microsoft_graph_code(
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
    Ok(token)
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

fn microsoft_authorization_url(
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

fn callback_redirect_uri(
    state: &impl SharedRuntimeState,
    req: &actix_web::HttpRequest,
) -> Result<String> {
    let conn = req.connection_info();
    let uri = crate::config::site_url::public_app_url_for_request(
        state.runtime_config(),
        "/api/v1/admin/policies/storage-authorization/callback",
        conn.scheme(),
        conn.host(),
    )
    .ok_or_else(|| {
        AsterError::validation_error(
            "cannot build storage credential callback redirect URI; configure public_site_url",
        )
    })?;
    if uri.starts_with('/') {
        return Err(AsterError::validation_error(
            "storage credential callback redirect URI must be absolute; configure public_site_url",
        ));
    }
    Ok(uri)
}

fn build_pkce_verifier() -> String {
    let mut bytes = [0_u8; 32];
    let mut rng = rand::rng();
    rng.fill(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn build_pkce_challenge(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn microsoft_authorization_url_uses_selected_cloud_and_pkce() {
        let url = microsoft_authorization_url(
            MicrosoftGraphCloud::China,
            "organizations",
            "client-id",
            "https://drive.example.com/api/v1/admin/policies/storage-authorization/callback",
            &[
                "offline_access".to_string(),
                "Files.ReadWrite.All".to_string(),
            ],
            "state",
            "challenge",
        )
        .unwrap();

        assert!(
            url.starts_with("https://login.chinacloudapi.cn/organizations/oauth2/v2.0/authorize?")
        );
        assert!(url.contains("response_type=code"));
        assert!(url.contains("client_id=client-id"));
        assert!(url.contains("code_challenge=challenge"));
        assert!(url.contains("code_challenge_method=S256"));
    }
}
