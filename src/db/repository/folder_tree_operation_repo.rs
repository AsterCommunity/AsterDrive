//! Persistence for bounded folder-tree mutation staging.

use chrono::{DateTime, Utc};
use sea_orm::{
    ActiveEnum, ColumnTrait, ConnectionTrait, EntityTrait, PaginatorTrait, QueryFilter,
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
) -> Result<()> {
    if resource_ids.is_empty() {
        return Ok(());
    }

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
    Ok(())
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
