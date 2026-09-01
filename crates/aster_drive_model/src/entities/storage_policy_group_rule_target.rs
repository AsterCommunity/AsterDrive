//! SeaORM entity: target owned by a storage placement rule.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    schema(as = StoragePolicyGroupRuleTarget)
)]
#[sea_orm(table_name = "storage_policy_group_rule_targets")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub rule_id: i64,
    pub policy_id: i64,
    pub weight: i32,
    pub is_enabled: bool,
    pub accepting_new_writes: bool,
    pub stable_order: i32,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: DateTimeUtc,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::storage_policy_group_rule::Entity",
        from = "Column::RuleId",
        to = "super::storage_policy_group_rule::Column::Id"
    )]
    Rule,
    #[sea_orm(
        belongs_to = "super::storage_policy::Entity",
        from = "Column::PolicyId",
        to = "super::storage_policy::Column::Id"
    )]
    StoragePolicy,
}

impl Related<super::storage_policy_group_rule::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Rule.def()
    }
}

impl Related<super::storage_policy::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::StoragePolicy.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
