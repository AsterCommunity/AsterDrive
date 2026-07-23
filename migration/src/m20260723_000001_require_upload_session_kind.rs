//! Require every upload session to carry an explicit data-plane kind.

use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::{ConnectionTrait, DbBackend, Statement};

#[derive(DeriveMigrationName)]
pub struct Migration;

const SQLITE_REBUILT_TABLE: &str = "upload_sessions__session_kind_rebuild";
const VALID_SESSION_KINDS: [&str; 9] = [
    "offset_staging",
    "stream_staging",
    "provider_relay_multipart",
    "provider_presigned_single",
    "provider_presigned_multipart",
    "remote_relay_multipart",
    "remote_presigned_single",
    "remote_presigned_multipart",
    "provider_direct_resumable",
];

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        reject_legacy_sessions(manager).await?;
        set_session_kind_nullability(manager, false).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        set_session_kind_nullability(manager, true).await
    }
}

async fn reject_legacy_sessions(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let valid_values = VALID_SESSION_KINDS
        .iter()
        .map(|value| format!("'{value}'"))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT COUNT(*) FROM upload_sessions \
         WHERE session_kind IS NULL OR session_kind NOT IN ({valid_values})"
    );
    let row = manager
        .get_connection()
        .query_one_raw(Statement::from_string(manager.get_database_backend(), sql))
        .await?
        .ok_or_else(|| DbErr::Migration("upload session validation returned no row".to_string()))?;
    let invalid_count = row.try_get_by_index::<i64>(0).map_err(|error| {
        DbErr::Migration(format!(
            "failed to decode invalid upload session count: {error}"
        ))
    })?;
    if invalid_count > 0 {
        return Err(DbErr::Migration(format!(
            "cannot require upload_sessions.session_kind: {invalid_count} legacy or invalid upload session(s) remain; remove them before upgrading to 0.5.0"
        )));
    }
    Ok(())
}

async fn set_session_kind_nullability(
    manager: &SchemaManager<'_>,
    nullable: bool,
) -> Result<(), DbErr> {
    match manager.get_database_backend() {
        DbBackend::Sqlite => rebuild_sqlite_upload_sessions(manager, nullable).await,
        DbBackend::Postgres | DbBackend::MySql => {
            let mut column = ColumnDef::new(UploadSessions::SessionKind);
            column.string_len(32);
            if nullable {
                column.null();
            } else {
                column.not_null();
            }
            manager
                .alter_table(
                    Table::alter()
                        .table(UploadSessions::Table)
                        .modify_column(column)
                        .to_owned(),
                )
                .await
        }
        backend => Err(DbErr::Migration(format!(
            "unsupported database backend for upload session kind migration: {backend:?}"
        ))),
    }
}

async fn rebuild_sqlite_upload_sessions(
    manager: &SchemaManager<'_>,
    nullable: bool,
) -> Result<(), DbErr> {
    let connection = manager.get_connection();
    connection
        .execute_unprepared("PRAGMA foreign_keys = OFF")
        .await?;

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
            .create_table(upload_sessions_table(
                Alias::new(SQLITE_REBUILT_TABLE),
                nullable,
            ))
            .await?;

        let columns = upload_session_columns();
        let mut select = Query::select();
        select.columns(columns).from(UploadSessions::Table);
        let mut insert = Query::insert();
        insert
            .into_table(Alias::new(SQLITE_REBUILT_TABLE))
            .columns(columns)
            .select_from(select)
            .map_err(|error| {
                DbErr::Migration(format!("failed to build upload session data copy: {error}"))
            })?;
        manager.execute(insert).await?;

        manager
            .drop_table(Table::drop().table(UploadSessions::Table).to_owned())
            .await?;
        manager
            .rename_table(
                Table::rename()
                    .table(Alias::new(SQLITE_REBUILT_TABLE), UploadSessions::Table)
                    .to_owned(),
            )
            .await?;
        create_upload_session_indexes(manager).await
    }
    .await;

    let restore_result = connection
        .execute_unprepared("PRAGMA foreign_keys = ON")
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
            "upload session table rebuild introduced {} foreign key violation(s)",
            violations.len()
        )));
    }
    Ok(())
}

fn upload_sessions_table<T>(table: T, nullable: bool) -> TableCreateStatement
where
    T: IntoIden,
{
    let table = table.into_iden();
    let mut session_kind = ColumnDef::new(UploadSessions::SessionKind);
    session_kind.string_len(32);
    if nullable {
        session_kind.null();
    } else {
        session_kind.not_null();
    }

    Table::create()
        .table(table.clone())
        .col(
            ColumnDef::new(UploadSessions::Id)
                .string_len(36)
                .not_null()
                .primary_key(),
        )
        .col(
            ColumnDef::new(UploadSessions::UserId)
                .big_integer()
                .not_null(),
        )
        .col(ColumnDef::new(UploadSessions::TeamId).big_integer().null())
        .col(
            ColumnDef::new(UploadSessions::FrontendClientId)
                .string_len(36)
                .null(),
        )
        .col(
            ColumnDef::new(UploadSessions::Filename)
                .string_len(255)
                .not_null(),
        )
        .col(
            ColumnDef::new(UploadSessions::TotalSize)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(UploadSessions::ChunkSize)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(UploadSessions::TotalChunks)
                .integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(UploadSessions::ReceivedCount)
                .integer()
                .not_null()
                .default(0),
        )
        .col(
            ColumnDef::new(UploadSessions::FolderId)
                .big_integer()
                .null(),
        )
        .col(
            ColumnDef::new(UploadSessions::PolicyId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(UploadSessions::Status)
                .string_len(16)
                .not_null()
                .default("uploading"),
        )
        .col(session_kind)
        .col(ColumnDef::new(UploadSessions::ObjectTempKey).text().null())
        .col(
            ColumnDef::new(UploadSessions::ObjectMultipartId)
                .text()
                .null(),
        )
        .col(
            ColumnDef::new(UploadSessions::ProviderSessionCiphertext)
                .text()
                .null(),
        )
        .col(ColumnDef::new(UploadSessions::FileId).big_integer().null())
        .col(ColumnDef::new(UploadSessions::CreatedAt).text().not_null())
        .col(ColumnDef::new(UploadSessions::ExpiresAt).text().not_null())
        .col(ColumnDef::new(UploadSessions::UpdatedAt).text().not_null())
        .foreign_key(
            ForeignKey::create()
                .from(table, UploadSessions::UserId)
                .to(Users::Table, Users::Id)
                .on_delete(ForeignKeyAction::Cascade),
        )
        .to_owned()
}

const fn upload_session_columns() -> [UploadSessions; 20] {
    [
        UploadSessions::Id,
        UploadSessions::UserId,
        UploadSessions::TeamId,
        UploadSessions::FrontendClientId,
        UploadSessions::Filename,
        UploadSessions::TotalSize,
        UploadSessions::ChunkSize,
        UploadSessions::TotalChunks,
        UploadSessions::ReceivedCount,
        UploadSessions::FolderId,
        UploadSessions::PolicyId,
        UploadSessions::Status,
        UploadSessions::SessionKind,
        UploadSessions::ObjectTempKey,
        UploadSessions::ObjectMultipartId,
        UploadSessions::ProviderSessionCiphertext,
        UploadSessions::FileId,
        UploadSessions::CreatedAt,
        UploadSessions::ExpiresAt,
        UploadSessions::UpdatedAt,
    ]
}

async fn create_upload_session_indexes(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    for index in [
        Index::create()
            .name("idx_upload_sessions_team_id")
            .table(UploadSessions::Table)
            .col(UploadSessions::TeamId)
            .to_owned(),
        Index::create()
            .name("idx_upload_sessions_status_expires_at")
            .table(UploadSessions::Table)
            .col(UploadSessions::Status)
            .col(UploadSessions::ExpiresAt)
            .to_owned(),
        Index::create()
            .name("idx_upload_sessions_frontend_client")
            .table(UploadSessions::Table)
            .col(UploadSessions::UserId)
            .col(UploadSessions::TeamId)
            .col(UploadSessions::FrontendClientId)
            .to_owned(),
    ] {
        manager.create_index(index).await?;
    }
    Ok(())
}

#[derive(DeriveIden, Clone, Copy)]
enum UploadSessions {
    Table,
    Id,
    UserId,
    TeamId,
    FrontendClientId,
    Filename,
    TotalSize,
    ChunkSize,
    TotalChunks,
    ReceivedCount,
    FolderId,
    PolicyId,
    Status,
    SessionKind,
    ObjectTempKey,
    ObjectMultipartId,
    ProviderSessionCiphertext,
    FileId,
    CreatedAt,
    ExpiresAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}
