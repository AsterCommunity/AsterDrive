//! 运行时模块导出。

pub mod shutdown;
pub mod startup;
pub mod tasks;

use crate::config::{Config, RuntimeConfig};
use crate::db::DbHandles;
use crate::metrics::SharedMetricsRecorder;
use crate::services::{
    events::storage_change::StorageChangeEvent, mail::sender::MailSender,
    share::ShareDownloadRollbackQueue,
};
use crate::storage::{DriverRegistry, PolicySnapshot, remote_protocol::RemoteProtocolRuntime};
use sea_orm::DatabaseConnection;
use std::sync::Arc;
use tokio::sync::Notify;

#[derive(Clone)]
pub struct PrimaryAppState {
    pub db_handles: DbHandles,
    pub driver_registry: Arc<DriverRegistry>,
    pub runtime_config: Arc<RuntimeConfig>,
    pub policy_snapshot: Arc<PolicySnapshot>,
    pub config: Arc<Config>,
    pub cache: Arc<dyn aster_forge_cache::CacheBackend>,
    pub metrics: SharedMetricsRecorder,
    pub mail_sender: Arc<dyn MailSender>,
    /// 文件/文件夹变更广播（SSE 消费）
    pub storage_change_tx: tokio::sync::broadcast::Sender<StorageChangeEvent>,
    /// 公开分享下载中途断连时的 download_count 回滚队列
    pub share_download_rollback: ShareDownloadRollbackQueue,
    /// 后台任务 dispatcher 唤醒信号。任务创建/重试后用它打断空闲退避 sleep。
    pub background_task_dispatch_wakeup: Arc<Notify>,
    /// Remote storage protocol runtime, including reverse tunnel state.
    pub remote_protocol: Arc<RemoteProtocolRuntime>,
}

#[derive(Clone)]
pub struct FollowerAppState {
    pub db_handles: DbHandles,
    pub driver_registry: Arc<DriverRegistry>,
    pub runtime_config: Arc<RuntimeConfig>,
    pub policy_snapshot: Arc<PolicySnapshot>,
    pub config: Arc<Config>,
    pub cache: Arc<dyn aster_forge_cache::CacheBackend>,
    pub metrics: SharedMetricsRecorder,
}

pub trait SharedRuntimeState {
    fn writer_db(&self) -> &DatabaseConnection;
    fn reader_db(&self) -> &DatabaseConnection;
    fn driver_registry(&self) -> &Arc<DriverRegistry>;
    fn runtime_config(&self) -> &Arc<RuntimeConfig>;
    fn policy_snapshot(&self) -> &Arc<PolicySnapshot>;
    fn config(&self) -> &Arc<Config>;
    fn cache(&self) -> &Arc<dyn aster_forge_cache::CacheBackend>;
    fn metrics(&self) -> &SharedMetricsRecorder;
}

pub trait MailRuntimeState: SharedRuntimeState {
    fn mail_sender(&self) -> &Arc<dyn MailSender>;
}

pub trait StorageChangeRuntimeState: SharedRuntimeState {
    fn storage_change_tx(&self) -> &tokio::sync::broadcast::Sender<StorageChangeEvent>;
}

pub trait ShareDownloadRuntimeState: SharedRuntimeState {
    fn share_download_rollback(&self) -> &ShareDownloadRollbackQueue;
}

pub trait RemoteProtocolRuntimeState: SharedRuntimeState {
    fn remote_protocol(&self) -> &Arc<RemoteProtocolRuntime>;
}

pub trait TaskRuntimeState: SharedRuntimeState {
    fn wake_background_task_dispatcher(&self);
}

pub trait FollowerRuntimeState: SharedRuntimeState {}

impl PrimaryAppState {
    pub fn new_background_task_dispatch_wakeup() -> Arc<Notify> {
        Arc::new(Notify::new())
    }

    pub fn new_remote_protocol() -> Arc<RemoteProtocolRuntime> {
        Arc::new(RemoteProtocolRuntime::new())
    }

    pub fn sqlite_read_write_split(&self) -> bool {
        self.db_handles.sqlite_read_write_split()
    }

    pub fn should_record_audit_action(&self, action: crate::types::AuditAction) -> bool {
        self.runtime_config.should_record_audit_action(action)
    }

    pub fn follower_view(&self) -> FollowerAppState {
        FollowerAppState::from(self)
    }
}

impl From<&PrimaryAppState> for FollowerAppState {
    fn from(state: &PrimaryAppState) -> Self {
        Self {
            db_handles: state.db_handles.clone(),
            driver_registry: state.driver_registry.clone(),
            runtime_config: state.runtime_config.clone(),
            policy_snapshot: state.policy_snapshot.clone(),
            config: state.config.clone(),
            cache: state.cache.clone(),
            metrics: state.metrics.clone(),
        }
    }
}

impl FollowerAppState {
    pub fn sqlite_read_write_split(&self) -> bool {
        self.db_handles.sqlite_read_write_split()
    }
}

impl SharedRuntimeState for PrimaryAppState {
    fn writer_db(&self) -> &DatabaseConnection {
        self.db_handles.writer()
    }

    fn reader_db(&self) -> &DatabaseConnection {
        self.db_handles.reader()
    }

    fn driver_registry(&self) -> &Arc<DriverRegistry> {
        &self.driver_registry
    }

    fn runtime_config(&self) -> &Arc<RuntimeConfig> {
        &self.runtime_config
    }

    fn policy_snapshot(&self) -> &Arc<PolicySnapshot> {
        &self.policy_snapshot
    }

    fn config(&self) -> &Arc<Config> {
        &self.config
    }

    fn cache(&self) -> &Arc<dyn aster_forge_cache::CacheBackend> {
        &self.cache
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
    fn storage_change_tx(&self) -> &tokio::sync::broadcast::Sender<StorageChangeEvent> {
        &self.storage_change_tx
    }
}

impl ShareDownloadRuntimeState for PrimaryAppState {
    fn share_download_rollback(&self) -> &ShareDownloadRollbackQueue {
        &self.share_download_rollback
    }
}

impl RemoteProtocolRuntimeState for PrimaryAppState {
    fn remote_protocol(&self) -> &Arc<RemoteProtocolRuntime> {
        &self.remote_protocol
    }
}

impl TaskRuntimeState for PrimaryAppState {
    fn wake_background_task_dispatcher(&self) {
        self.background_task_dispatch_wakeup.notify_one();
    }
}

impl SharedRuntimeState for FollowerAppState {
    fn writer_db(&self) -> &DatabaseConnection {
        self.db_handles.writer()
    }

    fn reader_db(&self) -> &DatabaseConnection {
        self.db_handles.reader()
    }

    fn driver_registry(&self) -> &Arc<DriverRegistry> {
        &self.driver_registry
    }

    fn runtime_config(&self) -> &Arc<RuntimeConfig> {
        &self.runtime_config
    }

    fn policy_snapshot(&self) -> &Arc<PolicySnapshot> {
        &self.policy_snapshot
    }

    fn config(&self) -> &Arc<Config> {
        &self.config
    }

    fn cache(&self) -> &Arc<dyn aster_forge_cache::CacheBackend> {
        &self.cache
    }

    fn metrics(&self) -> &SharedMetricsRecorder {
        &self.metrics
    }
}

impl FollowerRuntimeState for FollowerAppState {}

#[cfg(test)]
pub(crate) mod test_support {
    use super::SharedRuntimeState;
    use crate::config::{CacheConfig, Config, RuntimeConfig};
    use crate::metrics::SharedMetricsRecorder;
    use crate::storage::{DriverRegistry, PolicySnapshot};
    use sea_orm::DatabaseConnection;
    use std::sync::Arc;

    pub(crate) struct CacheOnlyState {
        cache: Arc<dyn aster_forge_cache::CacheBackend>,
    }

    impl CacheOnlyState {
        pub(crate) async fn new() -> Self {
            Self {
                cache: aster_forge_cache::create_cache(&CacheConfig {
                    backend: "memory".to_string(),
                    ..Default::default()
                })
                .await,
            }
        }
    }

    impl SharedRuntimeState for CacheOnlyState {
        fn writer_db(&self) -> &DatabaseConnection {
            panic!("cache-only test state must not access writer_db")
        }

        fn reader_db(&self) -> &DatabaseConnection {
            panic!("cache-only test state must not access reader_db")
        }

        fn driver_registry(&self) -> &Arc<DriverRegistry> {
            panic!("cache-only test state must not access driver_registry")
        }

        fn runtime_config(&self) -> &Arc<RuntimeConfig> {
            panic!("cache-only test state must not access runtime_config")
        }

        fn policy_snapshot(&self) -> &Arc<PolicySnapshot> {
            panic!("cache-only test state must not access policy_snapshot")
        }

        fn config(&self) -> &Arc<Config> {
            panic!("cache-only test state must not access config")
        }

        fn cache(&self) -> &Arc<dyn aster_forge_cache::CacheBackend> {
            &self.cache
        }

        fn metrics(&self) -> &SharedMetricsRecorder {
            panic!("cache-only test state must not access metrics")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{PrimaryAppState, SharedRuntimeState, TaskRuntimeState};
    use crate::config::{CacheConfig, Config, RuntimeConfig};
    use crate::services::share::build_share_download_rollback_queue;
    use crate::storage::{DriverRegistry, PolicySnapshot};
    use migration::Migrator;
    use std::sync::Arc;

    async fn setup_state() -> PrimaryAppState {
        let db = crate::db::connect_with_metrics(
            &crate::config::DatabaseConfig {
                url: "sqlite::memory:".to_string(),
                pool_size: 1,
                retry_count: 0,
            },
            crate::metrics::NoopMetrics::arc(),
        )
        .await
        .unwrap();
        Migrator::up(&db, None).await.unwrap();

        let cache = aster_forge_cache::create_cache(&CacheConfig {
            ..Default::default()
        })
        .await;
        let runtime_config = Arc::new(RuntimeConfig::new());
        let (storage_change_tx, _) = tokio::sync::broadcast::channel(
            crate::services::events::storage_change::STORAGE_CHANGE_CHANNEL_CAPACITY,
        );
        let (share_download_rollback, _worker) =
            build_share_download_rollback_queue(db.clone(), 1, crate::metrics::NoopMetrics::arc());

        let state = PrimaryAppState {
            db_handles: crate::db::DbHandles::single(db),
            driver_registry: Arc::new(DriverRegistry::noop()),
            runtime_config,
            policy_snapshot: Arc::new(PolicySnapshot::new()),
            config: Arc::new(Config::default()),
            cache,
            metrics: crate::metrics::NoopMetrics::arc(),
            mail_sender: crate::services::mail::sender::memory_sender(),
            storage_change_tx,
            share_download_rollback,
            background_task_dispatch_wakeup:
                crate::runtime::PrimaryAppState::new_background_task_dispatch_wakeup(),
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
            SharedRuntimeState::writer_db(&state).get_database_backend(),
            SharedRuntimeState::writer_db(&follower).get_database_backend()
        );
        assert_eq!(
            SharedRuntimeState::reader_db(&state).get_database_backend(),
            SharedRuntimeState::reader_db(&follower).get_database_backend()
        );
    }

    #[tokio::test]
    async fn primary_state_wakes_background_task_dispatcher() {
        let state = setup_state().await;
        let notified = state.background_task_dispatch_wakeup.notified();

        state.wake_background_task_dispatcher();

        tokio::time::timeout(std::time::Duration::from_secs(1), notified)
            .await
            .expect("background dispatcher wakeup should notify waiters");
    }
}
