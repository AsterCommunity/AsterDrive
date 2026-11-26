use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::BTreeMap;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::services::user::account;
use aster_drive_model::types::{AuditAction, AuditEntityType, TeamMemberRole};

/// A single audit log entry as returned to admins.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct AuditLogEntry {
    /// Audit entry id.
    pub id: i64,
    /// Acting user summary; `None` when the actor is the system or is no
    /// longer resolvable.
    pub user: Option<account::UserSummary>,
    /// Audited action.
    pub action: AuditAction,
    /// Type of the entity the action targeted.
    pub entity_type: AuditEntityType,
    /// Id of the targeted entity, when it has a numeric identity.
    pub entity_id: Option<i64>,
    /// Display name of the targeted entity; omitted for share entries so
    /// share tokens never reach responses.
    pub entity_name: Option<String>,
    /// Sanitized JSON details of the action, serialized as a string; `None`
    /// when the entry recorded no details or the stored payload was invalid.
    pub details: Option<String>,
    /// Presentation hints built at read time from the sanitized entry.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presentation: Option<AuditPresentation>,
    /// Client IP recorded with the action, if known.
    pub ip_address: Option<String>,
    /// Client user agent recorded with the action, if known.
    pub user_agent: Option<String>,
    /// When the action was performed (UTC).
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: DateTime<Utc>,
}

/// One localizable audit message: a translation code plus named parameters.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct AuditPresentationMessage {
    /// Translation code identifying the message.
    pub code: String,
    /// Named values interpolated into the translated message.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub params: BTreeMap<String, serde_json::Value>,
}

/// Presentation hints for rendering an audit entry in the UI.
///
/// Message codes and parameters are derived at read time, so historical rows
/// keep following the current presentation rules.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct AuditPresentation {
    /// One-line message describing the action.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<AuditPresentationMessage>,
    /// Message describing the entity the action targeted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<AuditPresentationMessage>,
    /// Action-specific message with extra parameters; `None` for actions
    /// without a detail rendering.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<AuditPresentationMessage>,
}

/// Team member audit entry as returned by the team audit history endpoint.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct TeamAuditEntryInfo {
    /// Audit entry id.
    pub id: i64,
    /// Audited action.
    pub action: AuditAction,
    /// Presentation hints built at read time from the sanitized entry.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presentation: Option<AuditPresentation>,
    /// User who performed the action.
    pub actor: Option<account::UserSummary>,
    /// When the action was performed (UTC).
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: DateTime<Utc>,
    /// Team member the action targeted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub member: Option<account::UserSummary>,
    /// Member role after the action, or the removed role for removals.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<TeamMemberRole>,
    /// Member role before a role change.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_role: Option<TeamMemberRole>,
    /// Member role after a role change.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_role: Option<TeamMemberRole>,
}
