//! 运行时模块导出。

pub mod logging;
pub mod panic;
pub mod shutdown;
pub mod startup;
pub mod tasks;

use crate::cache::CacheBackend;
use crate::config::{Config, RuntimeConfig};
use crate::db::DbHandles;
use crate::metrics_core::SharedMetricsRecorder;
use crate::services::{
    mail_service::MailSender, share_service::ShareDownloadRollbackQueue,
    storage_change_service::StorageChangeEvent,
};
use crate::storage::{DriverRegistry, PolicySnapshot};
use actix_web::web;
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
    pub cache: Arc<dyn CacheBackend>,
    pub metrics: SharedMetricsRecorder,
    pub mail_sender: Arc<dyn MailSender>,
    /// 文件/文件夹变更广播（SSE 消费）
    pub storage_change_tx: tokio::sync::broadcast::Sender<StorageChangeEvent>,
    /// 公开分享下载中途断连时的 download_count 回滚队列
    pub share_download_rollback: ShareDownloadRollbackQueue,
    /// 后台任务 dispatcher 唤醒信号。任务创建/重试后用它打断空闲退避 sleep。
    pub background_task_dispatch_wakeup: Arc<Notify>,
}

#[derive(Clone)]
pub struct FollowerAppState {
    pub db_handles: DbHandles,
    pub driver_registry: Arc<DriverRegistry>,
    pub policy_snapshot: Arc<PolicySnapshot>,
    pub config: Arc<Config>,
    pub cache: Arc<dyn CacheBackend>,
    pub metrics: SharedMetricsRecorder,
}

pub trait SharedRuntimeState {
    fn writer_db(&self) -> &DatabaseConnection;
    fn reader_db(&self) -> &DatabaseConnection;
    fn driver_registry(&self) -> &Arc<DriverRegistry>;
    fn policy_snapshot(&self) -> &Arc<PolicySnapshot>;
    fn config(&self) -> &Arc<Config>;
    fn cache(&self) -> &Arc<dyn CacheBackend>;
    fn metrics(&self) -> &SharedMetricsRecorder;
}

pub trait PrimaryRuntimeState: SharedRuntimeState {
    fn runtime_config(&self) -> &Arc<RuntimeConfig>;
    fn mail_sender(&self) -> &Arc<dyn MailSender>;
    fn storage_change_tx(&self) -> &tokio::sync::broadcast::Sender<StorageChangeEvent>;
    fn share_download_rollback(&self) -> &ShareDownloadRollbackQueue;
}

pub trait FollowerRuntimeState: SharedRuntimeState {}

impl PrimaryAppState {
    pub fn new_background_task_dispatch_wakeup() -> Arc<Notify> {
        Arc::new(Notify::new())
    }

    pub fn writer_db(&self) -> &DatabaseConnection {
        self.db_handles.writer()
    }

    pub fn reader_db(&self) -> &DatabaseConnection {
        self.db_handles.reader()
    }

    pub fn sqlite_read_write_split(&self) -> bool {
        self.db_handles.sqlite_read_write_split()
    }

    pub fn wake_background_task_dispatcher(&self) {
        self.background_task_dispatch_wakeup.notify_one();
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
            policy_snapshot: state.policy_snapshot.clone(),
            config: state.config.clone(),
            cache: state.cache.clone(),
            metrics: state.metrics.clone(),
        }
    }
}

impl FollowerAppState {
    pub fn writer_db(&self) -> &DatabaseConnection {
        self.db_handles.writer()
    }

    pub fn reader_db(&self) -> &DatabaseConnection {
        self.db_handles.reader()
    }

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

    fn policy_snapshot(&self) -> &Arc<PolicySnapshot> {
        &self.policy_snapshot
    }

    fn config(&self) -> &Arc<Config> {
        &self.config
    }

    fn cache(&self) -> &Arc<dyn CacheBackend> {
        &self.cache
    }

    fn metrics(&self) -> &SharedMetricsRecorder {
        &self.metrics
    }
}

impl PrimaryRuntimeState for PrimaryAppState {
    fn runtime_config(&self) -> &Arc<RuntimeConfig> {
        &self.runtime_config
    }

    fn mail_sender(&self) -> &Arc<dyn MailSender> {
        &self.mail_sender
    }

    fn storage_change_tx(&self) -> &tokio::sync::broadcast::Sender<StorageChangeEvent> {
        &self.storage_change_tx
    }

    fn share_download_rollback(&self) -> &ShareDownloadRollbackQueue {
        &self.share_download_rollback
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

    fn policy_snapshot(&self) -> &Arc<PolicySnapshot> {
        &self.policy_snapshot
    }

    fn config(&self) -> &Arc<Config> {
        &self.config
    }

    fn cache(&self) -> &Arc<dyn CacheBackend> {
        &self.cache
    }

    fn metrics(&self) -> &SharedMetricsRecorder {
        &self.metrics
    }
}

impl FollowerRuntimeState for FollowerAppState {}

impl<T: SharedRuntimeState> SharedRuntimeState for web::Data<T> {
    fn writer_db(&self) -> &DatabaseConnection {
        self.get_ref().writer_db()
    }

    fn reader_db(&self) -> &DatabaseConnection {
        self.get_ref().reader_db()
    }

    fn driver_registry(&self) -> &Arc<DriverRegistry> {
        self.get_ref().driver_registry()
    }

    fn policy_snapshot(&self) -> &Arc<PolicySnapshot> {
        self.get_ref().policy_snapshot()
    }

    fn config(&self) -> &Arc<Config> {
        self.get_ref().config()
    }

    fn cache(&self) -> &Arc<dyn CacheBackend> {
        self.get_ref().cache()
    }

    fn metrics(&self) -> &SharedMetricsRecorder {
        self.get_ref().metrics()
    }
}

impl<T: PrimaryRuntimeState> PrimaryRuntimeState for web::Data<T> {
    fn runtime_config(&self) -> &Arc<RuntimeConfig> {
        self.get_ref().runtime_config()
    }

    fn mail_sender(&self) -> &Arc<dyn MailSender> {
        self.get_ref().mail_sender()
    }

    fn storage_change_tx(&self) -> &tokio::sync::broadcast::Sender<StorageChangeEvent> {
        self.get_ref().storage_change_tx()
    }

    fn share_download_rollback(&self) -> &ShareDownloadRollbackQueue {
        self.get_ref().share_download_rollback()
    }
}

impl<T: FollowerRuntimeState> FollowerRuntimeState for web::Data<T> {}

#[cfg(test)]
mod tests {
    use super::{FollowerRuntimeState, PrimaryAppState, PrimaryRuntimeState, SharedRuntimeState};
    use crate::config::{CacheConfig, Config, RuntimeConfig};
    use crate::services::share_service::build_share_download_rollback_queue;
    use crate::storage::{DriverRegistry, PolicySnapshot};
    use actix_web::web;
    use migration::Migrator;
    use std::sync::Arc;

    async fn setup_state() -> PrimaryAppState {
        let db = crate::db::connect(&crate::config::DatabaseConfig {
            url: "sqlite::memory:".to_string(),
            pool_size: 1,
            retry_count: 0,
        })
        .await
        .unwrap();
        Migrator::up(&db, None).await.unwrap();

        let cache = crate::cache::create_cache(&CacheConfig {
            enabled: false,
            ..Default::default()
        })
        .await;
        let runtime_config = Arc::new(RuntimeConfig::new());
        let (storage_change_tx, _) = tokio::sync::broadcast::channel(
            crate::services::storage_change_service::STORAGE_CHANGE_CHANNEL_CAPACITY,
        );
        let (share_download_rollback, _worker) = build_share_download_rollback_queue(
            db.clone(),
            1,
            crate::metrics_core::NoopMetrics::arc(),
        );

        PrimaryAppState {
            db_handles: crate::db::DbHandles::single(db),
            driver_registry: Arc::new(DriverRegistry::noop()),
            runtime_config,
            policy_snapshot: Arc::new(PolicySnapshot::new()),
            config: Arc::new(Config::default()),
            cache,
            metrics: crate::metrics_core::NoopMetrics::arc(),
            mail_sender: crate::services::mail_service::memory_sender(),
            storage_change_tx,
            share_download_rollback,
            background_task_dispatch_wakeup:
                crate::runtime::PrimaryAppState::new_background_task_dispatch_wakeup(),
        }
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
    async fn web_data_forwards_primary_runtime_state_traits() {
        let state = setup_state().await;
        let data = web::Data::new(state.clone());

        assert_eq!(
            SharedRuntimeState::writer_db(&data).get_database_backend(),
            state.writer_db().get_database_backend()
        );
        assert_eq!(
            SharedRuntimeState::writer_db(&data).get_database_backend(),
            state.writer_db().get_database_backend()
        );
        assert_eq!(
            SharedRuntimeState::reader_db(&data).get_database_backend(),
            state.reader_db().get_database_backend()
        );
        assert!(Arc::ptr_eq(&state.runtime_config, data.runtime_config()));
        assert!(Arc::ptr_eq(&state.mail_sender, data.mail_sender()));
        assert!(Arc::ptr_eq(&state.driver_registry, data.driver_registry()));
        assert!(Arc::ptr_eq(
            &state.policy_snapshot,
            SharedRuntimeState::policy_snapshot(&data)
        ));
        assert!(Arc::ptr_eq(
            &state.config,
            SharedRuntimeState::config(&data)
        ));
        assert_eq!(
            state.storage_change_tx.receiver_count(),
            data.storage_change_tx().receiver_count()
        );
        let _ = data.share_download_rollback();
    }

    #[tokio::test]
    async fn web_data_forwards_follower_runtime_state_trait() {
        fn assert_follower_state<S: FollowerRuntimeState>(state: &S) {
            assert_eq!(state.cache().backend_name(), "noop");
        }

        let follower = setup_state().await.follower_view();
        let data = web::Data::new(follower);
        assert_eq!(
            SharedRuntimeState::writer_db(&data).get_database_backend(),
            data.get_ref().writer_db().get_database_backend()
        );
        assert_eq!(
            SharedRuntimeState::reader_db(&data).get_database_backend(),
            data.get_ref().reader_db().get_database_backend()
        );
        assert_follower_state(&data);
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
