//! Cross-ledger queries used to preview and execute a confirmed policy purge.

use aster_drive_model::entities::file::{self, Entity as File};
use sea_orm::{
    ColumnTrait, ConnectionTrait, DbBackend, EntityTrait, FromQueryResult, QueryFilter, QueryOrder,
};

use crate::errors::{AsterError, Result};

/// Authoritative impact of purging every file history that references a policy blob.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, FromQueryResult)]
pub struct StoragePolicyPurgeImpact {
    /// Distinct file histories that will be permanently removed.
    pub file_count: i64,
    /// Distinct affected files currently located in trash.
    pub trash_file_count: i64,
    /// Revisions whose blob points at the source policy.
    pub affected_revision_count: i64,
    /// Logical bytes represented by affected revisions.
    pub affected_logical_bytes: i64,
    /// Direct file shares removed with affected files.
    pub direct_share_count: i64,
}

/// Summarizes destructive policy-purge impact from the canonical revision ledger.
///
/// Current content is represented by the current canonical revision, so this query
/// intentionally uses revision histories instead of combining two projections and
/// risking double counting.
pub async fn summarize_storage_policy_purge_impact<C: ConnectionTrait>(
    db: &C,
    policy_id: i64,
) -> Result<StoragePolicyPurgeImpact> {
    let placeholder = if db.get_database_backend() == DbBackend::Postgres {
        "$1"
    } else {
        "?"
    };
    let integer_type = match db.get_database_backend() {
        DbBackend::Postgres => "BIGINT",
        DbBackend::MySql => "SIGNED",
        DbBackend::Sqlite | _ => "INTEGER",
    };
    let sql = format!(
        "SELECT \
            CAST(COUNT(DISTINCT f.id) AS {integer_type}) AS file_count, \
            CAST(COUNT(DISTINCT CASE WHEN f.deleted_at IS NOT NULL THEN f.id END) AS {integer_type}) AS trash_file_count, \
            CAST(COUNT(DISTINCT r.id) AS {integer_type}) AS affected_revision_count, \
            CAST((SELECT COALESCE(SUM(r3.logical_size), 0) \
             FROM file_revisions r3 \
             JOIN file_revision_histories h3 ON h3.id = r3.history_id \
             WHERE r3.retired_at IS NULL AND h3.file_id IN ( \
                SELECT DISTINCT h4.file_id FROM file_revision_histories h4 \
                JOIN file_revisions r4 ON r4.history_id = h4.id \
                JOIN file_blobs b4 ON b4.id = r4.blob_id \
                WHERE b4.policy_id = {placeholder} \
             )) AS {integer_type}) AS affected_logical_bytes, \
            CAST((SELECT COUNT(DISTINCT s.id) FROM shares s WHERE s.file_id IN ( \
                SELECT DISTINCT h2.file_id FROM file_revision_histories h2 \
                JOIN file_revisions r2 ON r2.history_id = h2.id \
                JOIN file_blobs b2 ON b2.id = r2.blob_id \
                WHERE b2.policy_id = {placeholder} \
            )) AS {integer_type}) AS direct_share_count \
         FROM file_blobs b \
         JOIN file_revisions r ON r.blob_id = b.id \
         JOIN file_revision_histories h ON h.id = r.history_id \
         JOIN files f ON f.id = h.file_id \
         WHERE b.policy_id = {placeholder} AND r.retired_at IS NULL"
    );
    let values = if db.get_database_backend() == DbBackend::Postgres {
        vec![policy_id.into()]
    } else {
        vec![policy_id.into(), policy_id.into(), policy_id.into()]
    };
    StoragePolicyPurgeImpact::find_by_statement(sea_orm::Statement::from_sql_and_values(
        db.get_database_backend(),
        sql,
        values,
    ))
    .one(db)
    .await
    .map_err(AsterError::from)?
    .ok_or_else(|| AsterError::internal_error("storage policy purge impact returned no row"))
}

/// Loads a stable file-ID page whose current or historical revision references a policy blob.
pub async fn find_files_referencing_policy_blobs_paginated<C: ConnectionTrait>(
    db: &C,
    policy_id: i64,
    after_file_id: i64,
    limit: u64,
) -> Result<Vec<file::Model>> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let (policy_placeholder, after_placeholder, limit_placeholder) =
        if db.get_database_backend() == DbBackend::Postgres {
            ("$1", "$2", "$3")
        } else {
            ("?", "?", "?")
        };
    #[derive(FromQueryResult)]
    struct FileIdRow {
        file_id: i64,
    }
    let sql = format!(
        "SELECT DISTINCT h.file_id AS file_id \
         FROM file_revision_histories h \
         JOIN file_revisions r ON r.history_id = h.id \
         JOIN file_blobs b ON b.id = r.blob_id \
         WHERE b.policy_id = {policy_placeholder} AND h.file_id > {after_placeholder} \
         ORDER BY h.file_id ASC LIMIT {limit_placeholder}"
    );
    let rows = FileIdRow::find_by_statement(sea_orm::Statement::from_sql_and_values(
        db.get_database_backend(),
        sql,
        [policy_id.into(), after_file_id.into(), limit.into()],
    ))
    .all(db)
    .await
    .map_err(AsterError::from)?;
    let ids = rows.into_iter().map(|row| row.file_id).collect::<Vec<_>>();
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    File::find()
        .filter(file::Column::Id.is_in(ids))
        .order_by_asc(file::Column::Id)
        .all(db)
        .await
        .map_err(AsterError::from)
}
