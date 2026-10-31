use chrono::Utc;
use sea_orm::{ConnectionTrait, DatabaseConnection};

use crate::config::RuntimeConfig;
use crate::runtime::SharedRuntimeState;
use aster_drive_model::types::{AuditAction, AuditEntityType};

use super::context::AuditContext;

pub const AUDIT_FLUSH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// Queries and exports must not present an incomplete buffer as a complete result.
pub async fn flush(state: &impl SharedRuntimeState) -> crate::errors::Result<()> {
    state
        .audit_writer()
        .flush(tokio::time::Instant::now() + AUDIT_FLUSH_TIMEOUT)
        .await
        .map_err(|_| {
            tracing::error!("audit flush failed; retained records require reconciliation");
            crate::errors::AsterError::internal_error("audit persistence is incomplete")
        })
}

pub fn should_record<S: SharedRuntimeState>(state: &S, action: AuditAction) -> bool {
    !action.is_workspace_mutation() && state.runtime_config().should_record_audit_action(action)
}

pub fn should_record_with_config(runtime_config: &RuntimeConfig, action: AuditAction) -> bool {
    !action.is_workspace_mutation() && runtime_config.should_record_audit_action(action)
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

    // Best effort: a rejected record was never accepted; direct failures remain owned
    // by the writer. Neither case may fall back to a second, potentially duplicate write.
    if let Err(error) = state.audit_writer().record(request).await {
        use aster_forge_runtime::RecordError;
        match error {
            RecordError::NotRunning(_) => {
                tracing::warn!(
                    action = action.as_str(),
                    "audit record rejected: writer not running"
                );
            }
            RecordError::Invalid { .. } => {
                tracing::warn!(
                    action = action.as_str(),
                    "audit record rejected: invalid fields"
                );
            }
            RecordError::Full(_) => {
                tracing::warn!(
                    action = action.as_str(),
                    "audit record rejected: writer full"
                );
            }
            RecordError::DirectFailed(failure) => {
                tracing::error!(action = action.as_str(), class = ?failure.class,
                    "audit direct write failed; record retained for reconciliation");
            }
        }
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
    if aster_forge_db::create_audit_log_row(db, request)
        .await
        .is_err()
    {
        // Do not log DB errors or requests: drivers can embed SQL values in errors.
        tracing::error!(
            action = action.as_str(),
            "best-effort direct audit write failed"
        );
    }
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

/// Writes audit details on the caller's transaction connection.
///
/// Unlike the normal best-effort writer, this helper propagates persistence
/// errors so a product mutation can roll back when its audit contract is
/// explicitly part of the same atomic operation.
pub async fn log_with_transaction<C, F>(
    db: &C,
    runtime_config: &RuntimeConfig,
    input: AuditLogInput<'_>,
    details: F,
) -> aster_forge_db::Result<()>
where
    C: ConnectionTrait,
    F: FnOnce() -> Option<serde_json::Value>,
{
    if !should_record_with_config(runtime_config, input.action) {
        return Ok(());
    }
    let request = audit_log_request(
        input.ctx,
        input.action,
        input.entity_type,
        input.entity_id,
        input.entity_name,
        details(),
    );
    aster_forge_db::create_audit_log_row(db, request)
        .await
        .map(|_| ())
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

/// Mandatory content-access evidence: never buffered or filtered by optional audit settings.
pub async fn log_required<C: ConnectionTrait>(
    db: &C,
    input: AuditLogInput<'_>,
    details: serde_json::Value,
) -> crate::errors::Result<()> {
    let request = audit_log_request(
        input.ctx,
        input.action,
        input.entity_type,
        input.entity_id,
        input.entity_name,
        Some(details),
    );
    aster_forge_db::create_audit_log_row(db, request)
        .await
        .map_err(|_| {
            tracing::error!(
                action = input.action.as_str(),
                "required content access audit persistence failed"
            );
            crate::errors::auth_forbidden_with_code(
                crate::api::api_error_code::ApiErrorCode::AdminContentAuditUnavailable,
                "content access audit unavailable",
            )
        })?;
    Ok(())
}
