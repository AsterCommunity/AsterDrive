//! 数据库子模块：`sqlite_search`。

use crate::errors::{AsterError, MapAsterErr, Result};
use sea_orm::{ConnectionTrait, DbBackend, Statement};
use std::collections::HashSet;

pub const SQLITE_SEARCH_MIGRATION_NAMES: &[&str] = &[
    aster_drive_migration::BASELINE_MIGRATION_NAME,
    "m20260415_000001_add_sqlite_search_fts",
    "m20260415_000002_add_user_search_acceleration",
    "m20260415_000003_add_team_search_acceleration",
];

const SQLITE_SEARCH_VIRTUAL_TABLES: &[&str] = &[
    "files_name_fts",
    "folders_name_fts",
    "users_search_fts",
    "teams_search_fts",
];
const SQLITE_SEARCH_SHADOW_TABLE_SUFFIXES: &[&str] =
    &["_config", "_content", "_data", "_docsize", "_idx"];
const SQLITE_SEARCH_TRIGGERS: &[&str] = &[
    "trg_files_name_fts_ai",
    "trg_files_name_fts_ad",
    "trg_files_name_fts_au",
    "trg_folders_name_fts_ai",
    "trg_folders_name_fts_ad",
    "trg_folders_name_fts_au",
    "trg_users_search_fts_ai",
    "trg_users_search_fts_ad",
    "trg_users_search_fts_au",
    "trg_teams_search_fts_ai",
    "trg_teams_search_fts_ad",
    "trg_teams_search_fts_au",
];
const SQLITE_SEARCH_OBJECTS: &[&str] = &[
    "files_name_fts",
    "folders_name_fts",
    "users_search_fts",
    "teams_search_fts",
    "trg_files_name_fts_ai",
    "trg_files_name_fts_ad",
    "trg_files_name_fts_au",
    "trg_folders_name_fts_ai",
    "trg_folders_name_fts_ad",
    "trg_folders_name_fts_au",
    "trg_users_search_fts_ai",
    "trg_users_search_fts_ad",
    "trg_users_search_fts_au",
    "trg_teams_search_fts_ai",
    "trg_teams_search_fts_ad",
    "trg_teams_search_fts_au",
];
const SQLITE_SEARCH_PROBE_TABLE: &str = "asterdrive_sqlite_search_probe_fts";

pub fn is_sqlite_search_table(name: &str) -> bool {
    SQLITE_SEARCH_VIRTUAL_TABLES.iter().any(|table| {
        name == *table
            || name
                .strip_prefix(table)
                .is_some_and(|suffix| SQLITE_SEARCH_SHADOW_TABLE_SUFFIXES.contains(&suffix))
    })
}

pub fn is_sqlite_search_object(name: &str) -> bool {
    is_sqlite_search_table(name) || SQLITE_SEARCH_TRIGGERS.contains(&name)
}

#[derive(Debug, Clone)]
pub struct SqliteSearchStatus {
    pub sqlite_version: String,
    pub missing_objects: Vec<String>,
    pub probe_error: Option<String>,
}

impl SqliteSearchStatus {
    pub fn is_ready(&self) -> bool {
        self.probe_error.is_none() && self.missing_objects.is_empty()
    }

    pub fn probe_supported(&self) -> bool {
        self.probe_error.is_none()
    }

    pub fn detail_lines(&self) -> Vec<String> {
        let mut details = vec![format!("sqlite_version={}", self.sqlite_version)];
        details.extend(
            self.missing_objects
                .iter()
                .map(|name| format!("missing_object={name}")),
        );
        if let Some(error) = &self.probe_error {
            details.push(format!("probe_error={error}"));
        }
        details
    }
}

pub async fn inspect_sqlite_search_status<C: ConnectionTrait>(
    db: &C,
) -> Result<Option<SqliteSearchStatus>> {
    if db.get_database_backend() != DbBackend::Sqlite {
        return Ok(None);
    }

    let sqlite_version = sqlite_scalar_string(db, "SELECT sqlite_version()").await?;
    let existing_objects = sqlite_existing_objects(db).await?;
    let missing_objects = SQLITE_SEARCH_OBJECTS
        .iter()
        .filter(|name| !existing_objects.contains(**name))
        .map(|name| (*name).to_string())
        .collect();

    Ok(Some(SqliteSearchStatus {
        sqlite_version,
        missing_objects,
        probe_error: sqlite_probe_trigram_support(db).await?,
    }))
}

pub async fn ensure_sqlite_search_ready<C: ConnectionTrait>(
    db: &C,
) -> Result<Option<SqliteSearchStatus>> {
    let Some(status) = inspect_sqlite_search_status(db).await? else {
        return Ok(None);
    };

    if status.is_ready() {
        return Ok(Some(status));
    }

    let mut message =
        "SQLite search acceleration is not ready; AsterDrive requires FTS5 with the trigram tokenizer for search features."
            .to_string();
    if !status.missing_objects.is_empty() {
        message.push_str(&format!(
            " missing_objects={}",
            status.missing_objects.join(",")
        ));
    }
    if let Some(error) = &status.probe_error {
        message.push_str(&format!(" probe_error={error}"));
    }
    message.push_str(&format!(" sqlite_version={}", status.sqlite_version));

    Err(AsterError::database_operation(message))
}

async fn sqlite_scalar_string<C: ConnectionTrait>(db: &C, sql: &str) -> Result<String> {
    let row = db
        .query_one_raw(Statement::from_string(DbBackend::Sqlite, sql))
        .await
        .map_err(AsterError::from)?
        .ok_or_else(|| AsterError::database_operation("SQLite scalar query returned no rows"))?;

    row.try_get_by_index::<String>(0)
        .map_aster_err(AsterError::database_operation)
}

async fn sqlite_existing_objects<C: ConnectionTrait>(db: &C) -> Result<HashSet<String>> {
    let names = SQLITE_SEARCH_OBJECTS
        .iter()
        .map(|name| format!("'{name}'"))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT name FROM sqlite_master \
         WHERE name IN ({names})"
    );
    let rows = db
        .query_all_raw(Statement::from_string(DbBackend::Sqlite, sql))
        .await
        .map_err(AsterError::from)?;

    rows.into_iter()
        .map(|row| {
            row.try_get_by_index::<String>(0)
                .map_aster_err(AsterError::database_operation)
        })
        .collect()
}

async fn sqlite_probe_trigram_support<C: ConnectionTrait>(db: &C) -> Result<Option<String>> {
    db.execute_unprepared(&format!(
        "DROP TABLE IF EXISTS temp.{SQLITE_SEARCH_PROBE_TABLE}"
    ))
    .await
    .map_err(AsterError::from)?;

    let create_result = db
        .execute_unprepared(&format!(
            "CREATE VIRTUAL TABLE temp.{SQLITE_SEARCH_PROBE_TABLE} \
             USING fts5(name, tokenize='trigram')"
        ))
        .await;

    let cleanup_result = db
        .execute_unprepared(&format!(
            "DROP TABLE IF EXISTS temp.{SQLITE_SEARCH_PROBE_TABLE}"
        ))
        .await;

    match (create_result, cleanup_result) {
        (Ok(_), Ok(_)) => Ok(None),
        (Ok(_), Err(err)) => Err(AsterError::from(err)),
        (Err(err), Ok(_)) => Ok(Some(err.to_string())),
        (Err(err), Err(cleanup_err)) => Ok(Some(format!("{err}; cleanup_error={cleanup_err}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::{is_sqlite_search_object, is_sqlite_search_table};

    #[test]
    fn sqlite_search_table_detection_includes_shadow_tables() {
        for table in [
            "files_name_fts",
            "folders_name_fts",
            "users_search_fts",
            "teams_search_fts",
            "files_name_fts_config",
            "files_name_fts_content",
            "files_name_fts_data",
            "files_name_fts_docsize",
            "files_name_fts_idx",
            "folders_name_fts_config",
            "folders_name_fts_content",
            "folders_name_fts_data",
            "folders_name_fts_docsize",
            "folders_name_fts_idx",
            "users_search_fts_config",
            "users_search_fts_content",
            "users_search_fts_data",
            "users_search_fts_docsize",
            "users_search_fts_idx",
            "teams_search_fts_config",
            "teams_search_fts_content",
            "teams_search_fts_data",
            "teams_search_fts_docsize",
            "teams_search_fts_idx",
        ] {
            assert!(is_sqlite_search_table(table), "expected {table} to match");
        }
    }

    #[test]
    fn sqlite_search_object_detection_excludes_unrelated_names() {
        for object in [
            "trg_files_name_fts_ai",
            "trg_files_name_fts_ad",
            "trg_files_name_fts_au",
            "trg_folders_name_fts_ai",
            "trg_folders_name_fts_ad",
            "trg_folders_name_fts_au",
            "trg_users_search_fts_ai",
            "trg_users_search_fts_ad",
            "trg_users_search_fts_au",
            "trg_teams_search_fts_ai",
            "trg_teams_search_fts_ad",
            "trg_teams_search_fts_au",
        ] {
            assert!(
                is_sqlite_search_object(object),
                "expected {object} to match"
            );
        }

        for object in [
            "files",
            "folders",
            "files_name_fts_segments",
            "folders_name_fts_shadow",
            "users_search_fts_segments",
            "teams_search_fts_segments",
            "trg_files_name_ai",
        ] {
            assert!(
                !is_sqlite_search_object(object),
                "did not expect {object} to match"
            );
        }
    }
}
