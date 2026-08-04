//! Storage connector descriptors for admin policy UI capability discovery.
//!
//! Descriptor 是 connector 对外声明的“配置/管理能力清单”。前端用它决定显示哪些
//! 字段、按钮和提示；后端服务也用它 gate 授权、连接测试、policy action 等入口。
//! 它不是 runtime driver，本文件不应该承载实际对象读写逻辑。

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use aster_drive_model::types::OBJECT_MULTIPART_MIN_PART_SIZE;

use crate::ConnectorId;

use super::field_contract::{StorageDescriptorFieldKind, StorageDescriptorFieldSemantics};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum StorageConnectorCredentialMode {
    /// 不需要密钥或远端绑定，例如纯本地路径。
    None,
    /// 使用 connector 自己定义字段的静态凭据。
    StaticSecret,
    /// 通过已注册 remote node 代理访问。
    RemoteNode,
    /// 需要用户完成 delegated OAuth 授权，例如 Microsoft Graph OneDrive。
    OauthDelegated,
}

/// Connector-backed policy data is visible from which primary instances.
///
/// This is a static connector capability. Deployment-specific filtering and
/// write guards must consume this field instead of maintaining a separate
/// core-owned provider allow/deny list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum StorageConnectorDeploymentScope {
    /// Policy data lives on the primary instance itself and is not shared with
    /// other primary instances.
    InstanceLocal,
    /// Every primary instance can safely resolve the same policy reference.
    SharedAcrossPrimaryInstances,
}

impl StorageConnectorDeploymentScope {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InstanceLocal => "instance_local",
            Self::SharedAcrossPrimaryInstances => "shared_across_primary_instances",
        }
    }

    pub const fn supports_multi_primary(self) -> bool {
        matches!(self, Self::SharedAcrossPrimaryInstances)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum StorageConnectorFieldScope {
    /// 写入当前 connector namespace 下的版本化 typed config envelope。
    ConnectorConfig,
    /// 写入独立的静态凭据通道，不进入 connector config JSON。
    StaticCredential,
    /// 写入授权应用配置，例如 OAuth client id/secret；不直接作为 driver 凭据。
    AuthorizationApplication,
    /// 仅作为一次 connector action 的输入，不进入 policy 或 credential 持久化。
    ActionInput,
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

/// Scalar type submitted by a select field.
///
/// Select is a UI control, not a wire type. Keeping the value type explicit is
/// required for dynamic choices such as a numeric remote node id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum StorageConnectorSelectValueKind {
    String,
    Integer,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum StorageConnectorSelectOptionValue {
    Integer(i64),
    String(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StorageConnectorSelectOption {
    /// Stable value submitted in the connector payload.
    pub value: StorageConnectorSelectOptionValue,
    /// Connector-owned frontend localization key.
    pub label_key: String,
    /// Optional connector-owned explanation for richer selectors.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description_key: Option<String>,
}

/// Platform-provided option catalogs that a connector field can consume.
///
/// The platform owns loading these catalogs. The connector only opts into one
/// and declares field dependencies, so the UI never infers behavior from a
/// provider id or field name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum StorageConnectorSelectDataSource {
    RemoteNodes,
    RemoteStorageTargets,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StorageConnectorSelectDescriptor {
    pub value_kind: StorageConnectorSelectValueKind,
    /// Connector-owned fixed choices. Mutually exclusive with `data_source`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<StorageConnectorSelectOption>,
    /// Platform catalog used to populate choices at runtime.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_source: Option<StorageConnectorSelectDataSource>,
    /// Field whose current value scopes the dynamic catalog.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depends_on: Option<String>,
}

/// Descriptor 可声明的 JSON 标量默认值。
///
/// Provider option 只允许标量配置；credential secret 使用独立 credential/application
/// config 通道，复杂对象也应拆成有明确字段 contract 的标量集合。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum StorageConnectorFieldDefaultValue {
    Boolean(bool),
    Integer(i64),
    String(String),
}

/// Controls when a connector-declared field default is applied.
///
/// Missing values use the descriptor default in both modes. Empty text is
/// distinct because some connector fields model an omitted optional value,
/// while others use an empty form value to request a connector-owned root or
/// local default path.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum StorageConnectorFieldDefaultMode {
    #[default]
    MissingOnly,
    MissingOrEmptyText,
}

impl StorageConnectorFieldDefaultMode {
    fn is_missing_only(&self) -> bool {
        *self == Self::MissingOnly
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StorageConnectorFieldValidation {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_integer: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_integer: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_length: Option<u32>,
}

impl From<StorageDescriptorFieldKind> for StorageConnectorFieldKind {
    fn from(value: StorageDescriptorFieldKind) -> Self {
        match value {
            StorageDescriptorFieldKind::Text => Self::Text,
            StorageDescriptorFieldKind::Secret => Self::Secret,
            StorageDescriptorFieldKind::Select => Self::Select,
            StorageDescriptorFieldKind::Boolean => Self::Boolean,
            StorageDescriptorFieldKind::Number => Self::Number,
        }
    }
}

impl From<StorageConnectorFieldKind> for StorageDescriptorFieldKind {
    fn from(value: StorageConnectorFieldKind) -> Self {
        match value {
            StorageConnectorFieldKind::Text => Self::Text,
            StorageConnectorFieldKind::Secret => Self::Secret,
            StorageConnectorFieldKind::Select => Self::Select,
            StorageConnectorFieldKind::Boolean => Self::Boolean,
            StorageConnectorFieldKind::Number => Self::Number,
        }
    }
}

/// Stable action identity owned by one connector.
///
/// The descriptor carries the action's fields and execution contract. This
/// newtype only prevents action IDs from being mixed with connector IDs and
/// arbitrary field names while crossing registry, API, and audit boundaries.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    derive(ToSchema),
    schema(value_type = String)
)]
pub struct StorageConnectorActionId(String);

impl StorageConnectorActionId {
    pub fn declared(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn validate(&self) -> Result<(), StorageConnectorActionIdError> {
        let value = self.as_str();
        if !(3..=128).contains(&value.len())
            || value.starts_with(['.', '-', '_'])
            || value.ends_with(['.', '-', '_'])
            || value.contains("..")
            || !value.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || b".-_".contains(&byte)
            })
        {
            return Err(StorageConnectorActionIdError);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageConnectorActionIdError;

impl fmt::Display for StorageConnectorActionIdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(
			"action id must be 3-128 lowercase ASCII letters, digits, '.', '-' or '_' and may not start or end with punctuation",
		)
    }
}

impl std::error::Error for StorageConnectorActionIdError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum StorageConnectorActionKind {
    /// Connector/plugin 自定义动作，可能修改远端状态。
    Custom,
    /// 授权流程入口。
    Authorization,
    /// 已授权 credential 校验入口。
    CredentialValidation,
    /// 连接测试入口。
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
    /// Connector 内唯一且稳定的 action ID。
    pub action_id: StorageConnectorActionId,
    /// 前端本地化 label key。
    pub label_key: String,
    /// 前端本地化说明 key。
    pub description_key: String,
    /// 用于把 action 归类到授权、连接测试、policy action 等入口。
    pub kind: StorageConnectorActionKind,
    /// 该 action 可通过哪些后端 endpoint 执行。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub endpoints: Vec<StorageConnectorActionEndpoint>,
    /// Action-owned input schema. Values are never persisted into the policy.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<StorageConnectorFieldDescriptor>,
    /// true 表示必须先保存 policy，draft 参数不能执行。
    pub requires_saved_policy: bool,
    /// true 表示执行前必须存在可用授权凭据。
    pub requires_authorization: bool,
    /// true 表示该动作会修改 provider 远端状态。
    pub mutates_remote_state: bool,
    /// true 表示 UI 在执行前应展示明确确认步骤。
    pub requires_confirmation: bool,
}

impl StorageConnectorActionDescriptor {
    /// Validate the connector-owned action schema before it enters a catalog.
    ///
    /// Registration-time validation prevents a plugin from advertising a UI
    /// contract that the generic dispatcher cannot route or normalize.
    pub fn validate(&self) -> Result<(), StorageConnectorActionDescriptorError> {
        self.action_id
            .validate()
            .map_err(|error| StorageConnectorActionDescriptorError(error.to_string()))?;
        if self.label_key.trim().is_empty() {
            return Err(StorageConnectorActionDescriptorError(
                "action label_key must not be empty".to_string(),
            ));
        }
        if self.description_key.trim().is_empty() {
            return Err(StorageConnectorActionDescriptorError(
                "action description_key must not be empty".to_string(),
            ));
        }
        if self.endpoints.is_empty() {
            return Err(StorageConnectorActionDescriptorError(
                "action must declare at least one endpoint".to_string(),
            ));
        }
        let mut endpoints = Vec::with_capacity(self.endpoints.len());
        for endpoint in &self.endpoints {
            if endpoints.contains(endpoint) {
                return Err(StorageConnectorActionDescriptorError(
                    "action must not declare the same endpoint more than once".to_string(),
                ));
            }
            endpoints.push(*endpoint);
            if !action_kind_accepts_endpoint(self.kind, *endpoint) {
                return Err(StorageConnectorActionDescriptorError(format!(
                    "action kind {:?} does not accept endpoint {:?}",
                    self.kind, endpoint
                )));
            }
        }

        let has_draft_endpoint = self.endpoints.iter().any(|endpoint| {
            matches!(
                endpoint,
                StorageConnectorActionEndpoint::ExecuteDraftStoragePolicyAction
                    | StorageConnectorActionEndpoint::TestPolicyParams
            )
        });
        if self.requires_saved_policy == has_draft_endpoint {
            return Err(StorageConnectorActionDescriptorError(
                "requires_saved_policy must be false exactly when a draft endpoint is declared"
                    .to_string(),
            ));
        }

        let mut field_names = HashSet::with_capacity(self.fields.len());
        for field in &self.fields {
            field.validate().map_err(|error| {
                StorageConnectorActionDescriptorError(format!(
                    "action field '{}' is invalid: {error}",
                    field.name
                ))
            })?;
            if field.scope != StorageConnectorFieldScope::ActionInput {
                return Err(StorageConnectorActionDescriptorError(format!(
                    "action field '{}' must use action_input scope",
                    field.name
                )));
            }
            if !field_names.insert(field.name.as_str()) {
                return Err(StorageConnectorActionDescriptorError(format!(
                    "action field '{}' is declared more than once",
                    field.name
                )));
            }
        }
        Ok(())
    }
}

fn action_kind_accepts_endpoint(
    kind: StorageConnectorActionKind,
    endpoint: StorageConnectorActionEndpoint,
) -> bool {
    matches!(
        (kind, endpoint),
        (
            StorageConnectorActionKind::Custom,
            StorageConnectorActionEndpoint::ExecuteDraftStoragePolicyAction
                | StorageConnectorActionEndpoint::ExecuteSavedStoragePolicyAction
        ) | (
            StorageConnectorActionKind::Authorization,
            StorageConnectorActionEndpoint::StartStorageAuthorization
        ) | (
            StorageConnectorActionKind::CredentialValidation,
            StorageConnectorActionEndpoint::ValidateStoragePolicyCredential
        ) | (
            StorageConnectorActionKind::ConnectionTest,
            StorageConnectorActionEndpoint::TestPolicyParams
                | StorageConnectorActionEndpoint::TestPolicyConnection
        )
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageConnectorActionDescriptorError(String);

impl fmt::Display for StorageConnectorActionDescriptorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for StorageConnectorActionDescriptorError {}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StorageConnectorUploadWorkflows {
    /// 后端/客户端可以用单请求写入小对象。
    pub simple_upload: bool,
    /// 单请求上传的静态语义。实际是否走 direct 仍受 policy chunk_size 限制。
    pub simple_upload_capabilities: StorageConnectorSimpleUploadCapabilities,
    /// 后端可以通过 `StreamUploadDriver` 把 reader 写入 provider。
    pub stream_upload: bool,
    /// 支持对象存储 multipart/block upload 语义。
    pub object_multipart_upload: bool,
    /// 对象存储 multipart/block upload 的具体语义。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub object_multipart_upload_capabilities:
        Option<StorageConnectorObjectMultipartUploadCapabilities>,
    /// 支持 provider-native resumable/session upload。
    pub provider_resumable_upload: bool,
    /// 支持浏览器/客户端使用 presigned URL 直传。
    pub presigned_upload: bool,
    /// 是否允许前端直接拿 provider-native session 上传。
    pub frontend_direct_provider_resumable_upload: bool,
    /// Provider-native resumable/session upload 的具体语义。
    ///
    /// 该字段只描述 provider 自己的 session/range 协议，例如 Microsoft Graph
    /// upload session。S3-compatible multipart/block upload 不应填这里。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_resumable_upload_capabilities:
        Option<StorageConnectorProviderResumableUploadCapabilities>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StorageConnectorSimpleUploadCapabilities {
    /// true 表示浏览器把对象发给 AsterDrive，由后端 relay 到 provider。
    pub server_side_relay: bool,
    /// true 表示单请求 direct/relay 上限由具体 policy chunk_size 决定。
    pub policy_limited: bool,
    /// Provider 自身单请求 API 的最大对象大小；None 表示当前 connector 不声明静态上限。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_provider_single_request_size: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StorageConnectorObjectMultipartUploadCapabilities {
    /// Provider 最小非 final part 大小。
    pub min_part_size: i64,
    /// true 表示实际 part size 由 policy chunk_size 决定，但会被 min_part_size 修正。
    pub policy_limited_part_size: bool,
    /// AsterDrive 服务端是否可以 relay 上传 part。
    pub relay_part_upload: bool,
    /// 浏览器是否可以通过 presigned URL 直传 part。
    pub presigned_part_upload: bool,
    /// 浏览器直传 part 后是否必须从响应读取 ETag。
    ///
    /// Azure block upload 通过 URL 中的 blockid 作为 completion token，因此不要求
    /// 浏览器能读 ETag；S3-compatible multipart 通常需要 ETag。
    pub presigned_part_etag_required: bool,
    /// complete 阶段是否需要显式提交 part 列表。
    pub explicit_complete_required: bool,
    /// 是否支持清理未完成的 provider multipart/block upload。
    pub abort_supported: bool,
    /// 是否支持查询 provider 已接收的 part/block 列表。
    pub list_parts_supported: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StorageConnectorProviderResumableUploadCapabilities {
    /// Provider 标识，例如 `microsoft_graph`。
    pub provider: String,
    /// 面向 UI/诊断的 session 名称，例如 `Microsoft Graph upload session`。
    pub session_label: String,
    /// Provider 接受的最小分片大小。
    pub min_fragment_size: usize,
    /// 后端默认使用的分片大小。
    pub default_fragment_size: usize,
    /// Provider 或当前实现允许的最大分片大小。
    pub max_fragment_size: usize,
    /// 分片边界对齐要求。
    pub fragment_alignment: usize,
    /// 小文件可绕过 resumable session 的大小上限。
    pub max_simple_upload_size: Option<u64>,
    /// 是否允许浏览器直接拿 provider session 上传。
    pub frontend_direct_upload: bool,
    /// Provider 是否在最后一个 range/fragment 接收后隐式完成 session。
    pub implicit_completion: bool,
    /// 当前实现是否向上层暴露 provider-native abort。
    pub abort_supported: bool,
    /// 当前实现是否向上层暴露 provider-native status/query。
    pub status_query_supported: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StorageConnectorCapabilities {
    /// 是否支持高效 range read。
    pub efficient_range: bool,
    /// 是否支持容量观测。
    pub capacity: bool,
    /// 是否支持底层对象路径列举。
    pub list: bool,
    /// 是否支持 presigned download。
    pub presigned_download: bool,
    /// 是否支持 provider/storage-native thumbnail。
    pub storage_native_thumbnail: bool,
    /// 是否支持 provider/storage-native media metadata。
    pub storage_native_media_metadata: bool,
    /// 是否需要或支持 remote node 绑定。
    pub remote_node_binding: bool,
    /// 是否暴露对象存储 upload/download strategy 选项。
    pub object_storage_transfer_strategy: bool,
    /// 底层对象路径采用 opaque UUID，还是保留原文件名作为 provider item name。
    pub object_naming: StorageConnectorObjectNamingMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum StorageConnectorObjectNamingMode {
    OpaqueUuid,
    OriginalFilename,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StorageConnectorFieldDescriptor {
    /// 提交 payload 中的字段名。
    pub name: String,
    /// 字段进入哪个配置域。
    pub scope: StorageConnectorFieldScope,
    /// 前端可用的基础控件类型。
    pub kind: StorageConnectorFieldKind,
    /// 前端本地化 label key。默认通常等于 `name`。
    pub label_key: String,
    /// 可选 placeholder，本地化策略由前端决定。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    /// 可选 help 文案 key。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub help_key: Option<String>,
    /// 字段必填校验失败时的前端文案 key。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required_message_key: Option<String>,
    /// endpoint 协议不合法时的前端文案 key。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invalid_protocol_message_key: Option<String>,
    /// endpoint 允许的 URL protocol，取值与浏览器 `URL.protocol` 一致，例如 `https:`。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_endpoint_protocols: Vec<String>,
    /// true 表示 endpoint 可以省略 URL scheme，由 connector 在后端补齐或解释。
    #[serde(default)]
    pub allow_endpoint_without_protocol: bool,
    /// true 表示该字段失焦时前端可以安全 trim。
    #[serde(default)]
    pub trim_on_blur: bool,
    /// 是否必填。复杂条件校验仍由 connector/service 做最终裁决。
    pub required: bool,
    /// 是否是敏感字段，前端应按 secret input 处理，后端不应明文回显。
    pub secret: bool,
    /// Select control contract. Present exactly when `kind` is `select`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub select: Option<StorageConnectorSelectDescriptor>,
    /// Connector schema 定义的默认值。省略表示该字段没有隐式默认值。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_value: Option<StorageConnectorFieldDefaultValue>,
    /// Connector-owned rule deciding whether an empty optional text field also
    /// resolves to `default_value`.
    #[serde(
        default,
        skip_serializing_if = "StorageConnectorFieldDefaultMode::is_missing_only"
    )]
    pub default_mode: StorageConnectorFieldDefaultMode,
    /// 可被前端用于即时反馈、且必须由后端再次执行的基础约束。
    #[serde(default)]
    pub validation: StorageConnectorFieldValidation,
}

impl StorageConnectorFieldDescriptor {
    pub fn validate(&self) -> Result<(), StorageConnectorFieldDescriptorError> {
        if self.name.trim().is_empty() {
            return Err(StorageConnectorFieldDescriptorError(
                "field name must not be empty".to_string(),
            ));
        }
        if self.label_key.trim().is_empty() {
            return Err(StorageConnectorFieldDescriptorError(format!(
                "field '{}' label_key must not be empty",
                self.name
            )));
        }

        if self.default_mode == StorageConnectorFieldDefaultMode::MissingOrEmptyText {
            if self.default_value.is_none() {
                return Err(StorageConnectorFieldDescriptorError(format!(
                    "field '{}' cannot apply an empty-text default without default_value",
                    self.name
                )));
            }
            if self.kind != StorageConnectorFieldKind::Text || self.required {
                return Err(StorageConnectorFieldDescriptorError(format!(
                    "field '{}' may use missing_or_empty_text only for optional text",
                    self.name
                )));
            }
        }

        match (self.kind, &self.select) {
            (StorageConnectorFieldKind::Select, Some(select)) => {
                let has_static_options = !select.options.is_empty();
                let has_data_source = select.data_source.is_some();
                if has_static_options == has_data_source {
                    return Err(StorageConnectorFieldDescriptorError(format!(
                        "select field '{}' must declare exactly one of options or data_source",
                        self.name
                    )));
                }
                if select.depends_on.is_some() && !has_data_source {
                    return Err(StorageConnectorFieldDescriptorError(format!(
                        "select field '{}' may declare depends_on only with data_source",
                        self.name
                    )));
                }
                match select.data_source {
                    Some(StorageConnectorSelectDataSource::RemoteNodes)
                        if select.value_kind != StorageConnectorSelectValueKind::Integer
                            || select.depends_on.is_some() =>
                    {
                        return Err(StorageConnectorFieldDescriptorError(format!(
                            "select field '{}' remote_nodes source requires integer values and no dependency",
                            self.name
                        )));
                    }
                    Some(StorageConnectorSelectDataSource::RemoteStorageTargets)
                        if select.value_kind != StorageConnectorSelectValueKind::String
                            || select.depends_on.is_none() =>
                    {
                        return Err(StorageConnectorFieldDescriptorError(format!(
                            "select field '{}' remote_storage_targets source requires string values and one dependency",
                            self.name
                        )));
                    }
                    _ => {}
                }

                let mut values = HashSet::with_capacity(select.options.len());
                for option in &select.options {
                    if option.label_key.trim().is_empty() {
                        return Err(StorageConnectorFieldDescriptorError(format!(
                            "select field '{}' option label_key must not be empty",
                            self.name
                        )));
                    }
                    let value_kind_matches = matches!(
                        (select.value_kind, &option.value),
                        (
                            StorageConnectorSelectValueKind::String,
                            StorageConnectorSelectOptionValue::String(_)
                        ) | (
                            StorageConnectorSelectValueKind::Integer,
                            StorageConnectorSelectOptionValue::Integer(_)
                        )
                    );
                    if !value_kind_matches {
                        return Err(StorageConnectorFieldDescriptorError(format!(
                            "select field '{}' option value does not match value_kind",
                            self.name
                        )));
                    }
                    if !values.insert(option.value.clone()) {
                        return Err(StorageConnectorFieldDescriptorError(format!(
                            "select field '{}' declares duplicate option values",
                            self.name
                        )));
                    }
                }
                if let Some(default) = &self.default_value {
                    let default_option = match default {
                        StorageConnectorFieldDefaultValue::String(value) => {
                            StorageConnectorSelectOptionValue::String(value.clone())
                        }
                        StorageConnectorFieldDefaultValue::Integer(value) => {
                            StorageConnectorSelectOptionValue::Integer(*value)
                        }
                        StorageConnectorFieldDefaultValue::Boolean(_) => {
                            return Err(StorageConnectorFieldDescriptorError(format!(
                                "select field '{}' default value does not match value_kind",
                                self.name
                            )));
                        }
                    };
                    let default_kind_matches = matches!(
                        (select.value_kind, &default_option),
                        (
                            StorageConnectorSelectValueKind::String,
                            StorageConnectorSelectOptionValue::String(_)
                        ) | (
                            StorageConnectorSelectValueKind::Integer,
                            StorageConnectorSelectOptionValue::Integer(_)
                        )
                    );
                    if !default_kind_matches
                        || (!select.options.is_empty()
                            && !select
                                .options
                                .iter()
                                .any(|option| option.value == default_option))
                    {
                        return Err(StorageConnectorFieldDescriptorError(format!(
                            "select field '{}' default value is not a declared option",
                            self.name
                        )));
                    }
                }
            }
            (StorageConnectorFieldKind::Select, None) => {
                return Err(StorageConnectorFieldDescriptorError(format!(
                    "select field '{}' is missing its select contract",
                    self.name
                )));
            }
            (_, Some(_)) => {
                return Err(StorageConnectorFieldDescriptorError(format!(
                    "non-select field '{}' must not declare a select contract",
                    self.name
                )));
            }
            (_, None) => {}
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageConnectorFieldDescriptorError(String);

impl fmt::Display for StorageConnectorFieldDescriptorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for StorageConnectorFieldDescriptorError {}

/// Typed connector configuration that is also the source of truth for the
/// admin form field contract.
///
/// Implementations are normally generated by [`crate::storage_connector_config`]
/// so persisted serde fields and descriptor fields cannot drift independently.
pub trait StorageConnectorConfigSchema {
    fn connector_config_fields() -> Vec<StorageConnectorFieldDescriptor>;

    fn credential_mode() -> StorageConnectorCredentialMode;

    fn credential_fields() -> Vec<StorageConnectorFieldDescriptor>;

    fn descriptor_fields() -> Vec<StorageConnectorFieldDescriptor> {
        let mut fields = Self::connector_config_fields();
        fields.extend(Self::credential_fields());
        fields
    }
}

/// Typed action input that is also the source of truth for its UI field schema.
///
/// Implementations are normally generated by
/// [`crate::storage_connector_action_schema`]. Connector code deserializes the
/// validated action values into this type instead of manually assembling JSON.
pub trait StorageConnectorActionSchema {
    fn action_fields() -> Vec<StorageConnectorFieldDescriptor>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageConnectorOptionsValidationError {
    FormatVersionMismatch {
        expected: u32,
        actual: u32,
    },
    NamespaceMismatch {
        expected: String,
        actual: String,
    },
    SchemaVersionMismatch {
        expected: u32,
        actual: u32,
    },
    InvalidFieldDescriptor {
        field: String,
        reason: String,
    },
    DuplicateField(String),
    UnknownField(String),
    MissingRequiredField(String),
    SecretField(String),
    TypeMismatch {
        field: String,
        expected: StorageConnectorFieldKind,
    },
    InvalidOption {
        field: String,
        value: String,
    },
    IntegerBelowMinimum {
        field: String,
        minimum: i64,
    },
    IntegerAboveMaximum {
        field: String,
        maximum: i64,
    },
    StringTooLong {
        field: String,
        maximum: u32,
    },
}

impl fmt::Display for StorageConnectorOptionsValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FormatVersionMismatch { expected, actual } => write!(
                formatter,
                "connector config format version mismatch: expected {expected}, got {actual}"
            ),
            Self::NamespaceMismatch { expected, actual } => write!(
                formatter,
                "provider options namespace mismatch: expected '{expected}', got '{actual}'"
            ),
            Self::SchemaVersionMismatch { expected, actual } => write!(
                formatter,
                "provider options schema version mismatch: expected {expected}, got {actual}"
            ),
            Self::InvalidFieldDescriptor { field, reason } => write!(
                formatter,
                "provider option field '{field}' has an invalid descriptor: {reason}"
            ),
            Self::DuplicateField(field) => {
                write!(
                    formatter,
                    "provider option field '{field}' is declared more than once"
                )
            }
            Self::UnknownField(field) => {
                write!(formatter, "unknown provider option field '{field}'")
            }
            Self::MissingRequiredField(field) => {
                write!(
                    formatter,
                    "required provider option field '{field}' is missing"
                )
            }
            Self::SecretField(field) => write!(
                formatter,
                "provider option field '{field}' is secret and must use credential storage"
            ),
            Self::TypeMismatch { field, expected } => write!(
                formatter,
                "provider option field '{field}' must be a {}",
                expected.as_str()
            ),
            Self::InvalidOption { field, value } => write!(
                formatter,
                "provider option field '{field}' has unsupported value '{value}'"
            ),
            Self::IntegerBelowMinimum { field, minimum } => write!(
                formatter,
                "provider option field '{field}' must be at least {minimum}"
            ),
            Self::IntegerAboveMaximum { field, maximum } => write!(
                formatter,
                "provider option field '{field}' must be at most {maximum}"
            ),
            Self::StringTooLong { field, maximum } => write!(
                formatter,
                "provider option field '{field}' exceeds maximum length {maximum}"
            ),
        }
    }
}

impl std::error::Error for StorageConnectorOptionsValidationError {}

impl StorageConnectorFieldKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Text => "string",
            Self::Secret => "secret string",
            Self::Select => "string",
            Self::Boolean => "boolean",
            Self::Number => "integer",
        }
    }
}

/// 按 connector descriptor 归一化并校验一个 provider option namespace。
///
/// 调用方必须传入 payload 顶层 namespace，避免 service 先丢掉 namespace 后让错误
/// connector 接收数据。返回值只包含 descriptor 声明的字段及补齐后的默认值。
pub fn normalize_storage_connector_config(
    descriptor: &StorageConnectorDescriptor,
    input: &crate::ConnectorConfigEnvelope,
) -> Result<crate::ConnectorConfigEnvelope, StorageConnectorOptionsValidationError> {
    if input.format_version != crate::CONNECTOR_CONFIG_FORMAT_VERSION {
        return Err(
            StorageConnectorOptionsValidationError::FormatVersionMismatch {
                expected: crate::CONNECTOR_CONFIG_FORMAT_VERSION,
                actual: input.format_version,
            },
        );
    }
    if descriptor.connector_id != input.connector_id {
        return Err(StorageConnectorOptionsValidationError::NamespaceMismatch {
            expected: descriptor.connector_id.as_str().to_string(),
            actual: input.connector_id.as_str().to_string(),
        });
    }
    if descriptor.config_schema_version != input.schema_version {
        return Err(
            StorageConnectorOptionsValidationError::SchemaVersionMismatch {
                expected: descriptor.config_schema_version,
                actual: input.schema_version,
            },
        );
    }

    let normalized = normalize_storage_connector_field_values(
        descriptor
            .fields
            .iter()
            .filter(|field| field.scope == StorageConnectorFieldScope::ConnectorConfig),
        &input.values,
        true,
    )?;

    Ok(crate::ConnectorConfigEnvelope {
        format_version: crate::CONNECTOR_CONFIG_FORMAT_VERSION,
        connector_id: input.connector_id.clone(),
        schema_version: descriptor.config_schema_version,
        values: normalized,
    })
}

/// Normalize one action invocation against the connector-owned action schema.
///
/// Unlike persisted connector config, action input may contain secret fields:
/// they are delivered only to the connector invocation and are never written
/// into the policy envelope.
pub fn normalize_storage_connector_action_input(
    descriptor: &StorageConnectorActionDescriptor,
    input: &BTreeMap<String, serde_json::Value>,
) -> Result<BTreeMap<String, serde_json::Value>, StorageConnectorOptionsValidationError> {
    normalize_storage_connector_field_values(
        descriptor
            .fields
            .iter()
            .filter(|field| field.scope == StorageConnectorFieldScope::ActionInput),
        input,
        false,
    )
}

/// Resolve one custom action endpoint and normalize its connector-owned input.
///
/// This shared boundary knows only connector metadata. Provider-specific
/// action IDs and typed input structs remain inside the connector.
pub fn normalize_storage_connector_custom_action_invocation(
    connector: &StorageConnectorDescriptor,
    action_id: &StorageConnectorActionId,
    endpoint: StorageConnectorActionEndpoint,
    input: &BTreeMap<String, serde_json::Value>,
) -> Result<BTreeMap<String, serde_json::Value>, StorageConnectorActionInvocationError> {
    let action = connector
        .actions
        .iter()
        .find(|action| {
            action.kind == StorageConnectorActionKind::Custom
                && &action.action_id == action_id
                && action.endpoints.contains(&endpoint)
        })
        .ok_or_else(|| StorageConnectorActionInvocationError::Unsupported {
            connector_id: connector.connector_id.clone(),
            action_id: action_id.clone(),
            endpoint,
        })?;
    normalize_storage_connector_action_input(action, input)
        .map_err(StorageConnectorActionInvocationError::InvalidInput)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageConnectorActionInvocationError {
    Unsupported {
        connector_id: ConnectorId,
        action_id: StorageConnectorActionId,
        endpoint: StorageConnectorActionEndpoint,
    },
    InvalidInput(StorageConnectorOptionsValidationError),
}

impl fmt::Display for StorageConnectorActionInvocationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported {
                connector_id,
                action_id,
                endpoint,
            } => write!(
                formatter,
                "storage connector action '{}' is not available through endpoint {:?} for connector '{}'",
                action_id.as_str(),
                endpoint,
                connector_id.as_str()
            ),
            Self::InvalidInput(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for StorageConnectorActionInvocationError {}

fn normalize_storage_connector_field_values<'a>(
    declared_fields: impl IntoIterator<Item = &'a StorageConnectorFieldDescriptor>,
    input: &BTreeMap<String, serde_json::Value>,
    reject_secrets: bool,
) -> Result<BTreeMap<String, serde_json::Value>, StorageConnectorOptionsValidationError> {
    let mut fields = HashMap::new();
    for field in declared_fields {
        if fields.insert(field.name.as_str(), field).is_some() {
            return Err(StorageConnectorOptionsValidationError::DuplicateField(
                field.name.clone(),
            ));
        }
    }

    for name in input.keys() {
        if !fields.contains_key(name.as_str()) {
            return Err(StorageConnectorOptionsValidationError::UnknownField(
                name.clone(),
            ));
        }
    }

    let mut normalized = BTreeMap::new();
    for field in fields.values() {
        if reject_secrets && (field.secret || field.kind == StorageConnectorFieldKind::Secret) {
            if input.contains_key(&field.name) {
                return Err(StorageConnectorOptionsValidationError::SecretField(
                    field.name.clone(),
                ));
            }
            continue;
        }

        let supplied = input.get(&field.name).cloned();
        let value = supplied
            .clone()
            .or_else(|| field.default_value.as_ref().map(default_value_to_json));
        let Some(mut value) = value else {
            if field.required {
                return Err(
                    StorageConnectorOptionsValidationError::MissingRequiredField(
                        field.name.clone(),
                    ),
                );
            }
            continue;
        };

        normalize_and_validate_connector_field_value(field, &mut value)?;
        if value.as_str().is_some_and(str::is_empty) && !field.required {
            if supplied.is_some()
                && field.default_mode == StorageConnectorFieldDefaultMode::MissingOrEmptyText
            {
                let default_value = field.default_value.as_ref().ok_or_else(|| {
                    StorageConnectorOptionsValidationError::InvalidFieldDescriptor {
                        field: field.name.clone(),
                        reason: "missing_or_empty_text requires default_value".to_string(),
                    }
                })?;
                value = default_value_to_json(default_value);
                normalize_and_validate_connector_field_value(field, &mut value)?;
            } else if supplied.is_some() {
                continue;
            }
        }
        normalized.insert(field.name.clone(), value);
    }
    Ok(normalized)
}

fn default_value_to_json(value: &StorageConnectorFieldDefaultValue) -> serde_json::Value {
    match value {
        StorageConnectorFieldDefaultValue::Boolean(value) => serde_json::Value::Bool(*value),
        StorageConnectorFieldDefaultValue::Integer(value) => {
            serde_json::Value::Number((*value).into())
        }
        StorageConnectorFieldDefaultValue::String(value) => {
            serde_json::Value::String(value.clone())
        }
    }
}

fn normalize_and_validate_connector_field_value(
    field: &StorageConnectorFieldDescriptor,
    value: &mut serde_json::Value,
) -> Result<(), StorageConnectorOptionsValidationError> {
    match field.kind {
        StorageConnectorFieldKind::Text | StorageConnectorFieldKind::Secret => {
            let Some(text) = value.as_str() else {
                return Err(StorageConnectorOptionsValidationError::TypeMismatch {
                    field: field.name.clone(),
                    expected: field.kind,
                });
            };
            let normalized = if field.trim_on_blur {
                text.trim()
            } else {
                text
            };
            if field.required && normalized.is_empty() {
                return Err(
                    StorageConnectorOptionsValidationError::MissingRequiredField(
                        field.name.clone(),
                    ),
                );
            }
            if let Some(maximum) = field.validation.max_length
                && normalized.chars().count() > maximum as usize
            {
                return Err(StorageConnectorOptionsValidationError::StringTooLong {
                    field: field.name.clone(),
                    maximum,
                });
            }
            *value = serde_json::Value::String(normalized.to_string());
        }
        StorageConnectorFieldKind::Select => {
            let Some(select) = field.select.as_ref() else {
                return Err(StorageConnectorOptionsValidationError::TypeMismatch {
                    field: field.name.clone(),
                    expected: field.kind,
                });
            };
            let option_value = match select.value_kind {
                StorageConnectorSelectValueKind::String => {
                    let Some(text) = value.as_str() else {
                        return Err(StorageConnectorOptionsValidationError::TypeMismatch {
                            field: field.name.clone(),
                            expected: field.kind,
                        });
                    };
                    StorageConnectorSelectOptionValue::String(text.to_string())
                }
                StorageConnectorSelectValueKind::Integer => {
                    let Some(integer) = value.as_i64() else {
                        return Err(StorageConnectorOptionsValidationError::TypeMismatch {
                            field: field.name.clone(),
                            expected: field.kind,
                        });
                    };
                    StorageConnectorSelectOptionValue::Integer(integer)
                }
            };
            if !select.options.is_empty()
                && !select
                    .options
                    .iter()
                    .any(|option| option.value == option_value)
            {
                return Err(StorageConnectorOptionsValidationError::InvalidOption {
                    field: field.name.clone(),
                    value: match option_value {
                        StorageConnectorSelectOptionValue::String(value) => value,
                        StorageConnectorSelectOptionValue::Integer(value) => value.to_string(),
                    },
                });
            }
        }
        StorageConnectorFieldKind::Boolean => {
            if !value.is_boolean() {
                return Err(StorageConnectorOptionsValidationError::TypeMismatch {
                    field: field.name.clone(),
                    expected: field.kind,
                });
            }
        }
        StorageConnectorFieldKind::Number => {
            let Some(integer) = value.as_i64() else {
                return Err(StorageConnectorOptionsValidationError::TypeMismatch {
                    field: field.name.clone(),
                    expected: field.kind,
                });
            };
            if let Some(minimum) = field.validation.min_integer
                && integer < minimum
            {
                return Err(
                    StorageConnectorOptionsValidationError::IntegerBelowMinimum {
                        field: field.name.clone(),
                        minimum,
                    },
                );
            }
            if let Some(maximum) = field.validation.max_integer
                && integer > maximum
            {
                return Err(
                    StorageConnectorOptionsValidationError::IntegerAboveMaximum {
                        field: field.name.clone(),
                        maximum,
                    },
                );
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StorageConnectorDescriptor {
    /// 持久化到 policy 的稳定 connector/plugin id。
    pub connector_id: ConnectorId,
    /// 人类可读名称。
    pub label: String,
    /// 人类可读说明。
    pub description: String,
    /// 管理端展示元数据。
    ///
    /// 这类 label/icon/helper 虽然最终由前端渲染，但语义上属于 connector：
    /// 新 connector 不应该要求前端再维护一份 driver 展示矩阵。
    pub ui: StorageConnectorUiDescriptor,
    /// connector 的主要凭据模式。
    pub credential_mode: StorageConnectorCredentialMode,
    /// policy 数据相对于多个 Primary 的可见范围。
    pub deployment_scope: StorageConnectorDeploymentScope,
    /// 是否能在首次系统初始化中直接创建一个可用的默认 policy。
    ///
    /// 需要先保存 policy、再跳转授权或完成其他后置配置的 connector 应设为 false。
    pub supports_initial_setup: bool,
    /// 是否需要额外授权才能成为可用 policy。
    pub requires_authorization: bool,
    /// 授权 provider，例如 `microsoft_graph`。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authorization_provider: Option<String>,
    /// 存储对象能力。
    pub capabilities: StorageConnectorCapabilities,
    /// 上传工作流能力。
    pub upload_workflows: StorageConnectorUploadWorkflows,
    /// 管理端配置字段声明。
    pub fields: Vec<StorageConnectorFieldDescriptor>,
    /// 当前 connector 能解析并输出的配置 schema 版本。
    pub config_schema_version: u32,
    /// 管理端/服务端可执行动作声明。
    pub actions: Vec<StorageConnectorActionDescriptor>,
    /// 用于开发追踪的相关 issue 编号，不参与业务逻辑。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related_issues: Vec<u16>,
}

impl StorageConnectorDescriptor {
    /// Validate the complete plugin-owned catalog contract before registration.
    pub fn validate(&self) -> Result<(), StorageConnectorDescriptorError> {
        self.connector_id
            .validate()
            .map_err(|error| StorageConnectorDescriptorError(error.to_string()))?;

        let mut field_names = HashSet::with_capacity(self.fields.len());
        for field in &self.fields {
            field.validate().map_err(|error| {
                StorageConnectorDescriptorError(format!(
                    "field '{}' is invalid: {error}",
                    field.name
                ))
            })?;
            if !field_names.insert((field.scope, field.name.as_str())) {
                return Err(StorageConnectorDescriptorError(format!(
                    "field '{}' is declared more than once in scope {:?}",
                    field.name, field.scope
                )));
            }
        }
        for field in &self.fields {
            if let Some(dependency) = field
                .select
                .as_ref()
                .and_then(|select| select.depends_on.as_deref())
            {
                if dependency == field.name {
                    return Err(StorageConnectorDescriptorError(format!(
                        "field '{}' must not depend on itself",
                        field.name
                    )));
                }
                if !field_names.contains(&(field.scope, dependency)) {
                    return Err(StorageConnectorDescriptorError(format!(
                        "field '{}' depends on undeclared field '{}' in scope {:?}",
                        field.name, dependency, field.scope
                    )));
                }
            }
        }
        for field in &self.fields {
            let mut visited = HashSet::new();
            let mut current = field;
            while let Some(dependency) = current
                .select
                .as_ref()
                .and_then(|select| select.depends_on.as_deref())
            {
                if !visited.insert((current.scope, current.name.as_str())) {
                    return Err(StorageConnectorDescriptorError(format!(
                        "field '{}' participates in a dependency cycle",
                        field.name
                    )));
                }
                let Some(next) = self.fields.iter().find(|candidate| {
                    candidate.scope == current.scope && candidate.name == dependency
                }) else {
                    break;
                };
                current = next;
            }
        }

        let mut action_ids = HashSet::with_capacity(self.actions.len());
        for action in &self.actions {
            action.validate().map_err(|error| {
                StorageConnectorDescriptorError(format!(
                    "action '{}' is invalid: {error}",
                    action.action_id.as_str()
                ))
            })?;
            if !action_ids.insert(action.action_id.clone()) {
                return Err(StorageConnectorDescriptorError(format!(
                    "action '{}' is declared more than once",
                    action.action_id.as_str()
                )));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageConnectorDescriptorError(String);

impl fmt::Display for StorageConnectorDescriptorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for StorageConnectorDescriptorError {}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StorageConnectorUiDescriptor {
    /// 前端 i18n label key。
    pub label_key: String,
    /// 前端 i18n description key。
    pub description_key: String,
    /// driver 选择卡片/上下文条图标资源。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_src: Option<String>,
    /// icon 库名称兜底。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_name: Option<String>,
    /// Connector-owned badge accent color.
    ///
    /// Keeping the color as structured RGB data lets external connectors pick
    /// their own presentation without extending a core-owned color enum or
    /// sending executable CSS through the descriptor API.
    pub badge_rgb: StorageConnectorBadgeRgb,
    /// 创建向导右侧 helper 文案 key。
    pub helper_key: String,
    /// 创建向导配置步骤标题 key。
    pub config_step_title_key: String,
    /// 创建向导配置步骤说明 key。
    pub config_step_description_key: String,
    /// 编辑页上下文说明 key。
    pub edit_context_key: String,
    /// base_path 为空时展示的 fallback 文案。
    pub base_path_empty_display: String,
    /// base_path input placeholder。
    pub base_path_placeholder: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StorageConnectorBadgeRgb {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

impl StorageConnectorBadgeRgb {
    pub const fn new(red: u8, green: u8, blue: u8) -> Self {
        Self { red, green, blue }
    }
}

pub struct ObjectStorageConnectorDescriptorInput {
    pub connector_id: ConnectorId,
    pub label: &'static str,
    pub description: &'static str,
    pub ui: StorageConnectorUiDescriptorInput,
    pub deployment_scope: StorageConnectorDeploymentScope,
    pub supports_initial_setup: bool,
    pub credential_mode: StorageConnectorCredentialMode,
    pub fields: Vec<StorageConnectorFieldDescriptor>,
    pub presigned_part_etag_required: bool,
    pub storage_native_processing: bool,
    pub config_schema_version: u32,
    pub related_issues: Vec<u16>,
}

pub fn object_storage_connector_descriptor(
    input: ObjectStorageConnectorDescriptorInput,
) -> StorageConnectorDescriptor {
    StorageConnectorDescriptor {
        connector_id: input.connector_id,
        label: input.label.to_string(),
        description: input.description.to_string(),
        ui: storage_connector_ui_descriptor(input.ui),
        credential_mode: input.credential_mode,
        deployment_scope: input.deployment_scope,
        supports_initial_setup: input.supports_initial_setup,
        requires_authorization: false,
        authorization_provider: None,
        capabilities: StorageConnectorCapabilities {
            efficient_range: true,
            capacity: true,
            list: true,
            presigned_download: true,
            storage_native_thumbnail: input.storage_native_processing,
            storage_native_media_metadata: input.storage_native_processing,
            remote_node_binding: false,
            object_storage_transfer_strategy: true,
            object_naming: StorageConnectorObjectNamingMode::OpaqueUuid,
        },
        upload_workflows: StorageConnectorUploadWorkflows {
            simple_upload: true,
            simple_upload_capabilities: server_relay_simple_upload_capabilities(None),
            stream_upload: true,
            object_multipart_upload: true,
            object_multipart_upload_capabilities: Some(object_multipart_upload_capabilities(
                ObjectMultipartUploadCapabilitiesInput {
                    presigned_part_etag_required: input.presigned_part_etag_required,
                },
            )),
            provider_resumable_upload: false,
            presigned_upload: true,
            frontend_direct_provider_resumable_upload: false,
            provider_resumable_upload_capabilities: None,
        },
        fields: input.fields,
        config_schema_version: input.config_schema_version,
        actions: vec![
            draft_connection_test_action_descriptor(),
            saved_connection_test_action_descriptor(false),
        ],
        related_issues: input.related_issues,
    }
}

pub fn server_relay_simple_upload_capabilities(
    max_provider_single_request_size: Option<u64>,
) -> StorageConnectorSimpleUploadCapabilities {
    StorageConnectorSimpleUploadCapabilities {
        server_side_relay: true,
        policy_limited: true,
        max_provider_single_request_size,
    }
}

pub struct ObjectMultipartUploadCapabilitiesInput {
    pub presigned_part_etag_required: bool,
}

pub fn object_multipart_upload_capabilities(
    input: ObjectMultipartUploadCapabilitiesInput,
) -> StorageConnectorObjectMultipartUploadCapabilities {
    StorageConnectorObjectMultipartUploadCapabilities {
        min_part_size: OBJECT_MULTIPART_MIN_PART_SIZE,
        policy_limited_part_size: true,
        relay_part_upload: true,
        presigned_part_upload: true,
        presigned_part_etag_required: input.presigned_part_etag_required,
        explicit_complete_required: true,
        abort_supported: true,
        list_parts_supported: true,
    }
}

pub struct StorageConnectorCustomActionDescriptorInput {
    pub action_id: StorageConnectorActionId,
    pub label_key: &'static str,
    pub description_key: &'static str,
    pub fields: Vec<StorageConnectorFieldDescriptor>,
    pub supports_draft: bool,
    pub supports_saved: bool,
    pub requires_authorization: bool,
    pub mutates_remote_state: bool,
    pub requires_confirmation: bool,
}

pub fn custom_action_descriptor(
    input: StorageConnectorCustomActionDescriptorInput,
) -> StorageConnectorActionDescriptor {
    let mut endpoints = Vec::new();
    if input.supports_draft {
        endpoints.push(StorageConnectorActionEndpoint::ExecuteDraftStoragePolicyAction);
    }
    if input.supports_saved {
        endpoints.push(StorageConnectorActionEndpoint::ExecuteSavedStoragePolicyAction);
    }
    StorageConnectorActionDescriptor {
        action_id: input.action_id,
        label_key: input.label_key.to_string(),
        description_key: input.description_key.to_string(),
        kind: StorageConnectorActionKind::Custom,
        endpoints,
        fields: input.fields,
        requires_saved_policy: !input.supports_draft,
        requires_authorization: input.requires_authorization,
        mutates_remote_state: input.mutates_remote_state,
        requires_confirmation: input.requires_confirmation,
    }
}

pub fn start_authorization_action_descriptor() -> StorageConnectorActionDescriptor {
    StorageConnectorActionDescriptor {
        action_id: StorageConnectorActionId::declared("start_authorization"),
        label_key: "policy_connector_start_authorization".to_string(),
        description_key: "policy_connector_start_authorization_desc".to_string(),
        kind: StorageConnectorActionKind::Authorization,
        endpoints: vec![StorageConnectorActionEndpoint::StartStorageAuthorization],
        fields: Vec::new(),
        requires_saved_policy: true,
        requires_authorization: false,
        mutates_remote_state: false,
        requires_confirmation: false,
    }
}

pub fn validate_credential_action_descriptor() -> StorageConnectorActionDescriptor {
    StorageConnectorActionDescriptor {
        action_id: StorageConnectorActionId::declared("validate_credential"),
        label_key: "policy_connector_validate_credential".to_string(),
        description_key: "policy_connector_validate_credential_desc".to_string(),
        kind: StorageConnectorActionKind::CredentialValidation,
        endpoints: vec![StorageConnectorActionEndpoint::ValidateStoragePolicyCredential],
        fields: Vec::new(),
        requires_saved_policy: true,
        requires_authorization: true,
        mutates_remote_state: false,
        requires_confirmation: false,
    }
}

pub fn draft_connection_test_action_descriptor() -> StorageConnectorActionDescriptor {
    StorageConnectorActionDescriptor {
        action_id: StorageConnectorActionId::declared("test_draft_connection"),
        label_key: "test_connection".to_string(),
        description_key: "policy_wizard_step_connection_desc".to_string(),
        kind: StorageConnectorActionKind::ConnectionTest,
        endpoints: vec![StorageConnectorActionEndpoint::TestPolicyParams],
        fields: Vec::new(),
        requires_saved_policy: false,
        requires_authorization: false,
        mutates_remote_state: false,
        requires_confirmation: false,
    }
}

pub fn saved_connection_test_action_descriptor(
    requires_authorization: bool,
) -> StorageConnectorActionDescriptor {
    StorageConnectorActionDescriptor {
        action_id: StorageConnectorActionId::declared("test_saved_connection"),
        label_key: "test_connection".to_string(),
        description_key: "policy_wizard_step_connection_desc".to_string(),
        kind: StorageConnectorActionKind::ConnectionTest,
        endpoints: vec![StorageConnectorActionEndpoint::TestPolicyConnection],
        fields: Vec::new(),
        requires_saved_policy: true,
        requires_authorization,
        mutates_remote_state: false,
        requires_confirmation: false,
    }
}

pub fn storage_connector_field(
    name: &str,
    scope: StorageConnectorFieldScope,
    kind: StorageConnectorFieldKind,
    required: bool,
    secret: bool,
) -> StorageConnectorFieldDescriptor {
    storage_connector_field_with_display(StorageConnectorFieldDisplayInput {
        name,
        scope,
        kind,
        required,
        secret,
        label_key: name,
        placeholder: None,
        help_key: None,
        required_message_key: None,
        invalid_protocol_message_key: None,
        allowed_endpoint_protocols: Vec::new(),
        allow_endpoint_without_protocol: false,
        trim_on_blur: false,
    })
}

pub struct StorageConnectorFieldDisplayInput<'a> {
    pub name: &'a str,
    pub scope: StorageConnectorFieldScope,
    pub kind: StorageConnectorFieldKind,
    pub required: bool,
    pub secret: bool,
    pub label_key: &'a str,
    pub placeholder: Option<&'a str>,
    pub help_key: Option<&'a str>,
    pub required_message_key: Option<&'a str>,
    pub invalid_protocol_message_key: Option<&'a str>,
    pub allowed_endpoint_protocols: Vec<&'a str>,
    pub allow_endpoint_without_protocol: bool,
    pub trim_on_blur: bool,
}

pub fn storage_connector_field_with_display(
    input: StorageConnectorFieldDisplayInput<'_>,
) -> StorageConnectorFieldDescriptor {
    let semantics = StorageDescriptorFieldSemantics::from_descriptor_bits(
        input.kind.into(),
        input.required,
        input.secret,
    );
    StorageConnectorFieldDescriptor {
        name: input.name.to_string(),
        scope: input.scope,
        kind: semantics.kind.into(),
        label_key: input.label_key.to_string(),
        placeholder: input.placeholder.map(ToOwned::to_owned),
        help_key: input.help_key.map(ToOwned::to_owned),
        required_message_key: input.required_message_key.map(ToOwned::to_owned),
        invalid_protocol_message_key: input.invalid_protocol_message_key.map(ToOwned::to_owned),
        allowed_endpoint_protocols: input
            .allowed_endpoint_protocols
            .into_iter()
            .map(ToOwned::to_owned)
            .collect(),
        allow_endpoint_without_protocol: input.allow_endpoint_without_protocol,
        trim_on_blur: input.trim_on_blur,
        required: semantics.required,
        secret: semantics.secret,
        select: None,
        default_value: None,
        default_mode: StorageConnectorFieldDefaultMode::MissingOnly,
        validation: StorageConnectorFieldValidation::default(),
    }
}

pub fn storage_connector_field_with_options(
    name: &str,
    scope: StorageConnectorFieldScope,
    kind: StorageConnectorFieldKind,
    required: bool,
    secret: bool,
    options: Vec<&str>,
) -> StorageConnectorFieldDescriptor {
    StorageConnectorFieldDescriptor {
        select: Some(StorageConnectorSelectDescriptor {
            value_kind: StorageConnectorSelectValueKind::String,
            options: options
                .into_iter()
                .map(|value| StorageConnectorSelectOption {
                    value: StorageConnectorSelectOptionValue::String(value.to_string()),
                    label_key: value.to_string(),
                    description_key: None,
                })
                .collect(),
            data_source: None,
            depends_on: None,
        }),
        ..storage_connector_field(name, scope, kind, required, secret)
    }
}

pub struct StorageConnectorSelectOptionInput<'a> {
    pub value: &'a str,
    pub label_key: &'a str,
    pub description_key: Option<&'a str>,
}

pub fn storage_connector_select_field(
    name: &str,
    scope: StorageConnectorFieldScope,
    required: bool,
    options: Vec<StorageConnectorSelectOptionInput<'_>>,
) -> StorageConnectorFieldDescriptor {
    StorageConnectorFieldDescriptor {
        select: Some(StorageConnectorSelectDescriptor {
            value_kind: StorageConnectorSelectValueKind::String,
            options: options
                .into_iter()
                .map(|option| StorageConnectorSelectOption {
                    value: StorageConnectorSelectOptionValue::String(option.value.to_string()),
                    label_key: option.label_key.to_string(),
                    description_key: option.description_key.map(ToOwned::to_owned),
                })
                .collect(),
            data_source: None,
            depends_on: None,
        }),
        ..storage_connector_field(
            name,
            scope,
            StorageConnectorFieldKind::Select,
            required,
            false,
        )
    }
}

pub fn storage_connector_dynamic_select_field(
    name: &str,
    scope: StorageConnectorFieldScope,
    required: bool,
    value_kind: StorageConnectorSelectValueKind,
    data_source: StorageConnectorSelectDataSource,
    depends_on: Option<&str>,
) -> StorageConnectorFieldDescriptor {
    StorageConnectorFieldDescriptor {
        select: Some(StorageConnectorSelectDescriptor {
            value_kind,
            options: Vec::new(),
            data_source: Some(data_source),
            depends_on: depends_on.map(ToOwned::to_owned),
        }),
        ..storage_connector_field(
            name,
            scope,
            StorageConnectorFieldKind::Select,
            required,
            false,
        )
    }
}

pub struct StorageConnectorUiDescriptorInput {
    pub label_key: &'static str,
    pub description_key: &'static str,
    pub icon_src: Option<&'static str>,
    pub icon_name: Option<&'static str>,
    pub badge_rgb: StorageConnectorBadgeRgb,
    pub helper_key: &'static str,
    pub config_step_title_key: &'static str,
    pub config_step_description_key: &'static str,
    pub edit_context_key: &'static str,
    pub base_path_empty_display: &'static str,
    pub base_path_placeholder: &'static str,
}

pub fn storage_connector_ui_descriptor(
    input: StorageConnectorUiDescriptorInput,
) -> StorageConnectorUiDescriptor {
    StorageConnectorUiDescriptor {
        label_key: input.label_key.to_string(),
        description_key: input.description_key.to_string(),
        icon_src: input.icon_src.map(ToOwned::to_owned),
        icon_name: input.icon_name.map(ToOwned::to_owned),
        badge_rgb: input.badge_rgb,
        helper_key: input.helper_key.to_string(),
        config_step_title_key: input.config_step_title_key.to_string(),
        config_step_description_key: input.config_step_description_key.to_string(),
        edit_context_key: input.edit_context_key.to_string(),
        base_path_empty_display: input.base_path_empty_display.to_string(),
        base_path_placeholder: input.base_path_placeholder.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{
        ObjectStorageConnectorDescriptorInput, StorageConnectorActionEndpoint,
        StorageConnectorActionId, StorageConnectorActionInvocationError,
        StorageConnectorActionKind, StorageConnectorBadgeRgb, StorageConnectorCredentialMode,
        StorageConnectorCustomActionDescriptorInput, StorageConnectorDeploymentScope,
        StorageConnectorFieldDefaultMode, StorageConnectorFieldDefaultValue,
        StorageConnectorFieldDisplayInput, StorageConnectorFieldKind, StorageConnectorFieldScope,
        StorageConnectorOptionsValidationError, StorageConnectorSelectDataSource,
        StorageConnectorSelectOption, StorageConnectorSelectOptionInput,
        StorageConnectorSelectOptionValue, StorageConnectorSelectValueKind,
        StorageConnectorUiDescriptorInput, custom_action_descriptor,
        normalize_storage_connector_action_input, normalize_storage_connector_config,
        normalize_storage_connector_custom_action_invocation,
        normalize_storage_connector_field_values, object_storage_connector_descriptor,
        storage_connector_dynamic_select_field, storage_connector_field,
        storage_connector_field_with_display, storage_connector_field_with_options,
        storage_connector_select_field,
    };
    use crate::{
        CONNECTOR_CONFIG_FORMAT_VERSION, ConnectorConfigEnvelope, ConnectorId,
        StorageConnectorActionSchema,
    };

    crate::storage_connector_action_schema! {
        struct TestPluginActionInput {
            path: String => storage_connector_field_with_display(StorageConnectorFieldDisplayInput {
                name: "path",
                scope: StorageConnectorFieldScope::ActionInput,
                kind: StorageConnectorFieldKind::Text,
                required: true,
                secret: false,
                label_key: "path",
                placeholder: None,
                help_key: None,
                required_message_key: None,
                invalid_protocol_message_key: None,
                allowed_endpoint_protocols: Vec::new(),
                allow_endpoint_without_protocol: false,
                trim_on_blur: true,
            }),
            mode: String => storage_connector_field_with_options(
                "mode",
                StorageConnectorFieldScope::ActionInput,
                StorageConnectorFieldKind::Select,
                true,
                false,
                vec!["check", "apply"],
            ),
            token: String => storage_connector_field(
                "token",
                StorageConnectorFieldScope::ActionInput,
                StorageConnectorFieldKind::Secret,
                true,
                true,
            ),
            force: bool => {
                let mut field = storage_connector_field(
                    "force",
                    StorageConnectorFieldScope::ActionInput,
                    StorageConnectorFieldKind::Boolean,
                    false,
                    false,
                );
                field.default_value = Some(StorageConnectorFieldDefaultValue::Boolean(false));
                field
            },
        }
    }

    fn plugin_action_descriptor() -> super::StorageConnectorActionDescriptor {
        custom_action_descriptor(StorageConnectorCustomActionDescriptorInput {
            action_id: StorageConnectorActionId::declared("plugin.validate_path"),
            label_key: "plugin_validate_path",
            description_key: "plugin_validate_path_desc",
            fields: TestPluginActionInput::action_fields(),
            supports_draft: true,
            supports_saved: false,
            requires_authorization: true,
            mutates_remote_state: false,
            requires_confirmation: true,
        })
    }

    fn s3_descriptor() -> super::StorageConnectorDescriptor {
        let mut path_style = storage_connector_field(
            "s3_path_style",
            StorageConnectorFieldScope::ConnectorConfig,
            StorageConnectorFieldKind::Boolean,
            false,
            false,
        );
        path_style.default_value = Some(StorageConnectorFieldDefaultValue::Boolean(true));

        let mut region = storage_connector_field_with_display(StorageConnectorFieldDisplayInput {
            name: "s3_region",
            scope: StorageConnectorFieldScope::ConnectorConfig,
            kind: StorageConnectorFieldKind::Text,
            required: false,
            secret: false,
            label_key: "s3_region",
            placeholder: Some("auto"),
            help_key: None,
            required_message_key: None,
            invalid_protocol_message_key: None,
            allowed_endpoint_protocols: Vec::new(),
            allow_endpoint_without_protocol: false,
            trim_on_blur: true,
        });
        region.default_value = Some(StorageConnectorFieldDefaultValue::String(
            "auto".to_string(),
        ));

        let timeout_field = |name: &str, default_value: i64| {
            let mut field = storage_connector_field(
                name,
                StorageConnectorFieldScope::ConnectorConfig,
                StorageConnectorFieldKind::Number,
                false,
                false,
            );
            field.default_value = Some(StorageConnectorFieldDefaultValue::Integer(default_value));
            field.validation.min_integer = Some(1);
            field
        };

        object_storage_connector_descriptor(ObjectStorageConnectorDescriptorInput {
            connector_id: ConnectorId::declared("asterdrive.storage.s3"),
            label: "S3",
            description: "test",
            ui: StorageConnectorUiDescriptorInput {
                label_key: "s3",
                description_key: "s3_desc",
                icon_src: None,
                icon_name: None,
                badge_rgb: StorageConnectorBadgeRgb::new(59, 130, 246),
                helper_key: "helper",
                config_step_title_key: "title",
                config_step_description_key: "description",
                edit_context_key: "edit",
                base_path_empty_display: "root",
                base_path_placeholder: "prefix",
            },
            deployment_scope: StorageConnectorDeploymentScope::SharedAcrossPrimaryInstances,
            supports_initial_setup: true,
            credential_mode: StorageConnectorCredentialMode::StaticSecret,
            fields: vec![
                path_style,
                region,
                timeout_field("s3_connect_timeout_secs", 5),
                timeout_field("s3_read_timeout_secs", 30),
                timeout_field("s3_operation_timeout_secs", 3_600),
            ],
            presigned_part_etag_required: true,
            storage_native_processing: false,
            config_schema_version: 3,
            related_issues: Vec::new(),
        })
    }

    #[test]
    fn badge_rgb_uses_structured_channels_and_rejects_out_of_range_values() {
        let color = StorageConnectorBadgeRgb::new(16, 185, 129);
        assert_eq!(
            serde_json::to_value(color).unwrap(),
            serde_json::json!({ "red": 16, "green": 185, "blue": 129 })
        );
        assert_eq!(
            serde_json::from_value::<StorageConnectorBadgeRgb>(serde_json::json!({
                "red": 0,
                "green": 255,
                "blue": 128
            }))
            .unwrap(),
            StorageConnectorBadgeRgb::new(0, 255, 128)
        );
        assert!(
            serde_json::from_value::<StorageConnectorBadgeRgb>(serde_json::json!({
                "red": 256,
                "green": 0,
                "blue": 0
            }))
            .is_err()
        );
    }

    #[test]
    fn connector_options_apply_defaults_and_normalize_text() {
        let descriptor = s3_descriptor();
        let input = ConnectorConfigEnvelope {
            format_version: CONNECTOR_CONFIG_FORMAT_VERSION,
            connector_id: ConnectorId::declared("asterdrive.storage.s3"),
            schema_version: 3,
            values: BTreeMap::from([(
                "s3_region".to_string(),
                serde_json::json!("  cn-beijing  "),
            )]),
        };

        let normalized = normalize_storage_connector_config(&descriptor, &input).unwrap();

        assert_eq!(normalized.values["s3_region"], "cn-beijing");
        assert_eq!(normalized.values["s3_path_style"], true);
        assert_eq!(normalized.values["s3_connect_timeout_secs"], 5);
        assert_eq!(normalized.values["s3_read_timeout_secs"], 30);
        assert_eq!(normalized.values["s3_operation_timeout_secs"], 3_600);
    }

    #[test]
    fn connector_default_mode_distinguishes_missing_and_empty_text() {
        let mut descriptor = s3_descriptor();
        let mut base_path = storage_connector_field(
            "base_path",
            StorageConnectorFieldScope::ConnectorConfig,
            StorageConnectorFieldKind::Text,
            false,
            false,
        );
        base_path.default_value = Some(StorageConnectorFieldDefaultValue::String(
            "connector-root".to_string(),
        ));
        base_path.default_mode = StorageConnectorFieldDefaultMode::MissingOrEmptyText;
        base_path.validate().unwrap();
        descriptor.fields.push(base_path.clone());

        for values in [
            BTreeMap::new(),
            BTreeMap::from([("base_path".to_string(), serde_json::json!(""))]),
        ] {
            let normalized = normalize_storage_connector_config(
                &descriptor,
                &ConnectorConfigEnvelope {
                    format_version: CONNECTOR_CONFIG_FORMAT_VERSION,
                    connector_id: ConnectorId::declared("asterdrive.storage.s3"),
                    schema_version: 3,
                    values,
                },
            )
            .unwrap();
            assert_eq!(normalized.values["base_path"], "connector-root");
        }

        let normalized = normalize_storage_connector_config(
            &descriptor,
            &ConnectorConfigEnvelope {
                format_version: CONNECTOR_CONFIG_FORMAT_VERSION,
                connector_id: ConnectorId::declared("asterdrive.storage.s3"),
                schema_version: 3,
                values: BTreeMap::from([("base_path".to_string(), serde_json::json!("tenant-a"))]),
            },
        )
        .unwrap();
        assert_eq!(normalized.values["base_path"], "tenant-a");

        base_path.default_mode = StorageConnectorFieldDefaultMode::MissingOnly;
        descriptor.fields.pop();
        descriptor.fields.push(base_path);
        let normalized = normalize_storage_connector_config(
            &descriptor,
            &ConnectorConfigEnvelope {
                format_version: CONNECTOR_CONFIG_FORMAT_VERSION,
                connector_id: ConnectorId::declared("asterdrive.storage.s3"),
                schema_version: 3,
                values: BTreeMap::from([("base_path".to_string(), serde_json::json!(""))]),
            },
        )
        .unwrap();
        assert!(!normalized.values.contains_key("base_path"));
    }

    #[test]
    fn empty_text_default_mode_rejects_invalid_descriptor_combinations() {
        let mut field = storage_connector_field(
            "base_path",
            StorageConnectorFieldScope::ConnectorConfig,
            StorageConnectorFieldKind::Text,
            false,
            false,
        );
        field.default_mode = StorageConnectorFieldDefaultMode::MissingOrEmptyText;
        assert!(field.validate().is_err());
        assert!(matches!(
            normalize_storage_connector_field_values(
                [&field],
                &BTreeMap::from([("base_path".to_string(), serde_json::json!(""))]),
                false,
            ),
            Err(StorageConnectorOptionsValidationError::InvalidFieldDescriptor { .. })
        ));

        field.default_value = Some(StorageConnectorFieldDefaultValue::String(String::new()));
        field.required = true;
        assert!(field.validate().is_err());

        field.required = false;
        field.kind = StorageConnectorFieldKind::Boolean;
        field.default_value = Some(StorageConnectorFieldDefaultValue::Boolean(false));
        assert!(field.validate().is_err());
    }

    #[test]
    fn connector_options_reject_namespace_version_unknown_type_and_range_errors() {
        let descriptor = s3_descriptor();
        let namespace = ConnectorConfigEnvelope {
            format_version: CONNECTOR_CONFIG_FORMAT_VERSION,
            connector_id: ConnectorId::declared("asterdrive.storage.s3"),
            schema_version: 3,
            values: BTreeMap::new(),
        };
        assert!(matches!(
            normalize_storage_connector_config(
                &descriptor,
                &ConnectorConfigEnvelope {
                    connector_id: ConnectorId::declared("wrong.namespace"),
                    ..namespace.clone()
                }
            ),
            Err(StorageConnectorOptionsValidationError::NamespaceMismatch { .. })
        ));

        let mut wrong_version = namespace.clone();
        wrong_version.schema_version = 2;
        assert!(matches!(
            normalize_storage_connector_config(&descriptor, &wrong_version),
            Err(StorageConnectorOptionsValidationError::SchemaVersionMismatch { .. })
        ));

        let mut unknown = namespace.clone();
        unknown
            .values
            .insert("secret_backdoor".to_string(), serde_json::json!(true));
        assert!(matches!(
            normalize_storage_connector_config(&descriptor, &unknown),
            Err(StorageConnectorOptionsValidationError::UnknownField(_))
        ));

        let mut wrong_type = namespace.clone();
        wrong_type
            .values
            .insert("s3_path_style".to_string(), serde_json::json!("false"));
        assert!(matches!(
            normalize_storage_connector_config(&descriptor, &wrong_type),
            Err(StorageConnectorOptionsValidationError::TypeMismatch { .. })
        ));

        let mut below_minimum = namespace;
        below_minimum
            .values
            .insert("s3_connect_timeout_secs".to_string(), serde_json::json!(0));
        assert!(matches!(
            normalize_storage_connector_config(&descriptor, &below_minimum),
            Err(StorageConnectorOptionsValidationError::IntegerBelowMinimum { .. })
        ));
    }

    #[test]
    fn connector_option_defaults_match_declared_scalar_types() {
        let descriptor = s3_descriptor();
        let path_style = descriptor
            .fields
            .iter()
            .find(|field| field.name == "s3_path_style")
            .unwrap();
        assert_eq!(
            path_style.default_value,
            Some(StorageConnectorFieldDefaultValue::Boolean(true))
        );
    }

    #[test]
    fn select_contract_preserves_connector_owned_labels_and_accepts_dynamic_integer_values() {
        let static_field = storage_connector_select_field(
            "mode",
            StorageConnectorFieldScope::ConnectorConfig,
            true,
            vec![StorageConnectorSelectOptionInput {
                value: "check",
                label_key: "plugin_mode_check",
                description_key: Some("plugin_mode_check_desc"),
            }],
        );
        let select = static_field.select.as_ref().unwrap();
        assert_eq!(select.value_kind, StorageConnectorSelectValueKind::String);
        assert_eq!(
            select.options,
            vec![StorageConnectorSelectOption {
                value: StorageConnectorSelectOptionValue::String("check".to_string()),
                label_key: "plugin_mode_check".to_string(),
                description_key: Some("plugin_mode_check_desc".to_string()),
            }]
        );
        static_field.validate().unwrap();

        let remote_node = storage_connector_dynamic_select_field(
            "remote_node_id",
            StorageConnectorFieldScope::ConnectorConfig,
            true,
            StorageConnectorSelectValueKind::Integer,
            StorageConnectorSelectDataSource::RemoteNodes,
            None,
        );
        remote_node.validate().unwrap();

        let mut descriptor = s3_descriptor();
        descriptor.fields = vec![remote_node];
        let valid = ConnectorConfigEnvelope {
            format_version: CONNECTOR_CONFIG_FORMAT_VERSION,
            connector_id: descriptor.connector_id.clone(),
            schema_version: descriptor.config_schema_version,
            values: BTreeMap::from([("remote_node_id".to_string(), serde_json::json!(7))]),
        };
        let normalized = normalize_storage_connector_config(&descriptor, &valid).unwrap();
        assert_eq!(normalized.values["remote_node_id"], 7);

        let mut wrong_type = valid;
        wrong_type
            .values
            .insert("remote_node_id".to_string(), serde_json::json!("7"));
        assert!(matches!(
            normalize_storage_connector_config(&descriptor, &wrong_type),
            Err(StorageConnectorOptionsValidationError::TypeMismatch { field, .. })
                if field == "remote_node_id"
        ));
    }

    #[test]
    fn select_field_validation_rejects_ambiguous_and_incoherent_contracts() {
        let valid = storage_connector_select_field(
            "mode",
            StorageConnectorFieldScope::ConnectorConfig,
            true,
            vec![StorageConnectorSelectOptionInput {
                value: "check",
                label_key: "plugin_mode_check",
                description_key: None,
            }],
        );
        let mut cases = Vec::new();

        let mut missing_contract = valid.clone();
        missing_contract.select = None;
        cases.push((missing_contract, "missing its select contract"));

        let mut non_select = valid.clone();
        non_select.kind = StorageConnectorFieldKind::Text;
        cases.push((non_select, "non-select field"));

        let mut both_sources = valid.clone();
        both_sources.select.as_mut().unwrap().data_source =
            Some(StorageConnectorSelectDataSource::RemoteNodes);
        cases.push((both_sources, "exactly one"));

        let mut duplicate_option = valid.clone();
        let first_option = duplicate_option.select.as_ref().unwrap().options[0].clone();
        duplicate_option
            .select
            .as_mut()
            .unwrap()
            .options
            .push(first_option);
        cases.push((duplicate_option, "duplicate option"));

        let mut wrong_option_type = valid.clone();
        wrong_option_type.select.as_mut().unwrap().options[0].value =
            StorageConnectorSelectOptionValue::Integer(1);
        cases.push((wrong_option_type, "does not match value_kind"));

        let mut invalid_default = valid.clone();
        invalid_default.default_value = Some(StorageConnectorFieldDefaultValue::String(
            "missing".to_string(),
        ));
        cases.push((invalid_default, "default value is not a declared option"));

        let wrong_dynamic_type = storage_connector_dynamic_select_field(
            "remote_node_id",
            StorageConnectorFieldScope::ConnectorConfig,
            true,
            StorageConnectorSelectValueKind::String,
            StorageConnectorSelectDataSource::RemoteNodes,
            None,
        );
        cases.push((wrong_dynamic_type, "remote_nodes source requires integer"));

        let missing_dynamic_dependency = storage_connector_dynamic_select_field(
            "remote_storage_target_key",
            StorageConnectorFieldScope::ConnectorConfig,
            true,
            StorageConnectorSelectValueKind::String,
            StorageConnectorSelectDataSource::RemoteStorageTargets,
            None,
        );
        cases.push((
            missing_dynamic_dependency,
            "remote_storage_targets source requires string values and one dependency",
        ));

        for (field, expected_message) in cases {
            let error = field
                .validate()
                .expect_err("invalid select contract must be rejected");
            assert!(
                error.to_string().contains(expected_message),
                "expected '{expected_message}' in '{error}'"
            );
        }
    }

    #[test]
    fn connector_descriptor_validation_rejects_missing_and_cyclic_dependencies() {
        let target_field = |name: &str, dependency: &str| {
            storage_connector_dynamic_select_field(
                name,
                StorageConnectorFieldScope::ConnectorConfig,
                true,
                StorageConnectorSelectValueKind::String,
                StorageConnectorSelectDataSource::RemoteStorageTargets,
                Some(dependency),
            )
        };

        let mut missing = s3_descriptor();
        missing.fields = vec![target_field("target", "remote_node_id")];
        assert!(
            missing
                .validate()
                .unwrap_err()
                .to_string()
                .contains("depends on undeclared field")
        );

        let mut cyclic = s3_descriptor();
        cyclic.fields = vec![
            target_field("target_a", "target_b"),
            target_field("target_b", "target_a"),
        ];
        assert!(
            cyclic
                .validate()
                .unwrap_err()
                .to_string()
                .contains("dependency cycle")
        );
    }

    #[test]
    fn custom_action_descriptor_exposes_identity_fields_and_execution_contract() {
        let descriptor = plugin_action_descriptor();

        assert_eq!(descriptor.action_id.as_str(), "plugin.validate_path");
        assert_eq!(descriptor.kind, StorageConnectorActionKind::Custom);
        assert_eq!(
            descriptor.endpoints,
            vec![StorageConnectorActionEndpoint::ExecuteDraftStoragePolicyAction]
        );
        assert!(!descriptor.requires_saved_policy);
        assert!(descriptor.requires_authorization);
        assert!(descriptor.requires_confirmation);
        assert_eq!(descriptor.fields, TestPluginActionInput::action_fields());
        assert!(
            descriptor
                .fields
                .iter()
                .all(|field| field.scope == StorageConnectorFieldScope::ActionInput)
        );
    }

    #[test]
    fn action_descriptor_validation_accepts_a_complete_connector_owned_schema() {
        plugin_action_descriptor()
            .validate()
            .expect("complete plugin action descriptor should be accepted");
    }

    #[test]
    fn action_descriptor_validation_rejects_incoherent_contract_boundaries() {
        let valid = plugin_action_descriptor();
        let mut cases = Vec::new();

        let mut empty_label = valid.clone();
        empty_label.label_key = "  ".to_string();
        cases.push((empty_label, "label_key"));

        let mut empty_description = valid.clone();
        empty_description.description_key.clear();
        cases.push((empty_description, "description_key"));

        let mut empty_endpoints = valid.clone();
        empty_endpoints.endpoints.clear();
        cases.push((empty_endpoints, "at least one endpoint"));

        let mut duplicate_endpoint = valid.clone();
        duplicate_endpoint
            .endpoints
            .push(StorageConnectorActionEndpoint::ExecuteDraftStoragePolicyAction);
        cases.push((duplicate_endpoint, "same endpoint"));

        let mut wrong_kind_endpoint = valid.clone();
        wrong_kind_endpoint.kind = StorageConnectorActionKind::Authorization;
        cases.push((wrong_kind_endpoint, "does not accept endpoint"));

        let mut saved_requirement = valid.clone();
        saved_requirement.requires_saved_policy = true;
        cases.push((saved_requirement, "requires_saved_policy"));

        let mut wrong_field_scope = valid.clone();
        wrong_field_scope.fields[0].scope = StorageConnectorFieldScope::ConnectorConfig;
        cases.push((wrong_field_scope, "action_input scope"));

        let mut duplicate_field = valid;
        duplicate_field
            .fields
            .push(duplicate_field.fields[0].clone());
        cases.push((duplicate_field, "declared more than once"));

        for (descriptor, expected_message) in cases {
            let error = descriptor
                .validate()
                .expect_err("incoherent action descriptor must be rejected");
            assert!(
                error.to_string().contains(expected_message),
                "expected '{expected_message}' in '{error}'"
            );
        }
    }

    #[test]
    fn action_id_validation_accepts_plugin_namespaces_and_rejects_ambiguous_keys() {
        for value in [
            "run",
            "configure_cors",
            "plugin.verify-path",
            "com.example.storage.verify_1",
        ] {
            assert!(
                StorageConnectorActionId::declared(value).validate().is_ok(),
                "valid action id rejected: {value}"
            );
        }

        for value in [
            "",
            "ab",
            "Invalid",
            "has space",
            ".leading",
            "trailing_",
            "double..segment",
            "插件.action",
        ] {
            assert!(
                StorageConnectorActionId::declared(value)
                    .validate()
                    .is_err(),
                "invalid action id accepted: {value}"
            );
        }
    }

    #[test]
    fn action_input_applies_defaults_normalizes_text_and_accepts_ephemeral_secrets() {
        let descriptor = plugin_action_descriptor();
        let input = BTreeMap::from([
            ("path".to_string(), serde_json::json!("  /uploads/file  ")),
            ("mode".to_string(), serde_json::json!("check")),
            ("token".to_string(), serde_json::json!("secret-value")),
        ]);

        let normalized = normalize_storage_connector_action_input(&descriptor, &input).unwrap();
        let decoded: TestPluginActionInput =
            serde_json::from_value(serde_json::to_value(&normalized).unwrap()).unwrap();

        assert_eq!(decoded.path, "/uploads/file");
        assert_eq!(decoded.mode, "check");
        assert_eq!(decoded.token, "secret-value");
        assert!(!decoded.force);
    }

    #[test]
    fn action_input_rejects_unknown_missing_invalid_option_and_nested_values() {
        let descriptor = plugin_action_descriptor();
        let valid = BTreeMap::from([
            ("path".to_string(), serde_json::json!("/uploads/file")),
            ("mode".to_string(), serde_json::json!("check")),
            ("token".to_string(), serde_json::json!("secret-value")),
        ]);

        let mut unknown = valid.clone();
        unknown.insert("undeclared".to_string(), serde_json::json!(true));
        assert!(matches!(
            normalize_storage_connector_action_input(&descriptor, &unknown),
            Err(StorageConnectorOptionsValidationError::UnknownField(field))
                if field == "undeclared"
        ));

        let mut missing = valid.clone();
        missing.remove("path");
        assert!(matches!(
            normalize_storage_connector_action_input(&descriptor, &missing),
            Err(StorageConnectorOptionsValidationError::MissingRequiredField(field))
                if field == "path"
        ));

        let mut invalid_option = valid.clone();
        invalid_option.insert("mode".to_string(), serde_json::json!("delete"));
        assert!(matches!(
            normalize_storage_connector_action_input(&descriptor, &invalid_option),
            Err(StorageConnectorOptionsValidationError::InvalidOption { field, value })
                if field == "mode" && value == "delete"
        ));

        let mut nested = valid;
        nested.insert("path".to_string(), serde_json::json!({ "nested": true }));
        assert!(matches!(
            normalize_storage_connector_action_input(&descriptor, &nested),
            Err(StorageConnectorOptionsValidationError::TypeMismatch { field, .. })
                if field == "path"
        ));
    }

    #[test]
    fn custom_action_invocation_resolves_endpoint_and_normalizes_values() {
        let mut connector = s3_descriptor();
        connector.actions = vec![plugin_action_descriptor()];
        let input = BTreeMap::from([
            ("path".to_string(), serde_json::json!("  /uploads/file  ")),
            ("mode".to_string(), serde_json::json!("check")),
            ("token".to_string(), serde_json::json!("secret-value")),
        ]);

        let normalized = normalize_storage_connector_custom_action_invocation(
            &connector,
            &StorageConnectorActionId::declared("plugin.validate_path"),
            StorageConnectorActionEndpoint::ExecuteDraftStoragePolicyAction,
            &input,
        )
        .expect("declared custom action endpoint should resolve");

        assert_eq!(normalized["path"], "/uploads/file");
        assert_eq!(normalized["force"], false);
    }

    #[test]
    fn custom_action_invocation_rejects_lookup_and_input_boundaries() {
        let mut connector = s3_descriptor();
        connector.actions = vec![plugin_action_descriptor()];

        for (action_id, endpoint) in [
            (
                "plugin.missing",
                StorageConnectorActionEndpoint::ExecuteDraftStoragePolicyAction,
            ),
            (
                "plugin.validate_path",
                StorageConnectorActionEndpoint::ExecuteSavedStoragePolicyAction,
            ),
        ] {
            assert!(matches!(
                normalize_storage_connector_custom_action_invocation(
                    &connector,
                    &StorageConnectorActionId::declared(action_id),
                    endpoint,
                    &BTreeMap::new(),
                ),
                Err(StorageConnectorActionInvocationError::Unsupported { .. })
            ));
        }

        for input in [
            BTreeMap::new(),
            BTreeMap::from([
                ("path".to_string(), serde_json::json!("/uploads/file")),
                ("mode".to_string(), serde_json::json!("check")),
                ("token".to_string(), serde_json::json!("secret-value")),
                ("undeclared".to_string(), serde_json::json!(true)),
            ]),
            BTreeMap::from([
                ("path".to_string(), serde_json::json!({ "nested": true })),
                ("mode".to_string(), serde_json::json!("check")),
                ("token".to_string(), serde_json::json!("secret-value")),
            ]),
        ] {
            assert!(matches!(
                normalize_storage_connector_custom_action_invocation(
                    &connector,
                    &StorageConnectorActionId::declared("plugin.validate_path"),
                    StorageConnectorActionEndpoint::ExecuteDraftStoragePolicyAction,
                    &input,
                ),
                Err(StorageConnectorActionInvocationError::InvalidInput(_))
            ));
        }
    }
}
