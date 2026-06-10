//! 仓储模块：`external_auth_provider_repo`。

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder,
    sea_query::Expr,
};

use crate::db::repository::pagination_repo::fetch_offset_page;
use crate::entities::external_auth_provider::{self, Entity as ExternalAuthProvider};
use crate::errors::{AsterError, Result};
use crate::types::ExternalAuthProviderKind;

pub async fn find_all(db: &DatabaseConnection) -> Result<Vec<external_auth_provider::Model>> {
    ExternalAuthProvider::find()
        .order_by_asc(external_auth_provider::Column::DisplayName)
        .order_by_asc(external_auth_provider::Column::Id)
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_paginated(
    db: &DatabaseConnection,
    limit: u64,
    offset: u64,
    supported_kinds: impl IntoIterator<Item = ExternalAuthProviderKind>,
) -> Result<(Vec<external_auth_provider::Model>, u64)> {
    fetch_offset_page(
        db,
        ExternalAuthProvider::find()
            .filter(external_auth_provider::Column::ProviderKind.is_in(supported_kinds))
            .order_by_asc(external_auth_provider::Column::DisplayName)
            .order_by_asc(external_auth_provider::Column::Id),
        limit,
        offset,
    )
    .await
}

pub async fn find_all_by_kind(
    db: &DatabaseConnection,
    kind: ExternalAuthProviderKind,
) -> Result<Vec<external_auth_provider::Model>> {
    ExternalAuthProvider::find()
        .filter(external_auth_provider::Column::ProviderKind.eq(kind))
        .order_by_asc(external_auth_provider::Column::DisplayName)
        .order_by_asc(external_auth_provider::Column::Id)
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_enabled(db: &DatabaseConnection) -> Result<Vec<external_auth_provider::Model>> {
    ExternalAuthProvider::find()
        .filter(external_auth_provider::Column::Enabled.eq(true))
        .order_by_asc(external_auth_provider::Column::DisplayName)
        .order_by_asc(external_auth_provider::Column::Id)
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_enabled_by_kind(
    db: &DatabaseConnection,
    kind: ExternalAuthProviderKind,
) -> Result<Vec<external_auth_provider::Model>> {
    ExternalAuthProvider::find()
        .filter(external_auth_provider::Column::Enabled.eq(true))
        .filter(external_auth_provider::Column::ProviderKind.eq(kind))
        .order_by_asc(external_auth_provider::Column::DisplayName)
        .order_by_asc(external_auth_provider::Column::Id)
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_by_id(db: &DatabaseConnection, id: i64) -> Result<external_auth_provider::Model> {
    ExternalAuthProvider::find_by_id(id)
        .one(db)
        .await
        .map_err(AsterError::from)?
        .ok_or_else(|| AsterError::record_not_found(format!("external auth provider #{id}")))
}

pub async fn find_by_kind_key(
    db: &DatabaseConnection,
    kind: ExternalAuthProviderKind,
    key: &str,
) -> Result<Option<external_auth_provider::Model>> {
    ExternalAuthProvider::find()
        .filter(external_auth_provider::Column::ProviderKind.eq(kind))
        .filter(external_auth_provider::Column::Key.eq(key))
        .one(db)
        .await
        .map_err(AsterError::from)
}

pub async fn create(
    db: &DatabaseConnection,
    model: external_auth_provider::ActiveModel,
) -> Result<external_auth_provider::Model> {
    model.insert(db).await.map_err(AsterError::from)
}

pub async fn update(
    db: &DatabaseConnection,
    model: external_auth_provider::ActiveModel,
) -> Result<external_auth_provider::Model> {
    model.update(db).await.map_err(AsterError::from)
}

pub async fn delete(db: &DatabaseConnection, id: i64) -> Result<()> {
    let result = ExternalAuthProvider::delete_by_id(id)
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    if result.rows_affected == 0 {
        return Err(AsterError::record_not_found(format!(
            "external auth provider #{id}"
        )));
    }
    Ok(())
}

pub async fn touch_updated_at(
    db: &DatabaseConnection,
    id: i64,
    updated_at: chrono::DateTime<Utc>,
) -> Result<bool> {
    let result = ExternalAuthProvider::update_many()
        .col_expr(
            external_auth_provider::Column::UpdatedAt,
            Expr::value(updated_at),
        )
        .filter(external_auth_provider::Column::Id.eq(id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}
