//! Align the historical product audit table with Forge's shared schema contract.

use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::DatabaseBackend;

#[derive(DeriveMigrationName)]
pub struct Migration;

const LEGACY_IP_ADDRESS_LEN: u32 = 45;
const FORGE_IP_ADDRESS_LEN: u32 = 128;
const SQLITE_LEGACY_TABLE: &str = "audit_logs__legacy_forge_contract";
const PRODUCT_ENTITY_INDEX: &str = "idx_audit_logs_entity";

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        match manager.get_database_backend() {
            DatabaseBackend::Sqlite => rebuild_sqlite_with_forge_schema(manager).await,
            DatabaseBackend::MySql | DatabaseBackend::Postgres => {
                alter_columns(manager, FORGE_IP_ADDRESS_LEN, true).await
            }
            backend => Err(DbErr::Migration(format!(
                "unsupported database backend for Forge audit contract migration: {backend:?}"
            ))),
        }
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        match manager.get_database_backend() {
            // SQLite does not enforce VARCHAR lengths, and rebuilding the shared table back to
            // the legacy product shape would reintroduce copied schema definitions.
            DatabaseBackend::Sqlite => Ok(()),
            DatabaseBackend::MySql | DatabaseBackend::Postgres => {
                alter_columns(manager, LEGACY_IP_ADDRESS_LEN, false).await
            }
            backend => Err(DbErr::Migration(format!(
                "unsupported database backend for Forge audit contract rollback: {backend:?}"
            ))),
        }
    }
}

async fn alter_columns(
    manager: &SchemaManager<'_>,
    ip_address_len: u32,
    system_user_default: bool,
) -> Result<(), DbErr> {
    let mut user_id = ColumnDef::new(AuditLogs::UserId);
    user_id.big_integer().not_null();
    if system_user_default {
        user_id.default(0);
    }
    manager
        .alter_table(
            Table::alter()
                .table(AuditLogs::Table)
                .modify_column(user_id)
                .to_owned(),
        )
        .await?;
    manager
        .alter_table(
            Table::alter()
                .table(AuditLogs::Table)
                .modify_column(
                    ColumnDef::new(AuditLogs::IpAddress)
                        .string_len(ip_address_len)
                        .null(),
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
                .table(AuditLogs::Table, Alias::new(SQLITE_LEGACY_TABLE))
                .to_owned(),
        )
        .await?;
    manager
        .create_table(aster_forge_db::create_audit_logs_table(
            DatabaseBackend::Sqlite,
        ))
        .await?;

    let columns = [
        AuditLogs::Id,
        AuditLogs::UserId,
        AuditLogs::Action,
        AuditLogs::EntityType,
        AuditLogs::EntityId,
        AuditLogs::EntityName,
        AuditLogs::Details,
        AuditLogs::IpAddress,
        AuditLogs::UserAgent,
        AuditLogs::CreatedAt,
    ];
    let mut select = Query::select();
    select
        .columns(columns)
        .from(Alias::new(SQLITE_LEGACY_TABLE));
    let mut insert = Query::insert();
    insert
        .into_table(AuditLogs::Table)
        .columns(columns)
        .select_from(select)
        .map_err(|error| {
            DbErr::Migration(format!("failed to build Forge audit data copy: {error}"))
        })?;
    manager.execute(insert).await?;
    manager
        .drop_table(
            Table::drop()
                .table(Alias::new(SQLITE_LEGACY_TABLE))
                .to_owned(),
        )
        .await?;

    for index in aster_forge_db::create_audit_logs_base_indexes() {
        manager.create_index(index).await?;
    }
    manager
        .create_index(
            Index::create()
                .name(PRODUCT_ENTITY_INDEX)
                .table(AuditLogs::Table)
                .col(AuditLogs::EntityType)
                .col(AuditLogs::EntityId)
                .to_owned(),
        )
        .await
}

#[derive(DeriveIden, Clone, Copy)]
enum AuditLogs {
    Table,
    Id,
    UserId,
    Action,
    EntityType,
    EntityId,
    EntityName,
    Details,
    IpAddress,
    UserAgent,
    CreatedAt,
}
