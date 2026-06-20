//! Add canonical connector application config storage separate from OAuth credentials.

use crate::column::json_text_column_for_final_schema;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(StorageConnectorApplicationConfigs::Table)
                    .if_not_exists()
                    .col(big_integer_pk(StorageConnectorApplicationConfigs::Id))
                    .col(
                        ColumnDef::new(StorageConnectorApplicationConfigs::PolicyId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(StorageConnectorApplicationConfigs::Provider)
                            .string_len(32)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(StorageConnectorApplicationConfigs::TenantId)
                            .string_len(255)
                            .null(),
                    )
                    .col(
                        ColumnDef::new(StorageConnectorApplicationConfigs::Scopes)
                            .text()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(StorageConnectorApplicationConfigs::ClientId)
                            .string_len(512)
                            .null(),
                    )
                    .col(
                        ColumnDef::new(StorageConnectorApplicationConfigs::ClientSecretCiphertext)
                            .text()
                            .null(),
                    )
                    .col(json_text_column_for_final_schema(
                        manager,
                        StorageConnectorApplicationConfigs::Metadata,
                    ))
                    .col(
                        crate::time::utc_date_time_column(
                            manager,
                            StorageConnectorApplicationConfigs::CreatedAt,
                        )
                        .not_null(),
                    )
                    .col(
                        crate::time::utc_date_time_column(
                            manager,
                            StorageConnectorApplicationConfigs::UpdatedAt,
                        )
                        .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(
                                StorageConnectorApplicationConfigs::Table,
                                StorageConnectorApplicationConfigs::PolicyId,
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
                    .name("idx_storage_connector_app_configs_policy_provider")
                    .table(StorageConnectorApplicationConfigs::Table)
                    .col(StorageConnectorApplicationConfigs::PolicyId)
                    .col(StorageConnectorApplicationConfigs::Provider)
                    .unique()
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_storage_connector_app_configs_provider")
                    .table(StorageConnectorApplicationConfigs::Table)
                    .col(StorageConnectorApplicationConfigs::Provider)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(StorageConnectorApplicationConfigs::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await
    }
}

fn big_integer_pk<T: IntoIden>(name: T) -> ColumnDef {
    let mut column = ColumnDef::new(name);
    column
        .big_integer()
        .not_null()
        .auto_increment()
        .primary_key();
    column
}

#[derive(DeriveIden)]
enum StoragePolicies {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum StorageConnectorApplicationConfigs {
    Table,
    Id,
    PolicyId,
    Provider,
    TenantId,
    Scopes,
    ClientId,
    ClientSecretCiphertext,
    Metadata,
    CreatedAt,
    UpdatedAt,
}
