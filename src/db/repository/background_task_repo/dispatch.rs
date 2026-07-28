use chrono::{DateTime, Utc};
use sea_orm::{
    ActiveEnum, ColumnTrait, ConnectionTrait, EntityTrait, ExprTrait, QueryFilter, QueryOrder,
    QuerySelect, Select, sea_query::Expr,
};

use super::common::claimable_condition;
use crate::errors::{AsterError, Result};
use aster_drive_model::entities::background_task::{self, Entity as BackgroundTask};
use aster_drive_model::types::{BackgroundTaskKind, BackgroundTaskStatus};

pub async fn list_claimable<C: ConnectionTrait>(
    db: &C,
    now: DateTime<Utc>,
    stale_before: DateTime<Utc>,
    limit: u64,
) -> Result<Vec<background_task::Model>> {
    list_claimable_query(now, stale_before, limit)
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn list_claimable_by_kinds<C: ConnectionTrait>(
    db: &C,
    now: DateTime<Utc>,
    stale_before: DateTime<Utc>,
    kinds: &[BackgroundTaskKind],
    limit: u64,
) -> Result<Vec<background_task::Model>> {
    if kinds.is_empty() || limit == 0 {
        return Ok(Vec::new());
    }

    list_claimable_query(now, stale_before, limit)
        .filter(background_task::Column::Kind.is_in(kinds.iter().copied()))
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn try_claim<C: ConnectionTrait>(
    db: &C,
    id: i64,
    expected_processing_token: i64,
    now: DateTime<Utc>,
    stale_before: DateTime<Utc>,
    next_processing_token: i64,
    lease_expires_at: DateTime<Utc>,
) -> Result<bool> {
    // try_claim 是一条 compare-and-swap：
    // 只有当 id 命中、旧 processing_token 仍匹配、并且任务此刻仍满足 claimable 条件时，
    // 才会把任务推进到 Processing，并原子地把 token 递增到 next_processing_token。
    //
    // 这样多个 dispatcher 并发捞到同一条任务时，只有一个能成功认领。
    let result = BackgroundTask::update_many()
        .col_expr(
            background_task::Column::Status,
            Expr::value(BackgroundTaskStatus::Processing.to_value()),
        )
        .col_expr(
            background_task::Column::ProcessingStartedAt,
            Expr::value(Some(now)),
        )
        .col_expr(
            background_task::Column::LastHeartbeatAt,
            Expr::value(Some(now)),
        )
        .col_expr(
            background_task::Column::ProcessingToken,
            Expr::value(next_processing_token),
        )
        .col_expr(
            background_task::Column::LeaseExpiresAt,
            Expr::value(Some(lease_expires_at)),
        )
        .col_expr(
            background_task::Column::StartedAt,
            Expr::col(background_task::Column::StartedAt).if_null(now),
        )
        .col_expr(background_task::Column::UpdatedAt, Expr::value(now))
        .filter(background_task::Column::Id.eq(id))
        .filter(background_task::Column::ProcessingToken.eq(expected_processing_token))
        .filter(claimable_condition(now, stale_before))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}

pub async fn touch_heartbeat<C: ConnectionTrait>(
    db: &C,
    id: i64,
    processing_token: i64,
    now: DateTime<Utc>,
    lease_expires_at: DateTime<Utc>,
) -> Result<bool> {
    // heartbeat 也带 token 条件。
    // 如果返回 false，说明任务虽然还在表里，但这条 worker 的 lease 已经过期了。
    let result = BackgroundTask::update_many()
        .col_expr(
            background_task::Column::LastHeartbeatAt,
            Expr::value(Some(now)),
        )
        .col_expr(
            background_task::Column::LeaseExpiresAt,
            Expr::value(Some(lease_expires_at)),
        )
        .filter(background_task::Column::Id.eq(id))
        .filter(background_task::Column::Status.eq(BackgroundTaskStatus::Processing))
        .filter(background_task::Column::ProcessingToken.eq(processing_token))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}

fn list_claimable_query(
    now: DateTime<Utc>,
    stale_before: DateTime<Utc>,
    limit: u64,
) -> Select<BackgroundTask> {
    BackgroundTask::find()
        .filter(claimable_condition(now, stale_before))
        .order_by_asc(background_task::Column::CreatedAt)
        .order_by_asc(background_task::Column::Id)
        .limit(limit)
}
