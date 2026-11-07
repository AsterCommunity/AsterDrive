//! Persistence for bounded folder-tree mutation staging.

use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use sea_orm::{
    ActiveEnum, ColumnTrait, ConnectionTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
    QuerySelect,
    sea_query::{Expr, ExprTrait, Query},
};

use crate::errors::{AsterError, Result};
use aster_drive_model::entities::{file, folder, folder_tree_operation_member as member};
use aster_drive_model::types::EntityType;

pub async fn stage_ids<C: ConnectionTrait>(
    db: &C,
    task_id: i64,
    resource_kind: EntityType,
    resource_ids: &[i64],
) -> Result<u64> {
    if resource_ids.is_empty() {
        return Ok(0);
    }

    // Caller holds the task/root serialization guard. Query only this bounded
    // input batch; MySQL no-op duplicate updates cannot provide an insert count.
    let resource_ids = resource_ids.iter().copied().collect::<BTreeSet<_>>();
    let existing = member::Entity::find()
        .filter(member::Column::TaskId.eq(task_id))
        .filter(member::Column::ResourceKind.eq(resource_kind))
        .filter(member::Column::ResourceId.is_in(resource_ids.iter().copied()))
        .count(db)
        .await
        .map_err(AsterError::from)?;
    let inserted = resource_ids.len() as u64 - existing;
    let models = resource_ids
        .iter()
        .copied()
        .map(|resource_id| member::ActiveModel {
            task_id: sea_orm::Set(task_id),
            resource_kind: sea_orm::Set(resource_kind),
            resource_id: sea_orm::Set(resource_id),
        });
    member::Entity::insert_many(models)
        .on_conflict_do_nothing_on([
            member::Column::TaskId,
            member::Column::ResourceKind,
            member::Column::ResourceId,
        ])
        .exec_without_returning(db)
        .await
        .map_err(AsterError::from)?;
    Ok(inserted)
}

pub async fn count<C: ConnectionTrait>(db: &C, task_id: i64) -> Result<u64> {
    member::Entity::find()
        .filter(member::Column::TaskId.eq(task_id))
        .count(db)
        .await
        .map_err(AsterError::from)
}

pub async fn clear<C: ConnectionTrait>(db: &C, task_id: i64) -> Result<()> {
    member::Entity::delete_many()
        .filter(member::Column::TaskId.eq(task_id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(())
}

pub async fn apply_delete<C: ConnectionTrait>(
    db: &C,
    task_id: i64,
    deleted_at: DateTime<Utc>,
) -> Result<(u64, u64)> {
    let file_ids = member_ids_subquery(task_id, EntityType::File);
    let folder_ids = member_ids_subquery(task_id, EntityType::Folder);
    let files = file::Entity::update_many()
        .col_expr(file::Column::DeletedAt, Expr::value(Some(deleted_at)))
        .filter(file::Column::Id.in_subquery(file_ids))
        .exec(db)
        .await
        .map_err(AsterError::from)?
        .rows_affected;
    let folders = folder::Entity::update_many()
        .col_expr(folder::Column::DeletedAt, Expr::value(Some(deleted_at)))
        .filter(folder::Column::Id.in_subquery(folder_ids))
        .exec(db)
        .await
        .map_err(AsterError::from)?
        .rows_affected;
    Ok((files, folders))
}

pub async fn apply_restore<C: ConnectionTrait>(db: &C, task_id: i64) -> Result<(u64, u64)> {
    let file_ids = member_ids_subquery(task_id, EntityType::File);
    let folder_ids = member_ids_subquery(task_id, EntityType::Folder);
    let files = file::Entity::update_many()
        .col_expr(
            file::Column::DeletedAt,
            Expr::value(Option::<DateTime<Utc>>::None),
        )
        .filter(file::Column::Id.in_subquery(file_ids))
        .exec(db)
        .await
        .map_err(AsterError::from)?
        .rows_affected;
    let folders = folder::Entity::update_many()
        .col_expr(
            folder::Column::DeletedAt,
            Expr::value(Option::<DateTime<Utc>>::None),
        )
        .filter(folder::Column::Id.in_subquery(folder_ids))
        .exec(db)
        .await
        .map_err(AsterError::from)?
        .rows_affected;
    Ok((files, folders))
}

fn member_ids_subquery(
    task_id: i64,
    resource_kind: EntityType,
) -> sea_orm::sea_query::SelectStatement {
    Query::select()
        .column(member::Column::ResourceId)
        .from(member::Entity)
        .and_where(Expr::col(member::Column::TaskId).eq(task_id))
        .and_where(Expr::col(member::Column::ResourceKind).eq(resource_kind.to_value()))
        .to_owned()
}

/// Bounded staged resource snapshots, read under the task's transaction/operation lock.
pub async fn staged_files<C: ConnectionTrait>(
    db: &C,
    task_id: i64,
    after: i64,
) -> Result<Vec<file::Model>> {
    file::Entity::find()
        .filter(file::Column::Id.in_subquery(member_ids_subquery(task_id, EntityType::File)))
        .filter(file::Column::Id.gt(after))
        .order_by_asc(file::Column::Id)
        .limit(256)
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn staged_folders<C: ConnectionTrait>(
    db: &C,
    task_id: i64,
    after: i64,
) -> Result<Vec<folder::Model>> {
    folder::Entity::find()
        .filter(folder::Column::Id.in_subquery(member_ids_subquery(task_id, EntityType::Folder)))
        .filter(folder::Column::Id.gt(after))
        .order_by_asc(folder::Column::Id)
        .limit(256)
        .all(db)
        .await
        .map_err(AsterError::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn staging_returns_only_new_batch_members_after_duplicates_and_retries() {
        let mut db = sea_orm::Database::connect("sqlite::memory:").await.unwrap();
        db.execute_unprepared("CREATE TABLE folder_tree_operation_members (task_id BIGINT NOT NULL, resource_kind TEXT NOT NULL, resource_id BIGINT NOT NULL, PRIMARY KEY(task_id, resource_kind, resource_id))").await.unwrap();
        let queries = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let observed = queries.clone();
        db.set_metric_callback(move |info| {
            observed
                .lock()
                .unwrap()
                .push(info.statement.sql.to_ascii_lowercase());
        });
        assert_eq!(
            stage_ids(&db, 7, EntityType::Folder, &[1, 1, 2])
                .await
                .unwrap(),
            2
        );
        assert_eq!(
            stage_ids(&db, 7, EntityType::Folder, &[2, 3])
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            stage_ids(&db, 7, EntityType::Folder, &[1, 2, 3])
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            stage_ids(&db, 7, EntityType::File, &[1, 2]).await.unwrap(),
            2
        );
        assert_eq!(
            stage_ids(&db, 8, EntityType::Folder, &[1]).await.unwrap(),
            1
        );
        assert_eq!(stage_ids(&db, 7, EntityType::Folder, &[]).await.unwrap(), 0);
        let batch_queries = queries.lock().unwrap().clone();
        assert!(
            batch_queries
                .iter()
                .filter(|sql| sql.contains("count("))
                .all(|sql| sql.contains("resource_id") && sql.contains(" in ("))
        );
        assert_eq!(count(&db, 7).await.unwrap(), 5);
    }
}
