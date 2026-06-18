use chrono::Utc;
use sea_orm::{ActiveModelTrait, IntoActiveModel, Set};
use serde::Serialize;

use crate::db::repository::{policy_repo, storage_policy_credential_repo};
use crate::errors::{AsterError, Result};
use crate::runtime::SharedRuntimeState;
use crate::storage::drivers::onedrive::{MicrosoftGraphClient, MicrosoftGraphClientConfig};
use crate::storage::error::StorageErrorKind;
use crate::types::{
    StorageCredentialProvider, StorageCredentialStatus, parse_storage_policy_options,
};

use super::{
    StoragePolicyCredentialInfo, build_microsoft_graph_credential_token_provider,
    resolve_onedrive_location,
};

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
    let options = parse_storage_policy_options(policy.options.as_ref());
    let token_provider = build_microsoft_graph_credential_token_provider(
        state.writer_db().clone(),
        state.config().auth.storage_credential_secret_key.clone(),
        &policy,
        &credential,
        options.effective_onedrive_cloud(),
    )?;
    let client = MicrosoftGraphClient::new(MicrosoftGraphClientConfig::with_token_provider(
        options.effective_onedrive_cloud().graph_base_url(),
        token_provider,
    ))?;
    let location = match resolve_onedrive_location(&client, &options).await {
        Ok(location) => location,
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
    let root_item = location.root_item;
    let now = Utc::now();
    let policy_id = credential.policy_id;
    let existing_metadata = serde_json::from_str::<serde_json::Value>(&credential.metadata).ok();
    let existing_client_id = existing_metadata
        .as_ref()
        .and_then(|metadata| metadata.get("client_id"))
        .and_then(serde_json::Value::as_str);
    let existing_client_secret_ciphertext = existing_metadata
        .as_ref()
        .and_then(|metadata| metadata.get("client_secret_ciphertext"))
        .and_then(serde_json::Value::as_str);
    let mut active = credential.into_active_model();
    active.account_label = Set(root_item.name.clone());
    active.subject = Set(Some(root_item.id.clone()));
    active.metadata = Set(super::oauth::storage_credential_metadata(
        super::oauth::StorageCredentialMetadataInput {
            encryption_key: &state.config().auth.storage_credential_secret_key,
            policy_id,
            cloud: options.effective_onedrive_cloud(),
            client_id: existing_client_id,
            client_secret: None,
            client_secret_ciphertext: existing_client_secret_ciphertext,
            drive_id: &location.drive_id,
            root_item_id: &root_item.id,
            root_item_name: root_item.name.as_deref(),
            id_token: None,
        },
    )?);
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
        root_item_id: root_item.id,
        root_item_name: root_item.name,
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
