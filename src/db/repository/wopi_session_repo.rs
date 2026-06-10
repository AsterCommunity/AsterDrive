//! 仓储模块：`wopi_session_repo`。

use crate::entities::wopi_session::{self, Entity as WopiSession};
use crate::errors::{AsterError, Result};
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

pub async fn create(
    db: &DatabaseConnection,
    model: wopi_session::ActiveModel,
) -> Result<wopi_session::Model> {
    model.insert(db).await.map_err(AsterError::from)
}

pub async fn find_by_token_hash(
    db: &DatabaseConnection,
    token_hash: &str,
) -> Result<Option<wopi_session::Model>> {
    WopiSession::find()
        .filter(wopi_session::Column::TokenHash.eq(token_hash))
        .one(db)
        .await
        .map_err(AsterError::from)
}

pub async fn delete_by_id(db: &DatabaseConnection, id: i64) -> Result<()> {
    WopiSession::delete_by_id(id)
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(())
}

pub async fn delete_expired(db: &DatabaseConnection) -> Result<u64> {
    let result = WopiSession::delete_many()
        .filter(wopi_session::Column::ExpiresAt.lt(Utc::now()))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected)
}
