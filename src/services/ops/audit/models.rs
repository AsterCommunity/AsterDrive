use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::BTreeMap;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::services::user::account;
use crate::types::{AuditAction, AuditEntityType, TeamMemberRole};

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct AuditLogEntry {
    pub id: i64,
    pub user: Option<account::UserSummary>,
    pub action: AuditAction,
    pub entity_type: AuditEntityType,
    pub entity_id: Option<i64>,
    pub entity_name: Option<String>,
    pub details: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presentation: Option<AuditPresentation>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct AuditPresentationMessage {
    pub code: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub params: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct AuditPresentation {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<AuditPresentationMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<AuditPresentationMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<AuditPresentationMessage>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct TeamAuditEntryInfo {
    pub id: i64,
    pub action: AuditAction,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presentation: Option<AuditPresentation>,
    pub actor: Option<account::UserSummary>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub member: Option<account::UserSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<TeamMemberRole>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_role: Option<TeamMemberRole>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_role: Option<TeamMemberRole>,
}
