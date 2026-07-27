//! Add the shared-database lookup used by provider relay resumable chunk claims.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_index(
                Index::create()
                    .name("idx_upload_sessions_provider_relay_ordering")
                    .table(UploadSessions::Table)
                    .col(UploadSessions::SessionKind)
                    .col(UploadSessions::Status)
                    .col(UploadSessions::ReceivedCount)
                    .col(UploadSessions::Id)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("idx_upload_sessions_provider_relay_ordering")
                    .table(UploadSessions::Table)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum UploadSessions {
    Table,
    Id,
    SessionKind,
    Status,
    ReceivedCount,
}
