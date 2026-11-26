//! SeaORM 实体定义：`audit_log`。

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::types::AuditAction;

/// Append-only audit trail entry for a security- or compliance-relevant action.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[cfg_attr(all(debug_assertions, feature = "openapi"), schema(as = AuditLogEntry))]
#[sea_orm(table_name = "audit_logs")]
pub struct Model {
    /// Auto-incrementing surrogate key.
    #[sea_orm(primary_key)]
    pub id: i64,
    /// Acting account; references `users.id`.
    pub user_id: i64,
    /// Canonical action name (snake_case wire name of [`AuditAction`]).
    ///
    /// [`AuditAction`]: crate::types::AuditAction
    pub action: AuditAction,
    /// Audited resource type as a snake_case wire name (`file`, `share`, `team`, ...);
    /// empty when the action does not target a resource.
    pub entity_type: String,
    /// Audited resource primary key; `None` for actions without a single numeric target.
    pub entity_id: Option<i64>,
    /// Resource display name snapshotted at write time so later renames never rewrite history.
    pub entity_name: Option<String>,
    /// JSON-encoded action-specific context (reason, remark, result, request identifiers).
    pub details: Option<String>,
    /// Client IP address behind the action; `None` when the transport cannot supply one.
    pub ip_address: Option<String>,
    /// Client `User-Agent` behind the action; `None` when the transport cannot supply one.
    pub user_agent: Option<String>,
    /// UTC instant the entry was persisted.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: DateTimeUtc,
}

/// No database-level relations are declared for this entity.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
