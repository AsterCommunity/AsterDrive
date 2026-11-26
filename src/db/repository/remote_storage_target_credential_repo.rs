//! Repository for encrypted remote-target connector credentials.

use crate::errors::{AsterError, Result};
use aster_drive_model::entities::remote_storage_target_credential::{
    self, Entity as RemoteStorageTargetCredential,
};
use sea_orm::{
    ColumnTrait, ConnectionTrait, EntityTrait, ExprTrait, QueryFilter, Set,
    sea_query::{Expr, OnConflict},
};

/// Loads the encrypted connector credential of one remote storage target.
/// Returns `None` when the target has no credential row.
pub async fn find_by_target<C: ConnectionTrait>(
    db: &C,
    target_id: i64,
) -> Result<Option<remote_storage_target_credential::Model>> {
    RemoteStorageTargetCredential::find()
        .filter(remote_storage_target_credential::Column::TargetId.eq(target_id))
        .one(db)
        .await
        .map_err(AsterError::from)
}

/// Writes the target's credential in one statement: inserts with revision 1,
/// or on conflict replaces connector id, schema version and ciphertext while
/// incrementing the revision atomically. Returns the reloaded row; errors if
/// it cannot be reloaded afterwards.
pub async fn upsert<C: ConnectionTrait>(
    db: &C,
    target_id: i64,
    connector_id: String,
    schema_version: i32,
    ciphertext: String,
) -> Result<remote_storage_target_credential::Model> {
    let now = chrono::Utc::now();
    let active = remote_storage_target_credential::ActiveModel {
        target_id: Set(target_id),
        connector_id: Set(connector_id),
        schema_version: Set(schema_version),
        revision: Set(1),
        ciphertext: Set(ciphertext),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    RemoteStorageTargetCredential::insert(active)
        .on_conflict(
            OnConflict::column(remote_storage_target_credential::Column::TargetId)
                .update_columns([
                    remote_storage_target_credential::Column::ConnectorId,
                    remote_storage_target_credential::Column::SchemaVersion,
                    remote_storage_target_credential::Column::Ciphertext,
                    remote_storage_target_credential::Column::UpdatedAt,
                ])
                .value(
                    remote_storage_target_credential::Column::Revision,
                    Expr::col(remote_storage_target_credential::Column::Revision).add(1),
                )
                .to_owned(),
        )
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    find_by_target(db, target_id).await?.ok_or_else(|| {
        AsterError::database_operation("remote target credential upsert returned no row")
    })
}

/// Removes the target's credential, if any; succeeds even when absent.
pub async fn delete_by_target<C: ConnectionTrait>(db: &C, target_id: i64) -> Result<()> {
    RemoteStorageTargetCredential::delete_many()
        .filter(remote_storage_target_credential::Column::TargetId.eq(target_id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(())
}
