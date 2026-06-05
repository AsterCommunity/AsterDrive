//! `folder_repo` 仓储子模块：`common`。

use sea_orm::{ColumnTrait, Condition, DbErr, SqlErr};

use crate::api::subcode::ApiSubcode;
use crate::entities::folder;
use crate::errors::{AsterError, validation_error_with_subcode};

pub fn duplicate_name_message(name: &str) -> String {
    format!("folder '{name}' already exists in this location")
}

pub fn duplicate_name_error(name: &str) -> AsterError {
    validation_error_with_subcode(ApiSubcode::FolderNameConflict, duplicate_name_message(name))
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
        && err.message().starts_with("folder '")
        && err.message().ends_with("' already exists in this location")
}

#[derive(Clone, Copy)]
pub(crate) enum FolderScope {
    Personal { user_id: i64 },
    Team { team_id: i64 },
}

pub(super) fn scope_condition(scope: FolderScope) -> Condition {
    match scope {
        FolderScope::Personal { user_id } => Condition::all()
            .add(folder::Column::OwnerUserId.eq(user_id))
            .add(folder::Column::TeamId.is_null()),
        FolderScope::Team { team_id } => Condition::all().add(folder::Column::TeamId.eq(team_id)),
    }
}

pub(super) fn active_scope_condition(scope: FolderScope) -> Condition {
    scope_condition(scope).add(folder::Column::DeletedAt.is_null())
}

pub(super) fn apply_parent_condition(cond: Condition, parent_id: Option<i64>) -> Condition {
    match parent_id {
        Some(parent_id) => cond.add(folder::Column::ParentId.eq(parent_id)),
        None => cond.add(folder::Column::ParentId.is_null()),
    }
}
