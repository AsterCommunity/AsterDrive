//! 仓储模块：`master_binding_repo`。

use crate::errors::{AsterError, Result};
use aster_drive_model::entities::master_binding::{self, Entity as MasterBinding};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder, SqlErr,
    TryInsertResult,
};

pub async fn find_by_id<C: ConnectionTrait>(db: &C, id: i64) -> Result<master_binding::Model> {
    MasterBinding::find_by_id(id)
        .one(db)
        .await
        .map_err(AsterError::from)?
        .ok_or_else(|| AsterError::record_not_found(format!("master_binding #{id}")))
}

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

pub async fn find_all<C: ConnectionTrait>(db: &C) -> Result<Vec<master_binding::Model>> {
    MasterBinding::find()
        .order_by_desc(master_binding::Column::CreatedAt)
        .order_by_desc(master_binding::Column::Id)
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn create<C: ConnectionTrait>(
    db: &C,
    model: master_binding::ActiveModel,
) -> Result<master_binding::Model> {
    model.insert(db).await.map_err(AsterError::from)
}

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

pub async fn update<C: ConnectionTrait>(
    db: &C,
    model: master_binding::ActiveModel,
) -> Result<master_binding::Model> {
    model.update(db).await.map_err(AsterError::from)
}
