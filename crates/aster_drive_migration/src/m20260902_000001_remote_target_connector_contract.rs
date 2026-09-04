//! Introduce connector-owned remote-target configuration and credentials.
//!
//! The application startup conversion from the 0.5.0 flattened columns is
//! intentionally kept outside schema migration so encryption can use the
//! configured master key and one transaction can cover every target.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(RemoteStorageTargets::Table)
                    .add_column(
                        ColumnDef::new(RemoteStorageTargets::ConnectorId)
                            .string_len(128)
                            .null(),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(RemoteStorageTargets::Table)
                    .add_column(
                        ColumnDef::new(RemoteStorageTargets::ConnectorConfig)
                            .text()
                            .null(),
                    )
                    .to_owned(),
            )
            .await?;

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
        manager
            .alter_table(
                Table::alter()
                    .table(RemoteStorageTargets::Table)
                    .drop_column(RemoteStorageTargets::ConnectorConfig)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(RemoteStorageTargets::Table)
                    .drop_column(RemoteStorageTargets::ConnectorId)
                    .to_owned(),
            )
            .await
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
