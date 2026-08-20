//! 数据库迁移：明确区分 remote-node probe 与 reverse-tunnel 运行态字段。

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // SQLite 对单条 ALTER TABLE 的支持最稳定；四个列逐一重命名并保留原值。
        rename_column(
            manager,
            ManagedFollowers::LastError,
            ManagedFollowers::LastProbeError,
        )
        .await?;
        rename_column(
            manager,
            ManagedFollowers::LastCheckedAt,
            ManagedFollowers::LastProbeAt,
        )
        .await?;
        rename_column(
            manager,
            ManagedFollowers::TunnelLastError,
            ManagedFollowers::TunnelRuntimeError,
        )
        .await?;
        rename_column(
            manager,
            ManagedFollowers::TunnelLastSeenAt,
            ManagedFollowers::TunnelLastHandshakeAt,
        )
        .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        rename_column(
            manager,
            ManagedFollowers::LastProbeError,
            ManagedFollowers::LastError,
        )
        .await?;
        rename_column(
            manager,
            ManagedFollowers::LastProbeAt,
            ManagedFollowers::LastCheckedAt,
        )
        .await?;
        rename_column(
            manager,
            ManagedFollowers::TunnelRuntimeError,
            ManagedFollowers::TunnelLastError,
        )
        .await?;
        rename_column(
            manager,
            ManagedFollowers::TunnelLastHandshakeAt,
            ManagedFollowers::TunnelLastSeenAt,
        )
        .await
    }
}

async fn rename_column(
    manager: &SchemaManager<'_>,
    from: ManagedFollowers,
    to: ManagedFollowers,
) -> Result<(), DbErr> {
    manager
        .alter_table(
            Table::alter()
                .table(ManagedFollowers::Table)
                .rename_column(from, to)
                .to_owned(),
        )
        .await
}

#[derive(DeriveIden)]
enum ManagedFollowers {
    Table,
    LastError,
    LastCheckedAt,
    TunnelLastError,
    TunnelLastSeenAt,
    LastProbeError,
    LastProbeAt,
    TunnelRuntimeError,
    TunnelLastHandshakeAt,
}
