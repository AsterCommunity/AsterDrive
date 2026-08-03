mod audit;
mod microsoft;
mod provider;
#[cfg(test)]
mod tests;

use chrono::{Duration, Utc};
use sea_orm::{ActiveValue::Set, ConnectionTrait, TransactionTrait};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use std::fmt;

use crate::db::repository::{
    policy_repo, storage_policy_authorization_flow_repo, storage_policy_connector_credential_repo,
};
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::runtime::SharedRuntimeState;
use crate::services::ops::audit::{AuditContext, AuditRequestInfo};
use crate::storage::StorageConnectorCredentialInfo;
use crate::storage::drivers::onedrive::{MicrosoftGraphClient, MicrosoftGraphClientConfig};
use aster_drive_model::entities::storage_policy_authorization_flow;
use aster_drive_model::types::{
    StorageAuthorizationFlowStatus, StorageCredentialProvider, StorageCredentialStatus,
};
use aster_forge_utils::id;

use super::{
    FLOW_TTL_SECS, MicrosoftGraphAuthorizationContext, MicrosoftGraphAuthorizationInput, crypto,
    normalize_optional_string, normalize_required_string, normalize_scopes,
    resolve_onedrive_location, scopes_to_json,
};
use audit::{
    OAUTH_AUDIT_EVENT_AUTHORIZATION_COMPLETED, OAUTH_AUDIT_EVENT_AUTHORIZATION_FAILED,
    OAUTH_AUDIT_EVENT_AUTHORIZATION_STARTED, OAUTH_AUDIT_RESULT_FAILED, OAUTH_AUDIT_RESULT_SUCCESS,
    StorageCredentialOauthAuditDetails, log_storage_credential_oauth_audit,
};
use microsoft::{
    MicrosoftGraphFlowContext, build_pkce_challenge, build_pkce_verifier,
    exchange_microsoft_graph_code, flow_client_secret_aad, microsoft_authorization_url,
    microsoft_graph_flow_cloud, microsoft_graph_flow_tenant,
};

pub(crate) use microsoft::{
    StorageCredentialMetadataInput, decrypt_application_client_secret,
    encrypt_application_client_secret, storage_credential_metadata,
};
pub(crate) use provider::{
    MicrosoftGraphCleanupTokenSnapshot, build_microsoft_graph_cleanup_token_provider,
    build_microsoft_graph_credential_token_provider,
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
    pub credential: StorageConnectorCredentialInfo,
}

pub(crate) async fn upsert_microsoft_graph_application_config<C: ConnectionTrait>(
    db: &C,
    encryption_key: &str,
    policy_id: i64,
    connector_config: &crate::storage::connectors::OneDriveConnectorConfigV1,
    input: crate::storage::connectors::OneDriveAuthorizationApplicationV1,
) -> Result<aster_drive_model::entities::storage_policy_connector_credential::Model> {
    let existing = storage_policy_connector_credential_repo::find_by_policy(db, policy_id).await?;
    let existing_payload = existing
        .as_ref()
        .map(|credential| {
            crate::storage::connectors::decode_typed_connector_credential::<
                crate::storage::connectors::OneDriveCredentialV1,
            >(
                encryption_key,
                credential,
                &aster_drive_storage::ConnectorId::declared(
                    crate::storage::connectors::OneDriveConnector::ID,
                ),
                1,
            )
        })
        .transpose()?;
    let cloud = connector_config.cloud;
    let tenant = normalize_optional_string(connector_config.tenant.clone())
        .unwrap_or_else(|| "common".to_string());
    let client_id = normalize_optional_string(Some(input.client_id))
        .or_else(|| {
            existing_payload
                .as_ref()
                .map(|payload| payload.application.client_id.clone())
        })
        .ok_or_else(|| AsterError::validation_error("client_id is required"))?;
    let client_secret = normalize_optional_string(Some(input.client_secret))
        .or_else(|| {
            existing_payload
                .as_ref()
                .map(|payload| payload.application.client_secret.clone())
        })
        .ok_or_else(|| AsterError::validation_error("client_secret is required"))?;
    let existing_scopes = existing_payload
        .as_ref()
        .map(|payload| payload.application.scopes.clone());
    let default_scopes =
        super::default_microsoft_graph_scopes_for_onedrive_config(connector_config);
    let scopes = match input
        .scopes
        .and_then(|value| normalize_optional_string(Some(value)))
    {
        Some(scopes) => super::normalize_scopes_with_default(
            Some(scopes.split_whitespace().map(ToOwned::to_owned).collect()),
            default_scopes,
        ),
        None => existing_scopes
            .filter(|scopes| !scopes.is_empty())
            .unwrap_or_else(|| super::normalize_scopes_with_default(None, default_scopes)),
    };
    let payload = crate::storage::connectors::OneDriveCredentialV1 {
        application: crate::storage::connectors::OneDriveApplicationCredentialV1 {
            cloud,
            tenant,
            client_id,
            client_secret,
            scopes,
        },
        authorization: existing_payload.and_then(|payload| payload.authorization),
    };
    crate::storage::connectors::persist_connector_credential_payload(
        db,
        encryption_key,
        policy_id,
        &aster_drive_storage::ConnectorId::declared(
            crate::storage::connectors::OneDriveConnector::ID,
        ),
        1,
        &payload,
    )
    .await?;
    storage_policy_connector_credential_repo::find_by_policy(db, policy_id)
        .await?
        .ok_or_else(|| {
            AsterError::record_not_found("OneDrive connector credential after application update")
        })
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

pub async fn start_authorization(
    state: &impl SharedRuntimeState,
    req: &actix_web::HttpRequest,
    policy_id: i64,
    created_by_user_id: i64,
    input: StorageAuthorizationStartInput,
) -> Result<StorageAuthorizationStartResponse> {
    let policy = policy_repo::find_by_id(state.writer_db(), policy_id).await?;
    crate::storage::connectors::ensure_storage_authorization_supported(
        state.driver_registry().connectors(),
        &policy,
        input.provider,
    )?;
    match input.provider {
        StorageCredentialProvider::MicrosoftGraph => {
            start_microsoft_graph_authorization(
                state,
                req,
                created_by_user_id,
                policy,
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
    created_by_user_id: i64,
    policy: aster_drive_model::entities::storage_policy::Model,
    input: Option<MicrosoftGraphAuthorizationInput>,
) -> Result<StorageAuthorizationStartResponse> {
    let input = input.unwrap_or_default();
    reject_unsaved_microsoft_graph_authorization_overrides(&input)?;
    let policy_id = policy.id;
    let existing_credential =
        storage_policy_connector_credential_repo::find_by_policy(state.writer_db(), policy_id)
            .await?;
    let existing_payload = existing_credential
        .as_ref()
        .map(|credential| {
            crate::storage::connectors::decode_typed_connector_credential::<
                crate::storage::connectors::OneDriveCredentialV1,
            >(
                &state.config().auth.storage_credential_secret_key,
                credential,
                &aster_drive_storage::ConnectorId::declared(
                    crate::storage::connectors::OneDriveConnector::ID,
                ),
                1,
            )
        })
        .transpose()?;
    let connector_config = crate::storage::connectors::OneDriveConnector::decode_config(&policy)?;
    let cloud = input
        .cloud
        .or_else(|| {
            existing_payload
                .as_ref()
                .map(|payload| payload.application.cloud)
        })
        .unwrap_or(connector_config.cloud);
    let tenant = normalize_optional_string(input.tenant)
        .or_else(|| {
            existing_payload
                .as_ref()
                .map(|payload| payload.application.tenant.clone())
        })
        .or_else(|| connector_config.tenant.clone())
        .unwrap_or_else(|| "common".to_string());
    let client_id = match normalize_optional_string(input.client_id).or_else(|| {
        existing_payload
            .as_ref()
            .map(|payload| payload.application.client_id.clone())
    }) {
        Some(client_id) => normalize_required_string(&client_id, "client_id", 512)?,
        None => return Err(AsterError::validation_error("client_id is required")),
    };
    let client_secret = match normalize_optional_string(input.client_secret) {
        Some(client_secret) => Some(SecretString::from(client_secret)),
        None => existing_payload
            .as_ref()
            .map(|payload| SecretString::from(payload.application.client_secret.clone())),
    };
    let client_secret = client_secret
        .map(|client_secret| {
            normalize_required_string(client_secret.expose_secret(), "client_secret", 2048)
                .map(SecretString::from)
        })
        .transpose()?
        .ok_or_else(|| {
            // AsterDrive stores OneDrive as a server-side backend. Treat the Microsoft app
            // as a confidential client so background refresh cannot silently fall back to
            // public-client OAuth semantics.
            AsterError::validation_error(
                "client_secret is required for Microsoft Graph storage authorization",
            )
        })?;
    let default_scopes =
        super::default_microsoft_graph_scopes_for_onedrive_config(&connector_config);
    let scopes = match input.scopes {
        Some(scopes) => super::normalize_scopes_with_default(Some(scopes), default_scopes),
        None => existing_payload
            .as_ref()
            .map(|payload| payload.application.scopes.clone())
            .filter(|scopes| !scopes.is_empty())
            .unwrap_or_else(|| super::normalize_scopes_with_default(None, default_scopes)),
    };
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
    let client_secret_ciphertext = Some(crypto::encrypt_token(
        &state.config().auth.storage_credential_secret_key,
        flow_client_secret_aad(policy_id, &state_hash).as_bytes(),
        client_secret.expose_secret(),
    )?);
    let context = MicrosoftGraphFlowContext {
        cloud,
        tenant: tenant.clone(),
        client_id: client_id.clone(),
        client_secret_ciphertext,
        scopes: scopes.clone(),
    };
    let now = Utc::now();
    let ttl =
        aster_forge_utils::numbers::u64_to_i64(FLOW_TTL_SECS, "storage authorization flow ttl")?;
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
    log_storage_credential_oauth_audit(
        state,
        &AuditRequestInfo::from_request(req).to_context(created_by_user_id),
        StorageCredentialOauthAuditDetails {
            event: OAUTH_AUDIT_EVENT_AUTHORIZATION_STARTED,
            result: OAUTH_AUDIT_RESULT_SUCCESS,
            policy_id: Some(policy_id),
            cloud: Some(cloud),
            tenant: Some(&tenant),
            client_secret_configured: Some(true),
            ..Default::default()
        },
    )
    .await;

    Ok(StorageAuthorizationStartResponse {
        authorization_url,
        expires_in: FLOW_TTL_SECS,
        provider: StorageCredentialProvider::MicrosoftGraph,
        microsoft_graph: Some(MicrosoftGraphAuthorizationContext {
            cloud,
            tenant,
            client_id,
            client_secret_configured: true,
            scopes,
        }),
    })
}

fn reject_unsaved_microsoft_graph_authorization_overrides(
    input: &MicrosoftGraphAuthorizationInput,
) -> Result<()> {
    if input.cloud.is_some()
        || normalize_optional_string(input.tenant.clone()).is_some()
        || normalize_optional_string(input.client_id.clone()).is_some()
        || normalize_optional_string(input.client_secret.clone()).is_some()
        || input.scopes.is_some()
    {
        return Err(AsterError::validation_error(
            "Microsoft Graph authorization overrides must be saved to storage connector application config before starting authorization",
        ));
    }
    Ok(())
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
        log_storage_credential_oauth_audit(
            state,
            &AuditContext::system(),
            StorageCredentialOauthAuditDetails {
                event: OAUTH_AUDIT_EVENT_AUTHORIZATION_FAILED,
                result: OAUTH_AUDIT_RESULT_FAILED,
                reason: Some(StorageAuthorizationFailureReason::ProviderError.as_str()),
                ..Default::default()
            },
        )
        .await;
        return Err(StorageAuthorizationCallbackError::new(
            StorageAuthorizationFailureReason::ProviderError,
            AsterError::auth_invalid_credentials(format!(
                "storage credential provider returned error: {description}"
            )),
        ));
    }
    let code = match query.code.as_deref() {
        Some(code) => code,
        None => {
            log_storage_credential_oauth_audit(
                state,
                &AuditContext::system(),
                StorageCredentialOauthAuditDetails {
                    event: OAUTH_AUDIT_EVENT_AUTHORIZATION_FAILED,
                    result: OAUTH_AUDIT_RESULT_FAILED,
                    reason: Some(StorageAuthorizationFailureReason::InvalidRequest.as_str()),
                    ..Default::default()
                },
            )
            .await;
            return Err(StorageAuthorizationCallbackError::new(
                StorageAuthorizationFailureReason::InvalidRequest,
                AsterError::auth_invalid_credentials("storage credential callback missing code"),
            ));
        }
    };
    let state_value = match query.state.as_deref() {
        Some(state_value) => state_value,
        None => {
            log_storage_credential_oauth_audit(
                state,
                &AuditContext::system(),
                StorageCredentialOauthAuditDetails {
                    event: OAUTH_AUDIT_EVENT_AUTHORIZATION_FAILED,
                    result: OAUTH_AUDIT_RESULT_FAILED,
                    reason: Some(StorageAuthorizationFailureReason::InvalidRequest.as_str()),
                    ..Default::default()
                },
            )
            .await;
            return Err(StorageAuthorizationCallbackError::new(
                StorageAuthorizationFailureReason::InvalidRequest,
                AsterError::auth_invalid_credentials("storage credential callback missing state"),
            ));
        }
    };

    let txn = state
        .writer_db()
        .begin()
        .await
        .map_err(|error| storage_authorization_callback_server_error(error.into()))?;
    let now = Utc::now();
    let flow = match storage_policy_authorization_flow_repo::consume_by_state_hash(
        &txn,
        &crypto::token_hash(state_value),
        now,
    )
    .await
    .map_err(storage_authorization_callback_server_error)?
    {
        Some(flow) => flow,
        None => {
            let _ = txn.rollback().await;
            log_storage_credential_oauth_audit(
                state,
                &AuditContext::system(),
                StorageCredentialOauthAuditDetails {
                    event: OAUTH_AUDIT_EVENT_AUTHORIZATION_FAILED,
                    result: OAUTH_AUDIT_RESULT_FAILED,
                    reason: Some(StorageAuthorizationFailureReason::InvalidState.as_str()),
                    ..Default::default()
                },
            )
            .await;
            return Err(StorageAuthorizationCallbackError::new(
                StorageAuthorizationFailureReason::InvalidState,
                AsterError::auth_invalid_credentials(
                    "storage credential state is invalid or expired",
                ),
            ));
        }
    };
    let flow_policy_id = flow.policy_id;
    let flow_user_id = flow.created_by_user_id;
    let flow_cloud = microsoft_graph_flow_cloud(&flow);
    let flow_tenant = microsoft_graph_flow_tenant(&flow);
    let policy_id = match flow.policy_id {
        Some(policy_id) => policy_id,
        None => {
            let _ = txn.rollback().await;
            return Err(storage_authorization_callback_server_error(
                AsterError::database_operation("storage authorization flow missing policy_id"),
            ));
        }
    };
    let policy = match policy_repo::find_by_id(&txn, policy_id)
        .await
        .map_err(storage_authorization_callback_server_error)
    {
        Ok(policy) => policy,
        Err(error) => {
            let _ = txn.rollback().await;
            return Err(error);
        }
    };
    if let Err(error) = crate::storage::connectors::ensure_storage_authorization_supported(
        state.driver_registry().connectors(),
        &policy,
        flow.provider,
    )
    .map_err(|error| {
        StorageAuthorizationCallbackError::new(
            StorageAuthorizationFailureReason::UnsupportedProvider,
            error,
        )
    }) {
        let _ = txn.rollback().await;
        log_storage_credential_oauth_audit(
            state,
            &AuditContext {
                user_id: flow_user_id,
                ip_address: None,
                user_agent: None,
            },
            StorageCredentialOauthAuditDetails {
                event: OAUTH_AUDIT_EVENT_AUTHORIZATION_FAILED,
                result: OAUTH_AUDIT_RESULT_FAILED,
                policy_id: flow_policy_id,
                cloud: flow_cloud,
                tenant: flow_tenant.as_deref(),
                reason: Some(error.reason().as_str()),
                ..Default::default()
            },
        )
        .await;
        return Err(error);
    }
    let connector_config = crate::storage::connectors::OneDriveConnector::decode_config(&policy)
        .map_err(storage_authorization_callback_server_error)?;
    // Keep Microsoft Graph token exchange and drive resolution outside the DB
    // transaction; provider latency must not hold SQLite/MySQL/Postgres locks.
    txn.commit()
        .await
        .map_err(|error| storage_authorization_callback_server_error(error.into()))?;
    let now = Utc::now();
    let credential_result = finish_authorization_provider_callback(
        &state.config().auth.storage_credential_secret_key,
        &flow,
        &connector_config,
        code,
        now,
    )
    .await;
    let credential = match credential_result {
        Ok(payload) => {
            let txn = state
                .writer_db()
                .begin()
                .await
                .map_err(|error| storage_authorization_callback_server_error(error.into()))?;
            let credential = match crate::storage::connectors::persist_connector_credential_payload(
                &txn,
                &state.config().auth.storage_credential_secret_key,
                policy_id,
                &aster_drive_storage::ConnectorId::declared(
                    crate::storage::connectors::OneDriveConnector::ID,
                ),
                1,
                &payload,
            )
            .await
            .map_err(storage_authorization_callback_server_error)
            {
                Ok(()) => {
                    crate::db::repository::storage_policy_connector_credential_repo::find_by_policy(
                        &txn, policy_id,
                    )
                    .await
                    .map_err(|error| storage_authorization_callback_server_error(error))?
                    .ok_or_else(|| {
                        storage_authorization_callback_server_error(AsterError::record_not_found(
                            "storage policy connector credential after authorization",
                        ))
                    })?
                }
                Err(error) => {
                    let _ = txn.rollback().await;
                    return Err(error);
                }
            };
            txn.commit()
                .await
                .map_err(|error| storage_authorization_callback_server_error(error.into()))?;
            credential
        }
        Err(error) => {
            let reason = error.reason().as_str();
            log_storage_credential_oauth_audit(
                state,
                &AuditContext {
                    user_id: flow_user_id,
                    ip_address: None,
                    user_agent: None,
                },
                StorageCredentialOauthAuditDetails {
                    event: OAUTH_AUDIT_EVENT_AUTHORIZATION_FAILED,
                    result: OAUTH_AUDIT_RESULT_FAILED,
                    policy_id: flow_policy_id,
                    cloud: flow_cloud,
                    tenant: flow_tenant.as_deref(),
                    reason: Some(reason),
                    ..Default::default()
                },
            )
            .await;
            return Err(error);
        }
    };
    state
        .driver_registry()
        .reload_storage_policy_credentials(state.writer_db(), state.config().as_ref())
        .await
        .map_err(storage_authorization_callback_server_error)?;
    crate::services::ops::config::runtime::publish_storage_topology_reload_after_commit(
        state,
        "authorize",
        "storage_policy_credential",
        policy_id,
    )
    .await;
    log_storage_credential_oauth_audit(
        state,
        &AuditContext {
            user_id: flow_user_id,
            ip_address: None,
            user_agent: None,
        },
        StorageCredentialOauthAuditDetails {
            event: OAUTH_AUDIT_EVENT_AUTHORIZATION_COMPLETED,
            result: OAUTH_AUDIT_RESULT_SUCCESS,
            policy_id: flow_policy_id,
            cloud: flow_cloud,
            tenant: flow_tenant.as_deref(),
            ..Default::default()
        },
    )
    .await;
    let policy = policy_repo::find_by_id(state.writer_db(), policy_id)
        .await
        .map_err(storage_authorization_callback_server_error)?;
    let credential_info = crate::storage::connectors::credential_info(
        state.driver_registry().connectors(),
        state.config().as_ref(),
        &policy,
        &credential,
    )
    .map_err(storage_authorization_callback_server_error)?
    .ok_or_else(|| {
        storage_authorization_callback_server_error(AsterError::record_not_found(
            "storage policy connector credential after authorization",
        ))
    })?;
    Ok(StorageAuthorizationCallbackOutcome {
        credential: credential_info,
    })
}

fn storage_authorization_callback_server_error(
    error: AsterError,
) -> StorageAuthorizationCallbackError {
    StorageAuthorizationCallbackError::new(StorageAuthorizationFailureReason::ServerError, error)
}

async fn finish_microsoft_graph_callback(
    encryption_key: &str,
    flow: &storage_policy_authorization_flow::Model,
    connector_config: &crate::storage::connectors::OneDriveConnectorConfigV1,
    code: &str,
    now: chrono::DateTime<Utc>,
) -> std::result::Result<
    crate::storage::connectors::OneDriveCredentialV1,
    StorageAuthorizationCallbackError,
> {
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
        Some(ciphertext) => crypto::decrypt_token(
            encryption_key,
            flow_client_secret_aad(policy_id, &flow.state_hash).as_bytes(),
            ciphertext,
        )
        .map(SecretString::from)
        .map_err(storage_authorization_callback_server_error)?,
        None => {
            return Err(StorageAuthorizationCallbackError::new(
                StorageAuthorizationFailureReason::InvalidRequest,
                AsterError::validation_error(
                    "client_secret is required for Microsoft Graph storage authorization",
                ),
            ));
        }
    };
    let token = exchange_microsoft_graph_code(
        &context,
        Some(&client_secret),
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
    let graph_client = MicrosoftGraphClient::new(MicrosoftGraphClientConfig::new(
        context.cloud.graph_base_url(),
        token.access_token.clone(),
    ))
    .map_err(|error| storage_authorization_callback_server_error(error.into()))?;
    let location = resolve_onedrive_location(&graph_client, connector_config)
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
    Ok(crate::storage::connectors::OneDriveCredentialV1 {
        application: crate::storage::connectors::OneDriveApplicationCredentialV1 {
            cloud: context.cloud,
            tenant: context.tenant.clone(),
            client_id: context.client_id.clone(),
            client_secret: client_secret.expose_secret().to_string(),
            scopes: context.scopes.clone(),
        },
        authorization: Some(
            crate::storage::connectors::OneDriveAuthorizationCredentialV1 {
                account_label: root_item.name.clone(),
                subject: Some(root_item.id.clone()),
                tenant_id: Some(context.tenant.clone()),
                scopes: granted_scopes,
                access_token: token.access_token,
                refresh_token: token.refresh_token.filter(|value| !value.trim().is_empty()),
                metadata: crate::storage::connectors::OneDriveAuthorizationMetadataV1 {
                    cloud: context.cloud,
                    drive_id: location.drive_id,
                    root_item_id: root_item.id,
                    root_item_name: root_item.name,
                    id_token_present: token.id_token.is_some(),
                },
                status: StorageCredentialStatus::Authorized,
                status_reason: None,
                expires_at,
                authorized_at: Some(now),
                last_refreshed_at: None,
                last_validated_at: None,
            },
        ),
    })
}

async fn finish_authorization_provider_callback(
    encryption_key: &str,
    flow: &storage_policy_authorization_flow::Model,
    connector_config: &crate::storage::connectors::OneDriveConnectorConfigV1,
    code: &str,
    now: chrono::DateTime<Utc>,
) -> std::result::Result<
    crate::storage::connectors::OneDriveCredentialV1,
    StorageAuthorizationCallbackError,
> {
    // Provider protocol handling stays in storage_policy::credential; the
    // connector layer only decides whether the policy is allowed to use it.
    match flow.provider {
        StorageCredentialProvider::MicrosoftGraph => {
            finish_microsoft_graph_callback(encryption_key, flow, connector_config, code, now).await
        }
        StorageCredentialProvider::GoogleDrive => Err(StorageAuthorizationCallbackError::new(
            StorageAuthorizationFailureReason::UnsupportedProvider,
            AsterError::unsupported_driver(
                "Google Drive storage credential authorization is not implemented yet",
            ),
        )),
    }
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
