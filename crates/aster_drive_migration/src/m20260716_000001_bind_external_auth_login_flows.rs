//! Browser binding hash for external authentication login flows.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(ExternalAuthLoginFlows::Table)
                    .add_column(
                        ColumnDef::new(ExternalAuthLoginFlows::BrowserBindingHash)
                            .string_len(64)
                            .null(),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(ExternalAuthLoginFlows::Table)
                    .drop_column(ExternalAuthLoginFlows::BrowserBindingHash)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum ExternalAuthLoginFlows {
    Table,
    BrowserBindingHash,
}
