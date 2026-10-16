//! Persist the cleanup interval independently of retry timestamps and index periodic backlog reads.
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
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
        manager
            .get_connection()
            .execute_unprepared(
                "UPDATE file_blobs SET cleanup_pending_since = updated_at WHERE ref_count <= 0",
            )
            .await?;
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
    RefCount,
    Backing,
    CleanupPendingSince,
}
