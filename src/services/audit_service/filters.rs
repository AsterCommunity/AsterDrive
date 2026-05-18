use chrono::{DateTime, Utc};
use serde::Deserialize;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::{IntoParams, ToSchema};

use crate::types::AuditEntityType;

#[derive(Deserialize)]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    derive(IntoParams, ToSchema)
)]
pub struct AuditLogFilterQuery {
    pub user_id: Option<i64>,
    pub action: Option<String>,
    pub entity_type: Option<AuditEntityType>,
    pub entity_id: Option<i64>,
    pub after: Option<String>,
    pub before: Option<String>,
}

pub struct AuditLogFilters {
    pub user_id: Option<i64>,
    pub action: Option<String>,
    pub entity_type: Option<AuditEntityType>,
    pub entity_id: Option<i64>,
    pub after: Option<DateTime<Utc>>,
    pub before: Option<DateTime<Utc>>,
}

impl AuditLogFilters {
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
