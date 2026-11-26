//! `teams` API DTO 定义。

use serde::Deserialize;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::{IntoParams, ToSchema};
use validator::{Validate, ValidationError};

use crate::api::pagination::AdminTeamMemberSortBy;
use aster_forge_api::SortOrder;

/// Default page size for team listings when no limit is requested.
pub const DEFAULT_TEAM_LIST_LIMIT: u64 = 100;
/// Upper bound for the team listing page size.
pub const MAX_TEAM_LIST_LIMIT: u64 = 200;

// ── Team CRUD ───────────────────────────────────────────────────────────────

/// Query parameters for listing teams.
#[derive(Debug, Deserialize)]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    derive(IntoParams, ToSchema)
)]
pub struct ListTeamsQuery {
    /// Filter by archived state; defaults to active teams only.
    pub archived: Option<bool>,
    /// Optional team name search keyword.
    pub keyword: Option<String>,
    /// Requested page size; clamped by `Self::limit`.
    pub limit: Option<u64>,
    /// Number of teams to skip.
    pub offset: Option<u64>,
}

impl ListTeamsQuery {
    /// Effective page size: `limit` clamped to `1..=MAX_TEAM_LIST_LIMIT`,
    /// falling back to `DEFAULT_TEAM_LIST_LIMIT`.
    pub fn limit(&self) -> u64 {
        self.limit
            .map(|limit| limit.clamp(1, MAX_TEAM_LIST_LIMIT))
            .unwrap_or(DEFAULT_TEAM_LIST_LIMIT)
    }

    /// Effective offset; defaults to 0.
    pub fn offset(&self) -> u64 {
        self.offset.unwrap_or(0)
    }
}

/// Create a new team.
#[derive(Debug, Deserialize, Validate)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct CreateTeamReq {
    /// Team name; validated for characters and length.
    #[validate(custom(function = "crate::api::dto::validation::validate_team_name"))]
    pub name: String,
    /// Optional team description.
    pub description: Option<String>,
}

/// Patch (partial update) a team.
#[derive(Debug, Deserialize, Validate)]
#[validate(schema(function = "validate_patch_team"))]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct PatchTeamReq {
    /// New team name; `None` leaves the current name unchanged.
    pub name: Option<String>,
    /// New team description; `None` leaves the current description unchanged.
    pub description: Option<String>,
}

// ── Team membership ──────────────────────────────────────────────────────────

/// Add a user to a team.
#[derive(Debug, Deserialize, Validate)]
#[validate(schema(function = "validate_add_team_member"))]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct AddTeamMemberReq {
    /// ID of the user to add; exactly one of `user_id`/`identifier` must be set.
    #[validate(range(min = 1, message = "user_id must be greater than 0"))]
    pub user_id: Option<i64>,
    /// Username or email of the user to add; exactly one of
    /// `user_id`/`identifier` must be set.
    pub identifier: Option<String>,
    /// Role for the new member; defaults to `member`.
    pub role: Option<aster_drive_model::types::TeamMemberRole>,
}

/// Patch a team member's role.
#[derive(Debug, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct PatchTeamMemberReq {
    /// New role for the team member.
    pub role: aster_drive_model::types::TeamMemberRole,
}

/// Query parameters for listing team members.
#[derive(Debug, Deserialize, Default)]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    derive(IntoParams, ToSchema)
)]
pub struct ListTeamMembersQuery {
    /// Optional username/display-name search keyword.
    pub keyword: Option<String>,
    /// Filter by member role.
    pub role: Option<aster_drive_model::types::TeamMemberRole>,
    /// Filter by user account status.
    pub status: Option<aster_drive_model::types::UserStatus>,
    /// Sort key; defaults to member role.
    pub sort_by: Option<AdminTeamMemberSortBy>,
    /// Sort direction; defaults to ascending.
    pub sort_order: Option<SortOrder>,
}

impl ListTeamMembersQuery {
    /// Effective sort key; defaults to `AdminTeamMemberSortBy::Role`.
    pub fn sort_by(&self) -> AdminTeamMemberSortBy {
        self.sort_by.unwrap_or(AdminTeamMemberSortBy::Role)
    }

    /// Effective sort direction; defaults to `SortOrder::Asc`.
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
