//! Repository helpers for `storage_policy_credentials`.

use chrono::Utc;
use sea_orm::{
    ActiveEnum, ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, Set,
    sea_query::Expr,
};

use crate::entities::storage_policy_credential::{self, Entity as StoragePolicyCredential};
use crate::errors::{AsterError, Result};
use crate::types::{StorageCredentialKind, StorageCredentialProvider, StorageCredentialStatus};

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

pub struct OAuthRefreshUpdate<'a> {
    pub policy_id: i64,
    pub provider: StorageCredentialProvider,
    pub credential_kind: StorageCredentialKind,
    pub expected_refresh_token_ciphertext: &'a str,
    pub access_token_ciphertext: String,
    pub refresh_token_ciphertext: Option<String>,
    pub expires_at: Option<chrono::DateTime<Utc>>,
    pub scopes: Option<String>,
    pub now: chrono::DateTime<Utc>,
}

pub async fn update_oauth_refresh_result_if_refresh_token_matches<C: ConnectionTrait>(
    db: &C,
    input: OAuthRefreshUpdate<'_>,
) -> Result<bool> {
    let mut update = StoragePolicyCredential::update_many()
        .col_expr(
            storage_policy_credential::Column::AccessTokenCiphertext,
            Expr::value(Some(input.access_token_ciphertext)),
        )
        .col_expr(
            storage_policy_credential::Column::RefreshTokenCiphertext,
            Expr::value(input.refresh_token_ciphertext),
        )
        .col_expr(
            storage_policy_credential::Column::ExpiresAt,
            Expr::value(input.expires_at),
        )
        .col_expr(
            storage_policy_credential::Column::LastRefreshedAt,
            Expr::value(Some(input.now)),
        )
        .col_expr(
            storage_policy_credential::Column::Status,
            Expr::value(StorageCredentialStatus::Authorized.to_value()),
        )
        .col_expr(
            storage_policy_credential::Column::StatusReason,
            Expr::value(Option::<String>::None),
        )
        .col_expr(
            storage_policy_credential::Column::UpdatedAt,
            Expr::value(input.now),
        )
        .filter(storage_policy_credential::Column::PolicyId.eq(input.policy_id))
        .filter(storage_policy_credential::Column::Provider.eq(input.provider))
        .filter(storage_policy_credential::Column::CredentialKind.eq(input.credential_kind))
        .filter(
            storage_policy_credential::Column::RefreshTokenCiphertext
                .eq(input.expected_refresh_token_ciphertext),
        );
    if let Some(scopes) = input.scopes {
        update = update.col_expr(
            storage_policy_credential::Column::Scopes,
            Expr::value(scopes),
        );
    }
    let result = update.exec(db).await.map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
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
