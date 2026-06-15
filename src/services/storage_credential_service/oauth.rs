use base64::Engine as _;
use chrono::{Duration, Utc};
use rand::RngExt;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, IntoActiveModel, TransactionTrait};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::db::repository::{
    policy_repo, storage_policy_authorization_flow_repo, storage_policy_credential_repo,
};
use crate::entities::{
    storage_policy, storage_policy_authorization_flow, storage_policy_credential,
};
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::runtime::SharedRuntimeState;
use crate::storage::drivers::onedrive::{
    MicrosoftGraphAccessTokenProvider, MicrosoftGraphClient, MicrosoftGraphClientConfig,
};
use crate::storage::error::{StorageErrorKind, storage_driver_error};
use crate::types::{
    DriverType, MicrosoftGraphCloud, StorageAuthorizationFlowStatus, StorageCredentialKind,
    StorageCredentialProvider, StorageCredentialStatus, parse_storage_policy_options,
};
use crate::utils::{OUTBOUND_HTTP_USER_AGENT, id};

use super::{
    FLOW_TTL_SECS, MicrosoftGraphAuthorizationContext, MicrosoftGraphAuthorizationInput,
    REDACTED_SECRET, StoragePolicyCredentialInfo, crypto, normalize_optional_string,
    normalize_required_string, normalize_scopes, resolve_onedrive_location, scopes_to_json,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StorageAuthorizationFailureReason {
    InvalidState,
    ProviderError,
    TokenExchangeFailed,
    DriveResolutionFailed,
    InvalidRequest,
    ServerError,
    UnsupportedProvider,
}

impl StorageAuthorizationFailureReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidState => "invalid_state",
            Self::ProviderError => "provider_error",
            Self::TokenExchangeFailed => "token_exchange_failed",
            Self::DriveResolutionFailed => "drive_resolution_failed",
            Self::InvalidRequest => "invalid_request",
            Self::ServerError => "server_error",
            Self::UnsupportedProvider => "unsupported_provider",
        }
    }
}

#[derive(Debug)]
pub struct StorageAuthorizationCallbackError {
    reason: StorageAuthorizationFailureReason,
    source: AsterError,
}

impl StorageAuthorizationCallbackError {
    fn new(reason: StorageAuthorizationFailureReason, source: AsterError) -> Self {
        Self { reason, source }
    }

    pub const fn reason(&self) -> StorageAuthorizationFailureReason {
        self.reason
    }

    pub fn source(&self) -> &AsterError {
        &self.source
    }
}

impl fmt::Display for StorageAuthorizationCallbackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.reason.as_str(), self.source)
    }
}

impl std::error::Error for StorageAuthorizationCallbackError {}

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

#[derive(Debug)]
pub(crate) struct MicrosoftGraphCredentialTokenProvider {
    db: sea_orm::DatabaseConnection,
    encryption_key: String,
    policy_id: i64,
    cloud: MicrosoftGraphCloud,
    tenant: String,
    client_id: String,
    client_secret: Option<String>,
    cache: Mutex<MicrosoftGraphCredentialTokenCache>,
    token_refresher: Arc<dyn MicrosoftGraphTokenRefresher>,
}

#[derive(Debug)]
struct MicrosoftGraphCredentialTokenCache {
    access_token: String,
    expires_at: Option<chrono::DateTime<Utc>>,
    refresh_token_ciphertext: Option<String>,
}

#[derive(Clone, Debug)]
struct MicrosoftGraphTokenRefreshRequest {
    cloud: MicrosoftGraphCloud,
    tenant: String,
    client_id: String,
    client_secret: Option<String>,
    refresh_token: String,
}

#[async_trait::async_trait]
trait MicrosoftGraphTokenRefresher: Send + Sync + fmt::Debug {
    async fn refresh_token(
        &self,
        request: MicrosoftGraphTokenRefreshRequest,
    ) -> Result<MicrosoftTokenResponse>;
}

#[derive(Debug)]
struct DefaultMicrosoftGraphTokenRefresher;

#[async_trait::async_trait]
impl MicrosoftGraphTokenRefresher for DefaultMicrosoftGraphTokenRefresher {
    async fn refresh_token(
        &self,
        request: MicrosoftGraphTokenRefreshRequest,
    ) -> Result<MicrosoftTokenResponse> {
        refresh_microsoft_graph_token(
            request.cloud,
            &request.tenant,
            &request.client_id,
            request.client_secret.as_deref(),
            &request.refresh_token,
        )
        .await
    }
}

pub(crate) fn build_microsoft_graph_credential_token_provider(
    db: sea_orm::DatabaseConnection,
    encryption_key: String,
    policy: &storage_policy::Model,
    credential: &storage_policy_credential::Model,
    cloud: MicrosoftGraphCloud,
) -> Result<Arc<dyn MicrosoftGraphAccessTokenProvider>> {
    build_microsoft_graph_credential_token_provider_with_refresher(
        db,
        encryption_key,
        policy,
        credential,
        cloud,
        Arc::new(DefaultMicrosoftGraphTokenRefresher),
    )
}

fn build_microsoft_graph_credential_token_provider_with_refresher(
    db: sea_orm::DatabaseConnection,
    encryption_key: String,
    policy: &storage_policy::Model,
    credential: &storage_policy_credential::Model,
    cloud: MicrosoftGraphCloud,
    token_refresher: Arc<dyn MicrosoftGraphTokenRefresher>,
) -> Result<Arc<dyn MicrosoftGraphAccessTokenProvider>> {
    let metadata = parse_metadata(&credential.metadata);
    let access_token_ciphertext =
        credential
            .access_token_ciphertext
            .as_deref()
            .ok_or_else(|| {
                storage_driver_error(
                    StorageErrorKind::Auth,
                    "storage credential is missing access token",
                )
            })?;
    let access_token = crypto::decrypt_token(
        &encryption_key,
        crypto::token_aad(
            credential.policy_id,
            StorageCredentialProvider::MicrosoftGraph.as_str(),
            "access",
        )
        .as_bytes(),
        access_token_ciphertext,
    )?;
    let client_id = normalize_optional_string(Some(policy.access_key.clone()))
        .or_else(|| {
            metadata
                .as_ref()
                .and_then(|metadata| metadata_string(metadata, "client_id"))
        })
        .ok_or_else(|| {
            storage_driver_error(
                StorageErrorKind::Auth,
                "storage credential is missing Microsoft Graph client_id; save the OneDrive policy application settings and reauthorize",
            )
        })?;
    let client_secret = match normalize_optional_string(Some(policy.secret_key.clone())) {
        Some(client_secret) => Some(client_secret),
        None => metadata
            .as_ref()
            .and_then(|metadata| metadata_string(metadata, "client_secret_ciphertext"))
            .map(|ciphertext| {
                decrypt_stored_client_secret(&encryption_key, credential.policy_id, &ciphertext)
            })
            .transpose()?,
    };
    Ok(Arc::new(MicrosoftGraphCredentialTokenProvider {
        db,
        encryption_key,
        policy_id: credential.policy_id,
        cloud,
        tenant: credential
            .tenant_id
            .clone()
            .filter(|tenant| !tenant.trim().is_empty())
            .unwrap_or_else(|| "common".to_string()),
        client_id,
        client_secret,
        cache: Mutex::new(MicrosoftGraphCredentialTokenCache {
            access_token,
            expires_at: credential.expires_at,
            refresh_token_ciphertext: credential.refresh_token_ciphertext.clone(),
        }),
        token_refresher,
    }))
}

#[async_trait::async_trait]
impl MicrosoftGraphAccessTokenProvider for MicrosoftGraphCredentialTokenProvider {
    async fn access_token(&self) -> Result<String> {
        {
            let cache = self.cache.lock().await;
            if cache
                .expires_at
                .is_none_or(|expires_at| expires_at > Utc::now() + Duration::seconds(60))
            {
                return Ok(cache.access_token.clone());
            }
        }
        self.refresh_access_token().await
    }

    async fn refresh_access_token(&self) -> Result<String> {
        let mut cache = self.cache.lock().await;
        let Some(refresh_token_ciphertext) = cache.refresh_token_ciphertext.as_deref() else {
            self.mark_reauth_required("storage credential is missing refresh token")
                .await?;
            return Err(storage_driver_error(
                StorageErrorKind::Auth,
                "storage credential is missing refresh token; reauthorize Microsoft Graph",
            ));
        };
        let refresh_token = crypto::decrypt_token(
            &self.encryption_key,
            crypto::token_aad(
                self.policy_id,
                StorageCredentialProvider::MicrosoftGraph.as_str(),
                "refresh",
            )
            .as_bytes(),
            refresh_token_ciphertext,
        )?;
        let token = match self
            .token_refresher
            .refresh_token(MicrosoftGraphTokenRefreshRequest {
                cloud: self.cloud,
                tenant: self.tenant.clone(),
                client_id: self.client_id.clone(),
                client_secret: self.client_secret.clone(),
                refresh_token,
            })
            .await
        {
            Ok(token) => token,
            Err(error) => {
                let _ = self.mark_reauth_required(error.message()).await;
                return Err(storage_driver_error(
                    StorageErrorKind::Auth,
                    format!("refresh Microsoft Graph access token: {error}"),
                ));
            }
        };
        let now = Utc::now();
        let expires_at = token
            .expires_in
            .and_then(|seconds| (seconds > 0).then(|| now + Duration::seconds(seconds)));
        let access_aad = crypto::token_aad(
            self.policy_id,
            StorageCredentialProvider::MicrosoftGraph.as_str(),
            "access",
        );
        let refresh_aad = crypto::token_aad(
            self.policy_id,
            StorageCredentialProvider::MicrosoftGraph.as_str(),
            "refresh",
        );
        let access_token_ciphertext = crypto::encrypt_token(
            &self.encryption_key,
            access_aad.as_bytes(),
            &token.access_token,
        )?;
        let refresh_token_ciphertext = match token.refresh_token.as_deref() {
            Some(refresh_token) if !refresh_token.trim().is_empty() => Some(crypto::encrypt_token(
                &self.encryption_key,
                refresh_aad.as_bytes(),
                refresh_token,
            )?),
            _ => cache.refresh_token_ciphertext.clone(),
        };
        let mut credential = storage_policy_credential_repo::find_by_policy_provider_kind(
            &self.db,
            self.policy_id,
            StorageCredentialProvider::MicrosoftGraph,
            StorageCredentialKind::OauthDelegated,
        )
        .await?
        .ok_or_else(|| AsterError::record_not_found("storage policy credential"))?
        .into_active_model();
        credential.access_token_ciphertext = Set(Some(access_token_ciphertext));
        credential.refresh_token_ciphertext = Set(refresh_token_ciphertext.clone());
        credential.expires_at = Set(expires_at);
        credential.last_refreshed_at = Set(Some(now));
        credential.status = Set(StorageCredentialStatus::Authorized);
        credential.status_reason = Set(None);
        credential.updated_at = Set(now);
        if let Some(scope) = token.scope.as_deref() {
            let scopes = normalize_scopes(Some(
                scope.split_whitespace().map(ToOwned::to_owned).collect(),
            ));
            credential.scopes = Set(scopes_to_json(&scopes)?);
        }
        credential
            .update(&self.db)
            .await
            .map_err(AsterError::from)?;

        cache.access_token = token.access_token;
        cache.expires_at = expires_at;
        cache.refresh_token_ciphertext = refresh_token_ciphertext;
        Ok(cache.access_token.clone())
    }
}

impl MicrosoftGraphCredentialTokenProvider {
    async fn mark_reauth_required(&self, reason: &str) -> Result<()> {
        let Some(credential) = storage_policy_credential_repo::find_by_policy_provider_kind(
            &self.db,
            self.policy_id,
            StorageCredentialProvider::MicrosoftGraph,
            StorageCredentialKind::OauthDelegated,
        )
        .await?
        else {
            return Ok(());
        };
        let now = Utc::now();
        let mut active = credential.into_active_model();
        active.status = Set(StorageCredentialStatus::ReauthRequired);
        active.status_reason = Set(Some(reason.to_string()));
        active.updated_at = Set(now);
        active.update(&self.db).await.map_err(AsterError::from)?;
        Ok(())
    }
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
                policy.access_key,
                policy.secret_key,
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
    saved_client_id: String,
    saved_client_secret: String,
    input: Option<MicrosoftGraphAuthorizationInput>,
) -> Result<StorageAuthorizationStartResponse> {
    let input = input.ok_or_else(|| {
        AsterError::validation_error("microsoft_graph authorization parameters are required")
    })?;
    let existing_credential = storage_policy_credential_repo::find_by_policy_provider_kind(
        state.writer_db(),
        policy_id,
        StorageCredentialProvider::MicrosoftGraph,
        StorageCredentialKind::OauthDelegated,
    )
    .await?;
    let existing_metadata = existing_credential
        .as_ref()
        .and_then(|credential| parse_metadata(&credential.metadata));
    let cloud = input
        .cloud
        .or_else(|| existing_metadata.as_ref().and_then(metadata_cloud))
        .unwrap_or_default();
    let tenant = normalize_optional_string(input.tenant)
        .or_else(|| {
            existing_credential
                .as_ref()
                .and_then(|credential| credential.tenant_id.clone())
        })
        .unwrap_or_else(|| "common".to_string());
    let client_id = match normalize_optional_string(input.client_id).or_else(|| {
        normalize_optional_string(Some(saved_client_id)).or_else(|| {
            existing_metadata
                .as_ref()
                .and_then(|metadata| metadata_string(metadata, "client_id"))
        })
    }) {
        Some(client_id) => normalize_required_string(&client_id, "client_id", 512)?,
        None => return Err(AsterError::validation_error("client_id is required")),
    };
    let client_secret = match normalize_optional_string(input.client_secret) {
        Some(client_secret) => Some(client_secret),
        None => match normalize_optional_string(Some(saved_client_secret)) {
            Some(client_secret) => Some(client_secret),
            None => existing_metadata
                .as_ref()
                .and_then(|metadata| metadata_string(metadata, "client_secret_ciphertext"))
                .map(|ciphertext| {
                    decrypt_stored_client_secret(
                        &state.config().auth.storage_credential_secret_key,
                        policy_id,
                        &ciphertext,
                    )
                })
                .transpose()?,
        },
    };
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
) -> std::result::Result<StorageAuthorizationCallbackOutcome, StorageAuthorizationCallbackError> {
    if let Some(error) = query.error.as_deref() {
        let description = query
            .error_description
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(error);
        return Err(StorageAuthorizationCallbackError::new(
            StorageAuthorizationFailureReason::ProviderError,
            AsterError::auth_invalid_credentials(format!(
                "storage credential provider returned error: {description}"
            )),
        ));
    }
    let code = query.code.as_deref().ok_or_else(|| {
        StorageAuthorizationCallbackError::new(
            StorageAuthorizationFailureReason::InvalidRequest,
            AsterError::auth_invalid_credentials("storage credential callback missing code"),
        )
    })?;
    let state_value = query.state.as_deref().ok_or_else(|| {
        StorageAuthorizationCallbackError::new(
            StorageAuthorizationFailureReason::InvalidRequest,
            AsterError::auth_invalid_credentials("storage credential callback missing state"),
        )
    })?;

    let txn = state
        .writer_db()
        .begin()
        .await
        .map_err(|error| storage_authorization_callback_server_error(error.into()))?;
    let now = Utc::now();
    let flow = storage_policy_authorization_flow_repo::consume_by_state_hash(
        &txn,
        &crypto::token_hash(state_value),
        now,
    )
    .await
    .map_err(storage_authorization_callback_server_error)?
    .ok_or_else(|| {
        StorageAuthorizationCallbackError::new(
            StorageAuthorizationFailureReason::InvalidState,
            AsterError::auth_invalid_credentials("storage credential state is invalid or expired"),
        )
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
            return Err(StorageAuthorizationCallbackError::new(
                StorageAuthorizationFailureReason::UnsupportedProvider,
                AsterError::unsupported_driver(
                    "Google Drive storage credential authorization is not implemented yet",
                ),
            ));
        }
    };
    txn.commit()
        .await
        .map_err(|error| storage_authorization_callback_server_error(error.into()))?;
    state
        .driver_registry()
        .reload_storage_policy_credentials(state.writer_db(), state.config().as_ref())
        .await
        .map_err(storage_authorization_callback_server_error)?;
    Ok(StorageAuthorizationCallbackOutcome {
        credential: credential.into(),
    })
}

fn storage_authorization_callback_server_error(
    error: AsterError,
) -> StorageAuthorizationCallbackError {
    StorageAuthorizationCallbackError::new(StorageAuthorizationFailureReason::ServerError, error)
}

async fn finish_microsoft_graph_callback<C: sea_orm::ConnectionTrait>(
    db: &C,
    encryption_key: &str,
    flow: &storage_policy_authorization_flow::Model,
    code: &str,
    now: chrono::DateTime<Utc>,
) -> std::result::Result<storage_policy_credential::Model, StorageAuthorizationCallbackError> {
    let policy_id = flow.policy_id.ok_or_else(|| {
        storage_authorization_callback_server_error(AsterError::database_operation(
            "storage authorization flow missing policy_id",
        ))
    })?;
    let context =
        serde_json::from_str::<MicrosoftGraphFlowContext>(&flow.context).map_err(|err| {
            storage_authorization_callback_server_error(AsterError::database_operation(format!(
                "invalid Microsoft Graph authorization context: {err}"
            )))
        })?;
    let pkce_verifier = flow.pkce_verifier.as_deref().ok_or_else(|| {
        storage_authorization_callback_server_error(AsterError::database_operation(
            "storage authorization flow missing PKCE verifier",
        ))
    })?;
    let client_secret = match context.client_secret_ciphertext.as_deref() {
        Some(ciphertext) => Some(
            crypto::decrypt_token(
                encryption_key,
                flow_client_secret_aad(policy_id, &flow.state_hash).as_bytes(),
                ciphertext,
            )
            .map_err(storage_authorization_callback_server_error)?,
        ),
        None => None,
    };
    let token = exchange_microsoft_graph_code(
        &context,
        client_secret.as_deref(),
        code,
        &flow.redirect_uri,
        pkce_verifier,
    )
    .await
    .map_err(|error| {
        StorageAuthorizationCallbackError::new(
            StorageAuthorizationFailureReason::TokenExchangeFailed,
            error,
        )
    })?;
    let policy = policy_repo::find_by_id(db, policy_id)
        .await
        .map_err(storage_authorization_callback_server_error)?;
    let options = parse_storage_policy_options(policy.options.as_ref());
    let graph_client = MicrosoftGraphClient::new(MicrosoftGraphClientConfig::new(
        context.cloud.graph_base_url(),
        token.access_token.clone(),
    ))
    .map_err(storage_authorization_callback_server_error)?;
    let location = resolve_onedrive_location(&graph_client, &options)
        .await
        .map_err(|error| {
            StorageAuthorizationCallbackError::new(
                StorageAuthorizationFailureReason::DriveResolutionFailed,
                error,
            )
        })?;
    let root_item = location.root_item;
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
        crypto::encrypt_token(encryption_key, access_aad.as_bytes(), &token.access_token)
            .map_err(storage_authorization_callback_server_error)?;
    let refresh_token_ciphertext = match token.refresh_token.as_deref() {
        Some(refresh_token) if !refresh_token.trim().is_empty() => Some(
            crypto::encrypt_token(encryption_key, refresh_aad.as_bytes(), refresh_token)
                .map_err(storage_authorization_callback_server_error)?,
        ),
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
            scopes: Set(scopes_to_json(&granted_scopes)
                .map_err(storage_authorization_callback_server_error)?),
            access_token_ciphertext: Set(Some(access_token_ciphertext)),
            refresh_token_ciphertext: Set(refresh_token_ciphertext),
            metadata: Set(storage_credential_metadata(
                encryption_key,
                policy_id,
                context.cloud,
                Some(&context.client_id),
                client_secret.as_deref(),
                None,
                &location.drive_id,
                &root_item.id,
                root_item.name.as_deref(),
                token.id_token.as_deref(),
            )
            .map_err(storage_authorization_callback_server_error)?),
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
    .map_err(storage_authorization_callback_server_error)
}

pub(super) fn storage_credential_metadata(
    encryption_key: &str,
    policy_id: i64,
    cloud: MicrosoftGraphCloud,
    client_id: Option<&str>,
    client_secret: Option<&str>,
    client_secret_ciphertext: Option<&str>,
    drive_id: &str,
    root_item_id: &str,
    root_item_name: Option<&str>,
    id_token: Option<&str>,
) -> Result<String> {
    let mut metadata = serde_json::json!({
        "cloud": cloud,
        "graph_base_url": cloud.graph_base_url(),
        "drive_id": drive_id,
        "root_item_id": root_item_id,
    });
    if let Some(client_id) = client_id {
        metadata["client_id"] = serde_json::Value::String(client_id.to_string());
    }
    if let Some(client_secret) = client_secret {
        let ciphertext = encrypt_stored_client_secret(encryption_key, policy_id, client_secret)?;
        metadata["client_secret_configured"] = serde_json::Value::Bool(true);
        metadata["client_secret_ciphertext"] = serde_json::Value::String(ciphertext);
    } else if let Some(ciphertext) = client_secret_ciphertext {
        metadata["client_secret_configured"] = serde_json::Value::Bool(true);
        metadata["client_secret_ciphertext"] = serde_json::Value::String(ciphertext.to_string());
    } else {
        metadata["client_secret_configured"] = serde_json::Value::Bool(false);
    }
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

fn stored_client_secret_aad(policy_id: i64) -> String {
    format!("storage_policy_credential:{policy_id}:microsoft_graph:client_secret")
}

fn encrypt_stored_client_secret(
    encryption_key: &str,
    policy_id: i64,
    client_secret: &str,
) -> Result<String> {
    crypto::encrypt_token(
        encryption_key,
        stored_client_secret_aad(policy_id).as_bytes(),
        client_secret,
    )
}

fn decrypt_stored_client_secret(
    encryption_key: &str,
    policy_id: i64,
    ciphertext: &str,
) -> Result<String> {
    crypto::decrypt_token(
        encryption_key,
        stored_client_secret_aad(policy_id).as_bytes(),
        ciphertext,
    )
}

fn parse_metadata(value: &str) -> Option<serde_json::Value> {
    serde_json::from_str(value).ok()
}

fn metadata_string(metadata: &serde_json::Value, key: &str) -> Option<String> {
    metadata
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn metadata_cloud(metadata: &serde_json::Value) -> Option<MicrosoftGraphCloud> {
    serde_json::from_value(metadata.get("cloud")?.clone()).ok()
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
    validate_microsoft_token_response(&token)?;
    Ok(token)
}

async fn refresh_microsoft_graph_token(
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
        .map_aster_err_ctx(
            "Microsoft Graph OAuth token refresh failed",
            AsterError::auth_invalid_credentials,
        )?;
    if !response.status().is_success() {
        return Err(microsoft_token_endpoint_error(response).await);
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

fn validate_microsoft_token_response(token: &MicrosoftTokenResponse) -> Result<()> {
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
    use crate::config::DatabaseConfig;
    use crate::db;
    use crate::types::{StoredStoragePolicyAllowedTypes, StoredStoragePolicyOptions};
    use migration::Migrator;
    use std::collections::VecDeque;
    use std::sync::Mutex as StdMutex;

    #[derive(Debug)]
    struct TestMicrosoftGraphTokenRefresher {
        requests: StdMutex<Vec<MicrosoftGraphTokenRefreshRequest>>,
        responses: StdMutex<VecDeque<Result<MicrosoftTokenResponse>>>,
    }

    impl TestMicrosoftGraphTokenRefresher {
        fn new(responses: Vec<Result<MicrosoftTokenResponse>>) -> Self {
            Self {
                requests: StdMutex::new(Vec::new()),
                responses: StdMutex::new(responses.into()),
            }
        }

        fn requests(&self) -> Vec<MicrosoftGraphTokenRefreshRequest> {
            self.requests
                .lock()
                .expect("refresh request log lock")
                .clone()
        }
    }

    #[async_trait::async_trait]
    impl MicrosoftGraphTokenRefresher for TestMicrosoftGraphTokenRefresher {
        async fn refresh_token(
            &self,
            request: MicrosoftGraphTokenRefreshRequest,
        ) -> Result<MicrosoftTokenResponse> {
            self.requests
                .lock()
                .expect("refresh request log lock")
                .push(request);
            self.responses
                .lock()
                .expect("refresh response queue lock")
                .pop_front()
                .expect("refresh response should be queued")
        }
    }

    fn microsoft_token_response(
        access_token: &str,
        refresh_token: Option<&str>,
        expires_in: i64,
    ) -> MicrosoftTokenResponse {
        MicrosoftTokenResponse {
            access_token: access_token.to_string(),
            refresh_token: refresh_token.map(ToOwned::to_owned),
            token_type: Some("Bearer".to_string()),
            expires_in: Some(expires_in),
            scope: Some("offline_access Files.ReadWrite.All".to_string()),
            id_token: None,
        }
    }

    async fn setup_db() -> sea_orm::DatabaseConnection {
        let db = db::connect_with_metrics(
            &DatabaseConfig {
                url: "sqlite::memory:".to_string(),
                pool_size: 1,
                retry_count: 0,
            },
            crate::metrics_core::NoopMetrics::arc(),
        )
        .await
        .expect("storage credential test DB should connect");
        Migrator::up(&db, None)
            .await
            .expect("storage credential migrations should succeed");
        db
    }

    async fn create_onedrive_policy(
        db: &sea_orm::DatabaseConnection,
        client_id: &str,
        client_secret: &str,
    ) -> storage_policy::Model {
        let now = Utc::now();
        policy_repo::create(
            db,
            storage_policy::ActiveModel {
                name: Set("onedrive".to_string()),
                driver_type: Set(DriverType::OneDrive),
                endpoint: Set(String::new()),
                bucket: Set(String::new()),
                access_key: Set(client_id.to_string()),
                secret_key: Set(client_secret.to_string()),
                base_path: Set(String::new()),
                remote_node_id: Set(None),
                max_file_size: Set(0),
                allowed_types: Set(StoredStoragePolicyAllowedTypes::empty()),
                options: Set(StoredStoragePolicyOptions::empty()),
                is_default: Set(false),
                chunk_size: Set(5_242_880),
                created_at: Set(now),
                updated_at: Set(now),
                ..Default::default()
            },
        )
        .await
        .expect("policy should insert")
    }

    async fn create_microsoft_graph_credential(
        db: &sea_orm::DatabaseConnection,
        encryption_key: &str,
        policy_id: i64,
        access_token: &str,
        refresh_token: Option<&str>,
        expires_at: Option<chrono::DateTime<Utc>>,
    ) -> storage_policy_credential::Model {
        let now = Utc::now();
        let access_token_ciphertext = crypto::encrypt_token(
            encryption_key,
            crypto::token_aad(
                policy_id,
                StorageCredentialProvider::MicrosoftGraph.as_str(),
                "access",
            )
            .as_bytes(),
            access_token,
        )
        .expect("access token should encrypt");
        let refresh_token_ciphertext = refresh_token
            .map(|refresh_token| {
                crypto::encrypt_token(
                    encryption_key,
                    crypto::token_aad(
                        policy_id,
                        StorageCredentialProvider::MicrosoftGraph.as_str(),
                        "refresh",
                    )
                    .as_bytes(),
                    refresh_token,
                )
            })
            .transpose()
            .expect("refresh token should encrypt");
        storage_policy_credential_repo::upsert_by_policy_provider_kind(
            db,
            storage_policy_credential::ActiveModel {
                policy_id: Set(policy_id),
                provider: Set(StorageCredentialProvider::MicrosoftGraph),
                credential_kind: Set(StorageCredentialKind::OauthDelegated),
                account_label: Set(Some("Drive".to_string())),
                subject: Set(Some("root".to_string())),
                tenant_id: Set(Some("common".to_string())),
                scopes: Set(r#"["offline_access","Files.ReadWrite.All"]"#.to_string()),
                access_token_ciphertext: Set(Some(access_token_ciphertext)),
                refresh_token_ciphertext: Set(refresh_token_ciphertext),
                metadata: Set(serde_json::json!({
                    "cloud": MicrosoftGraphCloud::Global,
                    "drive_id": "drive-id",
                    "root_item_id": "root"
                })
                .to_string()),
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
        .expect("credential should insert")
    }

    fn decrypt_stored_oauth_token(
        encryption_key: &str,
        policy_id: i64,
        kind: &str,
        ciphertext: &str,
    ) -> String {
        crypto::decrypt_token(
            encryption_key,
            crypto::token_aad(
                policy_id,
                StorageCredentialProvider::MicrosoftGraph.as_str(),
                kind,
            )
            .as_bytes(),
            ciphertext,
        )
        .expect("stored OAuth token should decrypt")
    }

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

    #[test]
    fn storage_authorization_failure_reason_values_are_stable() {
        assert_eq!(
            StorageAuthorizationFailureReason::InvalidState.as_str(),
            "invalid_state"
        );
        assert_eq!(
            StorageAuthorizationFailureReason::ProviderError.as_str(),
            "provider_error"
        );
        assert_eq!(
            StorageAuthorizationFailureReason::TokenExchangeFailed.as_str(),
            "token_exchange_failed"
        );
        assert_eq!(
            StorageAuthorizationFailureReason::DriveResolutionFailed.as_str(),
            "drive_resolution_failed"
        );
        assert_eq!(
            StorageAuthorizationFailureReason::InvalidRequest.as_str(),
            "invalid_request"
        );
        assert_eq!(
            StorageAuthorizationFailureReason::ServerError.as_str(),
            "server_error"
        );
        assert_eq!(
            StorageAuthorizationFailureReason::UnsupportedProvider.as_str(),
            "unsupported_provider"
        );
    }

    #[test]
    fn storage_metadata_encrypts_client_secret_for_reuse() {
        let key = "storage-token-test-master-key";
        let metadata = storage_credential_metadata(
            key,
            42,
            MicrosoftGraphCloud::Global,
            Some("client-id"),
            Some("client-secret"),
            None,
            "drive-id",
            "root",
            Some("Root"),
            None,
        )
        .unwrap();
        let parsed = serde_json::from_str::<serde_json::Value>(&metadata).unwrap();

        assert_eq!(parsed["client_id"], "client-id");
        assert_eq!(parsed["client_secret_configured"], true);
        assert_ne!(parsed["client_secret_ciphertext"], "client-secret");
        assert_eq!(
            decrypt_stored_client_secret(
                key,
                42,
                parsed["client_secret_ciphertext"].as_str().unwrap(),
            )
            .unwrap(),
            "client-secret"
        );
    }

    #[test]
    fn storage_metadata_preserves_existing_client_secret_ciphertext() {
        let key = "storage-token-test-master-key";
        let ciphertext = encrypt_stored_client_secret(key, 42, "client-secret").unwrap();
        let metadata = storage_credential_metadata(
            key,
            42,
            MicrosoftGraphCloud::China,
            Some("client-id"),
            None,
            Some(&ciphertext),
            "drive-id",
            "root",
            Some("Root"),
            None,
        )
        .unwrap();
        let parsed = serde_json::from_str::<serde_json::Value>(&metadata).unwrap();

        assert_eq!(parsed["client_secret_configured"], true);
        assert_eq!(parsed["client_secret_ciphertext"], ciphertext);
        assert_eq!(
            decrypt_stored_client_secret(
                key,
                42,
                parsed["client_secret_ciphertext"].as_str().unwrap(),
            )
            .unwrap(),
            "client-secret"
        );
    }

    #[test]
    fn microsoft_token_response_validation_accepts_bearer_or_missing_token_type() {
        validate_microsoft_token_response(&MicrosoftTokenResponse {
            access_token: "access-token".to_string(),
            refresh_token: None,
            token_type: Some("Bearer".to_string()),
            expires_in: Some(3600),
            scope: None,
            id_token: None,
        })
        .unwrap();

        validate_microsoft_token_response(&MicrosoftTokenResponse {
            access_token: "access-token".to_string(),
            refresh_token: None,
            token_type: None,
            expires_in: Some(3600),
            scope: None,
            id_token: None,
        })
        .unwrap();
    }

    #[test]
    fn microsoft_token_response_validation_rejects_blank_access_token() {
        let error = validate_microsoft_token_response(&MicrosoftTokenResponse {
            access_token: " ".to_string(),
            refresh_token: None,
            token_type: Some("Bearer".to_string()),
            expires_in: Some(3600),
            scope: None,
            id_token: None,
        })
        .unwrap_err();

        assert!(error.message().contains("missing access_token"));
    }

    #[test]
    fn microsoft_token_response_validation_rejects_unsupported_token_type() {
        let error = validate_microsoft_token_response(&MicrosoftTokenResponse {
            access_token: "access-token".to_string(),
            refresh_token: None,
            token_type: Some("mac".to_string()),
            expires_in: Some(3600),
            scope: None,
            id_token: None,
        })
        .unwrap_err();

        assert!(error.message().contains("unsupported token_type"));
    }

    #[tokio::test]
    async fn credential_token_provider_returns_cached_access_token_before_expiry() {
        let db = setup_db().await;
        let encryption_key = "storage-token-test-master-key";
        let policy = create_onedrive_policy(&db, "client-id", "client-secret").await;
        let credential = create_microsoft_graph_credential(
            &db,
            encryption_key,
            policy.id,
            "cached-access-token",
            None,
            Some(Utc::now() + Duration::minutes(10)),
        )
        .await;
        let provider = build_microsoft_graph_credential_token_provider(
            db.clone(),
            encryption_key.to_string(),
            &policy,
            &credential,
            MicrosoftGraphCloud::Global,
        )
        .expect("provider should build");

        let access_token = provider.access_token().await.expect("token should load");

        assert_eq!(access_token, "cached-access-token");
        let stored = storage_policy_credential_repo::find_by_policy_provider_kind(
            &db,
            policy.id,
            StorageCredentialProvider::MicrosoftGraph,
            StorageCredentialKind::OauthDelegated,
        )
        .await
        .expect("credential lookup should succeed")
        .expect("credential should exist");
        assert_eq!(stored.status, StorageCredentialStatus::Authorized);
        assert_eq!(stored.status_reason, None);
    }

    #[tokio::test]
    async fn credential_token_provider_marks_reauth_required_when_refresh_token_is_missing() {
        let db = setup_db().await;
        let encryption_key = "storage-token-test-master-key";
        let policy = create_onedrive_policy(&db, "client-id", "client-secret").await;
        let credential = create_microsoft_graph_credential(
            &db,
            encryption_key,
            policy.id,
            "expired-access-token",
            None,
            Some(Utc::now() - Duration::minutes(10)),
        )
        .await;
        let provider = build_microsoft_graph_credential_token_provider(
            db.clone(),
            encryption_key.to_string(),
            &policy,
            &credential,
            MicrosoftGraphCloud::Global,
        )
        .expect("provider should build");

        let error = provider.access_token().await.unwrap_err();

        assert_eq!(error.storage_error_kind(), Some(StorageErrorKind::Auth));
        let stored = storage_policy_credential_repo::find_by_policy_provider_kind(
            &db,
            policy.id,
            StorageCredentialProvider::MicrosoftGraph,
            StorageCredentialKind::OauthDelegated,
        )
        .await
        .expect("credential lookup should succeed")
        .expect("credential should exist");
        assert_eq!(stored.status, StorageCredentialStatus::ReauthRequired);
        assert!(
            stored
                .status_reason
                .as_deref()
                .unwrap_or_default()
                .contains("missing refresh token")
        );
    }

    #[tokio::test]
    async fn credential_token_provider_refresh_success_writes_new_access_and_refresh_tokens() {
        let db = setup_db().await;
        let encryption_key = "storage-token-test-master-key";
        let policy = create_onedrive_policy(&db, "client-id", "client-secret").await;
        let credential = create_microsoft_graph_credential(
            &db,
            encryption_key,
            policy.id,
            "expired-access-token",
            Some("old-refresh-token"),
            Some(Utc::now() - Duration::minutes(10)),
        )
        .await;
        let refresher = Arc::new(TestMicrosoftGraphTokenRefresher::new(vec![Ok(
            microsoft_token_response("new-access-token", Some("new-refresh-token"), 3600),
        )]));
        let provider = build_microsoft_graph_credential_token_provider_with_refresher(
            db.clone(),
            encryption_key.to_string(),
            &policy,
            &credential,
            MicrosoftGraphCloud::Global,
            refresher.clone(),
        )
        .expect("provider should build");

        let access_token = provider.access_token().await.expect("token should refresh");

        assert_eq!(access_token, "new-access-token");
        assert_eq!(refresher.requests().len(), 1);
        let request = refresher
            .requests()
            .into_iter()
            .next()
            .expect("request should be logged");
        assert_eq!(request.cloud, MicrosoftGraphCloud::Global);
        assert_eq!(request.tenant, "common");
        assert_eq!(request.client_id, "client-id");
        assert_eq!(request.client_secret.as_deref(), Some("client-secret"));
        assert_eq!(request.refresh_token, "old-refresh-token");

        let stored = storage_policy_credential_repo::find_by_policy_provider_kind(
            &db,
            policy.id,
            StorageCredentialProvider::MicrosoftGraph,
            StorageCredentialKind::OauthDelegated,
        )
        .await
        .expect("credential lookup should succeed")
        .expect("credential should exist");
        assert_eq!(stored.status, StorageCredentialStatus::Authorized);
        assert_eq!(stored.status_reason, None);
        assert!(stored.last_refreshed_at.is_some());
        assert!(
            stored
                .expires_at
                .is_some_and(|expires_at| expires_at > Utc::now())
        );
        assert_eq!(
            decrypt_stored_oauth_token(
                encryption_key,
                policy.id,
                "access",
                stored.access_token_ciphertext.as_deref().unwrap(),
            ),
            "new-access-token"
        );
        assert_eq!(
            decrypt_stored_oauth_token(
                encryption_key,
                policy.id,
                "refresh",
                stored.refresh_token_ciphertext.as_deref().unwrap(),
            ),
            "new-refresh-token"
        );
        assert_eq!(
            serde_json::from_str::<Vec<String>>(&stored.scopes).unwrap(),
            vec![
                "offline_access".to_string(),
                "Files.ReadWrite.All".to_string()
            ]
        );
    }

    #[tokio::test]
    async fn credential_token_provider_refresh_success_preserves_refresh_token_when_response_omits_it()
     {
        let db = setup_db().await;
        let encryption_key = "storage-token-test-master-key";
        let policy = create_onedrive_policy(&db, "client-id", "client-secret").await;
        let credential = create_microsoft_graph_credential(
            &db,
            encryption_key,
            policy.id,
            "expired-access-token",
            Some("old-refresh-token"),
            Some(Utc::now() - Duration::minutes(10)),
        )
        .await;
        let refresher = Arc::new(TestMicrosoftGraphTokenRefresher::new(vec![Ok(
            microsoft_token_response("new-access-token", None, 3600),
        )]));
        let provider = build_microsoft_graph_credential_token_provider_with_refresher(
            db.clone(),
            encryption_key.to_string(),
            &policy,
            &credential,
            MicrosoftGraphCloud::Global,
            refresher,
        )
        .expect("provider should build");

        let access_token = provider.access_token().await.expect("token should refresh");

        assert_eq!(access_token, "new-access-token");
        let stored = storage_policy_credential_repo::find_by_policy_provider_kind(
            &db,
            policy.id,
            StorageCredentialProvider::MicrosoftGraph,
            StorageCredentialKind::OauthDelegated,
        )
        .await
        .expect("credential lookup should succeed")
        .expect("credential should exist");
        assert_eq!(
            decrypt_stored_oauth_token(
                encryption_key,
                policy.id,
                "refresh",
                stored.refresh_token_ciphertext.as_deref().unwrap(),
            ),
            "old-refresh-token"
        );
        assert_eq!(
            decrypt_stored_oauth_token(
                encryption_key,
                policy.id,
                "access",
                stored.access_token_ciphertext.as_deref().unwrap(),
            ),
            "new-access-token"
        );
    }

    #[tokio::test]
    async fn credential_token_provider_refresh_failure_marks_reauth_required() {
        let db = setup_db().await;
        let encryption_key = "storage-token-test-master-key";
        let policy = create_onedrive_policy(&db, "client-id", "client-secret").await;
        let credential = create_microsoft_graph_credential(
            &db,
            encryption_key,
            policy.id,
            "expired-access-token",
            Some("old-refresh-token"),
            Some(Utc::now() - Duration::minutes(10)),
        )
        .await;
        let refresher = Arc::new(TestMicrosoftGraphTokenRefresher::new(vec![Err(
            AsterError::auth_invalid_credentials("invalid_grant"),
        )]));
        let provider = build_microsoft_graph_credential_token_provider_with_refresher(
            db.clone(),
            encryption_key.to_string(),
            &policy,
            &credential,
            MicrosoftGraphCloud::Global,
            refresher,
        )
        .expect("provider should build");

        let error = provider.access_token().await.unwrap_err();

        assert_eq!(error.storage_error_kind(), Some(StorageErrorKind::Auth));
        let stored = storage_policy_credential_repo::find_by_policy_provider_kind(
            &db,
            policy.id,
            StorageCredentialProvider::MicrosoftGraph,
            StorageCredentialKind::OauthDelegated,
        )
        .await
        .expect("credential lookup should succeed")
        .expect("credential should exist");
        assert_eq!(stored.status, StorageCredentialStatus::ReauthRequired);
        assert!(
            stored
                .status_reason
                .as_deref()
                .unwrap_or_default()
                .contains("invalid_grant")
        );
        assert_eq!(
            decrypt_stored_oauth_token(
                encryption_key,
                policy.id,
                "access",
                stored.access_token_ciphertext.as_deref().unwrap(),
            ),
            "expired-access-token"
        );
    }

    #[tokio::test]
    async fn credential_token_provider_requires_access_token_ciphertext() {
        let db = setup_db().await;
        let encryption_key = "storage-token-test-master-key";
        let policy = create_onedrive_policy(&db, "client-id", "client-secret").await;
        let mut credential = create_microsoft_graph_credential(
            &db,
            encryption_key,
            policy.id,
            "access-token",
            Some("refresh-token"),
            Some(Utc::now() + Duration::minutes(10)),
        )
        .await;
        credential.access_token_ciphertext = None;

        let error = build_microsoft_graph_credential_token_provider(
            db,
            encryption_key.to_string(),
            &policy,
            &credential,
            MicrosoftGraphCloud::Global,
        )
        .unwrap_err();

        assert_eq!(error.storage_error_kind(), Some(StorageErrorKind::Auth));
        assert!(error.message().contains("missing access token"));
    }

    #[tokio::test]
    async fn credential_token_provider_requires_client_id_from_policy_or_metadata() {
        let db = setup_db().await;
        let encryption_key = "storage-token-test-master-key";
        let policy = create_onedrive_policy(&db, " ", "client-secret").await;
        let credential = create_microsoft_graph_credential(
            &db,
            encryption_key,
            policy.id,
            "access-token",
            Some("refresh-token"),
            Some(Utc::now() + Duration::minutes(10)),
        )
        .await;

        let error = build_microsoft_graph_credential_token_provider(
            db,
            encryption_key.to_string(),
            &policy,
            &credential,
            MicrosoftGraphCloud::Global,
        )
        .unwrap_err();

        assert_eq!(error.storage_error_kind(), Some(StorageErrorKind::Auth));
        assert!(
            error
                .message()
                .contains("missing Microsoft Graph client_id")
        );
    }
}
