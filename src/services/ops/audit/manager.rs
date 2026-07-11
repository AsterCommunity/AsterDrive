use chrono::Utc;
use sea_orm::DatabaseConnection;
use std::sync::{Arc, OnceLock};
use std::time::Duration as StdDuration;

use crate::config::RuntimeConfig;
use crate::runtime::SharedRuntimeState;
use crate::types::{AuditAction, AuditEntityType};

use super::context::AuditContext;

pub(super) const AUDIT_LOG_QUEUE_CAPACITY: usize = 4096;
pub(super) const AUDIT_LOG_BATCH_SIZE: usize = 100;
const AUDIT_LOG_DELAYED_FLUSH_AFTER: StdDuration = StdDuration::from_secs(1);

static GLOBAL_AUDIT_LOG_MANAGER: OnceLock<Arc<AuditLogManager>> = OnceLock::new();

pub(super) struct AuditLogManager {
    writer: Arc<aster_forge_runtime::BufferedBatchWriter<aster_forge_db::AuditLogCreate>>,
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

async fn write_audit_log(db: &DatabaseConnection, request: aster_forge_db::AuditLogCreate) {
    if let Err(error) = aster_forge_db::create_audit_log_row(db, request).await {
        tracing::warn!("failed to write audit log: {error}");
    }
}

async fn write_audit_batch(
    db: &DatabaseConnection,
    batch: &mut Vec<aster_forge_db::AuditLogCreate>,
) {
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
        if let Err(error) = aster_forge_db::create_audit_log_requests(db, chunk).await {
            tracing::warn!(count, total, "failed to write audit log batch: {error}");
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
        let batch_db = db.clone();
        let single_db = db;
        let writer = aster_forge_runtime::BufferedBatchWriter::new(
            aster_forge_runtime::BufferedBatchConfig::new(
                AUDIT_LOG_QUEUE_CAPACITY,
                AUDIT_LOG_BATCH_SIZE,
                delayed_flush_after,
                "audit_log",
            ),
            move |mut batch| {
                let db = batch_db.clone();
                async move { write_audit_batch(&db, &mut batch).await }
            },
            move |request| {
                let db = single_db.clone();
                async move { write_audit_log(&db, request).await }
            },
        );
        Self {
            writer: Arc::new(writer),
        }
    }

    pub(super) async fn record(&self, request: aster_forge_db::AuditLogCreate) {
        self.writer.record(request).await;
    }

    pub(super) async fn flush(self: &Arc<Self>) {
        self.writer.flush().await;
    }

    pub(super) fn cancel(&self) {
        self.writer.cancel();
    }

    #[cfg(test)]
    pub(super) async fn lock_flush_for_test(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.writer.lock_flush_for_test().await
    }
}

pub fn should_record<S: SharedRuntimeState>(state: &S, action: AuditAction) -> bool {
    state.runtime_config().should_record_audit_action(action)
}

pub fn should_record_with_config(runtime_config: &RuntimeConfig, action: AuditAction) -> bool {
    runtime_config.should_record_audit_action(action)
}

#[derive(Debug, Clone, Copy)]
pub struct AuditLogInput<'a> {
    pub ctx: &'a AuditContext,
    pub action: AuditAction,
    pub entity_type: AuditEntityType,
    pub entity_id: Option<i64>,
    pub entity_name: Option<&'a str>,
}

fn audit_log_request(
    ctx: &AuditContext,
    action: AuditAction,
    entity_type: AuditEntityType,
    entity_id: Option<i64>,
    entity_name: Option<&str>,
    details: Option<serde_json::Value>,
) -> aster_forge_db::AuditLogCreate {
    aster_forge_db::AuditLogCreate {
        user_id: ctx.user_id,
        action: action.as_str().to_string(),
        entity_type: entity_type.as_str().to_string(),
        entity_id,
        entity_name: entity_name.map(ToOwned::to_owned),
        details: details.map(|value| value.to_string()),
        ip_address: ctx.ip_address.clone(),
        user_agent: ctx.user_agent.clone(),
        created_at: Utc::now(),
    }
}

async fn record_prechecked<S: SharedRuntimeState>(
    state: &S,
    ctx: &AuditContext,
    action: AuditAction,
    entity_type: AuditEntityType,
    entity_id: Option<i64>,
    entity_name: Option<&str>,
    details: Option<serde_json::Value>,
) {
    // Callers must pass the action-scope check before we allocate the DB model.
    let request = audit_log_request(ctx, action, entity_type, entity_id, entity_name, details);

    if let Some(manager) = GLOBAL_AUDIT_LOG_MANAGER.get() {
        manager.record(request).await;
    } else {
        write_audit_log(state.writer_db(), request).await;
    }
}

async fn record_prechecked_with_db(
    db: &DatabaseConnection,
    ctx: &AuditContext,
    action: AuditAction,
    entity_type: AuditEntityType,
    entity_id: Option<i64>,
    entity_name: Option<&str>,
    details: Option<serde_json::Value>,
) {
    let request = audit_log_request(ctx, action, entity_type, entity_id, entity_name, details);
    write_audit_log(db, request).await;
}

pub async fn log<S: SharedRuntimeState>(
    state: &S,
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

pub async fn log_with_db_and_config<F>(
    db: &DatabaseConnection,
    runtime_config: &RuntimeConfig,
    input: AuditLogInput<'_>,
    details: F,
) where
    F: FnOnce() -> Option<serde_json::Value>,
{
    if !should_record_with_config(runtime_config, input.action) {
        return;
    }

    let details = details();
    record_prechecked_with_db(
        db,
        input.ctx,
        input.action,
        input.entity_type,
        input.entity_id,
        input.entity_name,
        details,
    )
    .await;
}

pub async fn log_with_details<S, F>(
    state: &S,
    ctx: &AuditContext,
    action: AuditAction,
    entity_type: AuditEntityType,
    entity_id: Option<i64>,
    entity_name: Option<&str>,
    details: F,
) where
    S: SharedRuntimeState,
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
