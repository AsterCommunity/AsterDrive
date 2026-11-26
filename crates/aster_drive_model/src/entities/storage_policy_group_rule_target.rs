//! SeaORM entity: target owned by a storage placement rule.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

/// Target policy assigned to a placement rule, with its weight and write
/// acceptance state. (rule_id, policy_id) is unique.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    schema(as = StoragePolicyGroupRuleTarget)
)]
#[sea_orm(table_name = "storage_policy_group_rule_targets")]
pub struct Model {
    /// Primary key.
    #[sea_orm(primary_key)]
    pub id: i64,
    /// Owning placement rule.
    pub rule_id: i64,
    /// Storage policy selected by this target.
    pub policy_id: i64,
    /// Relative weight used by the `weighted_random` selection mode; ignored
    /// by `first_available`.
    pub weight: i32,
    /// Whether this target currently accepts new uploads; draining targets
    /// stay readable for existing data but are skipped for placement.
    pub accepting_new_writes: bool,
    /// Deterministic preference position among the rule's targets; the
    /// `first_available` selection mode picks the lowest value first.
    pub stable_order: i32,
    /// Row creation timestamp (UTC).
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: DateTimeUtc,
    /// Row last-update timestamp (UTC).
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: DateTimeUtc,
}

/// SeaORM relation definitions for this entity.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// The placement rule owning this target (FK `rule_id`).
    #[sea_orm(
        belongs_to = "super::storage_policy_group_rule::Entity",
        from = "Column::RuleId",
        to = "super::storage_policy_group_rule::Column::Id"
    )]
    Rule,
    /// The storage policy selected by this target (FK `policy_id`).
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
