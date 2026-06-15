//! Repository helpers for `storage_policy_credentials`.

use chrono::Utc;
use sea_orm::{ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, Set};

use crate::entities::storage_policy_credential::{self, Entity as StoragePolicyCredential};
use crate::errors::{AsterError, Result};
use crate::types::{StorageCredentialKind, StorageCredentialProvider};

pub async fn find_all<C: ConnectionTrait>(db: &C) -> Result<Vec<storage_policy_credential::Model>> {
    StoragePolicyCredential::find()
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_by_policy_provider_kind<C: ConnectionTrait>(
    db: &C,
    policy_id: i64,
    provider: StorageCredentialProvider,
    credential_kind: StorageCredentialKind,
) -> Result<Option<storage_policy_credential::Model>> {
    StoragePolicyCredential::find()
        .filter(storage_policy_credential::Column::PolicyId.eq(policy_id))
        .filter(storage_policy_credential::Column::Provider.eq(provider))
        .filter(storage_policy_credential::Column::CredentialKind.eq(credential_kind))
        .one(db)
        .await
        .map_err(AsterError::from)
}

pub async fn list_by_policy<C: ConnectionTrait>(
    db: &C,
    policy_id: i64,
) -> Result<Vec<storage_policy_credential::Model>> {
    StoragePolicyCredential::find()
        .filter(storage_policy_credential::Column::PolicyId.eq(policy_id))
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn upsert_by_policy_provider_kind<C: ConnectionTrait>(
    db: &C,
    mut model: storage_policy_credential::ActiveModel,
    now: chrono::DateTime<Utc>,
) -> Result<storage_policy_credential::Model> {
    let policy_id = active_i64(&model.policy_id, "policy_id")?;
    let provider = active_provider(&model.provider)?;
    let credential_kind = active_credential_kind(&model.credential_kind)?;

    if let Some(existing) =
        find_by_policy_provider_kind(db, policy_id, provider, credential_kind).await?
    {
        model.id = Set(existing.id);
        model.created_at = Set(existing.created_at);
        model.updated_at = Set(now);
        model.update(db).await.map_err(AsterError::from)
    } else {
        model.created_at = Set(now);
        model.updated_at = Set(now);
        model.insert(db).await.map_err(AsterError::from)
    }
}

fn active_i64(value: &sea_orm::ActiveValue<i64>, field: &str) -> Result<i64> {
    match value {
        sea_orm::ActiveValue::Set(value) | sea_orm::ActiveValue::Unchanged(value) => Ok(*value),
        sea_orm::ActiveValue::NotSet => Err(AsterError::internal_error(format!(
            "storage credential active model missing {field}"
        ))),
    }
}

fn active_provider(
    value: &sea_orm::ActiveValue<StorageCredentialProvider>,
) -> Result<StorageCredentialProvider> {
    match value {
        sea_orm::ActiveValue::Set(value) | sea_orm::ActiveValue::Unchanged(value) => Ok(*value),
        sea_orm::ActiveValue::NotSet => Err(AsterError::internal_error(format!(
            "storage credential active model missing provider"
        ))),
    }
}

fn active_credential_kind(
    value: &sea_orm::ActiveValue<StorageCredentialKind>,
) -> Result<StorageCredentialKind> {
    match value {
        sea_orm::ActiveValue::Set(value) | sea_orm::ActiveValue::Unchanged(value) => Ok(*value),
        sea_orm::ActiveValue::NotSet => Err(AsterError::internal_error(format!(
            "storage credential active model missing credential_kind"
        ))),
    }
}
