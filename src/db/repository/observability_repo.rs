//! Bounded result-set aggregates for periodic observation, never called by scrape.

use crate::errors::Result;
use aster_drive_model::{
    entities::{file_blob, managed_follower, upload_session},
    types::{UploadSessionKind, UploadSessionStatus},
};
use chrono::{DateTime, Utc};
use sea_orm::{
    ColumnTrait, ConnectionTrait, EntityTrait, ExprTrait, FromQueryResult, QueryFilter, QuerySelect,
};

#[derive(Debug, FromQueryResult)]
pub struct UploadBacklog {
    pub session_kind: UploadSessionKind,
    pub status: UploadSessionStatus,
    pub count: i64,
    pub oldest: Option<DateTime<Utc>>,
    pub expired: bool,
    pub reserved: Option<i64>,
}

fn upload_backlog_query(
    backend: sea_orm::DbBackend,
    now: DateTime<Utc>,
) -> sea_orm::Select<upload_session::Entity> {
    use upload_session::Column as C;
    let integer_type = match backend {
        sea_orm::DbBackend::Postgres => "bigint",
        sea_orm::DbBackend::MySql => "signed",
        _ => "integer",
    };
    let reserved = sea_orm::sea_query::Expr::col(C::TotalSize)
        .sum()
        .cast_as(integer_type);
    let expired = sea_orm::sea_query::Expr::Case(Box::new(
        sea_orm::sea_query::Expr::case(C::ExpiresAt.lte(now), true).finally(false),
    ));
    upload_session::Entity::find()
        .select_only()
        .column(C::SessionKind)
        .column(C::Status)
        .column_as(C::Status.count(), "count")
        .column_as(C::CreatedAt.min(), "oldest")
        .column_as(expired.clone(), "expired")
        .column_as(reserved, "reserved")
        .filter(C::Status.is_in([
            UploadSessionStatus::Uploading,
            UploadSessionStatus::Assembling,
            UploadSessionStatus::Failed,
            UploadSessionStatus::Presigned,
        ]))
        .group_by(C::SessionKind)
        .group_by(C::Status)
        .group_by(sea_orm::sea_query::Expr::col(
            sea_orm::sea_query::Alias::new("expired"),
        ))
}

pub async fn upload_backlog<C: ConnectionTrait>(
    db: &C,
    now: DateTime<Utc>,
) -> Result<Vec<UploadBacklog>> {
    Ok(upload_backlog_query(db.get_database_backend(), now)
        .into_model::<UploadBacklog>()
        .all(db)
        .await?)
}

#[derive(Debug, FromQueryResult)]
pub struct BlobBacklog {
    pub count: i64,
    pub oldest: Option<DateTime<Utc>>,
}
pub async fn blob_backlog<C: ConnectionTrait>(db: &C) -> Result<BlobBacklog> {
    use file_blob::Column as C;
    Ok(file_blob::Entity::find()
        .select_only()
        .column_as(C::RefCount.count(), "count")
        .column_as(C::CleanupPendingSince.min(), "oldest")
        .filter(C::RefCount.lte(0))
        .filter(C::Backing.eq(aster_drive_model::types::file_blob::FileBlobBacking::Stored))
        .into_model::<BlobBacklog>()
        .one(db)
        .await?
        .unwrap_or(BlobBacklog {
            count: 0,
            oldest: None,
        }))
}

#[derive(Debug, FromQueryResult)]
pub struct RemoteNodeCount {
    pub status: String,
    pub count: i64,
}
pub async fn remote_node_counts<C: ConnectionTrait>(db: &C) -> Result<Vec<RemoteNodeCount>> {
    use managed_follower::Column as C;
    let status = sea_orm::sea_query::Expr::Case(Box::new(
        sea_orm::sea_query::Expr::case(C::IsEnabled.eq(false), "disabled")
            .case(C::LastProbeAt.is_null(), "unknown")
            .case(C::LastProbeError.ne(""), "probe_failed")
            .finally("probe_ok"),
    ));
    Ok(managed_follower::Entity::find()
        .select_only()
        .column_as(status.clone(), "status")
        .column_as(C::IsEnabled.count(), "count")
        .group_by(sea_orm::sea_query::Expr::col(
            sea_orm::sea_query::Alias::new("status"),
        ))
        .into_model::<RemoteNodeCount>()
        .all(db)
        .await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm::{Database, DbBackend, QueryTrait, Statement};

    #[test]
    fn upload_aggregate_sql_uses_portable_integer_cast_and_one_expiration_expression() {
        for (backend, integer) in [
            (DbBackend::Sqlite, "integer"),
            (DbBackend::Postgres, "bigint"),
            (DbBackend::MySql, "signed"),
        ] {
            let sql = upload_backlog_query(backend, Utc::now()).build(backend).sql;
            assert!(sql.contains(&format!(" AS {integer})")), "{sql}");
            assert_eq!(
                sql.matches("CASE").count(),
                1,
                "group by alias must not duplicate bound expressions: {sql}"
            );
            assert!(
                sql.contains(" IN ("),
                "active status ranges must be indexable: {sql}"
            );
        }
    }

    #[tokio::test]
    async fn sqlite_aggregates_expiration_states_reservations_and_cleanup_claims() {
        let db = Database::connect("sqlite::memory:").await.unwrap();
        db.execute_unprepared("CREATE TABLE upload_sessions (id TEXT PRIMARY KEY, session_kind TEXT, status TEXT, total_size BIGINT, created_at TEXT, expires_at TEXT)").await.unwrap();
        db.execute_unprepared(
            "CREATE TABLE file_blobs (id BIGINT PRIMARY KEY, ref_count INTEGER, updated_at TEXT, backing TEXT DEFAULT 'stored')",
        )
        .await
        .unwrap();
        db.execute_unprepared(
            "CREATE TABLE managed_followers (id BIGINT PRIMARY KEY, is_enabled BOOLEAN, last_probe_at TEXT, last_probe_error TEXT DEFAULT '')",
        )
        .await
        .unwrap();
        db.execute_unprepared("INSERT INTO file_blobs (id, ref_count, updated_at) VALUES (10, 0, '2000-01-01 00:00:00+00:00'), (11, -1, '2000-01-01 00:00:00+00:00'), (12, 1, '2000-01-01 00:00:00+00:00')").await.unwrap();
        use aster_drive_migration::MigratorTrait;
        let migration = aster_drive_migration::Migrator::migrations().pop().unwrap();
        let manager = aster_drive_migration::SchemaManager::new(&db);
        migration.up(&manager).await.unwrap();
        let backfilled = db.query_one_raw(Statement::from_string(DbBackend::Sqlite,
            "SELECT COUNT(*) AS count FROM file_blobs WHERE cleanup_pending_since = updated_at AND ref_count <= 0".to_owned())).await.unwrap().unwrap().try_get::<i64>("", "count").unwrap();
        assert_eq!(
            backfilled, 2,
            "migration must backfill both unreferenced and claimed blobs"
        );
        let active = db.query_one_raw(Statement::from_string(DbBackend::Sqlite,
            "SELECT COUNT(*) AS count FROM file_blobs WHERE ref_count > 0 AND cleanup_pending_since IS NULL".to_owned())).await.unwrap().unwrap().try_get::<i64>("", "count").unwrap();
        assert_eq!(active, 1);
        db.execute_unprepared("DELETE FROM file_blobs")
            .await
            .unwrap();
        let now = Utc::now();
        let statement = upload_backlog_query(DbBackend::Sqlite, now).build(DbBackend::Sqlite);
        let plan = db
            .query_all_raw(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                format!("EXPLAIN QUERY PLAN {}", statement.sql),
                statement.values.unwrap().0,
            ))
            .await
            .unwrap();
        assert!(plan.iter().any(|row| {
            row.try_get::<String>("", "detail")
                .unwrap()
                .contains("USING COVERING INDEX idx_upload_sessions_observation")
        }));
        let before = now - chrono::Duration::hours(2);
        let after = now + chrono::Duration::hours(1);
        for (id, kind, status, size, expiration) in [
            ("a", "offset_staging", "uploading", 10_i64, before),
            ("b", "offset_staging", "uploading", 20, after),
            ("c", "provider_presigned_single", "presigned", 30, after),
            ("d", "stream", "completed", 1000, after),
            ("e", "offset_staging", "failed", 5, before),
        ] {
            db.execute_raw(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                "INSERT INTO upload_sessions VALUES (?, ?, ?, ?, ?, ?)",
                [
                    id.into(),
                    kind.into(),
                    status.into(),
                    size.into(),
                    before.into(),
                    expiration.into(),
                ],
            ))
            .await
            .unwrap();
        }
        let rows = upload_backlog(&db, now).await.unwrap();
        assert_eq!(rows.len(), 4);
        assert_eq!(rows.iter().map(|row| row.count).sum::<i64>(), 4);
        assert_eq!(
            rows.iter().map(|row| row.reserved.unwrap()).sum::<i64>(),
            65
        );
        assert_eq!(
            rows.iter()
                .filter(|row| row.expired)
                .map(|row| row.count)
                .sum::<i64>(),
            2
        );
        let empty = blob_backlog(&db).await.unwrap();
        assert_eq!(empty.count, 0);
        assert!(empty.oldest.is_none());
        db.execute_raw(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "INSERT INTO file_blobs (id, ref_count, updated_at, cleanup_pending_since) VALUES (1, 0, ?, ?), (2, -1, ?, ?), (3, 1, ?, NULL)",
            [before.into(), before.into(), now.into(), now.into(), before.into()],
        ))
        .await
        .unwrap();
        let blobs = blob_backlog(&db).await.unwrap();
        assert_eq!(blobs.count, 2);
        assert_eq!(blobs.oldest.unwrap().timestamp(), before.timestamp());
        db.execute_unprepared(
            "INSERT INTO managed_followers (id, is_enabled) VALUES (1, true), (2, false), (3, true)",
        )
        .await
        .unwrap();
        let nodes = remote_node_counts(&db).await.unwrap();
        assert_eq!(
            nodes
                .iter()
                .find(|row| row.status == "unknown")
                .unwrap()
                .count,
            2
        );
        assert_eq!(
            nodes
                .iter()
                .find(|row| row.status == "disabled")
                .unwrap()
                .count,
            1
        );
        db.execute_unprepared("DELETE FROM upload_sessions")
            .await
            .unwrap();
        assert!(upload_backlog(&db, now).await.unwrap().is_empty());
        migration.down(&manager).await.unwrap();
        let count = db.query_one_raw(Statement::from_string(DbBackend::Sqlite,
            "SELECT COUNT(*) AS count FROM file_blobs WHERE ref_count <= 0 AND backing = 'stored'".to_owned())).await.unwrap().unwrap().try_get::<i64>("", "count").unwrap();
        assert_eq!(count, 2, "observation rollback must preserve blob data");
    }
}
