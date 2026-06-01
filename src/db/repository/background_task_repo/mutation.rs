use chrono::{DateTime, Utc};
use sea_orm::{
    ActiveEnum, ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter,
    sea_query::Expr,
};

use crate::entities::background_task::{self, Entity as BackgroundTask};
use crate::errors::{AsterError, Result};
use crate::types::{BackgroundTaskKind, BackgroundTaskStatus};

pub async fn create<C: ConnectionTrait>(
    db: &C,
    model: background_task::ActiveModel,
) -> Result<background_task::Model> {
    model.insert(db).await.map_err(AsterError::from)
}

pub struct SystemRuntimeSuccessRefresh<'a> {
    pub id: i64,
    pub result_json: &'a str,
    pub status_text: Option<&'a str>,
    pub next_run_at: DateTime<Utc>,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

pub async fn refresh_system_runtime_success<C: ConnectionTrait>(
    db: &C,
    refresh: SystemRuntimeSuccessRefresh<'_>,
) -> Result<bool> {
    let result = BackgroundTask::update_many()
        .col_expr(
            background_task::Column::Status,
            Expr::value(BackgroundTaskStatus::Succeeded.to_value()),
        )
        .col_expr(
            background_task::Column::ResultJson,
            Expr::value(Some(refresh.result_json.to_string())),
        )
        .col_expr(background_task::Column::ProgressCurrent, Expr::value(1))
        .col_expr(background_task::Column::ProgressTotal, Expr::value(1))
        .col_expr(
            background_task::Column::StatusText,
            Expr::value(refresh.status_text.map(str::to_string)),
        )
        .col_expr(
            background_task::Column::NextRunAt,
            Expr::value(refresh.next_run_at),
        )
        .col_expr(
            background_task::Column::ProcessingStartedAt,
            Expr::value(Option::<DateTime<Utc>>::None),
        )
        .col_expr(
            background_task::Column::LastHeartbeatAt,
            Expr::value(Option::<DateTime<Utc>>::None),
        )
        .col_expr(
            background_task::Column::LeaseExpiresAt,
            Expr::value(Option::<DateTime<Utc>>::None),
        )
        .col_expr(
            background_task::Column::StartedAt,
            Expr::value(Some(refresh.started_at)),
        )
        .col_expr(
            background_task::Column::FinishedAt,
            Expr::value(Some(refresh.finished_at)),
        )
        .col_expr(
            background_task::Column::LastError,
            Expr::value(Option::<String>::None),
        )
        .col_expr(
            background_task::Column::FailureCanRetry,
            Expr::value(Option::<bool>::None),
        )
        .col_expr(
            background_task::Column::ExpiresAt,
            Expr::value(refresh.expires_at),
        )
        .col_expr(
            background_task::Column::UpdatedAt,
            Expr::value(refresh.finished_at),
        )
        .filter(background_task::Column::Id.eq(refresh.id))
        .filter(background_task::Column::Kind.eq(BackgroundTaskKind::SystemRuntime))
        .filter(background_task::Column::Status.eq(BackgroundTaskStatus::Succeeded))
        .exec(db)
        .await
        .map_err(AsterError::from)?;

    Ok(result.rows_affected == 1)
}

pub struct TaskProgressUpdate<'a> {
    pub id: i64,
    pub processing_token: i64,
    pub now: DateTime<Utc>,
    pub lease_expires_at: DateTime<Utc>,
    pub current: i64,
    pub total: i64,
    pub status_text: Option<&'a str>,
    pub steps_json: Option<&'a str>,
}

pub async fn mark_progress<C: ConnectionTrait>(
    db: &C,
    update: TaskProgressUpdate<'_>,
) -> Result<bool> {
    let mut statement = BackgroundTask::update_many()
        .col_expr(
            background_task::Column::ProgressCurrent,
            Expr::value(update.current),
        )
        .col_expr(
            background_task::Column::ProgressTotal,
            Expr::value(update.total),
        )
        .col_expr(
            background_task::Column::StatusText,
            Expr::value(update.status_text.map(str::to_string)),
        )
        .col_expr(
            background_task::Column::LastHeartbeatAt,
            Expr::value(Some(update.now)),
        )
        .col_expr(
            background_task::Column::LeaseExpiresAt,
            Expr::value(Some(update.lease_expires_at)),
        )
        .col_expr(background_task::Column::UpdatedAt, Expr::value(update.now))
        .filter(background_task::Column::Id.eq(update.id))
        .filter(background_task::Column::Status.eq(BackgroundTaskStatus::Processing))
        .filter(background_task::Column::ProcessingToken.eq(update.processing_token));
    if let Some(steps_json) = update.steps_json {
        statement = statement.col_expr(
            background_task::Column::StepsJson,
            Expr::value(Some(steps_json.to_string())),
        );
    }
    let result = statement.exec(db).await.map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}

pub async fn set_runtime_json<C: ConnectionTrait>(
    db: &C,
    id: i64,
    processing_token: i64,
    runtime_json: Option<&str>,
    now: DateTime<Utc>,
) -> Result<bool> {
    let result = BackgroundTask::update_many()
        .col_expr(
            background_task::Column::RuntimeJson,
            Expr::value(runtime_json.map(str::to_string)),
        )
        .col_expr(background_task::Column::UpdatedAt, Expr::value(now))
        .filter(background_task::Column::Id.eq(id))
        .filter(background_task::Column::Status.eq(BackgroundTaskStatus::Processing))
        .filter(background_task::Column::ProcessingToken.eq(processing_token))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}

pub async fn set_display_name<C: ConnectionTrait>(
    db: &C,
    id: i64,
    processing_token: i64,
    display_name: &str,
    now: DateTime<Utc>,
) -> Result<bool> {
    let result = BackgroundTask::update_many()
        .col_expr(
            background_task::Column::DisplayName,
            Expr::value(display_name.to_string()),
        )
        .col_expr(background_task::Column::UpdatedAt, Expr::value(now))
        .filter(background_task::Column::Id.eq(id))
        .filter(background_task::Column::Status.eq(BackgroundTaskStatus::Processing))
        .filter(background_task::Column::ProcessingToken.eq(processing_token))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}

pub struct TaskSuccessUpdate<'a> {
    pub id: i64,
    pub processing_token: i64,
    pub result_json: Option<&'a str>,
    pub steps_json: Option<&'a str>,
    pub current: i64,
    pub total: i64,
    pub status_text: Option<&'a str>,
    pub finished_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

pub async fn mark_succeeded<C: ConnectionTrait>(
    db: &C,
    success: TaskSuccessUpdate<'_>,
) -> Result<bool> {
    let mut update = BackgroundTask::update_many()
        .col_expr(
            background_task::Column::Status,
            Expr::value(BackgroundTaskStatus::Succeeded.to_value()),
        )
        .col_expr(
            background_task::Column::ResultJson,
            Expr::value(success.result_json.map(str::to_string)),
        )
        .col_expr(
            background_task::Column::ProgressCurrent,
            Expr::value(success.current),
        )
        .col_expr(
            background_task::Column::ProgressTotal,
            Expr::value(success.total),
        )
        .col_expr(
            background_task::Column::StatusText,
            Expr::value(success.status_text.map(str::to_string)),
        )
        .col_expr(
            background_task::Column::ProcessingStartedAt,
            Expr::value(Option::<DateTime<Utc>>::None),
        )
        .col_expr(
            background_task::Column::LastHeartbeatAt,
            Expr::value(Option::<DateTime<Utc>>::None),
        )
        .col_expr(
            background_task::Column::LeaseExpiresAt,
            Expr::value(Option::<DateTime<Utc>>::None),
        )
        .col_expr(
            background_task::Column::FinishedAt,
            Expr::value(Some(success.finished_at)),
        )
        .col_expr(
            background_task::Column::ExpiresAt,
            Expr::value(success.expires_at),
        )
        .col_expr(
            background_task::Column::UpdatedAt,
            Expr::value(success.finished_at),
        )
        .filter(background_task::Column::Id.eq(success.id))
        .filter(background_task::Column::Status.eq(BackgroundTaskStatus::Processing))
        .filter(background_task::Column::ProcessingToken.eq(success.processing_token));
    if let Some(steps_json) = success.steps_json {
        update = update.col_expr(
            background_task::Column::StepsJson,
            Expr::value(Some(steps_json.to_string())),
        );
    }
    let result = update.exec(db).await.map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}

pub async fn mark_retry<C: ConnectionTrait>(
    db: &C,
    id: i64,
    processing_token: i64,
    attempt_count: i32,
    next_run_at: DateTime<Utc>,
    last_error: &str,
    steps_json: Option<&str>,
) -> Result<bool> {
    let mut update = BackgroundTask::update_many()
        .col_expr(
            background_task::Column::Status,
            Expr::value(BackgroundTaskStatus::Retry.to_value()),
        )
        .col_expr(
            background_task::Column::AttemptCount,
            Expr::value(attempt_count),
        )
        .col_expr(background_task::Column::NextRunAt, Expr::value(next_run_at))
        .col_expr(
            background_task::Column::ProcessingStartedAt,
            Expr::value(Option::<DateTime<Utc>>::None),
        )
        .col_expr(
            background_task::Column::LastHeartbeatAt,
            Expr::value(Option::<DateTime<Utc>>::None),
        )
        .col_expr(
            background_task::Column::LeaseExpiresAt,
            Expr::value(Option::<DateTime<Utc>>::None),
        )
        .col_expr(
            background_task::Column::StatusText,
            Expr::value(Option::<String>::None),
        )
        .col_expr(
            background_task::Column::LastError,
            Expr::value(Some(last_error.to_string())),
        )
        .col_expr(background_task::Column::UpdatedAt, Expr::value(Utc::now()))
        .filter(background_task::Column::Id.eq(id))
        .filter(background_task::Column::Status.eq(BackgroundTaskStatus::Processing))
        .filter(background_task::Column::ProcessingToken.eq(processing_token));
    if let Some(steps_json) = steps_json {
        update = update.col_expr(
            background_task::Column::StepsJson,
            Expr::value(Some(steps_json.to_string())),
        );
    }
    let result = update.exec(db).await.map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}

pub async fn release_processing<C: ConnectionTrait>(
    db: &C,
    id: i64,
    processing_token: i64,
    now: DateTime<Utc>,
    status: BackgroundTaskStatus,
) -> Result<bool> {
    if !matches!(
        status,
        BackgroundTaskStatus::Pending | BackgroundTaskStatus::Retry
    ) {
        return Err(AsterError::internal_error(
            "released background task status must be pending or retry",
        ));
    }

    let result = BackgroundTask::update_many()
        .col_expr(
            background_task::Column::Status,
            Expr::value(status.to_value()),
        )
        .col_expr(background_task::Column::NextRunAt, Expr::value(now))
        .col_expr(
            background_task::Column::ProcessingStartedAt,
            Expr::value(Option::<DateTime<Utc>>::None),
        )
        .col_expr(
            background_task::Column::LastHeartbeatAt,
            Expr::value(Option::<DateTime<Utc>>::None),
        )
        .col_expr(
            background_task::Column::LeaseExpiresAt,
            Expr::value(Option::<DateTime<Utc>>::None),
        )
        .col_expr(
            background_task::Column::StatusText,
            Expr::value(Option::<String>::None),
        )
        .col_expr(background_task::Column::UpdatedAt, Expr::value(now))
        .filter(background_task::Column::Id.eq(id))
        .filter(background_task::Column::Status.eq(BackgroundTaskStatus::Processing))
        .filter(background_task::Column::ProcessingToken.eq(processing_token))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}

pub struct TaskFailureUpdate<'a> {
    pub id: i64,
    pub processing_token: i64,
    pub attempt_count: i32,
    pub last_error: &'a str,
    pub finished_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub steps_json: Option<&'a str>,
    pub failure_can_retry: bool,
}

pub async fn mark_failed<C: ConnectionTrait>(
    db: &C,
    update: TaskFailureUpdate<'_>,
) -> Result<bool> {
    let mut statement = BackgroundTask::update_many()
        .col_expr(
            background_task::Column::Status,
            Expr::value(BackgroundTaskStatus::Failed.to_value()),
        )
        .col_expr(
            background_task::Column::AttemptCount,
            Expr::value(update.attempt_count),
        )
        .col_expr(
            background_task::Column::ProcessingStartedAt,
            Expr::value(Option::<DateTime<Utc>>::None),
        )
        .col_expr(
            background_task::Column::LastHeartbeatAt,
            Expr::value(Option::<DateTime<Utc>>::None),
        )
        .col_expr(
            background_task::Column::LeaseExpiresAt,
            Expr::value(Option::<DateTime<Utc>>::None),
        )
        .col_expr(
            background_task::Column::StatusText,
            Expr::value(Option::<String>::None),
        )
        .col_expr(
            background_task::Column::LastError,
            Expr::value(Some(update.last_error.to_string())),
        )
        .col_expr(
            background_task::Column::FailureCanRetry,
            Expr::value(Some(update.failure_can_retry)),
        )
        .col_expr(
            background_task::Column::FinishedAt,
            Expr::value(Some(update.finished_at)),
        )
        .col_expr(
            background_task::Column::ExpiresAt,
            Expr::value(update.expires_at),
        )
        .col_expr(
            background_task::Column::UpdatedAt,
            Expr::value(update.finished_at),
        )
        .filter(background_task::Column::Id.eq(update.id))
        .filter(background_task::Column::Status.eq(BackgroundTaskStatus::Processing))
        .filter(background_task::Column::ProcessingToken.eq(update.processing_token));
    if let Some(steps_json) = update.steps_json {
        statement = statement.col_expr(
            background_task::Column::StepsJson,
            Expr::value(Some(steps_json.to_string())),
        );
    }
    let result = statement.exec(db).await.map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}

pub async fn reset_for_manual_retry<C: ConnectionTrait>(
    db: &C,
    id: i64,
    now: DateTime<Utc>,
    max_attempts: i32,
    steps_json: Option<&str>,
) -> Result<bool> {
    let mut update = BackgroundTask::update_many()
        .col_expr(
            background_task::Column::Status,
            Expr::value(BackgroundTaskStatus::Pending.to_value()),
        )
        .col_expr(background_task::Column::AttemptCount, Expr::value(0))
        .col_expr(background_task::Column::ProgressCurrent, Expr::value(0))
        .col_expr(
            background_task::Column::MaxAttempts,
            Expr::value(max_attempts),
        )
        .col_expr(background_task::Column::NextRunAt, Expr::value(now))
        .col_expr(
            background_task::Column::ProcessingStartedAt,
            Expr::value(Option::<DateTime<Utc>>::None),
        )
        .col_expr(
            background_task::Column::LastHeartbeatAt,
            Expr::value(Option::<DateTime<Utc>>::None),
        )
        .col_expr(
            background_task::Column::LeaseExpiresAt,
            Expr::value(Option::<DateTime<Utc>>::None),
        )
        .col_expr(
            background_task::Column::StartedAt,
            Expr::value(Option::<DateTime<Utc>>::None),
        )
        .col_expr(
            background_task::Column::FinishedAt,
            Expr::value(Option::<DateTime<Utc>>::None),
        )
        .col_expr(
            background_task::Column::StatusText,
            Expr::value(Option::<String>::None),
        )
        .col_expr(
            background_task::Column::LastError,
            Expr::value(Option::<String>::None),
        )
        .col_expr(
            background_task::Column::ResultJson,
            Expr::value(Option::<String>::None),
        )
        .col_expr(
            background_task::Column::FailureCanRetry,
            Expr::value(Option::<bool>::None),
        )
        .col_expr(background_task::Column::UpdatedAt, Expr::value(now))
        .filter(background_task::Column::Id.eq(id))
        .filter(background_task::Column::Status.eq(BackgroundTaskStatus::Failed));
    if let Some(steps_json) = steps_json {
        update = update.col_expr(
            background_task::Column::StepsJson,
            Expr::value(Some(steps_json.to_string())),
        );
    }
    let result = update.exec(db).await.map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}
