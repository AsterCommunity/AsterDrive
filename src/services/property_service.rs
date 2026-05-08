//! 服务模块：`property_service`。

use crate::db::repository::{file_repo, folder_repo, property_repo};
use crate::entities::entity_property;
use crate::errors::{AsterError, Result};
use crate::runtime::PrimaryAppState;
use crate::services::{file_service, folder_service};
use crate::types::EntityType;
use serde::Serialize;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct EntityProperty {
    pub id: i64,
    pub entity_type: EntityType,
    pub entity_id: i64,
    pub namespace: String,
    pub name: String,
    pub value: Option<String>,
}

impl From<entity_property::Model> for EntityProperty {
    fn from(model: entity_property::Model) -> Self {
        Self {
            id: model.id,
            entity_type: model.entity_type,
            entity_id: model.entity_id,
            namespace: model.namespace,
            name: model.name,
            value: model.value,
        }
    }
}

/// 验证实体归属并返回
async fn verify_ownership(
    state: &PrimaryAppState,
    entity_type: EntityType,
    entity_id: i64,
    user_id: i64,
) -> Result<()> {
    match entity_type {
        EntityType::File => {
            let f = file_repo::find_by_id(&state.db, entity_id).await?;
            file_service::ensure_personal_file_scope(&f)?;
            if f.owner_user_id != Some(user_id) {
                return Err(AsterError::auth_forbidden("not your file"));
            }
        }
        EntityType::Folder => {
            let f = folder_repo::find_by_id(&state.db, entity_id).await?;
            folder_service::ensure_personal_folder_scope(&f)?;
            if f.owner_user_id != Some(user_id) {
                return Err(AsterError::auth_forbidden("not your folder"));
            }
        }
    }
    Ok(())
}

/// 列出实体的所有属性
pub async fn list(
    state: &PrimaryAppState,
    entity_type: EntityType,
    entity_id: i64,
    user_id: i64,
) -> Result<Vec<EntityProperty>> {
    verify_ownership(state, entity_type, entity_id, user_id).await?;
    Ok(
        property_repo::find_by_entity(&state.db, entity_type, entity_id)
            .await?
            .into_iter()
            .map(Into::into)
            .collect(),
    )
}

/// 设置（新增/更新）属性
pub async fn set(
    state: &PrimaryAppState,
    entity_type: EntityType,
    entity_id: i64,
    user_id: i64,
    namespace: &str,
    name: &str,
    value: Option<&str>,
) -> Result<EntityProperty> {
    verify_ownership(state, entity_type, entity_id, user_id).await?;

    if namespace == "DAV:" {
        return Err(AsterError::auth_forbidden("DAV: namespace is read-only"));
    }

    // 输入长度限制
    if namespace.len() > 256 {
        return Err(AsterError::validation_error("namespace too long (max 256)"));
    }
    if name.len() > 256 {
        return Err(AsterError::validation_error(
            "property name too long (max 256)",
        ));
    }
    if let Some(v) = value
        && v.len() > 65536
    {
        return Err(AsterError::validation_error(
            "property value too long (max 64KB)",
        ));
    }

    property_repo::upsert(&state.db, entity_type, entity_id, namespace, name, value)
        .await
        .map(Into::into)
}

/// 删除单个属性
pub async fn delete(
    state: &PrimaryAppState,
    entity_type: EntityType,
    entity_id: i64,
    user_id: i64,
    namespace: &str,
    name: &str,
) -> Result<()> {
    verify_ownership(state, entity_type, entity_id, user_id).await?;

    if namespace == "DAV:" {
        return Err(AsterError::auth_forbidden("DAV: namespace is read-only"));
    }

    property_repo::delete_prop(&state.db, entity_type, entity_id, namespace, name).await
}
