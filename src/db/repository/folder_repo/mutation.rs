//! `folder_repo` 仓储子模块：`mutation`。

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DbBackend, EntityTrait, QueryFilter,
    TryInsertResult,
    sea_query::{Expr, OnConflict},
};

use crate::errors::{AsterError, Result};
use aster_drive_model::entities::folder::{self, Entity as Folder};

use super::common::{FolderScope, map_bulk_name_db_err, map_name_db_err};

pub async fn create_or_find_by_name_in_parent<C: ConnectionTrait>(
    db: &C,
    model: folder::ActiveModel,
    user_id: i64,
    parent_id: Option<i64>,
    name: &str,
) -> Result<folder::Model> {
    create_or_find_in_scope(
        db,
        model,
        FolderScope::Personal { user_id },
        parent_id,
        name,
    )
    .await
}

pub async fn create_or_find_by_name_in_parent_with_created<C: ConnectionTrait>(
    db: &C,
    model: folder::ActiveModel,
    user_id: i64,
    parent_id: Option<i64>,
    name: &str,
) -> Result<(folder::Model, bool)> {
    create_or_find_in_scope_with_created(
        db,
        model,
        FolderScope::Personal { user_id },
        parent_id,
        name,
    )
    .await
}

pub async fn create_or_find_by_name_in_team_parent<C: ConnectionTrait>(
    db: &C,
    model: folder::ActiveModel,
    team_id: i64,
    parent_id: Option<i64>,
    name: &str,
) -> Result<folder::Model> {
    create_or_find_in_scope(db, model, FolderScope::Team { team_id }, parent_id, name).await
}

pub async fn create_or_find_by_name_in_team_parent_with_created<C: ConnectionTrait>(
    db: &C,
    model: folder::ActiveModel,
    team_id: i64,
    parent_id: Option<i64>,
    name: &str,
) -> Result<(folder::Model, bool)> {
    create_or_find_in_scope_with_created(db, model, FolderScope::Team { team_id }, parent_id, name)
        .await
}

async fn create_or_find_in_scope<C: ConnectionTrait>(
    db: &C,
    model: folder::ActiveModel,
    scope: FolderScope,
    parent_id: Option<i64>,
    name: &str,
) -> Result<folder::Model> {
    if let Some(created) = insert_without_conflict(db, model).await? {
        return Ok(created);
    }

    let existing = match scope {
        FolderScope::Personal { user_id } => {
            super::query::lock_by_name_in_parent(db, user_id, parent_id, name).await?
        }
        FolderScope::Team { team_id } => {
            super::query::lock_by_name_in_team_parent(db, team_id, parent_id, name).await?
        }
    };
    existing
        .ok_or_else(|| AsterError::internal_error("folder insert conflict could not be reloaded"))
}

async fn create_or_find_in_scope_with_created<C: ConnectionTrait>(
    db: &C,
    model: folder::ActiveModel,
    scope: FolderScope,
    parent_id: Option<i64>,
    name: &str,
) -> Result<(folder::Model, bool)> {
    if let Some(created) = insert_without_conflict(db, model).await? {
        return Ok((created, true));
    }
    let existing = match scope {
        FolderScope::Personal { user_id } => {
            super::query::lock_by_name_in_parent(db, user_id, parent_id, name).await?
        }
        FolderScope::Team { team_id } => {
            super::query::lock_by_name_in_team_parent(db, team_id, parent_id, name).await?
        }
    };
    existing
        .map(|folder| (folder, false))
        .ok_or_else(|| AsterError::internal_error("folder insert conflict could not be reloaded"))
}

async fn insert_without_conflict<C: ConnectionTrait>(
    db: &C,
    model: folder::ActiveModel,
) -> Result<Option<folder::Model>> {
    // The live folder-name index is an expression index, so it cannot be named as a portable
    // PostgreSQL conflict target. PostgreSQL/SQLite accept an unqualified DO NOTHING. MySQL
    // uses LAST_INSERT_ID(id) so the duplicate winner remains visible under REPEATABLE READ.
    match Folder::insert(model)
        .on_conflict(folder_insert_conflict(db.get_database_backend()))
        .try_insert()
        .exec(db)
        .await
        .map_err(AsterError::from)?
    {
        TryInsertResult::Inserted(result) => Folder::find_by_id(result.last_insert_id)
            .one(db)
            .await
            .map_err(AsterError::from),
        TryInsertResult::Conflicted => Ok(None),
        TryInsertResult::Empty => Err(AsterError::internal_error(
            "folder insert produced no row or conflict result",
        )),
    }
}

fn folder_insert_conflict(backend: DbBackend) -> OnConflict {
    if backend == DbBackend::MySql {
        let mut on_conflict = OnConflict::columns([folder::Column::Id]);
        on_conflict.value(folder::Column::Id, Expr::cust("LAST_INSERT_ID(id)"));
        on_conflict
    } else {
        OnConflict::new().do_nothing().to_owned()
    }
}

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

#[cfg(test)]
mod tests {
    use sea_orm::{DbBackend, EntityTrait, QueryTrait, Set};

    use super::{folder, folder_insert_conflict};

    fn insert_sql(backend: DbBackend) -> String {
        folder::Entity::insert(folder::ActiveModel {
            name: Set("parallel".to_string()),
            ..Default::default()
        })
        .on_conflict(folder_insert_conflict(backend))
        .build(backend)
        .to_string()
    }

    #[test]
    fn folder_conflict_clause_is_backend_specific_and_non_failing() {
        let postgres = insert_sql(DbBackend::Postgres);
        assert!(
            postgres.contains("ON CONFLICT") && postgres.contains("DO NOTHING"),
            "{postgres}"
        );

        let sqlite = insert_sql(DbBackend::Sqlite);
        assert!(
            sqlite.contains("ON CONFLICT") && sqlite.contains("DO NOTHING"),
            "{sqlite}"
        );

        let mysql = insert_sql(DbBackend::MySql);
        assert!(
            mysql.contains("ON DUPLICATE KEY UPDATE `id` = LAST_INSERT_ID(id)"),
            "{mysql}"
        );
    }
}
