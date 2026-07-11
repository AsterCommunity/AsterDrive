//! Align the historical product system-config table with Forge's shared schema contract.

use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::DatabaseBackend;

#[derive(DeriveMigrationName)]
pub struct Migration;

const SQLITE_LEGACY_TABLE: &str = "system_config__legacy_forge_contract";
const PRODUCT_VISIBILITY_INDEX: &str = "idx_system_config_visibility";

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        match manager.get_database_backend() {
            DatabaseBackend::Sqlite => rebuild_sqlite_with_forge_schema(manager).await,
            DatabaseBackend::MySql | DatabaseBackend::Postgres => {
                align_variable_width_columns(manager).await?;
                manager
                    .create_index(aster_forge_db::create_system_config_key_unique_index())
                    .await
            }
            backend => Err(DbErr::Migration(format!(
                "unsupported database backend for Forge system-config migration: {backend:?}"
            ))),
        }
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        match manager.get_database_backend() {
            // SQLite accepts the Forge schema as a backwards-compatible superset in practice.
            DatabaseBackend::Sqlite => Ok(()),
            DatabaseBackend::MySql | DatabaseBackend::Postgres => {
                manager
                    .drop_index(aster_forge_db::drop_system_config_key_unique_index())
                    .await?;
                manager
                    .alter_table(
                        Table::alter()
                            .table(SystemConfig::Table)
                            .modify_column(
                                ColumnDef::new(SystemConfig::Namespace)
                                    .string_len(128)
                                    .not_null()
                                    .default(""),
                            )
                            .to_owned(),
                    )
                    .await?;
                manager
                    .alter_table(
                        Table::alter()
                            .table(SystemConfig::Table)
                            .modify_column(
                                ColumnDef::new(SystemConfig::Description)
                                    .text()
                                    .not_null()
                                    .default(""),
                            )
                            .to_owned(),
                    )
                    .await
            }
            backend => Err(DbErr::Migration(format!(
                "unsupported database backend for Forge system-config rollback: {backend:?}"
            ))),
        }
    }
}

async fn align_variable_width_columns(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .alter_table(
            Table::alter()
                .table(SystemConfig::Table)
                .modify_column(
                    ColumnDef::new(SystemConfig::Namespace)
                        .string_len(64)
                        .not_null()
                        .default(""),
                )
                .to_owned(),
        )
        .await?;
    manager
        .alter_table(
            Table::alter()
                .table(SystemConfig::Table)
                .modify_column(
                    ColumnDef::new(SystemConfig::Description)
                        .string_len(512)
                        .not_null(),
                )
                .to_owned(),
        )
        .await
}

async fn rebuild_sqlite_with_forge_schema(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .drop_table(
            Table::drop()
                .table(Alias::new(SQLITE_LEGACY_TABLE))
                .if_exists()
                .to_owned(),
        )
        .await?;
    manager
        .rename_table(
            Table::rename()
                .table(SystemConfig::Table, Alias::new(SQLITE_LEGACY_TABLE))
                .to_owned(),
        )
        .await?;
    manager
        .create_table(aster_forge_db::create_system_config_table(
            DatabaseBackend::Sqlite,
        ))
        .await?;

    let columns = [
        SystemConfig::Id,
        SystemConfig::Key,
        SystemConfig::Value,
        SystemConfig::ValueType,
        SystemConfig::RequiresRestart,
        SystemConfig::IsSensitive,
        SystemConfig::Source,
        SystemConfig::Visibility,
        SystemConfig::Namespace,
        SystemConfig::Category,
        SystemConfig::Description,
        SystemConfig::UpdatedAt,
        SystemConfig::UpdatedBy,
    ];
    let mut select = Query::select();
    select
        .columns(columns)
        .from(Alias::new(SQLITE_LEGACY_TABLE));
    let mut insert = Query::insert();
    insert
        .into_table(SystemConfig::Table)
        .columns(columns)
        .select_from(select)
        .map_err(|error| {
            DbErr::Migration(format!(
                "failed to build Forge system-config data copy: {error}"
            ))
        })?;
    manager.execute(insert).await?;
    manager
        .drop_table(
            Table::drop()
                .table(Alias::new(SQLITE_LEGACY_TABLE))
                .to_owned(),
        )
        .await?;
    manager
        .create_index(aster_forge_db::create_system_config_key_unique_index())
        .await?;
    manager
        .create_index(
            Index::create()
                .name(PRODUCT_VISIBILITY_INDEX)
                .table(SystemConfig::Table)
                .col(SystemConfig::Visibility)
                .to_owned(),
        )
        .await
}

#[derive(DeriveIden, Clone, Copy)]
enum SystemConfig {
    Table,
    Id,
    Key,
    Value,
    ValueType,
    RequiresRestart,
    IsSensitive,
    Source,
    Visibility,
    Namespace,
    Category,
    Description,
    UpdatedAt,
    UpdatedBy,
}
