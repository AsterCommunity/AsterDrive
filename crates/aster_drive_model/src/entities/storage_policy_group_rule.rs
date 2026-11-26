//! SeaORM entity: placement rule owned by a storage policy profile.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

/// Placement rule owned by a storage policy profile: matches an upload and
/// selects target policies according to priority order.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    schema(as = StoragePolicyGroupRule)
)]
#[sea_orm(table_name = "storage_policy_group_rules")]
pub struct Model {
    /// Primary key.
    #[sea_orm(primary_key)]
    pub id: i64,
    /// Owning storage policy profile.
    pub group_id: i64,
    /// Rule display name (unique within its profile per priority).
    pub name: String,
    /// Free-form rule description (`""` when unset).
    pub description: String,
    /// Evaluation order within the profile; lower value = higher priority
    /// (unique per profile).
    pub priority: i32,
    /// Whether the rule participates in placement evaluation.
    pub is_enabled: bool,
    /// Serialized `PlacementMatcher` (JSON) deciding which uploads the rule
    /// applies to, by size, extension and file category.
    pub matcher: String,
    /// How target policies are picked when the rule matches: `first_available`
    /// or `weighted_random`.
    pub selection_mode: String,
    /// What happens when no matched target can accept the upload:
    /// `next_rule` falls through to the next rule, `reject` fails the upload.
    pub unavailable_behavior: String,
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
    /// The storage policy profile owning this rule (FK `group_id`).
    #[sea_orm(
        belongs_to = "super::storage_policy_group::Entity",
        from = "Column::GroupId",
        to = "super::storage_policy_group::Column::Id"
    )]
    StoragePolicyGroup,
    /// Target policies assigned to this rule.
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
