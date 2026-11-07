//! `file_repo` 仓储子模块：`trash`。

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, ConnectionTrait, EntityTrait, ExprTrait,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Set, sea_query::Expr,
};

use crate::errors::{AsterError, Result};
use aster_drive_model::entities::file::{self, Entity as File};

use super::common::{FileScope, map_bulk_name_db_err, map_name_db_err, scope_condition};
use super::query::find_by_id;

/// 查询顶层已删除文件（cursor 分页），cursor = (deleted_at, id) 降序
fn top_level_deleted_condition(scope: FileScope) -> Condition {
    use sea_orm::sea_query::{Alias, Expr, Query};

    let folder_deleted_subquery = Query::select()
        .expr(Expr::val(1i32))
        .from_as(Alias::new("folders"), Alias::new("f2"))
        .and_where(
            Expr::col((Alias::new("f2"), Alias::new("id")))
                .equals((Alias::new("files"), file::Column::FolderId)),
        )
        .and_where(Expr::col((Alias::new("f2"), Alias::new("deleted_at"))).is_not_null())
        .to_owned();

    scope_condition(scope)
        .add(file::Column::DeletedAt.is_not_null())
        .add(
            Condition::any()
                .add(file::Column::FolderId.is_null())
                .add(Expr::exists(folder_deleted_subquery).not()),
        )
}

async fn find_top_level_deleted_paginated_in_scope<C: ConnectionTrait>(
    db: &C,
    scope: FileScope,
    limit: u64,
    after: Option<(chrono::DateTime<Utc>, i64)>,
) -> Result<(Vec<file::Model>, u64)> {
    let base_cond = top_level_deleted_condition(scope);
    let base = File::find().filter(base_cond);

    let total = base.clone().count(db).await.map_err(AsterError::from)?;
    if total == 0 || limit == 0 {
        return Ok((vec![], total));
    }

    let items = find_top_level_deleted_batch(db, scope, limit, after).await?;
    Ok((items, total))
}

/// Cursor-only scan for workers; listing totals belong to the UI query.
pub(crate) async fn find_top_level_deleted_batch<C: ConnectionTrait>(
    db: &C,
    scope: FileScope,
    limit: u64,
    after: Option<(chrono::DateTime<Utc>, i64)>,
) -> Result<Vec<file::Model>> {
    let mut q = File::find().filter(top_level_deleted_condition(scope));
    if let Some((after_deleted_at, after_id)) = after {
        q = q.filter(
            Condition::any()
                .add(file::Column::DeletedAt.lt(after_deleted_at))
                .add(
                    Condition::all()
                        .add(file::Column::DeletedAt.eq(after_deleted_at))
                        .add(file::Column::Id.gt(after_id)),
                ),
        );
    }

    let items = q
        .order_by_desc(file::Column::DeletedAt)
        .order_by_asc(file::Column::Id)
        .limit(limit)
        .all(db)
        .await
        .map_err(AsterError::from)?;

    Ok(items)
}

pub async fn find_top_level_deleted_paginated<C: ConnectionTrait>(
    db: &C,
    user_id: i64,
    limit: u64,
    after: Option<(chrono::DateTime<chrono::Utc>, i64)>,
) -> Result<(Vec<file::Model>, u64)> {
    find_top_level_deleted_paginated_in_scope(db, FileScope::Personal { user_id }, limit, after)
        .await
}

pub async fn find_top_level_deleted_by_team_paginated<C: ConnectionTrait>(
    db: &C,
    team_id: i64,
    limit: u64,
    after: Option<(chrono::DateTime<Utc>, i64)>,
) -> Result<(Vec<file::Model>, u64)> {
    find_top_level_deleted_paginated_in_scope(db, FileScope::Team { team_id }, limit, after).await
}

/// 硬删除文件记录（回收站清理用）
pub async fn delete<C: ConnectionTrait>(db: &C, id: i64) -> Result<()> {
    File::delete_by_id(id)
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(())
}

/// 批量硬删除文件记录
pub async fn delete_many<C: ConnectionTrait>(db: &C, ids: &[i64]) -> Result<()> {
    if ids.is_empty() {
        return Ok(());
    }
    File::delete_many()
        .filter(file::Column::Id.is_in(ids.iter().copied()))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(())
}

/// 软删除：标记 deleted_at
pub async fn soft_delete<C: ConnectionTrait>(db: &C, id: i64) -> Result<()> {
    let f = find_by_id(db, id).await?;
    let mut active: file::ActiveModel = f.into();
    active.deleted_at = Set(Some(Utc::now()));
    active.update(db).await.map_err(AsterError::from)?;
    Ok(())
}

/// 批量软删除：一次 UPDATE 标记多个文件的 deleted_at
pub async fn soft_delete_many<C: ConnectionTrait>(
    db: &C,
    ids: &[i64],
    now: chrono::DateTime<Utc>,
) -> Result<()> {
    if ids.is_empty() {
        return Ok(());
    }
    File::update_many()
        .col_expr(file::Column::DeletedAt, Expr::value(Some(now)))
        .filter(file::Column::Id.is_in(ids.iter().copied()))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(())
}

/// 恢复：清除 deleted_at
pub async fn restore<C: ConnectionTrait>(db: &C, id: i64) -> Result<()> {
    let f = find_by_id(db, id).await?;
    let name = f.name.clone();
    let mut active: file::ActiveModel = f.into();
    active.deleted_at = Set(None);
    active
        .update(db)
        .await
        .map_err(|err| map_name_db_err(err, &name))?;
    Ok(())
}

/// 批量恢复：一次 UPDATE 清除多个文件的 deleted_at
pub async fn restore_many<C: ConnectionTrait>(db: &C, ids: &[i64]) -> Result<()> {
    if ids.is_empty() {
        return Ok(());
    }
    File::update_many()
        .col_expr(
            file::Column::DeletedAt,
            Expr::value(Option::<chrono::DateTime<Utc>>::None),
        )
        .filter(file::Column::Id.is_in(ids.iter().copied()))
        .exec(db)
        .await
        .map_err(|err| {
            map_bulk_name_db_err(
                err,
                "one or more files already exist in their original folders",
            )
        })?;
    Ok(())
}

/// 查询用户回收站中的文件
pub async fn find_deleted_by_user<C: ConnectionTrait>(
    db: &C,
    user_id: i64,
) -> Result<Vec<file::Model>> {
    File::find()
        .filter(file::Column::OwnerUserId.eq(user_id))
        .filter(file::Column::TeamId.is_null())
        .filter(file::Column::DeletedAt.is_not_null())
        .order_by_desc(file::Column::DeletedAt)
        .all(db)
        .await
        .map_err(AsterError::from)
}

/// 查询某文件夹下的已删除文件（递归恢复/清理用，避免 N+1）
pub async fn find_deleted_in_folder<C: ConnectionTrait>(
    db: &C,
    folder_id: i64,
) -> Result<Vec<file::Model>> {
    File::find()
        .filter(file::Column::FolderId.eq(folder_id))
        .filter(file::Column::DeletedAt.is_not_null())
        .all(db)
        .await
        .map_err(AsterError::from)
}

/// 查询过期的已删除文件（自动清理用）
pub async fn find_expired_deleted<C: ConnectionTrait>(
    db: &C,
    before: chrono::DateTime<Utc>,
) -> Result<Vec<file::Model>> {
    File::find()
        .filter(file::Column::DeletedAt.is_not_null())
        .filter(file::Column::DeletedAt.lt(before))
        .all(db)
        .await
        .map_err(AsterError::from)
}

/// 查询用户的所有文件（含已删除，force_delete 用）
pub async fn find_all_by_user<C: ConnectionTrait>(
    db: &C,
    user_id: i64,
) -> Result<Vec<file::Model>> {
    File::find()
        .filter(file::Column::OwnerUserId.eq(user_id))
        .filter(file::Column::TeamId.is_null())
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_all_by_user_paginated<C: ConnectionTrait>(
    db: &C,
    user_id: i64,
    after_id: Option<i64>,
    limit: u64,
) -> Result<Vec<file::Model>> {
    let mut query = File::find()
        .filter(file::Column::OwnerUserId.eq(user_id))
        .filter(file::Column::TeamId.is_null())
        .order_by_asc(file::Column::Id)
        .limit(limit);
    if let Some(after_id) = after_id {
        query = query.filter(file::Column::Id.gt(after_id));
    }
    query.all(db).await.map_err(AsterError::from)
}

pub async fn find_all_by_team<C: ConnectionTrait>(
    db: &C,
    team_id: i64,
) -> Result<Vec<file::Model>> {
    File::find()
        .filter(file::Column::TeamId.eq(team_id))
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_all_by_team_paginated<C: ConnectionTrait>(
    db: &C,
    team_id: i64,
    after_id: Option<i64>,
    limit: u64,
) -> Result<Vec<file::Model>> {
    let mut query = File::find()
        .filter(file::Column::TeamId.eq(team_id))
        .order_by_asc(file::Column::Id)
        .limit(limit);
    if let Some(after_id) = after_id {
        query = query.filter(file::Column::Id.gt(after_id));
    }
    query.all(db).await.map_err(AsterError::from)
}

#[cfg(test)]
mod cursor_budget_tests {
    use super::*;
    use aster_drive_model::entities::folder;
    use sea_orm::{ActiveModelTrait, Database, DbBackend, Schema, Set};
    use std::sync::{Arc, Mutex};

    #[tokio::test]
    async fn purge_cursors_handle_equal_timestamps_without_counting_remaining_trash() {
        let mut db = Database::connect("sqlite::memory:").await.unwrap();
        db.execute_unprepared("PRAGMA foreign_keys = OFF")
            .await
            .unwrap();
        let schema = Schema::new(DbBackend::Sqlite);
        db.execute(&schema.create_table_from_entity(file::Entity))
            .await
            .unwrap();
        db.execute(&schema.create_table_from_entity(folder::Entity))
            .await
            .unwrap();
        let now = Utc::now();
        for id in 1..=5 {
            file::ActiveModel {
                id: Set(id),
                name: Set(format!("file-{id}")),
                blob_id: Set(1),
                size: Set(0),
                owner_user_id: Set(Some(7)),
                created_by_user_id: Set(Some(7)),
                created_by_username: Set("user".to_string()),
                mime_type: Set("text/plain".to_string()),
                created_at: Set(now),
                updated_at: Set(now),
                deleted_at: Set(Some(now)),
                ..Default::default()
            }
            .insert(&db)
            .await
            .unwrap();
            folder::ActiveModel {
                id: Set(id),
                name: Set(format!("folder-{id}")),
                owner_user_id: Set(Some(7)),
                created_by_user_id: Set(Some(7)),
                created_by_username: Set("user".to_string()),
                icon_kind: Set(aster_drive_model::types::FolderIconKind::Default),
                created_at: Set(now),
                updated_at: Set(now),
                deleted_at: Set(Some(now)),
                ..Default::default()
            }
            .insert(&db)
            .await
            .unwrap();
        }
        let queries = Arc::new(Mutex::new(Vec::new()));
        let observed = queries.clone();
        db.set_metric_callback(move |info| {
            observed.lock().unwrap().push(info.statement.sql.clone());
        });
        let mut file_ids = Vec::new();
        let mut folder_ids = Vec::new();
        let mut after = None;
        loop {
            let batch =
                find_top_level_deleted_batch(&db, FileScope::Personal { user_id: 7 }, 2, after)
                    .await
                    .unwrap();
            if batch.is_empty() {
                break;
            }
            after = batch.last().map(|file| (file.deleted_at.unwrap(), file.id));
            file_ids.extend(batch.into_iter().map(|file| file.id));
        }
        let mut after = None;
        loop {
            let batch = crate::db::repository::folder_repo::find_top_level_deleted_batch(
                &db,
                crate::db::repository::folder_repo::FolderScope::Personal { user_id: 7 },
                2,
                after,
            )
            .await
            .unwrap();
            if batch.is_empty() {
                break;
            }
            after = batch
                .last()
                .map(|folder| (folder.deleted_at.unwrap(), folder.id));
            folder_ids.extend(batch.into_iter().map(|folder| folder.id));
        }
        assert_eq!(file_ids, vec![1, 2, 3, 4, 5]);
        assert_eq!(folder_ids, vec![5, 4, 3, 2, 1]);
        let queries = queries.lock().unwrap();
        assert_eq!(queries.len(), 8);
        assert!(
            queries
                .iter()
                .all(|sql| !sql.to_ascii_lowercase().contains("count(")),
            "{queries:?}"
        );
    }
}
