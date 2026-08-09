use serde::Serialize;

use crate::db::repository::{policy_repo, storage_policy_connector_credential_repo};
use crate::errors::{AsterError, Result};
use crate::runtime::SharedRuntimeState;
use crate::storage::StorageConnectorCredentialInfo;

/// Successful credential validation together with the resolved provider root.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct StoragePolicyCredentialValidationResult {
    pub credential: StorageConnectorCredentialInfo,
    pub root_item_id: String,
    pub root_item_name: Option<String>,
}

/// Lists the connector credentials used by a storage policy.
///
/// This deliberately reads both the policy and credential rows from the writer
/// connection. The admin flow calls it immediately after authorization or
/// validation, and a reader replica may still contain the previous credential
/// status at that point.
pub async fn list_policy_credentials(
    state: &impl SharedRuntimeState,
    policy_id: i64,
) -> Result<Vec<StorageConnectorCredentialInfo>> {
    let policy = policy_repo::find_by_id(state.writer_db(), policy_id).await?;
    let credentials =
        storage_policy_connector_credential_repo::find_by_policy(state.writer_db(), policy_id)
            .await?
            .into_iter()
            .filter_map(|credential| {
                crate::storage::connectors::credential_info(
                    state.driver_registry().connectors(),
                    state.config().as_ref(),
                    &policy,
                    &credential,
                )
                .transpose()
            })
            .collect::<Result<Vec<_>>>()?;
    Ok(credentials)
}

/// Validates a policy credential and persists the resulting lifecycle state.
///
/// Provider failures may still produce a sanitized credential payload (for
/// example, an expired or reauthorization-required status). That payload is
/// committed before the original validation error is returned so a subsequent
/// writer-backed list reports the actionable state to the administrator.
pub async fn validate_policy_credential(
    state: &impl SharedRuntimeState,
    policy_id: i64,
) -> Result<StoragePolicyCredentialValidationResult> {
    let policy = policy_repo::find_by_id(state.writer_db(), policy_id).await?;
    let (provider, credential_kind) =
        crate::storage::connectors::ensure_storage_credential_validation_supported(
            state.driver_registry().connectors(),
            &policy,
        )?;
    let credential =
        storage_policy_connector_credential_repo::find_by_policy(state.writer_db(), policy_id)
            .await?
            .ok_or_else(|| AsterError::record_not_found("storage policy connector credential"))?;
    let info = crate::storage::connectors::credential_info(
        state.driver_registry().connectors(),
        state.config().as_ref(),
        &policy,
        &credential,
    )?
    .ok_or_else(|| AsterError::record_not_found("storage policy connector credential"))?;
    if info.provider != provider || info.credential_kind != credential_kind {
        return Err(AsterError::unsupported_driver(
            "storage credential does not match the selected connector authorization",
        ));
    }
    let validation = match crate::storage::connectors::validate_credential(
        state.driver_registry().connectors(),
        state.writer_db(),
        state.config().as_ref(),
        &policy,
        &credential,
    )
    .await
    {
        Ok(validation) => validation,
        Err(error) => {
            if let Some(payload) =
                crate::storage::connectors::credential_validation_failure_payload(
                    state.driver_registry().connectors(),
                    state.config().as_ref(),
                    &policy,
                    &credential,
                    error.storage_error_kind(),
                    error.message(),
                )?
            {
                crate::storage::connectors::persist_connector_credential_value(
                    state.writer_db(),
                    &state.config().auth.storage_credential_secret_key,
                    &credential,
                    payload,
                )
                .await?;
            }
            if let Err(reload_error) = state
                .driver_registry()
                .reload_storage_policy_credentials(state.writer_db(), state.config().as_ref())
                .await
            {
                tracing::warn!(
                    storage_policy_id = policy_id,
                    credential_provider = provider.as_str(),
                    "failed to reload storage policy credentials after validation failure: {reload_error}"
                );
            }
            if error.storage_error_kind().is_some() {
                crate::services::ops::config::runtime::publish_storage_topology_reload_after_commit(
                    state,
                    "update_status",
                    "storage_policy_credential",
                    policy_id,
                )
                .await;
            }
            return Err(error);
        }
    };
    let credential = crate::storage::connectors::persist_connector_credential_value(
        state.writer_db(),
        &state.config().auth.storage_credential_secret_key,
        &validation.credential,
        validation.credential_payload,
    )
    .await?;
    state
        .driver_registry()
        .reload_storage_policy_credentials(state.writer_db(), state.config().as_ref())
        .await?;
    crate::services::ops::config::runtime::publish_storage_topology_reload_after_commit(
        state,
        "validate",
        "storage_policy_credential",
        policy_id,
    )
    .await;

    Ok(StoragePolicyCredentialValidationResult {
        credential: crate::storage::connectors::credential_info(
            state.driver_registry().connectors(),
            state.config().as_ref(),
            &policy,
            &credential,
        )?
        .ok_or_else(|| AsterError::record_not_found("storage policy connector credential"))?,
        root_item_id: validation.root_item_id,
        root_item_name: validation.root_item_name,
    })
}
