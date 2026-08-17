//! Add connector-owned configuration and encrypted credentials for follower targets.

use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::DbBackend;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if !manager
            .has_column(
                RemoteStorageTargets::Table.to_string(),
                RemoteStorageTargets::ConnectorId.to_string(),
            )
            .await?
        {
            manager
                .alter_table(
                    Table::alter()
                        .table(RemoteStorageTargets::Table)
                        .add_column(
                            ColumnDef::new(RemoteStorageTargets::ConnectorId)
                                .string_len(128)
                                .not_null()
                                .default(""),
                        )
                        .to_owned(),
                )
                .await?;
        }
        if !manager
            .has_column(
                RemoteStorageTargets::Table.to_string(),
                RemoteStorageTargets::ConnectorConfig.to_string(),
            )
            .await?
        {
            let mut connector_config = ColumnDef::new(RemoteStorageTargets::ConnectorConfig);
            connector_config.text();
            if manager.get_database_backend() == DbBackend::MySql {
                connector_config.null();
            } else {
                connector_config.not_null().default("");
            }
            manager
                .alter_table(
                    Table::alter()
                        .table(RemoteStorageTargets::Table)
                        .add_column(&mut connector_config)
                        .to_owned(),
                )
                .await?;
        }
        if manager.get_database_backend() == DbBackend::MySql {
            manager
                .get_connection()
                .execute(
                    Query::update()
                        .table(RemoteStorageTargets::Table)
                        .value(RemoteStorageTargets::ConnectorConfig, "")
                        .and_where(Expr::col(RemoteStorageTargets::ConnectorConfig).is_null()),
                )
                .await?;
            manager
                .alter_table(
                    Table::alter()
                        .table(RemoteStorageTargets::Table)
                        .modify_column(
                            ColumnDef::new(RemoteStorageTargets::ConnectorConfig)
                                .text()
                                .not_null(),
                        )
                        .to_owned(),
                )
                .await?;
        }

        manager
            .create_table(
                Table::create()
                    .table(RemoteStorageTargetCredentials::Table)
                    .if_not_exists()
                    .col(aster_forge_db_migration::big_integer_primary_key(
                        RemoteStorageTargetCredentials::Id,
                    ))
                    .col(
                        ColumnDef::new(RemoteStorageTargetCredentials::TargetId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(RemoteStorageTargetCredentials::ConnectorId)
                            .string_len(128)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(RemoteStorageTargetCredentials::SchemaVersion)
                            .integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(RemoteStorageTargetCredentials::Revision)
                            .big_integer()
                            .not_null()
                            .default(1),
                    )
                    .col(
                        ColumnDef::new(RemoteStorageTargetCredentials::Ciphertext)
                            .text()
                            .not_null(),
                    )
                    .col(
                        aster_forge_db_migration::utc_date_time_column(
                            manager,
                            RemoteStorageTargetCredentials::CreatedAt,
                        )
                        .not_null(),
                    )
                    .col(
                        aster_forge_db_migration::utc_date_time_column(
                            manager,
                            RemoteStorageTargetCredentials::UpdatedAt,
                        )
                        .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_remote_storage_target_credentials_target")
                            .from(
                                RemoteStorageTargetCredentials::Table,
                                RemoteStorageTargetCredentials::TargetId,
                            )
                            .to(RemoteStorageTargets::Table, RemoteStorageTargets::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_remote_storage_target_credentials_target")
                    .table(RemoteStorageTargetCredentials::Table)
                    .col(RemoteStorageTargetCredentials::TargetId)
                    .unique()
                    .if_not_exists()
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(RemoteStorageTargetCredentials::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        for column in [
            RemoteStorageTargets::ConnectorConfig,
            RemoteStorageTargets::ConnectorId,
        ] {
            if manager
                .has_column(RemoteStorageTargets::Table.to_string(), column.to_string())
                .await?
            {
                manager
                    .alter_table(
                        Table::alter()
                            .table(RemoteStorageTargets::Table)
                            .drop_column(column)
                            .to_owned(),
                    )
                    .await?;
            }
        }
        Ok(())
    }
}

#[derive(DeriveIden)]
enum RemoteStorageTargets {
    Table,
    Id,
    ConnectorId,
    ConnectorConfig,
}

#[derive(DeriveIden)]
enum RemoteStorageTargetCredentials {
    Table,
    Id,
    TargetId,
    ConnectorId,
    SchemaVersion,
    Revision,
    Ciphertext,
    CreatedAt,
    UpdatedAt,
}
