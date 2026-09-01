use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_index(
                Index::create()
                    .name("idx_upload_sessions_folder_status_expires")
                    .table(UploadSessions::Table)
                    .col(UploadSessions::FolderId)
                    .col(UploadSessions::Status)
                    .col(UploadSessions::ExpiresAt)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("idx_upload_sessions_folder_status_expires")
                    .table(UploadSessions::Table)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum UploadSessions {
    Table,
    FolderId,
    Status,
    ExpiresAt,
}
