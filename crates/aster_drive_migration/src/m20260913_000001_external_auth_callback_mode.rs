//! Persist the external-auth callback route selected for each provider.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(ExternalAuthProviders::Table)
                    .add_column(
                        ColumnDef::new(ExternalAuthProviders::CallbackMode)
                            .string_len(16)
                            .not_null()
                            .default("legacy"),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(ExternalAuthProviders::Table)
                    .drop_column(ExternalAuthProviders::CallbackMode)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum ExternalAuthProviders {
    Table,
    CallbackMode,
}
