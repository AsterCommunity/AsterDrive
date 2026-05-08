//! `file_repo` 仓储子模块：`common`。

use sea_orm::{ColumnTrait, Condition, DbErr, SqlErr};

use crate::entities::file;
use crate::errors::{AsterError, validation_error_with_subcode};

pub fn duplicate_name_message(name: &str) -> String {
    format!("file '{name}' already exists in this folder")
}

pub fn duplicate_name_error(name: &str) -> AsterError {
    validation_error_with_subcode("file.name_conflict", duplicate_name_message(name))
}

pub fn is_name_conflict_db_err(err: &DbErr) -> bool {
    matches!(err.sql_err(), Some(SqlErr::UniqueConstraintViolation(_)))
}

pub fn map_name_db_err(err: DbErr, name: &str) -> AsterError {
    if is_name_conflict_db_err(&err) {
        duplicate_name_error(name)
    } else {
        AsterError::from(err)
    }
}

pub fn map_bulk_name_db_err(err: DbErr, message: &str) -> AsterError {
    if is_name_conflict_db_err(&err) {
        AsterError::validation_error(message)
    } else {
        AsterError::from(err)
    }
}

pub fn is_duplicate_name_error(err: &AsterError, name: &str) -> bool {
    matches!(err, AsterError::ValidationError(_)) && err.message() == duplicate_name_message(name)
}

pub fn is_any_duplicate_name_error(err: &AsterError) -> bool {
    matches!(err, AsterError::ValidationError(_))
        && err.message().starts_with("file '")
        && err.message().ends_with("' already exists in this folder")
}

#[derive(Clone, Copy)]
pub(super) enum FileScope {
    Personal { user_id: i64 },
    Team { team_id: i64 },
}

pub(super) fn scope_condition(scope: FileScope) -> Condition {
    match scope {
        FileScope::Personal { user_id } => Condition::all()
            .add(file::Column::OwnerUserId.eq(user_id))
            .add(file::Column::TeamId.is_null()),
        FileScope::Team { team_id } => Condition::all().add(file::Column::TeamId.eq(team_id)),
    }
}

pub(super) fn active_scope_condition(scope: FileScope) -> Condition {
    scope_condition(scope).add(file::Column::DeletedAt.is_null())
}

pub(super) fn apply_folder_condition(cond: Condition, folder_id: Option<i64>) -> Condition {
    match folder_id {
        Some(folder_id) => cond.add(file::Column::FolderId.eq(folder_id)),
        None => cond.add(file::Column::FolderId.is_null()),
    }
}
