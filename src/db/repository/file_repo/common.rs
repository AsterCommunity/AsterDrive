//! `file_repo` 仓储子模块：`common`。

use sea_orm::{ColumnTrait, Condition, DbErr, SqlErr};

use crate::api::api_error_code::ApiErrorCode;
use crate::errors::{AsterError, validation_error_with_code};
use aster_drive_model::entities::file;

/// Canonical duplicate-file-name message for `name`, shared by the raise and
/// detection helpers so both always agree on the exact text.
pub fn duplicate_name_message(name: &str) -> String {
    format!("file '{name}' already exists in this folder")
}

/// Builds the `FileNameConflict` validation error for a duplicate file name.
pub fn duplicate_name_error(name: &str) -> AsterError {
    validation_error_with_code(ApiErrorCode::FileNameConflict, duplicate_name_message(name))
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

/// Returns true when `err` is a duplicate-file-name validation error for any name.
pub fn is_any_duplicate_name_error(err: &AsterError) -> bool {
    matches!(err, AsterError::ValidationError(_))
        && err.message().starts_with("file '")
        && err.message().ends_with("' already exists in this folder")
}

/// Workspace scope narrowing every scoped file query: a personal space owned by
/// `user_id` (`team_id` must be NULL) or a shared team workspace.
#[derive(Clone, Copy)]
pub(crate) enum FileScope {
    /// Personal space of the given user; excludes team files.
    Personal { user_id: i64 },
    /// Shared team workspace; personal files are never matched.
    Team { team_id: i64 },
}

/// Builds the owner/team WHERE condition for `scope` without any deleted-at filter,
/// so both live and trashed rows match.
pub(super) fn scope_condition(scope: FileScope) -> Condition {
    match scope {
        FileScope::Personal { user_id } => Condition::all()
            .add(file::Column::OwnerUserId.eq(user_id))
            .add(file::Column::TeamId.is_null()),
        FileScope::Team { team_id } => Condition::all().add(file::Column::TeamId.eq(team_id)),
    }
}

/// [`scope_condition`] restricted to live rows: adds `deleted_at IS NULL`.
pub(super) fn active_scope_condition(scope: FileScope) -> Condition {
    scope_condition(scope).add(file::Column::DeletedAt.is_null())
}

/// Appends the folder containment predicate: an exact folder match, or root-level
/// (`folder_id IS NULL`) when `folder_id` is `None`.
pub(super) fn apply_folder_condition(cond: Condition, folder_id: Option<i64>) -> Condition {
    match folder_id {
        Some(folder_id) => cond.add(file::Column::FolderId.eq(folder_id)),
        None => cond.add(file::Column::FolderId.is_null()),
    }
}
