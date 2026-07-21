//! Shared reverse-tunnel owner directory for multi-primary deployments.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(RemoteTunnelOwners::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(RemoteTunnelOwners::RemoteNodeId)
                            .big_integer()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(RemoteTunnelOwners::RuntimeId)
                            .string_len(128)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(RemoteTunnelOwners::InternalEndpoint)
                            .string_len(512)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(RemoteTunnelOwners::FencingToken)
                            .string_len(128)
                            .not_null(),
                    )
                    .col(
                        crate::time::utc_date_time_column(
                            manager,
                            RemoteTunnelOwners::LeaseExpiresAt,
                        )
                        .not_null(),
                    )
                    .col(
                        crate::time::utc_date_time_column(manager, RemoteTunnelOwners::UpdatedAt)
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_remote_tunnel_owners_remote_node")
                            .from(RemoteTunnelOwners::Table, RemoteTunnelOwners::RemoteNodeId)
                            .to(ManagedFollowers::Table, ManagedFollowers::Id)
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
                    .table(RemoteTunnelOwners::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum RemoteTunnelOwners {
    Table,
    RemoteNodeId,
    RuntimeId,
    InternalEndpoint,
    FencingToken,
    LeaseExpiresAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum ManagedFollowers {
    Table,
    Id,
}
