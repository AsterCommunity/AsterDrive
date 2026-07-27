//! 数据库迁移：新增存储策略 blob 迁移任务检查点表。

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(StorageMigrationCheckpoints::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(StorageMigrationCheckpoints::TaskId)
                            .big_integer()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(StorageMigrationCheckpoints::SourcePolicyId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(StorageMigrationCheckpoints::TargetPolicyId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(StorageMigrationCheckpoints::PlanHash)
                            .string_len(64)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(StorageMigrationCheckpoints::Stage)
                            .string_len(32)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(StorageMigrationCheckpoints::LastProcessedBlobId)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(StorageMigrationCheckpoints::ScannedBlobs)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(StorageMigrationCheckpoints::MigratedBlobs)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(StorageMigrationCheckpoints::MergedBlobs)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(StorageMigrationCheckpoints::SkippedBlobs)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(StorageMigrationCheckpoints::FailedBlobs)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(StorageMigrationCheckpoints::MigratedBytes)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(StorageMigrationCheckpoints::LastError)
                            .text()
                            .null(),
                    )
                    .col(
                        aster_forge_db_migration::utc_date_time_column(
                            manager,
                            StorageMigrationCheckpoints::CreatedAt,
                        )
                        .not_null(),
                    )
                    .col(
                        aster_forge_db_migration::utc_date_time_column(
                            manager,
                            StorageMigrationCheckpoints::UpdatedAt,
                        )
                        .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(
                                StorageMigrationCheckpoints::Table,
                                StorageMigrationCheckpoints::TaskId,
                            )
                            .to(BackgroundTasks::Table, BackgroundTasks::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_storage_migration_checkpoints_source_target")
                    .table(StorageMigrationCheckpoints::Table)
                    .col(StorageMigrationCheckpoints::SourcePolicyId)
                    .col(StorageMigrationCheckpoints::TargetPolicyId)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(StorageMigrationCheckpoints::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum StorageMigrationCheckpoints {
    Table,
    TaskId,
    SourcePolicyId,
    TargetPolicyId,
    PlanHash,
    Stage,
    LastProcessedBlobId,
    ScannedBlobs,
    MigratedBlobs,
    MergedBlobs,
    SkippedBlobs,
    FailedBlobs,
    MigratedBytes,
    LastError,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum BackgroundTasks {
    Table,
    Id,
}
