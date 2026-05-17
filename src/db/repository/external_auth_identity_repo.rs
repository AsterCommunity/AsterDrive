//! 仓储模块：`external_auth_identity_repo`。

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter,
    QueryOrder, sea_query::Expr,
};

use crate::entities::external_auth_identity::{self, Entity as ExternalAuthIdentity};
use crate::errors::{AsterError, Result};

pub struct CreateExternalAuthIdentityInput<'a> {
    pub user_id: i64,
    pub provider_id: i64,
    pub identity_namespace: &'a str,
    pub subject: &'a str,
    pub email_snapshot: Option<&'a str>,
    pub display_name_snapshot: Option<&'a str>,
    pub now: chrono::DateTime<Utc>,
}

pub async fn list_for_user<C: ConnectionTrait>(
    db: &C,
    user_id: i64,
) -> Result<Vec<external_auth_identity::Model>> {
    ExternalAuthIdentity::find()
        .filter(external_auth_identity::Column::UserId.eq(user_id))
        .order_by_desc(external_auth_identity::Column::LastLoginAt)
        .order_by_desc(external_auth_identity::Column::CreatedAt)
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_by_provider_subject<C: ConnectionTrait>(
    db: &C,
    provider_id: i64,
    subject: &str,
) -> Result<Option<external_auth_identity::Model>> {
    ExternalAuthIdentity::find()
        .filter(external_auth_identity::Column::ProviderId.eq(provider_id))
        .filter(external_auth_identity::Column::Subject.eq(subject))
        .one(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_by_identity_namespace_subject<C: ConnectionTrait>(
    db: &C,
    identity_namespace: &str,
    subject: &str,
) -> Result<Option<external_auth_identity::Model>> {
    ExternalAuthIdentity::find()
        .filter(external_auth_identity::Column::IdentityNamespace.eq(identity_namespace))
        .filter(external_auth_identity::Column::Subject.eq(subject))
        .one(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_by_id_for_user<C: ConnectionTrait>(
    db: &C,
    id: i64,
    user_id: i64,
) -> Result<Option<external_auth_identity::Model>> {
    ExternalAuthIdentity::find()
        .filter(external_auth_identity::Column::Id.eq(id))
        .filter(external_auth_identity::Column::UserId.eq(user_id))
        .one(db)
        .await
        .map_err(AsterError::from)
}

pub async fn create<C: ConnectionTrait>(
    db: &C,
    model: external_auth_identity::ActiveModel,
) -> Result<external_auth_identity::Model> {
    model.insert(db).await.map_err(AsterError::from)
}

pub async fn create_identity<C: ConnectionTrait>(
    db: &C,
    input: CreateExternalAuthIdentityInput<'_>,
) -> Result<external_auth_identity::Model> {
    create(
        db,
        external_auth_identity::ActiveModel {
            user_id: Set(input.user_id),
            provider_id: Set(input.provider_id),
            identity_namespace: Set(input.identity_namespace.to_string()),
            subject: Set(input.subject.to_string()),
            email_snapshot: Set(input.email_snapshot.map(str::to_string)),
            display_name_snapshot: Set(input.display_name_snapshot.map(str::to_string)),
            created_at: Set(input.now),
            updated_at: Set(input.now),
            last_login_at: Set(Some(input.now)),
            ..Default::default()
        },
    )
    .await
}

pub async fn touch_login<C: ConnectionTrait>(
    db: &C,
    id: i64,
    email_snapshot: Option<&str>,
    display_name_snapshot: Option<&str>,
    now: chrono::DateTime<Utc>,
) -> Result<bool> {
    let mut update = ExternalAuthIdentity::update_many();
    if let Some(email_snapshot) = email_snapshot {
        update = update.col_expr(
            external_auth_identity::Column::EmailSnapshot,
            Expr::value(email_snapshot.to_string()),
        );
    }
    if let Some(display_name_snapshot) = display_name_snapshot {
        update = update.col_expr(
            external_auth_identity::Column::DisplayNameSnapshot,
            Expr::value(display_name_snapshot.to_string()),
        );
    }
    let result = update
        .col_expr(
            external_auth_identity::Column::LastLoginAt,
            Expr::value(Some(now)),
        )
        .col_expr(external_auth_identity::Column::UpdatedAt, Expr::value(now))
        .filter(external_auth_identity::Column::Id.eq(id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}

pub async fn delete_for_user<C: ConnectionTrait>(db: &C, id: i64, user_id: i64) -> Result<bool> {
    let result = ExternalAuthIdentity::delete_many()
        .filter(external_auth_identity::Column::Id.eq(id))
        .filter(external_auth_identity::Column::UserId.eq(user_id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}
