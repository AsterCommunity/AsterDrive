use chrono::Utc;
use sea_orm::{ActiveModelTrait, IntoActiveModel, Set};
use serde::Serialize;

use crate::db::repository::{policy_repo, storage_policy_credential_repo};
use crate::errors::{AsterError, Result};
use crate::runtime::SharedRuntimeState;
use crate::storage::drivers::onedrive::{MicrosoftGraphClient, MicrosoftGraphClientConfig};
use crate::types::{
    DriverType, StorageCredentialKind, StorageCredentialProvider, StorageCredentialStatus,
    parse_storage_policy_options,
};

use super::{StoragePolicyCredentialInfo, crypto};

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
    if policy.driver_type != DriverType::OneDrive
        || provider != StorageCredentialProvider::MicrosoftGraph
    {
        return Err(AsterError::validation_error(
            "storage credential validation is only supported for Microsoft Graph OneDrive policies",
        ));
    }
    let credential = storage_policy_credential_repo::find_by_policy_provider_kind(
        state.writer_db(),
        policy_id,
        provider,
        StorageCredentialKind::OauthDelegated,
    )
    .await?
    .ok_or_else(|| AsterError::record_not_found("storage policy credential"))?;
    let access_token_ciphertext =
        credential
            .access_token_ciphertext
            .as_deref()
            .ok_or_else(|| {
                AsterError::auth_invalid_credentials(
                    "storage policy credential is missing access token",
                )
            })?;
    let access_aad = crypto::token_aad(policy_id, provider.as_str(), "access");
    let access_token = crypto::decrypt_token(
        &state.config().auth.storage_credential_secret_key,
        access_aad.as_bytes(),
        access_token_ciphertext,
    )?;
    let options = parse_storage_policy_options(policy.options.as_ref());
    let drive_id = options.onedrive_drive_id.as_deref().ok_or_else(|| {
        AsterError::database_operation("OneDrive storage policy missing onedrive_drive_id")
    })?;
    let root_item_id = options.onedrive_root_item_id.as_deref().ok_or_else(|| {
        AsterError::database_operation("OneDrive storage policy missing onedrive_root_item_id")
    })?;
    let client = MicrosoftGraphClient::new(MicrosoftGraphClientConfig::new(
        options.effective_onedrive_cloud().graph_base_url(),
        access_token,
    ))?;
    let root_item = match client.get_drive_item_by_id(drive_id, root_item_id).await {
        Ok(root_item) => root_item,
        Err(error) => {
            let mut active = credential.clone().into_active_model();
            active.status = Set(match error.storage_error_kind() {
                Some(crate::storage::StorageErrorKind::Auth) => {
                    StorageCredentialStatus::ReauthRequired
                }
                Some(crate::storage::StorageErrorKind::Permission) => {
                    StorageCredentialStatus::PermissionDenied
                }
                _ => StorageCredentialStatus::Invalid,
            });
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
    active.account_label = Set(root_item.name.clone());
    active.subject = Set(Some(root_item.id.clone()));
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
