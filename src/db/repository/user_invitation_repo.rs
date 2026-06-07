//! 仓储模块：`user_invitation_repo`。

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, IntoActiveModel, PaginatorTrait,
    QueryFilter, QueryOrder, QuerySelect, sea_query::Expr,
};

use crate::entities::user_invitation::{self, Entity as UserInvitation};
use crate::errors::{AsterError, Result};
use crate::types::UserInvitationStatus;

pub async fn create<C: ConnectionTrait>(
    db: &C,
    model: user_invitation::ActiveModel,
) -> Result<user_invitation::Model> {
    model.insert(db).await.map_err(AsterError::from)
}

pub async fn find_by_id<C: ConnectionTrait>(db: &C, id: i64) -> Result<user_invitation::Model> {
    UserInvitation::find_by_id(id)
        .one(db)
        .await
        .map_err(AsterError::from)?
        .ok_or_else(|| AsterError::record_not_found("user invitation not found"))
}

pub async fn find_by_token_hash<C: ConnectionTrait>(
    db: &C,
    token_hash: &str,
) -> Result<Option<user_invitation::Model>> {
    UserInvitation::find()
        .filter(user_invitation::Column::TokenHash.eq(token_hash))
        .one(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_pending_by_email<C: ConnectionTrait>(
    db: &C,
    email: &str,
) -> Result<Vec<user_invitation::Model>> {
    UserInvitation::find()
        .filter(user_invitation::Column::Email.eq(email))
        .filter(user_invitation::Column::Status.eq(UserInvitationStatus::Pending))
        .order_by_desc(user_invitation::Column::CreatedAt)
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn list<C: ConnectionTrait>(
    db: &C,
    limit: u64,
    offset: u64,
) -> Result<(Vec<user_invitation::Model>, u64)> {
    let base_query = UserInvitation::find().order_by_desc(user_invitation::Column::CreatedAt);
    let total = base_query
        .clone()
        .count(db)
        .await
        .map_err(AsterError::from)?;
    let items = base_query
        .limit(limit)
        .offset(offset)
        .all(db)
        .await
        .map_err(AsterError::from)?;
    Ok((items, total))
}

pub async fn count_all<C: ConnectionTrait>(db: &C) -> Result<u64> {
    UserInvitation::find()
        .count(db)
        .await
        .map_err(AsterError::from)
}

pub async fn mark_revoked_if_pending<C: ConnectionTrait>(db: &C, id: i64) -> Result<bool> {
    let now = Utc::now();
    let result = UserInvitation::update_many()
        .col_expr(
            user_invitation::Column::Status,
            Expr::value(UserInvitationStatus::Revoked),
        )
        .col_expr(user_invitation::Column::UpdatedAt, Expr::value(now))
        .col_expr(user_invitation::Column::RevokedAt, Expr::value(Some(now)))
        .filter(user_invitation::Column::Id.eq(id))
        .filter(user_invitation::Column::Status.eq(UserInvitationStatus::Pending))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}

pub async fn mark_expired_if_pending<C: ConnectionTrait>(db: &C, id: i64) -> Result<bool> {
    let result = UserInvitation::update_many()
        .col_expr(
            user_invitation::Column::Status,
            Expr::value(UserInvitationStatus::Expired),
        )
        .col_expr(user_invitation::Column::UpdatedAt, Expr::value(Utc::now()))
        .filter(user_invitation::Column::Id.eq(id))
        .filter(user_invitation::Column::Status.eq(UserInvitationStatus::Pending))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}

pub async fn mark_accepted_if_pending<C: ConnectionTrait>(
    db: &C,
    id: i64,
    accepted_user_id: i64,
) -> Result<bool> {
    let now = Utc::now();
    let result = UserInvitation::update_many()
        .col_expr(
            user_invitation::Column::Status,
            Expr::value(UserInvitationStatus::Accepted),
        )
        .col_expr(
            user_invitation::Column::AcceptedUserId,
            Expr::value(Some(accepted_user_id)),
        )
        .col_expr(user_invitation::Column::AcceptedAt, Expr::value(Some(now)))
        .col_expr(user_invitation::Column::UpdatedAt, Expr::value(now))
        .filter(user_invitation::Column::Id.eq(id))
        .filter(user_invitation::Column::Status.eq(UserInvitationStatus::Pending))
        .filter(user_invitation::Column::ExpiresAt.gt(now))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}

pub async fn save<C: ConnectionTrait>(
    db: &C,
    invitation: user_invitation::Model,
) -> Result<user_invitation::Model> {
    invitation
        .into_active_model()
        .update(db)
        .await
        .map_err(AsterError::from)
}
