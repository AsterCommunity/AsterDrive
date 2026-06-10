//! 仓储模块：`passkey_repo`。

use chrono::Utc;
use sea_orm::{
    ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, QuerySelect,
    sea_query::Expr,
};

use crate::entities::passkey::{self, Entity as Passkey};
use crate::errors::{AsterError, Result};
use crate::types::StoredPasskeyCredential;

pub async fn list_for_user(db: &DatabaseConnection, user_id: i64) -> Result<Vec<passkey::Model>> {
    Passkey::find()
        .filter(passkey::Column::UserId.eq(user_id))
        .order_by_desc(passkey::Column::LastUsedAt)
        .order_by_desc(passkey::Column::CreatedAt)
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_by_id_for_user(
    db: &DatabaseConnection,
    id: i64,
    user_id: i64,
) -> Result<Option<passkey::Model>> {
    Passkey::find()
        .filter(passkey::Column::Id.eq(id))
        .filter(passkey::Column::UserId.eq(user_id))
        .one(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_by_credential_id(
    db: &DatabaseConnection,
    credential_id: &str,
) -> Result<Option<passkey::Model>> {
    Passkey::find()
        .filter(passkey::Column::CredentialId.eq(credential_id))
        .one(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_by_user_handle_and_credential_id(
    db: &DatabaseConnection,
    user_handle: &str,
    credential_id: &str,
) -> Result<Option<passkey::Model>> {
    Passkey::find()
        .filter(passkey::Column::UserHandle.eq(user_handle))
        .filter(passkey::Column::CredentialId.eq(credential_id))
        .one(db)
        .await
        .map_err(AsterError::from)
}

pub async fn user_handle_exists(db: &DatabaseConnection, user_handle: &str) -> Result<bool> {
    let found = Passkey::find()
        .select_only()
        .column(passkey::Column::Id)
        .filter(passkey::Column::UserHandle.eq(user_handle))
        .into_tuple::<i64>()
        .one(db)
        .await
        .map_err(AsterError::from)?;
    Ok(found.is_some())
}

pub async fn update_name_for_user(
    db: &DatabaseConnection,
    id: i64,
    user_id: i64,
    name: &str,
) -> Result<bool> {
    let result = Passkey::update_many()
        .col_expr(passkey::Column::Name, Expr::value(name.to_string()))
        .col_expr(passkey::Column::UpdatedAt, Expr::value(Utc::now()))
        .filter(passkey::Column::Id.eq(id))
        .filter(passkey::Column::UserId.eq(user_id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}

pub async fn update_credential_after_auth(
    db: &DatabaseConnection,
    id: i64,
    credential: StoredPasskeyCredential,
    backup_eligible: bool,
    backed_up: bool,
    sign_count: i64,
    last_used_at: chrono::DateTime<Utc>,
) -> Result<bool> {
    if sign_count < 0 {
        return Err(AsterError::validation_error(
            "passkey sign count cannot be negative",
        ));
    }

    let result = Passkey::update_many()
        .col_expr(passkey::Column::Credential, Expr::value(credential))
        .col_expr(
            passkey::Column::BackupEligible,
            Expr::value(backup_eligible),
        )
        .col_expr(passkey::Column::BackedUp, Expr::value(backed_up))
        .col_expr(passkey::Column::SignCount, Expr::value(sign_count))
        .col_expr(passkey::Column::LastUsedAt, Expr::value(Some(last_used_at)))
        .col_expr(passkey::Column::UpdatedAt, Expr::value(last_used_at))
        .filter(passkey::Column::Id.eq(id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}

pub async fn touch_last_used(
    db: &DatabaseConnection,
    id: i64,
    last_used_at: chrono::DateTime<Utc>,
) -> Result<bool> {
    let result = Passkey::update_many()
        .col_expr(passkey::Column::LastUsedAt, Expr::value(Some(last_used_at)))
        .col_expr(passkey::Column::UpdatedAt, Expr::value(last_used_at))
        .filter(passkey::Column::Id.eq(id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}

pub async fn delete_for_user(db: &DatabaseConnection, id: i64, user_id: i64) -> Result<bool> {
    let result = Passkey::delete_many()
        .filter(passkey::Column::Id.eq(id))
        .filter(passkey::Column::UserId.eq(user_id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}
