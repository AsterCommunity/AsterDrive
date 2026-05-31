use chrono::Utc;
use sea_orm::{DatabaseConnection, Set};
use std::sync::{
    Arc, OnceLock,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration as StdDuration;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::db::repository::audit_log_repo;
use crate::entities::audit_log;
use crate::runtime::PrimaryAppState;
use crate::types::{AuditAction, AuditEntityType};

use super::context::AuditContext;

pub(super) const AUDIT_LOG_QUEUE_CAPACITY: usize = 4096;
pub(super) const AUDIT_LOG_BATCH_SIZE: usize = 100;
const AUDIT_LOG_DELAYED_FLUSH_AFTER: StdDuration = StdDuration::from_secs(1);

static GLOBAL_AUDIT_LOG_MANAGER: OnceLock<Arc<AuditLogManager>> = OnceLock::new();

pub(super) struct AuditLogManager {
    db: DatabaseConnection,
    buffer: parking_lot::Mutex<Vec<audit_log::ActiveModel>>,
    flush_lock: Mutex<()>,
    flush_pending: AtomicBool,
    delayed_flush_pending: AtomicBool,
    delayed_flush_after: StdDuration,
    shutdown_token: CancellationToken,
}

struct FlushPendingReset {
    manager: Arc<AuditLogManager>,
    armed: bool,
}

impl Drop for FlushPendingReset {
    fn drop(&mut self) {
        if self.armed {
            self.manager.flush_pending.store(false, Ordering::Release);
        }
    }
}

impl FlushPendingReset {
    fn reset(&mut self) {
        self.manager.flush_pending.store(false, Ordering::Release);
        self.armed = false;
    }
}

struct DelayedFlushPendingReset {
    manager: Arc<AuditLogManager>,
    armed: bool,
}

impl Drop for DelayedFlushPendingReset {
    fn drop(&mut self) {
        if self.armed {
            self.manager
                .delayed_flush_pending
                .store(false, Ordering::Release);
        }
    }
}

impl DelayedFlushPendingReset {
    fn reset(&mut self) {
        self.manager
            .delayed_flush_pending
            .store(false, Ordering::Release);
        self.armed = false;
    }
}

pub fn init_global_audit_log_manager(db: DatabaseConnection) {
    let manager = Arc::new(AuditLogManager::new(db));
    match GLOBAL_AUDIT_LOG_MANAGER.set(manager) {
        Ok(()) => {}
        Err(_) => {
            tracing::warn!("global audit log manager is already initialized; ignoring");
        }
    }
}

pub async fn flush_global_audit_log_manager() {
    if let Some(manager) = GLOBAL_AUDIT_LOG_MANAGER.get() {
        manager.flush().await;
    }
}

pub async fn shutdown_global_audit_log_manager() {
    if let Some(manager) = GLOBAL_AUDIT_LOG_MANAGER.get() {
        manager.cancel();
        manager.flush().await;
    }
}

async fn write_audit_model(db: &DatabaseConnection, model: audit_log::ActiveModel) {
    if let Err(e) = audit_log_repo::create(db, model).await {
        tracing::warn!("failed to write audit log: {e}");
    }
}

async fn write_audit_batch(db: &DatabaseConnection, batch: &mut Vec<audit_log::ActiveModel>) {
    if batch.is_empty() {
        return;
    }

    let total = batch.len();
    let mut models = std::mem::take(batch).into_iter();
    loop {
        let chunk = models
            .by_ref()
            .take(AUDIT_LOG_BATCH_SIZE)
            .collect::<Vec<_>>();
        if chunk.is_empty() {
            break;
        }

        let count = chunk.len();
        if let Err(e) = audit_log_repo::create_many(db, chunk).await {
            tracing::warn!(count, total, "failed to write audit log batch: {e}");
        }
    }
}

impl AuditLogManager {
    pub(super) fn new(db: DatabaseConnection) -> Self {
        Self::new_with_delayed_flush_after(db, AUDIT_LOG_DELAYED_FLUSH_AFTER)
    }

    pub(super) fn new_with_delayed_flush_after(
        db: DatabaseConnection,
        delayed_flush_after: StdDuration,
    ) -> Self {
        Self {
            db,
            buffer: parking_lot::Mutex::new(Vec::with_capacity(AUDIT_LOG_BATCH_SIZE)),
            flush_lock: Mutex::new(()),
            flush_pending: AtomicBool::new(false),
            delayed_flush_pending: AtomicBool::new(false),
            delayed_flush_after,
            shutdown_token: CancellationToken::new(),
        }
    }

    pub(super) async fn record(self: &Arc<Self>, model: audit_log::ActiveModel) {
        let mut overflow_model = None;
        let should_flush;
        let should_schedule_delayed_flush;
        {
            let mut buffer = self.buffer.lock();
            if buffer.len() >= AUDIT_LOG_QUEUE_CAPACITY {
                overflow_model = Some(model);
                should_flush = false;
                should_schedule_delayed_flush = false;
            } else {
                let was_empty = buffer.is_empty();
                buffer.push(model);
                should_flush = buffer.len() >= AUDIT_LOG_BATCH_SIZE;
                should_schedule_delayed_flush = !should_flush && was_empty;
            }
        }

        if let Some(model) = overflow_model {
            tracing::warn!(
                capacity = AUDIT_LOG_QUEUE_CAPACITY,
                "audit log buffer is full; falling back to direct write"
            );
            self.schedule_flush();
            write_audit_model(&self.db, model).await;
            return;
        }

        if should_flush {
            self.schedule_flush();
        } else if should_schedule_delayed_flush {
            self.schedule_delayed_flush();
        }
    }

    fn schedule_flush(self: &Arc<Self>) {
        if self
            .flush_pending
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::Relaxed)
            .is_err()
        {
            return;
        }

        let manager = Arc::clone(self);
        drop(tokio::spawn(async move {
            let mut pending_reset = FlushPendingReset {
                manager: Arc::clone(&manager),
                armed: true,
            };
            {
                let _guard = manager.flush_lock.lock().await;
                manager.flush_buffer().await;
            }
            pending_reset.reset();
            manager.schedule_buffered_flush();
        }));
    }

    fn schedule_delayed_flush(self: &Arc<Self>) {
        if self
            .delayed_flush_pending
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::Relaxed)
            .is_err()
        {
            return;
        }

        let manager = Arc::clone(self);
        drop(tokio::spawn(async move {
            let mut pending_reset = DelayedFlushPendingReset {
                manager: Arc::clone(&manager),
                armed: true,
            };
            let delayed_flush_after = manager.delayed_flush_after;
            tokio::select! {
                biased;
                _ = manager.shutdown_token.cancelled() => return,
                _ = tokio::time::sleep(delayed_flush_after) => {}
            }

            {
                let _guard = manager.flush_lock.lock().await;
                manager.flush_buffer().await;
            }
            pending_reset.reset();
            manager.schedule_buffered_flush();
        }));
    }

    fn schedule_buffered_flush(self: &Arc<Self>) {
        let buffered_count = self.buffer.lock().len();
        if buffered_count >= AUDIT_LOG_BATCH_SIZE {
            self.schedule_flush();
        } else if buffered_count > 0 {
            self.schedule_delayed_flush();
        }
    }

    pub(super) async fn flush(self: &Arc<Self>) {
        let _guard = self.flush_lock.lock().await;
        self.flush_buffer().await;
        if self.buffer.lock().is_empty() {
            self.flush_pending.store(false, Ordering::Release);
            self.delayed_flush_pending.store(false, Ordering::Release);
        }
        self.schedule_buffered_flush();
    }

    pub(super) fn cancel(&self) {
        self.shutdown_token.cancel();
    }

    #[cfg(test)]
    pub(super) async fn lock_flush_for_test(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.flush_lock.lock().await
    }

    async fn flush_buffer(&self) {
        let mut models = {
            let mut buffer = self.buffer.lock();
            if buffer.is_empty() {
                return;
            }
            std::mem::take(&mut *buffer)
        };
        write_audit_batch(&self.db, &mut models).await;
    }
}

pub fn should_record(state: &PrimaryAppState, action: AuditAction) -> bool {
    state.should_record_audit_action(action)
}

async fn record_prechecked(
    state: &PrimaryAppState,
    ctx: &AuditContext,
    action: AuditAction,
    entity_type: AuditEntityType,
    entity_id: Option<i64>,
    entity_name: Option<&str>,
    details: Option<serde_json::Value>,
) {
    // Callers must pass the action-scope check before we allocate the DB model.
    let model = audit_log::ActiveModel {
        id: Default::default(),
        user_id: Set(ctx.user_id),
        action: Set(action),
        entity_type: Set(entity_type.as_str().to_string()),
        entity_id: Set(entity_id),
        entity_name: Set(entity_name.map(|s| s.to_string())),
        details: Set(details.map(|v| v.to_string())),
        ip_address: Set(ctx.ip_address.clone()),
        user_agent: Set(ctx.user_agent.clone()),
        created_at: Set(Utc::now()),
    };

    if let Some(manager) = GLOBAL_AUDIT_LOG_MANAGER.get() {
        manager.record(model).await;
    } else {
        write_audit_model(state.writer_db(), model).await;
    }
}

pub async fn log(
    state: &PrimaryAppState,
    ctx: &AuditContext,
    action: AuditAction,
    entity_type: AuditEntityType,
    entity_id: Option<i64>,
    entity_name: Option<&str>,
    details: Option<serde_json::Value>,
) {
    if !should_record(state, action) {
        return;
    }

    record_prechecked(
        state,
        ctx,
        action,
        entity_type,
        entity_id,
        entity_name,
        details,
    )
    .await;
}

pub async fn log_with_details<F>(
    state: &PrimaryAppState,
    ctx: &AuditContext,
    action: AuditAction,
    entity_type: AuditEntityType,
    entity_id: Option<i64>,
    entity_name: Option<&str>,
    details: F,
) where
    F: FnOnce() -> Option<serde_json::Value>,
{
    if !should_record(state, action) {
        return;
    }

    // Details can be expensive to serialize, so build them only after scope filtering.
    let details = details();
    record_prechecked(
        state,
        ctx,
        action,
        entity_type,
        entity_id,
        entity_name,
        details,
    )
    .await;
}
