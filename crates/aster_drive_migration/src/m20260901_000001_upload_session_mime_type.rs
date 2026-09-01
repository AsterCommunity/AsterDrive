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
                        ColumnDef::new(UploadSessions::MimeType)
                            .string_len(255)
                            .not_null()
                            .default("application/octet-stream"),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(UploadSessions::Table)
                    .drop_column(UploadSessions::MimeType)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum UploadSessions {
    Table,
    MimeType,
}
