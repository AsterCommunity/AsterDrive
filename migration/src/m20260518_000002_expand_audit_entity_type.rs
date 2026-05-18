//! 数据库迁移：收紧审计日志 entity_type，并放宽长度以兼容外部认证等较长实体名。

use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::DbBackend;

#[derive(DeriveMigrationName)]
pub struct Migration;

const OLD_ENTITY_TYPE_LEN: u32 = 16;
const NEW_ENTITY_TYPE_LEN: u32 = 64;
const SQLITE_TEMP_AUDIT_LOGS_TABLE: &str = "audit_logs__entity_type_resize";
const LEGACY_EMPTY_ENTITY_TYPE: &str = "user";

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        backfill_missing_entity_types(manager).await?;
        alter_entity_type_column(manager, NEW_ENTITY_TYPE_LEN, false).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        alter_entity_type_column(manager, OLD_ENTITY_TYPE_LEN, true).await
    }
}

async fn alter_entity_type_column(
    manager: &SchemaManager<'_>,
    entity_type_len: u32,
    nullable: bool,
) -> Result<(), DbErr> {
    match manager.get_database_backend() {
        DbBackend::Sqlite => rebuild_sqlite_audit_logs(manager, entity_type_len, nullable).await,
        DbBackend::Postgres | DbBackend::MySql => {
            let mut entity_type = ColumnDef::new(AuditLogs::EntityType);
            entity_type.string_len(entity_type_len);
            if nullable {
                entity_type.null();
            } else {
                entity_type.not_null();
            }

            manager
                .alter_table(
                    Table::alter()
                        .table(AuditLogs::Table)
                        .modify_column(entity_type.to_owned())
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
    nullable: bool,
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
        nullable,
    )
    .await?;

    let mut copy_select = Query::select();
    copy_select
        .column(AuditLogs::Id)
        .column(AuditLogs::UserId)
        .column(AuditLogs::Action)
        .expr(Expr::col(AuditLogs::EntityType).if_null(LEGACY_EMPTY_ENTITY_TYPE))
        .column(AuditLogs::EntityId)
        .column(AuditLogs::EntityName)
        .column(AuditLogs::Details)
        .column(AuditLogs::IpAddress)
        .column(AuditLogs::UserAgent)
        .column(AuditLogs::CreatedAt)
        .from(AuditLogs::Table);

    let mut copy_insert = Query::insert();
    copy_insert
        .into_table(Alias::new(SQLITE_TEMP_AUDIT_LOGS_TABLE))
        .columns([
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
        ])
        .select_from(copy_select.to_owned())
        .map_err(|error| {
            DbErr::Migration(format!("failed to build audit log copy statement: {error}"))
        })?;
    manager.execute(copy_insert.to_owned()).await?;

    manager
        .drop_table(Table::drop().table(AuditLogs::Table).to_owned())
        .await?;

    manager
        .rename_table(
            Table::rename()
                .table(Alias::new(SQLITE_TEMP_AUDIT_LOGS_TABLE), AuditLogs::Table)
                .to_owned(),
        )
        .await?;

    create_audit_log_indexes(manager).await
}

async fn create_audit_logs_table<T>(
    manager: &SchemaManager<'_>,
    table: T,
    entity_type_len: u32,
    nullable: bool,
) -> Result<(), DbErr>
where
    T: IntoIden,
{
    let mut entity_type = ColumnDef::new(AuditLogs::EntityType);
    entity_type.string_len(entity_type_len);
    if nullable {
        entity_type.null();
    } else {
        entity_type.not_null();
    }

    manager
        .create_table(
            Table::create()
                .table(table)
                .col(big_integer_pk(AuditLogs::Id))
                .col(ColumnDef::new(AuditLogs::UserId).big_integer().not_null())
                .col(ColumnDef::new(AuditLogs::Action).string_len(64).not_null())
                .col(entity_type)
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

async fn backfill_missing_entity_types(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let entity_type = Expr::case(
        Expr::col(AuditLogs::Action).is_in(["user_login", "user_logout"]),
        "auth_session",
    )
    .case(
        Expr::col(AuditLogs::Action).is_in(["user_passkey_login"]),
        "passkey",
    )
    .case(
        Expr::col(AuditLogs::Action).is_in(["config_action_execute", "config_update"]),
        "system_config",
    )
    .case(
        Expr::col(AuditLogs::Action).is_in(["batch_copy", "batch_delete", "batch_move"]),
        "batch",
    )
    .case(
        Expr::col(AuditLogs::Action).is_in(["share_batch_delete", "share_create", "share_delete"]),
        "share",
    )
    .case(
        Expr::col(AuditLogs::Action).is_in([
            "system_setup",
            "user_change_password",
            "user_register",
        ]),
        LEGACY_EMPTY_ENTITY_TYPE,
    )
    .finally(LEGACY_EMPTY_ENTITY_TYPE);

    manager
        .execute(
            Query::update()
                .table(AuditLogs::Table)
                .value(AuditLogs::EntityType, entity_type)
                .and_where(Expr::col(AuditLogs::EntityType).is_null())
                .to_owned(),
        )
        .await?;
    Ok(())
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
