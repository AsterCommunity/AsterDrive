use crate::db::repository::storage_policy_connector_credential_repo as repo;
use crate::services::storage_policy::credential::crypto;
use async_trait::async_trait;
use base64::Engine as _;
use chrono::Utc;
use provider::GoogleTokenProvider;
use rand::RngExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;

use crate::errors::{AsterError, Result, storage_driver_error};
use crate::storage::drivers::google_drive::{
    GoogleDriveDriver, GoogleDriveTokenProvider, http_client,
};
use aster_drive_model::entities::{storage_policy, storage_policy_connector_credential};
use aster_drive_model::types::{
    StorageCredentialKind, StorageCredentialProvider, StorageCredentialStatus,
};
use aster_drive_storage::StorageConnectorFieldDefaultValue;
use aster_drive_storage::connector_descriptor::{
    StorageConnectorBadgeRgb, StorageConnectorCapabilities,
    StorageConnectorCredentialManagementDescriptor, StorageConnectorCredentialStatusPresentation,
    StorageConnectorCredentialStatusTone, StorageConnectorDeploymentScope,
    StorageConnectorDescriptor, StorageConnectorFieldDisplayInput, StorageConnectorFieldKind,
    StorageConnectorFieldScope, StorageConnectorObjectNamingMode,
    StorageConnectorUiDescriptorInput, StorageConnectorUploadWorkflows,
    saved_connection_test_action_descriptor, server_relay_simple_upload_capabilities,
    start_authorization_action_descriptor, storage_connector_field,
    storage_connector_field_with_display, validate_credential_action_descriptor,
};
use aster_drive_storage::{StorageConnectorConfigSchema, StorageDriver, StorageErrorKind};

use super::{
    StorageConnector, StorageConnectorAuthorizationAudit, StorageConnectorAuthorizationCallback,
    StorageConnectorAuthorizationStart, StorageConnectorCredentialInput, StorageConnectorIcon,
    StorageConnectorRuntimeCredential, StorageConnectorUploadTransport,
    StorageCredentialValidationOutcome,
};

mod localization;
mod provider;
#[cfg(test)]
mod tests;

const GOOGLE_AUTH: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const GOOGLE_TOKEN: &str = "https://oauth2.googleapis.com/token";
const AUTH_TTL: u64 = 600;

pub struct GoogleDriveConnector;

aster_drive_storage::storage_connector_schema! {
    pub struct GoogleDriveConnectorConfigV1 {
        config {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub root_folder_id: Option<String> => storage_connector_field_with_display(StorageConnectorFieldDisplayInput {
            name: "root_folder_id", scope: StorageConnectorFieldScope::ConnectorConfig,
            kind: StorageConnectorFieldKind::Text, required: false, secret: false,
            label_key: "root_folder_id", placeholder: Some("Google Drive folder id"),
            help_key: Some("root_folder_id_desc"), required_message_key: None,
            invalid_protocol_message_key: None, allowed_endpoint_protocols: Vec::new(),
            allow_endpoint_without_protocol: false, trim_on_blur: true,
        }),
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub shared_drive_id: Option<String> => storage_connector_field_with_display(StorageConnectorFieldDisplayInput {
            name: "shared_drive_id", scope: StorageConnectorFieldScope::ConnectorConfig,
            kind: StorageConnectorFieldKind::Text, required: false, secret: false,
            label_key: "shared_drive_id", placeholder: Some("Shared drive id"),
            help_key: Some("shared_drive_id_desc"), required_message_key: None,
            invalid_protocol_message_key: None, allowed_endpoint_protocols: Vec::new(),
            allow_endpoint_without_protocol: false, trim_on_blur: true,
        }),
        #[serde(default)]
        pub app_data_folder: bool => {
            let mut field = storage_connector_field("app_data_folder", StorageConnectorFieldScope::ConnectorConfig, StorageConnectorFieldKind::Boolean, false, false);
            field.default_value = Some(StorageConnectorFieldDefaultValue::Boolean(false)); field
        },
        }
        credentials authorization_application GoogleDriveAuthorizationApplicationV1 {
            pub client_id: String => storage_connector_field("client_id", StorageConnectorFieldScope::AuthorizationApplication, StorageConnectorFieldKind::Text, true, false),
            pub client_secret: String => storage_connector_field("client_secret", StorageConnectorFieldScope::AuthorizationApplication, StorageConnectorFieldKind::Secret, true, true),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GoogleDriveCredentialV1 {
    pub application: GoogleDriveAuthorizationApplicationV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authorization: Option<GoogleDriveAuthorizationV1>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GoogleDriveAuthorizationV1 {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    pub scopes: Vec<String>,
    pub status: StorageCredentialStatus,
    #[serde(default)]
    pub status_reason: Option<String>,
    #[serde(default)]
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    pub authorized_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    pub last_refreshed_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    pub last_validated_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Clone)]
struct GoogleDriveRuntimeCredential {
    token_provider: Arc<dyn GoogleDriveTokenProvider>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct GoogleDriveFlowContext {
    application_ciphertext: String,
    credential_revision: i64,
    scope: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct GoogleDriveCleanupSnapshot {
    credential_ciphertext: String,
}
fn scope(config: &GoogleDriveConnectorConfigV1) -> &'static str {
    if config.app_data_folder {
        "https://www.googleapis.com/auth/drive.appdata"
    } else {
        "https://www.googleapis.com/auth/drive"
    }
}
fn audit() -> StorageConnectorAuthorizationAudit {
    StorageConnectorAuthorizationAudit {
        provider: StorageCredentialProvider::GoogleDrive,
        fields: serde_json::Map::new(),
    }
}
fn decode(
    key: &str,
    row: &storage_policy_connector_credential::Model,
) -> Result<GoogleDriveCredentialV1> {
    super::decode_typed_connector_credential(
        key,
        row,
        &aster_drive_storage::ConnectorId::declared(GoogleDriveConnector::ID),
        1,
    )
}
fn encode(payload: &GoogleDriveCredentialV1) -> Result<serde_json::Value> {
    serde_json::to_value(payload)
        .map_err(|_| AsterError::internal_error("serialize Google Drive credential"))
}
fn flow_aad(policy_id: i64, state_hash: &str) -> String {
    format!("google_drive_oauth:{policy_id}:{state_hash}")
}
impl GoogleDriveConnector {
    pub const ID: &'static str = "asterdrive.storage.google_drive";
    fn decode_config(policy: &storage_policy::Model) -> Result<GoogleDriveConnectorConfigV1> {
        super::common::decode_typed_policy_config(policy, Self::ID, 1).map(|(c, _)| c)
    }
    fn descriptor_definition() -> StorageConnectorDescriptor {
        StorageConnectorDescriptor {
            connector_id: aster_drive_storage::ConnectorId::declared(Self::ID),
            label: "Google Drive".into(),
            description: "Administrator-managed Google Drive storage backend".into(),
            ui: aster_drive_storage::connector_descriptor::storage_connector_ui_descriptor(
                StorageConnectorUiDescriptorInput {
                    label_key: "driver_type_google_drive",
                    description_key: "policy_wizard_google_drive_storage_desc",
                    badge_rgb: StorageConnectorBadgeRgb::new(66, 133, 244),
                    helper_key: "policy_wizard_google_drive_helper",
                    config_step_title_key: "policy_wizard_step_google_drive_title",
                    config_step_description_key: "policy_wizard_step_google_drive_desc",
                    edit_context_key: "policy_edit_context_google_drive_desc",
                    base_path_empty_display: "core:root",
                    base_path_placeholder: "folder id",
                },
            ),
            credential_mode: GoogleDriveConnectorConfigV1::credential_mode(),
            deployment_scope: StorageConnectorDeploymentScope::SharedAcrossPrimaryInstances,
            supports_initial_setup: false,
            requires_authorization: true,
            authorization_provider: Some("google_drive".into()),
            credential_management: Some(StorageConnectorCredentialManagementDescriptor {
                title_key: "google_drive_credential_title".into(),
                loading_key: "google_drive_credential_loading".into(),
                status_presentations: credential_status_presentations(),
                reauthorize_action_key: Some("google_drive_reauthorize_action".into()),
                authorized_at_key: Some("google_drive_authorized_at".into()),
                refreshed_at_key: Some("google_drive_refreshed_at".into()),
                validated_at_key: Some("google_drive_validated_at".into()),
                redirect_uri_key: Some("google_drive_redirect_uri".into()),
                redirect_uri_help_key: Some("google_drive_redirect_uri_help".into()),
                redirect_uri_copy_key: Some("google_drive_copy_redirect_uri".into()),
                save_before_authorize_key: Some("google_drive_save_before_authorize".into()),
                authorization_started_key: Some("google_drive_authorization_started".into()),
                save_before_validate_key: Some("google_drive_save_before_validate".into()),
                validation_success_key: Some("google_drive_validation_success".into()),
                validation_success_detail_key: None,
                created_authorize_next_key: None,
            }),
            capabilities: StorageConnectorCapabilities {
                efficient_range: true,
                capacity: false,
                list: false,
                presigned_download: false,
                custom_download_base_url: false,
                storage_native_thumbnail: false,
                storage_native_media_metadata: false,
                remote_node_binding: false,
                object_storage_transfer_strategy: false,
                object_naming: StorageConnectorObjectNamingMode::OpaqueUuid,
            },
            upload_workflows: StorageConnectorUploadWorkflows {
                simple_upload: true,
                simple_upload_capabilities: server_relay_simple_upload_capabilities(Some(
                    5 * 1024 * 1024,
                )),
                stream_upload: true,
                object_multipart_upload: false,
                object_multipart_upload_capabilities: None,
                provider_resumable_upload: true,
                presigned_upload: false,
                frontend_direct_provider_resumable_upload: false,
                provider_resumable_upload_capabilities: Some(
                    aster_drive_storage::connector_descriptor::StorageConnectorProviderResumableUploadCapabilities {
                        provider: "google_drive".into(),
                        session_label: "Google Drive resumable upload".into(),
                        min_fragment_size: 256 * 1024,
                        default_fragment_size: 8 * 1024 * 1024,
                        max_fragment_size: 8 * 1024 * 1024,
                        fragment_alignment: 256 * 1024,
                        max_simple_upload_size: Some(5 * 1024 * 1024),
                        frontend_direct_upload: false,
                        implicit_completion: true,
                        abort_supported: true,
                        status_query_supported: true,
                    },
                ),
            },
            fields: GoogleDriveConnectorConfigV1::descriptor_fields(),
            config_schema_version: 1,
            credential_schema_version: Some(1),
            actions: vec![
                start_authorization_action_descriptor(),
                validate_credential_action_descriptor(),
                saved_connection_test_action_descriptor(true),
            ],
            promotions: Vec::new(),
            related_issues: vec![128],
        }
    }
}

#[async_trait]
impl StorageConnector for GoogleDriveConnector {
    fn icon(&self) -> Option<StorageConnectorIcon> {
        None
    }
    fn descriptor(&self) -> StorageConnectorDescriptor {
        Self::descriptor_definition()
    }
    fn localization(&self) -> Result<aster_drive_storage::StorageConnectorLocalization> {
        super::localization::builtin_connector_localization(
            Self::ID,
            &self.descriptor(),
            localization::MESSAGES,
        )
    }
    fn validate_connector_config(
        &self,
        input: &aster_drive_storage::ConnectorConfigEnvelope,
    ) -> Result<aster_drive_storage::ConnectorConfigEnvelope> {
        let n = aster_drive_storage::connector_descriptor::normalize_storage_connector_config(
            &self.descriptor(),
            input,
        )
        .map_err(|e| AsterError::validation_error(e.to_string()))?;
        let mut c: GoogleDriveConnectorConfigV1 =
            super::common::decode_normalized_connector_config(&n)?;
        for value in [&mut c.root_folder_id, &mut c.shared_drive_id] {
            *value = value
                .take()
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty());
            if value.as_ref().is_some_and(|s| {
                s.len() > 256
                    || !s
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
            }) {
                return Err(AsterError::validation_error(
                    "Google Drive folder and drive IDs must contain only letters, digits, underscores or hyphens",
                ));
            }
        }
        if c.app_data_folder && (c.root_folder_id.is_some() || c.shared_drive_id.is_some()) {
            return Err(AsterError::validation_error(
                "Google Drive appDataFolder cannot be combined with root_folder_id or shared_drive_id",
            ));
        }
        super::common::encode_normalized_connector_config(n.connector_id, n.schema_version, c)
    }
    fn validate_credential_input(&self, input: &StorageConnectorCredentialInput) -> Result<()> {
        let c: GoogleDriveAuthorizationApplicationV1 =
            super::common::decode_authorization_application(input, Self::ID)?;
        super::common::validate_required_credential_field(&c.client_id, "client_id", Self::ID)?;
        super::common::validate_required_credential_field(
            &c.client_secret,
            "client_secret",
            Self::ID,
        )
    }
    async fn persist_credential(
        &self,
        db: &sea_orm::DatabaseTransaction,
        key: &str,
        policy_id: i64,
        _config: &aster_drive_storage::ConnectorConfigEnvelope,
        input: StorageConnectorCredentialInput,
    ) -> Result<()> {
        let mut application: GoogleDriveAuthorizationApplicationV1 =
            super::common::decode_authorization_application(&input, Self::ID)?;
        let existing = repo::find_by_policy(db, policy_id)
            .await?
            .as_ref()
            .map(|row| decode(key, row))
            .transpose()?;
        application.client_id = application.client_id.trim().into();
        application.client_secret = application.client_secret.trim().into();
        if application.client_secret.is_empty() {
            application.client_secret = existing
                .as_ref()
                .map(|p| p.application.client_secret.clone())
                .unwrap_or_default();
        }
        if application.client_id.is_empty() || application.client_secret.is_empty() {
            return Err(AsterError::validation_error(
                "Google OAuth client ID and secret are required",
            ));
        }
        let preserve = existing.as_ref().is_some_and(|p| {
            p.application.client_id == application.client_id
                && p.application.client_secret == application.client_secret
        });
        let payload = GoogleDriveCredentialV1 {
            application,
            authorization: if preserve {
                existing.and_then(|p| p.authorization)
            } else {
                None
            },
        };
        super::persist_connector_credential_payload(
            db,
            key,
            policy_id,
            &aster_drive_storage::ConnectorId::declared(Self::ID),
            1,
            &payload,
        )
        .await
    }
    async fn build_driver_from_connection(
        &self,
        _context: &super::StorageConnectorContext<'_>,
        _config: &aster_drive_storage::ConnectorConfigEnvelope,
        _credential: &StorageConnectorCredentialInput,
    ) -> Result<Box<dyn StorageDriver>> {
        Err(AsterError::unsupported_driver(
            "Google Drive draft connection test requires authorization",
        ))
    }
    fn build_runtime_driver(
        &self,
        registry: &crate::storage::DriverRegistry,
        policy: &storage_policy::Model,
    ) -> Result<super::StorageConnectorDriver> {
        let credential = registry.get_runtime_credential(policy.id).ok_or_else(|| {
            storage_driver_error(
                StorageErrorKind::Auth,
                "Google Drive policy is not authorized",
            )
        })?;
        Ok(super::StorageConnectorDriver::storage(
            self.build_authorized_driver(policy, credential)?,
        ))
    }
    fn build_authorized_driver(
        &self,
        policy: &storage_policy::Model,
        credential: StorageConnectorRuntimeCredential,
    ) -> Result<Arc<dyn StorageDriver>> {
        let config = Self::decode_config(policy)?;
        let credential = credential.require::<GoogleDriveRuntimeCredential>(Self::ID)?;
        Ok(Arc::new(GoogleDriveDriver::new(
            credential.token_provider.clone(),
            config.root_folder_id,
            config.shared_drive_id,
            config.app_data_folder,
        )))
    }
    async fn start_authorization(
        &self,
        context: &super::StorageConnectorContext<'_>,
        policy: &storage_policy::Model,
        redirect_uri: &str,
    ) -> Result<StorageConnectorAuthorizationStart> {
        let row = repo::find_by_policy(context.writer_db(), policy.id)
            .await?
            .ok_or_else(|| {
                AsterError::validation_error(
                    "save Google OAuth application credentials before authorization",
                )
            })?;
        let payload = decode(&context.config().auth.storage_credential_secret_key, &row)?;
        let config = Self::decode_config(policy)?;
        let state = format!(
            "storage_google_{}",
            aster_forge_utils::id::new_short_token()
        );
        let mut bytes = [0u8; 32];
        rand::rng().fill(&mut bytes);
        let verifier = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
        let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(Sha256::digest(verifier.as_bytes()));
        let url = authorization_url(
            &payload.application.client_id,
            redirect_uri,
            scope(&config),
            &state,
            &challenge,
        )?;
        let state_hash = crypto::token_hash(&state);
        let flow = GoogleDriveFlowContext {
            application_ciphertext: crypto::encrypt_token(
                &context.config().auth.storage_credential_secret_key,
                flow_aad(policy.id, &state_hash).as_bytes(),
                &serde_json::to_string(&payload.application).map_err(|_| {
                    AsterError::internal_error("serialize Google OAuth application")
                })?,
            )?,
            credential_revision: row.revision,
            scope: scope(&config).into(),
        };
        Ok(StorageConnectorAuthorizationStart {
            provider: StorageCredentialProvider::GoogleDrive,
            authorization_url: url,
            expires_in: AUTH_TTL,
            state,
            pkce_verifier: Some(verifier),
            scopes: vec![scope(&config).into()],
            context: serde_json::to_string(&flow)
                .map_err(|_| AsterError::internal_error("serialize Google OAuth flow"))?,
            audit: audit(),
        })
    }
    async fn finish_authorization(
        &self,
        context: &super::StorageConnectorContext<'_>,
        policy: &storage_policy::Model,
        flow: &aster_drive_model::entities::storage_policy_authorization_flow::Model,
        code: &str,
        now: chrono::DateTime<chrono::Utc>,
    ) -> std::result::Result<
        StorageConnectorAuthorizationCallback,
        super::StorageConnectorAuthorizationError,
    > {
        self.complete_authorization(context, policy, flow, code, now)
            .await
            .map_err(|e| {
                super::StorageConnectorAuthorizationError::new(
                    super::StorageAuthorizationFailureReason::TokenExchangeFailed,
                    e,
                )
            })
    }
    async fn load_runtime_credential(
        &self,
        _db: &sea_orm::DatabaseConnection,
        config: &crate::config::Config,
        _policy: &storage_policy::Model,
        credential: &storage_policy_connector_credential::Model,
    ) -> Result<Option<StorageConnectorRuntimeCredential>> {
        let payload = decode(&config.auth.storage_credential_secret_key, credential)?;
        if !payload
            .authorization
            .as_ref()
            .is_some_and(|a| a.status == StorageCredentialStatus::Authorized)
        {
            return Ok(None);
        }
        Ok(Some(StorageConnectorRuntimeCredential::new(
            aster_drive_storage::ConnectorId::declared(Self::ID),
            GoogleDriveRuntimeCredential {
                token_provider: GoogleTokenProvider::new(
                    Some(_db.clone()),
                    config.auth.storage_credential_secret_key.clone(),
                    credential.policy_id,
                    payload,
                ),
            },
        )))
    }
    fn credential_info(
        &self,
        _config: &crate::config::Config,
        credential: &storage_policy_connector_credential::Model,
    ) -> Result<Option<super::StorageConnectorCredentialInfo>> {
        let payload = decode(&_config.auth.storage_credential_secret_key, credential)?;
        let Some(auth) = payload.authorization else {
            return Ok(None);
        };
        Ok(Some(super::StorageConnectorCredentialInfo {
            id: credential.id,
            policy_id: credential.policy_id,
            provider: StorageCredentialProvider::GoogleDrive,
            credential_kind: StorageCredentialKind::OauthDelegated,
            account_label: None,
            subject: None,
            tenant_id: None,
            scopes: auth.scopes,
            status: auth.status,
            status_reason: auth.status_reason,
            expires_at: auth.expires_at,
            authorized_at: auth.authorized_at,
            last_refreshed_at: auth.last_refreshed_at,
            last_validated_at: auth.last_validated_at,
            created_at: credential.created_at,
            updated_at: credential.updated_at,
        }))
    }
    fn credential_validation_failure_payload(
        &self,
        _config: &crate::config::Config,
        _credential: &storage_policy_connector_credential::Model,
        _kind: Option<StorageErrorKind>,
        _reason: &str,
    ) -> Result<Option<serde_json::Value>> {
        let status = match _kind {
            Some(StorageErrorKind::Auth) => StorageCredentialStatus::ReauthRequired,
            Some(StorageErrorKind::Permission) => StorageCredentialStatus::PermissionDenied,
            Some(StorageErrorKind::Misconfigured) => StorageCredentialStatus::Invalid,
            _ => return Ok(None),
        };
        let mut payload = decode(&_config.auth.storage_credential_secret_key, _credential)?;
        let Some(auth) = payload.authorization.as_mut() else {
            return Ok(None);
        };
        auth.status = status;
        auth.status_reason = Some(
            match status {
                StorageCredentialStatus::ReauthRequired => {
                    "Google authorization expired or was revoked; reauthorize"
                }
                StorageCredentialStatus::PermissionDenied => {
                    "Google Drive permission is insufficient"
                }
                _ => "Check Google Drive application and storage configuration",
            }
            .into(),
        );
        encode(&payload).map(Some)
    }
    async fn validate_credential(
        &self,
        _db: &sea_orm::DatabaseConnection,
        _config: &crate::config::Config,
        _policy: &storage_policy::Model,
        credential: &storage_policy_connector_credential::Model,
    ) -> Result<StorageCredentialValidationOutcome> {
        let payload = decode(&_config.auth.storage_credential_secret_key, credential)?;
        let token_provider = GoogleTokenProvider::new(
            Some(_db.clone()),
            _config.auth.storage_credential_secret_key.clone(),
            _policy.id,
            payload,
        );
        let config = Self::decode_config(_policy)?;
        let driver = GoogleDriveDriver::new(
            token_provider,
            config.root_folder_id,
            config.shared_drive_id,
            config.app_data_folder,
        );
        let (root_item_id, root_item_name) = driver.validate_root().await?;
        // Token resolution can refresh the encrypted row. Never write the old tokens back.
        let current = repo::find_by_policy(_db, _policy.id)
            .await?
            .ok_or_else(|| AsterError::record_not_found("Google Drive credential"))?;
        let mut payload = decode(&_config.auth.storage_credential_secret_key, &current)?;
        let auth = payload
            .authorization
            .as_mut()
            .ok_or_else(|| AsterError::validation_error("Google Drive is not authorized"))?;
        auth.status = StorageCredentialStatus::Authorized;
        auth.status_reason = None;
        auth.last_validated_at = Some(Utc::now());
        Ok(StorageCredentialValidationOutcome {
            credential: current,
            credential_payload: encode(&payload)?,
            root_item_id,
            root_item_name,
        })
    }
    fn upload_transport(
        &self,
        _policy: &storage_policy::Model,
    ) -> Result<StorageConnectorUploadTransport> {
        Ok(StorageConnectorUploadTransport::ProviderResumable(
            aster_drive_model::types::ProviderResumableUploadStrategy::ServerRelay,
        ))
    }
    async fn cleanup_snapshot_for_policy(
        &self,
        context: &super::StorageConnectorContext<'_>,
        policy: &storage_policy::Model,
    ) -> Result<Option<super::StoragePolicyCleanupDriverSnapshot>> {
        let Some(row) = repo::find_by_policy(context.writer_db(), policy.id).await? else {
            tracing::warn!(
                policy_id = policy.id,
                "Google Drive policy cleanup has no saved credential; deferred object cleanup is unavailable"
            );
            return Ok(None);
        };
        let payload = decode(&context.config().auth.storage_credential_secret_key, &row)?;
        if payload
            .authorization
            .as_ref()
            .is_none_or(|auth| auth.status != StorageCredentialStatus::Authorized)
        {
            tracing::warn!(
                policy_id = policy.id,
                "Google Drive policy cleanup has no usable authorization; deferred object cleanup is unavailable"
            );
            return Ok(None);
        }
        super::StoragePolicyCleanupDriverSnapshot::encode(
            aster_drive_storage::ConnectorId::declared(Self::ID),
            1,
            &GoogleDriveCleanupSnapshot {
                credential_ciphertext: row.ciphertext,
            },
        )
        .map(Some)
    }
    fn cleanup_snapshot_required(&self) -> bool {
        true
    }
    async fn build_cleanup_driver(
        &self,
        context: &super::StorageConnectorContext<'_>,
        policy: &storage_policy::Model,
        snapshots: super::StoragePolicyCleanupSnapshots<'_>,
    ) -> Result<Arc<dyn StorageDriver>> {
        let snapshot: GoogleDriveCleanupSnapshot = snapshots
            .driver_snapshot
            .ok_or_else(|| {
                AsterError::validation_error("Google Drive cleanup credential snapshot is missing")
            })?
            .decode(Self::ID, 1)?;
        let key = &context.config().auth.storage_credential_secret_key;
        let plaintext = crypto::decrypt_connector_credential(
            key,
            policy.id,
            Self::ID,
            1,
            &snapshot.credential_ciphertext,
        )?;
        let payload: GoogleDriveCredentialV1 = serde_json::from_str(&plaintext).map_err(|_| {
            AsterError::database_operation("invalid Google Drive cleanup credential")
        })?;
        let config = Self::decode_config(policy)?;
        Ok(Arc::new(GoogleDriveDriver::new(
            GoogleTokenProvider::new(None, key.clone(), policy.id, payload),
            config.root_folder_id,
            config.shared_drive_id,
            config.app_data_folder,
        )))
    }
}

fn authorization_url(
    client_id: &str,
    redirect_uri: &str,
    scope: &str,
    state: &str,
    challenge: &str,
) -> Result<String> {
    reqwest::Url::parse_with_params(
        GOOGLE_AUTH,
        [
            ("client_id", client_id),
            ("redirect_uri", redirect_uri),
            ("response_type", "code"),
            ("access_type", "offline"),
            ("prompt", "consent"),
            ("scope", scope),
            ("state", state),
            ("code_challenge", challenge),
            ("code_challenge_method", "S256"),
        ],
    )
    .map(|u| u.to_string())
    .map_err(|_| AsterError::validation_error("invalid Google OAuth parameters"))
}
impl GoogleDriveConnector {
    async fn complete_authorization(
        &self,
        context: &super::StorageConnectorContext<'_>,
        policy: &storage_policy::Model,
        flow: &aster_drive_model::entities::storage_policy_authorization_flow::Model,
        code: &str,
        now: chrono::DateTime<Utc>,
    ) -> Result<StorageConnectorAuthorizationCallback> {
        if flow.provider != StorageCredentialProvider::GoogleDrive
            || flow.policy_id != Some(policy.id)
        {
            return Err(AsterError::validation_error(
                "Google OAuth flow does not belong to this policy",
            ));
        }
        let f: GoogleDriveFlowContext = serde_json::from_str(&flow.context)
            .map_err(|_| AsterError::database_operation("invalid Google OAuth flow context"))?;
        let key = &context.config().auth.storage_credential_secret_key;
        let plaintext = crypto::decrypt_token(
            key,
            flow_aad(policy.id, &flow.state_hash).as_bytes(),
            &f.application_ciphertext,
        )?;
        let app: GoogleDriveAuthorizationApplicationV1 =
            serde_json::from_str(&plaintext).map_err(|_| {
                AsterError::database_operation("invalid Google OAuth application context")
            })?;
        let verifier = flow
            .pkce_verifier
            .as_deref()
            .ok_or_else(|| AsterError::validation_error("Google OAuth PKCE verifier is missing"))?;
        let response = http_client()
            .post(GOOGLE_TOKEN)
            .form(&[
                ("code", code),
                ("client_id", app.client_id.as_str()),
                ("client_secret", app.client_secret.as_str()),
                ("redirect_uri", flow.redirect_uri.as_str()),
                ("code_verifier", verifier),
                ("grant_type", "authorization_code"),
            ])
            .send()
            .await
            .map_err(|_| {
                storage_driver_error(
                    StorageErrorKind::Transient,
                    "Google OAuth token endpoint is unavailable",
                )
            })?;
        if !response.status().is_success() {
            return Err(storage_driver_error(
                StorageErrorKind::Auth,
                "Google OAuth token exchange failed",
            ));
        }
        let token: provider::TokenResponse = response
            .json()
            .await
            .map_err(|_| AsterError::database_operation("invalid Google OAuth token response"))?;
        token.validate()?;
        let current = repo::find_by_policy(context.writer_db(), policy.id)
            .await?
            .ok_or_else(|| AsterError::record_not_found("Google Drive credential"))?;
        if current.revision != f.credential_revision
            || scope(&Self::decode_config(policy)?) != f.scope
        {
            return Err(AsterError::validation_error(
                "Google Drive configuration changed during authorization; restart authorization",
            ));
        }
        let expires_at = token.expires_at(now);
        let scopes: Vec<String> = token
            .scope
            .as_deref()
            .unwrap_or(&f.scope)
            .split_whitespace()
            .map(str::to_owned)
            .collect();
        if !scopes.contains(&f.scope) {
            return Err(storage_driver_error(
                StorageErrorKind::Permission,
                "Google OAuth did not grant the requested Drive scope",
            ));
        }
        let refresh_token = token
            .refresh_token
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                storage_driver_error(
                    StorageErrorKind::Auth,
                    "Google OAuth did not return an offline refresh token; reauthorize",
                )
            })?;
        let payload = GoogleDriveCredentialV1 {
            application: app,
            authorization: Some(GoogleDriveAuthorizationV1 {
                access_token: token.access_token,
                refresh_token: Some(refresh_token),
                scopes,
                status: StorageCredentialStatus::Authorized,
                status_reason: None,
                expires_at,
                authorized_at: Some(now),
                last_refreshed_at: None,
                last_validated_at: None,
            }),
        };
        Ok(StorageConnectorAuthorizationCallback {
            credential_payload: encode(&payload)?,
            audit: audit(),
        })
    }
}

fn credential_status_presentations()
-> std::collections::BTreeMap<String, StorageConnectorCredentialStatusPresentation> {
    [
        (
            "missing",
            "google_drive_credential_status_missing",
            StorageConnectorCredentialStatusTone::Neutral,
            None,
        ),
        (
            "authorized",
            "google_drive_credential_status_authorized",
            StorageConnectorCredentialStatusTone::Success,
            None,
        ),
        (
            "reauth_required",
            "google_drive_credential_status_reauth_required",
            StorageConnectorCredentialStatusTone::Warning,
            Some("google_drive_reauth_help"),
        ),
        (
            "permission_denied",
            "google_drive_credential_status_permission_denied",
            StorageConnectorCredentialStatusTone::Danger,
            Some("google_drive_permission_help"),
        ),
        (
            "revoked",
            "google_drive_credential_status_revoked",
            StorageConnectorCredentialStatusTone::Danger,
            Some("google_drive_reauth_help"),
        ),
        (
            "invalid",
            "google_drive_credential_status_invalid",
            StorageConnectorCredentialStatusTone::Danger,
            Some("google_drive_invalid_help"),
        ),
    ]
    .into_iter()
    .map(|(status, label, tone, description)| {
        (
            status.into(),
            StorageConnectorCredentialStatusPresentation {
                label_key: label.into(),
                tone,
                description_key: description.map(Into::into),
                attention_title_key: None,
                attention_guidance_key: None,
                reason_rules: Vec::new(),
                reason_fallback_key: description.map(Into::into),
            },
        )
    })
    .collect()
}
