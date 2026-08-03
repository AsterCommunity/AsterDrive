use async_trait::async_trait;
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::sync::Arc;

use crate::db::repository::storage_policy_connector_credential_repo;
use crate::errors::{AsterError, Result, storage_driver_error};
use crate::storage::drivers::onedrive::{
    MicrosoftGraphClient, MicrosoftGraphClientConfig, OneDriveDriver,
    microsoft_graph_upload_capabilities,
};
use aster_drive_model::entities::storage_policy;
use aster_drive_model::types::{
    MicrosoftGraphCloud, OneDriveAccountMode, ProviderDownloadFilenameMode,
    ProviderDownloadStrategy, ProviderResumableUploadStrategy, StorageCredentialKind,
    StorageCredentialProvider, StorageCredentialStatus,
};
use aster_drive_storage::StorageDriver;
use aster_drive_storage::connector_descriptor::{
    StorageConnectorCapabilities, StorageConnectorDeploymentScope, StorageConnectorDescriptor,
    StorageConnectorFieldKind, StorageConnectorFieldScope, StorageConnectorObjectNamingMode,
    StorageConnectorProviderResumableUploadCapabilities, StorageConnectorUiDescriptorInput,
    StorageConnectorUploadWorkflows, saved_connection_test_action_descriptor,
    server_relay_simple_upload_capabilities, start_authorization_action_descriptor,
    storage_connector_field, storage_connector_field_with_options, storage_connector_ui_descriptor,
    validate_credential_action_descriptor,
};
use aster_drive_storage::{StorageConnectorConfigSchema, StorageConnectorFieldDefaultValue};

use super::common::unsupported_draft_connection_test_error;
use super::{
    StorageConnector, StorageConnectorCredentialInput, StorageConnectorRuntimeCredential,
    StorageConnectorUploadTransport, StorageCredentialValidationOutcome,
    StoragePolicyCleanupDriverSnapshot, StoragePolicyCleanupOneDriveCredentialSnapshot,
    StoragePolicyCleanupSnapshots,
};

pub struct OneDriveConnector;

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
        pub base_path: String => storage_connector_field(
            "base_path", StorageConnectorFieldScope::ConnectorConfig,
            StorageConnectorFieldKind::Text, false, false,
        ),
        pub provider_resumable_upload_strategy: ProviderResumableUploadStrategy => onedrive_select_field(
            "provider_resumable_upload_strategy", vec!["server_relay", "frontend_direct"], "server_relay",
        ),
        pub provider_download_strategy: ProviderDownloadStrategy => onedrive_select_field(
            "provider_download_strategy", vec!["server_relay", "frontend_direct"], "server_relay",
        ),
        pub provider_download_filename_mode: ProviderDownloadFilenameMode => onedrive_select_field(
            "provider_download_filename_mode", vec!["provider_native", "strict_current"], "provider_native",
        ),
        pub cloud: MicrosoftGraphCloud => onedrive_select_field(
            "cloud", vec!["global", "china"], "global",
        ),
        pub account_mode: OneDriveAccountMode => onedrive_select_field(
            "account_mode", vec!["personal", "work_or_school", "sharepoint_site", "group_drive"], "personal",
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
    options: Vec<&str>,
    default_value: &str,
) -> aster_drive_storage::StorageConnectorFieldDescriptor {
    let mut field = storage_connector_field_with_options(
        name,
        StorageConnectorFieldScope::ConnectorConfig,
        StorageConnectorFieldKind::Select,
        true,
        false,
        options,
    );
    field.default_value = Some(StorageConnectorFieldDefaultValue::String(
        default_value.to_string(),
    ));
    field
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
            actions: vec![
                start_authorization_action_descriptor(),
                validate_credential_action_descriptor(),
                saved_connection_test_action_descriptor(true),
            ],
            driver_recommendations: Vec::new(),
            related_issues: vec![328, 329, 330, 349],
        }
    }
}

#[async_trait]
impl StorageConnector for OneDriveConnector {
    fn descriptor(&self) -> StorageConnectorDescriptor {
        Self::descriptor_definition()
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
    #[allow(deprecated)]
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
        let client_secret =
            crate::services::storage_policy::credential::decrypt_application_client_secret(
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
        let default_scopes = crate::services::storage_policy::credential::
            default_microsoft_graph_scopes_for_onedrive_config(&connector_config);
        let application_scopes = parse_legacy_scopes(
            &application.scopes,
            policy.id,
            "Microsoft Graph application scopes",
        )?;
        let application_scopes =
            crate::services::storage_policy::credential::normalize_scopes_with_default(
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
        crate::services::storage_policy::credential::upsert_microsoft_graph_application_config(
            db,
            encryption_key,
            policy_id,
            &config,
            application,
        )
        .await?;
        Ok(())
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
        let token_provider =
            match crate::services::storage_policy::credential::build_microsoft_graph_credential_token_provider(
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
        Ok(Some(StorageConnectorRuntimeCredential::MicrosoftGraph(
            super::models::OneDriveCredentialRuntime {
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
        let StorageConnectorRuntimeCredential::MicrosoftGraph(credential) = credential else {
            return Err(storage_driver_error(
                aster_drive_storage::StorageErrorKind::Auth,
                "OneDrive driver received incompatible runtime credentials",
            ));
        };
        let config = Self::decode_config(policy)?;
        let drive_id = config
            .drive_id
            .clone()
            .and_then(non_empty_string)
            .or_else(|| credential.drive_id.and_then(non_empty_string))
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
            .or_else(|| credential.root_item_id.and_then(non_empty_string))
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
            credential.token_provider,
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
        let token_provider =
            crate::services::storage_policy::credential::build_microsoft_graph_credential_token_provider(
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
        let location = crate::services::storage_policy::credential::resolve_onedrive_location(
            &client,
            &connector_config,
        )
        .await?;
        let root_item = location.root_item;
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
            .map(|snapshot| snapshot.map(StoragePolicyCleanupDriverSnapshot::MicrosoftGraph))
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
        let token_provider = crate::services::storage_policy::credential::build_microsoft_graph_cleanup_token_provider(
            context.config().auth.storage_credential_secret_key.clone(),
            policy,
            crate::services::storage_policy::credential::MicrosoftGraphCleanupTokenSnapshot {
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
) -> Result<Option<StoragePolicyCleanupOneDriveCredentialSnapshot>> {
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
    let client_secret_ciphertext =
        crate::services::storage_policy::credential::encrypt_application_client_secret(
            &config.auth.storage_credential_secret_key,
            policy.id,
            &client_secret,
        )?;

    Ok(Some(StoragePolicyCleanupOneDriveCredentialSnapshot {
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
) -> Result<&StoragePolicyCleanupOneDriveCredentialSnapshot> {
    match snapshots.driver_snapshot {
        Some(StoragePolicyCleanupDriverSnapshot::MicrosoftGraph(snapshot)) => Ok(snapshot),
        Some(_) => Err(AsterError::validation_error(
            "OneDrive storage policy cleanup received incompatible driver snapshot",
        )),
        None => Err(AsterError::validation_error(
            "OneDrive storage policy cleanup missing credential snapshot",
        )),
    }
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
        .filter_map(|scope| non_empty_string(scope))
        .fold(Vec::new(), |mut normalized, scope| {
            if !normalized.contains(&scope) {
                normalized.push(scope);
            }
            normalized
        }))
}

/// AsterDrive 0.5.0-only OAuth row conversion; remove in AsterDrive 0.6.0.
#[allow(deprecated)]
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

    #[test]
    fn non_empty_string_trims_and_filters_blank_values() {
        assert_eq!(
            non_empty_string(" root ".to_string()),
            Some("root".to_string())
        );
        assert_eq!(non_empty_string(" \n\t ".to_string()), None);
    }
}
