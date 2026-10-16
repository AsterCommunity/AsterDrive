//! Persist the cleanup interval independently of retry timestamps and index periodic backlog reads.
use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::ConnectionTrait;

const BACKFILL_BATCH_SIZE: u64 = 1_000;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    fn use_transaction(&self) -> Option<bool> {
        // Each backfill UPDATE commits independently; do not retain row locks
        // and WAL for the entire backlog in PostgreSQL's migration transaction.
        Some(false)
    }

    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if !manager
            .has_column("file_blobs", "cleanup_pending_since")
            .await?
        {
            manager
                .alter_table(
                    Table::alter()
                        .table(FileBlobs::Table)
                        .add_column(
                            aster_forge_db_migration::utc_date_time_column(
                                manager,
                                FileBlobs::CleanupPendingSince,
                            )
                            .null(),
                        )
                        .to_owned(),
                )
                .await?;
        }
        backfill_cleanup_pending_since(manager).await?;
        if !manager
            .has_index("upload_sessions", "idx_upload_sessions_observation")
            .await?
        {
            manager
                .create_index(
                    Index::create()
                        .name("idx_upload_sessions_observation")
                        .table(UploadSessions::Table)
                        .col(UploadSessions::Status)
                        .col(UploadSessions::SessionKind)
                        .col(UploadSessions::ExpiresAt)
                        .col(UploadSessions::CreatedAt)
                        .col(UploadSessions::TotalSize)
                        .to_owned(),
                )
                .await?;
        }
        if !manager
            .has_index("file_blobs", "idx_file_blobs_cleanup_observation")
            .await?
        {
            manager
                .create_index(
                    Index::create()
                        .name("idx_file_blobs_cleanup_observation")
                        .table(FileBlobs::Table)
                        .col(FileBlobs::RefCount)
                        .col(FileBlobs::Backing)
                        .col(FileBlobs::CleanupPendingSince)
                        .to_owned(),
                )
                .await?;
        }
        Ok(())
    }
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("idx_file_blobs_cleanup_observation")
                    .table(FileBlobs::Table)
                    .to_owned(),
            )
            .await?;
        manager
            .drop_index(
                Index::drop()
                    .name("idx_upload_sessions_observation")
                    .table(UploadSessions::Table)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(FileBlobs::Table)
                    .drop_column(FileBlobs::CleanupPendingSince)
                    .to_owned(),
            )
            .await
    }
}

async fn backfill_cleanup_pending_since(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let db = manager.get_connection();
    let mut after_id = None;
    loop {
        let mut select = Query::select();
        select
            .column(FileBlobs::Id)
            .from(FileBlobs::Table)
            .and_where(Expr::col(FileBlobs::RefCount).lte(0))
            .and_where(Expr::col(FileBlobs::CleanupPendingSince).is_null())
            .order_by(FileBlobs::Id, Order::Asc)
            .limit(BACKFILL_BATCH_SIZE);
        if let Some(id) = after_id {
            select.and_where(Expr::col(FileBlobs::Id).gt(id));
        }
        let rows = db.query_all(&select).await?;
        let ids = rows
            .into_iter()
            .map(|row| row.try_get_by_index::<i64>(0))
            .collect::<Result<Vec<_>, _>>()?;
        let Some(last_id) = ids.last().copied() else {
            break;
        };
        let update = Query::update()
            .table(FileBlobs::Table)
            .value(
                FileBlobs::CleanupPendingSince,
                Expr::col(FileBlobs::UpdatedAt),
            )
            .and_where(Expr::col(FileBlobs::Id).is_in(ids))
            .and_where(Expr::col(FileBlobs::RefCount).lte(0))
            .and_where(Expr::col(FileBlobs::CleanupPendingSince).is_null())
            .to_owned();
        db.execute(&update).await?;
        after_id = Some(last_id);
    }
    Ok(())
}

#[derive(DeriveIden)]
enum UploadSessions {
    Table,
    Status,
    SessionKind,
    ExpiresAt,
    CreatedAt,
    TotalSize,
}
#[derive(DeriveIden)]
enum FileBlobs {
    Table,
    Id,
    UpdatedAt,
    RefCount,
    Backing,
    CleanupPendingSince,
}

#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm_migration::sea_orm::Database;

    #[tokio::test]
    async fn paged_backfill_resumes_without_refreshing_existing_observations() {
        assert_eq!(Migration.use_transaction(), Some(false));
        let db = Database::connect("sqlite::memory:").await.unwrap();
        // Simulate a restart after the additive column DDL committed.
        db.execute_unprepared("CREATE TABLE file_blobs (id BIGINT PRIMARY KEY, ref_count INTEGER, backing TEXT, updated_at TEXT, cleanup_pending_since TEXT)").await.unwrap();
        db.execute_unprepared("CREATE TABLE upload_sessions (status TEXT, session_kind TEXT, expires_at TEXT, created_at TEXT, total_size BIGINT)").await.unwrap();
        let mut insert = Query::insert();
        insert.into_table(FileBlobs::Table).columns([
            FileBlobs::Id,
            FileBlobs::RefCount,
            FileBlobs::Backing,
            FileBlobs::UpdatedAt,
            FileBlobs::CleanupPendingSince,
        ]);
        let mut pending = 0_i64;
        for id in -1_i64..(BACKFILL_BATCH_SIZE as i64 * 2 + 3) {
            let refs = if id == -1 {
                -1_i32
            } else if id % 3 == 0 {
                1
            } else {
                0
            };
            pending += i64::from(refs <= 0);
            insert.values_panic([
                id.into(),
                refs.into(),
                "stored".into(),
                "2000-01-01 00:00:00+00:00".into(),
                if id == -1 {
                    "1999-01-01 00:00:00+00:00".into()
                } else {
                    Option::<String>::None.into()
                },
            ]);
        }
        assert!(pending > BACKFILL_BATCH_SIZE as i64);
        db.execute(&insert).await.unwrap();
        let manager = SchemaManager::new(&db);
        Migration.up(&manager).await.unwrap();
        let count = |sql: &'static str| async {
            db.query_one_raw(sea_orm_migration::sea_orm::Statement::from_string(
                db.get_database_backend(),
                sql.to_owned(),
            ))
            .await
            .unwrap()
            .unwrap()
            .try_get::<i64>("", "count")
            .unwrap()
        };
        assert_eq!(count("SELECT COUNT(*) AS count FROM file_blobs WHERE ref_count <= 0 AND cleanup_pending_since IS NOT NULL").await, pending);
        assert_eq!(count("SELECT COUNT(*) AS count FROM file_blobs WHERE ref_count > 0 AND cleanup_pending_since IS NOT NULL").await, 0);
        assert_eq!(count("SELECT COUNT(*) AS count FROM file_blobs WHERE id = -1 AND cleanup_pending_since = '1999-01-01 00:00:00+00:00'").await, 1);
        db.execute_unprepared("UPDATE file_blobs SET updated_at = '2050-01-01 00:00:00+00:00'")
            .await
            .unwrap();
        // Simulate a partially populated last page before retrying the migration.
        db.execute_unprepared("UPDATE file_blobs SET cleanup_pending_since = NULL WHERE id = 2002")
            .await
            .unwrap();
        Migration.up(&manager).await.unwrap();
        assert_eq!(count("SELECT COUNT(*) AS count FROM file_blobs WHERE ref_count <= 0 AND cleanup_pending_since IS NOT NULL").await, pending);
        assert_eq!(count("SELECT COUNT(*) AS count FROM file_blobs WHERE cleanup_pending_since = '2050-01-01 00:00:00+00:00'").await, 1);
        Migration.down(&manager).await.unwrap();
        assert!(
            !manager
                .has_column("file_blobs", "cleanup_pending_since")
                .await
                .unwrap()
        );
        assert_eq!(
            count("SELECT COUNT(*) AS count FROM file_blobs").await,
            BACKFILL_BATCH_SIZE as i64 * 2 + 4
        );
    }
}
