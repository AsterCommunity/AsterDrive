use chrono::Utc;
use sea_orm::{ActiveModelTrait, IntoActiveModel, Set};
use serde::Serialize;

use crate::db::repository::{policy_repo, storage_policy_credential_repo};
use crate::errors::{AsterError, Result};
use crate::runtime::SharedRuntimeState;
use aster_drive_model::types::{StorageCredentialProvider, StorageCredentialStatus};
use aster_drive_storage::error::StorageErrorKind;

use super::StoragePolicyCredentialInfo;

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct StoragePolicyCredentialValidationResult {
    pub credential: StoragePolicyCredentialInfo,
    pub root_item_id: String,
    pub root_item_name: Option<String>,
}

pub async fn list_policy_credentials(
    state: &impl SharedRuntimeState,
    policy_id: i64,
) -> Result<Vec<StoragePolicyCredentialInfo>> {
    policy_repo::find_by_id(state.reader_db(), policy_id).await?;
    let credentials = storage_policy_credential_repo::list_by_policy(state.reader_db(), policy_id)
        .await?
        .into_iter()
        .map(Into::into)
        .collect();
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
            policy.driver_type,
            provider,
        )?;
    let credential = storage_policy_credential_repo::find_by_policy_provider_kind(
        state.writer_db(),
        policy_id,
        provider,
        credential_kind,
    )
    .await?
    .ok_or_else(|| AsterError::record_not_found("storage policy credential"))?;
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
            let mut active = credential.clone().into_active_model();
            let status_transition =
                credential_status_transition(credential.status, error.storage_error_kind());
            if let Some(status) = status_transition {
                active.status = Set(status);
            }
            active.status_reason = Set(Some(error.message().to_string()));
            active.updated_at = Set(Utc::now());
            active
                .update(state.writer_db())
                .await
                .map_err(AsterError::from)?;
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
            if status_transition.is_some() {
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
    let now = Utc::now();
    let mut active = credential.into_active_model();
    active.account_label = Set(validation.account_label.clone());
    active.subject = Set(validation.subject.clone());
    active.metadata = Set(validation.metadata);
    active.status = Set(StorageCredentialStatus::Authorized);
    active.status_reason = Set(None);
    active.last_validated_at = Set(Some(now));
    active.updated_at = Set(now);
    let credential = active
        .update(state.writer_db())
        .await
        .map_err(AsterError::from)?;
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
        credential: credential.into(),
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
