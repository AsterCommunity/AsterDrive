//! 数据库迁移：放宽审计日志 entity_type 长度，兼容外部认证等较长实体名。

use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::{ConnectionTrait, DbBackend};

#[derive(DeriveMigrationName)]
pub struct Migration;

const OLD_ENTITY_TYPE_LEN: u32 = 16;
const NEW_ENTITY_TYPE_LEN: u32 = 64;
const SQLITE_TEMP_AUDIT_LOGS_TABLE: &str = "audit_logs__entity_type_resize";

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        alter_entity_type_len(manager, NEW_ENTITY_TYPE_LEN).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        alter_entity_type_len(manager, OLD_ENTITY_TYPE_LEN).await
    }
}

async fn alter_entity_type_len(
    manager: &SchemaManager<'_>,
    entity_type_len: u32,
) -> Result<(), DbErr> {
    match manager.get_database_backend() {
        DbBackend::Sqlite => rebuild_sqlite_audit_logs(manager, entity_type_len).await,
        DbBackend::Postgres | DbBackend::MySql => {
            manager
                .alter_table(
                    Table::alter()
                        .table(AuditLogs::Table)
                        .modify_column(
                            ColumnDef::new(AuditLogs::EntityType)
                                .string_len(entity_type_len)
                                .null()
                                .to_owned(),
                        )
                        .to_owned(),
                )
                .await
        }
        backend => Err(DbErr::Migration(format!(
            "unsupported database backend for audit log entity_type migration: {backend:?}"
        ))),
    }
}

async fn rebuild_sqlite_audit_logs(
    manager: &SchemaManager<'_>,
    entity_type_len: u32,
) -> Result<(), DbErr> {
    manager
        .drop_table(
            Table::drop()
                .table(Alias::new(SQLITE_TEMP_AUDIT_LOGS_TABLE))
                .if_exists()
                .to_owned(),
        )
        .await?;

    create_audit_logs_table(
        manager,
        Alias::new(SQLITE_TEMP_AUDIT_LOGS_TABLE),
        entity_type_len,
    )
    .await?;

    manager
        .get_connection()
        .execute_unprepared(&format!(
            "INSERT INTO \"{temp}\" (\
                \"id\", \
                \"user_id\", \
                \"action\", \
                \"entity_type\", \
                \"entity_id\", \
                \"entity_name\", \
                \"details\", \
                \"ip_address\", \
                \"user_agent\", \
                \"created_at\"\
             ) \
             SELECT \
                \"id\", \
                \"user_id\", \
                \"action\", \
                \"entity_type\", \
                \"entity_id\", \
                \"entity_name\", \
                \"details\", \
                \"ip_address\", \
                \"user_agent\", \
                \"created_at\" \
             FROM \"audit_logs\"",
            temp = SQLITE_TEMP_AUDIT_LOGS_TABLE
        ))
        .await?;

    manager
        .drop_table(Table::drop().table(AuditLogs::Table).to_owned())
        .await?;

    manager
        .get_connection()
        .execute_unprepared(&format!(
            "ALTER TABLE \"{temp}\" RENAME TO \"audit_logs\"",
            temp = SQLITE_TEMP_AUDIT_LOGS_TABLE
        ))
        .await?;

    create_audit_log_indexes(manager).await
}

async fn create_audit_logs_table<T>(
    manager: &SchemaManager<'_>,
    table: T,
    entity_type_len: u32,
) -> Result<(), DbErr>
where
    T: IntoIden,
{
    manager
        .create_table(
            Table::create()
                .table(table)
                .col(big_integer_pk(AuditLogs::Id))
                .col(ColumnDef::new(AuditLogs::UserId).big_integer().not_null())
                .col(ColumnDef::new(AuditLogs::Action).string_len(64).not_null())
                .col(
                    ColumnDef::new(AuditLogs::EntityType)
                        .string_len(entity_type_len)
                        .null(),
                )
                .col(ColumnDef::new(AuditLogs::EntityId).big_integer().null())
                .col(ColumnDef::new(AuditLogs::EntityName).string_len(255).null())
                .col(ColumnDef::new(AuditLogs::Details).text().null())
                .col(ColumnDef::new(AuditLogs::IpAddress).string_len(45).null())
                .col(ColumnDef::new(AuditLogs::UserAgent).string_len(512).null())
                .col(crate::time::utc_date_time_column(manager, AuditLogs::CreatedAt).not_null())
                .to_owned(),
        )
        .await
}

async fn create_audit_log_indexes(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    for index in [
        Index::create()
            .name("idx_audit_logs_user_id")
            .table(AuditLogs::Table)
            .col(AuditLogs::UserId)
            .to_owned(),
        Index::create()
            .name("idx_audit_logs_action")
            .table(AuditLogs::Table)
            .col(AuditLogs::Action)
            .to_owned(),
        Index::create()
            .name("idx_audit_logs_created_at")
            .table(AuditLogs::Table)
            .col(AuditLogs::CreatedAt)
            .to_owned(),
        Index::create()
            .name("idx_audit_logs_entity")
            .table(AuditLogs::Table)
            .col(AuditLogs::EntityType)
            .col(AuditLogs::EntityId)
            .to_owned(),
    ] {
        manager.create_index(index).await?;
    }

    Ok(())
}

fn big_integer_pk<T>(column: T) -> ColumnDef
where
    T: IntoIden,
{
    let mut column = ColumnDef::new(column);
    column
        .big_integer()
        .not_null()
        .auto_increment()
        .primary_key();
    column
}

#[derive(DeriveIden)]
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
