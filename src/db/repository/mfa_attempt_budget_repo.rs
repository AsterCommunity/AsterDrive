//! MFA budget persistence. Callers serialize mutation using the account row.
use crate::errors::{AsterError, Result};
use aster_drive_model::entities::{mfa_attempt_budget, user};
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, Set, sea_query::Expr};

/// Obtain the account writer lock before consistent reads on every supported database.
pub(crate) async fn lock_account<C: ConnectionTrait>(db: &C, user_id: i64) -> Result<user::Model> {
    user::Entity::update_many()
        .col_expr(
            user::Column::SessionVersion,
            Expr::col(user::Column::SessionVersion),
        )
        .filter(user::Column::Id.eq(user_id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    crate::db::repository::user_repo::find_by_id(db, user_id).await
}

pub(crate) async fn find<C: ConnectionTrait>(
    db: &C,
    user_id: i64,
    purpose: &str,
) -> Result<Option<mfa_attempt_budget::Model>> {
    mfa_attempt_budget::Entity::find_by_id((user_id, purpose.to_string()))
        .one(db)
        .await
        .map_err(AsterError::from)
}

pub(crate) async fn save<C: ConnectionTrait>(
    db: &C,
    model: mfa_attempt_budget::Model,
) -> Result<()> {
    let active = mfa_attempt_budget::ActiveModel {
        user_id: Set(model.user_id),
        purpose: Set(model.purpose),
        failure_count: Set(model.failure_count),
        window_started_at: Set(model.window_started_at),
        blocked_until: Set(model.blocked_until),
        updated_at: Set(model.updated_at),
    };
    mfa_attempt_budget::Entity::insert(active)
        .on_conflict(
            sea_orm::sea_query::OnConflict::columns([
                mfa_attempt_budget::Column::UserId,
                mfa_attempt_budget::Column::Purpose,
            ])
            .update_columns([
                mfa_attempt_budget::Column::FailureCount,
                mfa_attempt_budget::Column::WindowStartedAt,
                mfa_attempt_budget::Column::BlockedUntil,
                mfa_attempt_budget::Column::UpdatedAt,
            ])
            .to_owned(),
        )
        .exec_without_returning(db)
        .await
        .map_err(AsterError::from)?;
    Ok(())
}

pub(crate) async fn delete_for_user<C: ConnectionTrait>(db: &C, user_id: i64) -> Result<()> {
    mfa_attempt_budget::Entity::delete_many()
        .filter(mfa_attempt_budget::Column::UserId.eq(user_id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(())
}
