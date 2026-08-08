//! Repository for encrypted connector-owned storage-policy credentials.

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DbBackend, EntityTrait, QueryFilter,
    QuerySelect, Set, sea_query::Expr,
};

use crate::errors::{AsterError, Result};
use aster_drive_model::entities::storage_policy_connector_credential::{
    self, Entity as StoragePolicyConnectorCredential,
};

pub async fn find_all<C: ConnectionTrait>(
    db: &C,
) -> Result<Vec<storage_policy_connector_credential::Model>> {
    StoragePolicyConnectorCredential::find()
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_by_policy<C: ConnectionTrait>(
    db: &C,
    policy_id: i64,
) -> Result<Option<storage_policy_connector_credential::Model>> {
    StoragePolicyConnectorCredential::find()
        .filter(storage_policy_connector_credential::Column::PolicyId.eq(policy_id))
        .one(db)
        .await
        .map_err(AsterError::from)
}

pub async fn lock_by_policy<C: ConnectionTrait>(
    db: &C,
    policy_id: i64,
) -> Result<Option<storage_policy_connector_credential::Model>> {
    let query = StoragePolicyConnectorCredential::find()
        .filter(storage_policy_connector_credential::Column::PolicyId.eq(policy_id));
    match db.get_database_backend() {
        DbBackend::Postgres | DbBackend::MySql => query.lock_exclusive().one(db).await,
        DbBackend::Sqlite => query.one(db).await,
        _ => query.one(db).await,
    }
    .map_err(AsterError::from)
}

pub async fn upsert<C: ConnectionTrait>(
    db: &C,
    policy_id: i64,
    connector_id: String,
    schema_version: i32,
    ciphertext: String,
) -> Result<storage_policy_connector_credential::Model> {
    let now = Utc::now();
    if let Some(existing) = find_by_policy(db, policy_id).await? {
        let next_revision = existing.revision.checked_add(1).ok_or_else(|| {
            AsterError::database_operation("storage connector credential revision overflow")
        })?;
        let mut active: storage_policy_connector_credential::ActiveModel = existing.into();
        active.connector_id = Set(connector_id);
        active.schema_version = Set(schema_version);
        active.revision = Set(next_revision);
        active.ciphertext = Set(ciphertext);
        active.updated_at = Set(now);
        return active.update(db).await.map_err(AsterError::from);
    }

    storage_policy_connector_credential::ActiveModel {
        id: Default::default(),
        policy_id: Set(policy_id),
        connector_id: Set(connector_id),
        schema_version: Set(schema_version),
        revision: Set(1),
        ciphertext: Set(ciphertext),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(db)
    .await
    .map_err(AsterError::from)
}

/// Replace an encrypted connector payload only when the caller still owns the
/// revision it decoded. Refreshable connectors use this to prevent token
/// rotation from being overwritten by a concurrent primary instance.
pub async fn update_if_revision<C: ConnectionTrait>(
    db: &C,
    policy_id: i64,
    connector_id: &str,
    schema_version: i32,
    expected_revision: i64,
    ciphertext: String,
) -> Result<bool> {
    let next_revision = expected_revision.checked_add(1).ok_or_else(|| {
        AsterError::database_operation("storage connector credential revision overflow")
    })?;
    let result = StoragePolicyConnectorCredential::update_many()
        .col_expr(
            storage_policy_connector_credential::Column::Ciphertext,
            Expr::value(ciphertext),
        )
        .col_expr(
            storage_policy_connector_credential::Column::Revision,
            Expr::value(next_revision),
        )
        .col_expr(
            storage_policy_connector_credential::Column::UpdatedAt,
            Expr::value(Utc::now()),
        )
        .filter(storage_policy_connector_credential::Column::PolicyId.eq(policy_id))
        .filter(storage_policy_connector_credential::Column::ConnectorId.eq(connector_id))
        .filter(storage_policy_connector_credential::Column::SchemaVersion.eq(schema_version))
        .filter(storage_policy_connector_credential::Column::Revision.eq(expected_revision))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}

pub async fn delete_by_policy<C: ConnectionTrait>(db: &C, policy_id: i64) -> Result<()> {
    StoragePolicyConnectorCredential::delete_many()
        .filter(storage_policy_connector_credential::Column::PolicyId.eq(policy_id))
        .exec(db)
        .await
        .map(|_| ())
        .map_err(AsterError::from)
}
