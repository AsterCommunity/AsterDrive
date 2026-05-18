use chrono::{DateTime, Utc};
use serde::Serialize;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::services::user_service;
use crate::types::{AuditAction, AuditEntityType, TeamMemberRole};

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct AuditLogEntry {
    pub id: i64,
    pub user: Option<user_service::UserSummary>,
    pub action: AuditAction,
    pub entity_type: AuditEntityType,
    pub entity_id: Option<i64>,
    pub entity_name: Option<String>,
    pub details: Option<String>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct TeamAuditEntryInfo {
    pub id: i64,
    pub action: AuditAction,
    pub actor: Option<user_service::UserSummary>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub member: Option<user_service::UserSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<TeamMemberRole>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_role: Option<TeamMemberRole>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_role: Option<TeamMemberRole>,
}
