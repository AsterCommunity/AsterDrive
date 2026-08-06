//! 存储驱动实现：`remote`。

mod list;
mod multipart;
mod presigned;
mod storage_driver;
mod stream_upload;
#[cfg(test)]
mod tests;

use crate::errors::{AsterError, Result};
use crate::services::remote::capability::RemoteCapabilityResolver;
use crate::storage::remote_protocol::RemoteStorageClient;
use aster_drive_model::entities::managed_follower;
use aster_drive_storage::object_key;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteDriverConfig {
    pub base_path: String,
    pub remote_storage_target_key: Option<String>,
    pub max_file_size: i64,
}

pub struct RemoteDriver {
    client: RemoteStorageClient,
    base_path: String,
    supports_capacity: bool,
    uses_reverse_tunnel: bool,
}

impl RemoteDriver {
    const MULTIPART_UPLOADS_PREFIX: &str = "uploads";

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
        let client = client.with_policy_context(
            config.remote_storage_target_key.as_deref(),
            config.max_file_size,
        );
        Ok(Self {
            client,
            base_path: config.base_path.trim_matches('/').to_string(),
            supports_capacity: resolver.capabilities().supports_capacity,
            uses_reverse_tunnel: follower
                .transport_mode
                .resolves_to_reverse_tunnel(&follower.base_url),
        })
    }

    fn object_key(&self, path: &str) -> String {
        object_key::join_key_prefix(&self.base_path, path)
    }

    fn strip_base_path<'a>(&self, object_key: &'a str) -> Option<&'a str> {
        object_key::strip_key_prefix(&self.base_path, object_key)
    }

    fn multipart_parts_prefix(upload_id: &str) -> String {
        format!("{}/{upload_id}/parts", Self::MULTIPART_UPLOADS_PREFIX)
    }

    fn multipart_part_key(upload_id: &str, part_number: i32) -> Result<String> {
        if part_number <= 0 {
            return Err(AsterError::validation_error(format!(
                "multipart part_number must be positive, got {part_number}"
            )));
        }
        Ok(format!(
            "{}/{}",
            Self::multipart_parts_prefix(upload_id),
            part_number
        ))
    }
}
