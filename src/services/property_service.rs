//! 服务模块：`property_service`。

use crate::db::repository::{file_repo, folder_repo, property_repo};
use crate::entities::entity_property;
use crate::errors::{AsterError, Result};
use crate::runtime::SharedRuntimeState;
use crate::services::files::{file, folder};
use crate::types::EntityType;
use serde::Serialize;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

pub const SYSTEM_PROPERTY_NAMESPACE_PREFIX: &str = "system.";

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

pub fn is_system_namespace(namespace: &str) -> bool {
    namespace.starts_with(SYSTEM_PROPERTY_NAMESPACE_PREFIX)
}

pub fn is_dav_namespace(namespace: &str) -> bool {
    namespace == "DAV:"
}

pub fn is_protected_namespace(namespace: &str) -> bool {
    is_dav_namespace(namespace) || is_system_namespace(namespace)
}

fn ensure_user_namespace_mutable(namespace: &str) -> Result<()> {
    if is_dav_namespace(namespace) {
        return Err(AsterError::auth_forbidden("DAV: namespace is read-only"));
    }
    if is_system_namespace(namespace) {
        return Err(AsterError::auth_forbidden("system namespace is read-only"));
    }
    Ok(())
}

/// 验证实体归属并返回
async fn verify_ownership(
    state: &impl SharedRuntimeState,
    entity_type: EntityType,
    entity_id: i64,
    user_id: i64,
) -> Result<()> {
    match entity_type {
        EntityType::File => {
            let f = file_repo::find_by_id(state.writer_db(), entity_id).await?;
            file::ensure_personal_file_scope(&f)?;
            if f.owner_user_id != Some(user_id) {
                return Err(AsterError::auth_forbidden("not your file"));
            }
        }
        EntityType::Folder => {
            let f = folder_repo::find_by_id(state.writer_db(), entity_id).await?;
            folder::ensure_personal_folder_scope(&f)?;
            if f.owner_user_id != Some(user_id) {
                return Err(AsterError::auth_forbidden("not your folder"));
            }
        }
    }
    Ok(())
}

/// 列出实体的所有属性
pub async fn list(
    state: &impl SharedRuntimeState,
    entity_type: EntityType,
    entity_id: i64,
    user_id: i64,
) -> Result<Vec<EntityProperty>> {
    verify_ownership(state, entity_type, entity_id, user_id).await?;
    Ok(
        property_repo::find_by_entity(state.writer_db(), entity_type, entity_id)
            .await?
            .into_iter()
            .filter(|prop| !is_protected_namespace(&prop.namespace))
            .map(Into::into)
            .collect(),
    )
}

/// 设置（新增/更新）属性
pub async fn set(
    state: &impl SharedRuntimeState,
    entity_type: EntityType,
    entity_id: i64,
    user_id: i64,
    namespace: &str,
    name: &str,
    value: Option<&str>,
) -> Result<EntityProperty> {
    verify_ownership(state, entity_type, entity_id, user_id).await?;

    ensure_user_namespace_mutable(namespace)?;

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

    property_repo::upsert(
        state.writer_db(),
        entity_type,
        entity_id,
        namespace,
        name,
        value,
    )
    .await
    .map(Into::into)
}

/// 删除单个属性
pub async fn delete(
    state: &impl SharedRuntimeState,
    entity_type: EntityType,
    entity_id: i64,
    user_id: i64,
    namespace: &str,
    name: &str,
) -> Result<()> {
    verify_ownership(state, entity_type, entity_id, user_id).await?;

    ensure_user_namespace_mutable(namespace)?;

    property_repo::delete_prop(state.writer_db(), entity_type, entity_id, namespace, name).await?;
    tracing::debug!(
        entity_type = ?entity_type,
        entity_id,
        user_id,
        namespace,
        name,
        "deleted entity property"
    );
    Ok(())
}
