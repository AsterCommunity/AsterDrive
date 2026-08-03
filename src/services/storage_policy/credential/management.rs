use serde::Serialize;

use crate::db::repository::{policy_repo, storage_policy_connector_credential_repo};
use crate::errors::{AsterError, Result};
use crate::runtime::SharedRuntimeState;
use crate::storage::StorageConnectorCredentialInfo;
use aster_drive_model::types::{StorageCredentialProvider, StorageCredentialStatus};
use aster_drive_storage::error::StorageErrorKind;

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct StoragePolicyCredentialValidationResult {
    pub credential: StorageConnectorCredentialInfo,
    pub root_item_id: String,
    pub root_item_name: Option<String>,
}

pub async fn list_policy_credentials(
    state: &impl SharedRuntimeState,
    policy_id: i64,
) -> Result<Vec<StorageConnectorCredentialInfo>> {
    policy_repo::find_by_id(state.reader_db(), policy_id).await?;
    let policy = policy_repo::find_by_id(state.reader_db(), policy_id).await?;
    let credentials =
        storage_policy_connector_credential_repo::find_by_policy(state.reader_db(), policy_id)
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

pub async fn validate_policy_credential(
    state: &impl SharedRuntimeState,
    policy_id: i64,
    provider: StorageCredentialProvider,
) -> Result<StoragePolicyCredentialValidationResult> {
    let policy = policy_repo::find_by_id(state.writer_db(), policy_id).await?;
    let credential_kind =
        crate::storage::connectors::ensure_storage_credential_validation_supported(
            state.driver_registry().connectors(),
            &policy,
            provider,
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
        &credential,
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

fn credential_status_for_validation_error(
    kind: Option<StorageErrorKind>,
) -> Option<StorageCredentialStatus> {
    match kind {
        Some(StorageErrorKind::Auth) => Some(StorageCredentialStatus::ReauthRequired),
        Some(StorageErrorKind::Permission) => Some(StorageCredentialStatus::PermissionDenied),
        Some(StorageErrorKind::Misconfigured) => Some(StorageCredentialStatus::Invalid),
        _ => None,
    }
}

fn credential_status_transition(
    current: StorageCredentialStatus,
    kind: Option<StorageErrorKind>,
) -> Option<StorageCredentialStatus> {
    let next = credential_status_for_validation_error(kind)?;
    (next != current).then_some(next)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credential_status_for_validation_error_only_persists_deterministic_failures() {
        assert_eq!(
            credential_status_for_validation_error(Some(StorageErrorKind::Auth)),
            Some(StorageCredentialStatus::ReauthRequired)
        );
        assert_eq!(
            credential_status_for_validation_error(Some(StorageErrorKind::Permission)),
            Some(StorageCredentialStatus::PermissionDenied)
        );
        assert_eq!(
            credential_status_for_validation_error(Some(StorageErrorKind::Misconfigured)),
            Some(StorageCredentialStatus::Invalid)
        );
        assert_eq!(
            credential_status_for_validation_error(Some(StorageErrorKind::Transient)),
            None
        );
        assert_eq!(
            credential_status_for_validation_error(Some(StorageErrorKind::RateLimited)),
            None
        );
        assert_eq!(
            credential_status_for_validation_error(Some(StorageErrorKind::Unknown)),
            None
        );
    }

    #[test]
    fn credential_status_transition_only_reports_topology_changes() {
        let current_statuses = [
            StorageCredentialStatus::Authorized,
            StorageCredentialStatus::ReauthRequired,
            StorageCredentialStatus::PermissionDenied,
            StorageCredentialStatus::Revoked,
            StorageCredentialStatus::Invalid,
        ];
        let deterministic_failures = [
            (
                StorageErrorKind::Auth,
                StorageCredentialStatus::ReauthRequired,
            ),
            (
                StorageErrorKind::Permission,
                StorageCredentialStatus::PermissionDenied,
            ),
            (
                StorageErrorKind::Misconfigured,
                StorageCredentialStatus::Invalid,
            ),
        ];

        for (kind, target) in deterministic_failures {
            for current in current_statuses {
                assert_eq!(
                    credential_status_transition(current, Some(kind)),
                    (current != target).then_some(target),
                    "unexpected transition from {} after {kind:?}",
                    current.as_str(),
                );
            }
        }
    }

    #[test]
    fn credential_status_transition_ignores_non_deterministic_failures() {
        for kind in [
            None,
            Some(StorageErrorKind::Transient),
            Some(StorageErrorKind::RateLimited),
            Some(StorageErrorKind::Unknown),
        ] {
            assert_eq!(
                credential_status_transition(StorageCredentialStatus::Authorized, kind),
                None
            );
        }
    }
}
