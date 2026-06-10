//! 仓储模块：`user_profile_repo`。

use std::collections::HashMap;

use crate::entities::user_profile::{self, Entity as UserProfile};
use crate::errors::{AsterError, Result};
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

pub async fn find_by_user_id(
    db: &DatabaseConnection,
    user_id: i64,
) -> Result<Option<user_profile::Model>> {
    UserProfile::find_by_id(user_id)
        .one(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_by_user_ids(
    db: &DatabaseConnection,
    user_ids: &[i64],
) -> Result<HashMap<i64, user_profile::Model>> {
    if user_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let rows = UserProfile::find()
        .filter(user_profile::Column::UserId.is_in(user_ids.iter().copied()))
        .all(db)
        .await
        .map_err(AsterError::from)?;

    Ok(rows.into_iter().map(|row| (row.user_id, row)).collect())
}

pub async fn create(
    db: &DatabaseConnection,
    model: user_profile::ActiveModel,
) -> Result<user_profile::Model> {
    model.insert(db).await.map_err(AsterError::from)
}

pub async fn update(
    db: &DatabaseConnection,
    model: user_profile::ActiveModel,
) -> Result<user_profile::Model> {
    model.update(db).await.map_err(AsterError::from)
}
