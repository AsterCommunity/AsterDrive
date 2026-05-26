//! 数据库迁移：新增登录邮件验证码表。

use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::DbBackend;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        create_mfa_email_codes(manager).await?;
        create_mfa_email_codes_single_active_index(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(MfaEmailCodes::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await
    }
}

async fn create_mfa_email_codes(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            Table::create()
                .table(MfaEmailCodes::Table)
                .if_not_exists()
                .col(big_integer_pk(MfaEmailCodes::Id))
                .col(
                    ColumnDef::new(MfaEmailCodes::FlowId)
                        .big_integer()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(MfaEmailCodes::UserId)
                        .big_integer()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(MfaEmailCodes::CodeHash)
                        .string_len(255)
                        .not_null(),
                )
                .col(
                    crate::time::utc_date_time_column(manager, MfaEmailCodes::ExpiresAt).not_null(),
                )
                .col(crate::time::utc_date_time_column(manager, MfaEmailCodes::ConsumedAt).null())
                .col(
                    crate::time::utc_date_time_column(manager, MfaEmailCodes::CreatedAt).not_null(),
                )
                .foreign_key(
                    ForeignKey::create()
                        .from(MfaEmailCodes::Table, MfaEmailCodes::FlowId)
                        .to(MfaLoginFlows::Table, MfaLoginFlows::Id)
                        .on_delete(ForeignKeyAction::Cascade),
                )
                .foreign_key(
                    ForeignKey::create()
                        .from(MfaEmailCodes::Table, MfaEmailCodes::UserId)
                        .to(Users::Table, Users::Id)
                        .on_delete(ForeignKeyAction::Cascade),
                )
                .to_owned(),
        )
        .await?;

    for index in [
        Index::create()
            .name("idx_mfa_email_codes_flow_id")
            .table(MfaEmailCodes::Table)
            .col(MfaEmailCodes::FlowId)
            .to_owned(),
        Index::create()
            .name("idx_mfa_email_codes_user_created")
            .table(MfaEmailCodes::Table)
            .col(MfaEmailCodes::UserId)
            .col(MfaEmailCodes::CreatedAt)
            .to_owned(),
        Index::create()
            .name("idx_mfa_email_codes_expires_at")
            .table(MfaEmailCodes::Table)
            .col(MfaEmailCodes::ExpiresAt)
            .to_owned(),
    ] {
        manager.create_index(index).await?;
    }

    Ok(())
}

async fn create_mfa_email_codes_single_active_index(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    let statement = match manager.get_database_backend() {
        DbBackend::Sqlite | DbBackend::Postgres => {
            "CREATE UNIQUE INDEX idx_mfa_email_codes_single_active \
             ON mfa_email_codes ( \
                user_id, \
                (CASE WHEN consumed_at IS NULL THEN 1 ELSE NULL END) \
             );"
        }
        DbBackend::MySql => {
            "CREATE UNIQUE INDEX idx_mfa_email_codes_single_active \
             ON mfa_email_codes ( \
                user_id, \
                ((CASE WHEN consumed_at IS NULL THEN 1 ELSE NULL END)) \
             );"
        }
        backend => {
            return Err(DbErr::Migration(format!(
                "unsupported database backend for MFA email code active index: {backend:?}"
            )));
        }
    };

    manager
        .get_connection()
        .execute_unprepared(statement)
        .await
        .map(|_| ())
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
enum Users {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum MfaLoginFlows {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum MfaEmailCodes {
    Table,
    Id,
    FlowId,
    UserId,
    CodeHash,
    ExpiresAt,
    ConsumedAt,
    CreatedAt,
}
