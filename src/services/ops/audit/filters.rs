use chrono::{DateTime, Utc};
use serde::Deserialize;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::{IntoParams, ToSchema};

use aster_drive_model::types::AuditEntityType;

#[derive(Deserialize)]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    derive(IntoParams, ToSchema)
)]
/// Raw audit log filters as accepted on admin listing and export endpoints.
pub struct AuditLogFilterQuery {
    /// Restrict entries to actions performed by this user.
    pub user_id: Option<i64>,
    /// Restrict entries to this exact audit action name (e.g. `user_login`).
    pub action: Option<String>,
    /// Restrict entries to this entity type.
    pub entity_type: Option<AuditEntityType>,
    /// Restrict entries to this entity id.
    pub entity_id: Option<i64>,
    /// Lower time bound as an RFC 3339 string; entries created at or after
    /// this instant are kept.
    pub after: Option<String>,
    /// Upper time bound as an RFC 3339 string; entries created at or before
    /// this instant are kept.
    pub before: Option<String>,
}

/// Normalized audit log filters shared by listing, team history, and CSV
/// export paths.
#[derive(Debug, Clone, Default)]
pub struct AuditLogFilters {
    /// Restrict entries to actions performed by this user.
    pub user_id: Option<i64>,
    /// Restrict entries to this exact audit action name (e.g. `user_login`).
    pub action: Option<String>,
    /// Restrict entries to this entity type.
    pub entity_type: Option<AuditEntityType>,
    /// Restrict entries to this entity id.
    pub entity_id: Option<i64>,
    /// Inclusive lower bound on `created_at`.
    pub after: Option<DateTime<Utc>>,
    /// Inclusive upper bound on `created_at`.
    pub before: Option<DateTime<Utc>>,
}

impl AuditLogFilters {
    /// Converts the raw query filters, parsing the RFC 3339 time bounds.
    ///
    /// Unparseable `after`/`before` values are silently dropped instead of
    /// rejecting the request, so the filter degrades to an unbounded range.
    pub fn from_query(query: &AuditLogFilterQuery) -> Self {
        Self {
            user_id: query.user_id,
            action: query.action.clone(),
            entity_type: query.entity_type,
            entity_id: query.entity_id,
            after: query
                .after
                .as_deref()
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&Utc)),
            before: query
                .before
                .as_deref()
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&Utc)),
        }
    }
}
