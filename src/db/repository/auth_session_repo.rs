//! 仓储模块：`auth_session_repo`。

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, ExprTrait,
    QueryFilter, QueryOrder, sea_query::Expr,
};

use crate::errors::{AsterError, Result};
use aster_drive_model::entities::auth_session::{self, Entity as AuthSession};

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

/// Product persistence inputs for one conditional refresh mutation. No credential strings are held.
pub struct RefreshRotation<'a> {
    /// Account fence already validated by the caller's transaction.
    pub user_id: i64,
    /// Stable session identity, preventing rotation across sessions.
    pub session_id: &'a str,
    /// Identity the conditional update must consume exactly once.
    pub current_refresh_jti: &'a str,
    /// Unique replacement identity; schema constraints remain authoritative.
    pub next_refresh_jti: &'a str,
    /// Replacement credential's exclusive expiry.
    pub refresh_expires_at: chrono::DateTime<Utc>,
    /// Current request metadata; absent values preserve history.
    pub ip_address: Option<&'a str>,
    /// Current request metadata; never synthesize replay evidence from history.
    pub user_agent: Option<&'a str>,
    /// Decision time; persisted last-seen time never moves backwards.
    pub last_seen_at: chrono::DateTime<Utc>,
}

/// Performs the authoritative refresh CAS, fenced by account, stable session, current JTI,
/// revocation and expiry. Account/version checks belong to the caller's writer transaction.
/// Missing request metadata preserves stored values, but must not be used as replay evidence.
///
/// # Errors
/// Returns the product database error; `false` means the conditional mutation lost its fence.
pub async fn rotate_refresh_fenced<C: ConnectionTrait>(
    db: &C,
    rotation: RefreshRotation<'_>,
) -> Result<bool> {
    let RefreshRotation {
        user_id,
        session_id,
        current_refresh_jti,
        next_refresh_jti,
        refresh_expires_at,
        ip_address,
        user_agent,
        last_seen_at,
    } = rotation;
    let mut query = AuthSession::update_many()
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
        // JWT decisions have second precision; persisted timestamps can have subsecond precision.
        .col_expr(
            auth_session::Column::LastSeenAt,
            Expr::case(
                Expr::col(auth_session::Column::LastSeenAt).gt(last_seen_at),
                Expr::col(auth_session::Column::LastSeenAt),
            )
            .finally(last_seen_at)
            .into(),
        )
        .filter(auth_session::Column::Id.eq(session_id))
        .filter(auth_session::Column::UserId.eq(user_id))
        .filter(auth_session::Column::CurrentRefreshJti.eq(current_refresh_jti))
        .filter(auth_session::Column::RevokedAt.is_null())
        .filter(auth_session::Column::RefreshExpiresAt.gt(last_seen_at));
    if let Some(ip_address) = ip_address {
        query = query.col_expr(auth_session::Column::IpAddress, Expr::value(ip_address));
    }
    if let Some(user_agent) = user_agent {
        query = query.col_expr(auth_session::Column::UserAgent, Expr::value(user_agent));
    }
    let result = query.exec(db).await.map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}

pub async fn delete<C: ConnectionTrait>(db: &C, id: &str) -> Result<bool> {
    let result = AuthSession::delete_by_id(id.to_string())
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
