//! Storage connector descriptors for admin policy UI capability discovery.
//!
//! Descriptor 是 connector 对外声明的“配置/管理能力清单”。前端用它决定显示哪些
//! 字段、按钮和提示；后端服务也用它 gate 授权、连接测试、policy action 等入口。
//! 它不是 runtime driver，本文件不应该承载实际对象读写逻辑。

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::fmt;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use aster_drive_model::types::OBJECT_MULTIPART_MIN_PART_SIZE;

use crate::ConnectorId;

use super::field_contract::{StorageDescriptorFieldKind, StorageDescriptorFieldSemantics};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum StorageConnectorCredentialMode {
    /// 不需要密钥或远端绑定，例如纯本地路径。
    None,
    /// 使用 access_key / secret_key 这类静态密钥。
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
/// `DriverType` allow/deny list.
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum StorageConnectorFieldScope {
    /// 写入 `storage_policies` 通用连接字段，例如 endpoint/bucket/base_path。
    Connection,
    /// 写入当前 connector namespace 下的版本化 provider options。
    ///
    /// 这类字段由 connector 负责 normalize/validate，core policy options 不应知道
    /// 字段名，也不应把该值转存回 `StoragePolicyOptions`。
    ConnectorOptions,
    /// 写入 connector-owned application config，不应混进 legacy access_key/secret_key。
    ApplicationCredential,
    /// 绑定外部 runtime 资源，例如 remote node。
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum StorageConnectorAffordanceAction {
    /// 展示/启用 OAuth 或类似授权入口。
    StartAuthorization,
    /// 展示/启用已授权 credential 的校验入口。
    ValidateCredential,
    /// 展示/启用未保存参数连接测试入口。
    TestDraftConnection,
    /// 展示/启用已保存 policy 连接测试入口。
    TestSavedConnection,
}

impl StorageConnectorAffordanceAction {
    pub const fn as_str(self) -> &'static str {
        match self {
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
    /// 在 Tencent COS 上配置 CORS。
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum StorageConnectorActionKind {
    /// Provider/policy 专属动作，可能修改远端状态。
    PolicyAction,
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
    /// 真正的 policy/provider action。和 `affordance_action` 二选一。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy_action: Option<StoragePolicyExecutableAction>,
    /// UI/服务 affordance。和 `policy_action` 二选一。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub affordance_action: Option<StorageConnectorAffordanceAction>,
    /// 用于把 action 归类到授权、连接测试、policy action 等入口。
    pub kind: StorageConnectorActionKind,
    /// 该 action 可通过哪些后端 endpoint 执行。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub endpoints: Vec<StorageConnectorActionEndpoint>,
    /// true 表示必须先保存 policy，draft 参数不能执行。
    pub requires_saved_policy: bool,
    /// true 表示执行前必须存在可用授权凭据。
    pub requires_authorization: bool,
    /// true 表示该动作会修改 provider 远端状态。
    pub mutates_remote_state: bool,
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// select/radio 等枚举控件的稳定取值。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<String>,
    /// Connector schema 定义的默认值。省略表示该字段没有隐式默认值。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_value: Option<StorageConnectorFieldDefaultValue>,
    /// 可被前端用于即时反馈、且必须由后端再次执行的基础约束。
    #[serde(default)]
    pub validation: StorageConnectorFieldValidation,
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

    let mut fields = HashMap::new();
    for field in descriptor
        .fields
        .iter()
        .filter(|field| field.scope == StorageConnectorFieldScope::ConnectorOptions)
    {
        if fields.insert(field.name.as_str(), field).is_some() {
            return Err(StorageConnectorOptionsValidationError::DuplicateField(
                field.name.clone(),
            ));
        }
    }

    for name in input.values.keys() {
        if !fields.contains_key(name.as_str()) {
            return Err(StorageConnectorOptionsValidationError::UnknownField(
                name.clone(),
            ));
        }
    }

    let mut normalized = BTreeMap::new();
    for field in fields.values() {
        if field.secret || field.kind == StorageConnectorFieldKind::Secret {
            if input.values.contains_key(&field.name) {
                return Err(StorageConnectorOptionsValidationError::SecretField(
                    field.name.clone(),
                ));
            }
            continue;
        }

        let value = input
            .values
            .get(&field.name)
            .cloned()
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

        normalize_and_validate_provider_option_value(field, &mut value)?;
        if value.as_str().is_some_and(str::is_empty) && !field.required {
            continue;
        }
        normalized.insert(field.name.clone(), value);
    }

    Ok(crate::ConnectorConfigEnvelope {
        format_version: crate::CONNECTOR_CONFIG_FORMAT_VERSION,
        connector_id: input.connector_id.clone(),
        schema_version: descriptor.config_schema_version,
        values: normalized,
    })
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

fn normalize_and_validate_provider_option_value(
    field: &StorageConnectorFieldDescriptor,
    value: &mut serde_json::Value,
) -> Result<(), StorageConnectorOptionsValidationError> {
    match field.kind {
        StorageConnectorFieldKind::Text | StorageConnectorFieldKind::Select => {
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
            if field.kind == StorageConnectorFieldKind::Select
                && !field.options.iter().any(|option| option == normalized)
            {
                return Err(StorageConnectorOptionsValidationError::InvalidOption {
                    field: field.name.clone(),
                    value: normalized.to_string(),
                });
            }
            *value = serde_json::Value::String(normalized.to_string());
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
        StorageConnectorFieldKind::Secret => {
            return Err(StorageConnectorOptionsValidationError::SecretField(
                field.name.clone(),
            ));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StorageConnectorEndpointHostRule {
    /// Exact hostname match after URL parsing and lower-casing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub equals: Option<String>,
    /// Suffix hostname match after URL parsing and lower-casing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ends_with: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StorageConnectorDriverRecommendation {
    /// Candidate connector that should be suggested for matching endpoint hosts.
    pub target_connector_id: ConnectorId,
    /// Host rules owned by the source connector.
    ///
    /// This keeps provider-detection rules in connector metadata instead of in
    /// the admin UI. Frontend code only performs generic URL host matching.
    pub endpoint_host_rules: Vec<StorageConnectorEndpointHostRule>,
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
    /// Connector-owned recommendations for moving a policy to a more specific driver.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub driver_recommendations: Vec<StorageConnectorDriverRecommendation>,
    /// 用于开发追踪的相关 issue 编号，不参与业务逻辑。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related_issues: Vec<u16>,
}

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

pub struct ObjectStorageConnectorDescriptorInput {
    pub connector_id: ConnectorId,
    pub label: &'static str,
    pub description: &'static str,
    pub ui: StorageConnectorUiDescriptorInput,
    pub deployment_scope: StorageConnectorDeploymentScope,
    pub supports_initial_setup: bool,
    pub fields: ObjectStorageFieldDescriptorInput,
    pub include_s3_path_style: bool,
    pub include_s3_region: bool,
    pub include_s3_timeouts: bool,
    pub presigned_part_etag_required: bool,
    pub storage_native_processing: bool,
    pub config_schema_version: u32,
    pub related_issues: Vec<u16>,
}

pub struct ObjectStorageFieldDescriptorInput {
    pub endpoint_placeholder: &'static str,
    pub endpoint_help_key: &'static str,
    pub endpoint_protocol_error_key: &'static str,
    pub bucket_required_message_key: &'static str,
    pub access_key_label_key: &'static str,
    pub secret_key_label_key: &'static str,
    pub access_key_trim_on_blur: bool,
}

pub fn object_storage_connector_descriptor(
    input: ObjectStorageConnectorDescriptorInput,
) -> StorageConnectorDescriptor {
    let mut fields = vec![
        storage_connector_field_with_display(StorageConnectorFieldDisplayInput {
            name: "endpoint",
            scope: StorageConnectorFieldScope::Connection,
            kind: StorageConnectorFieldKind::Text,
            required: true,
            secret: false,
            label_key: "endpoint",
            placeholder: Some(input.fields.endpoint_placeholder),
            help_key: Some(input.fields.endpoint_help_key),
            required_message_key: None,
            invalid_protocol_message_key: Some(input.fields.endpoint_protocol_error_key),
            allowed_endpoint_protocols: vec!["http:", "https:"],
            allow_endpoint_without_protocol: false,
            trim_on_blur: false,
        }),
        storage_connector_field_with_display(StorageConnectorFieldDisplayInput {
            name: "bucket",
            scope: StorageConnectorFieldScope::Connection,
            kind: StorageConnectorFieldKind::Text,
            required: true,
            secret: false,
            label_key: "bucket",
            placeholder: None,
            help_key: None,
            required_message_key: Some(input.fields.bucket_required_message_key),
            invalid_protocol_message_key: None,
            allowed_endpoint_protocols: Vec::new(),
            allow_endpoint_without_protocol: false,
            trim_on_blur: false,
        }),
        storage_connector_field_with_display(StorageConnectorFieldDisplayInput {
            name: "access_key",
            scope: StorageConnectorFieldScope::Connection,
            kind: StorageConnectorFieldKind::Text,
            required: true,
            secret: false,
            label_key: input.fields.access_key_label_key,
            placeholder: None,
            help_key: None,
            required_message_key: None,
            invalid_protocol_message_key: None,
            allowed_endpoint_protocols: Vec::new(),
            allow_endpoint_without_protocol: false,
            trim_on_blur: input.fields.access_key_trim_on_blur,
        }),
        storage_connector_field_with_display(StorageConnectorFieldDisplayInput {
            name: "secret_key",
            scope: StorageConnectorFieldScope::Connection,
            kind: StorageConnectorFieldKind::Secret,
            required: true,
            secret: true,
            label_key: input.fields.secret_key_label_key,
            placeholder: None,
            help_key: None,
            required_message_key: None,
            invalid_protocol_message_key: None,
            allowed_endpoint_protocols: Vec::new(),
            allow_endpoint_without_protocol: false,
            trim_on_blur: false,
        }),
        storage_connector_field(
            "base_path",
            StorageConnectorFieldScope::Connection,
            StorageConnectorFieldKind::Text,
            false,
            false,
        ),
        {
            let mut field = storage_connector_field_with_options(
                "object_storage_upload_strategy",
                StorageConnectorFieldScope::ConnectorOptions,
                StorageConnectorFieldKind::Select,
                true,
                false,
                vec!["relay_stream", "presigned"],
            );
            field.default_value = Some(StorageConnectorFieldDefaultValue::String(
                "relay_stream".to_string(),
            ));
            field
        },
        {
            let mut field = storage_connector_field_with_options(
                "object_storage_download_strategy",
                StorageConnectorFieldScope::ConnectorOptions,
                StorageConnectorFieldKind::Select,
                true,
                false,
                vec!["relay_stream", "presigned"],
            );
            field.default_value = Some(StorageConnectorFieldDefaultValue::String(
                "relay_stream".to_string(),
            ));
            field
        },
    ];
    if input.include_s3_path_style {
        let mut field = storage_connector_field_with_display(StorageConnectorFieldDisplayInput {
            name: "s3_path_style",
            scope: StorageConnectorFieldScope::ConnectorOptions,
            kind: StorageConnectorFieldKind::Boolean,
            required: false,
            secret: false,
            label_key: "s3_path_style",
            placeholder: None,
            help_key: Some("s3_path_style_desc"),
            required_message_key: None,
            invalid_protocol_message_key: None,
            allowed_endpoint_protocols: Vec::new(),
            allow_endpoint_without_protocol: false,
            trim_on_blur: false,
        });
        field.default_value = Some(StorageConnectorFieldDefaultValue::Boolean(true));
        fields.push(field);
    }
    if input.include_s3_region {
        let mut field = storage_connector_field_with_display(StorageConnectorFieldDisplayInput {
            name: "s3_region",
            scope: StorageConnectorFieldScope::ConnectorOptions,
            kind: StorageConnectorFieldKind::Text,
            required: false,
            secret: false,
            label_key: "s3_region",
            placeholder: Some("auto"),
            help_key: Some("s3_region_desc"),
            required_message_key: None,
            invalid_protocol_message_key: None,
            allowed_endpoint_protocols: Vec::new(),
            allow_endpoint_without_protocol: false,
            trim_on_blur: true,
        });
        field.default_value = Some(StorageConnectorFieldDefaultValue::String(
            "auto".to_string(),
        ));
        field.validation.max_length = Some(128);
        fields.push(field);
    }
    if input.include_s3_timeouts {
        for (name, default_value) in [
            ("s3_connect_timeout_secs", 5),
            ("s3_read_timeout_secs", 30),
            ("s3_operation_timeout_secs", 3_600),
        ] {
            let mut field = storage_connector_field(
                name,
                StorageConnectorFieldScope::ConnectorOptions,
                StorageConnectorFieldKind::Number,
                false,
                false,
            );
            field.default_value = Some(StorageConnectorFieldDefaultValue::Integer(default_value));
            field.validation.min_integer = Some(1);
            fields.push(field);
        }
    }

    StorageConnectorDescriptor {
        connector_id: input.connector_id,
        label: input.label.to_string(),
        description: input.description.to_string(),
        ui: storage_connector_ui_descriptor(input.ui),
        credential_mode: StorageConnectorCredentialMode::StaticSecret,
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
        fields,
        config_schema_version: input.config_schema_version,
        actions: vec![
            draft_connection_test_action_descriptor(),
            saved_connection_test_action_descriptor(false),
        ],
        driver_recommendations: Vec::new(),
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

pub fn endpoint_driver_recommendation(
    target_connector_id: ConnectorId,
    endpoint_host_rules: Vec<StorageConnectorEndpointHostRule>,
) -> StorageConnectorDriverRecommendation {
    StorageConnectorDriverRecommendation {
        target_connector_id,
        endpoint_host_rules,
    }
}

pub fn endpoint_host_rule(
    equals: Option<&str>,
    ends_with: Option<&str>,
) -> StorageConnectorEndpointHostRule {
    StorageConnectorEndpointHostRule {
        equals: equals.map(ToOwned::to_owned),
        ends_with: ends_with.map(ToOwned::to_owned),
    }
}

pub fn policy_action_descriptor(
    action: StoragePolicyExecutableAction,
) -> StorageConnectorActionDescriptor {
    StorageConnectorActionDescriptor {
        policy_action: Some(action),
        affordance_action: None,
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

pub fn start_authorization_action_descriptor() -> StorageConnectorActionDescriptor {
    StorageConnectorActionDescriptor {
        policy_action: None,
        affordance_action: Some(StorageConnectorAffordanceAction::StartAuthorization),
        kind: StorageConnectorActionKind::Authorization,
        endpoints: vec![StorageConnectorActionEndpoint::StartStorageAuthorization],
        requires_saved_policy: true,
        requires_authorization: false,
        mutates_remote_state: false,
    }
}

pub fn validate_credential_action_descriptor() -> StorageConnectorActionDescriptor {
    StorageConnectorActionDescriptor {
        policy_action: None,
        affordance_action: Some(StorageConnectorAffordanceAction::ValidateCredential),
        kind: StorageConnectorActionKind::CredentialValidation,
        endpoints: vec![StorageConnectorActionEndpoint::ValidateStoragePolicyCredential],
        requires_saved_policy: true,
        requires_authorization: true,
        mutates_remote_state: false,
    }
}

pub fn draft_connection_test_action_descriptor() -> StorageConnectorActionDescriptor {
    StorageConnectorActionDescriptor {
        policy_action: None,
        affordance_action: Some(StorageConnectorAffordanceAction::TestDraftConnection),
        kind: StorageConnectorActionKind::ConnectionTest,
        endpoints: vec![StorageConnectorActionEndpoint::TestPolicyParams],
        requires_saved_policy: false,
        requires_authorization: false,
        mutates_remote_state: false,
    }
}

pub fn saved_connection_test_action_descriptor(
    requires_authorization: bool,
) -> StorageConnectorActionDescriptor {
    StorageConnectorActionDescriptor {
        policy_action: None,
        affordance_action: Some(StorageConnectorAffordanceAction::TestSavedConnection),
        kind: StorageConnectorActionKind::ConnectionTest,
        endpoints: vec![StorageConnectorActionEndpoint::TestPolicyConnection],
        requires_saved_policy: true,
        requires_authorization,
        mutates_remote_state: false,
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
        options: Vec::new(),
        default_value: None,
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
        options: options.into_iter().map(ToOwned::to_owned).collect(),
        ..storage_connector_field(name, scope, kind, required, secret)
    }
}

pub struct StorageConnectorUiDescriptorInput {
    pub label_key: &'static str,
    pub description_key: &'static str,
    pub icon_src: Option<&'static str>,
    pub icon_name: Option<&'static str>,
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
        ObjectStorageConnectorDescriptorInput, ObjectStorageFieldDescriptorInput,
        StorageConnectorDeploymentScope, StorageConnectorFieldDefaultValue,
        StorageConnectorOptionsValidationError, StorageConnectorUiDescriptorInput,
        normalize_storage_connector_config, object_storage_connector_descriptor,
    };
    use crate::{CONNECTOR_CONFIG_FORMAT_VERSION, ConnectorConfigEnvelope, ConnectorId};

    fn s3_descriptor() -> super::StorageConnectorDescriptor {
        object_storage_connector_descriptor(ObjectStorageConnectorDescriptorInput {
            connector_id: ConnectorId::declared("asterdrive.storage.s3"),
            label: "S3",
            description: "test",
            ui: StorageConnectorUiDescriptorInput {
                label_key: "s3",
                description_key: "s3_desc",
                icon_src: None,
                icon_name: None,
                helper_key: "helper",
                config_step_title_key: "title",
                config_step_description_key: "description",
                edit_context_key: "edit",
                base_path_empty_display: "root",
                base_path_placeholder: "prefix",
            },
            deployment_scope: StorageConnectorDeploymentScope::SharedAcrossPrimaryInstances,
            supports_initial_setup: true,
            fields: ObjectStorageFieldDescriptorInput {
                endpoint_placeholder: "https://s3.example.com",
                endpoint_help_key: "endpoint_help",
                endpoint_protocol_error_key: "endpoint_protocol",
                bucket_required_message_key: "bucket_required",
                access_key_label_key: "access_key",
                secret_key_label_key: "secret_key",
                access_key_trim_on_blur: false,
            },
            include_s3_path_style: true,
            include_s3_region: true,
            include_s3_timeouts: true,
            presigned_part_etag_required: true,
            storage_native_processing: false,
            config_schema_version: 3,
            related_issues: Vec::new(),
        })
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
}
