//! Persist the primary's effective reverse-tunnel decision on follower bindings.

use sea_orm_migration::prelude::*;

const MASTER_BINDINGS_TABLE: &str = "master_bindings";
const REVERSE_TUNNEL_ENABLED_COLUMN: &str = "reverse_tunnel_enabled";

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if manager
            .has_column(MASTER_BINDINGS_TABLE, REVERSE_TUNNEL_ENABLED_COLUMN)
            .await?
        {
            return Ok(());
        }

        manager
            .alter_table(
                Table::alter()
                    .table(MasterBindings::Table)
                    .add_column(
                        ColumnDef::new(MasterBindings::ReverseTunnelEnabled)
                            .boolean()
                            .not_null()
                            .default(true),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(MasterBindings::Table)
                    .drop_column(MasterBindings::ReverseTunnelEnabled)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum MasterBindings {
    Table,
    ReverseTunnelEnabled,
}
