use chrono::{DateTime, Utc};
use sea_orm::{
    ActiveEnum, ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter,
    sea_query::Expr,
};

use crate::errors::{AsterError, Result};
use aster_drive_model::entities::background_task::{self, Entity as BackgroundTask};
use aster_drive_model::types::{BackgroundTaskKind, BackgroundTaskStatus};

/// Inserts a new background task row.
pub async fn create<C: ConnectionTrait>(
    db: &C,
    model: background_task::ActiveModel,
) -> Result<background_task::Model> {
    model.insert(db).await.map_err(AsterError::from)
}

/// Clears the dedupe key of a failed or canceled task so the key can be reused.
///
/// The update only applies when the row still carries the given dedupe key.
pub async fn clear_unsuccessful_dedupe_key<C: ConnectionTrait>(
    db: &C,
    id: i64,
    dedupe_key: &str,
) -> Result<()> {
    BackgroundTask::update_many()
        .col_expr(
            background_task::Column::DedupeKey,
            Expr::value(Option::<String>::None),
        )
        .filter(background_task::Column::Id.eq(id))
        .filter(
            background_task::Column::Status
                .is_in([BackgroundTaskStatus::Failed, BackgroundTaskStatus::Canceled]),
        )
        .filter(background_task::Column::DedupeKey.eq(dedupe_key))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(())
}

/// Values for refreshing a `SystemRuntime` task row that is already succeeded.
pub struct SystemRuntimeSuccessRefresh<'a> {
    /// Id of the background task row to refresh.
    pub id: i64,
    /// JSON payload recorded as the task result.
    pub result_json: &'a str,
    /// Optional human-readable status text.
    pub status_text: Option<&'a str>,
    /// Next scheduled run time.
    pub next_run_at: DateTime<Utc>,
    /// Run start time recorded on the row.
    pub started_at: DateTime<Utc>,
    /// Run finish time recorded on the row.
    pub finished_at: DateTime<Utc>,
    /// Time at which the result expires.
    pub expires_at: DateTime<Utc>,
}

/// Refreshes an already succeeded `SystemRuntime` task with a new run's result and schedule.
///
/// Progress is normalized to 1/1, processing and lease bookkeeping is cleared, and the
/// recorded error is dropped. Returns `false` when no matching succeeded row was updated.
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

/// Progress and lease renewal values for a task currently being processed.
pub struct TaskProgressUpdate<'a> {
    /// Id of the background task row.
    pub id: i64,
    /// Processing token leased by the worker; must match the row.
    pub processing_token: i64,
    /// Current time used for the heartbeat and update timestamp.
    pub now: DateTime<Utc>,
    /// New lease expiry time.
    pub lease_expires_at: DateTime<Utc>,
    /// Current progress value.
    pub current: i64,
    /// Total progress value.
    pub total: i64,
    /// Optional human-readable status text.
    pub status_text: Option<&'a str>,
    /// Optional step breakdown JSON to persist.
    pub steps_json: Option<&'a str>,
}

/// Records progress for a task in `Processing` state and renews its lease and heartbeat.
///
/// Returns `false` when the row is not in `Processing` state or the processing token does
/// not match.
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

/// Replaces the runtime JSON of a task in `Processing` state.
///
/// Returns `false` when the row is not in `Processing` state or the processing token does
/// not match.
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

/// Sets the display name of a task in `Processing` state.
///
/// Returns `false` when the row is not in `Processing` state or the processing token does
/// not match.
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

/// Values for transitioning a leased task to the succeeded state.
pub struct TaskSuccessUpdate<'a> {
    /// Id of the background task row.
    pub id: i64,
    /// Processing token leased by the worker; must match the row.
    pub processing_token: i64,
    /// Optional JSON payload recorded as the task result.
    pub result_json: Option<&'a str>,
    /// Optional step breakdown JSON to persist.
    pub steps_json: Option<&'a str>,
    /// Final progress value.
    pub current: i64,
    /// Final progress total.
    pub total: i64,
    /// Optional human-readable status text.
    pub status_text: Option<&'a str>,
    /// Completion time recorded on the row.
    pub finished_at: DateTime<Utc>,
    /// Time at which the result expires.
    pub expires_at: DateTime<Utc>,
}

/// Transitions a leased task in `Processing` state to `Succeeded`.
///
/// Records the result, final progress, and finish time and clears processing and lease
/// bookkeeping. Returns `false` when the row is not in `Processing` state or the
/// processing token does not match.
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

/// Transitions a leased task in `Processing` state back to `Retry` with a next run time.
///
/// Records the attempt count and last error, clears processing and lease bookkeeping and
/// the status text, and optionally persists a step breakdown. Returns `false` when the
/// row is not in `Processing` state or the processing token does not match.
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

/// Hands a leased task row back to the dispatcher during cooperative shutdown.
///
/// The released status must be `Pending` or `Retry`; the lease is returned without
/// spending retry budget or recording a business failure. Crashes and stale workers are
/// still handled by normal lease expiry and reclaim. Returns `false` when the row is not
/// in `Processing` state or the processing token does not match.
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

/// Values for transitioning a leased task to the failed state.
pub struct TaskFailureUpdate<'a> {
    /// Id of the background task row.
    pub id: i64,
    /// Processing token leased by the worker; must match the row.
    pub processing_token: i64,
    /// Attempt count to record.
    pub attempt_count: i32,
    /// Error message recorded as the failure reason.
    pub last_error: &'a str,
    /// Failure time recorded on the row.
    pub finished_at: DateTime<Utc>,
    /// Time at which the failure record expires.
    pub expires_at: DateTime<Utc>,
    /// Optional step breakdown JSON to persist.
    pub steps_json: Option<&'a str>,
    /// Whether the failure still allows the task to be retried.
    pub failure_can_retry: bool,
}

/// Transitions a leased task in `Processing` state to `Failed`.
///
/// Records the attempt count, last error, and retry permission, clears processing and
/// lease bookkeeping, and stamps the finish time. Returns `false` when the row is not in
/// `Processing` state or the processing token does not match.
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

/// Resets a failed or canceled task for another manual run.
///
/// Restores `Pending` state with a fresh attempt count and progress, the given max
/// attempts and run schedule, and clears all run bookkeeping, results, and errors.
/// Returns `false` when the row is not in `Failed` or `Canceled` state.
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
        .filter(
            background_task::Column::Status
                .is_in([BackgroundTaskStatus::Failed, BackgroundTaskStatus::Canceled]),
        );
    if let Some(steps_json) = steps_json {
        update = update.col_expr(
            background_task::Column::StepsJson,
            Expr::value(Some(steps_json.to_string())),
        );
    }
    let result = update.exec(db).await.map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}
