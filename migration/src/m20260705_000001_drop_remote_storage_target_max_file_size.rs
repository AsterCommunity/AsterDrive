//! Remove target-level upload size limits from remote storage targets.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if manager
            .has_column(
                RemoteStorageTargets::Table.to_string(),
                RemoteStorageTargets::MaxFileSize.to_string(),
            )
            .await?
        {
            manager
                .alter_table(
                    Table::alter()
                        .table(RemoteStorageTargets::Table)
                        .drop_column(RemoteStorageTargets::MaxFileSize)
                        .to_owned(),
                )
                .await?;
        }

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if !manager
            .has_column(
                RemoteStorageTargets::Table.to_string(),
                RemoteStorageTargets::MaxFileSize.to_string(),
            )
            .await?
        {
            manager
                .alter_table(
                    Table::alter()
                        .table(RemoteStorageTargets::Table)
                        .add_column(
                            ColumnDef::new(RemoteStorageTargets::MaxFileSize)
                                .big_integer()
                                .not_null()
                                .default(0),
                        )
                        .to_owned(),
                )
                .await?;
        }

        Ok(())
    }
}

#[derive(DeriveIden)]
enum RemoteStorageTargets {
    Table,
    MaxFileSize,
}
