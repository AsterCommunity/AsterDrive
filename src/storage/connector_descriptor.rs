//! Storage connector descriptors for admin policy UI capability discovery.

use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::types::DriverType;

pub trait StorageConnectorDescriptorProvider {
    fn storage_connector_descriptor() -> StorageConnectorDescriptor;

    fn storage_connector_supports_action(action: StorageConnectorAction) -> bool {
        Self::storage_connector_descriptor()
            .actions
            .iter()
            .any(|descriptor| descriptor.action == action)
    }

    fn storage_connector_supports_policy_action(action: StoragePolicyExecutableAction) -> bool {
        Self::storage_connector_descriptor()
            .actions
            .iter()
            .any(|descriptor| {
                descriptor.kind == StorageConnectorActionKind::PolicyAction
                    && descriptor.action == action.into()
            })
    }

    fn storage_connector_supports_draft_connection_test() -> bool {
        Self::storage_connector_descriptor()
            .actions
            .iter()
            .any(|descriptor| {
                descriptor.action == StorageConnectorAction::TestDraftConnection
                    && descriptor.kind == StorageConnectorActionKind::ConnectionTest
            })
    }

    fn storage_connector_supports_saved_connection_test() -> bool {
        Self::storage_connector_descriptor()
            .actions
            .iter()
            .any(|descriptor| {
                descriptor.action == StorageConnectorAction::TestSavedConnection
                    && descriptor.kind == StorageConnectorActionKind::ConnectionTest
            })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum StorageConnectorCredentialMode {
    None,
    StaticSecret,
    RemoteNode,
    OauthDelegated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum StorageConnectorFieldScope {
    Connection,
    PolicyOptions,
    ApplicationCredential,
    RemoteNodeBinding,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum StorageConnectorFieldKind {
    Text,
    Secret,
    Select,
    Boolean,
    Number,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum StorageConnectorAction {
    ConfigureTencentCosCors,
    StartAuthorization,
    ValidateCredential,
    TestDraftConnection,
    TestSavedConnection,
}

impl StorageConnectorAction {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ConfigureTencentCosCors => "configure_tencent_cos_cors",
            Self::StartAuthorization => "start_authorization",
            Self::ValidateCredential => "validate_credential",
            Self::TestDraftConnection => "test_draft_connection",
            Self::TestSavedConnection => "test_saved_connection",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum StoragePolicyExecutableAction {
    ConfigureTencentCosCors,
}

impl StoragePolicyExecutableAction {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ConfigureTencentCosCors => "configure_tencent_cos_cors",
        }
    }

    pub const fn mutates_remote_state(self) -> bool {
        match self {
            Self::ConfigureTencentCosCors => true,
        }
    }
}

impl From<StoragePolicyExecutableAction> for StorageConnectorAction {
    fn from(value: StoragePolicyExecutableAction) -> Self {
        match value {
            StoragePolicyExecutableAction::ConfigureTencentCosCors => Self::ConfigureTencentCosCors,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum StorageConnectorActionKind {
    PolicyAction,
    Authorization,
    CredentialValidation,
    ConnectionTest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum StorageConnectorActionEndpoint {
    ExecuteDraftStoragePolicyAction,
    ExecuteSavedStoragePolicyAction,
    StartStorageAuthorization,
    ValidateStoragePolicyCredential,
    TestPolicyParams,
    TestPolicyConnection,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StorageConnectorActionDescriptor {
    pub action: StorageConnectorAction,
    pub kind: StorageConnectorActionKind,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub endpoints: Vec<StorageConnectorActionEndpoint>,
    pub requires_saved_policy: bool,
    pub requires_authorization: bool,
    pub mutates_remote_state: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StorageConnectorUploadWorkflows {
    pub simple_upload: bool,
    pub stream_upload: bool,
    pub object_multipart_upload: bool,
    pub provider_resumable_upload: bool,
    pub presigned_upload: bool,
    pub frontend_direct_provider_resumable_upload: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StorageConnectorCapabilities {
    pub efficient_range: bool,
    pub capacity: bool,
    pub list: bool,
    pub presigned_download: bool,
    pub storage_native_thumbnail: bool,
    pub storage_native_media_metadata: bool,
    pub remote_node_binding: bool,
    pub s3_transfer_strategy: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StorageConnectorFieldDescriptor {
    pub name: String,
    pub scope: StorageConnectorFieldScope,
    pub kind: StorageConnectorFieldKind,
    pub required: bool,
    pub secret: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub legacy_policy_field: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StorageConnectorDescriptor {
    pub driver_type: DriverType,
    pub enabled: bool,
    pub label: String,
    pub description: String,
    pub credential_mode: StorageConnectorCredentialMode,
    pub requires_authorization: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authorization_provider: Option<String>,
    pub capabilities: StorageConnectorCapabilities,
    pub upload_workflows: StorageConnectorUploadWorkflows,
    pub fields: Vec<StorageConnectorFieldDescriptor>,
    pub actions: Vec<StorageConnectorActionDescriptor>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related_issues: Vec<u16>,
}

pub(crate) fn object_storage_connector_descriptor(
    driver_type: DriverType,
    label: &str,
    description: &str,
    storage_native_processing: bool,
    related_issues: Vec<u16>,
) -> StorageConnectorDescriptor {
    StorageConnectorDescriptor {
        driver_type,
        enabled: true,
        label: label.to_string(),
        description: description.to_string(),
        credential_mode: StorageConnectorCredentialMode::StaticSecret,
        requires_authorization: false,
        authorization_provider: None,
        capabilities: StorageConnectorCapabilities {
            efficient_range: true,
            capacity: true,
            list: true,
            presigned_download: true,
            storage_native_thumbnail: storage_native_processing,
            storage_native_media_metadata: storage_native_processing,
            remote_node_binding: false,
            s3_transfer_strategy: true,
        },
        upload_workflows: StorageConnectorUploadWorkflows {
            simple_upload: true,
            stream_upload: true,
            object_multipart_upload: true,
            provider_resumable_upload: false,
            presigned_upload: true,
            frontend_direct_provider_resumable_upload: false,
        },
        fields: vec![
            storage_connector_field(
                "endpoint",
                StorageConnectorFieldScope::Connection,
                StorageConnectorFieldKind::Text,
                true,
                false,
            ),
            storage_connector_field(
                "bucket",
                StorageConnectorFieldScope::Connection,
                StorageConnectorFieldKind::Text,
                true,
                false,
            ),
            storage_connector_field(
                "access_key",
                StorageConnectorFieldScope::Connection,
                StorageConnectorFieldKind::Text,
                true,
                false,
            ),
            storage_connector_field(
                "secret_key",
                StorageConnectorFieldScope::Connection,
                StorageConnectorFieldKind::Secret,
                true,
                true,
            ),
            storage_connector_field(
                "base_path",
                StorageConnectorFieldScope::Connection,
                StorageConnectorFieldKind::Text,
                false,
                false,
            ),
        ],
        actions: vec![
            draft_connection_test_action_descriptor(),
            saved_connection_test_action_descriptor(false),
        ],
        related_issues,
    }
}

pub(crate) fn policy_action_descriptor(
    action: StoragePolicyExecutableAction,
) -> StorageConnectorActionDescriptor {
    StorageConnectorActionDescriptor {
        action: action.into(),
        kind: StorageConnectorActionKind::PolicyAction,
        endpoints: vec![
            StorageConnectorActionEndpoint::ExecuteDraftStoragePolicyAction,
            StorageConnectorActionEndpoint::ExecuteSavedStoragePolicyAction,
        ],
        requires_saved_policy: false,
        requires_authorization: false,
        mutates_remote_state: action.mutates_remote_state(),
    }
}

pub(crate) fn start_authorization_action_descriptor() -> StorageConnectorActionDescriptor {
    StorageConnectorActionDescriptor {
        action: StorageConnectorAction::StartAuthorization,
        kind: StorageConnectorActionKind::Authorization,
        endpoints: vec![StorageConnectorActionEndpoint::StartStorageAuthorization],
        requires_saved_policy: true,
        requires_authorization: false,
        mutates_remote_state: false,
    }
}

pub(crate) fn validate_credential_action_descriptor() -> StorageConnectorActionDescriptor {
    StorageConnectorActionDescriptor {
        action: StorageConnectorAction::ValidateCredential,
        kind: StorageConnectorActionKind::CredentialValidation,
        endpoints: vec![StorageConnectorActionEndpoint::ValidateStoragePolicyCredential],
        requires_saved_policy: true,
        requires_authorization: true,
        mutates_remote_state: false,
    }
}

pub(crate) fn draft_connection_test_action_descriptor() -> StorageConnectorActionDescriptor {
    StorageConnectorActionDescriptor {
        action: StorageConnectorAction::TestDraftConnection,
        kind: StorageConnectorActionKind::ConnectionTest,
        endpoints: vec![StorageConnectorActionEndpoint::TestPolicyParams],
        requires_saved_policy: false,
        requires_authorization: false,
        mutates_remote_state: false,
    }
}

pub(crate) fn saved_connection_test_action_descriptor(
    requires_authorization: bool,
) -> StorageConnectorActionDescriptor {
    StorageConnectorActionDescriptor {
        action: StorageConnectorAction::TestSavedConnection,
        kind: StorageConnectorActionKind::ConnectionTest,
        endpoints: vec![StorageConnectorActionEndpoint::TestPolicyConnection],
        requires_saved_policy: true,
        requires_authorization,
        mutates_remote_state: false,
    }
}

pub(crate) fn storage_connector_field(
    name: &str,
    scope: StorageConnectorFieldScope,
    kind: StorageConnectorFieldKind,
    required: bool,
    secret: bool,
) -> StorageConnectorFieldDescriptor {
    StorageConnectorFieldDescriptor {
        name: name.to_string(),
        scope,
        kind,
        required,
        secret,
        legacy_policy_field: None,
        options: Vec::new(),
    }
}

pub(crate) fn storage_connector_field_with_legacy(
    name: &str,
    scope: StorageConnectorFieldScope,
    kind: StorageConnectorFieldKind,
    required: bool,
    secret: bool,
    legacy_policy_field: &str,
) -> StorageConnectorFieldDescriptor {
    StorageConnectorFieldDescriptor {
        legacy_policy_field: Some(legacy_policy_field.to_string()),
        ..storage_connector_field(name, scope, kind, required, secret)
    }
}

pub(crate) fn storage_connector_field_with_options(
    name: &str,
    scope: StorageConnectorFieldScope,
    kind: StorageConnectorFieldKind,
    required: bool,
    secret: bool,
    options: Vec<&str>,
) -> StorageConnectorFieldDescriptor {
    StorageConnectorFieldDescriptor {
        options: options.into_iter().map(ToOwned::to_owned).collect(),
        ..storage_connector_field(name, scope, kind, required, secret)
    }
}
