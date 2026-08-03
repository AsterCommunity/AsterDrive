//! Add the connector-owned encrypted storage-policy credential store.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(StoragePolicyConnectorCredentials::Table)
                    .if_not_exists()
                    .col(aster_forge_db_migration::big_integer_primary_key(
                        StoragePolicyConnectorCredentials::Id,
                    ))
                    .col(
                        ColumnDef::new(StoragePolicyConnectorCredentials::PolicyId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(StoragePolicyConnectorCredentials::ConnectorId)
                            .string_len(128)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(StoragePolicyConnectorCredentials::SchemaVersion)
                            .integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(StoragePolicyConnectorCredentials::Revision)
                            .big_integer()
                            .not_null()
                            .default(1),
                    )
                    .col(
                        ColumnDef::new(StoragePolicyConnectorCredentials::Ciphertext)
                            .text()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(StoragePolicyConnectorCredentials::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(StoragePolicyConnectorCredentials::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_storage_policy_connector_credentials_policy")
                            .from(
                                StoragePolicyConnectorCredentials::Table,
                                StoragePolicyConnectorCredentials::PolicyId,
                            )
                            .to(StoragePolicies::Table, StoragePolicies::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_storage_policy_connector_credentials_policy")
                    .table(StoragePolicyConnectorCredentials::Table)
                    .col(StoragePolicyConnectorCredentials::PolicyId)
                    .unique()
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(StoragePolicyConnectorCredentials::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum StoragePolicyConnectorCredentials {
    Table,
    Id,
    PolicyId,
    ConnectorId,
    SchemaVersion,
    Revision,
    Ciphertext,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum StoragePolicies {
    Table,
    Id,
}
