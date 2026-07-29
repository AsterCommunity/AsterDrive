//! Align the historical product mail-outbox table with Forge's shared schema contract.

use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::DatabaseBackend;

#[derive(DeriveMigrationName)]
pub struct Migration;

const SQLITE_LEGACY_TABLE: &str = "mail_outbox__legacy_forge_contract";

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        match manager.get_database_backend() {
            DatabaseBackend::Sqlite => rebuild_sqlite_with_forge_schema(manager).await,
            DatabaseBackend::MySql | DatabaseBackend::Postgres => {
                alter_template_code_width(manager, 64).await
            }
            backend => Err(DbErr::Migration(format!(
                "unsupported database backend for Forge mail-outbox migration: {backend:?}"
            ))),
        }
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        match manager.get_database_backend() {
            DatabaseBackend::Sqlite => Ok(()),
            DatabaseBackend::MySql | DatabaseBackend::Postgres => {
                alter_template_code_width(manager, 32).await
            }
            backend => Err(DbErr::Migration(format!(
                "unsupported database backend for Forge mail-outbox rollback: {backend:?}"
            ))),
        }
    }
}

async fn alter_template_code_width(manager: &SchemaManager<'_>, width: u32) -> Result<(), DbErr> {
    manager
        .alter_table(
            Table::alter()
                .table(MailOutbox::Table)
                .modify_column(
                    ColumnDef::new(MailOutbox::TemplateCode)
                        .string_len(width)
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
                .table(MailOutbox::Table, Alias::new(SQLITE_LEGACY_TABLE))
                .to_owned(),
        )
        .await?;
    manager
        .create_table(aster_forge_db::create_mail_outbox_table(
            DatabaseBackend::Sqlite,
        ))
        .await?;

    let columns = [
        MailOutbox::Id,
        MailOutbox::TemplateCode,
        MailOutbox::ToAddress,
        MailOutbox::ToName,
        MailOutbox::PayloadJson,
        MailOutbox::Status,
        MailOutbox::AttemptCount,
        MailOutbox::NextAttemptAt,
        MailOutbox::ProcessingStartedAt,
        MailOutbox::SentAt,
        MailOutbox::LastError,
        MailOutbox::CreatedAt,
        MailOutbox::UpdatedAt,
    ];
    let mut select = Query::select();
    select
        .columns(columns)
        .from(Alias::new(SQLITE_LEGACY_TABLE));
    let mut insert = Query::insert();
    insert
        .into_table(MailOutbox::Table)
        .columns(columns)
        .select_from(select)
        .map_err(|error| {
            DbErr::Migration(format!(
                "failed to build Forge mail-outbox data copy: {error}"
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
        .create_index(aster_forge_db::create_mail_outbox_due_index())
        .await?;
    manager
        .create_index(aster_forge_db::create_mail_outbox_processing_index())
        .await?;
    manager
        .create_index(aster_forge_db::create_mail_outbox_sent_at_index())
        .await
}

#[derive(DeriveIden, Clone, Copy)]
enum MailOutbox {
    Table,
    Id,
    TemplateCode,
    ToAddress,
    ToName,
    PayloadJson,
    Status,
    AttemptCount,
    NextAttemptAt,
    ProcessingStartedAt,
    SentAt,
    LastError,
    CreatedAt,
    UpdatedAt,
}
