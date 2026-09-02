//! Repository for encrypted remote-target connector credentials.

use crate::errors::{AsterError, Result};
use aster_drive_model::entities::remote_storage_target_credential::{
    self, Entity as RemoteStorageTargetCredential,
};
use sea_orm::{ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, Set};

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

pub async fn upsert<C: ConnectionTrait>(
    db: &C,
    target_id: i64,
    connector_id: String,
    schema_version: i32,
    ciphertext: String,
) -> Result<remote_storage_target_credential::Model> {
    if let Some(existing) = find_by_target(db, target_id).await? {
        let revision = existing.revision.checked_add(1).ok_or_else(|| {
            AsterError::internal_error("remote target credential revision overflow")
        })?;
        let mut active: remote_storage_target_credential::ActiveModel = existing.into();
        active.connector_id = Set(connector_id);
        active.schema_version = Set(schema_version);
        active.revision = Set(revision);
        active.ciphertext = Set(ciphertext);
        active.updated_at = Set(chrono::Utc::now());
        return active.update(db).await.map_err(AsterError::from);
    }

    remote_storage_target_credential::ActiveModel {
        target_id: Set(target_id),
        connector_id: Set(connector_id),
        schema_version: Set(schema_version),
        revision: Set(1),
        ciphertext: Set(ciphertext),
        created_at: Set(chrono::Utc::now()),
        updated_at: Set(chrono::Utc::now()),
        ..Default::default()
    }
    .insert(db)
    .await
    .map_err(AsterError::from)
}

pub async fn delete_by_target<C: ConnectionTrait>(db: &C, target_id: i64) -> Result<()> {
    RemoteStorageTargetCredential::delete_many()
        .filter(remote_storage_target_credential::Column::TargetId.eq(target_id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(())
}
