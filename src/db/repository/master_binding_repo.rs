//! 仓储模块：`master_binding_repo`。

use crate::errors::{AsterError, Result};
use aster_drive_model::entities::master_binding::{self, Entity as MasterBinding};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder, SqlErr,
    TryInsertResult,
};

/// Loads a master binding by id, returning `record_not_found` when absent.
pub async fn find_by_id<C: ConnectionTrait>(db: &C, id: i64) -> Result<master_binding::Model> {
    MasterBinding::find_by_id(id)
        .one(db)
        .await
        .map_err(AsterError::from)?
        .ok_or_else(|| AsterError::record_not_found(format!("master_binding #{id}")))
}

/// Resolves a binding by the access key a follower authenticates with.
/// Returns `None` when no binding carries the key.
pub async fn find_by_access_key<C: ConnectionTrait>(
    db: &C,
    access_key: &str,
) -> Result<Option<master_binding::Model>> {
    MasterBinding::find()
        .filter(master_binding::Column::AccessKey.eq(access_key))
        .one(db)
        .await
        .map_err(AsterError::from)
}

/// Resolves the binding owning an object-storage namespace. Returns `None`
/// when the namespace is unbound; the column carries a uniqueness constraint.
pub async fn find_by_storage_namespace<C: ConnectionTrait>(
    db: &C,
    storage_namespace: &str,
) -> Result<Option<master_binding::Model>> {
    MasterBinding::find()
        .filter(master_binding::Column::StorageNamespace.eq(storage_namespace))
        .one(db)
        .await
        .map_err(AsterError::from)
}

/// Lists every master binding, newest registrations first.
pub async fn find_all<C: ConnectionTrait>(db: &C) -> Result<Vec<master_binding::Model>> {
    MasterBinding::find()
        .order_by_desc(master_binding::Column::CreatedAt)
        .order_by_desc(master_binding::Column::Id)
        .all(db)
        .await
        .map_err(AsterError::from)
}

/// Inserts a new master binding; namespace uniqueness violations surface as
/// database errors.
pub async fn create<C: ConnectionTrait>(
    db: &C,
    model: master_binding::ActiveModel,
) -> Result<master_binding::Model> {
    model.insert(db).await.map_err(AsterError::from)
}

/// Inserts a binding without claiming an already-bound storage namespace:
/// on conflict the insert is skipped and `None` returned (handling both the
/// ignored-conflict path and a raw unique-violation race), otherwise the
/// inserted binding is reloaded.
pub async fn create_ignoring_storage_namespace_conflict<C: ConnectionTrait>(
    db: &C,
    model: master_binding::ActiveModel,
) -> Result<Option<master_binding::Model>> {
    match MasterBinding::insert(model)
        .on_conflict_do_nothing_on([master_binding::Column::StorageNamespace])
        .exec(db)
        .await
    {
        Ok(TryInsertResult::Inserted(result)) => {
            find_by_id(db, result.last_insert_id).await.map(Some)
        }
        Ok(TryInsertResult::Conflicted) => Ok(None),
        Ok(TryInsertResult::Empty) => Err(AsterError::internal_error(
            "master binding insert produced empty result",
        )),
        Err(err) if matches!(err.sql_err(), Some(SqlErr::UniqueConstraintViolation(_))) => Ok(None),
        Err(err) => Err(AsterError::from(err)),
    }
}

/// Persists the given binding model's changed fields.
pub async fn update<C: ConnectionTrait>(
    db: &C,
    model: master_binding::ActiveModel,
) -> Result<master_binding::Model> {
    model.update(db).await.map_err(AsterError::from)
}

/// Advances a binding's `applied_revision` to the desired revision only when
/// the row still desires exactly that revision and has not applied it yet, so
/// stale acknowledgements cannot move the counter past `desired_revision`.
pub async fn mark_applied_revision<C: ConnectionTrait>(
    db: &C,
    id: i64,
    desired_revision: i64,
) -> Result<()> {
    MasterBinding::update_many()
        .col_expr(
            master_binding::Column::AppliedRevision,
            sea_orm::sea_query::Expr::value(desired_revision),
        )
        .filter(master_binding::Column::Id.eq(id))
        .filter(master_binding::Column::DesiredRevision.eq(desired_revision))
        .filter(master_binding::Column::AppliedRevision.lt(desired_revision))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(())
}
