//! SeaORM 实体定义：`storage_policy_group`。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

/// Storage policy profile row. A profile groups placement rules that route
/// uploads to concrete storage policies; users and teams reference one group.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    schema(as = StoragePolicyGroup)
)]
#[sea_orm(table_name = "storage_policy_groups")]
pub struct Model {
    /// Primary key.
    #[sea_orm(primary_key)]
    pub id: i64,
    /// Profile display name.
    pub name: String,
    /// Free-form profile description (`""` when unset).
    pub description: String,
    /// Whether the profile participates in placement routing.
    pub is_enabled: bool,
    /// Whether this profile is the instance-wide default assignment for
    /// users/teams without an explicit binding.
    pub is_default: bool,
    /// Versioned typed admission payload. Legacy groups keep an empty/default
    /// payload until the placement migration materializes their rules.
    pub admission_config: String,
    /// Serialized UploadExecutionPreference; parsed at snapshot reload.
    pub upload_execution_preference: String,
    /// Monotonic topology revision persisted with every profile mutation.
    pub routing_revision: i64,
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
    /// Placement rules owned by this profile.
    #[sea_orm(has_many = "super::storage_policy_group_rule::Entity")]
    StoragePolicyGroupRules,
    /// Users assigned to this profile.
    #[sea_orm(has_many = "super::user::Entity")]
    Users,
}

impl Related<super::storage_policy_group_rule::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::StoragePolicyGroupRules.def()
    }
}

impl Related<super::user::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Users.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
