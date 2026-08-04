//! 存储子模块：`registry`。

#[cfg(any(test, debug_assertions))]
use super::drivers::s3::S3Driver;
use super::metrics_driver::{MetricsMultipartStorageDriver, MetricsStorageDriver};
use crate::config::Config;
use crate::db::repository::{managed_follower_repo, master_binding_repo, policy_repo};
use crate::errors::{AsterError, Result};
use crate::storage::connectors::{
    StorageConnectorRegistry, StorageConnectorRuntimeCredential, builtin_storage_connector_registry,
};
use crate::storage::remote_protocol::RemoteProtocolRuntime;
use aster_drive_metrics::SharedMetricsRecorder;
use aster_drive_model::entities::storage_policy;
use aster_drive_storage::{
    MultipartStorageDriver, StorageDriver, StorageErrorKind, storage_driver_error,
};
use dashmap::DashMap;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

/// 已实例化的 driver。
///
/// `storage` 是业务路径统一使用的驱动；启用 metrics 时它会在创建 entry 时包一层
/// `MetricsStorageDriver`。`multipart` 是分片上传专用路径；启用 metrics 时同样包一层
/// `MetricsMultipartStorageDriver`，保证 `get_driver().extensions().multipart` 和
/// `get_multipart_driver()` 两条入口都记录指标。
#[derive(Clone)]
struct DriverEntry {
    storage: Arc<dyn StorageDriver>,
    multipart: Option<Arc<dyn MultipartStorageDriver>>,
}

impl DriverEntry {
    fn storage_driver(&self) -> Arc<dyn StorageDriver> {
        self.storage.clone()
    }

    fn multipart_driver(&self) -> Option<Arc<dyn MultipartStorageDriver>> {
        self.multipart.clone()
    }
}

pub struct DriverRegistry {
    /// policy_id → 已实例化的 driver
    drivers: DashMap<i64, DriverEntry>,
    driver_init_lock: parking_lot::Mutex<()>,
    managed_followers_by_id:
        RwLock<HashMap<i64, aster_drive_model::entities::managed_follower::Model>>,
    master_bindings_by_access_key:
        RwLock<HashMap<String, aster_drive_model::entities::master_binding::Model>>,
    runtime_credentials_by_policy_id: RwLock<HashMap<i64, StorageConnectorRuntimeCredential>>,
    // Configuration descriptors and runtime factories must come from one
    // registry; otherwise the admin surface can advertise a connector that the
    // runtime cache cannot construct (or construct one the UI cannot describe).
    connectors: Arc<StorageConnectorRegistry>,
    metrics: SharedMetricsRecorder,
    remote_protocol: RwLock<Option<Arc<RemoteProtocolRuntime>>>,
}

const STORAGE_CONNECTOR_METRIC_LABEL: &str = "storage_connector";

impl DriverRegistry {
    pub fn new(metrics: SharedMetricsRecorder) -> Result<Self> {
        let connectors = Arc::new(builtin_storage_connector_registry()?);
        Ok(Self::with_connectors(metrics, connectors))
    }

    pub(crate) fn with_connectors(
        metrics: SharedMetricsRecorder,
        connectors: Arc<StorageConnectorRegistry>,
    ) -> Self {
        Self {
            drivers: DashMap::new(),
            driver_init_lock: parking_lot::Mutex::new(()),
            managed_followers_by_id: RwLock::new(HashMap::new()),
            master_bindings_by_access_key: RwLock::new(HashMap::new()),
            runtime_credentials_by_policy_id: RwLock::new(HashMap::new()),
            connectors,
            metrics,
            remote_protocol: RwLock::new(None),
        }
    }

    pub fn noop() -> Result<Self> {
        Self::new(aster_drive_metrics::NoopMetrics::arc())
    }

    pub(crate) fn connectors(&self) -> &StorageConnectorRegistry {
        &self.connectors
    }

    /// Reload the policy routing snapshot through the same connector registry
    /// that constructs runtime drivers.
    pub async fn reload_policy_snapshot(
        &self,
        snapshot: &crate::storage::PolicySnapshot,
        db: &sea_orm::DatabaseConnection,
    ) -> Result<()> {
        snapshot.reload(db, self.connectors()).await
    }

    /// 根据 StoragePolicy 获取或创建 driver（惰性实例化）
    pub fn get_driver(&self, policy: &storage_policy::Model) -> Result<Arc<dyn StorageDriver>> {
        Ok(self.get_entry(policy)?.storage_driver())
    }

    pub(crate) fn get_cached_driver(&self, policy_id: i64) -> Option<Arc<dyn StorageDriver>> {
        self.drivers
            .get(&policy_id)
            .map(|entry| entry.storage_driver())
    }

    /// 获取支持 multipart upload 的 driver。
    ///
    /// 如果策略对应的 driver 不支持 multipart（如 LocalDriver），返回 `Err`。
    pub fn get_multipart_driver(
        &self,
        policy: &storage_policy::Model,
    ) -> Result<Arc<dyn MultipartStorageDriver>> {
        self.get_entry(policy)?.multipart_driver().ok_or_else(|| {
            AsterError::from(storage_driver_error(
                StorageErrorKind::Unsupported,
                format!(
                    "storage policy {} (connector: {}) does not support multipart upload",
                    policy.id, policy.connector_id
                ),
            ))
        })
    }

    pub(crate) fn build_uncached_driver(
        &self,
        policy: &storage_policy::Model,
    ) -> Result<Arc<dyn StorageDriver>> {
        // Long-running maintenance jobs may touch cold object-storage policies once
        // and then go idle for hours. Build a driver for that job without inserting
        // it into the shared registry, so SDK clients and HTTP pools do not become
        // process-lifetime cache entries just because maintenance scanned them.
        Ok(self.create_entry(policy)?.storage_driver())
    }

    /// 策略更新后使缓存的 driver 失效
    pub fn invalidate(&self, policy_id: i64) {
        let _guard = self.driver_init_lock.lock();
        self.drivers.remove(&policy_id);
    }

    pub fn invalidate_all(&self) {
        let _guard = self.driver_init_lock.lock();
        self.drivers.clear();
    }

    pub async fn reload_primary_state(
        &self,
        db: &sea_orm::DatabaseConnection,
        config: &Config,
    ) -> Result<()> {
        self.reload_managed_followers(db).await?;
        self.reload_master_bindings(db).await?;
        self.reload_storage_policy_credentials(db, config).await
    }

    pub fn set_remote_protocol(&self, remote_protocol: Arc<RemoteProtocolRuntime>) {
        *self.remote_protocol.write() = Some(remote_protocol);
        self.invalidate_all();
    }

    pub async fn reload_follower_state(&self, db: &sea_orm::DatabaseConnection) -> Result<()> {
        self.reload_master_bindings(db).await
    }

    pub async fn reload_managed_followers(&self, db: &sea_orm::DatabaseConnection) -> Result<()> {
        let followers = managed_follower_repo::find_all(db).await?;
        let mut by_id = HashMap::with_capacity(followers.len());
        for follower in followers {
            by_id.insert(follower.id, follower);
        }
        *self.managed_followers_by_id.write() = by_id;
        Ok(())
    }

    pub async fn reload_master_bindings(&self, db: &sea_orm::DatabaseConnection) -> Result<()> {
        let bindings = master_binding_repo::find_all(db).await?;
        let mut by_access_key = HashMap::with_capacity(bindings.len());
        for binding in bindings {
            by_access_key.insert(binding.access_key.clone(), binding);
        }
        *self.master_bindings_by_access_key.write() = by_access_key;
        Ok(())
    }

    pub async fn reload_storage_policy_credentials(
        &self,
        db: &sea_orm::DatabaseConnection,
        config: &Config,
    ) -> Result<()> {
        let mut by_policy_id = HashMap::new();

        let connector_credentials =
            crate::db::repository::storage_policy_connector_credential_repo::find_all(db).await?;
        for credential in connector_credentials {
            let policy = match policy_repo::find_by_id(db, credential.policy_id).await {
                Ok(policy) => policy,
                Err(error) => {
                    tracing::warn!(
                        policy_id = credential.policy_id,
                        error = %error,
                        "skipping connector credential reload because policy lookup failed"
                    );
                    continue;
                }
            };
            if policy.connector_id != credential.connector_id {
                tracing::warn!(
                    policy_id = policy.id,
                    policy_connector_id = %policy.connector_id,
                    credential_connector_id = %credential.connector_id,
                    "skipping connector credential with mismatched connector id"
                );
                continue;
            }
            let connector = match self.connectors().require_policy(&policy) {
                Ok(connector) => connector,
                Err(error) => {
                    tracing::warn!(policy_id = policy.id, error = %error, "skipping connector credential reload because connector lookup failed");
                    continue;
                }
            };
            let runtime_credential = match connector
                .load_runtime_credential(db, config, &policy, &credential)
                .await
            {
                Ok(Some(runtime_credential)) => runtime_credential,
                Ok(None) => continue,
                Err(error) => {
                    tracing::warn!(
                        policy_id = credential.policy_id,
                        credential_id = credential.id,
                        error = %error,
                        "skipping storage credential reload because connector runtime credential loading failed"
                    );
                    continue;
                }
            };
            by_policy_id.insert(policy.id, runtime_credential);
        }
        *self.runtime_credentials_by_policy_id.write() = by_policy_id;
        self.invalidate_all();
        Ok(())
    }

    pub fn get_managed_follower(
        &self,
        follower_id: i64,
    ) -> Option<aster_drive_model::entities::managed_follower::Model> {
        self.managed_followers_by_id
            .read()
            .get(&follower_id)
            .cloned()
    }

    pub fn find_master_binding_by_access_key(
        &self,
        access_key: &str,
    ) -> Option<aster_drive_model::entities::master_binding::Model> {
        self.master_bindings_by_access_key
            .read()
            .get(access_key)
            .cloned()
    }

    pub(crate) fn get_runtime_credential(
        &self,
        policy_id: i64,
    ) -> Option<StorageConnectorRuntimeCredential> {
        self.runtime_credentials_by_policy_id
            .read()
            .get(&policy_id)
            .cloned()
    }

    pub(crate) fn remote_protocol(&self) -> Option<Arc<RemoteProtocolRuntime>> {
        self.remote_protocol.read().clone()
    }

    #[cfg(any(test, debug_assertions))]
    pub fn insert_for_test(&self, policy_id: i64, driver: Arc<dyn StorageDriver>) {
        self.drivers.insert(
            policy_id,
            DriverEntry {
                storage: driver,
                multipart: None,
            },
        );
    }

    /// Insert the exact S3 driver instance for tests that need raw S3 behavior.
    ///
    /// This intentionally bypasses metrics wrapping so tests can rely on the
    /// provided `Arc<S3Driver>` being the stored storage and multipart object.
    #[cfg(any(test, debug_assertions))]
    pub fn insert_s3_for_test(&self, policy_id: i64, driver: Arc<S3Driver>) {
        let storage: Arc<dyn StorageDriver> = driver.clone();
        let multipart: Arc<dyn MultipartStorageDriver> = driver;
        self.drivers.insert(
            policy_id,
            DriverEntry {
                storage,
                multipart: Some(multipart),
            },
        );
    }

    #[cfg(any(test, debug_assertions))]
    pub fn has_cached_driver_for_test(&self, policy_id: i64) -> bool {
        self.drivers.contains_key(&policy_id)
    }

    fn get_entry(&self, policy: &storage_policy::Model) -> Result<DriverEntry> {
        if let Some(entry) = self.drivers.get(&policy.id) {
            return Ok(entry.clone());
        }
        let _guard = self.driver_init_lock.lock();
        if let Some(entry) = self.drivers.get(&policy.id) {
            return Ok(entry.clone());
        }
        let entry = self.create_entry(policy)?;
        self.drivers.insert(policy.id, entry.clone());
        Ok(entry)
    }

    fn create_entry(&self, policy: &storage_policy::Model) -> Result<DriverEntry> {
        let driver = self
            .connectors
            .require_policy(policy)?
            .build_runtime_driver(self, policy)?;
        Ok(self.build_entry(
            STORAGE_CONNECTOR_METRIC_LABEL,
            driver.storage,
            driver.multipart,
        ))
    }

    fn build_entry(
        &self,
        metric_label: &'static str,
        storage: Arc<dyn StorageDriver>,
        multipart: Option<Arc<dyn MultipartStorageDriver>>,
    ) -> DriverEntry {
        let (storage, multipart) = if self.metrics.enabled() {
            let multipart = multipart.map(|driver| {
                Arc::new(MetricsMultipartStorageDriver::new(
                    driver,
                    metric_label,
                    self.metrics.clone(),
                )) as Arc<dyn MultipartStorageDriver>
            });
            let storage = Arc::new(MetricsStorageDriver::new(
                storage,
                metric_label,
                self.metrics.clone(),
                multipart.clone(),
            )) as Arc<dyn StorageDriver>;
            (storage, multipart)
        } else {
            (storage, multipart)
        };
        DriverEntry { storage, multipart }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::api_error_code::ApiErrorCode;
    use crate::storage::connectors::{
        StorageConnector, StorageConnectorContext, StorageConnectorCredentialInput,
        StorageConnectorDriver, StorageConnectorUploadTransport,
    };
    use aster_drive_metrics::MetricsRecorder;
    use aster_drive_model::types::{RemoteDownloadStrategy, RemoteUploadStrategy};
    use aster_drive_storage::error::{
        Result as StorageResult, StorageErrorKind, storage_driver_error,
    };
    use parking_lot::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    #[derive(Default)]
    struct CapturingMetrics {
        storage_operations: Mutex<Vec<&'static str>>,
    }

    impl MetricsRecorder for CapturingMetrics {
        fn enabled(&self) -> bool {
            true
        }

        fn record_storage_driver_operation(
            &self,
            _driver: &'static str,
            operation: &'static str,
            _status: &'static str,
            _kind: &'static str,
            _duration_seconds: f64,
        ) {
            self.storage_operations.lock().push(operation);
        }
    }

    struct TestMultipartDriver;

    struct CapturingConnector {
        descriptor: aster_drive_storage::StorageConnectorDescriptor,
        runtime_builds: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl StorageConnector for CapturingConnector {
        fn descriptor(&self) -> aster_drive_storage::StorageConnectorDescriptor {
            self.descriptor.clone()
        }

        fn localization(&self) -> Result<aster_drive_storage::StorageConnectorLocalization> {
            let locale = aster_drive_model::types::LocaleTag::parse("en")
                .map_err(AsterError::internal_error)?;
            let messages = self
                .descriptor
                .localization_message_ids()
                .into_iter()
                .map(|message_id| (message_id.to_string(), message_id.to_string()))
                .collect();
            aster_drive_storage::StorageConnectorLocalization::new(
                self.descriptor.connector_id.clone(),
                locale.clone(),
                "test",
                std::collections::BTreeMap::from([(locale, messages)]),
            )
            .map_err(|error| AsterError::internal_error(error.to_string()))
        }

        async fn build_draft_driver(
            &self,
            _context: &StorageConnectorContext<'_>,
            _policy: &storage_policy::Model,
            _credential: &StorageConnectorCredentialInput,
        ) -> Result<Box<dyn StorageDriver>> {
            Ok(Box::new(TestMultipartDriver))
        }

        fn build_runtime_driver(
            &self,
            _registry: &DriverRegistry,
            _policy: &storage_policy::Model,
        ) -> Result<StorageConnectorDriver> {
            self.runtime_builds.fetch_add(1, Ordering::SeqCst);
            Ok(StorageConnectorDriver::storage(Arc::new(
                TestMultipartDriver,
            )))
        }

        fn upload_transport(
            &self,
            _policy: &storage_policy::Model,
        ) -> Result<StorageConnectorUploadTransport> {
            Ok(StorageConnectorUploadTransport::Local)
        }
    }

    #[async_trait::async_trait]
    impl StorageDriver for TestMultipartDriver {
        async fn put(&self, _path: &str, _data: &[u8]) -> StorageResult<String> {
            panic!("not used")
        }

        async fn get(&self, _path: &str) -> StorageResult<Vec<u8>> {
            panic!("not used")
        }

        async fn get_stream(
            &self,
            _path: &str,
        ) -> StorageResult<Box<dyn tokio::io::AsyncRead + Unpin + Send>> {
            panic!("not used")
        }

        async fn delete(&self, _path: &str) -> StorageResult<()> {
            panic!("not used")
        }

        async fn exists(&self, _path: &str) -> StorageResult<bool> {
            panic!("not used")
        }

        async fn metadata(
            &self,
            _path: &str,
        ) -> StorageResult<aster_drive_storage::traits::driver::BlobMetadata> {
            panic!("not used")
        }

        fn extensions(&self) -> aster_drive_storage::traits::StorageDriverExtensions<'_> {
            aster_drive_storage::traits::StorageDriverExtensions {
                multipart: Some(self),
                ..Default::default()
            }
        }
    }

    #[async_trait::async_trait]
    impl MultipartStorageDriver for TestMultipartDriver {
        async fn create_multipart_upload(&self, _path: &str) -> StorageResult<String> {
            Ok("upload-1".to_string())
        }

        async fn presigned_upload_part_url(
            &self,
            _path: &str,
            _upload_id: &str,
            _part_number: i32,
            _expires: Duration,
        ) -> StorageResult<String> {
            panic!("not used")
        }

        async fn complete_multipart_upload(
            &self,
            _path: &str,
            _upload_id: &str,
            _parts: Vec<(i32, String)>,
        ) -> StorageResult<()> {
            panic!("not used")
        }

        async fn upload_multipart_part(
            &self,
            _path: &str,
            _upload_id: &str,
            _part_number: i32,
            _data: &[u8],
        ) -> StorageResult<String> {
            panic!("not used")
        }

        async fn abort_multipart_upload(&self, _path: &str, _upload_id: &str) -> StorageResult<()> {
            Err(storage_driver_error(
                StorageErrorKind::NotFound,
                "multipart upload missing",
            ))
        }

        async fn list_uploaded_part_details(
            &self,
            _path: &str,
            _upload_id: &str,
        ) -> StorageResult<Vec<aster_drive_storage::traits::UploadedMultipartPart>> {
            panic!("not used")
        }
    }

    fn local_policy() -> storage_policy::Model {
        let mut policy =
            crate::storage::connectors::test_support::local_policy("data/test-local-driver");
        policy.id = 42;
        policy.name = "local policy".to_string();
        policy.chunk_size = 5_242_880;
        policy
    }

    fn remote_policy(remote_node_id: Option<i64>) -> storage_policy::Model {
        let mut policy = crate::storage::connectors::test_support::remote_policy(
            "base",
            remote_node_id,
            RemoteDownloadStrategy::RelayStream,
            RemoteUploadStrategy::RelayStream,
        );
        policy.id = 42;
        policy.name = "remote policy".to_string();
        policy.chunk_size = 5_242_880;
        policy
    }

    fn managed_follower(is_enabled: bool) -> aster_drive_model::entities::managed_follower::Model {
        let now = chrono::Utc::now();
        aster_drive_model::entities::managed_follower::Model {
            id: 7,
            name: "follower".to_string(),
            base_url: "http://storage.example.com/root/".to_string(),
            access_key: "follower-ak".to_string(),
            secret_key: "follower-sk".to_string(),
            is_enabled,
            transport_mode: aster_drive_model::types::RemoteNodeTransportMode::Direct,
            last_capabilities: serde_json::to_string(
                &crate::storage::remote_protocol::RemoteStorageCapabilities::current(),
            )
            .expect("current remote capabilities should serialize"),
            last_error: String::new(),
            last_checked_at: None,
            tunnel_last_error: String::new(),
            tunnel_last_seen_at: None,
            created_at: now,
            updated_at: now,
        }
    }

    fn registry_with_follower(
        follower: aster_drive_model::entities::managed_follower::Model,
    ) -> DriverRegistry {
        let registry = DriverRegistry::noop().expect("built-in storage connector registry");
        registry
            .managed_followers_by_id
            .write()
            .insert(follower.id, follower);
        registry
    }

    #[test]
    fn runtime_driver_construction_dispatches_through_injected_connector_registry() {
        let descriptor = builtin_storage_connector_registry()
            .expect("built-in connector registry")
            .require_connector(&aster_drive_storage::ConnectorId::declared(
                "asterdrive.storage.local",
            ))
            .expect("local connector")
            .descriptor();
        let runtime_builds = Arc::new(AtomicUsize::new(0));
        let connectors = Arc::new(
            StorageConnectorRegistry::new(vec![Arc::new(CapturingConnector {
                descriptor,
                runtime_builds: runtime_builds.clone(),
            })])
            .expect("capturing connector registry"),
        );
        let registry =
            DriverRegistry::with_connectors(aster_drive_metrics::NoopMetrics::arc(), connectors);
        let policy = local_policy();

        let first = registry
            .get_driver(&policy)
            .expect("injected connector should construct the runtime driver");
        let second = registry
            .get_driver(&policy)
            .expect("cached runtime driver should resolve");

        assert_eq!(runtime_builds.load(Ordering::SeqCst), 1);
        assert!(Arc::ptr_eq(&first, &second));
        assert!(
            registry.get_multipart_driver(&policy).is_err(),
            "storage-only connector bundles must not expose multipart support"
        );
    }

    #[test]
    fn connector_registry_accepts_non_builtin_plugin_id_without_a_parallel_path() {
        let mut descriptor = builtin_storage_connector_registry()
            .expect("built-in connector registry")
            .require_connector(&aster_drive_storage::ConnectorId::declared(
                "asterdrive.storage.local",
            ))
            .expect("local connector")
            .descriptor();
        let plugin_id = aster_drive_storage::ConnectorId::declared("com.example.storage");
        descriptor.connector_id = plugin_id.clone();

        let registry = StorageConnectorRegistry::new(vec![Arc::new(CapturingConnector {
            descriptor,
            runtime_builds: Arc::new(AtomicUsize::new(0)),
        })])
        .expect("third-party plugin should use the same connector registry");

        assert_eq!(
            registry
                .require_connector(&plugin_id)
                .expect("third-party plugin id should resolve")
                .descriptor()
                .connector_id,
            plugin_id
        );
    }

    #[test]
    fn connector_registry_rejects_invalid_and_duplicate_action_ids() {
        let mut descriptor = builtin_storage_connector_registry()
            .expect("built-in connector registry")
            .require_connector(&aster_drive_storage::ConnectorId::declared(
                "asterdrive.storage.local",
            ))
            .expect("local connector")
            .descriptor();
        descriptor.actions[0].action_id =
            aster_drive_storage::StorageConnectorActionId::declared("Invalid Action");
        let invalid = match StorageConnectorRegistry::new(vec![Arc::new(CapturingConnector {
            descriptor,
            runtime_builds: Arc::new(AtomicUsize::new(0)),
        })]) {
            Ok(_) => panic!("invalid action ids must fail connector registration"),
            Err(error) => error,
        };
        let invalid = invalid.to_string();
        assert!(invalid.contains("declares an invalid descriptor"));
        assert!(invalid.contains("action 'Invalid Action' is invalid"));
        assert!(invalid.contains("action id must be 3-128 lowercase ASCII"));

        let mut descriptor = builtin_storage_connector_registry()
            .expect("built-in connector registry")
            .require_connector(&aster_drive_storage::ConnectorId::declared(
                "asterdrive.storage.local",
            ))
            .expect("local connector")
            .descriptor();
        descriptor.actions.push(descriptor.actions[0].clone());
        let duplicate = match StorageConnectorRegistry::new(vec![Arc::new(CapturingConnector {
            descriptor,
            runtime_builds: Arc::new(AtomicUsize::new(0)),
        })]) {
            Ok(_) => panic!("duplicate action ids must fail connector registration"),
            Err(error) => error,
        };
        let duplicate = duplicate.to_string();
        assert!(duplicate.contains("declares an invalid descriptor"));
        assert!(duplicate.contains("action 'test_draft_connection' is declared more than once"));
    }

    #[test]
    fn action_descriptor_lookup_is_namespaced_by_connector_id() {
        let base = builtin_storage_connector_registry()
            .expect("built-in connector registry")
            .require_connector(&aster_drive_storage::ConnectorId::declared(
                "asterdrive.storage.local",
            ))
            .expect("local connector")
            .descriptor();
        let shared_action_id =
            aster_drive_storage::StorageConnectorActionId::declared("plugin.verify_path");

        let mut first = base.clone();
        first.connector_id = aster_drive_storage::ConnectorId::declared("com.example.first");
        first.actions = vec![aster_drive_storage::custom_action_descriptor(
            aster_drive_storage::StorageConnectorCustomActionDescriptorInput {
                action_id: shared_action_id.clone(),
                label_key: "first.verify_path",
                description_key: "first.verify_path_desc",
                fields: Vec::new(),
                supports_draft: true,
                supports_saved: false,
                requires_authorization: false,
                mutates_remote_state: false,
                requires_confirmation: false,
            },
        )];

        let mut second = base;
        second.connector_id = aster_drive_storage::ConnectorId::declared("com.example.second");
        second.actions = vec![aster_drive_storage::custom_action_descriptor(
            aster_drive_storage::StorageConnectorCustomActionDescriptorInput {
                action_id: shared_action_id.clone(),
                label_key: "second.verify_path",
                description_key: "second.verify_path_desc",
                fields: Vec::new(),
                supports_draft: false,
                supports_saved: true,
                requires_authorization: true,
                mutates_remote_state: true,
                requires_confirmation: true,
            },
        )];

        let registry = StorageConnectorRegistry::new(vec![
            Arc::new(CapturingConnector {
                descriptor: first,
                runtime_builds: Arc::new(AtomicUsize::new(0)),
            }),
            Arc::new(CapturingConnector {
                descriptor: second,
                runtime_builds: Arc::new(AtomicUsize::new(0)),
            }),
        ])
        .expect("the same action id may be declared by different connectors");

        let first = registry
            .action_descriptor(
                &aster_drive_storage::ConnectorId::declared("com.example.first"),
                &shared_action_id,
            )
            .expect("first connector lookup")
            .expect("first action");
        let second = registry
            .action_descriptor(
                &aster_drive_storage::ConnectorId::declared("com.example.second"),
                &shared_action_id,
            )
            .expect("second connector lookup")
            .expect("second action");

        assert_eq!(first.label_key, "first.verify_path");
        assert!(!first.mutates_remote_state);
        assert_eq!(second.label_key, "second.verify_path");
        assert!(second.mutates_remote_state);
        assert!(
            registry
                .action_descriptor(
                    &aster_drive_storage::ConnectorId::declared("com.example.first"),
                    &aster_drive_storage::StorageConnectorActionId::declared("plugin.missing"),
                )
                .expect("known connector lookup")
                .is_none()
        );
    }

    #[test]
    fn metrics_enabled_driver_is_wrapped_once_and_cached() {
        let registry = DriverRegistry::new(Arc::new(CapturingMetrics::default()))
            .expect("built-in storage connector registry");
        let policy = local_policy();

        let driver1 = registry
            .get_driver(&policy)
            .expect("local driver should be created");
        let driver2 = registry
            .get_driver(&policy)
            .expect("cached local driver should be returned");

        assert!(
            Arc::ptr_eq(&driver1, &driver2),
            "metrics wrapper should be cached with the driver entry"
        );
    }

    #[test]
    fn uncached_driver_build_does_not_populate_shared_cache() {
        let registry = DriverRegistry::new(Arc::new(CapturingMetrics::default()))
            .expect("built-in storage connector registry");
        let policy = local_policy();

        let uncached = registry
            .build_uncached_driver(&policy)
            .expect("uncached local driver should be created");

        assert!(
            !registry.has_cached_driver_for_test(policy.id),
            "uncached construction must not insert a shared registry entry"
        );

        let cached = registry
            .get_driver(&policy)
            .expect("cached local driver should be created separately");
        assert!(
            !Arc::ptr_eq(&uncached, &cached),
            "the later shared-cache lookup should not reuse the task-local driver"
        );
        assert!(
            registry.has_cached_driver_for_test(policy.id),
            "normal get_driver should still populate the shared cache"
        );
    }

    #[test]
    fn cached_driver_lookup_is_read_only() {
        let registry = DriverRegistry::new(Arc::new(CapturingMetrics::default()))
            .expect("built-in storage connector registry");
        let policy = local_policy();

        assert!(
            registry.get_cached_driver(policy.id).is_none(),
            "cold cache lookup must not construct a driver"
        );
        assert!(
            !registry.has_cached_driver_for_test(policy.id),
            "cold cache lookup must leave the shared cache empty"
        );

        let cached = registry
            .get_driver(&policy)
            .expect("driver should be cached by normal lookup");
        let cached_lookup = registry
            .get_cached_driver(policy.id)
            .expect("cached lookup should return the existing driver");
        assert!(Arc::ptr_eq(&cached, &cached_lookup));
    }

    #[tokio::test]
    async fn metrics_enabled_multipart_driver_records_operations() {
        let metrics = Arc::new(CapturingMetrics::default());
        let registry =
            DriverRegistry::new(metrics.clone()).expect("built-in storage connector registry");
        let policy = remote_policy(Some(7));
        let driver = Arc::new(TestMultipartDriver);
        let storage: Arc<dyn StorageDriver> = driver.clone();
        let multipart: Arc<dyn MultipartStorageDriver> = driver;

        registry.drivers.insert(
            policy.id,
            registry.build_entry(STORAGE_CONNECTOR_METRIC_LABEL, storage, Some(multipart)),
        );

        let multipart_driver = registry
            .get_multipart_driver(&policy)
            .expect("test multipart driver should be available");
        let upload_id = multipart_driver
            .create_multipart_upload("object.bin")
            .await
            .expect("multipart create should succeed");
        let error = multipart_driver
            .abort_multipart_upload("object.bin", &upload_id)
            .await
            .expect_err("abort should fail for test driver");

        assert_eq!(error.kind(), StorageErrorKind::NotFound);
        assert_eq!(
            metrics.storage_operations.lock().as_slice(),
            &["create_multipart_upload", "abort_multipart_upload"]
        );
    }

    #[test]
    fn remote_policy_requires_remote_node_id() {
        let registry = DriverRegistry::noop().expect("built-in storage connector registry");

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
        let registry = DriverRegistry::noop().expect("built-in storage connector registry");

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
        assert_eq!(
            error.api_error_code_override(),
            Some(ApiErrorCode::RemoteNodeDisabled)
        );
        assert!(error.message().contains("remote node #7 is disabled"));
    }

    #[tokio::test]
    async fn remote_policy_resolves_enabled_follower_driver_capabilities() {
        let registry = registry_with_follower(managed_follower(true));
        let policy = remote_policy(Some(7));

        let driver = registry
            .get_driver(&policy)
            .expect("enabled follower should create remote driver");

        assert!(driver.extensions().list.is_some());
        assert!(driver.extensions().stream_upload.is_some());
        assert!(driver.extensions().presigned.is_some());
        assert!(driver.extensions().multipart.is_some());

        let extensions = driver.extensions();
        assert!(extensions.list.is_some());
        assert!(extensions.stream_upload.is_some());
        assert!(extensions.presigned.is_some());
        assert!(extensions.multipart.is_some());

        let presigned = driver
            .extensions()
            .presigned
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
        policy.storage_config = crate::storage::connectors::test_support::remote_policy(
            "base",
            Some(7),
            RemoteDownloadStrategy::Presigned,
            RemoteUploadStrategy::RelayStream,
        )
        .storage_config;

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
