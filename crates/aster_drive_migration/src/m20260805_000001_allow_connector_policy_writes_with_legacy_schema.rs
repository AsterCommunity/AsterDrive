//! Keep the 0.5.x compatibility schema writable by connector-owned policy models.
//!
//! The connector refactor deliberately retains the legacy `storage_policies`
//! columns throughout 0.5.x so startup can import old credentials. The current
//! SeaORM entity no longer writes those columns. The historical `driver_type`
//! column lacks a default on every backend, and MySQL also omitted the legacy
//! `TEXT options` default for server-version compatibility. Give `driver_type`
//! an empty compatibility default and make only MySQL's deprecated `options`
//! column nullable instead of reintroducing legacy fields into the application
//! model.
//!
//! Issue #463 owns the 0.6.0 migration that physically removes all retained
//! columns, indexes, foreign keys, and deprecated credential tables.

use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::{ConnectionTrait, DbBackend, Statement};

#[derive(DeriveMigrationName)]
pub struct Migration;

const SQLITE_REBUILT_TABLE: &str = "storage_policies__legacy_driver_default_rebuild";
const UNCONFIGURED_CONNECTOR_ID: &str = "asterdrive.storage.unconfigured";
const UNCONFIGURED_STORAGE_CONFIG: &str = concat!(
    r#"{"format_version":1,"connector":{"format_version":1,"connector_id":"asterdrive.storage.unconfigured","schema_version":1,"values":{}},"#,
    r#""behavior":{"format_version":1,"schema_version":1,"values":{}}}"#,
);

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        set_legacy_driver_type_default(manager, true).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        set_legacy_driver_type_default(manager, false).await
    }
}

async fn set_legacy_driver_type_default(
    manager: &SchemaManager<'_>,
    has_default: bool,
) -> Result<(), DbErr> {
    // Some development snapshots already used the slim policy schema. Keep the
    // 0.5.x compatibility migration tolerant of that shape without recreating
    // a column that #463 will ultimately remove.
    if !manager
        .has_column(
            StoragePolicies::Table.to_string(),
            StoragePolicies::DriverType.to_string(),
        )
        .await?
    {
        return Ok(());
    }

    match manager.get_database_backend() {
        DbBackend::Sqlite => rebuild_sqlite_storage_policies(manager, has_default).await,
        DbBackend::Postgres => set_postgres_legacy_driver_type_default(manager, has_default).await,
        DbBackend::MySql => {
            modify_legacy_driver_type(manager, has_default).await?;
            set_mysql_legacy_options_nullable(manager, has_default).await
        }
        backend => Err(DbErr::Migration(format!(
            "unsupported database backend for storage policy compatibility migration: {backend:?}"
        ))),
    }
}

async fn set_postgres_legacy_driver_type_default(
    manager: &SchemaManager<'_>,
    has_default: bool,
) -> Result<(), DbErr> {
    let statement = if has_default {
        "ALTER TABLE \"storage_policies\" ALTER COLUMN \"driver_type\" SET DEFAULT ''"
    } else {
        "ALTER TABLE \"storage_policies\" ALTER COLUMN \"driver_type\" DROP DEFAULT"
    };
    manager
        .get_connection()
        .execute_unprepared(statement)
        .await
        .map(|_| ())
}

async fn modify_legacy_driver_type(
    manager: &SchemaManager<'_>,
    has_default: bool,
) -> Result<(), DbErr> {
    let mut driver_type = ColumnDef::new(StoragePolicies::DriverType);
    driver_type.string_len(32).not_null();
    if has_default {
        driver_type.default("");
    }
    manager
        .alter_table(
            Table::alter()
                .table(StoragePolicies::Table)
                .modify_column(driver_type)
                .to_owned(),
        )
        .await
}

async fn set_mysql_legacy_options_nullable(
    manager: &SchemaManager<'_>,
    nullable: bool,
) -> Result<(), DbErr> {
    if !nullable {
        manager
            .get_connection()
            .execute(
                &Query::update()
                    .table(StoragePolicies::Table)
                    .value(StoragePolicies::Options, "{}")
                    .and_where(Expr::col(StoragePolicies::Options).is_null())
                    .to_owned(),
            )
            .await?;
    }

    let mut options = ColumnDef::new(StoragePolicies::Options);
    options.text();
    if nullable {
        options.null();
    } else {
        options.not_null();
    }
    manager
        .alter_table(
            Table::alter()
                .table(StoragePolicies::Table)
                .modify_column(options)
                .to_owned(),
        )
        .await
}

async fn rebuild_sqlite_storage_policies(
    manager: &SchemaManager<'_>,
    driver_type_has_default: bool,
) -> Result<(), DbErr> {
    let connection = manager.get_connection();
    let foreign_keys_enabled = sqlite_foreign_keys_enabled(connection).await?;
    if foreign_keys_enabled {
        connection
            .execute_unprepared("PRAGMA foreign_keys = OFF")
            .await?;
    }

    let rebuild_result = async {
        manager
            .drop_table(
                Table::drop()
                    .table(Alias::new(SQLITE_REBUILT_TABLE))
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        manager
            .create_table(storage_policies_table(
                manager,
                Alias::new(SQLITE_REBUILT_TABLE),
                driver_type_has_default,
            ))
            .await?;

        let columns = storage_policy_columns();
        let mut select = Query::select();
        select.columns(columns).from(StoragePolicies::Table);
        let mut insert = Query::insert();
        insert
            .into_table(Alias::new(SQLITE_REBUILT_TABLE))
            .columns(columns)
            .select_from(select)
            .map_err(|error| {
                DbErr::Migration(format!(
                    "failed to build storage policy compatibility data copy: {error}"
                ))
            })?;
        manager.execute(insert).await?;

        manager
            .drop_table(Table::drop().table(StoragePolicies::Table).to_owned())
            .await?;
        manager
            .rename_table(
                Table::rename()
                    .table(Alias::new(SQLITE_REBUILT_TABLE), StoragePolicies::Table)
                    .to_owned(),
            )
            .await?;
        create_legacy_storage_policy_indexes(manager).await
    }
    .await;

    let restore_result = async {
        if foreign_keys_enabled {
            connection
                .execute_unprepared("PRAGMA foreign_keys = ON")
                .await?;
        }
        Ok::<(), DbErr>(())
    }
    .await;
    rebuild_result?;
    restore_result?;

    let violations = connection
        .query_all_raw(Statement::from_string(
            DbBackend::Sqlite,
            "PRAGMA foreign_key_check",
        ))
        .await?;
    if !violations.is_empty() {
        return Err(DbErr::Migration(format!(
            "storage policy compatibility rebuild introduced {} foreign key violation(s)",
            violations.len()
        )));
    }
    Ok(())
}

async fn sqlite_foreign_keys_enabled<C>(connection: &C) -> Result<bool, DbErr>
where
    C: ConnectionTrait,
{
    let row = connection
        .query_one_raw(Statement::from_string(
            DbBackend::Sqlite,
            "PRAGMA foreign_keys",
        ))
        .await?
        .ok_or_else(|| DbErr::Migration("SQLite foreign-key status returned no row".to_string()))?;
    Ok(row.try_get_by_index::<i32>(0)? != 0)
}

fn storage_policies_table<T>(
    manager: &SchemaManager<'_>,
    table: T,
    driver_type_has_default: bool,
) -> TableCreateStatement
where
    T: IntoIden,
{
    let table = table.into_iden();
    let mut driver_type = ColumnDef::new(StoragePolicies::DriverType);
    driver_type.string_len(32).not_null();
    if driver_type_has_default {
        driver_type.default("");
    }

    Table::create()
        .table(table)
        .col(aster_forge_db_migration::big_integer_primary_key(
            StoragePolicies::Id,
        ))
        .col(
            ColumnDef::new(StoragePolicies::Name)
                .string_len(128)
                .not_null(),
        )
        .col(driver_type)
        .col(
            ColumnDef::new(StoragePolicies::Endpoint)
                .string_len(512)
                .not_null()
                .default(""),
        )
        .col(
            ColumnDef::new(StoragePolicies::Bucket)
                .string_len(255)
                .not_null()
                .default(""),
        )
        .col(
            ColumnDef::new(StoragePolicies::AccessKey)
                .string_len(512)
                .not_null()
                .default(""),
        )
        .col(
            ColumnDef::new(StoragePolicies::SecretKey)
                .string_len(512)
                .not_null()
                .default(""),
        )
        .col(
            ColumnDef::new(StoragePolicies::BasePath)
                .string_len(512)
                .not_null()
                .default(""),
        )
        .col(
            ColumnDef::new(StoragePolicies::RemoteNodeId)
                .big_integer()
                .null(),
        )
        .col(
            ColumnDef::new(StoragePolicies::MaxFileSize)
                .big_integer()
                .not_null()
                .default(0),
        )
        .col(
            ColumnDef::new(StoragePolicies::AllowedTypes)
                .text()
                .not_null()
                .default("[]"),
        )
        .col(
            ColumnDef::new(StoragePolicies::Options)
                .text()
                .not_null()
                .default("{}"),
        )
        .col(
            ColumnDef::new(StoragePolicies::IsDefault)
                .boolean()
                .not_null()
                .default(false),
        )
        .col(
            ColumnDef::new(StoragePolicies::ChunkSize)
                .big_integer()
                .not_null()
                .default(5_242_880i64),
        )
        .col(
            aster_forge_db_migration::utc_date_time_column(manager, StoragePolicies::CreatedAt)
                .not_null(),
        )
        .col(
            aster_forge_db_migration::utc_date_time_column(manager, StoragePolicies::UpdatedAt)
                .not_null(),
        )
        .col(
            ColumnDef::new(StoragePolicies::RemoteStorageTargetKey)
                .string_len(255)
                .null(),
        )
        .col(
            ColumnDef::new(StoragePolicies::ConnectorId)
                .string_len(128)
                .not_null()
                .default(UNCONFIGURED_CONNECTOR_ID),
        )
        .col(
            ColumnDef::new(StoragePolicies::StorageConfig)
                .text()
                .not_null()
                .default(UNCONFIGURED_STORAGE_CONFIG),
        )
        .to_owned()
}

const fn storage_policy_columns() -> [StoragePolicies; 19] {
    [
        StoragePolicies::Id,
        StoragePolicies::Name,
        StoragePolicies::DriverType,
        StoragePolicies::Endpoint,
        StoragePolicies::Bucket,
        StoragePolicies::AccessKey,
        StoragePolicies::SecretKey,
        StoragePolicies::BasePath,
        StoragePolicies::RemoteNodeId,
        StoragePolicies::MaxFileSize,
        StoragePolicies::AllowedTypes,
        StoragePolicies::Options,
        StoragePolicies::IsDefault,
        StoragePolicies::ChunkSize,
        StoragePolicies::CreatedAt,
        StoragePolicies::UpdatedAt,
        StoragePolicies::RemoteStorageTargetKey,
        StoragePolicies::ConnectorId,
        StoragePolicies::StorageConfig,
    ]
}

async fn create_legacy_storage_policy_indexes(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    for index in [
        Index::create()
            .name("idx_storage_policies_remote_node_id")
            .table(StoragePolicies::Table)
            .col(StoragePolicies::RemoteNodeId)
            .to_owned(),
        Index::create()
            .name("idx_storage_policies_remote_target")
            .table(StoragePolicies::Table)
            .col(StoragePolicies::RemoteNodeId)
            .col(StoragePolicies::RemoteStorageTargetKey)
            .to_owned(),
    ] {
        manager.create_index(index).await?;
    }
    Ok(())
}

#[derive(DeriveIden, Clone, Copy)]
enum StoragePolicies {
    Table,
    Id,
    Name,
    DriverType,
    Endpoint,
    Bucket,
    AccessKey,
    SecretKey,
    BasePath,
    RemoteNodeId,
    MaxFileSize,
    AllowedTypes,
    Options,
    IsDefault,
    ChunkSize,
    CreatedAt,
    UpdatedAt,
    RemoteStorageTargetKey,
    ConnectorId,
    StorageConfig,
}
