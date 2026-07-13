use chrono::Utc;
use sea_orm::DatabaseConnection;

use crate::config::RuntimeConfig;
use crate::runtime::SharedRuntimeState;
use crate::types::{AuditAction, AuditEntityType};

use super::context::AuditContext;

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

    aster_forge_audit::record_audit_log(state.writer_db(), request).await;
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
    aster_forge_audit::write_audit_log_direct(db, request).await;
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
