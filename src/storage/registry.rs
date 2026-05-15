//! 存储子模块：`registry`。

use super::StorageErrorKind;
use super::driver::StorageDriver;
use super::drivers::local::LocalDriver;
use super::drivers::remote::RemoteDriver;
use super::drivers::s3::S3Driver;
use super::error::storage_driver_error;
use super::multipart::MultipartStorageDriver;
use crate::db::repository::{managed_follower_repo, master_binding_repo};
use crate::entities::storage_policy;
use crate::errors::{Result, precondition_failed_with_subcode};
use crate::types::{DriverType, parse_storage_policy_options};
use dashmap::DashMap;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

/// 已实例化的 driver，按类型区分以支持 multipart downcast。
#[derive(Clone)]
enum DriverEntry {
    Local(Arc<LocalDriver>),
    Remote(Arc<RemoteDriver>),
    S3(Arc<S3Driver>),
    #[cfg(test)]
    Mock(Arc<dyn StorageDriver>),
}

impl DriverEntry {
    fn as_storage_driver(&self) -> Arc<dyn StorageDriver> {
        match self {
            DriverEntry::Local(d) => d.clone(),
            DriverEntry::Remote(d) => d.clone(),
            DriverEntry::S3(d) => d.clone(),
            #[cfg(test)]
            DriverEntry::Mock(d) => d.clone(),
        }
    }

    fn as_multipart_driver(&self) -> Option<Arc<dyn MultipartStorageDriver>> {
        match self {
            DriverEntry::Local(_) => None,
            DriverEntry::Remote(d) => Some(d.clone()),
            DriverEntry::S3(d) => Some(d.clone()),
            #[cfg(test)]
            DriverEntry::Mock(_) => None,
        }
    }
}

pub struct DriverRegistry {
    /// policy_id → 已实例化的 driver
    drivers: DashMap<i64, DriverEntry>,
    managed_followers_by_id: RwLock<HashMap<i64, crate::entities::managed_follower::Model>>,
    master_bindings_by_access_key: RwLock<HashMap<String, crate::entities::master_binding::Model>>,
}

impl DriverRegistry {
    pub fn new() -> Self {
        Self {
            drivers: DashMap::new(),
            managed_followers_by_id: RwLock::new(HashMap::new()),
            master_bindings_by_access_key: RwLock::new(HashMap::new()),
        }
    }

    /// 根据 StoragePolicy 获取或创建 driver（惰性实例化）
    pub fn get_driver(&self, policy: &storage_policy::Model) -> Result<Arc<dyn StorageDriver>> {
        Ok(self.get_entry(policy)?.as_storage_driver())
    }

    /// 获取支持 multipart upload 的 driver。
    ///
    /// 如果策略对应的 driver 不支持 multipart（如 LocalDriver），返回 `Err`。
    pub fn get_multipart_driver(
        &self,
        policy: &storage_policy::Model,
    ) -> Result<Arc<dyn MultipartStorageDriver>> {
        self.get_entry(policy)?
            .as_multipart_driver()
            .ok_or_else(|| {
                storage_driver_error(
                    StorageErrorKind::Unsupported,
                    format!(
                        "storage policy {} (driver: {:?}) does not support multipart upload",
                        policy.id, policy.driver_type
                    ),
                )
            })
    }

    /// 策略更新后使缓存的 driver 失效
    pub fn invalidate(&self, policy_id: i64) {
        self.drivers.remove(&policy_id);
    }

    pub fn invalidate_all(&self) {
        self.drivers.clear();
    }

    pub async fn reload_primary_state<C: sea_orm::ConnectionTrait>(&self, db: &C) -> Result<()> {
        self.reload_managed_followers(db).await?;
        self.reload_master_bindings(db).await
    }

    pub async fn reload_follower_state<C: sea_orm::ConnectionTrait>(&self, db: &C) -> Result<()> {
        self.reload_master_bindings(db).await
    }

    pub async fn reload_managed_followers<C: sea_orm::ConnectionTrait>(
        &self,
        db: &C,
    ) -> Result<()> {
        let followers = managed_follower_repo::find_all(db).await?;
        let mut by_id = HashMap::with_capacity(followers.len());
        for follower in followers {
            by_id.insert(follower.id, follower);
        }
        *self.managed_followers_by_id.write() = by_id;
        Ok(())
    }

    pub async fn reload_master_bindings<C: sea_orm::ConnectionTrait>(&self, db: &C) -> Result<()> {
        let bindings = master_binding_repo::find_all(db).await?;
        let mut by_access_key = HashMap::with_capacity(bindings.len());
        for binding in bindings {
            by_access_key.insert(binding.access_key.clone(), binding);
        }
        *self.master_bindings_by_access_key.write() = by_access_key;
        Ok(())
    }

    pub fn get_managed_follower(
        &self,
        follower_id: i64,
    ) -> Option<crate::entities::managed_follower::Model> {
        self.managed_followers_by_id
            .read()
            .get(&follower_id)
            .cloned()
    }

    pub fn find_master_binding_by_access_key(
        &self,
        access_key: &str,
    ) -> Option<crate::entities::master_binding::Model> {
        self.master_bindings_by_access_key
            .read()
            .get(access_key)
            .cloned()
    }

    #[cfg(test)]
    pub fn insert_for_test(&self, policy_id: i64, driver: Arc<dyn StorageDriver>) {
        self.drivers.insert(policy_id, DriverEntry::Mock(driver));
    }

    #[cfg(test)]
    pub fn insert_s3_for_test(&self, policy_id: i64, driver: Arc<S3Driver>) {
        self.drivers.insert(policy_id, DriverEntry::S3(driver));
    }

    fn get_entry(&self, policy: &storage_policy::Model) -> Result<DriverEntry> {
        if let Some(entry) = self.drivers.get(&policy.id) {
            return Ok(entry.clone());
        }
        let entry = self.create_entry(policy)?;
        self.drivers.insert(policy.id, entry.clone());
        Ok(entry)
    }

    fn create_entry(&self, policy: &storage_policy::Model) -> Result<DriverEntry> {
        match policy.driver_type {
            DriverType::Local => Ok(DriverEntry::Local(Arc::new(LocalDriver::new(policy)?))),
            DriverType::Remote => {
                let remote_node_id = policy.remote_node_id.ok_or_else(|| {
                    storage_driver_error(
                        StorageErrorKind::Misconfigured,
                        "remote storage policy missing remote_node_id",
                    )
                })?;
                let remote_node = self.get_managed_follower(remote_node_id).ok_or_else(|| {
                    storage_driver_error(
                        StorageErrorKind::Misconfigured,
                        format!("remote node #{remote_node_id} not loaded in registry"),
                    )
                })?;
                if !remote_node.is_enabled {
                    return Err(precondition_failed_with_subcode(
                        "remote_node.disabled",
                        format!("remote node #{remote_node_id} is disabled"),
                    ));
                }
                let capabilities =
                    crate::storage::remote_protocol::RemoteStorageCapabilities::from_stored_json(
                        &remote_node.last_capabilities,
                    );
                let options = parse_storage_policy_options(policy.options.as_ref());
                if let Err(error) =
                    capabilities.validate_for_remote_policy(remote_node_id, policy.id, &options)
                {
                    tracing::warn!(
                        remote_node_id,
                        policy_id = policy.id,
                        protocol_version = %capabilities.protocol_version,
                        min_supported_protocol_version = %capabilities.min_supported_protocol_version,
                        "remote storage policy protocol compatibility check failed: {error}"
                    );
                    return Err(error);
                }
                Ok(DriverEntry::Remote(Arc::new(RemoteDriver::new(
                    policy,
                    &remote_node,
                )?)))
            }
            DriverType::S3 => Ok(DriverEntry::S3(Arc::new(S3Driver::new(policy)?))),
        }
    }
}

impl Default for DriverRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{StoredStoragePolicyAllowedTypes, StoredStoragePolicyOptions};
    use std::time::Duration;

    fn remote_policy(remote_node_id: Option<i64>) -> storage_policy::Model {
        let now = chrono::Utc::now();
        storage_policy::Model {
            id: 42,
            name: "remote policy".to_string(),
            driver_type: DriverType::Remote,
            endpoint: String::new(),
            bucket: String::new(),
            access_key: String::new(),
            secret_key: String::new(),
            base_path: "base".to_string(),
            remote_node_id,
            max_file_size: 0,
            allowed_types: StoredStoragePolicyAllowedTypes::empty(),
            options: StoredStoragePolicyOptions::empty(),
            is_default: false,
            chunk_size: 5_242_880,
            created_at: now,
            updated_at: now,
        }
    }

    fn managed_follower(is_enabled: bool) -> crate::entities::managed_follower::Model {
        let now = chrono::Utc::now();
        crate::entities::managed_follower::Model {
            id: 7,
            name: "follower".to_string(),
            base_url: "http://storage.example.com/root/".to_string(),
            access_key: "follower-ak".to_string(),
            secret_key: "follower-sk".to_string(),
            is_enabled,
            last_capabilities: serde_json::to_string(
                &crate::storage::remote_protocol::RemoteStorageCapabilities::current(),
            )
            .expect("current remote capabilities should serialize"),
            last_error: String::new(),
            last_checked_at: None,
            created_at: now,
            updated_at: now,
        }
    }

    fn registry_with_follower(
        follower: crate::entities::managed_follower::Model,
    ) -> DriverRegistry {
        let registry = DriverRegistry::new();
        registry
            .managed_followers_by_id
            .write()
            .insert(follower.id, follower);
        registry
    }

    #[test]
    fn remote_policy_requires_remote_node_id() {
        let registry = DriverRegistry::new();

        let error = match registry.get_driver(&remote_policy(None)) {
            Ok(_) => panic!("remote policy without node id should fail"),
            Err(error) => error,
        };

        assert_eq!(error.code(), "E031");
        assert_eq!(
            error.storage_error_kind(),
            Some(StorageErrorKind::Misconfigured)
        );
        assert!(error.message().contains("missing remote_node_id"));
    }

    #[test]
    fn remote_policy_requires_loaded_follower() {
        let registry = DriverRegistry::new();

        let error = match registry.get_driver(&remote_policy(Some(7))) {
            Ok(_) => panic!("remote policy without loaded follower should fail"),
            Err(error) => error,
        };

        assert_eq!(error.code(), "E031");
        assert_eq!(
            error.storage_error_kind(),
            Some(StorageErrorKind::Misconfigured)
        );
        assert!(error.message().contains("remote node #7 not loaded"));
    }

    #[test]
    fn remote_policy_rejects_disabled_follower() {
        let registry = registry_with_follower(managed_follower(false));

        let error = match registry.get_driver(&remote_policy(Some(7))) {
            Ok(_) => panic!("disabled follower should fail"),
            Err(error) => error,
        };

        assert_eq!(error.code(), "E060");
        assert_eq!(
            error.storage_error_kind(),
            Some(StorageErrorKind::Precondition)
        );
        assert_eq!(error.api_error_subcode(), Some("remote_node.disabled"));
        assert!(error.message().contains("remote node #7 is disabled"));
    }

    #[tokio::test]
    async fn remote_policy_resolves_enabled_follower_driver_capabilities() {
        let registry = registry_with_follower(managed_follower(true));
        let policy = remote_policy(Some(7));

        let driver = registry
            .get_driver(&policy)
            .expect("enabled follower should create remote driver");

        assert!(driver.as_list().is_some());
        assert!(driver.as_stream_upload().is_some());
        assert!(driver.as_presigned().is_some());
        assert!(driver.as_multipart().is_some());

        let presigned = driver
            .as_presigned()
            .expect("remote driver should support presigned URLs")
            .presigned_put_url("files/object.bin", Duration::from_secs(60))
            .await
            .expect("presigned URL should build")
            .expect("remote driver should return URL");
        let parsed = reqwest::Url::parse(&presigned).expect("presigned URL should parse");

        assert_eq!(
            parsed.path(),
            "/root/api/v1/internal/storage/objects/base/files/object.bin"
        );
        assert!(
            parsed
                .query_pairs()
                .any(|(key, value)| key == "aster_access_key" && value == "follower-ak"),
            "expected follower access key in '{presigned}'"
        );
    }

    #[test]
    fn remote_policy_rejects_missing_protocol_discovery() {
        let mut follower = managed_follower(true);
        follower.last_capabilities = "{}".to_string();
        let registry = registry_with_follower(follower);

        let error = match registry.get_driver(&remote_policy(Some(7))) {
            Ok(_) => panic!("unknown capabilities should block remote driver initialization"),
            Err(error) => error,
        };

        assert_eq!(error.code(), "E031");
        assert_eq!(
            error.storage_error_kind(),
            Some(StorageErrorKind::Misconfigured)
        );
        assert!(error.message().contains("protocol incompatible"));
        assert!(error.message().contains("remote node #7"));
    }

    #[test]
    fn remote_policy_rejects_presigned_download_when_range_cors_missing() {
        let mut capabilities =
            crate::storage::remote_protocol::RemoteStorageCapabilities::current();
        capabilities.browser_cors.allowed_headers = vec!["content-type".to_string()];
        capabilities.browser_cors.exposed_headers =
            vec!["Accept-Ranges".to_string(), "Content-Length".to_string()];
        let mut follower = managed_follower(true);
        follower.last_capabilities =
            serde_json::to_string(&capabilities).expect("test capabilities should serialize");
        let registry = registry_with_follower(follower);
        let mut policy = remote_policy(Some(7));
        policy.options =
            crate::types::serialize_storage_policy_options(&crate::types::StoragePolicyOptions {
                remote_download_strategy: Some(crate::types::RemoteDownloadStrategy::Presigned),
                ..Default::default()
            })
            .expect("policy options should serialize");

        let error = match registry.get_driver(&policy) {
            Ok(_) => panic!("incomplete browser CORS should block remote presigned download"),
            Err(error) => error,
        };

        assert_eq!(error.code(), "E031");
        assert_eq!(
            error.storage_error_kind(),
            Some(StorageErrorKind::Misconfigured)
        );
        assert!(
            error
                .message()
                .contains("browser CORS contract is incomplete")
        );
        assert!(error.message().contains("allowed_headers missing range"));
        assert!(
            error
                .message()
                .contains("exposed_headers missing Content-Range")
        );
    }
}
