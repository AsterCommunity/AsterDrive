//! 数据库迁移：为上传会话增加前端实例可见性字段。

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(UploadSessions::Table)
                    .add_column(
                        ColumnDef::new(UploadSessions::FrontendClientId)
                            .string_len(36)
                            .null()
                            .to_owned(),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_upload_sessions_frontend_client")
                    .table(UploadSessions::Table)
                    .col(UploadSessions::UserId)
                    .col(UploadSessions::TeamId)
                    .col(UploadSessions::FrontendClientId)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("idx_upload_sessions_frontend_client")
                    .table(UploadSessions::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(UploadSessions::Table)
                    .drop_column(UploadSessions::FrontendClientId)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum UploadSessions {
    Table,
    UserId,
    TeamId,
    FrontendClientId,
}
