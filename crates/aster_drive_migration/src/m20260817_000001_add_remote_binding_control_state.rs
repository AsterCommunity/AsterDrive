//! Add durable binding control-plane state for primary/follower reconciliation.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(MasterBindings::Table)
                    .add_column(
                        ColumnDef::new(MasterBindings::ResolvedTransport)
                            .string_len(32)
                            .not_null()
                            .default("reverse_tunnel"),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(MasterBindings::Table)
                    .add_column(
                        ColumnDef::new(MasterBindings::DesiredRevision)
                            .big_integer()
                            .not_null()
                            .default(1),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(MasterBindings::Table)
                    .add_column(
                        ColumnDef::new(MasterBindings::AppliedRevision)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(ManagedFollowers::Table)
                    .add_column(
                        ColumnDef::new(ManagedFollowers::BindingRevision)
                            .big_integer()
                            .not_null()
                            .default(1),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(ManagedFollowers::Table)
                    .add_column(
                        ColumnDef::new(ManagedFollowers::BindingAppliedRevision)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(ManagedFollowers::Table)
                    .drop_column(ManagedFollowers::BindingAppliedRevision)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(ManagedFollowers::Table)
                    .drop_column(ManagedFollowers::BindingRevision)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(MasterBindings::Table)
                    .drop_column(MasterBindings::AppliedRevision)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(MasterBindings::Table)
                    .drop_column(MasterBindings::DesiredRevision)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(MasterBindings::Table)
                    .drop_column(MasterBindings::ResolvedTransport)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum MasterBindings {
    Table,
    ResolvedTransport,
    DesiredRevision,
    AppliedRevision,
}

#[derive(DeriveIden)]
enum ManagedFollowers {
    Table,
    BindingRevision,
    BindingAppliedRevision,
}
