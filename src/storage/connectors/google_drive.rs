use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt, sync::Arc};
use tokio::sync::Mutex;

use crate::db::repository::storage_policy_connector_credential_repo;
use crate::errors::{AsterError, Result, storage_driver_error};
use crate::storage::drivers::google_drive::{
    GoogleDriveAccessTokenProvider, GoogleDriveClient, GoogleDriveClientConfig, GoogleDriveDriver,
};
use aster_drive_model::entities::storage_policy;
use aster_drive_model::types::{
    ProviderDownloadStrategy, ProviderResumableUploadStrategy, StorageCredentialKind,
    StorageCredentialProvider, StorageCredentialStatus,
};
use aster_drive_storage::StorageDriver;
use aster_drive_storage::connector_descriptor::{
    StorageConnectorBadgeRgb, StorageConnectorCapabilities,
    StorageConnectorCredentialManagementDescriptor, StorageConnectorCredentialStatusPresentation,
    StorageConnectorCredentialStatusTone, StorageConnectorDeploymentScope,
    StorageConnectorDescriptor, StorageConnectorFieldKind, StorageConnectorFieldScope,
    StorageConnectorObjectNamingMode, StorageConnectorUiDescriptorInput,
    StorageConnectorUploadWorkflows, saved_connection_test_action_descriptor,
    server_relay_simple_upload_capabilities, start_authorization_action_descriptor,
    storage_connector_field, storage_connector_select_field, storage_connector_ui_descriptor,
    validate_credential_action_descriptor,
};
use aster_drive_storage::{
    StorageConnectorConfigSchema, StorageConnectorCredentialMode, StorageConnectorFieldDefaultValue,
};
use aster_forge_utils::id;

use super::{
    StorageAuthorizationFailureReason, StorageConnector, StorageConnectorAuthorizationAudit,
    StorageConnectorAuthorizationCallback, StorageConnectorAuthorizationError,
    StorageConnectorAuthorizationStart, StorageConnectorCredentialInfo,
    StorageConnectorCredentialInput, StorageConnectorRuntimeCredential,
    StorageConnectorUploadTransport,
};

mod localization;
mod oauth;

const AUTHORIZATION_FLOW_TTL_SECS: u64 = 300;
const SCHEMA_VERSION: u32 = 1;

pub struct GoogleDriveConnector;

aster_drive_storage::storage_connector_schema! {
    pub struct GoogleDriveConnectorConfigV1 {
        config {
            pub base_path: String => {
                let mut field = storage_connector_field("base_path", StorageConnectorFieldScope::ConnectorConfig, StorageConnectorFieldKind::Text, false, false);
                field.default_value = Some(StorageConnectorFieldDefaultValue::String(String::new()));
                field.default_mode = aster_drive_storage::StorageConnectorFieldDefaultMode::MissingOrEmptyText;
                field
            },
            pub root_folder_id: String => storage_connector_field("root_folder_id", StorageConnectorFieldScope::ConnectorConfig, StorageConnectorFieldKind::Text, false, false),
            #[serde(default)]
            pub shared_drive_id: Option<String> => storage_connector_field("shared_drive_id", StorageConnectorFieldScope::ConnectorConfig, StorageConnectorFieldKind::Text, false, false),
            #[serde(default)]
            pub app_data_folder: bool => storage_connector_field("app_data_folder", StorageConnectorFieldScope::ConnectorConfig, StorageConnectorFieldKind::Boolean, false, false),
            #[serde(default = "default_chunk_size")]
            pub chunk_size: i64 => google_chunk_field(),
            pub provider_resumable_upload_strategy: ProviderResumableUploadStrategy => google_strategy_field("provider_resumable_upload_strategy"),
            pub provider_download_strategy: ProviderDownloadStrategy => google_strategy_field("provider_download_strategy"),
        }
        credentials authorization_application GoogleDriveAuthorizationApplicationV1 {
            pub client_id: String => storage_connector_field("client_id", StorageConnectorFieldScope::AuthorizationApplication, StorageConnectorFieldKind::Text, true, false),
            pub client_secret: String => storage_connector_field("client_secret", StorageConnectorFieldScope::AuthorizationApplication, StorageConnectorFieldKind::Secret, true, true),
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub scopes: Option<String> => storage_connector_field("scopes", StorageConnectorFieldScope::AuthorizationApplication, StorageConnectorFieldKind::Text, false, false),
        }
    }
}

fn default_chunk_size() -> i64 {
    8 * 1024 * 1024
}

fn google_chunk_field() -> aster_drive_storage::StorageConnectorFieldDescriptor {
    let mut field = storage_connector_field(
        "chunk_size",
        StorageConnectorFieldScope::ConnectorConfig,
        StorageConnectorFieldKind::Number,
        false,
        false,
    );
    field.default_value = Some(StorageConnectorFieldDefaultValue::Integer(
        default_chunk_size(),
    ));
    field
}

fn google_strategy_field(name: &str) -> aster_drive_storage::StorageConnectorFieldDescriptor {
    let description_key = match name {
        "provider_resumable_upload_strategy" => {
            "provider_resumable_upload_strategy_server_relay_desc"
        }
        _ => "provider_download_strategy_server_relay_desc",
    };
    let mut field = storage_connector_select_field(
        name,
        StorageConnectorFieldScope::ConnectorConfig,
        true,
        vec![aster_drive_storage::StorageConnectorSelectOptionInput {
            value: "server_relay",
            label_key: "provider_resumable_upload_strategy_server_relay",
            description_key: Some(description_key),
        }],
    );
    field.default_value = Some(StorageConnectorFieldDefaultValue::String(
        "server_relay".to_string(),
    ));
    field
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct GoogleDriveCredentialV1 {
    application: GoogleDriveOAuthApplicationPayload,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    authorization: Option<GoogleDriveAuthorizationV1>,
}

impl fmt::Debug for GoogleDriveCredentialV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GoogleDriveCredentialV1")
            .field("application", &self.application)
            .field("authorization", &self.authorization)
            .finish()
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct GoogleDriveOAuthApplicationPayload {
    client_id: String,
    client_secret: String,
    scopes: Option<String>,
}

impl fmt::Debug for GoogleDriveOAuthApplicationPayload {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GoogleDriveOAuthApplicationPayload")
            .field("client_id", &self.client_id)
            .field("client_secret", &"***REDACTED***")
            .field("scopes", &self.scopes)
            .finish()
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct GoogleDriveAuthorizationV1 {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    scopes: Vec<String>,
    #[serde(default)]
    expires_at: Option<DateTime<Utc>>,
    status: StorageCredentialStatus,
    #[serde(default)]
    status_reason: Option<String>,
    #[serde(default)]
    authorized_at: Option<DateTime<Utc>>,
    #[serde(default)]
    last_refreshed_at: Option<DateTime<Utc>>,
    #[serde(default)]
    last_validated_at: Option<DateTime<Utc>>,
}

impl fmt::Debug for GoogleDriveAuthorizationV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GoogleDriveAuthorizationV1")
            .field("access_token", &"***REDACTED***")
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "***REDACTED***"),
            )
            .field("scopes", &self.scopes)
            .field("expires_at", &self.expires_at)
            .field("status", &self.status)
            .finish()
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct GoogleDriveFlowContext {
    client_id: String,
    client_secret_ciphertext: String,
    scopes: Vec<String>,
}

#[derive(Clone)]
struct GoogleDriveRuntimeCredential {
    access_token: String,
    refresh_token: Option<String>,
    expires_at: Option<DateTime<Utc>>,
    client_id: String,
    client_secret: String,
}

struct GoogleDriveTokenProvider {
    client_id: String,
    client_secret: SecretString,
    refresh_token: Option<SecretString>,
    cache: Mutex<(String, Option<DateTime<Utc>>)>,
}

impl fmt::Debug for GoogleDriveTokenProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GoogleDriveTokenProvider")
            .field("client_id", &self.client_id)
            .field("client_secret", &"***REDACTED***")
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "***REDACTED***"),
            )
            .finish()
    }
}

#[async_trait]
impl GoogleDriveAccessTokenProvider for GoogleDriveTokenProvider {
    async fn access_token(&self) -> aster_drive_storage::Result<String> {
        let cache = self.cache.lock().await;
        if cache
            .1
            .is_some_and(|expires_at| expires_at > Utc::now() + Duration::seconds(60))
            || self.refresh_token.is_none()
        {
            return Ok(cache.0.clone());
        }
        drop(cache);
        self.refresh_access_token().await
    }

    async fn refresh_access_token(&self) -> aster_drive_storage::Result<String> {
        let refresh = self.refresh_token.as_ref().ok_or_else(|| {
            aster_drive_storage::storage_driver_error(
                aster_drive_storage::StorageErrorKind::Auth,
                "Google Drive credential has no refresh token",
            )
        })?;
        let token = oauth::refresh_token(
            &self.client_id,
            Some(&self.client_secret),
            refresh.expose_secret(),
        )
        .await
        .map_err(|error| {
            aster_drive_storage::storage_driver_error(
                aster_drive_storage::StorageErrorKind::Auth,
                error.to_string(),
            )
        })?;
        let mut cache = self.cache.lock().await;
        cache.0 = token.access_token.clone();
        cache.1 = token
            .expires_in
            .and_then(|seconds| (seconds > 0).then(|| Utc::now() + Duration::seconds(seconds)));
        Ok(cache.0.clone())
    }
}

impl GoogleDriveConnector {
    pub const ID: &'static str = "asterdrive.storage.google_drive";

    fn decode_config(policy: &storage_policy::Model) -> Result<GoogleDriveConnectorConfigV1> {
        super::common::decode_typed_policy_config(policy, Self::ID, SCHEMA_VERSION)
            .map(|(config, _)| config)
    }

    fn validate_config(config: &GoogleDriveConnectorConfigV1) -> Result<()> {
        if !config.app_data_folder && config.root_folder_id.trim().is_empty() {
            return Err(AsterError::validation_error(
                "Google Drive root_folder_id is required unless app_data_folder is enabled",
            ));
        }
        if config.chunk_size < 256 * 1024
            || config.chunk_size > 64 * 1024 * 1024
            || config.chunk_size % (256 * 1024) != 0
        {
            return Err(AsterError::validation_error(
                "Google Drive chunk_size must be aligned to 256 KiB and between 256 KiB and 64 MiB",
            ));
        }
        Ok(())
    }

    fn descriptor_definition() -> StorageConnectorDescriptor {
        StorageConnectorDescriptor {
            connector_id: aster_drive_storage::ConnectorId::declared(Self::ID),
            label: "Google Drive".to_string(),
            description: "Administrator-managed Google Drive storage policy".to_string(),
            ui: storage_connector_ui_descriptor(StorageConnectorUiDescriptorInput {
                label_key: "driver_type_google_drive",
                description_key: "policy_wizard_google_drive_storage_desc",
                badge_rgb: StorageConnectorBadgeRgb::new(66, 133, 244),
                helper_key: "policy_wizard_google_drive_helper",
                config_step_title_key: "policy_wizard_step_google_drive_title",
                config_step_description_key: "policy_wizard_step_google_drive_desc",
                edit_context_key: "policy_edit_context_google_drive_desc",
                base_path_empty_display: "core:root",
                base_path_placeholder: "tenant/prefix",
            }),
            credential_mode: StorageConnectorCredentialMode::OauthDelegated,
            deployment_scope: StorageConnectorDeploymentScope::SharedAcrossPrimaryInstances,
            supports_initial_setup: false,
            requires_authorization: true,
            authorization_provider: Some("google_drive".to_string()),
            credential_management: Some(StorageConnectorCredentialManagementDescriptor {
                title_key: "google_drive_credential_title".to_string(),
                loading_key: "google_drive_credential_loading".to_string(),
                status_presentations: BTreeMap::from([
                    (
                        "missing".to_string(),
                        StorageConnectorCredentialStatusPresentation {
                            label_key: "google_drive_credential_status_missing".to_string(),
                            tone: StorageConnectorCredentialStatusTone::Warning,
                            description_key: None,
                            attention_title_key: None,
                            attention_guidance_key: None,
                            reason_rules: Vec::new(),
                            reason_fallback_key: None,
                        },
                    ),
                    (
                        "authorized".to_string(),
                        StorageConnectorCredentialStatusPresentation {
                            label_key: "google_drive_credential_status_authorized".to_string(),
                            tone: StorageConnectorCredentialStatusTone::Success,
                            description_key: None,
                            attention_title_key: None,
                            attention_guidance_key: None,
                            reason_rules: Vec::new(),
                            reason_fallback_key: None,
                        },
                    ),
                    (
                        "reauth_required".to_string(),
                        StorageConnectorCredentialStatusPresentation {
                            label_key: "google_drive_credential_status_reauth_required".to_string(),
                            tone: StorageConnectorCredentialStatusTone::Warning,
                            description_key: None,
                            attention_title_key: None,
                            attention_guidance_key: None,
                            reason_rules: Vec::new(),
                            reason_fallback_key: None,
                        },
                    ),
                    (
                        "permission_denied".to_string(),
                        StorageConnectorCredentialStatusPresentation {
                            label_key: "google_drive_credential_status_permission_denied"
                                .to_string(),
                            tone: StorageConnectorCredentialStatusTone::Danger,
                            description_key: None,
                            attention_title_key: None,
                            attention_guidance_key: None,
                            reason_rules: Vec::new(),
                            reason_fallback_key: None,
                        },
                    ),
                    (
                        "invalid".to_string(),
                        StorageConnectorCredentialStatusPresentation {
                            label_key: "google_drive_credential_status_invalid".to_string(),
                            tone: StorageConnectorCredentialStatusTone::Danger,
                            description_key: None,
                            attention_title_key: None,
                            attention_guidance_key: None,
                            reason_rules: Vec::new(),
                            reason_fallback_key: None,
                        },
                    ),
                ]),
                reauthorize_action_key: Some("google_drive_reauthorize_action".to_string()),
                authorized_at_key: None,
                refreshed_at_key: None,
                validated_at_key: Some("google_drive_credential_validated_at".to_string()),
                redirect_uri_key: None,
                redirect_uri_help_key: None,
                redirect_uri_copy_key: None,
                save_before_authorize_key: None,
                authorization_started_key: None,
                save_before_validate_key: None,
                validation_success_key: None,
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
                simple_upload_capabilities: server_relay_simple_upload_capabilities(None),
                stream_upload: true,
                object_multipart_upload: false,
                object_multipart_upload_capabilities: None,
                provider_resumable_upload: true,
                presigned_upload: false,
                frontend_direct_provider_resumable_upload: false,
                provider_resumable_upload_capabilities: Some({
                    let c =
                        crate::storage::drivers::google_drive::google_drive_upload_capabilities();
                    aster_drive_storage::connector_descriptor::StorageConnectorProviderResumableUploadCapabilities { provider: c.provider.to_string(), session_label: c.session_label.to_string(), min_fragment_size: c.min_fragment_size, default_fragment_size: c.default_fragment_size, max_fragment_size: c.max_fragment_size, fragment_alignment: c.fragment_alignment, max_simple_upload_size: c.max_simple_upload_size, frontend_direct_upload: c.frontend_direct_upload, implicit_completion: c.implicit_completion, abort_supported: c.abort_supported, status_query_supported: c.status_query_supported }
                }),
            },
            fields: GoogleDriveConnectorConfigV1::descriptor_fields(),
            config_schema_version: SCHEMA_VERSION,
            credential_schema_version: Some(SCHEMA_VERSION),
            actions: vec![
                saved_connection_test_action_descriptor(true),
                start_authorization_action_descriptor(),
                validate_credential_action_descriptor(),
            ],
            promotions: Vec::new(),
            related_issues: vec![128],
        }
    }
}

#[async_trait]
impl StorageConnector for GoogleDriveConnector {
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
        let normalized =
            aster_drive_storage::connector_descriptor::normalize_storage_connector_config(
                &self.descriptor(),
                input,
            )
            .map_err(|e| AsterError::validation_error(e.to_string()))?;
        let config: GoogleDriveConnectorConfigV1 =
            super::common::decode_normalized_connector_config(&normalized)?;
        Self::validate_config(&config)?;
        super::common::encode_normalized_connector_config(
            normalized.connector_id,
            normalized.schema_version,
            config,
        )
    }

    fn validate_credential_input(&self, input: &StorageConnectorCredentialInput) -> Result<()> {
        let app: GoogleDriveAuthorizationApplicationV1 =
            super::common::decode_authorization_application(input, Self::ID)?;
        if app.client_id.trim().is_empty() || app.client_secret.trim().is_empty() {
            return Err(AsterError::validation_error(
                "Google OAuth client_id and client_secret are required",
            ));
        }
        Ok(())
    }

    async fn persist_credential(
        &self,
        db: &sea_orm::DatabaseTransaction,
        key: &str,
        policy_id: i64,
        _config: &aster_drive_storage::ConnectorConfigEnvelope,
        credential: StorageConnectorCredentialInput,
    ) -> Result<()> {
        let app: GoogleDriveAuthorizationApplicationV1 =
            super::common::decode_authorization_application(&credential, Self::ID)?;
        let existing =
            storage_policy_connector_credential_repo::find_by_policy(db, policy_id).await?;
        let authorization = existing
            .as_ref()
            .and_then(|record| {
                super::decode_typed_connector_credential::<GoogleDriveCredentialV1>(
                    key,
                    record,
                    &aster_drive_storage::ConnectorId::declared(Self::ID),
                    SCHEMA_VERSION,
                )
                .ok()
            })
            .and_then(|payload| payload.authorization);
        super::persist_connector_credential_payload(
            db,
            key,
            policy_id,
            &aster_drive_storage::ConnectorId::declared(Self::ID),
            SCHEMA_VERSION,
            &GoogleDriveCredentialV1 {
                application: GoogleDriveOAuthApplicationPayload {
                    client_id: app.client_id,
                    client_secret: app.client_secret,
                    scopes: app.scopes,
                },
                authorization,
            },
        )
        .await
    }

    async fn start_authorization(
        &self,
        context: &super::StorageConnectorContext<'_>,
        policy: &storage_policy::Model,
        redirect_uri: &str,
    ) -> Result<StorageConnectorAuthorizationStart> {
        let record = storage_policy_connector_credential_repo::find_by_policy(
            context.writer_db(),
            policy.id,
        )
        .await?
        .ok_or_else(|| {
            AsterError::validation_error(
                "save the Google Drive OAuth application before starting authorization",
            )
        })?;
        let payload: GoogleDriveCredentialV1 = super::decode_typed_connector_credential(
            &context.config().auth.storage_credential_secret_key,
            &record,
            &aster_drive_storage::ConnectorId::declared(Self::ID),
            SCHEMA_VERSION,
        )?;
        let app = payload.application;
        let scopes = normalize_scopes(app.scopes.as_deref());
        let state = format!("storage_oauth_{}", id::new_short_token());
        let state_hash = crate::services::storage_policy::credential::crypto::token_hash(&state);
        let verifier = oauth::build_pkce_verifier();
        let secret = crate::services::storage_policy::credential::crypto::encrypt_token(
            &context.config().auth.storage_credential_secret_key,
            oauth::flow_client_secret_aad(policy.id, &state_hash).as_bytes(),
            &app.client_secret,
        )?;
        let flow_context = GoogleDriveFlowContext {
            client_id: app.client_id.clone(),
            client_secret_ciphertext: secret,
            scopes: scopes.clone(),
        };
        let authorization_url = oauth::authorization_url(
            &app.client_id,
            redirect_uri,
            &scopes,
            &state,
            &oauth::build_pkce_challenge(&verifier),
        )?;
        Ok(StorageConnectorAuthorizationStart {
            provider: StorageCredentialProvider::GoogleDrive,
            authorization_url,
            expires_in: AUTHORIZATION_FLOW_TTL_SECS,
            state,
            pkce_verifier: Some(verifier),
            scopes,
            context: serde_json::to_string(&flow_context)
                .map_err(|e| AsterError::internal_error(e.to_string()))?,
            audit: StorageConnectorAuthorizationAudit {
                provider: StorageCredentialProvider::GoogleDrive,
                fields: BTreeMap::from([(
                    String::from("client_id_configured"),
                    serde_json::Value::Bool(true),
                )])
                .into_iter()
                .collect(),
            },
        })
    }

    async fn finish_authorization(
        &self,
        context: &super::StorageConnectorContext<'_>,
        policy: &storage_policy::Model,
        flow: &aster_drive_model::entities::storage_policy_authorization_flow::Model,
        code: &str,
        now: DateTime<Utc>,
    ) -> std::result::Result<
        StorageConnectorAuthorizationCallback,
        StorageConnectorAuthorizationError,
    > {
        if flow.provider != StorageCredentialProvider::GoogleDrive {
            return Err(StorageConnectorAuthorizationError::new(
                StorageAuthorizationFailureReason::UnsupportedProvider,
                AsterError::unsupported_driver("Google Drive authorization provider mismatch"),
            ));
        }
        let flow_context: GoogleDriveFlowContext =
            serde_json::from_str(&flow.context).map_err(|e| {
                StorageConnectorAuthorizationError::new(
                    StorageAuthorizationFailureReason::ServerError,
                    AsterError::database_operation(e.to_string()),
                )
            })?;
        let verifier = flow.pkce_verifier.as_deref().ok_or_else(|| {
            StorageConnectorAuthorizationError::new(
                StorageAuthorizationFailureReason::ServerError,
                AsterError::database_operation(
                    "Google Drive authorization flow missing PKCE verifier",
                ),
            )
        })?;
        let secret = crate::services::storage_policy::credential::crypto::decrypt_token(
            &context.config().auth.storage_credential_secret_key,
            oauth::flow_client_secret_aad(policy.id, &flow.state_hash).as_bytes(),
            &flow_context.client_secret_ciphertext,
        )
        .map(SecretString::from)
        .map_err(|e| {
            StorageConnectorAuthorizationError::new(
                StorageAuthorizationFailureReason::ServerError,
                e,
            )
        })?;
        let token = oauth::exchange_code(
            &flow_context.client_id,
            Some(&secret),
            code,
            &flow.redirect_uri,
            verifier,
        )
        .await
        .map_err(|e| {
            StorageConnectorAuthorizationError::new(
                StorageAuthorizationFailureReason::TokenExchangeFailed,
                e,
            )
        })?;
        let expires_at = token
            .expires_in
            .and_then(|seconds| (seconds > 0).then(|| now + Duration::seconds(seconds)));
        let authorization = GoogleDriveAuthorizationV1 {
            access_token: token.access_token,
            refresh_token: token.refresh_token,
            scopes: normalize_scopes(token.scope.as_deref())
                .into_iter()
                .chain(flow_context.scopes.clone())
                .collect(),
            expires_at,
            status: StorageCredentialStatus::Authorized,
            status_reason: None,
            authorized_at: Some(now),
            last_refreshed_at: None,
            last_validated_at: None,
        };
        let payload = GoogleDriveCredentialV1 {
            application: GoogleDriveOAuthApplicationPayload {
                client_id: flow_context.client_id,
                client_secret: secret.expose_secret().to_string(),
                scopes: Some(flow_context.scopes.join(" ")),
            },
            authorization: Some(authorization),
        };
        Ok(StorageConnectorAuthorizationCallback {
            credential_payload: serde_json::to_value(payload).map_err(|e| {
                StorageConnectorAuthorizationError::new(
                    StorageAuthorizationFailureReason::ServerError,
                    AsterError::internal_error(e.to_string()),
                )
            })?,
            audit: StorageConnectorAuthorizationAudit {
                provider: StorageCredentialProvider::GoogleDrive,
                fields: serde_json::Map::new(),
            },
        })
    }

    async fn build_driver_from_connection(
        &self,
        _context: &super::StorageConnectorContext<'_>,
        _config: &aster_drive_storage::ConnectorConfigEnvelope,
        _credential: &StorageConnectorCredentialInput,
    ) -> Result<Box<dyn StorageDriver>> {
        Err(AsterError::unsupported_driver(
            "Google Drive draft connection tests require an authorized policy",
        ))
    }

    fn build_runtime_driver(
        &self,
        registry: &crate::storage::DriverRegistry,
        policy: &storage_policy::Model,
    ) -> Result<super::StorageConnectorDriver> {
        let credential = registry.get_runtime_credential(policy.id).ok_or_else(|| {
            storage_driver_error(
                aster_drive_storage::StorageErrorKind::Auth,
                "Google Drive storage policy is missing authorized credentials",
            )
        })?;
        Ok(super::StorageConnectorDriver::storage(
            self.build_authorized_driver(policy, credential)?,
        ))
    }
    fn upload_transport(
        &self,
        _policy: &storage_policy::Model,
    ) -> Result<StorageConnectorUploadTransport> {
        Ok(StorageConnectorUploadTransport::ProviderResumable(
            ProviderResumableUploadStrategy::ServerRelay,
        ))
    }
    fn direct_download_enabled(&self, _policy: &storage_policy::Model) -> Result<bool> {
        Ok(false)
    }
    fn credential_info(
        &self,
        config: &crate::config::Config,
        credential: &aster_drive_model::entities::storage_policy_connector_credential::Model,
    ) -> Result<Option<StorageConnectorCredentialInfo>> {
        let payload: GoogleDriveCredentialV1 = super::decode_typed_connector_credential(
            &config.auth.storage_credential_secret_key,
            credential,
            &aster_drive_storage::ConnectorId::declared(Self::ID),
            SCHEMA_VERSION,
        )?;
        let Some(auth) = payload.authorization else {
            return Ok(None);
        };
        Ok(Some(StorageConnectorCredentialInfo {
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
        let mut payload: GoogleDriveCredentialV1 = super::decode_typed_connector_credential(
            &config.auth.storage_credential_secret_key,
            credential,
            &aster_drive_storage::ConnectorId::declared(Self::ID),
            SCHEMA_VERSION,
        )?;
        let Some(authorization) = payload.authorization.as_mut() else {
            return Ok(None);
        };
        authorization.status = status;
        authorization.status_reason = Some(reason.to_string());
        serde_json::to_value(payload).map(Some).map_err(|error| {
            AsterError::internal_error(format!(
                "serialize Google Drive credential validation failure: {error}"
            ))
        })
    }
    async fn load_runtime_credential(
        &self,
        _db: &sea_orm::DatabaseConnection,
        config: &crate::config::Config,
        _policy: &storage_policy::Model,
        credential: &aster_drive_model::entities::storage_policy_connector_credential::Model,
    ) -> Result<Option<StorageConnectorRuntimeCredential>> {
        let payload: GoogleDriveCredentialV1 = super::decode_typed_connector_credential(
            &config.auth.storage_credential_secret_key,
            credential,
            &aster_drive_storage::ConnectorId::declared(Self::ID),
            SCHEMA_VERSION,
        )?;
        let Some(auth) = payload.authorization else {
            return Ok(None);
        };
        if auth.status != StorageCredentialStatus::Authorized || auth.access_token.trim().is_empty()
        {
            return Ok(None);
        }
        Ok(Some(StorageConnectorRuntimeCredential::new(
            aster_drive_storage::ConnectorId::declared(Self::ID),
            GoogleDriveRuntimeCredential {
                access_token: auth.access_token,
                refresh_token: auth.refresh_token,
                expires_at: auth.expires_at,
                client_id: payload.application.client_id,
                client_secret: payload.application.client_secret,
            },
        )))
    }
    fn build_authorized_driver(
        &self,
        policy: &storage_policy::Model,
        credential: StorageConnectorRuntimeCredential,
    ) -> Result<Arc<dyn StorageDriver>> {
        let config = Self::decode_config(policy)?;
        let runtime = credential.require::<GoogleDriveRuntimeCredential>(Self::ID)?;
        let provider = GoogleDriveTokenProvider {
            client_id: runtime.client_id.clone(),
            client_secret: SecretString::from(runtime.client_secret.clone()),
            refresh_token: runtime.refresh_token.clone().map(SecretString::from),
            cache: Mutex::new((runtime.access_token.clone(), runtime.expires_at)),
        };
        let client = GoogleDriveClient::new(GoogleDriveClientConfig::with_token_provider(
            Arc::new(provider),
        ))?;
        Ok(Arc::new(GoogleDriveDriver::new(
            client,
            config.base_path,
            config.root_folder_id,
            config.shared_drive_id,
            config.app_data_folder,
            policy.id,
            config.chunk_size,
        )?))
    }
    async fn validate_credential(
        &self,
        db: &sea_orm::DatabaseConnection,
        config: &crate::config::Config,
        policy: &storage_policy::Model,
        credential: &aster_drive_model::entities::storage_policy_connector_credential::Model,
    ) -> Result<super::StorageCredentialValidationOutcome> {
        let connector_config = Self::decode_config(policy)?;
        Self::validate_config(&connector_config)?;
        let mut payload: GoogleDriveCredentialV1 = super::decode_typed_connector_credential(
            &config.auth.storage_credential_secret_key,
            credential,
            &aster_drive_storage::ConnectorId::declared(Self::ID),
            SCHEMA_VERSION,
        )?;
        let authorization = payload.authorization.as_ref().ok_or_else(|| {
            AsterError::validation_error(
                "Google Drive connector credential has not been authorized",
            )
        })?;
        if authorization.status != StorageCredentialStatus::Authorized
            || authorization.access_token.trim().is_empty()
        {
            return Err(AsterError::validation_error(
                "Google Drive connector credential is not authorized",
            ));
        }
        let provider = GoogleDriveTokenProvider {
            client_id: payload.application.client_id.clone(),
            client_secret: SecretString::from(payload.application.client_secret.clone()),
            refresh_token: authorization.refresh_token.clone().map(SecretString::from),
            cache: Mutex::new((authorization.access_token.clone(), authorization.expires_at)),
        };
        let client = GoogleDriveClient::new(GoogleDriveClientConfig::with_token_provider(
            Arc::new(provider),
        ))?;
        client.about_storage().await?;
        if !connector_config.app_data_folder {
            client
                .get_folder(
                    &connector_config.root_folder_id,
                    connector_config.shared_drive_id.as_deref(),
                )
                .await?;
        }
        let current = storage_policy_connector_credential_repo::find_by_policy(db, policy.id)
            .await?
            .ok_or_else(|| AsterError::record_not_found("storage policy connector credential"))?;
        payload = super::decode_typed_connector_credential(
            &config.auth.storage_credential_secret_key,
            &current,
            &aster_drive_storage::ConnectorId::declared(Self::ID),
            SCHEMA_VERSION,
        )?;
        let auth = payload.authorization.as_mut().ok_or_else(|| {
            AsterError::validation_error(
                "Google Drive connector credential has not been authorized",
            )
        })?;
        auth.status = StorageCredentialStatus::Authorized;
        auth.status_reason = None;
        auth.last_validated_at = Some(Utc::now());
        Ok(super::StorageCredentialValidationOutcome {
            credential: current,
            credential_payload: serde_json::to_value(payload).map_err(|error| {
                AsterError::internal_error(format!(
                    "serialize validated Google Drive connector credential: {error}"
                ))
            })?,
            root_item_id: connector_config.root_folder_id,
            root_item_name: None,
        })
    }
}

fn normalize_scopes(value: Option<&str>) -> Vec<String> {
    let mut result = Vec::new();
    for scope in value
        .unwrap_or("https://www.googleapis.com/auth/drive")
        .split_whitespace()
    {
        if !scope.is_empty() && !result.iter().any(|item| item == scope) {
            result.push(scope.to_string());
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptor_is_self_contained_and_requires_authorization() {
        let connector = GoogleDriveConnector;
        let descriptor = connector.descriptor();
        assert_eq!(descriptor.connector_id.as_str(), GoogleDriveConnector::ID);
        assert_eq!(
            descriptor.authorization_provider.as_deref(),
            Some("google_drive")
        );
        assert!(descriptor.requires_authorization);
        assert!(connector.localization().is_ok());
    }

    #[test]
    fn config_rejects_unaligned_chunks_and_missing_root() {
        assert!(
            GoogleDriveConnector::validate_config(&GoogleDriveConnectorConfigV1 {
                base_path: String::new(),
                root_folder_id: String::new(),
                shared_drive_id: None,
                app_data_folder: false,
                chunk_size: default_chunk_size(),
                provider_resumable_upload_strategy: ProviderResumableUploadStrategy::ServerRelay,
                provider_download_strategy: ProviderDownloadStrategy::ServerRelay,
            })
            .is_err()
        );
    }
}
