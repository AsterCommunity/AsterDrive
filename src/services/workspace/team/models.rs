//! 团队服务子模块：`models`。
//!
//! 定义团队与成员的服务层输入输出模型；管理端与成员端入口共用同一套类型。

use serde::Serialize;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::api::pagination::AdminTeamMemberSortBy;
use crate::services::user::account;
use aster_drive_model::types::{TeamMemberRole, UserStatus};
use aster_forge_api::SortOrder;

#[derive(Debug, Clone)]
/// Payload for creating a team; the caller becomes its owner.
pub struct CreateTeamInput {
    /// Team display name, unique among active teams.
    pub name: String,
    /// Optional human-readable team description.
    pub description: Option<String>,
}

/// Payload for updating a team profile; `None` fields are left unchanged.
#[derive(Debug, Clone, Default)]
pub struct UpdateTeamInput {
    /// New team display name, or `None` to keep the current one.
    pub name: Option<String>,
    /// New team description, or `None` to keep the current one.
    pub description: Option<String>,
}

/// Payload for adding a member; the target user is given by `user_id` or `identifier`, not both.
#[derive(Debug, Clone)]
pub struct AddTeamMemberInput {
    /// Target user ID; mutually exclusive with `identifier`.
    pub user_id: Option<i64>,
    /// Target username or email; mutually exclusive with `user_id`.
    pub identifier: Option<String>,
    /// Role assigned to the new member.
    pub role: TeamMemberRole,
}

/// Admin payload for creating a team on behalf of a user, with quota and policy overrides.
#[derive(Debug, Clone)]
pub struct AdminCreateTeamInput {
    /// Team display name.
    pub name: String,
    /// Optional human-readable team description.
    pub description: Option<String>,
    /// User ID of the designated team admin; mutually exclusive with `admin_identifier`.
    pub admin_user_id: Option<i64>,
    /// Username or email of the designated team admin; mutually exclusive with `admin_user_id`.
    pub admin_identifier: Option<String>,
    /// Storage quota override in bytes; `None` applies the platform default.
    pub storage_quota: Option<i64>,
    /// Policy group whose storage policy governs the team; resolved to a required default when `None`.
    pub policy_group_id: Option<i64>,
}

/// Admin payload for updating a team; `None` fields are left unchanged.
#[derive(Debug, Clone, Default)]
pub struct AdminUpdateTeamInput {
    /// New team display name, or `None` to keep the current one.
    pub name: Option<String>,
    /// New team description, or `None` to keep the current one.
    pub description: Option<String>,
    /// New storage quota in bytes, or `None` to keep the current one.
    pub storage_quota: Option<i64>,
    /// New policy group ID, or `None` to keep the current one.
    pub policy_group_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
/// Team view as seen by one of its members.
pub struct TeamInfo {
    /// Team ID.
    pub id: i64,
    /// Team display name.
    pub name: String,
    /// Team description; empty when unset.
    pub description: String,
    /// Creator summary; `None` when the creator account no longer resolves.
    pub created_by: Option<account::UserSummary>,
    /// Role of the requesting user in this team.
    pub my_role: TeamMemberRole,
    /// Current number of members.
    pub member_count: u64,
    /// Storage used by the team workspace in bytes.
    pub storage_used: i64,
    /// Storage quota in bytes.
    pub storage_quota: i64,
    /// Policy group governing the team's storage policy; `None` when unassigned.
    pub policy_group_id: Option<i64>,
    /// Creation time.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Last row update time.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: chrono::DateTime<chrono::Utc>,
    /// Archive time; omitted while the team is active.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub archived_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
/// One team membership as exposed to team and admin views.
pub struct TeamMemberInfo {
    /// Membership row ID.
    pub id: i64,
    /// Team the membership belongs to.
    pub team_id: i64,
    /// Member user ID.
    pub user_id: i64,
    /// Member email, kept on the membership for display when the account is later deleted.
    pub email: String,
    /// Member account summary.
    pub user: account::UserSummary,
    /// Member account status, such as active or disabled.
    pub status: UserStatus,
    /// Role held in the team.
    pub role: TeamMemberRole,
    /// When the member joined the team.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Last membership update time, including role changes.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone)]
/// Filter and sort parameters for member list queries.
pub struct TeamMemberListFilters {
    /// Matches username or email, or an exact user ID; lowercased, `None` disables filtering.
    pub keyword: Option<String>,
    /// Restricts the page to one role; `None` returns all roles.
    pub role: Option<TeamMemberRole>,
    /// Restricts the page to one account status; `None` returns all statuses.
    pub status: Option<UserStatus>,
    /// Sort key for the member page.
    pub sort_by: AdminTeamMemberSortBy,
    /// Sort direction applied to `sort_by`.
    pub sort_order: SortOrder,
}

impl Default for TeamMemberListFilters {
    fn default() -> Self {
        Self {
            keyword: None,
            role: None,
            status: None,
            sort_by: AdminTeamMemberSortBy::Role,
            sort_order: SortOrder::Asc,
        }
    }
}

impl TeamMemberListFilters {
    /// Builds filters from raw query inputs, trimming the keyword and defaulting the sort to role ascending.
    pub fn from_inputs(
        keyword: Option<&str>,
        role: Option<TeamMemberRole>,
        status: Option<UserStatus>,
    ) -> Self {
        Self {
            keyword: keyword
                .map(str::trim)
                .filter(|keyword| !keyword.is_empty())
                .map(str::to_lowercase),
            role,
            status,
            sort_by: AdminTeamMemberSortBy::Role,
            sort_order: SortOrder::Asc,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
/// Offset-paginated member list with role aggregates for ownership guarding in the UI.
pub struct TeamMemberPage {
    /// Members in this page, sorted per the requested filters.
    pub items: Vec<TeamMemberInfo>,
    /// Total number of members matching the filters.
    pub total: u64,
    /// Page size.
    pub limit: u64,
    /// Offset into the filtered result set.
    pub offset: u64,
    /// Number of owners in the whole team.
    pub owner_count: u64,
    /// Number of members who can manage the team (owners plus admins).
    pub manager_count: u64,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
/// Team view for the admin console; identical to `TeamInfo` minus the requester-relative role.
pub struct AdminTeamInfo {
    /// Team ID.
    pub id: i64,
    /// Team display name.
    pub name: String,
    /// Team description; empty when unset.
    pub description: String,
    /// Creator summary; `None` when the creator account no longer resolves.
    pub created_by: Option<account::UserSummary>,
    /// Current number of members.
    pub member_count: u64,
    /// Storage used by the team workspace in bytes.
    pub storage_used: i64,
    /// Storage quota in bytes.
    pub storage_quota: i64,
    /// Policy group governing the team's storage policy; `None` when unassigned.
    pub policy_group_id: Option<i64>,
    /// Creation time.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Last row update time.
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: chrono::DateTime<chrono::Utc>,
    /// Archive time; omitted while the team is active.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub archived_at: Option<chrono::DateTime<chrono::Utc>>,
}
