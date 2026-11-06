//! 仓储模块：`mfa_login_flow_repo`。

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, DbBackend, EntityTrait,
    ExprTrait, QueryFilter, QuerySelect, sea_query::Expr,
};

use crate::errors::{AsterError, Result};
use aster_drive_model::entities::mfa_login_flow::{self, Entity as MfaLoginFlow};

pub async fn create(
    db: &DatabaseConnection,
    model: mfa_login_flow::ActiveModel,
) -> Result<mfa_login_flow::Model> {
    model.insert(db).await.map_err(AsterError::from)
}

pub async fn find_by_flow_token_hash<C: ConnectionTrait>(
    db: &C,
    flow_token_hash: &str,
) -> Result<Option<mfa_login_flow::Model>> {
    MfaLoginFlow::find()
        .filter(mfa_login_flow::Column::FlowTokenHash.eq(flow_token_hash))
        .one(db)
        .await
        .map_err(AsterError::from)
}

/// Acquire the flow's verification guard before reading its attempt budget. Must be called in
/// a writer transaction and held through factor consumption, session insertion and commit.
/// SQLite has no SELECT FOR UPDATE: an initial no-op UPDATE acquires its writer reservation
/// before any transactional snapshot read, avoiding a deferred read-to-write upgrade race.
pub async fn lock_for_verification<C: ConnectionTrait>(
    db: &C,
    flow_token_hash: &str,
) -> Result<Option<mfa_login_flow::Model>> {
    let query =
        MfaLoginFlow::find().filter(mfa_login_flow::Column::FlowTokenHash.eq(flow_token_hash));
    match db.get_database_backend() {
        DbBackend::Sqlite => {
            MfaLoginFlow::update_many()
                .col_expr(
                    mfa_login_flow::Column::AttemptCount,
                    Expr::col(mfa_login_flow::Column::AttemptCount),
                )
                .filter(mfa_login_flow::Column::FlowTokenHash.eq(flow_token_hash))
                .exec(db)
                .await
                .map_err(AsterError::from)?;
            query.one(db).await.map_err(AsterError::from)
        }
        _ => query
            .lock_exclusive()
            .one(db)
            .await
            .map_err(AsterError::from),
    }
}

pub async fn increment_attempts<C: ConnectionTrait>(
    db: &C,
    id: i64,
    expected_attempts: i32,
    max_attempts: i32,
    now: chrono::DateTime<Utc>,
    consume_at: Option<chrono::DateTime<Utc>>,
) -> Result<bool> {
    let mut update = MfaLoginFlow::update_many()
        .col_expr(
            mfa_login_flow::Column::AttemptCount,
            Expr::col(mfa_login_flow::Column::AttemptCount).add(1),
        )
        .filter(mfa_login_flow::Column::Id.eq(id))
        .filter(mfa_login_flow::Column::AttemptCount.eq(expected_attempts))
        .filter(mfa_login_flow::Column::AttemptCount.lt(max_attempts))
        .filter(mfa_login_flow::Column::ExpiresAt.gt(now))
        .filter(mfa_login_flow::Column::ConsumedAt.is_null());
    if let Some(consume_at) = consume_at {
        update = update.col_expr(
            mfa_login_flow::Column::ConsumedAt,
            Expr::value(Some(consume_at)),
        );
    }
    let result = update.exec(db).await.map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}

pub async fn consume<C: ConnectionTrait>(
    db: &C,
    id: i64,
    expected_attempts: i32,
    max_attempts: i32,
    now: chrono::DateTime<Utc>,
) -> Result<bool> {
    let result = MfaLoginFlow::update_many()
        .col_expr(mfa_login_flow::Column::ConsumedAt, Expr::value(Some(now)))
        .filter(mfa_login_flow::Column::Id.eq(id))
        .filter(mfa_login_flow::Column::AttemptCount.eq(expected_attempts))
        .filter(mfa_login_flow::Column::AttemptCount.lt(max_attempts))
        .filter(mfa_login_flow::Column::ExpiresAt.gt(now))
        .filter(mfa_login_flow::Column::ConsumedAt.is_null())
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}

pub async fn delete_all_for_user<C: ConnectionTrait>(db: &C, user_id: i64) -> Result<u64> {
    let result = MfaLoginFlow::delete_many()
        .filter(mfa_login_flow::Column::UserId.eq(user_id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected)
}

pub async fn cleanup_expired(db: &DatabaseConnection, now: chrono::DateTime<Utc>) -> Result<u64> {
    let result = MfaLoginFlow::delete_many()
        .filter(mfa_login_flow::Column::ExpiresAt.lt(now))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected)
}
