//! 仓储模块：`mail_outbox_repo`。

use chrono::{DateTime, Utc};
use sea_orm::{
    ActiveEnum, ActiveModelTrait, ColumnTrait, Condition, ConnectionTrait, DatabaseConnection,
    EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, sea_query::Expr,
};

use crate::entities::mail_outbox::{self, Entity as MailOutbox};
use crate::errors::{AsterError, Result};
use crate::types::{MailOutboxStatus, StoredMailPayload};

pub async fn create<C: ConnectionTrait>(
    db: &C,
    model: mail_outbox::ActiveModel,
) -> Result<mail_outbox::Model> {
    model.insert(db).await.map_err(AsterError::from)
}

pub async fn list_claimable(
    db: &DatabaseConnection,
    now: DateTime<Utc>,
    stale_before: DateTime<Utc>,
    limit: u64,
) -> Result<Vec<mail_outbox::Model>> {
    MailOutbox::find()
        .filter(claimable_condition(now, stale_before))
        .order_by_asc(mail_outbox::Column::CreatedAt)
        .limit(limit)
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn try_claim(
    db: &DatabaseConnection,
    id: i64,
    now: DateTime<Utc>,
    stale_before: DateTime<Utc>,
) -> Result<bool> {
    let result = MailOutbox::update_many()
        .col_expr(
            mail_outbox::Column::Status,
            Expr::value(MailOutboxStatus::Processing.to_value()),
        )
        .col_expr(
            mail_outbox::Column::ProcessingStartedAt,
            Expr::value(Some(now)),
        )
        .col_expr(mail_outbox::Column::UpdatedAt, Expr::value(now))
        .filter(mail_outbox::Column::Id.eq(id))
        .filter(claimable_condition(now, stale_before))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}

pub async fn mark_sent(db: &DatabaseConnection, id: i64, sent_at: DateTime<Utc>) -> Result<bool> {
    let result = MailOutbox::update_many()
        .col_expr(
            mail_outbox::Column::Status,
            Expr::value(MailOutboxStatus::Sent.to_value()),
        )
        .col_expr(mail_outbox::Column::SentAt, Expr::value(Some(sent_at)))
        .col_expr(
            mail_outbox::Column::ProcessingStartedAt,
            Expr::value(Option::<DateTime<Utc>>::None),
        )
        .col_expr(
            mail_outbox::Column::LastError,
            Expr::value(Option::<String>::None),
        )
        .col_expr(
            mail_outbox::Column::PayloadJson,
            Expr::value(StoredMailPayload::CLEARED_JSON),
        )
        .col_expr(mail_outbox::Column::UpdatedAt, Expr::value(sent_at))
        .filter(mail_outbox::Column::Id.eq(id))
        .filter(mail_outbox::Column::Status.eq(MailOutboxStatus::Processing))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}

pub async fn mark_retry(
    db: &DatabaseConnection,
    id: i64,
    attempt_count: i32,
    next_attempt_at: DateTime<Utc>,
    last_error: &str,
) -> Result<bool> {
    let result = MailOutbox::update_many()
        .col_expr(
            mail_outbox::Column::Status,
            Expr::value(MailOutboxStatus::Retry.to_value()),
        )
        .col_expr(
            mail_outbox::Column::AttemptCount,
            Expr::value(attempt_count),
        )
        .col_expr(
            mail_outbox::Column::NextAttemptAt,
            Expr::value(next_attempt_at),
        )
        .col_expr(
            mail_outbox::Column::ProcessingStartedAt,
            Expr::value(Option::<DateTime<Utc>>::None),
        )
        .col_expr(
            mail_outbox::Column::LastError,
            Expr::value(Some(last_error)),
        )
        .col_expr(mail_outbox::Column::UpdatedAt, Expr::value(Utc::now()))
        .filter(mail_outbox::Column::Id.eq(id))
        .filter(mail_outbox::Column::Status.eq(MailOutboxStatus::Processing))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}

pub async fn mark_failed(
    db: &DatabaseConnection,
    id: i64,
    attempt_count: i32,
    failed_at: DateTime<Utc>,
    last_error: &str,
) -> Result<bool> {
    let result = MailOutbox::update_many()
        .col_expr(
            mail_outbox::Column::Status,
            Expr::value(MailOutboxStatus::Failed.to_value()),
        )
        .col_expr(
            mail_outbox::Column::AttemptCount,
            Expr::value(attempt_count),
        )
        .col_expr(mail_outbox::Column::NextAttemptAt, Expr::value(failed_at))
        .col_expr(
            mail_outbox::Column::ProcessingStartedAt,
            Expr::value(Option::<DateTime<Utc>>::None),
        )
        .col_expr(
            mail_outbox::Column::LastError,
            Expr::value(Some(last_error)),
        )
        .col_expr(
            mail_outbox::Column::PayloadJson,
            Expr::value(StoredMailPayload::CLEARED_JSON),
        )
        .col_expr(mail_outbox::Column::UpdatedAt, Expr::value(failed_at))
        .filter(mail_outbox::Column::Id.eq(id))
        .filter(mail_outbox::Column::Status.eq(MailOutboxStatus::Processing))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}

pub async fn count_active(db: &DatabaseConnection) -> Result<u64> {
    MailOutbox::find()
        .filter(
            mail_outbox::Column::Status.is_in([MailOutboxStatus::Pending, MailOutboxStatus::Retry]),
        )
        .count(db)
        .await
        .map_err(AsterError::from)
}

fn claimable_condition(now: DateTime<Utc>, stale_before: DateTime<Utc>) -> Condition {
    Condition::any()
        .add(
            Condition::all()
                .add(
                    mail_outbox::Column::Status
                        .is_in([MailOutboxStatus::Pending, MailOutboxStatus::Retry]),
                )
                .add(mail_outbox::Column::NextAttemptAt.lte(now)),
        )
        .add(
            Condition::all()
                .add(mail_outbox::Column::Status.eq(MailOutboxStatus::Processing))
                .add(mail_outbox::Column::ProcessingStartedAt.lte(stale_before)),
        )
}
