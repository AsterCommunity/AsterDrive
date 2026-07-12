//! `teams` API DTO 定义。

use serde::Deserialize;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::{IntoParams, ToSchema};
use validator::{Validate, ValidationError};

use crate::api::pagination::AdminTeamMemberSortBy;
use aster_forge_api::SortOrder;

pub const DEFAULT_TEAM_LIST_LIMIT: u64 = 100;
pub const MAX_TEAM_LIST_LIMIT: u64 = 200;

// ── Team CRUD ───────────────────────────────────────────────────────────────

/// Query parameters for listing teams.
#[derive(Debug, Deserialize)]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    derive(IntoParams, ToSchema)
)]
pub struct ListTeamsQuery {
    pub archived: Option<bool>,
    pub keyword: Option<String>,
    pub limit: Option<u64>,
    pub offset: Option<u64>,
}

impl ListTeamsQuery {
    pub fn limit(&self) -> u64 {
        self.limit
            .map(|limit| limit.clamp(1, MAX_TEAM_LIST_LIMIT))
            .unwrap_or(DEFAULT_TEAM_LIST_LIMIT)
    }

    pub fn offset(&self) -> u64 {
        self.offset.unwrap_or(0)
    }
}

/// Create a new team.
#[derive(Debug, Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct CreateTeamReq {
    #[validate(custom(function = "crate::api::dto::validation::validate_team_name"))]
    pub name: String,
    pub description: Option<String>,
}

/// Patch (partial update) a team.
#[derive(Debug, Deserialize, Validate)]
#[validate(schema(function = "validate_patch_team"))]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct PatchTeamReq {
    pub name: Option<String>,
    pub description: Option<String>,
}

// ── Team membership ──────────────────────────────────────────────────────────

/// Add a user to a team.
#[derive(Debug, Deserialize, Validate)]
#[validate(schema(function = "validate_add_team_member"))]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct AddTeamMemberReq {
    #[validate(range(min = 1, message = "user_id must be greater than 0"))]
    pub user_id: Option<i64>,
    pub identifier: Option<String>,
    pub role: Option<crate::types::TeamMemberRole>,
}

/// Patch a team member's role.
#[derive(Debug, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct PatchTeamMemberReq {
    pub role: crate::types::TeamMemberRole,
}

/// Query parameters for listing team members.
#[derive(Debug, Deserialize, Default)]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    derive(IntoParams, ToSchema)
)]
pub struct ListTeamMembersQuery {
    pub keyword: Option<String>,
    pub role: Option<crate::types::TeamMemberRole>,
    pub status: Option<crate::types::UserStatus>,
    pub sort_by: Option<AdminTeamMemberSortBy>,
    pub sort_order: Option<SortOrder>,
}

impl ListTeamMembersQuery {
    pub fn sort_by(&self) -> AdminTeamMemberSortBy {
        self.sort_by.unwrap_or(AdminTeamMemberSortBy::Role)
    }

    pub fn sort_order(&self) -> SortOrder {
        self.sort_order.unwrap_or(SortOrder::Asc)
    }
}

fn validate_patch_team(value: &PatchTeamReq) -> std::result::Result<(), ValidationError> {
    if let Some(name) = value.name.as_deref() {
        crate::api::dto::validation::validate_team_name(name)?;
    }
    Ok(())
}

fn validate_add_team_member(value: &AddTeamMemberReq) -> std::result::Result<(), ValidationError> {
    let identifier = value
        .identifier
        .as_deref()
        .map(str::trim)
        .filter(|identifier| !identifier.is_empty());

    match (value.user_id, identifier) {
        (Some(_), Some(_)) => Err(crate::api::dto::validation::message_validation_error(
            "specify either user_id or identifier, not both",
        )),
        (None, None) => Err(crate::api::dto::validation::message_validation_error(
            "user_id or identifier is required",
        )),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_TEAM_LIST_LIMIT, ListTeamsQuery, MAX_TEAM_LIST_LIMIT};

    #[test]
    fn list_teams_query_applies_default_and_max_limit() {
        let default_query = ListTeamsQuery {
            archived: None,
            keyword: None,
            limit: None,
            offset: None,
        };
        assert_eq!(default_query.limit(), DEFAULT_TEAM_LIST_LIMIT);
        assert_eq!(default_query.offset(), 0);

        let oversized_query = ListTeamsQuery {
            archived: None,
            keyword: None,
            limit: Some(MAX_TEAM_LIST_LIMIT + 1),
            offset: Some(25),
        };
        assert_eq!(oversized_query.limit(), MAX_TEAM_LIST_LIMIT);
        assert_eq!(oversized_query.offset(), 25);
    }
}
