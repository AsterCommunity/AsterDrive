//! 仓储模块：`auth_session_repo`。

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, QueryFilter,
    QueryOrder, sea_query::Expr,
};

use crate::entities::auth_session::{self, Entity as AuthSession};
use crate::errors::{AsterError, Result};

pub async fn create<C: ConnectionTrait>(
    db: &C,
    model: auth_session::ActiveModel,
) -> Result<auth_session::Model> {
    model.insert(db).await.map_err(AsterError::from)
}

pub async fn find_by_id(db: &DatabaseConnection, id: &str) -> Result<Option<auth_session::Model>> {
    AuthSession::find_by_id(id.to_string())
        .one(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_by_id_for_user<C: ConnectionTrait>(
    db: &C,
    user_id: i64,
    id: &str,
) -> Result<Option<auth_session::Model>> {
    AuthSession::find()
        .filter(auth_session::Column::UserId.eq(user_id))
        .filter(auth_session::Column::Id.eq(id))
        .one(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_by_refresh_jti<C: ConnectionTrait>(
    db: &C,
    refresh_jti: &str,
) -> Result<Option<auth_session::Model>> {
    AuthSession::find()
        .filter(auth_session::Column::CurrentRefreshJti.eq(refresh_jti))
        .one(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_by_previous_refresh_jti<C: ConnectionTrait>(
    db: &C,
    refresh_jti: &str,
) -> Result<Option<auth_session::Model>> {
    AuthSession::find()
        .filter(auth_session::Column::PreviousRefreshJti.eq(refresh_jti))
        .one(db)
        .await
        .map_err(AsterError::from)
}

pub async fn list_active_for_user(
    db: &DatabaseConnection,
    user_id: i64,
) -> Result<Vec<auth_session::Model>> {
    AuthSession::find()
        .filter(auth_session::Column::UserId.eq(user_id))
        .filter(auth_session::Column::RevokedAt.is_null())
        .filter(auth_session::Column::RefreshExpiresAt.gt(Utc::now()))
        .order_by_desc(auth_session::Column::LastSeenAt)
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn rotate_refresh<C: ConnectionTrait>(
    db: &C,
    current_refresh_jti: &str,
    next_refresh_jti: &str,
    refresh_expires_at: chrono::DateTime<Utc>,
    ip_address: Option<&str>,
    user_agent: Option<&str>,
    last_seen_at: chrono::DateTime<Utc>,
) -> Result<bool> {
    let result = AuthSession::update_many()
        .col_expr(
            auth_session::Column::CurrentRefreshJti,
            Expr::value(next_refresh_jti.to_string()),
        )
        .col_expr(
            auth_session::Column::PreviousRefreshJti,
            Expr::value(Some(current_refresh_jti.to_string())),
        )
        .col_expr(
            auth_session::Column::RefreshExpiresAt,
            Expr::value(refresh_expires_at),
        )
        .col_expr(
            auth_session::Column::IpAddress,
            Expr::value(ip_address.map(str::to_string)),
        )
        .col_expr(
            auth_session::Column::UserAgent,
            Expr::value(user_agent.map(str::to_string)),
        )
        .col_expr(auth_session::Column::LastSeenAt, Expr::value(last_seen_at))
        .col_expr(
            auth_session::Column::RevokedAt,
            Expr::value(Option::<chrono::DateTime<Utc>>::None),
        )
        .filter(auth_session::Column::CurrentRefreshJti.eq(current_refresh_jti))
        .filter(auth_session::Column::RevokedAt.is_null())
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}

pub async fn delete<C: ConnectionTrait>(db: &C, id: &str) -> Result<bool> {
    let result = AuthSession::delete_by_id(id.to_string())
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}

pub async fn delete_by_refresh_jti<C: ConnectionTrait>(db: &C, refresh_jti: &str) -> Result<bool> {
    let result = AuthSession::delete_many()
        .filter(auth_session::Column::CurrentRefreshJti.eq(refresh_jti))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}

pub async fn revoke_by_refresh_jti(
    db: &DatabaseConnection,
    refresh_jti: &str,
    revoked_at: chrono::DateTime<Utc>,
) -> Result<bool> {
    let result = AuthSession::update_many()
        .col_expr(
            auth_session::Column::RevokedAt,
            Expr::value(Some(revoked_at)),
        )
        .filter(auth_session::Column::CurrentRefreshJti.eq(refresh_jti))
        .filter(auth_session::Column::RevokedAt.is_null())
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}

pub async fn delete_all_for_user<C: ConnectionTrait>(db: &C, user_id: i64) -> Result<u64> {
    let result = AuthSession::delete_many()
        .filter(auth_session::Column::UserId.eq(user_id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected)
}

pub async fn delete_all_for_user_except_id<C: ConnectionTrait>(
    db: &C,
    user_id: i64,
    keep_id: &str,
) -> Result<u64> {
    let result = AuthSession::delete_many()
        .filter(auth_session::Column::UserId.eq(user_id))
        .filter(auth_session::Column::Id.ne(keep_id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected)
}

pub async fn revoke_by_id_for_user<C: ConnectionTrait>(
    db: &C,
    user_id: i64,
    id: &str,
    revoked_at: chrono::DateTime<Utc>,
) -> Result<bool> {
    let result = AuthSession::update_many()
        .col_expr(
            auth_session::Column::RevokedAt,
            Expr::value(Some(revoked_at)),
        )
        .filter(auth_session::Column::UserId.eq(user_id))
        .filter(auth_session::Column::Id.eq(id))
        .filter(auth_session::Column::RevokedAt.is_null())
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}

pub async fn revoke_all_for_user_except_id<C: ConnectionTrait>(
    db: &C,
    user_id: i64,
    keep_id: &str,
    revoked_at: chrono::DateTime<Utc>,
) -> Result<u64> {
    let result = AuthSession::update_many()
        .col_expr(
            auth_session::Column::RevokedAt,
            Expr::value(Some(revoked_at)),
        )
        .filter(auth_session::Column::UserId.eq(user_id))
        .filter(auth_session::Column::Id.ne(keep_id))
        .filter(auth_session::Column::RevokedAt.is_null())
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected)
}

pub async fn delete_expired(db: &DatabaseConnection) -> Result<u64> {
    let result = AuthSession::delete_many()
        .filter(auth_session::Column::RefreshExpiresAt.lt(Utc::now()))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected)
}
