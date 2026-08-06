use async_trait::async_trait;
use sea_orm::ConnectionTrait;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use crate::db::repository::storage_policy_connector_credential_repo;
use crate::errors::{AsterError, Result, storage_driver_error};
use crate::storage::drivers::onedrive::{
    MicrosoftGraphAccessTokenProvider, MicrosoftGraphClient, MicrosoftGraphClientConfig,
    OneDriveDriver, microsoft_graph_upload_capabilities,
};
use aster_drive_model::entities::storage_policy;
use aster_drive_model::types::{
    MicrosoftGraphCloud, ProviderDownloadFilenameMode, ProviderDownloadStrategy,
    ProviderResumableUploadStrategy, StorageCredentialKind, StorageCredentialProvider,
    StorageCredentialStatus,
};
use aster_drive_storage::StorageDriver;
use aster_drive_storage::connector_descriptor::{
    StorageConnectorBadgeRgb, StorageConnectorCapabilities,
    StorageConnectorCredentialManagementDescriptor, StorageConnectorDeploymentScope,
    StorageConnectorDescriptor, StorageConnectorFieldKind, StorageConnectorFieldScope,
    StorageConnectorObjectNamingMode, StorageConnectorProviderResumableUploadCapabilities,
    StorageConnectorUiDescriptorInput, StorageConnectorUploadWorkflows,
    saved_connection_test_action_descriptor, server_relay_simple_upload_capabilities,
    start_authorization_action_descriptor, storage_connector_field, storage_connector_select_field,
    storage_connector_ui_descriptor, validate_credential_action_descriptor,
};
use aster_drive_storage::{
    StorageConnectorConfigSchema, StorageConnectorFieldDefaultValue,
    StorageConnectorSelectOptionInput,
};
use aster_forge_utils::id;

use super::common::unsupported_draft_connection_test_error;
use super::{
    StorageAuthorizationFailureReason, StorageConnector, StorageConnectorAuthorizationAudit,
    StorageConnectorAuthorizationCallback, StorageConnectorAuthorizationError,
    StorageConnectorAuthorizationStart, StorageConnectorCredentialInput,
    StorageConnectorRuntimeCredential, StorageConnectorUploadTransport,
    StorageCredentialValidationOutcome, StoragePolicyCleanupDriverSnapshot,
    StoragePolicyCleanupSnapshots,
};

mod localization;
mod oauth;
mod provider;

#[cfg(test)]
pub(crate) use oauth::encrypt_application_client_secret;

const AUTHORIZATION_FLOW_TTL_SECS: u64 = 300;
const CLEANUP_SNAPSHOT_SCHEMA_VERSION: u32 = 1;

pub struct OneDriveConnector;

/// Connector-owned Microsoft Graph drive location mode.
///
/// This enum is persisted only inside the versioned OneDrive connector config.
/// Core storage-policy models and orchestration treat the value as opaque.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub(crate) enum OneDriveAccountMode {
    Personal,
    WorkOrSchool,
    SharepointSite,
    GroupDrive,
}

pub(crate) struct OneDriveResolvedLocation {
    pub(crate) drive_id: String,
    pub(crate) root_item: crate::storage::drivers::onedrive::MicrosoftGraphDriveItem,
}

#[derive(Clone)]
struct OneDriveRuntimeCredential {
    token_provider: Arc<dyn MicrosoftGraphAccessTokenProvider>,
    drive_id: Option<String>,
    root_item_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct OneDriveCleanupSnapshotV1 {
    cloud: MicrosoftGraphCloud,
    #[serde(default)]
    tenant_id: Option<String>,
    #[serde(default)]
    client_id: Option<String>,
    #[serde(default)]
    client_secret_ciphertext: Option<String>,
    drive_id: String,
    root_item_id: String,
    access_token_ciphertext: String,
    #[serde(default)]
    refresh_token_ciphertext: Option<String>,
    #[serde(default)]
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Default, Deserialize)]
struct LegacyOneDriveMetadata {
    #[serde(default)]
    cloud: Option<MicrosoftGraphCloud>,
    #[serde(default)]
    drive_id: Option<String>,
    #[serde(default)]
    root_item_id: Option<String>,
    #[serde(default)]
    root_item_name: Option<String>,
    #[serde(default)]
    id_token_present: bool,
    #[serde(default)]
    id_token: Option<serde_json::Value>,
}

aster_drive_storage::storage_connector_schema! {
    pub struct OneDriveConnectorConfigV1 {
        config {
        pub base_path: String => {
            let mut field = storage_connector_field(
                "base_path", StorageConnectorFieldScope::ConnectorConfig,
                StorageConnectorFieldKind::Text, false, false,
            );
            field.default_value = Some(StorageConnectorFieldDefaultValue::String(String::new()));
            field.default_mode = aster_drive_storage::StorageConnectorFieldDefaultMode::MissingOrEmptyText;
            field
        },
        pub provider_resumable_upload_strategy: ProviderResumableUploadStrategy => onedrive_select_field(
            "provider_resumable_upload_strategy",
            vec![
                select_option(
                    "server_relay",
                    "provider_resumable_upload_strategy_server_relay",
                    Some("provider_resumable_upload_strategy_server_relay_desc"),
                ),
                select_option(
                    "frontend_direct",
                    "provider_resumable_upload_strategy_frontend_direct",
                    Some("provider_resumable_upload_strategy_frontend_direct_desc"),
                ),
            ],
            "server_relay",
        ),
        pub provider_download_strategy: ProviderDownloadStrategy => onedrive_select_field(
            "provider_download_strategy",
            vec![
                select_option(
                    "server_relay",
                    "provider_download_strategy_server_relay",
                    Some("provider_download_strategy_server_relay_desc"),
                ),
                select_option(
                    "frontend_direct",
                    "provider_download_strategy_frontend_direct",
                    Some("provider_download_strategy_frontend_direct_desc"),
                ),
            ],
            "server_relay",
        ),
        pub provider_download_filename_mode: ProviderDownloadFilenameMode => onedrive_select_field(
            "provider_download_filename_mode",
            vec![
                select_option(
                    "provider_native",
                    "provider_download_filename_mode_provider_native",
                    Some("provider_download_filename_mode_provider_native_desc"),
                ),
                select_option(
                    "strict_current",
                    "provider_download_filename_mode_strict_current",
                    Some("provider_download_filename_mode_strict_current_desc"),
                ),
            ],
            "provider_native",
        ),
        pub cloud: MicrosoftGraphCloud => onedrive_select_field(
            "cloud",
            vec![
                select_option("global", "onedrive_cloud_global", None),
                select_option("china", "onedrive_cloud_china", None),
            ],
            "global",
        ),
        pub account_mode: OneDriveAccountMode => onedrive_select_field(
            "account_mode",
            vec![
                select_option("personal", "onedrive_account_mode_personal", None),
                select_option(
                    "work_or_school",
                    "onedrive_account_mode_work_or_school",
                    None,
                ),
                select_option(
                    "sharepoint_site",
                    "onedrive_account_mode_sharepoint_site",
                    None,
                ),
                select_option("group_drive", "onedrive_account_mode_group_drive", None),
            ],
            "personal",
        ),
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub tenant: Option<String> => onedrive_optional_text_field("tenant"),
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub drive_id: Option<String> => onedrive_optional_text_field("drive_id"),
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub root_item_id: Option<String> => onedrive_optional_text_field("root_item_id"),
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub site_id: Option<String> => onedrive_optional_text_field("site_id"),
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub group_id: Option<String> => onedrive_optional_text_field("group_id"),
        }
        credentials authorization_application OneDriveAuthorizationApplicationV1 {
            pub client_id: String => storage_connector_field(
                "client_id", StorageConnectorFieldScope::AuthorizationApplication,
                StorageConnectorFieldKind::Text, true, false,
            ),
            pub client_secret: String => storage_connector_field(
                "client_secret", StorageConnectorFieldScope::AuthorizationApplication,
                StorageConnectorFieldKind::Secret, true, true,
            ),
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub scopes: Option<String> => storage_connector_field(
                "scopes", StorageConnectorFieldScope::AuthorizationApplication,
                StorageConnectorFieldKind::Text, false, false,
            ),
        }
    }
}

fn onedrive_select_field(
    name: &str,
    options: Vec<StorageConnectorSelectOptionInput<'static>>,
    default_value: &str,
) -> aster_drive_storage::StorageConnectorFieldDescriptor {
    let mut field = storage_connector_select_field(
        name,
        StorageConnectorFieldScope::ConnectorConfig,
        true,
        options,
    );
    field.default_value = Some(StorageConnectorFieldDefaultValue::String(
        default_value.to_string(),
    ));
    field
}

const fn select_option(
    value: &'static str,
    label_key: &'static str,
    description_key: Option<&'static str>,
) -> StorageConnectorSelectOptionInput<'static> {
    StorageConnectorSelectOptionInput {
        value,
        label_key,
        description_key,
    }
}

fn onedrive_optional_text_field(
    name: &str,
) -> aster_drive_storage::StorageConnectorFieldDescriptor {
    storage_connector_field(
        name,
        StorageConnectorFieldScope::ConnectorConfig,
        StorageConnectorFieldKind::Text,
        false,
        false,
    )
}

impl OneDriveConnector {
    pub const ID: &'static str = "asterdrive.storage.onedrive";

    pub(crate) fn decode_config(
        policy: &storage_policy::Model,
    ) -> Result<OneDriveConnectorConfigV1> {
        super::common::decode_typed_policy_config(policy, Self::ID, 1).map(|(config, _)| config)
    }

    pub(crate) async fn resolve_location(
        client: &MicrosoftGraphClient,
        policy: &storage_policy::Model,
    ) -> Result<OneDriveResolvedLocation> {
        let config = Self::decode_config(policy)?;
        let drive_id = match normalized_option(config.drive_id) {
            Some(value) => value,
            None => match config.account_mode {
                OneDriveAccountMode::Personal | OneDriveAccountMode::WorkOrSchool => {
                    client.get_me_drive().await?.id
                }
                OneDriveAccountMode::SharepointSite => {
                    let site_id = normalized_option(config.site_id).ok_or_else(|| {
                        AsterError::validation_error(
                            "OneDrive sharepoint_site policy missing onedrive_site_id",
                        )
                    })?;
                    client.get_site_drive(&site_id).await?.id
                }
                OneDriveAccountMode::GroupDrive => {
                    let group_id = normalized_option(config.group_id).ok_or_else(|| {
                        AsterError::validation_error(
                            "OneDrive group_drive policy missing onedrive_group_id",
                        )
                    })?;
                    client.get_group_drive(&group_id).await?.id
                }
            },
        };
        if drive_id.trim().is_empty() {
            return Err(AsterError::database_operation(
                "Microsoft Graph returned empty OneDrive drive id",
            ));
        }
        let root_item_id =
            normalized_option(config.root_item_id).unwrap_or_else(|| "root".to_string());
        let root_item = if root_item_id.eq_ignore_ascii_case("root") {
            client.get_drive_root(&drive_id).await?
        } else {
            client
                .get_drive_item_by_id(&drive_id, &root_item_id)
                .await?
        };
        Ok(OneDriveResolvedLocation {
            drive_id,
            root_item,
        })
    }

    async fn upsert_application_config<C: ConnectionTrait>(
        db: &C,
        encryption_key: &str,
        policy_id: i64,
        connector_config: &OneDriveConnectorConfigV1,
        input: OneDriveAuthorizationApplicationV1,
    ) -> Result<aster_drive_model::entities::storage_policy_connector_credential::Model> {
        let existing =
            storage_policy_connector_credential_repo::find_by_policy(db, policy_id).await?;
        let existing_payload = existing
            .as_ref()
            .map(|credential| {
                super::decode_typed_connector_credential::<OneDriveCredentialV1>(
                    encryption_key,
                    credential,
                    &aster_drive_storage::ConnectorId::declared(Self::ID),
                    1,
                )
            })
            .transpose()?;
        let client_id = normalized_option(Some(input.client_id))
            .or_else(|| {
                existing_payload
                    .as_ref()
                    .map(|payload| payload.application.client_id.clone())
            })
            .ok_or_else(|| AsterError::validation_error("client_id is required"))?;
        let client_secret = normalized_option(Some(input.client_secret))
            .or_else(|| {
                existing_payload
                    .as_ref()
                    .map(|payload| payload.application.client_secret.clone())
            })
            .ok_or_else(|| AsterError::validation_error("client_secret is required"))?;
        let existing_scopes = existing_payload
            .as_ref()
            .map(|payload| payload.application.scopes.clone());
        let default_scopes = default_microsoft_graph_scopes(connector_config);
        let scopes = match input
            .scopes
            .and_then(|value| normalized_option(Some(value)))
        {
            Some(scopes) => normalize_microsoft_graph_scopes(
                Some(scopes.split_whitespace().map(ToOwned::to_owned).collect()),
                default_scopes,
            ),
            None => existing_scopes
                .filter(|scopes| !scopes.is_empty())
                .unwrap_or_else(|| normalize_microsoft_graph_scopes(None, default_scopes)),
        };
        let tenant = normalized_option(connector_config.tenant.clone())
            .unwrap_or_else(|| "common".to_string());
        let application = OneDriveApplicationCredentialV1 {
            cloud: connector_config.cloud,
            tenant,
            client_id,
            client_secret,
            scopes,
        };
        let preserve_authorization = existing_payload
            .as_ref()
            .is_some_and(|payload| same_application_identity(&payload.application, &application));
        let payload = OneDriveCredentialV1 {
            application,
            authorization: preserve_authorization
                .then(|| existing_payload.and_then(|payload| payload.authorization))
                .flatten(),
        };
        super::persist_connector_credential_payload(
            db,
            encryption_key,
            policy_id,
            &aster_drive_storage::ConnectorId::declared(Self::ID),
            1,
            &payload,
        )
        .await?;
        storage_policy_connector_credential_repo::find_by_policy(db, policy_id)
            .await?
            .ok_or_else(|| {
                AsterError::record_not_found(
                    "OneDrive connector credential after application update",
                )
            })
    }

    fn validate_semantics(config: &OneDriveConnectorConfigV1) -> Result<()> {
        let non_empty =
            |value: Option<&String>| value.is_some_and(|value| !value.trim().is_empty());
        match config.account_mode {
            OneDriveAccountMode::SharepointSite if !non_empty(config.site_id.as_ref()) => {
                return Err(AsterError::validation_error(
                    "OneDrive sharepoint_site configuration requires site_id",
                ));
            }
            OneDriveAccountMode::GroupDrive if !non_empty(config.group_id.as_ref()) => {
                return Err(AsterError::validation_error(
                    "OneDrive group_drive configuration requires group_id",
                ));
            }
            _ => {}
        }
        if config.account_mode != OneDriveAccountMode::SharepointSite
            && non_empty(config.site_id.as_ref())
        {
            return Err(AsterError::validation_error(
                "OneDrive site_id is only valid for sharepoint_site account mode",
            ));
        }
        if config.account_mode != OneDriveAccountMode::GroupDrive
            && non_empty(config.group_id.as_ref())
        {
            return Err(AsterError::validation_error(
                "OneDrive group_id is only valid for group_drive account mode",
            ));
        }
        Ok(())
    }
}

fn same_application_identity(
    left: &OneDriveApplicationCredentialV1,
    right: &OneDriveApplicationCredentialV1,
) -> bool {
    left.cloud == right.cloud
        && left.tenant == right.tenant
        && left.client_id == right.client_id
        && left.client_secret == right.client_secret
        && left.scopes == right.scopes
}

fn normalized_option(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn normalize_microsoft_graph_scopes(
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

fn microsoft_graph_authorization_audit(
    cloud: MicrosoftGraphCloud,
    tenant: &str,
) -> StorageConnectorAuthorizationAudit {
    let mut fields = serde_json::Map::new();
    fields.insert(
        "cloud".to_string(),
        serde_json::to_value(cloud).unwrap_or(serde_json::Value::Null),
    );
    fields.insert(
        "tenant".to_string(),
        serde_json::Value::String(tenant.to_string()),
    );
    fields.insert(
        "client_secret_configured".to_string(),
        serde_json::Value::Bool(true),
    );
    StorageConnectorAuthorizationAudit {
        provider: StorageCredentialProvider::MicrosoftGraph,
        fields,
    }
}

fn default_microsoft_graph_scopes(config: &OneDriveConnectorConfigV1) -> &'static str {
    match config.account_mode {
        OneDriveAccountMode::Personal | OneDriveAccountMode::WorkOrSchool
            if config.drive_id.is_none() =>
        {
            "offline_access Files.ReadWrite"
        }
        OneDriveAccountMode::Personal | OneDriveAccountMode::WorkOrSchool => {
            "offline_access Files.ReadWrite.All"
        }
        OneDriveAccountMode::SharepointSite | OneDriveAccountMode::GroupDrive => {
            "offline_access Files.ReadWrite.All Sites.ReadWrite.All"
        }
    }
}

/// Complete persisted credential state owned by the OneDrive connector.
///
/// The whole value is encrypted by the generic connector credential store;
/// core persistence never interprets the Microsoft application or token
/// fields. Authorization updates replace this payload with revision-based CAS.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OneDriveCredentialV1 {
    pub(crate) application: OneDriveApplicationCredentialV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) authorization: Option<OneDriveAuthorizationCredentialV1>,
}

impl fmt::Debug for OneDriveCredentialV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OneDriveCredentialV1")
            .field("application", &self.application)
            .field("authorization", &self.authorization)
            .finish()
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OneDriveApplicationCredentialV1 {
    pub(crate) cloud: MicrosoftGraphCloud,
    pub(crate) tenant: String,
    pub(crate) client_id: String,
    pub(crate) client_secret: String,
    pub(crate) scopes: Vec<String>,
}

impl fmt::Debug for OneDriveApplicationCredentialV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OneDriveApplicationCredentialV1")
            .field("cloud", &self.cloud)
            .field("tenant", &self.tenant)
            .field("client_id", &self.client_id)
            .field("client_secret", &"***REDACTED***")
            .field("scopes", &self.scopes)
            .finish()
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OneDriveAuthorizationCredentialV1 {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) account_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) subject: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) tenant_id: Option<String>,
    pub(crate) scopes: Vec<String>,
    pub(crate) access_token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) refresh_token: Option<String>,
    pub(crate) metadata: OneDriveAuthorizationMetadataV1,
    pub(crate) status: StorageCredentialStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) status_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) expires_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) authorized_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) last_refreshed_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) last_validated_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl fmt::Debug for OneDriveAuthorizationCredentialV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OneDriveAuthorizationCredentialV1")
            .field("account_label", &self.account_label)
            .field("subject", &self.subject)
            .field("tenant_id", &self.tenant_id)
            .field("scopes", &self.scopes)
            .field("access_token", &"***REDACTED***")
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "***REDACTED***"),
            )
            .field("metadata", &self.metadata)
            .field("status", &self.status)
            .field("status_reason", &self.status_reason)
            .field("expires_at", &self.expires_at)
            .field("authorized_at", &self.authorized_at)
            .field("last_refreshed_at", &self.last_refreshed_at)
            .field("last_validated_at", &self.last_validated_at)
            .finish()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OneDriveAuthorizationMetadataV1 {
    pub(crate) cloud: MicrosoftGraphCloud,
    pub(crate) drive_id: String,
    pub(crate) root_item_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) root_item_name: Option<String>,
    #[serde(default)]
    pub(crate) id_token_present: bool,
}

impl OneDriveConnector {
    fn descriptor_definition() -> StorageConnectorDescriptor {
        let upload_capabilities = microsoft_graph_upload_capabilities();
        StorageConnectorDescriptor {
            connector_id: aster_drive_storage::ConnectorId::declared(Self::ID),
            label: "OneDrive / SharePoint".to_string(),
            description: "Microsoft Graph-backed OneDrive or SharePoint storage policy".to_string(),
            ui: storage_connector_ui_descriptor(StorageConnectorUiDescriptorInput {
                label_key: "driver_type_onedrive",
                description_key: "policy_wizard_onedrive_storage_desc",
                icon_src: Some("/static/storage/onedrive.svg"),
                icon_name: None,
                badge_rgb: StorageConnectorBadgeRgb::new(59, 130, 246),
                helper_key: "policy_wizard_onedrive_helper",
                config_step_title_key: "policy_wizard_step_onedrive_title",
                config_step_description_key: "policy_wizard_step_onedrive_desc",
                edit_context_key: "policy_edit_context_onedrive_desc",
                base_path_empty_display: "core:root",
                base_path_placeholder: "tenant/prefix",
            }),
            credential_mode: OneDriveConnectorConfigV1::credential_mode(),
            deployment_scope: StorageConnectorDeploymentScope::SharedAcrossPrimaryInstances,
            supports_initial_setup: false,
            requires_authorization: true,
            authorization_provider: Some("microsoft_graph".to_string()),
            credential_management: Some(StorageConnectorCredentialManagementDescriptor {
                title_key: "onedrive_credential_title".to_string(),
                loading_key: "onedrive_credential_loading".to_string(),
                status_keys: BTreeMap::from([
                    (
                        "authorized".to_string(),
                        "onedrive_credential_status_authorized".to_string(),
                    ),
                    (
                        "reauth_required".to_string(),
                        "onedrive_credential_status_reauth_required".to_string(),
                    ),
                    (
                        "permission_denied".to_string(),
                        "onedrive_credential_status_permission_denied".to_string(),
                    ),
                    (
                        "revoked".to_string(),
                        "onedrive_credential_status_revoked".to_string(),
                    ),
                    (
                        "invalid".to_string(),
                        "onedrive_credential_status_invalid".to_string(),
                    ),
                    (
                        "missing".to_string(),
                        "onedrive_credential_status_missing".to_string(),
                    ),
                ]),
                redirect_uri_key: Some("onedrive_redirect_uri".to_string()),
                save_before_authorize_key: Some("onedrive_save_before_authorize".to_string()),
                authorization_started_key: Some("onedrive_authorization_started".to_string()),
                save_before_validate_key: Some("onedrive_save_before_validate".to_string()),
                validation_success_key: Some("onedrive_validation_success".to_string()),
                validation_success_detail_key: Some("onedrive_validation_success_root".to_string()),
                created_authorize_next_key: Some(
                    "policy_connector_created_authorize_next".to_string(),
                ),
            }),
            capabilities: StorageConnectorCapabilities {
                efficient_range: true,
                capacity: true,
                list: false,
                presigned_download: true,
                storage_native_thumbnail: false,
                storage_native_media_metadata: false,
                remote_node_binding: false,
                object_storage_transfer_strategy: false,
                object_naming: StorageConnectorObjectNamingMode::OriginalFilename,
            },
            upload_workflows: StorageConnectorUploadWorkflows {
                simple_upload: true,
                simple_upload_capabilities: server_relay_simple_upload_capabilities(
                    upload_capabilities.max_simple_upload_size,
                ),
                stream_upload: true,
                object_multipart_upload: false,
                object_multipart_upload_capabilities: None,
                provider_resumable_upload: true,
                presigned_upload: false,
                frontend_direct_provider_resumable_upload: true,
                provider_resumable_upload_capabilities: Some(
                    StorageConnectorProviderResumableUploadCapabilities {
                        provider: upload_capabilities.provider.to_string(),
                        session_label: upload_capabilities.session_label.to_string(),
                        min_fragment_size: upload_capabilities.min_fragment_size,
                        default_fragment_size: upload_capabilities.default_fragment_size,
                        max_fragment_size: upload_capabilities.max_fragment_size,
                        fragment_alignment: upload_capabilities.fragment_alignment,
                        max_simple_upload_size: upload_capabilities.max_simple_upload_size,
                        frontend_direct_upload: upload_capabilities.frontend_direct_upload,
                        implicit_completion: upload_capabilities.implicit_completion,
                        abort_supported: upload_capabilities.abort_supported,
                        status_query_supported: upload_capabilities.status_query_supported,
                    },
                ),
            },
            fields: OneDriveConnectorConfigV1::descriptor_fields(),
            config_schema_version: 1,
            credential_schema_version: Some(1),
            actions: vec![
                start_authorization_action_descriptor(),
                validate_credential_action_descriptor(),
                saved_connection_test_action_descriptor(true),
            ],
            related_issues: vec![328, 329, 330, 349],
        }
    }
}

#[async_trait]
impl StorageConnector for OneDriveConnector {
    fn descriptor(&self) -> StorageConnectorDescriptor {
        Self::descriptor_definition()
    }

    fn localization(&self) -> Result<aster_drive_storage::StorageConnectorLocalization> {
        let descriptor = Self::descriptor_definition();
        super::localization::builtin_connector_localization(
            Self::ID,
            &descriptor,
            localization::MESSAGES,
        )
    }

    fn validate_connector_config(
        &self,
        input: &aster_drive_storage::ConnectorConfigEnvelope,
    ) -> Result<aster_drive_storage::ConnectorConfigEnvelope> {
        let normalized =
            aster_drive_storage::connector_descriptor::normalize_storage_connector_config(
                &self.descriptor(),
                input,
            )
            .map_err(|error| AsterError::validation_error(error.to_string()))?;
        let config: OneDriveConnectorConfigV1 =
            super::common::decode_normalized_connector_config(&normalized)?;
        Self::validate_semantics(&config)?;
        super::common::encode_normalized_connector_config(
            normalized.connector_id,
            normalized.schema_version,
            config,
        )
    }

    fn validate_credential_input(&self, input: &StorageConnectorCredentialInput) -> Result<()> {
        let credential: OneDriveAuthorizationApplicationV1 =
            super::common::decode_authorization_application(input, Self::ID)?;
        super::common::validate_required_credential_field(
            &credential.client_id,
            "client_id",
            Self::ID,
        )?;
        super::common::validate_required_credential_field(
            &credential.client_secret,
            "client_secret",
            Self::ID,
        )
    }

    /// AsterDrive 0.5.0-only legacy import; remove with the deprecated stores
    /// and trait hook in AsterDrive 0.6.0.
    #[expect(
        deprecated,
        reason = "AsterDrive 0.5.0 OneDrive credential import is removed in 0.6.0"
    )]
    fn import_legacy_credential(
        &self,
        encryption_key: &str,
        policy: &storage_policy::Model,
        input: super::LegacyStorageConnectorCredentialInput,
    ) -> Result<Option<serde_json::Value>> {
        if input.static_credential.is_some() {
            return Err(AsterError::database_operation(format!(
                "OneDrive storage policy {} contains incompatible legacy static credentials",
                policy.id
            )));
        }

        let Some(application) = input.application_config else {
            if input.authorization.is_some() {
                return Err(AsterError::database_operation(format!(
                    "OneDrive storage policy {} has legacy OAuth credentials without application credentials",
                    policy.id
                )));
            }
            return Ok(None);
        };
        if application.provider != StorageCredentialProvider::MicrosoftGraph {
            return Err(AsterError::database_operation(format!(
                "OneDrive storage policy {} has incompatible legacy application provider '{}'",
                policy.id,
                application.provider.as_str()
            )));
        }

        let connector_config = Self::decode_config(policy)?;
        let client_id = required_legacy_value(
            application.client_id,
            policy.id,
            "Microsoft Graph client_id",
        )?;
        let client_secret_ciphertext = required_legacy_value(
            application.client_secret_ciphertext,
            policy.id,
            "Microsoft Graph client_secret ciphertext",
        )?;
        let client_secret = oauth::decrypt_application_client_secret(
            encryption_key,
            policy.id,
            &client_secret_ciphertext,
        )?;
        let client_secret = required_legacy_value(
            Some(client_secret.expose_secret().to_string()),
            policy.id,
            "Microsoft Graph client_secret",
        )?;
        let tenant = normalized_legacy_value(application.tenant_id)
            .or_else(|| normalized_legacy_value(connector_config.tenant.clone()))
            .unwrap_or_else(|| "common".to_string());
        let default_scopes = default_microsoft_graph_scopes(&connector_config);
        let application_scopes = parse_legacy_scopes(
            &application.scopes,
            policy.id,
            "Microsoft Graph application scopes",
        )?;
        let application_scopes = normalize_microsoft_graph_scopes(
            (!application_scopes.is_empty()).then_some(application_scopes),
            default_scopes,
        );
        let application = OneDriveApplicationCredentialV1 {
            cloud: connector_config.cloud,
            tenant: tenant.clone(),
            client_id,
            client_secret,
            scopes: application_scopes.clone(),
        };

        let authorization = input
            .authorization
            .map(|authorization| {
                import_legacy_onedrive_authorization(
                    encryption_key,
                    policy,
                    &connector_config,
                    &application,
                    authorization,
                )
            })
            .transpose()?;
        serde_json::to_value(OneDriveCredentialV1 {
            application,
            authorization,
        })
        .map(Some)
        .map_err(|error| {
            AsterError::database_operation(format!(
                "serialize migrated OneDrive credential for policy {}: {error}",
                policy.id
            ))
        })
    }

    async fn persist_credential(
        &self,
        db: &sea_orm::DatabaseTransaction,
        encryption_key: &str,
        policy_id: i64,
        connector_config: &aster_drive_storage::ConnectorConfigEnvelope,
        credential: StorageConnectorCredentialInput,
    ) -> Result<()> {
        let application: OneDriveAuthorizationApplicationV1 =
            super::common::decode_authorization_application(&credential, Self::ID)?;
        let config: OneDriveConnectorConfigV1 =
            super::common::decode_normalized_connector_config(connector_config)?;
        Self::upsert_application_config(db, encryption_key, policy_id, &config, application)
            .await?;
        Ok(())
    }

    async fn start_authorization(
        &self,
        context: &super::StorageConnectorContext<'_>,
        policy: &storage_policy::Model,
        redirect_uri: &str,
    ) -> Result<StorageConnectorAuthorizationStart> {
        let credential = storage_policy_connector_credential_repo::find_by_policy(
            context.writer_db(),
            policy.id,
        )
        .await?
        .ok_or_else(|| {
            AsterError::validation_error(
                "save the OneDrive authorization application before starting authorization",
            )
        })?;
        let payload: OneDriveCredentialV1 = super::decode_typed_connector_credential(
            &context.config().auth.storage_credential_secret_key,
            &credential,
            &aster_drive_storage::ConnectorId::declared(Self::ID),
            1,
        )?;
        let application = payload.application;
        let client_id = normalized_option(Some(application.client_id))
            .ok_or_else(|| AsterError::validation_error("Microsoft Graph client_id is required"))?;
        let client_secret =
            normalized_option(Some(application.client_secret)).ok_or_else(|| {
                AsterError::validation_error(
                    "Microsoft Graph client_secret is required for storage authorization",
                )
            })?;
        let tenant =
            normalized_option(Some(application.tenant)).unwrap_or_else(|| "common".to_string());
        let connector_config = Self::decode_config(policy)?;
        let scopes = normalize_microsoft_graph_scopes(
            (!application.scopes.is_empty()).then_some(application.scopes),
            default_microsoft_graph_scopes(&connector_config),
        );
        let state = format!("storage_oauth_{}", id::new_short_token());
        let state_hash = crate::services::storage_policy::credential::crypto::token_hash(&state);
        let pkce_verifier = oauth::build_pkce_verifier();
        let pkce_challenge = oauth::build_pkce_challenge(&pkce_verifier);
        let authorization_url = oauth::microsoft_authorization_url(
            application.cloud,
            &tenant,
            &client_id,
            redirect_uri,
            &scopes,
            &state,
            &pkce_challenge,
        )?;
        let client_secret_ciphertext =
            crate::services::storage_policy::credential::crypto::encrypt_token(
                &context.config().auth.storage_credential_secret_key,
                oauth::flow_client_secret_aad(policy.id, &state_hash).as_bytes(),
                &client_secret,
            )?;
        let flow_context = oauth::MicrosoftGraphFlowContext {
            cloud: application.cloud,
            tenant: tenant.clone(),
            client_id: client_id.clone(),
            client_secret_ciphertext: Some(client_secret_ciphertext),
            scopes: scopes.clone(),
        };
        let context_json = serde_json::to_string(&flow_context).map_err(|error| {
            AsterError::internal_error(format!("serialize OneDrive authorization context: {error}"))
        })?;
        Ok(StorageConnectorAuthorizationStart {
            provider: StorageCredentialProvider::MicrosoftGraph,
            authorization_url,
            expires_in: AUTHORIZATION_FLOW_TTL_SECS,
            state,
            pkce_verifier: Some(pkce_verifier),
            scopes,
            context: context_json,
            audit: microsoft_graph_authorization_audit(application.cloud, &tenant),
        })
    }

    async fn finish_authorization(
        &self,
        _context: &super::StorageConnectorContext<'_>,
        policy: &storage_policy::Model,
        flow: &aster_drive_model::entities::storage_policy_authorization_flow::Model,
        code: &str,
        now: chrono::DateTime<chrono::Utc>,
    ) -> std::result::Result<
        StorageConnectorAuthorizationCallback,
        StorageConnectorAuthorizationError,
    > {
        if flow.provider != StorageCredentialProvider::MicrosoftGraph {
            return Err(StorageConnectorAuthorizationError::new(
                StorageAuthorizationFailureReason::UnsupportedProvider,
                AsterError::unsupported_driver(format!(
                    "OneDrive authorization flow has unsupported provider '{}'",
                    flow.provider.as_str()
                )),
            ));
        }
        let flow_context = serde_json::from_str::<oauth::MicrosoftGraphFlowContext>(&flow.context)
            .map_err(|error| {
                StorageConnectorAuthorizationError::new(
                    StorageAuthorizationFailureReason::ServerError,
                    AsterError::database_operation(format!(
                        "invalid OneDrive authorization context: {error}"
                    )),
                )
            })?;
        let pkce_verifier = flow.pkce_verifier.as_deref().ok_or_else(|| {
            StorageConnectorAuthorizationError::new(
                StorageAuthorizationFailureReason::ServerError,
                AsterError::database_operation(
                    "OneDrive authorization flow is missing PKCE verifier",
                ),
            )
        })?;
        let client_secret_ciphertext = flow_context
            .client_secret_ciphertext
            .as_deref()
            .ok_or_else(|| {
                StorageConnectorAuthorizationError::new(
                    StorageAuthorizationFailureReason::InvalidRequest,
                    AsterError::validation_error(
                        "Microsoft Graph client_secret is required for storage authorization",
                    ),
                )
            })?;
        let client_secret = crate::services::storage_policy::credential::crypto::decrypt_token(
            &_context.config().auth.storage_credential_secret_key,
            oauth::flow_client_secret_aad(policy.id, &flow.state_hash).as_bytes(),
            client_secret_ciphertext,
        )
        .map(SecretString::from)
        .map_err(|error| {
            StorageConnectorAuthorizationError::new(
                StorageAuthorizationFailureReason::ServerError,
                error,
            )
        })?;
        let token = oauth::exchange_microsoft_graph_code(
            &flow_context,
            Some(&client_secret),
            code,
            &flow.redirect_uri,
            pkce_verifier,
        )
        .await
        .map_err(|error| {
            StorageConnectorAuthorizationError::new(
                StorageAuthorizationFailureReason::TokenExchangeFailed,
                error,
            )
        })?;
        let graph_client = MicrosoftGraphClient::new(MicrosoftGraphClientConfig::new(
            flow_context.cloud.graph_base_url(),
            token.access_token.clone(),
        ))
        .map_err(|error| {
            StorageConnectorAuthorizationError::new(
                StorageAuthorizationFailureReason::ServerError,
                error.into(),
            )
        })?;
        let location = Self::resolve_location(&graph_client, policy)
            .await
            .map_err(|error| {
                StorageConnectorAuthorizationError::new(
                    StorageAuthorizationFailureReason::DriveResolutionFailed,
                    error,
                )
            })?;
        let expires_at = token
            .expires_in
            .and_then(|seconds| (seconds > 0).then(|| now + chrono::Duration::seconds(seconds)));
        let granted_scopes = token
            .scope
            .as_deref()
            .map(|scope| {
                normalize_microsoft_graph_scopes(
                    Some(scope.split_whitespace().map(ToOwned::to_owned).collect()),
                    "",
                )
            })
            .filter(|scopes| !scopes.is_empty())
            .unwrap_or_else(|| flow_context.scopes.clone());
        let root_item = location.root_item;
        let payload = OneDriveCredentialV1 {
            application: OneDriveApplicationCredentialV1 {
                cloud: flow_context.cloud,
                tenant: flow_context.tenant.clone(),
                client_id: flow_context.client_id,
                client_secret: client_secret.expose_secret().to_string(),
                scopes: flow_context.scopes,
            },
            authorization: Some(OneDriveAuthorizationCredentialV1 {
                account_label: root_item.name.clone(),
                subject: Some(root_item.id.clone()),
                tenant_id: Some(flow_context.tenant.clone()),
                scopes: granted_scopes,
                access_token: token.access_token,
                refresh_token: token.refresh_token.filter(|value| !value.trim().is_empty()),
                metadata: OneDriveAuthorizationMetadataV1 {
                    cloud: flow_context.cloud,
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
            }),
        };
        let credential_payload = serde_json::to_value(payload).map_err(|error| {
            StorageConnectorAuthorizationError::new(
                StorageAuthorizationFailureReason::ServerError,
                AsterError::internal_error(format!(
                    "serialize OneDrive authorization credential: {error}"
                )),
            )
        })?;
        Ok(StorageConnectorAuthorizationCallback {
            credential_payload,
            audit: microsoft_graph_authorization_audit(flow_context.cloud, &flow_context.tenant),
        })
    }

    async fn build_draft_driver(
        &self,
        context: &super::StorageConnectorContext<'_>,
        policy: &storage_policy::Model,
        credential: &StorageConnectorCredentialInput,
    ) -> Result<Box<dyn StorageDriver>> {
        let _ = (context, policy, credential);
        Err(unsupported_draft_connection_test_error(self.descriptor()))
    }

    fn build_runtime_driver(
        &self,
        registry: &crate::storage::DriverRegistry,
        policy: &storage_policy::Model,
    ) -> Result<super::StorageConnectorDriver> {
        let credential = registry.get_runtime_credential(policy.id).ok_or_else(|| {
            storage_driver_error(
                aster_drive_storage::StorageErrorKind::Auth,
                format!(
                    "OneDrive storage policy {} is missing authorized Microsoft Graph credentials",
                    policy.id
                ),
            )
        })?;
        Ok(super::StorageConnectorDriver::storage(
            self.build_authorized_driver(policy, credential)?,
        ))
    }

    fn upload_transport(
        &self,
        policy: &storage_policy::Model,
    ) -> Result<StorageConnectorUploadTransport> {
        let config = Self::decode_config(policy)?;
        Ok(StorageConnectorUploadTransport::ProviderResumable(
            config.provider_resumable_upload_strategy,
        ))
    }

    fn presigned_download_enabled(&self, policy: &storage_policy::Model) -> Result<bool> {
        let config = Self::decode_config(policy)?;
        Ok(config.provider_download_strategy == ProviderDownloadStrategy::FrontendDirect)
    }

    fn presigned_download_requires_filename_match(
        &self,
        policy: &storage_policy::Model,
    ) -> Result<bool> {
        let config = Self::decode_config(policy)?;
        Ok(config.provider_download_filename_mode == ProviderDownloadFilenameMode::StrictCurrent)
    }

    fn credential_info(
        &self,
        config: &crate::config::Config,
        credential: &aster_drive_model::entities::storage_policy_connector_credential::Model,
    ) -> Result<Option<super::StorageConnectorCredentialInfo>> {
        let payload: OneDriveCredentialV1 = super::decode_typed_connector_credential(
            &config.auth.storage_credential_secret_key,
            credential,
            &aster_drive_storage::ConnectorId::declared(Self::ID),
            1,
        )?;
        let Some(authorization) = payload.authorization else {
            return Ok(None);
        };
        Ok(Some(super::StorageConnectorCredentialInfo {
            id: credential.id,
            policy_id: credential.policy_id,
            provider: StorageCredentialProvider::MicrosoftGraph,
            credential_kind: StorageCredentialKind::OauthDelegated,
            account_label: authorization.account_label,
            subject: authorization.subject,
            tenant_id: authorization.tenant_id,
            scopes: authorization.scopes,
            status: authorization.status,
            status_reason: authorization.status_reason,
            expires_at: authorization.expires_at,
            authorized_at: authorization.authorized_at,
            last_refreshed_at: authorization.last_refreshed_at,
            last_validated_at: authorization.last_validated_at,
            created_at: credential.created_at,
            updated_at: credential.updated_at,
        }))
    }

    fn credential_validation_failure_payload(
        &self,
        config: &crate::config::Config,
        credential: &aster_drive_model::entities::storage_policy_connector_credential::Model,
        error_kind: Option<aster_drive_storage::StorageErrorKind>,
        reason: &str,
    ) -> Result<Option<serde_json::Value>> {
        let Some(status) = (match error_kind {
            Some(aster_drive_storage::StorageErrorKind::Auth) => {
                Some(StorageCredentialStatus::ReauthRequired)
            }
            Some(aster_drive_storage::StorageErrorKind::Permission) => {
                Some(StorageCredentialStatus::PermissionDenied)
            }
            Some(aster_drive_storage::StorageErrorKind::Misconfigured) => {
                Some(StorageCredentialStatus::Invalid)
            }
            _ => None,
        }) else {
            return Ok(None);
        };
        let mut payload: OneDriveCredentialV1 = super::decode_typed_connector_credential(
            &config.auth.storage_credential_secret_key,
            credential,
            &aster_drive_storage::ConnectorId::declared(Self::ID),
            1,
        )?;
        let Some(authorization) = payload.authorization.as_mut() else {
            return Ok(None);
        };
        authorization.status = status;
        authorization.status_reason = Some(reason.to_string());
        serde_json::to_value(payload).map(Some).map_err(|error| {
            AsterError::internal_error(format!(
                "serialize OneDrive credential validation failure: {error}"
            ))
        })
    }

    async fn load_runtime_credential(
        &self,
        db: &sea_orm::DatabaseConnection,
        config: &crate::config::Config,
        policy: &storage_policy::Model,
        credential: &aster_drive_model::entities::storage_policy_connector_credential::Model,
    ) -> Result<Option<StorageConnectorRuntimeCredential>> {
        let payload: OneDriveCredentialV1 = super::decode_typed_connector_credential(
            &config.auth.storage_credential_secret_key,
            credential,
            &aster_drive_storage::ConnectorId::declared(Self::ID),
            1,
        )?;
        let Some(authorization) = payload.authorization.as_ref() else {
            return Ok(None);
        };
        if authorization.status != StorageCredentialStatus::Authorized {
            return Ok(None);
        }
        let drive_id = Some(authorization.metadata.drive_id.clone());
        let root_item_id = Some(authorization.metadata.root_item_id.clone());
        let token_provider = match provider::build_microsoft_graph_credential_token_provider(
            db.clone(),
            config.auth.storage_credential_secret_key.clone(),
            policy,
            credential,
            payload.clone(),
        ) {
            Ok(token_provider) => token_provider,
            Err(error) => {
                tracing::warn!(
                    policy_id = credential.policy_id,
                    credential_id = credential.id,
                    error = %error,
                    "skipping OneDrive credential reload because token provider initialization failed"
                );
                return Ok(None);
            }
        };
        Ok(Some(StorageConnectorRuntimeCredential::new(
            aster_drive_storage::ConnectorId::declared(Self::ID),
            OneDriveRuntimeCredential {
                token_provider,
                drive_id,
                root_item_id,
            },
        )))
    }

    fn build_authorized_driver(
        &self,
        policy: &storage_policy::Model,
        credential: StorageConnectorRuntimeCredential,
    ) -> Result<Arc<dyn StorageDriver>> {
        let credential = credential.require::<OneDriveRuntimeCredential>(Self::ID)?;
        let config = Self::decode_config(policy)?;
        let drive_id = config
            .drive_id
            .clone()
            .and_then(non_empty_string)
            .or_else(|| credential.drive_id.clone().and_then(non_empty_string))
            .ok_or_else(|| {
                storage_driver_error(
                    aster_drive_storage::StorageErrorKind::Misconfigured,
                    "OneDrive storage policy missing resolved drive_id; reauthorize Microsoft Graph",
                )
            })?;
        let configured_root_item_id = config
            .root_item_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let root_item_id = configured_root_item_id
            .filter(|value| !value.eq_ignore_ascii_case("root"))
            .map(ToOwned::to_owned)
            .or_else(|| credential.root_item_id.clone().and_then(non_empty_string))
            .or_else(|| configured_root_item_id.map(ToOwned::to_owned))
            .ok_or_else(|| {
                aster_drive_storage::error::storage_driver_error(
                    aster_drive_storage::StorageErrorKind::Misconfigured,
                    "OneDrive storage policy missing resolved root_item_id; reauthorize Microsoft Graph",
                )
            })?;
        if root_item_id.trim().is_empty() {
            return Err(storage_driver_error(
                aster_drive_storage::StorageErrorKind::Misconfigured,
                "OneDrive storage policy missing resolved root_item_id; reauthorize Microsoft Graph",
            ));
        }
        if drive_id.trim().is_empty() {
            return Err(storage_driver_error(
                aster_drive_storage::StorageErrorKind::Misconfigured,
                "OneDrive storage policy missing resolved drive_id; reauthorize Microsoft Graph",
            ));
        }
        let client = MicrosoftGraphClient::new(MicrosoftGraphClientConfig::with_token_provider(
            config.cloud.graph_base_url(),
            credential.token_provider.clone(),
        ))?;
        Ok(Arc::new(OneDriveDriver::new(
            client,
            drive_id,
            root_item_id,
            config.base_path,
            policy.chunk_size,
        )))
    }

    async fn validate_credential(
        &self,
        db: &sea_orm::DatabaseConnection,
        config: &crate::config::Config,
        policy: &storage_policy::Model,
        credential: &aster_drive_model::entities::storage_policy_connector_credential::Model,
    ) -> Result<StorageCredentialValidationOutcome> {
        let connector_config = Self::decode_config(policy)?;
        let mut payload: OneDriveCredentialV1 = super::decode_typed_connector_credential(
            &config.auth.storage_credential_secret_key,
            credential,
            &aster_drive_storage::ConnectorId::declared(Self::ID),
            1,
        )?;
        let token_provider = provider::build_microsoft_graph_credential_token_provider(
            db.clone(),
            config.auth.storage_credential_secret_key.clone(),
            policy,
            credential,
            payload.clone(),
        )?;
        let client = MicrosoftGraphClient::new(MicrosoftGraphClientConfig::with_token_provider(
            connector_config.cloud.graph_base_url(),
            token_provider,
        ))?;
        let location = Self::resolve_location(&client, policy).await?;
        let root_item = location.root_item;

        // Token resolution may rotate access and refresh tokens through a
        // revision CAS. Re-read the row before updating validation metadata so
        // this request never writes the pre-rotation payload back over the
        // provider's newer credential state.
        let current_credential =
            storage_policy_connector_credential_repo::find_by_policy(db, policy.id)
                .await?
                .ok_or_else(|| {
                    AsterError::record_not_found("storage policy connector credential")
                })?;
        payload = super::decode_typed_connector_credential(
            &config.auth.storage_credential_secret_key,
            &current_credential,
            &aster_drive_storage::ConnectorId::declared(Self::ID),
            1,
        )?;
        let authorization = payload.authorization.as_mut().ok_or_else(|| {
            AsterError::validation_error("OneDrive connector credential has not been authorized")
        })?;
        authorization.account_label = root_item.name.clone();
        authorization.subject = Some(root_item.id.clone());
        authorization.status = StorageCredentialStatus::Authorized;
        authorization.status_reason = None;
        authorization.last_validated_at = Some(chrono::Utc::now());
        authorization.metadata = OneDriveAuthorizationMetadataV1 {
            cloud: connector_config.cloud,
            drive_id: location.drive_id.clone(),
            root_item_id: root_item.id.clone(),
            root_item_name: root_item.name.clone(),
            id_token_present: authorization.metadata.id_token_present,
        };
        Ok(StorageCredentialValidationOutcome {
            credential: current_credential,
            credential_payload: serde_json::to_value(payload).map_err(|error| {
                AsterError::internal_error(format!(
                    "serialize validated OneDrive connector credential: {error}"
                ))
            })?,
            root_item_id: root_item.id,
            root_item_name: root_item.name,
        })
    }

    async fn cleanup_snapshot_for_policy(
        &self,
        context: &super::StorageConnectorContext<'_>,
        policy: &storage_policy::Model,
    ) -> Result<Option<StoragePolicyCleanupDriverSnapshot>> {
        onedrive_credential_snapshot_for_policy(context.writer_db(), context.config(), policy)
            .await
            .and_then(|snapshot| {
                snapshot
                    .map(|snapshot| {
                        StoragePolicyCleanupDriverSnapshot::encode(
                            aster_drive_storage::ConnectorId::declared(Self::ID),
                            CLEANUP_SNAPSHOT_SCHEMA_VERSION,
                            &snapshot,
                        )
                    })
                    .transpose()
            })
    }

    fn cleanup_snapshot_required(&self) -> bool {
        true
    }

    async fn build_cleanup_driver(
        &self,
        context: &super::StorageConnectorContext<'_>,
        policy: &storage_policy::Model,
        snapshots: StoragePolicyCleanupSnapshots<'_>,
    ) -> Result<Arc<dyn StorageDriver>> {
        let credential = onedrive_snapshot_from_cleanup_input(snapshots)?;
        let connector_config = Self::decode_config(policy)?;
        let token_provider = provider::build_microsoft_graph_cleanup_token_provider(
            context.config().auth.storage_credential_secret_key.clone(),
            policy,
            provider::MicrosoftGraphCleanupTokenSnapshot {
                cloud: credential.cloud,
                tenant_id: credential.tenant_id.clone(),
                client_id: credential.client_id.clone(),
                client_secret_ciphertext: credential.client_secret_ciphertext.clone(),
                access_token_ciphertext: credential.access_token_ciphertext.clone(),
                refresh_token_ciphertext: credential.refresh_token_ciphertext.clone(),
                expires_at: credential.expires_at,
            },
        )?;
        let client = MicrosoftGraphClient::new(MicrosoftGraphClientConfig::with_token_provider(
            credential.cloud.graph_base_url(),
            token_provider,
        ))?;
        Ok(Arc::new(OneDriveDriver::new(
            client,
            credential.drive_id.clone(),
            credential.root_item_id.clone(),
            connector_config.base_path,
            policy.chunk_size,
        )))
    }
}

async fn onedrive_credential_snapshot_for_policy(
    db: &sea_orm::DatabaseConnection,
    config: &crate::config::Config,
    policy: &storage_policy::Model,
) -> Result<Option<OneDriveCleanupSnapshotV1>> {
    let Some(credential) =
        storage_policy_connector_credential_repo::find_by_policy(db, policy.id).await?
    else {
        tracing::warn!(
            policy_id = policy.id,
            "OneDrive storage policy cleanup missing credential snapshot; skipping deferred cleanup"
        );
        return Ok(None);
    };
    let payload: OneDriveCredentialV1 = super::decode_typed_connector_credential(
        &config.auth.storage_credential_secret_key,
        &credential,
        &aster_drive_storage::ConnectorId::declared(OneDriveConnector::ID),
        1,
    )?;
    let Some(authorization) = payload.authorization else {
        tracing::warn!(
            policy_id = policy.id,
            "OneDrive storage policy cleanup missing authorization snapshot; skipping deferred cleanup"
        );
        return Ok(None);
    };
    if authorization.status != StorageCredentialStatus::Authorized {
        tracing::warn!(
            policy_id = policy.id,
            status = ?authorization.status,
            "OneDrive storage policy credential is not authorized; skipping deferred cleanup"
        );
        return Ok(None);
    }
    let Some(access_token) = non_empty_string(authorization.access_token) else {
        tracing::warn!(
            policy_id = policy.id,
            "OneDrive storage policy cleanup missing access token snapshot; skipping deferred cleanup"
        );
        return Ok(None);
    };
    let Some(client_id) = non_empty_string(payload.application.client_id) else {
        tracing::warn!(
            policy_id = policy.id,
            "OneDrive storage policy cleanup missing client_id snapshot; skipping deferred cleanup"
        );
        return Ok(None);
    };
    let Some(client_secret) = non_empty_string(payload.application.client_secret) else {
        tracing::warn!(
            policy_id = policy.id,
            "OneDrive storage policy cleanup missing client secret snapshot; skipping deferred cleanup"
        );
        return Ok(None);
    };
    let Some(drive_id) = non_empty_string(authorization.metadata.drive_id) else {
        tracing::warn!(
            policy_id = policy.id,
            "OneDrive storage policy cleanup missing drive_id snapshot; skipping deferred cleanup"
        );
        return Ok(None);
    };
    let Some(root_item_id) = non_empty_string(authorization.metadata.root_item_id) else {
        tracing::warn!(
            policy_id = policy.id,
            "OneDrive storage policy cleanup missing root_item_id snapshot; skipping deferred cleanup"
        );
        return Ok(None);
    };
    let access_token_ciphertext =
        crate::services::storage_policy::credential::crypto::encrypt_token(
            &config.auth.storage_credential_secret_key,
            crate::services::storage_policy::credential::crypto::token_aad(
                policy.id,
                StorageCredentialProvider::MicrosoftGraph.as_str(),
                "access",
            )
            .as_bytes(),
            &access_token,
        )?;
    let refresh_token_ciphertext = authorization
        .refresh_token
        .and_then(non_empty_string)
        .map(|refresh_token| {
            crate::services::storage_policy::credential::crypto::encrypt_token(
                &config.auth.storage_credential_secret_key,
                crate::services::storage_policy::credential::crypto::token_aad(
                    policy.id,
                    StorageCredentialProvider::MicrosoftGraph.as_str(),
                    "refresh",
                )
                .as_bytes(),
                &refresh_token,
            )
        })
        .transpose()?;
    let client_secret_ciphertext = oauth::encrypt_application_client_secret(
        &config.auth.storage_credential_secret_key,
        policy.id,
        &client_secret,
    )?;

    Ok(Some(OneDriveCleanupSnapshotV1 {
        cloud: authorization.metadata.cloud,
        tenant_id: authorization
            .tenant_id
            .and_then(non_empty_string)
            .or_else(|| non_empty_string(payload.application.tenant)),
        client_id: Some(client_id),
        client_secret_ciphertext: Some(client_secret_ciphertext),
        drive_id,
        root_item_id,
        access_token_ciphertext,
        refresh_token_ciphertext,
        expires_at: authorization.expires_at,
    }))
}

fn onedrive_snapshot_from_cleanup_input(
    snapshots: StoragePolicyCleanupSnapshots<'_>,
) -> Result<OneDriveCleanupSnapshotV1> {
    snapshots
        .driver_snapshot
        .ok_or_else(|| {
            AsterError::validation_error(
                "OneDrive storage policy cleanup missing credential snapshot",
            )
        })?
        .decode(OneDriveConnector::ID, CLEANUP_SNAPSHOT_SCHEMA_VERSION)
}

fn non_empty_string(value: String) -> Option<String> {
    let value = value.trim().to_string();
    if value.is_empty() { None } else { Some(value) }
}

fn normalized_legacy_value(value: Option<String>) -> Option<String> {
    value.and_then(non_empty_string)
}

fn required_legacy_value(value: Option<String>, policy_id: i64, field: &str) -> Result<String> {
    normalized_legacy_value(value).ok_or_else(|| {
        AsterError::database_operation(format!(
            "OneDrive storage policy {policy_id} is missing legacy {field}"
        ))
    })
}

fn parse_legacy_scopes(raw: &str, policy_id: i64, field: &str) -> Result<Vec<String>> {
    if raw.trim().is_empty() {
        return Ok(Vec::new());
    }
    let scopes = serde_json::from_str::<Vec<String>>(raw).map_err(|error| {
        AsterError::database_operation(format!(
            "OneDrive storage policy {policy_id} has invalid legacy {field}: {error}"
        ))
    })?;
    Ok(scopes
        .into_iter()
        .filter_map(non_empty_string)
        .fold(Vec::new(), |mut normalized, scope| {
            if !normalized.contains(&scope) {
                normalized.push(scope);
            }
            normalized
        }))
}

/// AsterDrive 0.5.0-only OAuth row conversion; remove in AsterDrive 0.6.0.
#[expect(
    deprecated,
    reason = "AsterDrive 0.5.0 OneDrive authorization conversion is removed in 0.6.0"
)]
fn import_legacy_onedrive_authorization(
    encryption_key: &str,
    policy: &storage_policy::Model,
    connector_config: &OneDriveConnectorConfigV1,
    application: &OneDriveApplicationCredentialV1,
    authorization: aster_drive_model::deprecated::storage_policy_credential::Model,
) -> Result<OneDriveAuthorizationCredentialV1> {
    if authorization.provider != StorageCredentialProvider::MicrosoftGraph
        || authorization.credential_kind != StorageCredentialKind::OauthDelegated
    {
        return Err(AsterError::database_operation(format!(
            "OneDrive storage policy {} has incompatible legacy authorization provider '{}' and kind '{}'",
            policy.id,
            authorization.provider.as_str(),
            authorization.credential_kind.as_str()
        )));
    }
    let access_token_ciphertext = required_legacy_value(
        authorization.access_token_ciphertext,
        policy.id,
        "Microsoft Graph access token ciphertext",
    )?;
    let access_token = crate::services::storage_policy::credential::crypto::decrypt_token(
        encryption_key,
        crate::services::storage_policy::credential::crypto::token_aad(
            policy.id,
            StorageCredentialProvider::MicrosoftGraph.as_str(),
            "access",
        )
        .as_bytes(),
        &access_token_ciphertext,
    )?;
    let access_token = required_legacy_value(
        Some(access_token),
        policy.id,
        "Microsoft Graph access token",
    )?;
    let refresh_token = authorization
        .refresh_token_ciphertext
        .map(|ciphertext| {
            let ciphertext = required_legacy_value(
                Some(ciphertext),
                policy.id,
                "Microsoft Graph refresh token ciphertext",
            )?;
            crate::services::storage_policy::credential::crypto::decrypt_token(
                encryption_key,
                crate::services::storage_policy::credential::crypto::token_aad(
                    policy.id,
                    StorageCredentialProvider::MicrosoftGraph.as_str(),
                    "refresh",
                )
                .as_bytes(),
                &ciphertext,
            )
            .and_then(|value| {
                required_legacy_value(Some(value), policy.id, "Microsoft Graph refresh token")
            })
        })
        .transpose()?;
    let metadata: LegacyOneDriveMetadata =
        serde_json::from_str(&authorization.metadata).map_err(|error| {
            AsterError::database_operation(format!(
                "OneDrive storage policy {} has invalid legacy authorization metadata: {error}",
                policy.id
            ))
        })?;
    if metadata
        .cloud
        .is_some_and(|cloud| cloud != connector_config.cloud)
    {
        return Err(AsterError::database_operation(format!(
            "OneDrive storage policy {} has conflicting legacy Microsoft Graph cloud",
            policy.id
        )));
    }
    let drive_id = required_legacy_value(
        metadata
            .drive_id
            .or_else(|| connector_config.drive_id.clone()),
        policy.id,
        "Microsoft Graph drive_id",
    )?;
    let root_item_id = required_legacy_value(
        metadata
            .root_item_id
            .or_else(|| connector_config.root_item_id.clone()),
        policy.id,
        "Microsoft Graph root_item_id",
    )?;
    let scopes = parse_legacy_scopes(
        &authorization.scopes,
        policy.id,
        "Microsoft Graph authorization scopes",
    )?;

    Ok(OneDriveAuthorizationCredentialV1 {
        account_label: normalized_legacy_value(authorization.account_label),
        subject: normalized_legacy_value(authorization.subject),
        tenant_id: normalized_legacy_value(authorization.tenant_id)
            .or_else(|| Some(application.tenant.clone())),
        scopes: if scopes.is_empty() {
            application.scopes.clone()
        } else {
            scopes
        },
        access_token,
        refresh_token,
        metadata: OneDriveAuthorizationMetadataV1 {
            cloud: connector_config.cloud,
            drive_id,
            root_item_id,
            root_item_name: normalized_legacy_value(metadata.root_item_name),
            id_token_present: metadata.id_token_present || metadata.id_token.is_some(),
        },
        status: authorization.status,
        status_reason: normalized_legacy_value(authorization.status_reason),
        expires_at: authorization.expires_at,
        authorized_at: authorization.authorized_at,
        last_refreshed_at: authorization.last_refreshed_at,
        last_validated_at: authorization.last_validated_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use aster_drive_storage::StoragePolicyBehaviorConfig;
    use sea_orm::{ActiveModelTrait, IntoActiveModel};

    const KEY: &str = "onedrive-connector-test-key-32bytes";

    fn connector_config(
        account_mode: OneDriveAccountMode,
        tenant: Option<&str>,
        drive_id: Option<&str>,
        site_id: Option<&str>,
        group_id: Option<&str>,
    ) -> OneDriveConnectorConfigV1 {
        OneDriveConnectorConfigV1 {
            base_path: String::new(),
            provider_resumable_upload_strategy: ProviderResumableUploadStrategy::ServerRelay,
            provider_download_strategy: ProviderDownloadStrategy::ServerRelay,
            provider_download_filename_mode: ProviderDownloadFilenameMode::ProviderNative,
            cloud: MicrosoftGraphCloud::Global,
            account_mode,
            tenant: tenant.map(ToOwned::to_owned),
            drive_id: drive_id.map(ToOwned::to_owned),
            root_item_id: None,
            site_id: site_id.map(ToOwned::to_owned),
            group_id: group_id.map(ToOwned::to_owned),
        }
    }

    fn policy(config: OneDriveConnectorConfigV1) -> storage_policy::Model {
        crate::storage::connectors::test_support::policy(
            OneDriveConnector::ID,
            1,
            config,
            StoragePolicyBehaviorConfig::default(),
        )
    }

    #[test]
    fn non_empty_string_trims_and_filters_blank_values() {
        assert_eq!(
            non_empty_string(" root ".to_string()),
            Some("root".to_string())
        );
        assert_eq!(non_empty_string(" \n\t ".to_string()), None);
    }

    #[test]
    fn default_scopes_follow_connector_owned_account_mode_rules() {
        let cases = [
            (
                connector_config(OneDriveAccountMode::Personal, None, None, None, None),
                "offline_access Files.ReadWrite",
            ),
            (
                connector_config(
                    OneDriveAccountMode::WorkOrSchool,
                    None,
                    Some("drive-id"),
                    None,
                    None,
                ),
                "offline_access Files.ReadWrite.All",
            ),
            (
                connector_config(
                    OneDriveAccountMode::SharepointSite,
                    None,
                    None,
                    Some("site-id"),
                    None,
                ),
                "offline_access Files.ReadWrite.All Sites.ReadWrite.All",
            ),
            (
                connector_config(
                    OneDriveAccountMode::GroupDrive,
                    None,
                    None,
                    None,
                    Some("group-id"),
                ),
                "offline_access Files.ReadWrite.All Sites.ReadWrite.All",
            ),
        ];

        for (config, expected_scopes) in cases {
            assert_eq!(default_microsoft_graph_scopes(&config), expected_scopes);
        }
    }

    #[test]
    fn connector_tenant_normalization_trims_and_rejects_blank_values() {
        assert_eq!(normalized_option(Some(" \n ".to_string())), None);
        assert_eq!(
            normalized_option(Some(" organizations ".to_string())),
            Some("organizations".to_string())
        );
    }

    #[test]
    fn application_identity_includes_every_token_issuing_input() {
        let base = OneDriveApplicationCredentialV1 {
            cloud: MicrosoftGraphCloud::Global,
            tenant: "common".to_string(),
            client_id: "client".to_string(),
            client_secret: "secret".to_string(),
            scopes: vec!["offline_access".to_string(), "Files.ReadWrite".to_string()],
        };
        assert!(same_application_identity(&base, &base.clone()));

        let variants = [
            OneDriveApplicationCredentialV1 {
                cloud: MicrosoftGraphCloud::China,
                ..base.clone()
            },
            OneDriveApplicationCredentialV1 {
                tenant: "organizations".to_string(),
                ..base.clone()
            },
            OneDriveApplicationCredentialV1 {
                client_id: "other-client".to_string(),
                ..base.clone()
            },
            OneDriveApplicationCredentialV1 {
                client_secret: "other-secret".to_string(),
                ..base.clone()
            },
            OneDriveApplicationCredentialV1 {
                scopes: vec!["offline_access".to_string()],
                ..base.clone()
            },
        ];
        for changed in variants {
            assert!(!same_application_identity(&base, &changed));
        }
    }

    #[tokio::test]
    async fn application_update_preserves_authorization_only_for_the_same_identity() {
        let db = crate::db::connect_with_metrics(
            &crate::config::DatabaseConfig {
                url: "sqlite::memory:".into(),
                pool_size: 1,
                retry_count: 0,
            },
            aster_drive_metrics::NoopMetrics::arc(),
        )
        .await
        .unwrap();
        crate::storage::connectors::test_support::migrate_current_storage_test_schema(&db).await;
        let config = connector_config(
            OneDriveAccountMode::Personal,
            Some("common"),
            None,
            None,
            None,
        );
        let stored_policy = policy(config.clone())
            .into_active_model()
            .insert(&db)
            .await
            .unwrap();

        let first = OneDriveConnector::upsert_application_config(
            &db,
            KEY,
            stored_policy.id,
            &config,
            OneDriveAuthorizationApplicationV1 {
                client_id: "client-a".to_string(),
                client_secret: "secret-a".to_string(),
                scopes: None,
            },
        )
        .await
        .unwrap();
        let mut payload: OneDriveCredentialV1 = super::super::decode_typed_connector_credential(
            KEY,
            &first,
            &aster_drive_storage::ConnectorId::declared(OneDriveConnector::ID),
            1,
        )
        .unwrap();
        payload.authorization = Some(OneDriveAuthorizationCredentialV1 {
            account_label: Some("Documents".to_string()),
            subject: Some("root".to_string()),
            tenant_id: Some("common".to_string()),
            scopes: payload.application.scopes.clone(),
            access_token: "access-token".to_string(),
            refresh_token: Some("refresh-token".to_string()),
            metadata: OneDriveAuthorizationMetadataV1 {
                cloud: MicrosoftGraphCloud::Global,
                drive_id: "drive-id".to_string(),
                root_item_id: "root".to_string(),
                root_item_name: Some("Documents".to_string()),
                id_token_present: false,
            },
            status: StorageCredentialStatus::Authorized,
            status_reason: None,
            expires_at: None,
            authorized_at: Some(chrono::Utc::now()),
            last_refreshed_at: None,
            last_validated_at: None,
        });
        super::super::persist_connector_credential_payload(
            &db,
            KEY,
            stored_policy.id,
            &aster_drive_storage::ConnectorId::declared(OneDriveConnector::ID),
            1,
            &payload,
        )
        .await
        .unwrap();

        let updated = OneDriveConnector::upsert_application_config(
            &db,
            KEY,
            stored_policy.id,
            &config,
            OneDriveAuthorizationApplicationV1 {
                client_id: "client-a".to_string(),
                client_secret: "  ".to_string(),
                scopes: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(updated.revision, 3);
        let payload: OneDriveCredentialV1 = super::super::decode_typed_connector_credential(
            KEY,
            &updated,
            &aster_drive_storage::ConnectorId::declared(OneDriveConnector::ID),
            1,
        )
        .unwrap();
        assert_eq!(payload.application.client_id, "client-a");
        assert_eq!(payload.application.client_secret, "secret-a");
        assert!(!payload.application.scopes.is_empty());
        assert_eq!(
            payload.authorization.unwrap().refresh_token.as_deref(),
            Some("refresh-token")
        );

        let changed = OneDriveConnector::upsert_application_config(
            &db,
            KEY,
            stored_policy.id,
            &config,
            OneDriveAuthorizationApplicationV1 {
                client_id: "client-b".to_string(),
                client_secret: "  ".to_string(),
                scopes: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(changed.revision, 4);
        let payload: OneDriveCredentialV1 = super::super::decode_typed_connector_credential(
            KEY,
            &changed,
            &aster_drive_storage::ConnectorId::declared(OneDriveConnector::ID),
            1,
        )
        .unwrap();
        assert_eq!(payload.application.client_id, "client-b");
        assert_eq!(payload.application.client_secret, "secret-a");
        assert!(payload.authorization.is_none());
    }
}

#[cfg(test)]
#[path = "../../services/storage_policy/credential/oauth/tests.rs"]
mod oauth_tests;
