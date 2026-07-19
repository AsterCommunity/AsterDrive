//! Storage connector definitions for policy configuration and admin actions.
//!
//! Connectors own configuration-time behavior: descriptors, connection field
//! normalization, credential requirements, draft/saved connection tests, and
//! connector-specific admin actions. Runtime object operations remain in
//! `StorageDriver` implementations.
//!
//! 简单说：`StorageConnector` 管“怎么把 policy 配好并告诉管理端这个 driver 能做什么”，
//! `StorageDriver` 管“policy 已经配好后怎么读写对象”。如果一段逻辑需要数据库、
//! OAuth、表单字段、连接测试或策略动作，它通常属于 connector，而不是 driver。

mod azure_blob;
mod common;
mod local;
mod models;
mod onedrive;
mod remote;
mod s3;
mod sftp;
mod tencent_cos;
mod upload;

#[cfg(test)]
mod tests;

use async_trait::async_trait;
use sea_orm::ConnectionTrait;
use std::sync::Arc;

use crate::entities::storage_policy;
use crate::errors::Result;
use crate::runtime::{RemoteProtocolRuntimeState, SharedRuntimeState};
use crate::storage::StorageDriver;
use crate::storage::connector_descriptor::{
    StorageConnectorActionKind, StorageConnectorAffordanceAction, StorageConnectorDescriptor,
    StorageConnectorDescriptorProvider, StorageConnectorObjectNamingMode,
    StoragePolicyExecutableAction,
};
use crate::storage::drivers::{
    azure_blob::AzureBlobDriver, local::LocalDriver, s3::S3Driver, sftp::SftpDriver,
    tencent_cos::TencentCosDriver,
};
use crate::types::{DriverType, StorageCredentialKind, StorageCredentialProvider};

use azure_blob::AzureBlobConnector;
pub use common::unsupported_multipart_error;
use local::LocalConnector;
pub use models::{
    ExecuteDraftStorageConnectorActionInput, ExecuteSavedStorageConnectorActionInput,
    MicrosoftGraphApplicationConfigInput, StorageConnectorActionResult,
    StorageConnectorApplicationConfigInput, StorageConnectorConnectionInput,
    TencentCosCorsConfigResult, TestDraftStorageConnectorConnectionInput,
};
pub(crate) use models::{
    StorageConnectorCredentialRequirement, StorageConnectorRuntimeCredential,
    StorageCredentialValidationOutcome, StoragePolicyCleanupDriverSnapshot,
    StoragePolicyCleanupOneDriveCredentialSnapshot, StoragePolicyCleanupRemoteNodeSnapshot,
    StoragePolicyCleanupSnapshots,
};
use onedrive::OneDriveConnector;
use remote::RemoteConnector;
use s3::S3Connector;
use sftp::SftpConnector;
use tencent_cos::TencentCosConnector;
pub use upload::{StorageConnectorChunkedCompletion, StorageConnectorUploadTransport};

#[async_trait(?Send)]
trait StorageConnector: StorageConnectorDescriptorProvider + Send + Sync + Sized {
    /// 当前 connector 对应的持久化 driver type。
    fn driver_type() -> DriverType;

    /// 规范化连接字段，例如 endpoint/bucket/container 的格式。
    ///
    /// 这里处理的是 policy 存储前的配置形状，不应该做远端写入探测。
    fn normalize_connection_fields(endpoint: &str, bucket: &str) -> Result<(String, String)>;

    /// 校验连接凭据字段是否满足当前 connector 的最低要求。
    ///
    /// 例如静态密钥型 connector 要求 access key / secret key 非空；OAuth delegated
    /// 型 connector 可以把 app credential 放到 `application_config`，不一定使用这俩字段。
    fn validate_connection_credentials(input: &StorageConnectorConnectionInput) -> Result<()>;

    /// draft 连接测试时，是否允许在 access/secret 留空时回填已保存 policy 的静态凭据。
    fn supports_saved_draft_credentials() -> bool {
        false
    }

    /// 将管理端提交的连接字段转换成 `storage_policies` 表里的通用存储字段。
    ///
    /// 默认实现拒绝 application credential，因为大多数 driver 没有 provider app config。
    /// OneDrive 等 connector 可以覆盖这里，把 client_id/client_secret 从 legacy
    /// access_key/secret_key 中移走，改由专门配置表保存。
    fn prepare_connection_for_storage(
        input: StorageConnectorConnectionInput,
        application_config: &StorageConnectorApplicationConfigInput,
    ) -> Result<StorageConnectorConnectionInput> {
        if !application_config.is_empty() {
            return Err(crate::errors::AsterError::validation_error(format!(
                "application credential config is not valid for {} storage policies",
                Self::driver_type().as_str()
            )));
        }
        Ok(input)
    }

    /// 校验并解析外部绑定关系，例如 remote node 绑定。
    ///
    /// 返回值是规范化后的 remote_node_id。普通本地/object storage connector 默认拒绝绑定。
    async fn validate_connection_binding<C: ConnectionTrait + Sync>(
        _db: &C,
        input: &StorageConnectorConnectionInput,
    ) -> Result<Option<i64>> {
        common::reject_unexpected_remote_storage_target_key(
            input.remote_storage_target_key.as_deref(),
        )?;
        common::reject_unexpected_remote_node(input.remote_node_id)
    }

    /// 校验 driver-specific policy options。
    ///
    /// 这里是 connector 阻止无效 option 组合的地方，例如非 OneDrive driver 不允许
    /// OneDrive options，未声明原生缩略图能力的 driver 不允许开启 native thumbnail。
    async fn validate_policy_options<C: ConnectionTrait + Sync>(
        db: &C,
        remote_node_id: Option<i64>,
        options: &crate::types::StoragePolicyOptions,
    ) -> Result<()> {
        let _ = (db, remote_node_id);
        common::ensure_storage_native_processing_supported(
            Self::storage_connector_descriptor(),
            options,
        )?;
        common::ensure_onedrive_options_absent(options)?;
        common::ensure_sftp_options_absent(options)
    }

    /// 持久化 provider application credential/config。
    ///
    /// 这和 delegated OAuth token 不是一回事：app config 是 client_id/client_secret/
    /// tenant/scopes 这类连接器应用配置；OAuth token 仍由 credential service 管。
    async fn persist_application_config<C: ConnectionTrait + Sync>(
        db: &C,
        encryption_key: &str,
        policy_id: i64,
        options: &crate::types::StoragePolicyOptions,
        application_config: StorageConnectorApplicationConfigInput,
    ) -> Result<()> {
        let _ = (db, encryption_key, policy_id, options);
        if !application_config.is_empty() {
            return Err(crate::errors::AsterError::validation_error(format!(
                "application credential config is not valid for {} storage policies",
                Self::driver_type().as_str()
            )));
        }
        Ok(())
    }

    /// 用未保存或临时拼出的 policy 构建 driver，供 draft connection test 使用。
    ///
    /// 需要已保存 policy ID、OAuth 授权或 credential row 的 connector 应拒绝 draft test，
    /// 只暴露 saved connection test。
    async fn build_draft_driver<S: RemoteProtocolRuntimeState + Sync + ?Sized>(
        state: &S,
        policy: &storage_policy::Model,
    ) -> Result<Box<dyn StorageDriver>>;

    /// 返回上传服务应该使用的上传传输模型。
    ///
    /// 这是运行时上传调度的入口，不要让 upload service 直接 match `DriverType`。
    fn upload_transport(policy: &storage_policy::Model) -> StorageConnectorUploadTransport;

    /// 声明 connector 在构建 runtime driver 前需要的授权凭据。
    ///
    /// 静态密钥型 connector 不需要 delegated credential，默认返回 `None`。
    /// OAuth 型 connector 应在这里声明 provider/kind，让 registry 只做通用分发。
    fn runtime_credential_requirement() -> Option<StorageConnectorCredentialRequirement> {
        None
    }

    /// 将持久化 credential/config 组装成 connector runtime material。
    ///
    /// 默认 connector 不需要 runtime credential。需要 OAuth token provider、root id
    /// 等 provider-specific 状态的 connector 覆盖这里，避免 registry 直接理解这些表。
    async fn load_runtime_credential(
        db: &sea_orm::DatabaseConnection,
        config: &crate::config::Config,
        policy: &storage_policy::Model,
        credential: &crate::entities::storage_policy_credential::Model,
    ) -> Result<Option<StorageConnectorRuntimeCredential>> {
        let _ = (db, config, policy, credential);
        Ok(None)
    }

    /// 使用 connector-owned runtime credential 构建需要授权的 driver。
    ///
    /// 普通 connector 的 driver 构建仍由 registry 完成；OAuth-backed connector 覆盖
    /// 这里，把 token provider / resolved root 等 provider-specific material 消化掉。
    fn build_authorized_driver(
        policy: &storage_policy::Model,
        credential: StorageConnectorRuntimeCredential,
    ) -> Result<Arc<dyn StorageDriver>> {
        let _ = (policy, credential);
        Err(crate::storage::error::storage_driver_error(
            crate::storage::StorageErrorKind::Unsupported,
            format!(
                "{} storage policies do not use runtime credential driver construction",
                Self::driver_type().as_str()
            ),
        ))
    }

    /// 执行 connector-specific credential validation。
    ///
    /// Credential service 负责查找 credential、写状态和刷新 registry；connector 负责
    /// provider client、root/drive 解析和 metadata 这类 provider-specific 语义。
    async fn validate_credential(
        db: &sea_orm::DatabaseConnection,
        config: &crate::config::Config,
        policy: &storage_policy::Model,
        credential: &crate::entities::storage_policy_credential::Model,
    ) -> Result<StorageCredentialValidationOutcome> {
        let _ = (db, config, policy, credential);
        Err(crate::errors::AsterError::unsupported_driver(format!(
            "credential validation is not implemented for {} storage policies",
            Self::driver_type().as_str()
        )))
    }

    /// 当前 policy 是否启用 presigned download。
    ///
    /// descriptor 只说明 connector 是否具备该能力；这里读取具体 policy option。
    fn presigned_download_enabled(policy: &storage_policy::Model) -> bool {
        let _ = policy;
        false
    }

    /// Whether provider-native downloads must match AsterDrive's current filename.
    fn presigned_download_requires_filename_match(policy: &storage_policy::Model) -> bool {
        let _ = policy;
        false
    }

    /// 对未保存参数执行连接测试。默认实现会先通过 descriptor 检查该 connector 是否支持。
    async fn test_draft_connection<S: RemoteProtocolRuntimeState + Sync + ?Sized>(
        state: &S,
        input: TestDraftStorageConnectorConnectionInput,
    ) -> Result<()> {
        if !Self::storage_connector_supports_draft_connection_test() {
            return Err(common::unsupported_draft_connection_test_error(
                Self::storage_connector_descriptor(),
            ));
        }
        let connection = if Self::supports_saved_draft_credentials() {
            common::merge_saved_static_credentials_for_draft(
                state.writer_db(),
                input.policy_id,
                input.connection,
                "draft storage policy connection test",
            )
            .await?
        } else {
            input.connection
        };
        let policy =
            common::build_connection_test_policy::<Self, _>(state.writer_db(), connection).await?;
        let driver = Self::build_draft_driver(state, &policy).await?;
        common::probe_storage_driver(driver.as_ref(), "connection test failed").await
    }

    /// 对已保存 policy 执行连接测试。适合需要 policy ID / OAuth credential 的 connector。
    async fn test_saved_connection<S: SharedRuntimeState + Sync + ?Sized>(
        state: &S,
        policy: &storage_policy::Model,
    ) -> Result<()> {
        if !Self::storage_connector_supports_saved_connection_test() {
            return Err(common::unsupported_saved_connection_test_error(
                Self::storage_connector_descriptor(),
            ));
        }
        let driver = state.driver_registry().get_driver(policy)?;
        common::probe_storage_driver(driver.as_ref(), "write test failed").await
    }

    /// 执行已保存 policy 上的 connector-specific 管理动作。
    ///
    /// 例如 Tencent COS CORS 配置。普通 driver 不需要覆盖，默认返回 unsupported。
    async fn execute_saved_action<S: SharedRuntimeState + Sync + ?Sized>(
        state: &S,
        policy: &storage_policy::Model,
        action: StoragePolicyExecutableAction,
    ) -> Result<StorageConnectorActionResult> {
        let _ = (state, policy);
        Err(common::unsupported_policy_action_error(
            Self::storage_connector_descriptor(),
            action,
        ))
    }

    /// 执行 draft 参数上的 connector-specific 管理动作。
    ///
    /// 只有不依赖 saved policy / credential row 的动作才应该支持 draft 执行。
    async fn execute_draft_action<S: RemoteProtocolRuntimeState + Sync + ?Sized>(
        state: &S,
        input: ExecuteDraftStorageConnectorActionInput,
    ) -> Result<StorageConnectorActionResult> {
        let _ = state;
        Err(common::unsupported_policy_action_error(
            Self::storage_connector_descriptor(),
            input.action,
        ))
    }
}

/// Static built-in connector registry.
///
/// Keep configuration-time behavior here instead of on `StorageDriver`: drivers
/// are already-built object operators, while connectors know how to validate,
/// authorize, snapshot, and rebuild those drivers for admin/task workflows.
/// Issue #212 can extend this registry with plugin-provided registrations
/// without adding new `match DriverType` dispatch sites.
struct StorageConnectorRegistration {
    driver_type: DriverType,
    connector: BuiltinStorageConnector,
    cleanup_snapshot_required: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BuiltinStorageConnector {
    Local,
    S3,
    Sftp,
    AzureBlob,
    TencentCos,
    Remote,
    OneDrive,
}

impl BuiltinStorageConnector {
    fn descriptor(self) -> StorageConnectorDescriptor {
        match self {
            Self::Local => LocalConnector::storage_connector_descriptor(),
            Self::S3 => S3Connector::storage_connector_descriptor(),
            Self::Sftp => SftpConnector::storage_connector_descriptor(),
            Self::AzureBlob => AzureBlobConnector::storage_connector_descriptor(),
            Self::TencentCos => TencentCosConnector::storage_connector_descriptor(),
            Self::Remote => RemoteConnector::storage_connector_descriptor(),
            Self::OneDrive => OneDriveConnector::storage_connector_descriptor(),
        }
    }

    async fn normalize_policy_connection<C: ConnectionTrait + Sync>(
        self,
        db: &C,
        input: StorageConnectorConnectionInput,
    ) -> Result<StorageConnectorConnectionInput> {
        match self {
            Self::Local => {
                common::normalize_policy_connection_for::<LocalConnector, _>(db, input).await
            }
            Self::S3 => common::normalize_policy_connection_for::<S3Connector, _>(db, input).await,
            Self::Sftp => {
                common::normalize_policy_connection_for::<SftpConnector, _>(db, input).await
            }
            Self::AzureBlob => {
                common::normalize_policy_connection_for::<AzureBlobConnector, _>(db, input).await
            }
            Self::TencentCos => {
                common::normalize_policy_connection_for::<TencentCosConnector, _>(db, input).await
            }
            Self::Remote => {
                common::normalize_policy_connection_for::<RemoteConnector, _>(db, input).await
            }
            Self::OneDrive => {
                common::normalize_policy_connection_for::<OneDriveConnector, _>(db, input).await
            }
        }
    }

    fn prepare_connection_for_storage(
        self,
        input: StorageConnectorConnectionInput,
        application_config: &StorageConnectorApplicationConfigInput,
    ) -> Result<StorageConnectorConnectionInput> {
        match self {
            Self::Local => {
                LocalConnector::prepare_connection_for_storage(input, application_config)
            }
            Self::S3 => S3Connector::prepare_connection_for_storage(input, application_config),
            Self::Sftp => SftpConnector::prepare_connection_for_storage(input, application_config),
            Self::AzureBlob => {
                AzureBlobConnector::prepare_connection_for_storage(input, application_config)
            }
            Self::TencentCos => {
                TencentCosConnector::prepare_connection_for_storage(input, application_config)
            }
            Self::Remote => {
                RemoteConnector::prepare_connection_for_storage(input, application_config)
            }
            Self::OneDrive => {
                OneDriveConnector::prepare_connection_for_storage(input, application_config)
            }
        }
    }

    async fn validate_policy_options<C: ConnectionTrait + Sync>(
        self,
        db: &C,
        remote_node_id: Option<i64>,
        options: &crate::types::StoragePolicyOptions,
    ) -> Result<()> {
        match self {
            Self::Local => {
                LocalConnector::validate_policy_options(db, remote_node_id, options).await
            }
            Self::S3 => S3Connector::validate_policy_options(db, remote_node_id, options).await,
            Self::Sftp => SftpConnector::validate_policy_options(db, remote_node_id, options).await,
            Self::AzureBlob => {
                AzureBlobConnector::validate_policy_options(db, remote_node_id, options).await
            }
            Self::TencentCos => {
                TencentCosConnector::validate_policy_options(db, remote_node_id, options).await
            }
            Self::Remote => {
                RemoteConnector::validate_policy_options(db, remote_node_id, options).await
            }
            Self::OneDrive => {
                OneDriveConnector::validate_policy_options(db, remote_node_id, options).await
            }
        }
    }

    async fn persist_application_config<C: ConnectionTrait + Sync>(
        self,
        db: &C,
        encryption_key: &str,
        policy_id: i64,
        options: &crate::types::StoragePolicyOptions,
        application_config: StorageConnectorApplicationConfigInput,
    ) -> Result<()> {
        match self {
            Self::Local => {
                LocalConnector::persist_application_config(
                    db,
                    encryption_key,
                    policy_id,
                    options,
                    application_config,
                )
                .await
            }
            Self::S3 => {
                S3Connector::persist_application_config(
                    db,
                    encryption_key,
                    policy_id,
                    options,
                    application_config,
                )
                .await
            }
            Self::Sftp => {
                SftpConnector::persist_application_config(
                    db,
                    encryption_key,
                    policy_id,
                    options,
                    application_config,
                )
                .await
            }
            Self::AzureBlob => {
                AzureBlobConnector::persist_application_config(
                    db,
                    encryption_key,
                    policy_id,
                    options,
                    application_config,
                )
                .await
            }
            Self::TencentCos => {
                TencentCosConnector::persist_application_config(
                    db,
                    encryption_key,
                    policy_id,
                    options,
                    application_config,
                )
                .await
            }
            Self::Remote => {
                RemoteConnector::persist_application_config(
                    db,
                    encryption_key,
                    policy_id,
                    options,
                    application_config,
                )
                .await
            }
            Self::OneDrive => {
                OneDriveConnector::persist_application_config(
                    db,
                    encryption_key,
                    policy_id,
                    options,
                    application_config,
                )
                .await
            }
        }
    }

    async fn test_draft_connection<S: RemoteProtocolRuntimeState + Sync + ?Sized>(
        self,
        state: &S,
        input: TestDraftStorageConnectorConnectionInput,
    ) -> Result<()> {
        match self {
            Self::Local => LocalConnector::test_draft_connection(state, input).await,
            Self::S3 => S3Connector::test_draft_connection(state, input).await,
            Self::Sftp => SftpConnector::test_draft_connection(state, input).await,
            Self::AzureBlob => AzureBlobConnector::test_draft_connection(state, input).await,
            Self::TencentCos => TencentCosConnector::test_draft_connection(state, input).await,
            Self::Remote => RemoteConnector::test_draft_connection(state, input).await,
            Self::OneDrive => OneDriveConnector::test_draft_connection(state, input).await,
        }
    }

    async fn test_saved_connection<S: SharedRuntimeState + Sync + ?Sized>(
        self,
        state: &S,
        policy: &storage_policy::Model,
    ) -> Result<()> {
        match self {
            Self::Local => LocalConnector::test_saved_connection(state, policy).await,
            Self::S3 => S3Connector::test_saved_connection(state, policy).await,
            Self::Sftp => SftpConnector::test_saved_connection(state, policy).await,
            Self::AzureBlob => AzureBlobConnector::test_saved_connection(state, policy).await,
            Self::TencentCos => TencentCosConnector::test_saved_connection(state, policy).await,
            Self::Remote => RemoteConnector::test_saved_connection(state, policy).await,
            Self::OneDrive => OneDriveConnector::test_saved_connection(state, policy).await,
        }
    }

    async fn execute_saved_action<S: SharedRuntimeState + Sync + ?Sized>(
        self,
        state: &S,
        policy: &storage_policy::Model,
        action: StoragePolicyExecutableAction,
    ) -> Result<StorageConnectorActionResult> {
        match self {
            Self::Local => LocalConnector::execute_saved_action(state, policy, action).await,
            Self::S3 => S3Connector::execute_saved_action(state, policy, action).await,
            Self::Sftp => SftpConnector::execute_saved_action(state, policy, action).await,
            Self::AzureBlob => {
                AzureBlobConnector::execute_saved_action(state, policy, action).await
            }
            Self::TencentCos => {
                TencentCosConnector::execute_saved_action(state, policy, action).await
            }
            Self::Remote => RemoteConnector::execute_saved_action(state, policy, action).await,
            Self::OneDrive => OneDriveConnector::execute_saved_action(state, policy, action).await,
        }
    }

    async fn execute_draft_action<S: RemoteProtocolRuntimeState + Sync + ?Sized>(
        self,
        state: &S,
        input: ExecuteDraftStorageConnectorActionInput,
    ) -> Result<StorageConnectorActionResult> {
        match self {
            Self::Local => LocalConnector::execute_draft_action(state, input).await,
            Self::S3 => S3Connector::execute_draft_action(state, input).await,
            Self::Sftp => SftpConnector::execute_draft_action(state, input).await,
            Self::AzureBlob => AzureBlobConnector::execute_draft_action(state, input).await,
            Self::TencentCos => TencentCosConnector::execute_draft_action(state, input).await,
            Self::Remote => RemoteConnector::execute_draft_action(state, input).await,
            Self::OneDrive => OneDriveConnector::execute_draft_action(state, input).await,
        }
    }

    fn upload_transport(self, policy: &storage_policy::Model) -> StorageConnectorUploadTransport {
        match self {
            Self::Local => LocalConnector::upload_transport(policy),
            Self::S3 => S3Connector::upload_transport(policy),
            Self::Sftp => SftpConnector::upload_transport(policy),
            Self::AzureBlob => AzureBlobConnector::upload_transport(policy),
            Self::TencentCos => TencentCosConnector::upload_transport(policy),
            Self::Remote => RemoteConnector::upload_transport(policy),
            Self::OneDrive => OneDriveConnector::upload_transport(policy),
        }
    }

    fn presigned_download_enabled(self, policy: &storage_policy::Model) -> bool {
        match self {
            Self::Local => LocalConnector::presigned_download_enabled(policy),
            Self::S3 => S3Connector::presigned_download_enabled(policy),
            Self::Sftp => SftpConnector::presigned_download_enabled(policy),
            Self::AzureBlob => AzureBlobConnector::presigned_download_enabled(policy),
            Self::TencentCos => TencentCosConnector::presigned_download_enabled(policy),
            Self::Remote => RemoteConnector::presigned_download_enabled(policy),
            Self::OneDrive => OneDriveConnector::presigned_download_enabled(policy),
        }
    }

    fn presigned_download_requires_filename_match(self, policy: &storage_policy::Model) -> bool {
        match self {
            Self::Local => LocalConnector::presigned_download_requires_filename_match(policy),
            Self::S3 => S3Connector::presigned_download_requires_filename_match(policy),
            Self::Sftp => SftpConnector::presigned_download_requires_filename_match(policy),
            Self::AzureBlob => {
                AzureBlobConnector::presigned_download_requires_filename_match(policy)
            }
            Self::TencentCos => {
                TencentCosConnector::presigned_download_requires_filename_match(policy)
            }
            Self::Remote => RemoteConnector::presigned_download_requires_filename_match(policy),
            Self::OneDrive => OneDriveConnector::presigned_download_requires_filename_match(policy),
        }
    }

    fn runtime_credential_requirement(self) -> Option<StorageConnectorCredentialRequirement> {
        match self {
            Self::Local => LocalConnector::runtime_credential_requirement(),
            Self::S3 => S3Connector::runtime_credential_requirement(),
            Self::Sftp => SftpConnector::runtime_credential_requirement(),
            Self::AzureBlob => AzureBlobConnector::runtime_credential_requirement(),
            Self::TencentCos => TencentCosConnector::runtime_credential_requirement(),
            Self::Remote => RemoteConnector::runtime_credential_requirement(),
            Self::OneDrive => OneDriveConnector::runtime_credential_requirement(),
        }
    }

    async fn load_runtime_credential(
        self,
        db: &sea_orm::DatabaseConnection,
        config: &crate::config::Config,
        policy: &storage_policy::Model,
        credential: &crate::entities::storage_policy_credential::Model,
    ) -> Result<Option<StorageConnectorRuntimeCredential>> {
        match self {
            Self::Local => {
                LocalConnector::load_runtime_credential(db, config, policy, credential).await
            }
            Self::S3 => S3Connector::load_runtime_credential(db, config, policy, credential).await,
            Self::Sftp => {
                SftpConnector::load_runtime_credential(db, config, policy, credential).await
            }
            Self::AzureBlob => {
                AzureBlobConnector::load_runtime_credential(db, config, policy, credential).await
            }
            Self::TencentCos => {
                TencentCosConnector::load_runtime_credential(db, config, policy, credential).await
            }
            Self::Remote => {
                RemoteConnector::load_runtime_credential(db, config, policy, credential).await
            }
            Self::OneDrive => {
                OneDriveConnector::load_runtime_credential(db, config, policy, credential).await
            }
        }
    }

    fn build_authorized_driver(
        self,
        policy: &storage_policy::Model,
        credential: StorageConnectorRuntimeCredential,
    ) -> Result<Arc<dyn StorageDriver>> {
        match self {
            Self::Local => LocalConnector::build_authorized_driver(policy, credential),
            Self::S3 => S3Connector::build_authorized_driver(policy, credential),
            Self::Sftp => SftpConnector::build_authorized_driver(policy, credential),
            Self::AzureBlob => AzureBlobConnector::build_authorized_driver(policy, credential),
            Self::TencentCos => TencentCosConnector::build_authorized_driver(policy, credential),
            Self::Remote => RemoteConnector::build_authorized_driver(policy, credential),
            Self::OneDrive => OneDriveConnector::build_authorized_driver(policy, credential),
        }
    }

    async fn validate_credential(
        self,
        db: &sea_orm::DatabaseConnection,
        config: &crate::config::Config,
        policy: &storage_policy::Model,
        credential: &crate::entities::storage_policy_credential::Model,
    ) -> Result<StorageCredentialValidationOutcome> {
        match self {
            Self::Local => {
                LocalConnector::validate_credential(db, config, policy, credential).await
            }
            Self::S3 => S3Connector::validate_credential(db, config, policy, credential).await,
            Self::Sftp => SftpConnector::validate_credential(db, config, policy, credential).await,
            Self::AzureBlob => {
                AzureBlobConnector::validate_credential(db, config, policy, credential).await
            }
            Self::TencentCos => {
                TencentCosConnector::validate_credential(db, config, policy, credential).await
            }
            Self::Remote => {
                RemoteConnector::validate_credential(db, config, policy, credential).await
            }
            Self::OneDrive => {
                OneDriveConnector::validate_credential(db, config, policy, credential).await
            }
        }
    }

    async fn cleanup_snapshot_for_policy<S: SharedRuntimeState + Sync + ?Sized>(
        self,
        state: &S,
        policy: &storage_policy::Model,
    ) -> Result<Option<StoragePolicyCleanupDriverSnapshot>> {
        match self {
            Self::Remote => RemoteConnector::cleanup_snapshot_for_policy(state, policy).await,
            Self::OneDrive => OneDriveConnector::cleanup_snapshot_for_policy(state, policy).await,
            Self::Local | Self::S3 | Self::Sftp | Self::AzureBlob | Self::TencentCos => Ok(None),
        }
    }

    async fn build_cleanup_driver<S: RemoteProtocolRuntimeState + Sync + ?Sized>(
        self,
        state: &S,
        policy: &storage_policy::Model,
        snapshots: StoragePolicyCleanupSnapshots<'_>,
    ) -> Result<Arc<dyn StorageDriver>> {
        match self {
            Self::Local => Ok(Arc::new(LocalDriver::new(policy)?)),
            Self::S3 => Ok(Arc::new(S3Driver::new(policy)?)),
            Self::Sftp => Ok(Arc::new(SftpDriver::new(policy)?)),
            Self::AzureBlob => Ok(Arc::new(AzureBlobDriver::new(policy)?)),
            Self::TencentCos => Ok(Arc::new(TencentCosDriver::new(policy)?)),
            Self::Remote => RemoteConnector::build_cleanup_driver(state, policy, snapshots).await,
            Self::OneDrive => {
                OneDriveConnector::build_cleanup_driver(state, policy, snapshots).await
            }
        }
    }

    fn validate_promotion_candidate(self, policy: &storage_policy::Model) -> Result<()> {
        match self {
            Self::TencentCos => TencentCosConnector::validate_promotion_candidate(policy),
            _ => Err(crate::errors::validation_error_with_code(
                crate::api::api_error_code::ApiErrorCode::PolicyPromotionTargetUnsupported,
                format!(
                    "promoting S3-compatible policy to '{}' is not supported",
                    policy.driver_type.as_str()
                ),
            )),
        }
    }
}

static CONNECTOR_REGISTRATIONS: &[StorageConnectorRegistration] = &[
    StorageConnectorRegistration {
        driver_type: DriverType::Local,
        connector: BuiltinStorageConnector::Local,
        cleanup_snapshot_required: false,
    },
    StorageConnectorRegistration {
        driver_type: DriverType::S3,
        connector: BuiltinStorageConnector::S3,
        cleanup_snapshot_required: false,
    },
    StorageConnectorRegistration {
        driver_type: DriverType::Sftp,
        connector: BuiltinStorageConnector::Sftp,
        cleanup_snapshot_required: false,
    },
    StorageConnectorRegistration {
        driver_type: DriverType::AzureBlob,
        connector: BuiltinStorageConnector::AzureBlob,
        cleanup_snapshot_required: false,
    },
    StorageConnectorRegistration {
        driver_type: DriverType::TencentCos,
        connector: BuiltinStorageConnector::TencentCos,
        cleanup_snapshot_required: false,
    },
    StorageConnectorRegistration {
        driver_type: DriverType::Remote,
        connector: BuiltinStorageConnector::Remote,
        cleanup_snapshot_required: true,
    },
    StorageConnectorRegistration {
        driver_type: DriverType::OneDrive,
        connector: BuiltinStorageConnector::OneDrive,
        cleanup_snapshot_required: true,
    },
];

fn connector_for(driver_type: DriverType) -> Result<&'static StorageConnectorRegistration> {
    CONNECTOR_REGISTRATIONS
        .iter()
        .find(|connector| connector.driver_type == driver_type)
        .ok_or_else(|| {
            crate::errors::AsterError::internal_error(format!(
                "storage connector '{}' is not registered",
                driver_type.as_str()
            ))
        })
}

pub fn list_storage_driver_descriptors() -> Vec<StorageConnectorDescriptor> {
    CONNECTOR_REGISTRATIONS
        .iter()
        .map(|connector| connector.connector.descriptor())
        .collect()
}

pub fn storage_driver_descriptor(driver_type: DriverType) -> Result<StorageConnectorDescriptor> {
    Ok(connector_for(driver_type)?.connector.descriptor())
}

pub fn storage_connector_supports_native_thumbnail(driver_type: DriverType) -> Result<bool> {
    Ok(storage_driver_descriptor(driver_type)?
        .capabilities
        .storage_native_thumbnail)
}

pub fn storage_connector_supports_native_media_metadata(driver_type: DriverType) -> Result<bool> {
    Ok(storage_driver_descriptor(driver_type)?
        .capabilities
        .storage_native_media_metadata)
}

pub fn storage_authorization_provider(
    driver_type: DriverType,
) -> Result<Option<StorageCredentialProvider>> {
    Ok(storage_driver_descriptor(driver_type)?
        .authorization_provider
        .as_deref()
        .and_then(|provider| provider.parse().ok()))
}

pub fn ensure_storage_authorization_supported(
    driver_type: DriverType,
    provider: StorageCredentialProvider,
) -> Result<StorageCredentialKind> {
    let descriptor = storage_driver_descriptor(driver_type)?;
    let starts_authorization = descriptor.actions.iter().any(|action| {
        action.affordance_action == Some(StorageConnectorAffordanceAction::StartAuthorization)
            && action.kind == StorageConnectorActionKind::Authorization
    });
    let supported_provider = descriptor
        .authorization_provider
        .as_deref()
        .and_then(|provider| provider.parse().ok());
    if starts_authorization && supported_provider == Some(provider) {
        return Ok(StorageCredentialKind::OauthDelegated);
    }
    Err(crate::errors::AsterError::unsupported_driver(format!(
        "storage credential authorization provider '{}' is not supported for {} storage policies",
        provider.as_str(),
        driver_type.as_str()
    )))
}

/// Gate credential validation through connector-declared actions so credential
/// services never need to know which storage drivers expose validation.
pub fn ensure_storage_credential_validation_supported(
    driver_type: DriverType,
    provider: StorageCredentialProvider,
) -> Result<StorageCredentialKind> {
    let descriptor = storage_driver_descriptor(driver_type)?;
    let validates_credential = descriptor.actions.iter().any(|action| {
        action.affordance_action == Some(StorageConnectorAffordanceAction::ValidateCredential)
            && action.kind == StorageConnectorActionKind::CredentialValidation
    });
    let supported_provider = descriptor
        .authorization_provider
        .as_deref()
        .and_then(|provider| provider.parse().ok());
    if validates_credential && supported_provider == Some(provider) {
        return Ok(StorageCredentialKind::OauthDelegated);
    }
    Err(crate::errors::AsterError::unsupported_driver(format!(
        "storage credential validation provider '{}' is not supported for {} storage policies",
        provider.as_str(),
        driver_type.as_str()
    )))
}

pub async fn normalize_policy_connection<C: ConnectionTrait + Sync>(
    db: &C,
    input: StorageConnectorConnectionInput,
) -> Result<StorageConnectorConnectionInput> {
    let connector = connector_for(input.driver_type)?;
    connector
        .connector
        .normalize_policy_connection(db, input)
        .await
}

pub fn prepare_connection_for_storage(
    input: StorageConnectorConnectionInput,
    application_config: &StorageConnectorApplicationConfigInput,
) -> Result<StorageConnectorConnectionInput> {
    connector_for(input.driver_type)?
        .connector
        .prepare_connection_for_storage(input, application_config)
}

pub async fn validate_policy_options<C: ConnectionTrait + Sync>(
    db: &C,
    driver_type: DriverType,
    remote_node_id: Option<i64>,
    options: &crate::types::StoragePolicyOptions,
) -> Result<()> {
    connector_for(driver_type)?
        .connector
        .validate_policy_options(db, remote_node_id, options)
        .await
}

pub async fn persist_application_config<C: ConnectionTrait + Sync>(
    db: &C,
    driver_type: DriverType,
    encryption_key: &str,
    policy_id: i64,
    options: &crate::types::StoragePolicyOptions,
    application_config: StorageConnectorApplicationConfigInput,
) -> Result<()> {
    connector_for(driver_type)?
        .connector
        .persist_application_config(db, encryption_key, policy_id, options, application_config)
        .await
}

pub async fn test_draft_connection<S: RemoteProtocolRuntimeState + Sync>(
    state: &S,
    input: TestDraftStorageConnectorConnectionInput,
) -> Result<()> {
    let connector = connector_for(input.connection.driver_type)?;
    connector
        .connector
        .test_draft_connection(state, input)
        .await
}

pub async fn test_saved_connection<S: SharedRuntimeState + Sync>(
    state: &S,
    policy: &storage_policy::Model,
) -> Result<()> {
    connector_for(policy.driver_type)?
        .connector
        .test_saved_connection(state, policy)
        .await
}

pub async fn execute_saved_action<S: SharedRuntimeState + Sync>(
    state: &S,
    policy: &storage_policy::Model,
    action: StoragePolicyExecutableAction,
) -> Result<StorageConnectorActionResult> {
    connector_for(policy.driver_type)?
        .connector
        .execute_saved_action(state, policy, action)
        .await
}

pub async fn execute_draft_action<S: RemoteProtocolRuntimeState + Sync>(
    state: &S,
    input: ExecuteDraftStorageConnectorActionInput,
) -> Result<StorageConnectorActionResult> {
    let connector = connector_for(input.connection.driver_type)?;
    connector.connector.execute_draft_action(state, input).await
}

pub fn validate_driver_promotion_source(source: DriverType) -> Result<()> {
    if !storage_driver_descriptor(source)?
        .driver_recommendations
        .is_empty()
    {
        return Ok(());
    }
    Err(crate::errors::validation_error_with_code(
        crate::api::api_error_code::ApiErrorCode::PolicyPromotionSourceUnsupported,
        "only generic S3-compatible policies can be promoted",
    ))
}

pub fn validate_driver_promotion_target(source: DriverType, target: DriverType) -> Result<()> {
    if storage_driver_descriptor(source)?
        .driver_recommendations
        .iter()
        .any(|recommendation| recommendation.target_driver_type == target)
    {
        return Ok(());
    }
    Err(crate::errors::validation_error_with_code(
        crate::api::api_error_code::ApiErrorCode::PolicyPromotionTargetUnsupported,
        format!(
            "promoting S3-compatible policy to '{}' is not supported",
            target.as_str()
        ),
    ))
}

pub fn validate_driver_promotion_candidate(policy: &storage_policy::Model) -> Result<()> {
    connector_for(policy.driver_type)?
        .connector
        .validate_promotion_candidate(policy)
}

pub fn resolve_policy_upload_transport(
    policy: &storage_policy::Model,
) -> Result<StorageConnectorUploadTransport> {
    Ok(connector_for(policy.driver_type)?
        .connector
        .upload_transport(policy))
}

pub fn resolve_policy_object_naming(
    policy: &storage_policy::Model,
) -> Result<StorageConnectorObjectNamingMode> {
    Ok(connector_for(policy.driver_type)?
        .connector
        .descriptor()
        .capabilities
        .object_naming)
}

pub fn presigned_download_enabled(policy: &storage_policy::Model) -> Result<bool> {
    Ok(connector_for(policy.driver_type)?
        .connector
        .presigned_download_enabled(policy))
}

pub fn presigned_download_requires_filename_match(policy: &storage_policy::Model) -> Result<bool> {
    Ok(connector_for(policy.driver_type)?
        .connector
        .presigned_download_requires_filename_match(policy))
}

pub(crate) fn runtime_credential_requirement(
    driver_type: DriverType,
) -> Result<Option<StorageConnectorCredentialRequirement>> {
    Ok(connector_for(driver_type)?
        .connector
        .runtime_credential_requirement())
}

pub(crate) async fn load_runtime_credential(
    db: &sea_orm::DatabaseConnection,
    config: &crate::config::Config,
    policy: &storage_policy::Model,
    credential: &crate::entities::storage_policy_credential::Model,
) -> Result<Option<StorageConnectorRuntimeCredential>> {
    connector_for(policy.driver_type)?
        .connector
        .load_runtime_credential(db, config, policy, credential)
        .await
}

pub(crate) fn build_authorized_driver(
    policy: &storage_policy::Model,
    credential: StorageConnectorRuntimeCredential,
) -> Result<Arc<dyn StorageDriver>> {
    connector_for(policy.driver_type)?
        .connector
        .build_authorized_driver(policy, credential)
}

pub(crate) async fn validate_credential(
    db: &sea_orm::DatabaseConnection,
    config: &crate::config::Config,
    policy: &storage_policy::Model,
    credential: &crate::entities::storage_policy_credential::Model,
) -> Result<StorageCredentialValidationOutcome> {
    connector_for(policy.driver_type)?
        .connector
        .validate_credential(db, config, policy, credential)
        .await
}

pub fn streaming_direct_upload_eligible(
    policy: &storage_policy::Model,
    declared_size: i64,
) -> Result<bool> {
    Ok(resolve_policy_upload_transport(policy)?
        .supports_streaming_direct_upload(policy, declared_size))
}

pub(crate) async fn cleanup_snapshot_for_policy<S: SharedRuntimeState + Sync>(
    state: &S,
    policy: &storage_policy::Model,
) -> Result<Option<StoragePolicyCleanupDriverSnapshot>> {
    connector_for(policy.driver_type)?
        .connector
        .cleanup_snapshot_for_policy(state, policy)
        .await
}

pub(crate) fn can_create_cleanup_task_with_snapshot(
    driver_type: DriverType,
    driver_snapshot: &Option<StoragePolicyCleanupDriverSnapshot>,
) -> bool {
    connector_for(driver_type)
        .map(|connector| !connector.cleanup_snapshot_required || driver_snapshot.is_some())
        .unwrap_or(false)
}

pub(crate) async fn build_cleanup_driver<S: RemoteProtocolRuntimeState + Sync>(
    state: &S,
    policy: &storage_policy::Model,
    snapshots: StoragePolicyCleanupSnapshots<'_>,
) -> Result<Arc<dyn StorageDriver>> {
    connector_for(policy.driver_type)?
        .connector
        .build_cleanup_driver(state, policy, snapshots)
        .await
}
