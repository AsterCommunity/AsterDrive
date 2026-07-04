//! Add explicit remote storage target ownership to remote storage policies.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if !manager
            .has_column(
                StoragePolicies::Table.to_string(),
                StoragePolicies::RemoteStorageTargetKey.to_string(),
            )
            .await?
        {
            manager
                .alter_table(
                    Table::alter()
                        .table(StoragePolicies::Table)
                        .add_column(
                            ColumnDef::new(StoragePolicies::RemoteStorageTargetKey)
                                .string_len(255)
                                .null(),
                        )
                        .to_owned(),
                )
                .await?;
        }

        manager
            .create_index(
                Index::create()
                    .name("idx_storage_policies_remote_target")
                    .table(StoragePolicies::Table)
                    .col(StoragePolicies::RemoteNodeId)
                    .col(StoragePolicies::RemoteStorageTargetKey)
                    .if_not_exists()
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("idx_storage_policies_remote_target")
                    .table(StoragePolicies::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(StoragePolicies::Table)
                    .drop_column(StoragePolicies::RemoteStorageTargetKey)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum StoragePolicies {
    Table,
    RemoteNodeId,
    RemoteStorageTargetKey,
}
