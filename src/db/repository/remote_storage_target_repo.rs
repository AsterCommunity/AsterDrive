//! 仓储模块：`remote_storage_target_repo`。

use crate::api::api_error_code::ApiErrorCode;
use crate::entities::remote_storage_target::{self, Entity as RemoteStorageTarget};
use crate::errors::{AsterError, Result, validation_error_with_code};
use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, DatabaseTransaction,
    EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, Set,
};

pub async fn find_by_id(db: &DatabaseConnection, id: i64) -> Result<remote_storage_target::Model> {
    find_by_id_in_connection(db, id).await
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

pub async fn find_all_by_binding(
    db: &DatabaseConnection,
    master_binding_id: i64,
) -> Result<Vec<remote_storage_target::Model>> {
    RemoteStorageTarget::find()
        .filter(remote_storage_target::Column::MasterBindingId.eq(master_binding_id))
        .order_by_desc(remote_storage_target::Column::IsDefault)
        .order_by_desc(remote_storage_target::Column::CreatedAt)
        .order_by_desc(remote_storage_target::Column::Id)
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_default_by_binding(
    db: &DatabaseConnection,
    master_binding_id: i64,
) -> Result<Option<remote_storage_target::Model>> {
    RemoteStorageTarget::find()
        .filter(remote_storage_target::Column::MasterBindingId.eq(master_binding_id))
        .filter(remote_storage_target::Column::IsDefault.eq(true))
        .order_by_desc(remote_storage_target::Column::UpdatedAt)
        .order_by_desc(remote_storage_target::Column::Id)
        .one(db)
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

/// Must be called inside a transaction, for example through `with_transaction`.
///
/// This loads and validates the target via `find_by_id`, clears existing
/// defaults with `update_many`, then marks the target with `update`; without
/// transaction semantics, concurrent readers can observe a temporary no-default
/// state and concurrent writers can create conflicting defaults.
pub async fn set_only_default_for_binding(
    db: &DatabaseTransaction,
    master_binding_id: i64,
    profile_id: i64,
) -> Result<()> {
    let existing = find_by_id_in_connection(db, profile_id).await?;
    if existing.master_binding_id != master_binding_id {
        return Err(validation_error_with_code(
            ApiErrorCode::ManagedIngressBindingMismatch,
            format!(
                "managed remote storage target #{profile_id} does not belong to master_binding #{master_binding_id}"
            ),
        ));
    }

    RemoteStorageTarget::update_many()
        .filter(remote_storage_target::Column::MasterBindingId.eq(master_binding_id))
        .col_expr(remote_storage_target::Column::IsDefault, Expr::value(false))
        .exec(db)
        .await
        .map_err(AsterError::from)?;

    let mut active: remote_storage_target::ActiveModel = existing.into();
    active.is_default = Set(true);
    active.updated_at = Set(chrono::Utc::now());
    update(db, active).await?;
    Ok(())
}

async fn find_by_id_in_connection<C: ConnectionTrait>(
    db: &C,
    id: i64,
) -> Result<remote_storage_target::Model> {
    RemoteStorageTarget::find_by_id(id)
        .one(db)
        .await
        .map_err(AsterError::from)?
        .ok_or_else(|| AsterError::record_not_found(format!("remote_storage_target #{id}")))
}
