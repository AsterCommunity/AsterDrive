//! SeaORM 实体定义：`entity_property`。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::types::EntityType;

/// Namespaced property attached to a file or folder, backing WebDAV `PROPFIND`/`PROPPATCH`.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[cfg_attr(all(debug_assertions, feature = "openapi"), schema(as = EntityProperty))]
#[sea_orm(table_name = "entity_properties")]
pub struct Model {
    /// Auto-incrementing surrogate key.
    #[sea_orm(primary_key)]
    pub id: i64,
    /// Resource kind the property is attached to.
    pub entity_type: EntityType,
    /// Primary key of the resource the property is attached to (`files.id` or `folders.id`).
    pub entity_id: i64,
    /// Property namespace (XML namespace URI or `system.`-prefixed internal namespace);
    /// case-sensitive, empty string when the property has no namespace.
    pub namespace: String,
    /// Local property name, unique per (entity, namespace); case-sensitive.
    pub name: String,
    /// Property value; `None` models a valueless property rather than an empty string.
    pub value: Option<String>,
}

/// No database-level relations are declared for this entity.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
