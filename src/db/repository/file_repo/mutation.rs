//! `file_repo` 仓储子模块：`mutation`。

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, Set, sea_query::Expr,
};

use crate::entities::file::{self, Entity as File};
use crate::errors::{AsterError, Result};

use super::common::{map_bulk_name_db_err, map_name_db_err};

pub async fn create<C: ConnectionTrait>(db: &C, model: file::ActiveModel) -> Result<file::Model> {
    model.insert(db).await.map_err(AsterError::from)
}

#[derive(Debug, Clone)]
pub struct CreateFileWithBlobInput<'a> {
    pub name: &'a str,
    pub folder_id: Option<i64>,
    pub team_id: Option<i64>,
    pub blob_id: i64,
    pub size: i64,
    pub owner_user_id: Option<i64>,
    pub created_by_user_id: Option<i64>,
    pub created_by_username: &'a str,
    pub mime_type: &'a str,
    pub now: chrono::DateTime<Utc>,
}

pub async fn create_with_blob<C: ConnectionTrait>(
    db: &C,
    input: CreateFileWithBlobInput<'_>,
) -> Result<file::Model> {
    let CreateFileWithBlobInput {
        name,
        folder_id,
        team_id,
        blob_id,
        size,
        owner_user_id,
        created_by_user_id,
        created_by_username,
        mime_type,
        now,
    } = input;

    File::insert(file::ActiveModel {
        name: Set(name.to_string()),
        folder_id: Set(folder_id),
        team_id: Set(team_id),
        blob_id: Set(blob_id),
        size: Set(size),
        owner_user_id: Set(owner_user_id),
        created_by_user_id: Set(created_by_user_id),
        created_by_username: Set(created_by_username.to_string()),
        mime_type: Set(mime_type.to_string()),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    })
    .exec_with_returning(db)
    .await
    .map_err(|err| map_name_db_err(err, name))
}

/// 批量插入文件记录（不返回创建的 Model，批量复制用）
pub async fn create_many<C: ConnectionTrait>(db: &C, models: Vec<file::ActiveModel>) -> Result<()> {
    if models.is_empty() {
        return Ok(());
    }
    File::insert_many(models).exec(db).await.map_err(|err| {
        map_bulk_name_db_err(err, "one or more files already exist in this folder")
    })?;
    Ok(())
}

/// 批量移动文件到同一文件夹
pub async fn move_many_to_folder<C: ConnectionTrait>(
    db: &C,
    ids: &[i64],
    folder_id: Option<i64>,
    now: chrono::DateTime<Utc>,
) -> Result<()> {
    if ids.is_empty() {
        return Ok(());
    }
    File::update_many()
        .col_expr(file::Column::FolderId, Expr::value(folder_id))
        .col_expr(file::Column::UpdatedAt, Expr::value(now))
        .filter(file::Column::Id.is_in(ids.iter().copied()))
        .exec(db)
        .await
        .map_err(|err| {
            map_bulk_name_db_err(err, "one or more files already exist in target folder")
        })?;
    Ok(())
}
