use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use std::sync::Arc;

use crate::storage::StoragePolicyExecutableAction;
use crate::storage::drivers::onedrive::MicrosoftGraphAccessTokenProvider;
use crate::types::{
    DriverType, MicrosoftGraphCloud, RemoteNodeTransportMode, StorageCredentialKind,
    StorageCredentialProvider,
};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct MicrosoftGraphApplicationConfigInput {
    pub cloud: Option<MicrosoftGraphCloud>,
    pub tenant: Option<String>,
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    pub scopes: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StorageConnectorApplicationConfigInput {
    pub microsoft_graph: Option<MicrosoftGraphApplicationConfigInput>,
}

impl StorageConnectorApplicationConfigInput {
    pub fn is_empty(&self) -> bool {
        self.microsoft_graph.is_none()
    }
}

#[derive(Debug, Clone)]
pub struct StorageConnectorConnectionInput {
    pub driver_type: DriverType,
    pub endpoint: String,
    pub bucket: String,
    pub access_key: String,
    pub secret_key: String,
    pub base_path: String,
    pub remote_node_id: Option<i64>,
    pub options: crate::types::StoragePolicyOptions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StorageConnectorCredentialRequirement {
    pub provider: StorageCredentialProvider,
    pub credential_kind: StorageCredentialKind,
    pub requires_application_config: bool,
    pub requires_authorization: bool,
}

#[derive(Clone)]
pub(crate) struct OneDriveCredentialRuntime {
    pub token_provider: Arc<dyn MicrosoftGraphAccessTokenProvider>,
    pub drive_id: Option<String>,
    pub root_item_id: Option<String>,
}

#[derive(Clone)]
pub(crate) enum StorageConnectorRuntimeCredential {
    MicrosoftGraph(OneDriveCredentialRuntime),
}

#[derive(Debug, Clone)]
pub(crate) struct StorageCredentialValidationOutcome {
    pub account_label: Option<String>,
    pub subject: Option<String>,
    pub metadata: String,
    pub root_item_id: String,
    pub root_item_name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ExecuteSavedStorageConnectorActionInput {
    pub action: StoragePolicyExecutableAction,
}

#[derive(Debug, Clone)]
pub struct ExecuteDraftStorageConnectorActionInput {
    pub action: StoragePolicyExecutableAction,
    pub policy_id: Option<i64>,
    pub connection: StorageConnectorConnectionInput,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct TencentCosCorsConfigResult {
    pub rule_id: String,
    pub allowed_origins: Vec<String>,
    pub request_id: Option<String>,
    pub preserved_rule_count: usize,
    pub replaced_existing_rule: bool,
    pub response_vary: bool,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StorageConnectorActionResult {
    pub action: StoragePolicyExecutableAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tencent_cos_cors: Option<TencentCosCorsConfigResult>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct StoragePolicyCleanupRemoteNodeSnapshot {
    pub id: i64,
    pub name: String,
    pub base_url: String,
    #[serde(default)]
    pub transport_mode: RemoteNodeTransportMode,
    pub access_key_ciphertext: String,
    pub secret_key_ciphertext: String,
    #[serde(default)]
    pub last_capabilities: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct StoragePolicyCleanupOneDriveCredentialSnapshot {
    pub cloud: crate::types::MicrosoftGraphCloud,
    #[serde(default)]
    pub tenant_id: Option<String>,
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub client_secret_ciphertext: Option<String>,
    pub drive_id: String,
    pub root_item_id: String,
    pub access_token_ciphertext: String,
    #[serde(default)]
    pub refresh_token_ciphertext: Option<String>,
    #[serde(default)]
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum StoragePolicyCleanupDriverSnapshot {
    RemoteNode(StoragePolicyCleanupRemoteNodeSnapshot),
    MicrosoftGraph(StoragePolicyCleanupOneDriveCredentialSnapshot),
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct StoragePolicyCleanupSnapshots<'a> {
    pub driver_snapshot: Option<&'a StoragePolicyCleanupDriverSnapshot>,
    pub legacy_onedrive_credential: Option<&'a StoragePolicyCleanupOneDriveCredentialSnapshot>,
    pub legacy_remote_node: Option<&'a StoragePolicyCleanupRemoteNodeSnapshot>,
}

impl From<crate::storage::drivers::tencent_cos::cors::TencentCosCorsApplyResult>
    for TencentCosCorsConfigResult
{
    fn from(value: crate::storage::drivers::tencent_cos::cors::TencentCosCorsApplyResult) -> Self {
        Self {
            rule_id: value.rule_id,
            allowed_origins: value.allowed_origins,
            request_id: value.request_id,
            preserved_rule_count: value.preserved_rule_count,
            replaced_existing_rule: value.replaced_existing_rule,
            response_vary: value.response_vary,
        }
    }
}
