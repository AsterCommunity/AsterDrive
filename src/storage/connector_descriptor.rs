//! Storage connector descriptors for admin policy UI capability discovery.
//!
//! Descriptor 是 connector 对外声明的“配置/管理能力清单”。前端用它决定显示哪些
//! 字段、按钮和提示；后端服务也用它 gate 授权、连接测试、policy action 等入口。
//! 它不是 runtime driver，本文件不应该承载实际对象读写逻辑。

use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::types::{DriverType, OBJECT_MULTIPART_MIN_PART_SIZE};

/// 为 connector 提供静态/半静态 descriptor。
///
/// 内置 connector 目前直接返回静态结构；未来 plugin connector 也应该走同一层，
/// 让 UI 和后端管理入口不再到处 match `DriverType`。
pub trait StorageConnectorDescriptorProvider {
    /// 返回当前 connector 的配置字段、能力、上传工作流和可执行动作声明。
    fn storage_connector_descriptor() -> StorageConnectorDescriptor;

    /// 查询 connector 是否声明了某个 UI/服务 affordance。
    ///
    /// Affordance action 是“显示/启用某个系统入口”，例如授权、校验凭据、连接测试。
    fn storage_connector_supports_affordance_action(
        action: StorageConnectorAffordanceAction,
    ) -> bool {
        Self::storage_connector_descriptor()
            .actions
            .iter()
            .any(|descriptor| descriptor.affordance_action == Some(action))
    }

    /// 查询 connector 是否支持某个真正的 provider/policy 动作。
    ///
    /// Policy action 可能会修改远端状态，例如配置 Tencent COS CORS。
    fn storage_connector_supports_policy_action(action: StoragePolicyExecutableAction) -> bool {
        Self::storage_connector_descriptor()
            .actions
            .iter()
            .any(|descriptor| descriptor.policy_action == Some(action))
    }

    fn storage_connector_supports_draft_connection_test() -> bool {
        Self::storage_connector_descriptor()
            .actions
            .iter()
            .any(|descriptor| {
                descriptor.affordance_action
                    == Some(StorageConnectorAffordanceAction::TestDraftConnection)
                    && descriptor.kind == StorageConnectorActionKind::ConnectionTest
            })
    }

    fn storage_connector_supports_saved_connection_test() -> bool {
        Self::storage_connector_descriptor()
            .actions
            .iter()
            .any(|descriptor| {
                descriptor.affordance_action
                    == Some(StorageConnectorAffordanceAction::TestSavedConnection)
                    && descriptor.kind == StorageConnectorActionKind::ConnectionTest
            })
    }
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum StorageConnectorFieldScope {
    /// 写入 `storage_policies` 通用连接字段，例如 endpoint/bucket/base_path。
    Connection,
    /// 写入 `StoragePolicyOptions` 的 driver-specific option。
    PolicyOptions,
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
    /// 是否暴露 S3-compatible upload/download strategy 选项。
    pub s3_transfer_strategy: bool,
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
    /// 同一字段只对部分 driver 可见时使用。为空表示不额外限制。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub visible_when_driver_types: Vec<DriverType>,
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
    /// Candidate driver that should be suggested for matching endpoint hosts.
    pub target_driver_type: DriverType,
    /// Host rules owned by the source connector.
    ///
    /// This keeps provider-detection rules in connector metadata instead of in
    /// the admin UI. Frontend code only performs generic URL host matching.
    pub endpoint_host_rules: Vec<StorageConnectorEndpointHostRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct StorageConnectorDescriptor {
    /// 持久化到 policy 的 driver type。
    pub driver_type: DriverType,
    /// 当前部署是否启用该 connector。
    pub enabled: bool,
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

pub(crate) struct ObjectStorageConnectorDescriptorInput {
    pub(crate) driver_type: DriverType,
    pub(crate) label: &'static str,
    pub(crate) description: &'static str,
    pub(crate) ui: StorageConnectorUiDescriptorInput,
    pub(crate) fields: ObjectStorageFieldDescriptorInput,
    pub(crate) include_s3_path_style: bool,
    pub(crate) presigned_part_etag_required: bool,
    pub(crate) storage_native_processing: bool,
    pub(crate) related_issues: Vec<u16>,
}

pub(crate) struct ObjectStorageFieldDescriptorInput {
    pub(crate) endpoint_placeholder: &'static str,
    pub(crate) endpoint_help_key: &'static str,
    pub(crate) endpoint_protocol_error_key: &'static str,
    pub(crate) bucket_required_message_key: &'static str,
    pub(crate) access_key_label_key: &'static str,
    pub(crate) secret_key_label_key: &'static str,
    pub(crate) access_key_trim_on_blur: bool,
}

pub(crate) fn object_storage_connector_descriptor(
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
            trim_on_blur: false,
            visible_when_driver_types: Vec::new(),
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
            trim_on_blur: false,
            visible_when_driver_types: Vec::new(),
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
            trim_on_blur: input.fields.access_key_trim_on_blur,
            visible_when_driver_types: Vec::new(),
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
            trim_on_blur: false,
            visible_when_driver_types: Vec::new(),
        }),
        storage_connector_field(
            "base_path",
            StorageConnectorFieldScope::Connection,
            StorageConnectorFieldKind::Text,
            false,
            false,
        ),
        storage_connector_field_with_options(
            "s3_upload_strategy",
            StorageConnectorFieldScope::PolicyOptions,
            StorageConnectorFieldKind::Select,
            true,
            false,
            vec!["relay_stream", "presigned"],
        ),
        storage_connector_field_with_options(
            "s3_download_strategy",
            StorageConnectorFieldScope::PolicyOptions,
            StorageConnectorFieldKind::Select,
            true,
            false,
            vec!["relay_stream", "presigned"],
        ),
    ];
    if input.include_s3_path_style {
        fields.push(storage_connector_field_with_display(
            StorageConnectorFieldDisplayInput {
                name: "s3_path_style",
                scope: StorageConnectorFieldScope::PolicyOptions,
                kind: StorageConnectorFieldKind::Boolean,
                required: false,
                secret: false,
                label_key: "s3_path_style",
                placeholder: None,
                help_key: Some("s3_path_style_desc"),
                required_message_key: None,
                invalid_protocol_message_key: None,
                trim_on_blur: false,
                visible_when_driver_types: vec![DriverType::S3],
            },
        ));
    }

    StorageConnectorDescriptor {
        driver_type: input.driver_type,
        enabled: true,
        label: input.label.to_string(),
        description: input.description.to_string(),
        ui: storage_connector_ui_descriptor(input.ui),
        credential_mode: StorageConnectorCredentialMode::StaticSecret,
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
            s3_transfer_strategy: true,
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
        actions: vec![
            draft_connection_test_action_descriptor(),
            saved_connection_test_action_descriptor(false),
        ],
        driver_recommendations: Vec::new(),
        related_issues: input.related_issues,
    }
}

pub(crate) fn server_relay_simple_upload_capabilities(
    max_provider_single_request_size: Option<u64>,
) -> StorageConnectorSimpleUploadCapabilities {
    StorageConnectorSimpleUploadCapabilities {
        server_side_relay: true,
        policy_limited: true,
        max_provider_single_request_size,
    }
}

pub(crate) struct ObjectMultipartUploadCapabilitiesInput {
    pub(crate) presigned_part_etag_required: bool,
}

pub(crate) fn object_multipart_upload_capabilities(
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

pub(crate) fn endpoint_driver_recommendation(
    target_driver_type: DriverType,
    endpoint_host_rules: Vec<StorageConnectorEndpointHostRule>,
) -> StorageConnectorDriverRecommendation {
    StorageConnectorDriverRecommendation {
        target_driver_type,
        endpoint_host_rules,
    }
}

pub(crate) fn endpoint_host_rule(
    equals: Option<&str>,
    ends_with: Option<&str>,
) -> StorageConnectorEndpointHostRule {
    StorageConnectorEndpointHostRule {
        equals: equals.map(ToOwned::to_owned),
        ends_with: ends_with.map(ToOwned::to_owned),
    }
}

pub(crate) fn policy_action_descriptor(
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

pub(crate) fn start_authorization_action_descriptor() -> StorageConnectorActionDescriptor {
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

pub(crate) fn validate_credential_action_descriptor() -> StorageConnectorActionDescriptor {
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

pub(crate) fn draft_connection_test_action_descriptor() -> StorageConnectorActionDescriptor {
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

pub(crate) fn saved_connection_test_action_descriptor(
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

pub(crate) fn storage_connector_field(
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
        trim_on_blur: false,
        visible_when_driver_types: Vec::new(),
    })
}

pub(crate) struct StorageConnectorFieldDisplayInput<'a> {
    pub(crate) name: &'a str,
    pub(crate) scope: StorageConnectorFieldScope,
    pub(crate) kind: StorageConnectorFieldKind,
    pub(crate) required: bool,
    pub(crate) secret: bool,
    pub(crate) label_key: &'a str,
    pub(crate) placeholder: Option<&'a str>,
    pub(crate) help_key: Option<&'a str>,
    pub(crate) required_message_key: Option<&'a str>,
    pub(crate) invalid_protocol_message_key: Option<&'a str>,
    pub(crate) trim_on_blur: bool,
    pub(crate) visible_when_driver_types: Vec<DriverType>,
}

pub(crate) fn storage_connector_field_with_display(
    input: StorageConnectorFieldDisplayInput<'_>,
) -> StorageConnectorFieldDescriptor {
    StorageConnectorFieldDescriptor {
        name: input.name.to_string(),
        scope: input.scope,
        kind: input.kind,
        label_key: input.label_key.to_string(),
        placeholder: input.placeholder.map(ToOwned::to_owned),
        help_key: input.help_key.map(ToOwned::to_owned),
        required_message_key: input.required_message_key.map(ToOwned::to_owned),
        invalid_protocol_message_key: input.invalid_protocol_message_key.map(ToOwned::to_owned),
        trim_on_blur: input.trim_on_blur,
        required: input.required,
        secret: input.secret,
        options: Vec::new(),
        visible_when_driver_types: input.visible_when_driver_types,
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

pub(crate) struct StorageConnectorUiDescriptorInput {
    pub(crate) label_key: &'static str,
    pub(crate) description_key: &'static str,
    pub(crate) icon_src: Option<&'static str>,
    pub(crate) icon_name: Option<&'static str>,
    pub(crate) helper_key: &'static str,
    pub(crate) config_step_title_key: &'static str,
    pub(crate) config_step_description_key: &'static str,
    pub(crate) edit_context_key: &'static str,
    pub(crate) base_path_empty_display: &'static str,
    pub(crate) base_path_placeholder: &'static str,
}

pub(crate) fn storage_connector_ui_descriptor(
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
