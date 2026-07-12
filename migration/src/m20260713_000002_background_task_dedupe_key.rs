//! Optional dedupe key for idempotent scheduled runtime task records.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(BackgroundTasks::Table)
                    .add_column(
                        ColumnDef::new(BackgroundTasks::DedupeKey)
                            .string_len(191)
                            .null(),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_background_tasks_dedupe_key_unique")
                    .table(BackgroundTasks::Table)
                    .col(BackgroundTasks::DedupeKey)
                    .unique()
                    .if_not_exists()
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("idx_background_tasks_dedupe_key_unique")
                    .table(BackgroundTasks::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(BackgroundTasks::Table)
                    .drop_column(BackgroundTasks::DedupeKey)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum BackgroundTasks {
    Table,
    DedupeKey,
}
