//! 数据库迁移：为 remote node 增加 transport mode 与 tunnel 状态字段。

use sea_orm_migration::prelude::*;

const MANAGED_FOLLOWERS_TABLE: &str = "managed_followers";
const TRANSPORT_MODE_COLUMN: &str = "transport_mode";
const TUNNEL_LAST_ERROR_COLUMN: &str = "tunnel_last_error";
const TUNNEL_LAST_SEEN_AT_COLUMN: &str = "tunnel_last_seen_at";

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let backend = manager.get_database_backend();

        if !manager
            .has_column(MANAGED_FOLLOWERS_TABLE, TRANSPORT_MODE_COLUMN)
            .await?
        {
            manager
                .alter_table(
                    Table::alter()
                        .table(ManagedFollowers::Table)
                        .add_column(
                            ColumnDef::new(ManagedFollowers::TransportMode)
                                .string_len(32)
                                .not_null()
                                .default("direct"),
                        )
                        .to_owned(),
                )
                .await?;
        }

        if !manager
            .has_column(MANAGED_FOLLOWERS_TABLE, TUNNEL_LAST_ERROR_COLUMN)
            .await?
        {
            manager
                .alter_table(
                    Table::alter()
                        .table(ManagedFollowers::Table)
                        .add_column(text_not_null_for_backend(
                            backend,
                            ManagedFollowers::TunnelLastError,
                            Some(""),
                        ))
                        .to_owned(),
                )
                .await?;
        }

        if !manager
            .has_column(MANAGED_FOLLOWERS_TABLE, TUNNEL_LAST_SEEN_AT_COLUMN)
            .await?
        {
            manager
                .alter_table(
                    Table::alter()
                        .table(ManagedFollowers::Table)
                        .add_column(
                            aster_forge_db_migration::utc_date_time_column(
                                manager,
                                ManagedFollowers::TunnelLastSeenAt,
                            )
                            .null(),
                        )
                        .to_owned(),
                )
                .await?;
        }

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(ManagedFollowers::Table)
                    .drop_column(ManagedFollowers::TunnelLastSeenAt)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(ManagedFollowers::Table)
                    .drop_column(ManagedFollowers::TunnelLastError)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(ManagedFollowers::Table)
                    .drop_column(ManagedFollowers::TransportMode)
                    .to_owned(),
            )
            .await
    }
}

fn text_not_null_for_backend(
    backend: sea_orm_migration::sea_orm::DbBackend,
    column: impl Iden + 'static,
    default: Option<&str>,
) -> ColumnDef {
    let mut def = ColumnDef::new(column);
    match backend {
        sea_orm_migration::sea_orm::DbBackend::MySql => {
            def.string_len(1024);
        }
        sea_orm_migration::sea_orm::DbBackend::Postgres
        | sea_orm_migration::sea_orm::DbBackend::Sqlite
        | _ => {
            def.text();
        }
    }
    def.not_null();
    if let Some(default) = default {
        def.default(default);
    }
    def
}

#[derive(DeriveIden)]
enum ManagedFollowers {
    Table,
    TransportMode,
    TunnelLastError,
    TunnelLastSeenAt,
}
