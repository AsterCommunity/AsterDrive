//! `folder_repo` 仓储子模块：`mutation`。

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, sea_query::Expr,
};

use crate::errors::{AsterError, Result};
use aster_drive_model::entities::folder::{self, Entity as Folder};

use super::common::{map_bulk_name_db_err, map_name_db_err};

pub async fn create<C: ConnectionTrait>(
    db: &C,
    model: folder::ActiveModel,
) -> Result<folder::Model> {
    let name = model.name.clone().take().unwrap_or_default();
    model
        .insert(db)
        .await
        .map_err(|err| map_name_db_err(err, &name))
}

/// 批量插入文件夹记录（不返回创建的 Model，目录树复制用）
pub async fn create_many<C: ConnectionTrait>(
    db: &C,
    models: Vec<folder::ActiveModel>,
) -> Result<()> {
    if models.is_empty() {
        return Ok(());
    }
    Folder::insert_many(models).exec(db).await.map_err(|err| {
        map_bulk_name_db_err(err, "one or more folders already exist in this location")
    })?;
    Ok(())
}

/// 批量移动文件夹到同一父文件夹
pub async fn move_many_to_parent<C: ConnectionTrait>(
    db: &C,
    ids: &[i64],
    parent_id: Option<i64>,
    now: chrono::DateTime<Utc>,
) -> Result<()> {
    if ids.is_empty() {
        return Ok(());
    }
    Folder::update_many()
        .col_expr(folder::Column::ParentId, Expr::value(parent_id))
        .col_expr(folder::Column::UpdatedAt, Expr::value(now))
        .filter(folder::Column::Id.is_in(ids.iter().copied()))
        .exec(db)
        .await
        .map_err(|err| {
            map_bulk_name_db_err(err, "one or more folders already exist in target folder")
        })?;
    Ok(())
}

/// 硬删除文件夹记录（回收站清理用）
pub async fn delete<C: ConnectionTrait>(db: &C, id: i64) -> Result<()> {
    Folder::delete_by_id(id)
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(())
}

/// 批量硬删除文件夹记录
pub async fn delete_many<C: ConnectionTrait>(db: &C, ids: &[i64]) -> Result<()> {
    if ids.is_empty() {
        return Ok(());
    }
    Folder::delete_many()
        .filter(folder::Column::Id.is_in(ids.iter().copied()))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(())
}

/// 清除引用某存储策略的所有 folder.policy_id（策略删除时调用）
pub async fn clear_policy_references<C: ConnectionTrait>(db: &C, policy_id: i64) -> Result<u64> {
    let result = Folder::update_many()
        .col_expr(folder::Column::PolicyId, Expr::value(Option::<i64>::None))
        .filter(folder::Column::PolicyId.eq(policy_id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected)
}
