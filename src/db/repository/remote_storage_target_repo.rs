//! 仓储模块：`remote_storage_target_repo`。

use crate::errors::{AsterError, Result};
use aster_drive_model::entities::remote_storage_target::{self, Entity as RemoteStorageTarget};
use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, PaginatorTrait, QueryFilter,
    QueryOrder,
};

pub async fn find_by_id<C: ConnectionTrait>(
    db: &C,
    id: i64,
) -> Result<remote_storage_target::Model> {
    RemoteStorageTarget::find_by_id(id)
        .one(db)
        .await
        .map_err(AsterError::from)?
        .ok_or_else(|| AsterError::record_not_found(format!("remote_storage_target #{id}")))
}

pub async fn find_by_binding_and_target_key<C: ConnectionTrait>(
    db: &C,
    master_binding_id: i64,
    target_key: &str,
) -> Result<Option<remote_storage_target::Model>> {
    RemoteStorageTarget::find()
        .filter(remote_storage_target::Column::MasterBindingId.eq(master_binding_id))
        .filter(remote_storage_target::Column::TargetKey.eq(target_key))
        .one(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_all_by_binding<C: ConnectionTrait>(
    db: &C,
    master_binding_id: i64,
) -> Result<Vec<remote_storage_target::Model>> {
    RemoteStorageTarget::find()
        .filter(remote_storage_target::Column::MasterBindingId.eq(master_binding_id))
        .order_by_desc(remote_storage_target::Column::CreatedAt)
        .order_by_desc(remote_storage_target::Column::Id)
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn count_by_binding<C: ConnectionTrait>(db: &C, master_binding_id: i64) -> Result<u64> {
    RemoteStorageTarget::find()
        .filter(remote_storage_target::Column::MasterBindingId.eq(master_binding_id))
        .count(db)
        .await
        .map_err(AsterError::from)
}

pub async fn create<C: ConnectionTrait>(
    db: &C,
    model: remote_storage_target::ActiveModel,
) -> Result<remote_storage_target::Model> {
    model.insert(db).await.map_err(AsterError::from)
}

pub async fn update<C: ConnectionTrait>(
    db: &C,
    model: remote_storage_target::ActiveModel,
) -> Result<remote_storage_target::Model> {
    model.update(db).await.map_err(AsterError::from)
}

/// Persist reconciliation state only while the target still has the desired
/// revision that was validated. A stale result is ignored and the latest row
/// is returned without touching connector configuration.
pub async fn update_reconciliation_if_revision<C: ConnectionTrait>(
    db: &C,
    target_id: i64,
    desired_revision: i64,
    applied_revision: Option<i64>,
    last_error: &str,
) -> Result<remote_storage_target::Model> {
    let mut query = RemoteStorageTarget::update_many()
        .filter(remote_storage_target::Column::Id.eq(target_id))
        .filter(remote_storage_target::Column::DesiredRevision.eq(desired_revision))
        .col_expr(
            remote_storage_target::Column::LastError,
            Expr::value(last_error.to_string()),
        )
        .col_expr(
            remote_storage_target::Column::UpdatedAt,
            Expr::value(chrono::Utc::now()),
        );
    if let Some(applied_revision) = applied_revision {
        query = query.col_expr(
            remote_storage_target::Column::AppliedRevision,
            Expr::value(applied_revision),
        );
    }
    let result = query.exec(db).await.map_err(AsterError::from)?;
    if result.rows_affected == 0 {
        return find_by_id(db, target_id).await;
    }
    find_by_id(db, target_id).await
}

pub async fn delete_by_binding_and_target_key<C: ConnectionTrait>(
    db: &C,
    master_binding_id: i64,
    target_key: &str,
) -> Result<()> {
    let model = find_by_binding_and_target_key(db, master_binding_id, target_key)
        .await?
        .ok_or_else(|| {
            AsterError::record_not_found(format!("remote_storage_target '{target_key}'"))
        })?;
    RemoteStorageTarget::delete_by_id(model.id)
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(())
}
