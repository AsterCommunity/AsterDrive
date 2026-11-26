//! 运行时模块导出。

pub mod assembly;
mod audit_recovery;
pub mod components;
pub mod follower_health;
pub mod role;
pub mod shutdown;
pub mod startup;
pub mod tasks;
#[cfg(test)]
pub(crate) mod test_logs;

use crate::config::{Config, RuntimeConfig};
use crate::services::events::storage_change::StorageChangeBus;
use crate::storage::{DriverRegistry, PolicySnapshot, remote_protocol::RemoteProtocolRuntime};
use aster_drive_metrics::SharedMetricsRecorder;
use aster_forge_db::DbHandles;
use aster_forge_mail::MailSender;
use sea_orm::DatabaseConnection;
use std::sync::Arc;

/// Shared application state for a primary node process; cheap to clone and
/// shared by all HTTP workers and background services.
#[derive(Clone)]
pub struct PrimaryAppState {
    /// Writer/reader database handles; writes go through the writer handle.
    pub db_handles: DbHandles,
    /// Buffered audit writer that persists audit events outside request paths.
    pub audit_writer: Arc<aster_forge_audit::AuditWriter>,
    /// Registered storage connectors used to resolve and build storage drivers.
    pub driver_registry: Arc<DriverRegistry>,
    /// Hot-reloadable runtime configuration, distinct from the static file config.
    pub runtime_config: Arc<RuntimeConfig>,
    /// Snapshot of current storage policies used for quota and policy checks.
    pub policy_snapshot: Arc<PolicySnapshot>,
    /// Static configuration loaded at startup.
    pub config: Arc<Config>,
    /// Rebuildable cache projection; never authoritative for permissions or quotas.
    pub cache: Arc<dyn aster_forge_cache::CacheBackend>,
    /// Cross-instance config synchronization runtime.
    pub config_sync: aster_forge_config::ConfigSyncRuntime,
    /// Shared metrics recorder for request and service instrumentation.
    pub metrics: SharedMetricsRecorder,
    /// Outbound mail sender; implementations decide actual delivery transport.
    pub mail_sender: Arc<dyn MailSender>,
    /// Local storage-change broadcast with an optional cross-instance transport.
    pub storage_change_bus: StorageChangeBus,
    /// Admission registry enforcing per-account and per-instance limits for
    /// storage event streams.
    pub storage_event_streams: crate::services::events::storage_streams::StorageEventStreams,
    /// 后台任务 dispatcher 唤醒信号。任务创建/重试后用它打断空闲退避 sleep。
    pub background_task_dispatch_wakeup: aster_forge_tasks::TaskWakeup,
    /// Remote storage protocol runtime, including reverse tunnel state.
    pub remote_protocol: Arc<RemoteProtocolRuntime>,
}

/// Shared application state for a follower node process; cheap to clone and
/// shared by all HTTP workers.
#[derive(Clone)]
pub struct FollowerAppState {
    /// Process-local evidence of recent control-plane communication with the primary.
    pub binding_health: Arc<follower_health::FollowerBindingHealth>,
    /// Writer/reader database handles; writes go through the writer handle.
    pub db_handles: DbHandles,
    /// Buffered audit writer that persists audit events outside request paths.
    pub audit_writer: Arc<aster_forge_audit::AuditWriter>,
    /// Registered storage connectors used to resolve and build storage drivers.
    pub driver_registry: Arc<DriverRegistry>,
    /// Hot-reloadable runtime configuration, distinct from the static file config.
    pub runtime_config: Arc<RuntimeConfig>,
    /// Snapshot of current storage policies used for quota and policy checks.
    pub policy_snapshot: Arc<PolicySnapshot>,
    /// Static configuration loaded at startup.
    pub config: Arc<Config>,
    /// Rebuildable cache projection; never authoritative for permissions or quotas.
    pub cache: Arc<dyn aster_forge_cache::CacheBackend>,
    /// Cross-instance config synchronization runtime.
    pub config_sync: aster_forge_config::ConfigSyncRuntime,
    /// Shared metrics recorder for request and service instrumentation.
    pub metrics: SharedMetricsRecorder,
}

/// Runtime dependencies required by storage connector configuration workflows.
///
/// This boundary is limited to persistence, connector lookup, and the two
/// configuration views needed to construct or reload a connector. Connector
/// workflows must not inherit reader projections, cache, metrics, or unrelated
/// product services from the concrete application state.
pub trait StorageConnectorRuntimeState {
    /// Database connection used for transactional writes and read-after-write reads.
    fn writer_db(&self) -> &DatabaseConnection;

    /// Registry for looking up and building storage connectors.
    fn driver_registry(&self) -> &Arc<DriverRegistry>;

    /// Hot-reloadable runtime configuration.
    fn runtime_config(&self) -> &Arc<RuntimeConfig>;

    /// Static startup configuration.
    fn config(&self) -> &Arc<Config>;
}

/// Product runtime dependencies shared by primary and follower nodes.
///
/// Storage connector workflows should prefer StorageConnectorRuntimeState;
/// this broader contract is for use cases that also need read projections or
/// other product-wide runtime services.
pub trait SharedRuntimeState: StorageConnectorRuntimeState {
    /// Buffered audit writer for emitting audit events.
    fn audit_writer(&self) -> &Arc<aster_forge_audit::AuditWriter>;

    /// Database connection for reads that tolerate normal replication lag.
    fn reader_db(&self) -> &DatabaseConnection;

    /// Snapshot of current storage policies.
    fn policy_snapshot(&self) -> &Arc<PolicySnapshot>;

    /// Rebuildable cache projection.
    fn cache(&self) -> &Arc<dyn aster_forge_cache::CacheBackend>;

    /// Cross-instance config synchronization runtime.
    fn config_sync(&self) -> &aster_forge_config::ConfigSyncRuntime;

    /// Shared metrics recorder.
    fn metrics(&self) -> &SharedMetricsRecorder;
}

/// Runtime contract for mail-sending workflows.
pub trait MailRuntimeState: SharedRuntimeState {
    /// Outbound mail sender for product mail workflows.
    fn mail_sender(&self) -> &Arc<dyn MailSender>;
}

/// Runtime contract for workflows that publish storage change events.
pub trait StorageChangeRuntimeState: SharedRuntimeState {
    /// Broadcast bus that fans storage changes out to subscribers.
    fn storage_change_bus(&self) -> &StorageChangeBus;
}

/// Runtime contract for remote node protocol workflows.
pub trait RemoteProtocolRuntimeState: SharedRuntimeState {
    /// Runtime backing remote storage protocol traffic, including reverse tunnels.
    fn remote_protocol(&self) -> &Arc<RemoteProtocolRuntime>;
}

/// Runtime contract for background task execution workflows.
pub trait TaskRuntimeState: SharedRuntimeState {
    /// Wakeup signal that interrupts background task idle backoff.
    fn task_wakeup(&self) -> &aster_forge_tasks::TaskWakeup;
}

/// Runtime contract for follower node workflows.
pub trait FollowerRuntimeState: SharedRuntimeState {
    /// Process-local evidence of recent primary control-plane communication.
    fn binding_health(&self) -> &follower_health::FollowerBindingHealth;
}

impl PrimaryAppState {
    /// Creates the remote protocol runtime handed to the driver registry;
    /// one instance per process.
    pub fn new_remote_protocol() -> Arc<RemoteProtocolRuntime> {
        Arc::new(RemoteProtocolRuntime::new())
    }

    /// Whether SQLite storage is configured with separate reader/writer handles.
    pub fn sqlite_read_write_split(&self) -> bool {
        self.db_handles.sqlite_read_write_split()
    }

    /// Database connection for transactional writes and read-after-write reads.
    pub fn writer_db(&self) -> &DatabaseConnection {
        self.db_handles.writer()
    }

    /// Registry for looking up and building storage connectors.
    pub fn driver_registry(&self) -> &Arc<DriverRegistry> {
        &self.driver_registry
    }

    /// Hot-reloadable runtime configuration.
    pub fn runtime_config(&self) -> &Arc<RuntimeConfig> {
        &self.runtime_config
    }

    /// Static startup configuration.
    pub fn config(&self) -> &Arc<Config> {
        &self.config
    }

    /// Cross-instance config synchronization runtime.
    pub fn config_sync(&self) -> &aster_forge_config::ConfigSyncRuntime {
        &self.config_sync
    }

    /// Projects this primary state into follower-shaped state sharing the same
    /// dependencies; follower-only fields (e.g. binding health) start empty.
    pub fn follower_view(&self) -> FollowerAppState {
        FollowerAppState::from(self)
    }
}

impl From<&PrimaryAppState> for FollowerAppState {
    fn from(state: &PrimaryAppState) -> Self {
        Self {
            binding_health: Arc::default(),
            db_handles: state.db_handles.clone(),
            audit_writer: state.audit_writer.clone(),
            driver_registry: state.driver_registry.clone(),
            runtime_config: state.runtime_config.clone(),
            policy_snapshot: state.policy_snapshot.clone(),
            config: state.config.clone(),
            cache: state.cache.clone(),
            config_sync: state.config_sync.clone(),
            metrics: state.metrics.clone(),
        }
    }
}

impl FollowerAppState {
    /// Whether SQLite storage is configured with separate reader/writer handles.
    pub fn sqlite_read_write_split(&self) -> bool {
        self.db_handles.sqlite_read_write_split()
    }

    /// Database connection for transactional writes and read-after-write reads.
    pub fn writer_db(&self) -> &DatabaseConnection {
        self.db_handles.writer()
    }

    /// Registry for looking up and building storage connectors.
    pub fn driver_registry(&self) -> &Arc<DriverRegistry> {
        &self.driver_registry
    }

    /// Hot-reloadable runtime configuration.
    pub fn runtime_config(&self) -> &Arc<RuntimeConfig> {
        &self.runtime_config
    }

    /// Static startup configuration.
    pub fn config(&self) -> &Arc<Config> {
        &self.config
    }

    /// Cross-instance config synchronization runtime.
    pub fn config_sync(&self) -> &aster_forge_config::ConfigSyncRuntime {
        &self.config_sync
    }
}

impl StorageConnectorRuntimeState for PrimaryAppState {
    fn writer_db(&self) -> &DatabaseConnection {
        self.db_handles.writer()
    }

    fn driver_registry(&self) -> &Arc<DriverRegistry> {
        &self.driver_registry
    }

    fn runtime_config(&self) -> &Arc<RuntimeConfig> {
        &self.runtime_config
    }

    fn config(&self) -> &Arc<Config> {
        &self.config
    }
}

impl SharedRuntimeState for PrimaryAppState {
    fn audit_writer(&self) -> &Arc<aster_forge_audit::AuditWriter> {
        &self.audit_writer
    }

    fn reader_db(&self) -> &DatabaseConnection {
        self.db_handles.reader()
    }

    fn policy_snapshot(&self) -> &Arc<PolicySnapshot> {
        &self.policy_snapshot
    }

    fn cache(&self) -> &Arc<dyn aster_forge_cache::CacheBackend> {
        &self.cache
    }

    fn config_sync(&self) -> &aster_forge_config::ConfigSyncRuntime {
        &self.config_sync
    }

    fn metrics(&self) -> &SharedMetricsRecorder {
        &self.metrics
    }
}

impl MailRuntimeState for PrimaryAppState {
    fn mail_sender(&self) -> &Arc<dyn MailSender> {
        &self.mail_sender
    }
}

impl StorageChangeRuntimeState for PrimaryAppState {
    fn storage_change_bus(&self) -> &StorageChangeBus {
        &self.storage_change_bus
    }
}

impl RemoteProtocolRuntimeState for PrimaryAppState {
    fn remote_protocol(&self) -> &Arc<RemoteProtocolRuntime> {
        &self.remote_protocol
    }
}

impl TaskRuntimeState for PrimaryAppState {
    fn task_wakeup(&self) -> &aster_forge_tasks::TaskWakeup {
        &self.background_task_dispatch_wakeup
    }
}

impl StorageConnectorRuntimeState for FollowerAppState {
    fn writer_db(&self) -> &DatabaseConnection {
        self.db_handles.writer()
    }

    fn driver_registry(&self) -> &Arc<DriverRegistry> {
        &self.driver_registry
    }

    fn runtime_config(&self) -> &Arc<RuntimeConfig> {
        &self.runtime_config
    }

    fn config(&self) -> &Arc<Config> {
        &self.config
    }
}

impl SharedRuntimeState for FollowerAppState {
    fn audit_writer(&self) -> &Arc<aster_forge_audit::AuditWriter> {
        &self.audit_writer
    }

    fn reader_db(&self) -> &DatabaseConnection {
        self.db_handles.reader()
    }

    fn policy_snapshot(&self) -> &Arc<PolicySnapshot> {
        &self.policy_snapshot
    }

    fn cache(&self) -> &Arc<dyn aster_forge_cache::CacheBackend> {
        &self.cache
    }

    fn config_sync(&self) -> &aster_forge_config::ConfigSyncRuntime {
        &self.config_sync
    }

    fn metrics(&self) -> &SharedMetricsRecorder {
        &self.metrics
    }
}

impl FollowerRuntimeState for FollowerAppState {
    fn binding_health(&self) -> &follower_health::FollowerBindingHealth {
        &self.binding_health
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::{SharedRuntimeState, StorageConnectorRuntimeState};
    use crate::config::{Config, RuntimeConfig};
    use crate::storage::{DriverRegistry, PolicySnapshot};
    use aster_drive_metrics::SharedMetricsRecorder;
    use aster_forge_cache::CacheConfig;
    use sea_orm::DatabaseConnection;
    use std::sync::Arc;

    pub(crate) fn initialize_jwt_crypto_provider() {
        static INIT: std::sync::Once = std::sync::Once::new();
        INIT.call_once(|| {
            aster_forge_auth::initialize_jwt_crypto_provider()
                .expect("test JWT crypto provider should initialize");
        });
    }

    pub(crate) async fn audit_writer(
        db: &DatabaseConnection,
    ) -> Arc<aster_forge_audit::AuditWriter> {
        let writer = Arc::new(aster_forge_audit::AuditWriter::new(db.clone()));
        writer
            .start()
            .await
            .expect("test audit writer should start");
        writer
    }

    pub(crate) struct CacheOnlyState {
        cache: Arc<dyn aster_forge_cache::CacheBackend>,
        config: Arc<Config>,
        config_sync: aster_forge_config::ConfigSyncRuntime,
    }

    impl CacheOnlyState {
        pub(crate) async fn new() -> Self {
            Self {
                cache: aster_forge_cache::create_cache(&CacheConfig {
                    backend: "memory".to_string(),
                    ..Default::default()
                })
                .await,
                config: Arc::new(Config::default()),
                config_sync: aster_forge_config::ConfigSyncRuntime::disabled_for_test(
                    "aster_drive",
                ),
            }
        }
    }

    impl StorageConnectorRuntimeState for CacheOnlyState {
        fn writer_db(&self) -> &DatabaseConnection {
            panic!("cache-only test state must not access writer_db")
        }

        fn driver_registry(&self) -> &Arc<DriverRegistry> {
            panic!("cache-only test state must not access driver_registry")
        }

        fn runtime_config(&self) -> &Arc<RuntimeConfig> {
            panic!("cache-only test state must not access runtime_config")
        }

        fn config(&self) -> &Arc<Config> {
            &self.config
        }
    }

    impl SharedRuntimeState for CacheOnlyState {
        fn audit_writer(&self) -> &Arc<aster_forge_audit::AuditWriter> {
            panic!("cache-only test state must not access audit_writer")
        }

        fn reader_db(&self) -> &DatabaseConnection {
            panic!("cache-only test state must not access reader_db")
        }

        fn policy_snapshot(&self) -> &Arc<PolicySnapshot> {
            panic!("cache-only test state must not access policy_snapshot")
        }

        fn cache(&self) -> &Arc<dyn aster_forge_cache::CacheBackend> {
            &self.cache
        }

        fn config_sync(&self) -> &aster_forge_config::ConfigSyncRuntime {
            &self.config_sync
        }

        fn metrics(&self) -> &SharedMetricsRecorder {
            panic!("cache-only test state must not access metrics")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{PrimaryAppState, SharedRuntimeState, StorageConnectorRuntimeState};
    use crate::config::{Config, RuntimeConfig};
    use crate::storage::{DriverRegistry, PolicySnapshot};
    use aster_drive_migration::Migrator;
    use aster_forge_cache::CacheConfig;
    use std::sync::Arc;

    async fn setup_state() -> PrimaryAppState {
        let db = crate::db::connect_with_metrics(
            &crate::config::DatabaseConfig {
                url: "sqlite::memory:".into(),
                pool_size: 1,
                retry_count: 0,
            },
            aster_drive_metrics::NoopMetrics::arc(),
        )
        .await
        .unwrap();
        Migrator::up(&db, None).await.unwrap();

        let cache = aster_forge_cache::create_cache(&CacheConfig {
            ..Default::default()
        })
        .await;
        let runtime_config = Arc::new(RuntimeConfig::new());
        let storage_change_bus = crate::services::events::storage_change::StorageChangeBus::new(
            crate::services::events::storage_change::STORAGE_CHANGE_CHANNEL_CAPACITY,
        );

        let state = PrimaryAppState {
            audit_writer: crate::runtime::test_support::audit_writer(&db).await,
            db_handles: aster_forge_db::DbHandles::single(db),
            driver_registry: Arc::new(
                DriverRegistry::noop().expect("built-in storage connector registry"),
            ),
            runtime_config,
            policy_snapshot: Arc::new(PolicySnapshot::new()),
            config: Arc::new(Config::default()),
            cache,
            config_sync: aster_forge_config::ConfigSyncRuntime::disabled_for_test("aster_drive"),
            metrics: aster_drive_metrics::NoopMetrics::arc(),
            mail_sender: aster_forge_mail::memory_sender(),
            storage_change_bus,
            storage_event_streams: Default::default(),
            background_task_dispatch_wakeup: aster_forge_tasks::TaskWakeup::default(),
            remote_protocol: crate::runtime::PrimaryAppState::new_remote_protocol(),
        };
        state
            .driver_registry
            .set_remote_protocol(state.remote_protocol.clone());
        state
    }

    #[tokio::test]
    async fn primary_state_follower_view_shares_runtime_dependencies() {
        let state = setup_state().await;
        let follower = state.follower_view();

        assert!(Arc::ptr_eq(
            &state.driver_registry,
            follower.driver_registry()
        ));
        assert!(Arc::ptr_eq(
            &state.runtime_config,
            follower.runtime_config()
        ));
        assert!(Arc::ptr_eq(
            &state.policy_snapshot,
            follower.policy_snapshot()
        ));
        assert!(Arc::ptr_eq(&state.config, follower.config()));
        assert!(Arc::ptr_eq(&state.cache, follower.cache()));
        assert_eq!(
            state.writer_db().get_database_backend(),
            follower.writer_db().get_database_backend()
        );
        assert_eq!(
            state.reader_db().get_database_backend(),
            follower.reader_db().get_database_backend()
        );
        assert_eq!(
            StorageConnectorRuntimeState::writer_db(&state).get_database_backend(),
            StorageConnectorRuntimeState::writer_db(&follower).get_database_backend()
        );
        assert_eq!(
            SharedRuntimeState::reader_db(&state).get_database_backend(),
            SharedRuntimeState::reader_db(&follower).get_database_backend()
        );
    }
}
