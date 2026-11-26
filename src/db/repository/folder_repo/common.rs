//! `folder_repo` 仓储子模块：`common`。

use sea_orm::{ColumnTrait, Condition, DbErr, SqlErr};

use crate::api::api_error_code::ApiErrorCode;
use crate::errors::{AsterError, validation_error_with_code};
use aster_drive_model::entities::folder;

/// Canonical duplicate-folder-name message for `name`, shared by the raise and
/// detection helpers so both always agree on the exact text.
pub fn duplicate_name_message(name: &str) -> String {
    format!("folder '{name}' already exists in this location")
}

/// Builds the `FolderNameConflict` validation error for a duplicate folder name.
pub fn duplicate_name_error(name: &str) -> AsterError {
    validation_error_with_code(
        ApiErrorCode::FolderNameConflict,
        duplicate_name_message(name),
    )
}

/// Returns true when the database rejected the write on the live-name unique index,
/// i.e. `err` is a unique-constraint violation.
pub fn is_name_conflict_db_err(err: &DbErr) -> bool {
    matches!(err.sql_err(), Some(SqlErr::UniqueConstraintViolation(_)))
}

/// Maps a single-row write error: unique-constraint violations become the duplicate-name
/// validation error for `name`; every other error passes through unchanged.
pub fn map_name_db_err(err: DbErr, name: &str) -> AsterError {
    if is_name_conflict_db_err(&err) {
        duplicate_name_error(name)
    } else {
        AsterError::from(err)
    }
}

/// Like [`map_name_db_err`], but for bulk writes the caller supplies the aggregate
/// `message` because the conflicting row cannot be identified.
pub fn map_bulk_name_db_err(err: DbErr, message: &str) -> AsterError {
    if is_name_conflict_db_err(&err) {
        AsterError::validation_error(message)
    } else {
        AsterError::from(err)
    }
}

/// Returns true when `err` is exactly the duplicate-name validation error for `name`.
pub fn is_duplicate_name_error(err: &AsterError, name: &str) -> bool {
    matches!(err, AsterError::ValidationError(_)) && err.message() == duplicate_name_message(name)
}

/// Returns true when `err` is a duplicate-folder-name validation error for any name.
pub fn is_any_duplicate_name_error(err: &AsterError) -> bool {
    matches!(err, AsterError::ValidationError(_))
        && err.message().starts_with("folder '")
        && err.message().ends_with("' already exists in this location")
}

/// Workspace scope narrowing every scoped folder query: a personal space owned by
/// `user_id` (`team_id` must be NULL) or a shared team workspace.
#[derive(Clone, Copy)]
pub(crate) enum FolderScope {
    /// Personal space of the given user; excludes team folders.
    Personal { user_id: i64 },
    /// Shared team workspace; personal folders are never matched.
    Team { team_id: i64 },
}

/// Builds the owner/team WHERE condition for `scope` without any deleted-at filter,
/// so both live and trashed rows match.
pub(super) fn scope_condition(scope: FolderScope) -> Condition {
    match scope {
        FolderScope::Personal { user_id } => Condition::all()
            .add(folder::Column::OwnerUserId.eq(user_id))
            .add(folder::Column::TeamId.is_null()),
        FolderScope::Team { team_id } => Condition::all().add(folder::Column::TeamId.eq(team_id)),
    }
}

/// [`scope_condition`] restricted to live rows: adds `deleted_at IS NULL`.
pub(super) fn active_scope_condition(scope: FolderScope) -> Condition {
    scope_condition(scope).add(folder::Column::DeletedAt.is_null())
}

/// Appends the parent containment predicate: an exact parent match, or root-level
/// (`parent_id IS NULL`) when `parent_id` is `None`.
pub(super) fn apply_parent_condition(cond: Condition, parent_id: Option<i64>) -> Condition {
    match parent_id {
        Some(parent_id) => cond.add(folder::Column::ParentId.eq(parent_id)),
        None => cond.add(folder::Column::ParentId.is_null()),
    }
}
