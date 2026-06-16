//! OAuth-managed storage policy credential service.

pub(crate) mod crypto;
mod management;
mod oauth;

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::entities::storage_policy_credential;
use crate::errors::{AsterError, Result};
use crate::storage::drivers::onedrive::{MicrosoftGraphClient, MicrosoftGraphDriveItem};
use crate::types::{
    MicrosoftGraphCloud, OneDriveAccountMode, StorageCredentialKind, StorageCredentialProvider,
    StorageCredentialStatus, StoragePolicyOptions,
};

pub use management::{
    StoragePolicyCredentialValidationResult, list_policy_credentials, validate_policy_credential,
};
pub use oauth::{
    StorageAuthorizationCallbackOutcome, StorageAuthorizationCallbackQuery,
    StorageAuthorizationStartInput, StorageAuthorizationStartResponse,
    finish_authorization_callback, start_authorization,
};

pub(crate) use oauth::{
    MicrosoftGraphCleanupTokenSnapshot, build_microsoft_graph_cleanup_token_provider,
    build_microsoft_graph_credential_token_provider,
};

const FLOW_TTL_SECS: u64 = 300;
const DEFAULT_MICROSOFT_GRAPH_SCOPES: &str =
    "offline_access Files.ReadWrite.All Sites.ReadWrite.All";
const DEFAULT_MICROSOFT_GRAPH_USER_DRIVE_SCOPES: &str = "offline_access Files.ReadWrite";
const DEFAULT_MICROSOFT_GRAPH_ANY_DRIVE_SCOPES: &str = "offline_access Files.ReadWrite.All";
const DEFAULT_MICROSOFT_GRAPH_SHARED_DRIVE_SCOPES: &str =
    "offline_access Files.ReadWrite.All Sites.ReadWrite.All";
const REDACTED_SECRET: &str = "***REDACTED***";

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct StoragePolicyCredentialInfo {
    pub id: i64,
    pub policy_id: i64,
    pub provider: StorageCredentialProvider,
    pub credential_kind: StorageCredentialKind,
    pub account_label: Option<String>,
    pub subject: Option<String>,
    pub tenant_id: Option<String>,
    pub scopes: Vec<String>,
    pub status: StorageCredentialStatus,
    pub status_reason: Option<String>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub expires_at: Option<chrono::DateTime<Utc>>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub authorized_at: Option<chrono::DateTime<Utc>>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub last_refreshed_at: Option<chrono::DateTime<Utc>>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub last_validated_at: Option<chrono::DateTime<Utc>>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: chrono::DateTime<Utc>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: chrono::DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct StorageCredentialProviderInfo {
    pub provider: StorageCredentialProvider,
    pub display_name: String,
    pub supported: bool,
    pub default_scopes: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct MicrosoftGraphAuthorizationContext {
    pub cloud: MicrosoftGraphCloud,
    pub tenant: String,
    pub client_id: String,
    pub client_secret_configured: bool,
    pub scopes: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
pub struct MicrosoftGraphAuthorizationInput {
    pub cloud: Option<MicrosoftGraphCloud>,
    pub tenant: Option<String>,
    // Microsoft application settings currently live with the OAuth credential metadata so
    // reauthorization can reuse them. Promote this to a shared app-config model only if
    // multiple storage policies need to share one Microsoft app registration.
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    pub scopes: Option<Vec<String>>,
}

#[derive(Clone, Debug)]
pub(crate) struct ResolvedOneDriveLocation {
    pub drive_id: String,
    pub root_item: MicrosoftGraphDriveItem,
}

impl From<storage_policy_credential::Model> for StoragePolicyCredentialInfo {
    fn from(model: storage_policy_credential::Model) -> Self {
        Self {
            id: model.id,
            policy_id: model.policy_id,
            provider: model.provider,
            credential_kind: model.credential_kind,
            account_label: model.account_label,
            subject: model.subject,
            tenant_id: model.tenant_id,
            scopes: parse_scopes_json(&model.scopes),
            status: model.status,
            status_reason: model.status_reason,
            expires_at: model.expires_at,
            authorized_at: model.authorized_at,
            last_refreshed_at: model.last_refreshed_at,
            last_validated_at: model.last_validated_at,
            created_at: model.created_at,
            updated_at: model.updated_at,
        }
    }
}

pub fn list_supported_providers() -> Vec<StorageCredentialProviderInfo> {
    vec![
        StorageCredentialProviderInfo {
            provider: StorageCredentialProvider::MicrosoftGraph,
            display_name: "Microsoft Graph".to_string(),
            supported: true,
            default_scopes: normalize_scopes(None),
        },
        StorageCredentialProviderInfo {
            provider: StorageCredentialProvider::GoogleDrive,
            display_name: "Google Drive".to_string(),
            supported: false,
            default_scopes: Vec::new(),
        },
    ]
}

fn normalize_scopes(value: Option<Vec<String>>) -> Vec<String> {
    normalize_scopes_with_default(value, DEFAULT_MICROSOFT_GRAPH_SCOPES)
}

pub(crate) fn normalize_scopes_with_default(
    value: Option<Vec<String>>,
    default_scopes: &str,
) -> Vec<String> {
    let input = value.unwrap_or_else(|| {
        default_scopes
            .split_whitespace()
            .map(ToOwned::to_owned)
            .collect()
    });
    let mut scopes = Vec::new();
    for scope in input {
        let scope = scope.trim();
        if !scope.is_empty() && !scopes.iter().any(|existing| existing == scope) {
            scopes.push(scope.to_string());
        }
    }
    if scopes.is_empty() {
        default_scopes
            .split_whitespace()
            .map(ToOwned::to_owned)
            .collect()
    } else {
        scopes
    }
}

pub(crate) fn default_microsoft_graph_scopes_for_onedrive_options(
    options: &StoragePolicyOptions,
) -> &'static str {
    match options.onedrive_account_mode {
        Some(OneDriveAccountMode::Personal | OneDriveAccountMode::WorkOrSchool)
            if options.onedrive_drive_id.is_none() =>
        {
            DEFAULT_MICROSOFT_GRAPH_USER_DRIVE_SCOPES
        }
        Some(OneDriveAccountMode::Personal | OneDriveAccountMode::WorkOrSchool) => {
            DEFAULT_MICROSOFT_GRAPH_ANY_DRIVE_SCOPES
        }
        Some(OneDriveAccountMode::SharepointSite | OneDriveAccountMode::GroupDrive) => {
            DEFAULT_MICROSOFT_GRAPH_SHARED_DRIVE_SCOPES
        }
        None => DEFAULT_MICROSOFT_GRAPH_SCOPES,
    }
}

pub(crate) async fn resolve_onedrive_location(
    client: &MicrosoftGraphClient,
    options: &StoragePolicyOptions,
) -> Result<ResolvedOneDriveLocation> {
    let account_mode = options.onedrive_account_mode.ok_or_else(|| {
        AsterError::validation_error("OneDrive storage policy missing onedrive_account_mode")
    })?;
    let drive_id = match normalized_option_ref(options.onedrive_drive_id.as_deref()) {
        Some(value) => value.to_string(),
        None => match account_mode {
            OneDriveAccountMode::Personal | OneDriveAccountMode::WorkOrSchool => {
                client.get_me_drive().await?.id
            }
            OneDriveAccountMode::SharepointSite => {
                let site_id = normalized_option_ref(options.onedrive_site_id.as_deref())
                    .ok_or_else(|| {
                        AsterError::validation_error(
                            "OneDrive sharepoint_site policy missing onedrive_site_id",
                        )
                    })?;
                client.get_site_drive(site_id).await?.id
            }
            OneDriveAccountMode::GroupDrive => {
                let group_id = normalized_option_ref(options.onedrive_group_id.as_deref())
                    .ok_or_else(|| {
                        AsterError::validation_error(
                            "OneDrive group_drive policy missing onedrive_group_id",
                        )
                    })?;
                client.get_group_drive(group_id).await?.id
            }
        },
    };
    if drive_id.trim().is_empty() {
        return Err(AsterError::database_operation(
            "Microsoft Graph returned empty OneDrive drive id",
        ));
    }
    let configured_root_item_id = options
        .onedrive_root_item_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("root");
    let root_item = if configured_root_item_id.eq_ignore_ascii_case("root") {
        client.get_drive_root(&drive_id).await?
    } else {
        client
            .get_drive_item_by_id(&drive_id, configured_root_item_id)
            .await?
    };
    Ok(ResolvedOneDriveLocation {
        drive_id,
        root_item,
    })
}

fn normalized_option_ref(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn scopes_to_json(scopes: &[String]) -> crate::errors::Result<String> {
    serde_json::to_string(scopes).map_err(|err| {
        crate::errors::AsterError::internal_error(format!(
            "failed to serialize storage credential scopes: {err}"
        ))
    })
}

fn parse_scopes_json(value: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(value).unwrap_or_else(|_| {
        value
            .split_whitespace()
            .map(str::trim)
            .filter(|scope| !scope.is_empty())
            .map(ToOwned::to_owned)
            .collect()
    })
}

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn normalize_required_string(
    value: &str,
    field: &str,
    max_len: usize,
) -> crate::errors::Result<String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(crate::errors::AsterError::validation_error(format!(
            "{field} is required"
        )));
    }
    if value.len() > max_len {
        return Err(crate::errors::AsterError::validation_error(format!(
            "{field} must be at most {max_len} bytes"
        )));
    }
    Ok(value.to_string())
}
