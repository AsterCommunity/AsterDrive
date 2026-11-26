//! Provider evidence: developer-docs/zh-CN/reference/storage-provider-evidence.md#remote
//! 存储驱动实现：`remote`。

mod list;
mod multipart;
mod presigned;
mod storage_driver;
mod stream_upload;
#[cfg(test)]
mod tests;

use crate::errors::Result;
use crate::services::remote::capability::RemoteCapabilityResolver;
use crate::storage::remote_protocol::RemoteStorageCapabilities;
use crate::storage::remote_protocol::RemoteStorageClient;
use crate::storage::remote_protocol::RemoteStorageTargetRuntimeCapabilities;
use aster_drive_model::entities::managed_follower;
use aster_drive_storage::object_key;

/// 远端节点存储驱动配置。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteDriverConfig {
    /// 拼接在每条 driver 相对路径之前的对象 key 前缀。
    pub base_path: String,
    /// follower 上标识远端存储策略的 storage target key。
    pub remote_storage_target_key: String,
    /// 下发给远端节点策略校验的单文件大小上限（字节）。
    pub max_file_size: i64,
    /// Maximum time spent acquiring a probe slot and querying remote-node capacity.
    pub capacity_probe_timeout: std::time::Duration,
}

/// 通过远端存储协议把对象 I/O 委托给 managed 远端节点执行的驱动。
pub struct RemoteDriver {
    client: RemoteStorageClient,
    base_path: String,
    supports_capacity: bool,
    supports_compose: bool,
    uses_reverse_tunnel: bool,
    max_multipart_parts: u64,
    max_multipart_part_size: Option<u64>,
    max_multipart_object_size: Option<u64>,
    target_runtime_capabilities: Option<RemoteStorageTargetRuntimeCapabilities>,
    capacity_probe_timeout: std::time::Duration,
}

impl RemoteDriver {
    /// 从 follower 记录构建驱动：校验协议兼容性并采纳其上报的 compose/multipart 限制。
    pub fn new(config: &RemoteDriverConfig, follower: &managed_follower::Model) -> Result<Self> {
        Self::from_client(
            config,
            follower,
            RemoteStorageClient::new(
                &follower.base_url,
                &follower.access_key,
                &follower.secret_key,
            )?,
        )
    }

    pub(crate) fn new_with_client(
        config: &RemoteDriverConfig,
        follower: &managed_follower::Model,
        client: RemoteStorageClient,
    ) -> Result<Self> {
        Self::from_client(config, follower, client)
    }

    fn from_client(
        config: &RemoteDriverConfig,
        follower: &managed_follower::Model,
        client: RemoteStorageClient,
    ) -> Result<Self> {
        let resolver = RemoteCapabilityResolver::from_remote_node(follower);
        resolver.ensure_protocol_compatible("remote storage driver")?;
        let client =
            client.with_policy_context(&config.remote_storage_target_key, config.max_file_size);
        let protocol_capabilities =
            RemoteStorageCapabilities::from_stored_json(&follower.last_capabilities);
        let max_multipart_parts = protocol_capabilities
            .limits
            .compose_max_parts
            .and_then(|value| u64::try_from(value).ok())
            .filter(|value| *value > 0)
            .unwrap_or(10_000);
        let max_multipart_part_size = protocol_capabilities
            .limits
            .max_ingress_size
            .and_then(|value| u64::try_from(value).ok())
            .filter(|value| *value > 0);
        let max_multipart_object_size = protocol_capabilities
            .limits
            .compose_max_object_size
            .and_then(|value| u64::try_from(value).ok())
            .filter(|value| *value > 0);
        let target_runtime_capabilities = resolver
            .target_runtime_capabilities(&config.remote_storage_target_key)
            .cloned();
        Ok(Self {
            client,
            base_path: config.base_path.trim_matches('/').to_string(),
            supports_capacity: resolver.capabilities().supports_capacity,
            supports_compose: resolver.capabilities().features.compose,
            uses_reverse_tunnel: follower
                .transport_mode
                .resolves_to_reverse_tunnel(&follower.base_url),
            max_multipart_parts,
            max_multipart_part_size,
            max_multipart_object_size,
            target_runtime_capabilities,
            capacity_probe_timeout: config.capacity_probe_timeout,
        })
    }

    fn object_key(&self, path: &str) -> String {
        object_key::join_key_prefix(&self.base_path, path)
    }

    fn strip_base_path<'a>(&self, object_key: &'a str) -> Option<&'a str> {
        object_key::strip_key_prefix(&self.base_path, object_key)
    }
}
