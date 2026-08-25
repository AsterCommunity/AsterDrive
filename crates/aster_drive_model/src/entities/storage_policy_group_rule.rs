//! SeaORM entity: placement rule owned by a storage policy profile.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    schema(as = StoragePolicyGroupRule)
)]
#[sea_orm(table_name = "storage_policy_group_rules")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub group_id: i64,
    pub name: String,
    pub description: String,
    pub priority: i32,
    pub is_enabled: bool,
    pub matcher: String,
    pub selection_mode: String,
    pub unavailable_behavior: String,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: DateTimeUtc,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::storage_policy_group::Entity",
        from = "Column::GroupId",
        to = "super::storage_policy_group::Column::Id"
    )]
    StoragePolicyGroup,
    #[sea_orm(has_many = "super::storage_policy_group_rule_target::Entity")]
    Targets,
}

impl Related<super::storage_policy_group::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::StoragePolicyGroup.def()
    }
}

impl Related<super::storage_policy_group_rule_target::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Targets.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
