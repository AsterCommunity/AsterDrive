use chrono::Utc;
use sea_orm::{ActiveModelTrait, IntoActiveModel, Set};
use serde::Serialize;

use crate::db::repository::{policy_repo, storage_policy_credential_repo};
use crate::errors::{AsterError, Result};
use crate::runtime::SharedRuntimeState;
use crate::storage::error::StorageErrorKind;
use crate::types::{StorageCredentialProvider, StorageCredentialStatus};

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
            if let Some(status) = credential_status_for_validation_error(error.storage_error_kind())
            {
                active.status = Set(status);
            }
            active.status_reason = Set(Some(error.message().to_string()));
            active.updated_at = Set(Utc::now());
            active
                .update(state.writer_db())
                .await
                .map_err(AsterError::from)?;
            let _ = state
                .driver_registry()
                .reload_storage_policy_credentials(state.writer_db(), state.config().as_ref())
                .await;
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
}
