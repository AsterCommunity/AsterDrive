//! 仓储模块：`user_profile_repo`。

use std::collections::HashMap;

use crate::errors::{AsterError, Result};
use aster_drive_model::entities::user_profile::{self, Entity as UserProfile};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, DbBackend, EntityTrait,
    QueryFilter, QuerySelect, Select,
};

pub async fn find_by_user_id(
    db: &DatabaseConnection,
    user_id: i64,
) -> Result<Option<user_profile::Model>> {
    UserProfile::find_by_id(user_id)
        .one(db)
        .await
        .map_err(AsterError::from)
}

fn find_by_id_for_update(backend: DbBackend, user_id: i64) -> Select<UserProfile> {
    let query = UserProfile::find_by_id(user_id);
    match backend {
        DbBackend::Postgres | DbBackend::MySql => query.lock_exclusive(),
        _ => query,
    }
}

pub async fn lock_by_user_id<C: ConnectionTrait>(
    db: &C,
    user_id: i64,
) -> Result<Option<user_profile::Model>> {
    find_by_id_for_update(db.get_database_backend(), user_id)
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

pub async fn create<C: ConnectionTrait>(
    db: &C,
    model: user_profile::ActiveModel,
) -> Result<user_profile::Model> {
    model.insert(db).await.map_err(AsterError::from)
}

pub async fn update<C: ConnectionTrait>(
    db: &C,
    model: user_profile::ActiveModel,
) -> Result<user_profile::Model> {
    model.update(db).await.map_err(AsterError::from)
}

#[cfg(test)]
mod tests {
    use sea_orm::QueryTrait;

    use super::*;

    #[test]
    fn profile_lock_query_uses_for_update_only_on_supported_backends() {
        let sqlite = find_by_id_for_update(DbBackend::Sqlite, 42)
            .build(DbBackend::Sqlite)
            .to_string();
        let postgres = find_by_id_for_update(DbBackend::Postgres, 42)
            .build(DbBackend::Postgres)
            .to_string();
        let mysql = find_by_id_for_update(DbBackend::MySql, 42)
            .build(DbBackend::MySql)
            .to_string();

        assert!(!sqlite.contains("FOR UPDATE"), "{sqlite}");
        assert!(postgres.contains("FOR UPDATE"), "{postgres}");
        assert!(mysql.contains("FOR UPDATE"), "{mysql}");
    }
}
