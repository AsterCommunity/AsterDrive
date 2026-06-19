//! Repository helpers for `storage_connector_application_configs`.

use chrono::Utc;
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, Set, sea_query::OnConflict};

use crate::entities::storage_connector_application_config::{
    self, Entity as StorageConnectorApplicationConfig,
};
use crate::errors::{AsterError, Result};
use crate::types::StorageCredentialProvider;

pub async fn find_by_policy_provider<C: ConnectionTrait>(
    db: &C,
    policy_id: i64,
    provider: StorageCredentialProvider,
) -> Result<Option<storage_connector_application_config::Model>> {
    StorageConnectorApplicationConfig::find()
        .filter(storage_connector_application_config::Column::PolicyId.eq(policy_id))
        .filter(storage_connector_application_config::Column::Provider.eq(provider))
        .one(db)
        .await
        .map_err(AsterError::from)
}

pub async fn upsert_by_policy_provider<C: ConnectionTrait>(
    db: &C,
    mut model: storage_connector_application_config::ActiveModel,
    now: chrono::DateTime<Utc>,
) -> Result<storage_connector_application_config::Model> {
    let policy_id = active_i64(&model.policy_id, "policy_id")?;
    let provider = active_provider(&model.provider)?;

    model.created_at = Set(now);
    model.updated_at = Set(now);

    StorageConnectorApplicationConfig::insert(model)
        .on_conflict(
            OnConflict::columns([
                storage_connector_application_config::Column::PolicyId,
                storage_connector_application_config::Column::Provider,
            ])
            .update_columns([
                storage_connector_application_config::Column::TenantId,
                storage_connector_application_config::Column::Scopes,
                storage_connector_application_config::Column::ClientId,
                storage_connector_application_config::Column::ClientSecretCiphertext,
                storage_connector_application_config::Column::Metadata,
                storage_connector_application_config::Column::UpdatedAt,
            ])
            .to_owned(),
        )
        .exec(db)
        .await
        .map_err(AsterError::from)?;

    find_by_policy_provider(db, policy_id, provider)
        .await?
        .ok_or_else(|| {
            AsterError::record_not_found("storage connector application config after upsert")
        })
}

fn active_i64(value: &sea_orm::ActiveValue<i64>, field: &str) -> Result<i64> {
    match value {
        sea_orm::ActiveValue::Set(value) | sea_orm::ActiveValue::Unchanged(value) => Ok(*value),
        sea_orm::ActiveValue::NotSet => Err(AsterError::internal_error(format!(
            "storage connector application config active model missing {field}"
        ))),
    }
}

fn active_provider(
    value: &sea_orm::ActiveValue<StorageCredentialProvider>,
) -> Result<StorageCredentialProvider> {
    match value {
        sea_orm::ActiveValue::Set(value) | sea_orm::ActiveValue::Unchanged(value) => Ok(*value),
        sea_orm::ActiveValue::NotSet => Err(AsterError::internal_error(
            "storage connector application config active model missing provider".to_string(),
        )),
    }
}
