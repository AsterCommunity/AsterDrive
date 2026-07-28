//! SeaORM 实体定义：`entity_property`。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::types::EntityType;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[cfg_attr(all(debug_assertions, feature = "openapi"), schema(as = EntityProperty))]
#[sea_orm(table_name = "entity_properties")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub entity_type: EntityType,
    pub entity_id: i64,
    pub namespace: String,
    pub name: String,
    pub value: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
