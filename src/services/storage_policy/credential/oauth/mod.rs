pub(crate) mod audit;

use chrono::Utc;
use sea_orm::{ActiveValue::Set, TransactionTrait};
use serde::{Deserialize, Serialize};
use std::fmt;

use crate::db::repository::{
    policy_repo, storage_policy_authorization_flow_repo, storage_policy_connector_credential_repo,
};
use crate::errors::{AsterError, Result};
use crate::runtime::{SharedRuntimeState, StorageConnectorRuntimeState};
use crate::services::ops::audit::{AuditContext, AuditRequestInfo};
use crate::storage::StorageConnectorCredentialInfo;
use aster_drive_model::entities::storage_policy_authorization_flow;
use aster_drive_model::types::StorageAuthorizationFlowStatus;

use audit::{
    OAUTH_AUDIT_EVENT_AUTHORIZATION_COMPLETED, OAUTH_AUDIT_EVENT_AUTHORIZATION_FAILED,
    OAUTH_AUDIT_EVENT_AUTHORIZATION_STARTED, OAUTH_AUDIT_RESULT_FAILED, OAUTH_AUDIT_RESULT_SUCCESS,
    StorageCredentialOauthAuditDetails, log_storage_credential_oauth_audit,
};

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct StorageAuthorizationStartResponse {
    pub authorization_url: String,
    pub expires_in: u64,
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

#[derive(Debug)]
pub(crate) struct StorageAuthorizationCallbackError {
    reason: crate::storage::connectors::StorageAuthorizationFailureReason,
    source: AsterError,
}

impl StorageAuthorizationCallbackError {
    fn new(
        reason: crate::storage::connectors::StorageAuthorizationFailureReason,
        source: AsterError,
    ) -> Self {
        Self { reason, source }
    }

    pub(crate) const fn reason(
        &self,
    ) -> crate::storage::connectors::StorageAuthorizationFailureReason {
        self.reason
    }
}

impl fmt::Display for StorageAuthorizationCallbackError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.reason.as_str(), self.source)
    }
}

impl std::error::Error for StorageAuthorizationCallbackError {}

pub async fn start_authorization(
    state: &(impl StorageConnectorRuntimeState + Sync),
    req: &actix_web::HttpRequest,
    policy_id: i64,
    created_by_user_id: i64,
) -> Result<StorageAuthorizationStartResponse> {
    let policy = policy_repo::find_by_id(state.writer_db(), policy_id).await?;
    let registry = state.driver_registry().connectors();
    let expected_provider =
        crate::storage::connectors::ensure_storage_authorization_supported(registry, &policy)?;
    let connector = registry.require_policy(&policy)?;
    let redirect_uri = callback_redirect_uri(state, req)?;
    let context = crate::storage::connectors::shared_connector_context(state);
    let start = connector
        .start_authorization(&context, &policy, &redirect_uri)
        .await?;
    if start.provider != expected_provider || start.audit.provider != start.provider {
        return Err(AsterError::internal_error(format!(
            "storage connector '{}' returned inconsistent authorization provider metadata",
            policy.connector_id
        )));
    }

    let now = Utc::now();
    let ttl =
        aster_forge_utils::numbers::u64_to_i64(start.expires_in, "storage authorization flow ttl")?;
    let state_hash = super::crypto::token_hash(&start.state);
    storage_policy_authorization_flow_repo::cancel_pending_for_policy(
        state.writer_db(),
        policy_id,
        now,
    )
    .await?;
    storage_policy_authorization_flow_repo::create(
        state.writer_db(),
        storage_policy_authorization_flow::ActiveModel {
            provider: Set(start.provider),
            policy_id: Set(Some(policy_id)),
            created_by_user_id: Set(created_by_user_id),
            state_hash: Set(state_hash),
            pkce_verifier: Set(start.pkce_verifier),
            redirect_uri: Set(redirect_uri),
            scopes: Set(serialize_scopes(&start.scopes)?),
            context: Set(start.context),
            status: Set(StorageAuthorizationFlowStatus::Pending),
            created_at: Set(now),
            expires_at: Set(now + chrono::Duration::seconds(ttl)),
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
            connector_id: Some(&policy.connector_id),
            provider: Some(start.provider),
            fields: Some(&start.audit.fields),
            ..Default::default()
        },
    )
    .await;

    Ok(StorageAuthorizationStartResponse {
        authorization_url: start.authorization_url,
        expires_in: start.expires_in,
    })
}

pub(crate) async fn finish_authorization_callback(
    state: &(impl SharedRuntimeState + Sync),
    query: &StorageAuthorizationCallbackQuery,
) -> std::result::Result<StorageAuthorizationCallbackOutcome, StorageAuthorizationCallbackError> {
    use crate::storage::connectors::StorageAuthorizationFailureReason;

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
        &super::crypto::token_hash(state_value),
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
    let policy_id = flow.policy_id.ok_or_else(|| {
        storage_authorization_callback_server_error(AsterError::database_operation(
            "storage authorization flow missing policy_id",
        ))
    })?;
    let policy = policy_repo::find_by_id(&txn, policy_id)
        .await
        .map_err(storage_authorization_callback_server_error)?;
    let registry = state.driver_registry().connectors();
    let expected_provider =
        crate::storage::connectors::ensure_storage_authorization_supported(registry, &policy)
            .map_err(|error| {
                StorageAuthorizationCallbackError::new(
                    StorageAuthorizationFailureReason::UnsupportedProvider,
                    error,
                )
            })?;
    if expected_provider != flow.provider {
        return Err(StorageAuthorizationCallbackError::new(
            StorageAuthorizationFailureReason::UnsupportedProvider,
            AsterError::unsupported_driver(format!(
                "storage authorization flow provider '{}' does not match connector '{}'",
                flow.provider.as_str(),
                policy.connector_id
            )),
        ));
    }
    txn.commit()
        .await
        .map_err(|error| storage_authorization_callback_server_error(error.into()))?;

    // Provider network calls stay outside the transaction. The consumed flow
    // remains one-time even when token exchange or remote discovery fails.
    let connector = registry
        .require_policy(&policy)
        .map_err(storage_authorization_callback_server_error)?;
    let connector_context = crate::storage::connectors::shared_connector_context(state);
    let callback = connector
        .finish_authorization(&connector_context, &policy, &flow, code, Utc::now())
        .await
        .map_err(|error| {
            StorageAuthorizationCallbackError::new(error.reason(), error.into_source())
        });
    let callback = match callback {
        Ok(callback) => callback,
        Err(error) => {
            log_storage_credential_oauth_audit(
                state,
                &AuditContext {
                    user_id: flow.created_by_user_id,
                    ip_address: None,
                    user_agent: None,
                },
                StorageCredentialOauthAuditDetails {
                    event: OAUTH_AUDIT_EVENT_AUTHORIZATION_FAILED,
                    result: OAUTH_AUDIT_RESULT_FAILED,
                    policy_id: Some(policy_id),
                    connector_id: Some(&policy.connector_id),
                    provider: Some(flow.provider),
                    reason: Some(error.reason().as_str()),
                    ..Default::default()
                },
            )
            .await;
            return Err(error);
        }
    };
    if callback.audit.provider != flow.provider {
        return Err(storage_authorization_callback_server_error(
            AsterError::internal_error(format!(
                "storage connector '{}' returned inconsistent callback provider metadata",
                policy.connector_id
            )),
        ));
    }

    let descriptor = connector.descriptor();
    let txn = state
        .writer_db()
        .begin()
        .await
        .map_err(|error| storage_authorization_callback_server_error(error.into()))?;
    crate::storage::connectors::persist_connector_credential_payload(
        &txn,
        &state.config().auth.storage_credential_secret_key,
        policy_id,
        &descriptor.connector_id,
        crate::storage::connectors::credential_schema_version(&descriptor)
            .map_err(storage_authorization_callback_server_error)?,
        &callback.credential_payload,
    )
    .await
    .map_err(storage_authorization_callback_server_error)?;
    let credential = storage_policy_connector_credential_repo::find_by_policy(&txn, policy_id)
        .await
        .map_err(storage_authorization_callback_server_error)?
        .ok_or_else(|| {
            storage_authorization_callback_server_error(AsterError::record_not_found(
                "storage policy connector credential after authorization",
            ))
        })?;
    txn.commit()
        .await
        .map_err(|error| storage_authorization_callback_server_error(error.into()))?;

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
            user_id: flow.created_by_user_id,
            ip_address: None,
            user_agent: None,
        },
        StorageCredentialOauthAuditDetails {
            event: OAUTH_AUDIT_EVENT_AUTHORIZATION_COMPLETED,
            result: OAUTH_AUDIT_RESULT_SUCCESS,
            policy_id: Some(policy_id),
            connector_id: Some(&policy.connector_id),
            provider: Some(flow.provider),
            fields: Some(&callback.audit.fields),
            ..Default::default()
        },
    )
    .await;
    let credential_info = crate::storage::connectors::credential_info(
        registry,
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

fn serialize_scopes(scopes: &[String]) -> Result<String> {
    serde_json::to_string(scopes).map_err(|error| {
        AsterError::internal_error(format!("serialize storage authorization scopes: {error}"))
    })
}

fn storage_authorization_callback_server_error(
    error: AsterError,
) -> StorageAuthorizationCallbackError {
    StorageAuthorizationCallbackError::new(
        crate::storage::connectors::StorageAuthorizationFailureReason::ServerError,
        error,
    )
}

fn callback_redirect_uri(
    state: &impl StorageConnectorRuntimeState,
    req: &actix_web::HttpRequest,
) -> Result<String> {
    let connection = req.connection_info();
    let uri = crate::config::site_url::public_app_url_for_request(
        state.runtime_config(),
        "/api/v1/admin/policies/storage-authorization/callback",
        connection.scheme(),
        connection.host(),
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
