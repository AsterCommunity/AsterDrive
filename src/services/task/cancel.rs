//! Durable, scoped cooperative cancellation. Committed product mutations are retained.
use super::types::TaskInfo;
use crate::{
    api::api_error_code::ApiErrorCode,
    errors::{AsterError, Result},
    runtime::TaskRuntimeState,
    services::{
        ops::audit::{self, AuditContext},
        workspace::storage::{self, WorkspaceStorageScope},
    },
};
use aster_drive_model::{
    entities::background_task,
    types::{BackgroundTaskKind, BackgroundTaskStatus},
};
use aster_forge_db::transaction;
use aster_forge_tasks::TaskLease;
use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait,
    IntoActiveModel, QueryFilter, QuerySelect, Set, sea_query::Expr,
};

pub(crate) async fn request(
    state: &impl TaskRuntimeState,
    scope: WorkspaceStorageScope,
    id: i64,
    ctx: &AuditContext,
) -> Result<TaskInfo> {
    let changed = transaction::with_transaction(state.writer_db(), async |txn| {
        storage::require_scope_access_with_db(state, txn, scope).await?;
        let mut query = background_task::Entity::find_by_id(id);
        if txn.get_database_backend() != sea_orm::DbBackend::Sqlite {
            query = query.lock_exclusive();
        }
        let task = query
            .one(txn)
            .await?
            .ok_or_else(|| AsterError::record_not_found("task not found"))?;
        super::ensure_task_in_scope(&task, scope)?;
        if task.kind == BackgroundTaskKind::SystemRuntime {
            return Err(crate::errors::precondition_failed_with_code(
                ApiErrorCode::TaskCancelNotAllowed,
                "system maintenance cannot be canceled through a workspace",
            ));
        }
        let queued = matches!(
            task.status,
            BackgroundTaskStatus::Pending | BackgroundTaskStatus::Retry
        );
        match task.status {
            BackgroundTaskStatus::Canceled | BackgroundTaskStatus::Canceling => return Ok(false),
            BackgroundTaskStatus::Succeeded | BackgroundTaskStatus::Failed => {
                return Err(AsterError::conflict("task has already finished")
                    .with_api_error_code(ApiErrorCode::TaskCancelNotAllowed));
            }
            _ => {}
        }
        let mut update = task.clone().into_active_model();
        update.updated_at = Set(Utc::now());
        update.status = Set(if queued {
            BackgroundTaskStatus::Canceled
        } else {
            BackgroundTaskStatus::Canceling
        });
        update.status_text = Set(Some(
            if queued {
                "Canceled"
            } else {
                "Cancellation requested"
            }
            .to_string(),
        ));
        if queued {
            update.finished_at = Set(Some(Utc::now()));
            update.expires_at = Set(super::task_expiration_from(state, Utc::now()));
            update.processing_started_at = Set(None);
            update.last_heartbeat_at = Set(None);
            update.lease_expires_at = Set(None);
            update.failure_can_retry = Set(Some(true));
            super::folder_tree::cleanup_terminal_operation_on(txn, &task).await?;
        }
        update.update(txn).await?;
        Ok(true)
    })
    .await?;
    let result = super::get_task_in_scope(state, scope, id).await?;
    if changed {
        audit::log_with_details(
            state,
            ctx,
            audit::AuditAction::TaskCancelRequested,
            audit::AuditEntityType::Task,
            Some(id),
            Some(&result.display_name),
            || {
                audit::details(audit::TaskCancelAuditDetails {
                    status: result.status.as_str().to_string(),
                    completed_changes_retained: true,
                })
            },
        )
        .await;
        state.wake_background_task_dispatcher();
    }
    Ok(result)
}

/// Only the owner may acknowledge cancellation; cleanup commits with the terminal state.
pub(crate) async fn acknowledge(
    db: &DatabaseConnection,
    task: &background_task::Model,
    lease: TaskLease,
    expires_at: chrono::DateTime<Utc>,
) -> Result<bool> {
    transaction::with_transaction(db, async |txn| {
        let updated = background_task::Entity::update_many()
            .col_expr(
                background_task::Column::Status,
                Expr::value(BackgroundTaskStatus::Canceled),
            )
            .col_expr(
                background_task::Column::FinishedAt,
                Expr::value(Some(Utc::now())),
            )
            .col_expr(background_task::Column::UpdatedAt, Expr::value(Utc::now()))
            .col_expr(background_task::Column::ExpiresAt, Expr::value(expires_at))
            .col_expr(
                background_task::Column::StatusText,
                Expr::value(Some("Canceled; completed changes retained".to_string())),
            )
            .col_expr(
                background_task::Column::ProcessingStartedAt,
                Expr::value(Option::<chrono::DateTime<Utc>>::None),
            )
            .col_expr(
                background_task::Column::LastHeartbeatAt,
                Expr::value(Option::<chrono::DateTime<Utc>>::None),
            )
            .col_expr(
                background_task::Column::LeaseExpiresAt,
                Expr::value(Option::<chrono::DateTime<Utc>>::None),
            )
            .col_expr(
                background_task::Column::FailureCanRetry,
                Expr::value(Some(true)),
            )
            .filter(background_task::Column::Id.eq(lease.task_id))
            .filter(background_task::Column::ProcessingToken.eq(lease.processing_token))
            .filter(background_task::Column::Status.eq(BackgroundTaskStatus::Canceling))
            .exec(txn)
            .await?
            .rows_affected
            == 1;
        if updated {
            super::folder_tree::cleanup_terminal_operation_on(txn, task).await?;
        }
        Ok(updated)
    })
    .await
}
