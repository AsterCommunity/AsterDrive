//! Atomically associate task output files with their producing logical task entry.
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(TaskResourcePublications::Table)
                    .col(
                        ColumnDef::new(TaskResourcePublications::TaskId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(TaskResourcePublications::EntryKey)
                            .string_len(64)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(TaskResourcePublications::ResourceSnapshot)
                            .text()
                            .not_null(),
                    )
                    .primary_key(
                        Index::create()
                            .col(TaskResourcePublications::TaskId)
                            .col(TaskResourcePublications::EntryKey),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_task_resource_publications_task")
                            .from(
                                TaskResourcePublications::Table,
                                TaskResourcePublications::TaskId,
                            )
                            .to(BackgroundTasks::Table, BackgroundTasks::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(TaskResourcePublications::Table)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum TaskResourcePublications {
    Table,
    TaskId,
    EntryKey,
    ResourceSnapshot,
}
#[derive(DeriveIden)]
enum BackgroundTasks {
    Table,
    Id,
}
