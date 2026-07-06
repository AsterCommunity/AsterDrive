//! 仓储模块：`webdav_account_repo`。

use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, QueryFilter,
    QueryOrder,
};

use crate::db::repository::pagination_repo::fetch_offset_page;
use crate::entities::webdav_account::{self, Entity as WebdavAccount};
use crate::errors::{AsterError, Result};

pub async fn find_by_id(db: &DatabaseConnection, id: i64) -> Result<webdav_account::Model> {
    WebdavAccount::find_by_id(id)
        .one(db)
        .await
        .map_err(AsterError::from)?
        .ok_or_else(|| AsterError::record_not_found(format!("webdav_account #{id}")))
}

pub async fn find_by_username(
    db: &DatabaseConnection,
    username: &str,
) -> Result<Option<webdav_account::Model>> {
    WebdavAccount::find()
        .filter(webdav_account::Column::Username.eq(username))
        .one(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_by_user(
    db: &DatabaseConnection,
    user_id: i64,
) -> Result<Vec<webdav_account::Model>> {
    WebdavAccount::find()
        .filter(webdav_account::Column::UserId.eq(user_id))
        .filter(webdav_account::Column::TeamId.is_null())
        .order_by_asc(webdav_account::Column::Id)
        .all(db)
        .await
        .map_err(AsterError::from)
}

/// Enumerates every WebDAV account owned by the user, including team-linked
/// accounts. Unlike `find_by_user`, this intentionally does not filter `team_id`.
pub async fn find_all_by_user(
    db: &DatabaseConnection,
    user_id: i64,
) -> Result<Vec<webdav_account::Model>> {
    WebdavAccount::find()
        .filter(webdav_account::Column::UserId.eq(user_id))
        .order_by_asc(webdav_account::Column::Id)
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_by_team(
    db: &DatabaseConnection,
    team_id: i64,
) -> Result<Vec<webdav_account::Model>> {
    WebdavAccount::find()
        .filter(webdav_account::Column::TeamId.eq(team_id))
        .order_by_asc(webdav_account::Column::Id)
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_by_team_and_user(
    db: &DatabaseConnection,
    team_id: i64,
    user_id: i64,
) -> Result<Vec<webdav_account::Model>> {
    WebdavAccount::find()
        .filter(webdav_account::Column::TeamId.eq(team_id))
        .filter(webdav_account::Column::UserId.eq(user_id))
        .order_by_asc(webdav_account::Column::Id)
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_by_user_paginated(
    db: &DatabaseConnection,
    user_id: i64,
    limit: u64,
    offset: u64,
) -> Result<(Vec<webdav_account::Model>, u64)> {
    fetch_offset_page(
        db,
        WebdavAccount::find()
            .filter(webdav_account::Column::UserId.eq(user_id))
            .filter(webdav_account::Column::TeamId.is_null())
            .order_by_asc(webdav_account::Column::Id),
        limit,
        offset,
    )
    .await
}

pub async fn find_by_team_paginated(
    db: &DatabaseConnection,
    team_id: i64,
    limit: u64,
    offset: u64,
) -> Result<(Vec<webdav_account::Model>, u64)> {
    fetch_offset_page(
        db,
        WebdavAccount::find()
            .filter(webdav_account::Column::TeamId.eq(team_id))
            .order_by_asc(webdav_account::Column::Id),
        limit,
        offset,
    )
    .await
}

pub async fn find_by_team_and_user_paginated(
    db: &DatabaseConnection,
    team_id: i64,
    user_id: i64,
    limit: u64,
    offset: u64,
) -> Result<(Vec<webdav_account::Model>, u64)> {
    fetch_offset_page(
        db,
        WebdavAccount::find()
            .filter(webdav_account::Column::TeamId.eq(team_id))
            .filter(webdav_account::Column::UserId.eq(user_id))
            .order_by_asc(webdav_account::Column::Id),
        limit,
        offset,
    )
    .await
}

pub async fn create(
    db: &DatabaseConnection,
    model: webdav_account::ActiveModel,
) -> Result<webdav_account::Model> {
    model.insert(db).await.map_err(AsterError::from)
}

pub async fn update(
    db: &DatabaseConnection,
    model: webdav_account::ActiveModel,
) -> Result<webdav_account::Model> {
    model.update(db).await.map_err(AsterError::from)
}

pub async fn delete(db: &DatabaseConnection, id: i64) -> Result<()> {
    WebdavAccount::delete_by_id(id)
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(())
}

/// 批量删除用户的所有 WebDAV 账号
pub async fn delete_all_by_user<C: ConnectionTrait>(db: &C, user_id: i64) -> Result<u64> {
    let res = WebdavAccount::delete_many()
        .filter(webdav_account::Column::UserId.eq(user_id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(res.rows_affected)
}

pub async fn delete_all_by_team<C: ConnectionTrait>(db: &C, team_id: i64) -> Result<u64> {
    let res = WebdavAccount::delete_many()
        .filter(webdav_account::Column::TeamId.eq(team_id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(res.rows_affected)
}
