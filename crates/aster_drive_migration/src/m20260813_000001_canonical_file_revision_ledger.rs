//! Replace mutable file history with an immutable canonical revision ledger.

use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::{
    ConnectionTrait, DbBackend, TransactionTrait, prelude::DateTimeUtc,
};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        create_revision_histories(manager).await?;
        create_revisions(manager).await?;
        create_revision_properties(manager).await?;
        create_indexes(manager).await?;
        backfill_ledger(manager).await?;
        reset_postgres_sequences(manager).await?;
        manager
            .drop_table(Table::drop().table(FileVersions::Table).to_owned())
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        create_legacy_file_versions(manager).await?;
        restore_legacy_history(manager).await?;
        manager
            .drop_table(
                Table::drop()
                    .table(FileRevisionProperties::Table)
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(Table::drop().table(FileRevisions::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(FileRevisionHistories::Table).to_owned())
            .await
    }
}

async fn reset_postgres_sequences(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    if manager.get_database_backend() != DbBackend::Postgres {
        return Ok(());
    }
    for table in ["file_revision_histories", "file_revisions"] {
        manager
            .get_connection()
            .execute_unprepared(&format!(
                "SELECT setval(pg_get_serial_sequence('{table}', 'id'), \
                 COALESCE((SELECT MAX(id) FROM {table}), 0) + 1, false)"
            ))
            .await?;
    }
    Ok(())
}

async fn create_revision_histories(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            Table::create()
                .table(FileRevisionHistories::Table)
                .col(aster_forge_db_migration::big_integer_primary_key(
                    FileRevisionHistories::Id,
                ))
                .col(
                    ColumnDef::new(FileRevisionHistories::PublicId)
                        .string_len(36)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(FileRevisionHistories::FileId)
                        .big_integer()
                        .null(),
                )
                .col(
                    ColumnDef::new(FileRevisionHistories::CurrentRevisionId)
                        .big_integer()
                        .null(),
                )
                .col(
                    ColumnDef::new(FileRevisionHistories::NextSequence)
                        .big_integer()
                        .not_null(),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(
                        manager,
                        FileRevisionHistories::DeltavControlledAt,
                    )
                    .null(),
                )
                .col(
                    ColumnDef::new(FileRevisionHistories::DeltavRootRevisionId)
                        .big_integer()
                        .null(),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(
                        manager,
                        FileRevisionHistories::CreatedAt,
                    )
                    .not_null(),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(
                        manager,
                        FileRevisionHistories::RetiredAt,
                    )
                    .null(),
                )
                .foreign_key(
                    ForeignKey::create()
                        .name("fk_file_revision_histories_file")
                        .from(FileRevisionHistories::Table, FileRevisionHistories::FileId)
                        .to(Files::Table, Files::Id)
                        .on_delete(ForeignKeyAction::Restrict),
                )
                .to_owned(),
        )
        .await
}

async fn create_revisions(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            Table::create()
                .table(FileRevisions::Table)
                .col(aster_forge_db_migration::big_integer_primary_key(
                    FileRevisions::Id,
                ))
                .col(
                    ColumnDef::new(FileRevisions::PublicId)
                        .string_len(36)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(FileRevisions::HistoryId)
                        .big_integer()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(FileRevisions::Sequence)
                        .big_integer()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(FileRevisions::PredecessorRevisionId)
                        .big_integer()
                        .null(),
                )
                .col(ColumnDef::new(FileRevisions::BlobId).big_integer().null())
                .col(
                    ColumnDef::new(FileRevisions::LogicalSize)
                        .big_integer()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(FileRevisions::MimeType)
                        .string_len(128)
                        .null(),
                )
                .col(
                    ColumnDef::new(FileRevisions::Etag)
                        .string_len(64)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(FileRevisions::ContentSha256)
                        .string_len(64)
                        .null(),
                )
                .col(
                    ColumnDef::new(FileRevisions::CreatorUserId)
                        .big_integer()
                        .null(),
                )
                .col(
                    ColumnDef::new(FileRevisions::CreatorDisplayName)
                        .string_len(255)
                        .null(),
                )
                .col(ColumnDef::new(FileRevisions::Comment).text().null())
                .col(
                    ColumnDef::new(FileRevisions::Reason)
                        .string_len(32)
                        .not_null(),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(
                        manager,
                        FileRevisions::CreatedAt,
                    )
                    .not_null(),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(
                        manager,
                        FileRevisions::RetiredAt,
                    )
                    .null(),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(
                        manager,
                        FileRevisions::PurgedAt,
                    )
                    .null(),
                )
                .foreign_key(
                    ForeignKey::create()
                        .name("fk_file_revisions_history")
                        .from(FileRevisions::Table, FileRevisions::HistoryId)
                        .to(FileRevisionHistories::Table, FileRevisionHistories::Id)
                        .on_delete(ForeignKeyAction::Restrict),
                )
                .foreign_key(
                    ForeignKey::create()
                        .name("fk_file_revisions_blob")
                        .from(FileRevisions::Table, FileRevisions::BlobId)
                        .to(FileBlobs::Table, FileBlobs::Id)
                        .on_delete(ForeignKeyAction::Restrict),
                )
                .foreign_key(
                    ForeignKey::create()
                        .name("fk_file_revisions_creator")
                        .from(FileRevisions::Table, FileRevisions::CreatorUserId)
                        .to(Users::Table, Users::Id)
                        .on_delete(ForeignKeyAction::SetNull),
                )
                .to_owned(),
        )
        .await
}

async fn create_revision_properties(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            Table::create()
                .table(FileRevisionProperties::Table)
                .col(
                    ColumnDef::new(FileRevisionProperties::RevisionId)
                        .big_integer()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(FileRevisionProperties::Namespace)
                        .string_len(256)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(FileRevisionProperties::Name)
                        .string_len(256)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(FileRevisionProperties::XmlValue)
                        .text()
                        .null(),
                )
                .primary_key(
                    Index::create()
                        .col(FileRevisionProperties::RevisionId)
                        .col(FileRevisionProperties::Namespace)
                        .col(FileRevisionProperties::Name),
                )
                .foreign_key(
                    ForeignKey::create()
                        .name("fk_file_revision_properties_revision")
                        .from(
                            FileRevisionProperties::Table,
                            FileRevisionProperties::RevisionId,
                        )
                        .to(FileRevisions::Table, FileRevisions::Id)
                        .on_delete(ForeignKeyAction::Cascade),
                )
                .to_owned(),
        )
        .await
}

async fn create_indexes(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    for index in [
        Index::create()
            .name("uq_file_revision_histories_public_id")
            .table(FileRevisionHistories::Table)
            .col(FileRevisionHistories::PublicId)
            .unique()
            .to_owned(),
        Index::create()
            .name("uq_file_revision_histories_file_id")
            .table(FileRevisionHistories::Table)
            .col(FileRevisionHistories::FileId)
            .unique()
            .to_owned(),
        Index::create()
            .name("uq_file_revisions_public_id")
            .table(FileRevisions::Table)
            .col(FileRevisions::PublicId)
            .unique()
            .to_owned(),
        Index::create()
            .name("uq_file_revisions_history_sequence")
            .table(FileRevisions::Table)
            .col(FileRevisions::HistoryId)
            .col(FileRevisions::Sequence)
            .unique()
            .to_owned(),
        Index::create()
            .name("idx_file_revisions_history_created_id")
            .table(FileRevisions::Table)
            .col(FileRevisions::HistoryId)
            .col(FileRevisions::CreatedAt)
            .col(FileRevisions::Id)
            .to_owned(),
        Index::create()
            .name("idx_file_revisions_blob_id")
            .table(FileRevisions::Table)
            .col(FileRevisions::BlobId)
            .to_owned(),
    ] {
        manager.create_index(index).await?;
    }
    Ok(())
}

async fn backfill_ledger(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let db = manager.get_connection();
    let legacy_rows = load_legacy_revisions(db).await?;
    let files = load_files(db).await?;
    let mut next_revision_id = legacy_rows
        .iter()
        .map(|revision| revision.id)
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(|| DbErr::Migration("file revision id space exhausted".to_string()))?;
    let txn = db.begin().await?;

    for file in files {
        let history_id = file.id;
        let history_public_id = uuid::Uuid::new_v4().hyphenated().to_string();
        let mut predecessor = None;
        let mut sequence = 1_i64;

        insert_history(&txn, history_id, &history_public_id, &file).await?;
        for legacy in legacy_rows
            .iter()
            .filter(|revision| revision.file_id == file.id)
        {
            insert_revision(
                &txn,
                RevisionInsert {
                    id: legacy.id,
                    history_id,
                    sequence,
                    predecessor_revision_id: predecessor,
                    blob_id: legacy.blob_id,
                    logical_size: legacy.size,
                    mime_type: None,
                    creator_user_id: None,
                    creator_display_name: None,
                    reason: "migration_history",
                    created_at: legacy.created_at,
                },
            )
            .await?;
            predecessor = Some(legacy.id);
            sequence += 1;
        }

        let current_revision_id = next_revision_id;
        next_revision_id = next_revision_id
            .checked_add(1)
            .ok_or_else(|| DbErr::Migration("file revision id space exhausted".to_string()))?;
        insert_revision(
            &txn,
            RevisionInsert {
                id: current_revision_id,
                history_id,
                sequence,
                predecessor_revision_id: predecessor,
                blob_id: file.blob_id,
                logical_size: file.size,
                mime_type: Some(file.mime_type.clone()),
                creator_user_id: file.created_by_user_id,
                creator_display_name: Some(file.created_by_username.clone()),
                reason: "migration_current",
                created_at: file.updated_at,
            },
        )
        .await?;
        snapshot_live_user_properties(&txn, file.id, current_revision_id).await?;
        update_history_head(&txn, history_id, current_revision_id, sequence + 1).await?;
    }

    txn.commit().await
}

async fn load_legacy_revisions<C: ConnectionTrait>(db: &C) -> Result<Vec<LegacyRevision>, DbErr> {
    let mut select = Query::select();
    select
        .columns([
            FileVersions::Id,
            FileVersions::FileId,
            FileVersions::BlobId,
            FileVersions::Size,
            FileVersions::CreatedAt,
        ])
        .from(FileVersions::Table)
        .order_by(FileVersions::FileId, Order::Asc)
        .order_by(FileVersions::Version, Order::Asc)
        .order_by(FileVersions::Id, Order::Asc);
    db.query_all(&select)
        .await?
        .into_iter()
        .map(|row| {
            Ok(LegacyRevision {
                id: row.try_get_by_index(0)?,
                file_id: row.try_get_by_index(1)?,
                blob_id: row.try_get_by_index(2)?,
                size: row.try_get_by_index(3)?,
                created_at: row.try_get_by_index(4)?,
            })
        })
        .collect()
}

async fn load_files<C: ConnectionTrait>(db: &C) -> Result<Vec<LegacyFile>, DbErr> {
    let mut select = Query::select();
    select
        .columns([
            Files::Id,
            Files::BlobId,
            Files::Size,
            Files::MimeType,
            Files::CreatedByUserId,
            Files::CreatedByUsername,
            Files::CreatedAt,
            Files::UpdatedAt,
        ])
        .from(Files::Table)
        .order_by(Files::Id, Order::Asc);
    db.query_all(&select)
        .await?
        .into_iter()
        .map(|row| {
            Ok(LegacyFile {
                id: row.try_get_by_index(0)?,
                blob_id: row.try_get_by_index(1)?,
                size: row.try_get_by_index(2)?,
                mime_type: row.try_get_by_index(3)?,
                created_by_user_id: row.try_get_by_index(4)?,
                created_by_username: row.try_get_by_index(5)?,
                created_at: row.try_get_by_index(6)?,
                updated_at: row.try_get_by_index(7)?,
            })
        })
        .collect()
}

async fn insert_history<C: ConnectionTrait>(
    db: &C,
    history_id: i64,
    public_id: &str,
    file: &LegacyFile,
) -> Result<(), DbErr> {
    let mut insert = Query::insert();
    insert
        .into_table(FileRevisionHistories::Table)
        .columns([
            FileRevisionHistories::Id,
            FileRevisionHistories::PublicId,
            FileRevisionHistories::FileId,
            FileRevisionHistories::NextSequence,
            FileRevisionHistories::CreatedAt,
        ])
        .values_panic([
            history_id.into(),
            public_id.into(),
            file.id.into(),
            1_i64.into(),
            file.created_at.into(),
        ]);
    db.execute(&insert).await?;
    Ok(())
}

async fn insert_revision<C: ConnectionTrait>(
    db: &C,
    revision: RevisionInsert<'_>,
) -> Result<(), DbErr> {
    let public_id = uuid::Uuid::new_v4().hyphenated().to_string();
    let etag = uuid::Uuid::new_v4().simple().to_string();
    let mut insert = Query::insert();
    insert
        .into_table(FileRevisions::Table)
        .columns([
            FileRevisions::Id,
            FileRevisions::PublicId,
            FileRevisions::HistoryId,
            FileRevisions::Sequence,
            FileRevisions::PredecessorRevisionId,
            FileRevisions::BlobId,
            FileRevisions::LogicalSize,
            FileRevisions::MimeType,
            FileRevisions::Etag,
            FileRevisions::CreatorUserId,
            FileRevisions::CreatorDisplayName,
            FileRevisions::Reason,
            FileRevisions::CreatedAt,
        ])
        .values_panic([
            revision.id.into(),
            public_id.into(),
            revision.history_id.into(),
            revision.sequence.into(),
            revision.predecessor_revision_id.into(),
            revision.blob_id.into(),
            revision.logical_size.into(),
            revision.mime_type.into(),
            etag.into(),
            revision.creator_user_id.into(),
            revision.creator_display_name.into(),
            revision.reason.into(),
            revision.created_at.into(),
        ]);
    db.execute(&insert).await?;
    Ok(())
}

async fn snapshot_live_user_properties<C: ConnectionTrait>(
    db: &C,
    file_id: i64,
    revision_id: i64,
) -> Result<(), DbErr> {
    let mut select = Query::select();
    select
        .expr(Expr::val(revision_id))
        .column(EntityProperties::Namespace)
        .column(EntityProperties::Name)
        .column(EntityProperties::Value)
        .from(EntityProperties::Table)
        .and_where(Expr::col(EntityProperties::EntityType).eq("file"))
        .and_where(Expr::col(EntityProperties::EntityId).eq(file_id))
        .and_where(Expr::col(EntityProperties::Namespace).ne("DAV:"))
        .and_where(Expr::col(EntityProperties::Namespace).not_like("system.%"));
    let mut insert = Query::insert();
    insert
        .into_table(FileRevisionProperties::Table)
        .columns([
            FileRevisionProperties::RevisionId,
            FileRevisionProperties::Namespace,
            FileRevisionProperties::Name,
            FileRevisionProperties::XmlValue,
        ])
        .select_from(select)
        .map_err(|error| DbErr::Migration(error.to_string()))?;
    db.execute(&insert).await?;
    Ok(())
}

async fn update_history_head<C: ConnectionTrait>(
    db: &C,
    history_id: i64,
    current_revision_id: i64,
    next_sequence: i64,
) -> Result<(), DbErr> {
    let mut update = Query::update();
    update
        .table(FileRevisionHistories::Table)
        .values([
            (
                FileRevisionHistories::CurrentRevisionId,
                current_revision_id.into(),
            ),
            (FileRevisionHistories::NextSequence, next_sequence.into()),
        ])
        .and_where(Expr::col(FileRevisionHistories::Id).eq(history_id));
    db.execute(&update).await?;
    Ok(())
}

async fn create_legacy_file_versions(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            Table::create()
                .table(FileVersions::Table)
                .col(aster_forge_db_migration::big_integer_primary_key(
                    FileVersions::Id,
                ))
                .col(
                    ColumnDef::new(FileVersions::FileId)
                        .big_integer()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(FileVersions::BlobId)
                        .big_integer()
                        .not_null(),
                )
                .col(ColumnDef::new(FileVersions::Version).integer().not_null())
                .col(ColumnDef::new(FileVersions::Size).big_integer().not_null())
                .col(
                    aster_forge_db_migration::utc_date_time_column(
                        manager,
                        FileVersions::CreatedAt,
                    )
                    .not_null(),
                )
                .foreign_key(
                    ForeignKey::create()
                        .from(FileVersions::Table, FileVersions::BlobId)
                        .to(FileBlobs::Table, FileBlobs::Id)
                        .on_delete(ForeignKeyAction::Restrict),
                )
                .to_owned(),
        )
        .await?;
    manager
        .create_index(
            Index::create()
                .name("idx_file_versions_file_id")
                .table(FileVersions::Table)
                .col(FileVersions::FileId)
                .to_owned(),
        )
        .await?;
    manager
        .create_index(
            Index::create()
                .name("idx_file_versions_blob_id")
                .table(FileVersions::Table)
                .col(FileVersions::BlobId)
                .to_owned(),
        )
        .await
}

async fn restore_legacy_history(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let mut select = Query::select();
    select
        .column((FileRevisions::Table, FileRevisions::Id))
        .column((FileRevisionHistories::Table, FileRevisionHistories::FileId))
        .column((FileRevisions::Table, FileRevisions::BlobId))
        .column((FileRevisions::Table, FileRevisions::Sequence))
        .column((FileRevisions::Table, FileRevisions::LogicalSize))
        .column((FileRevisions::Table, FileRevisions::CreatedAt))
        .from(FileRevisions::Table)
        .inner_join(
            FileRevisionHistories::Table,
            Expr::col((FileRevisions::Table, FileRevisions::HistoryId))
                .equals((FileRevisionHistories::Table, FileRevisionHistories::Id)),
        )
        .and_where(
            Expr::col((FileRevisions::Table, FileRevisions::Id)).ne(Expr::col((
                FileRevisionHistories::Table,
                FileRevisionHistories::CurrentRevisionId,
            ))),
        )
        .and_where(Expr::col(FileRevisions::BlobId).is_not_null())
        .and_where(Expr::col(FileRevisionHistories::FileId).is_not_null());
    let mut insert = Query::insert();
    insert
        .into_table(FileVersions::Table)
        .columns([
            FileVersions::Id,
            FileVersions::FileId,
            FileVersions::BlobId,
            FileVersions::Version,
            FileVersions::Size,
            FileVersions::CreatedAt,
        ])
        .select_from(select)
        .map_err(|error| DbErr::Migration(error.to_string()))?;
    manager.execute(insert).await?;
    Ok(())
}

struct LegacyRevision {
    id: i64,
    file_id: i64,
    blob_id: i64,
    size: i64,
    created_at: DateTimeUtc,
}

struct LegacyFile {
    id: i64,
    blob_id: i64,
    size: i64,
    mime_type: String,
    created_by_user_id: Option<i64>,
    created_by_username: String,
    created_at: DateTimeUtc,
    updated_at: DateTimeUtc,
}

struct RevisionInsert<'a> {
    id: i64,
    history_id: i64,
    sequence: i64,
    predecessor_revision_id: Option<i64>,
    blob_id: i64,
    logical_size: i64,
    mime_type: Option<String>,
    creator_user_id: Option<i64>,
    creator_display_name: Option<String>,
    reason: &'a str,
    created_at: DateTimeUtc,
}

#[derive(DeriveIden)]
enum FileRevisionHistories {
    Table,
    Id,
    PublicId,
    FileId,
    CurrentRevisionId,
    NextSequence,
    DeltavControlledAt,
    DeltavRootRevisionId,
    CreatedAt,
    RetiredAt,
}

#[derive(DeriveIden)]
enum FileRevisions {
    Table,
    Id,
    PublicId,
    HistoryId,
    Sequence,
    PredecessorRevisionId,
    BlobId,
    LogicalSize,
    MimeType,
    Etag,
    ContentSha256,
    CreatorUserId,
    CreatorDisplayName,
    Comment,
    Reason,
    CreatedAt,
    RetiredAt,
    PurgedAt,
}

#[derive(DeriveIden)]
enum FileRevisionProperties {
    Table,
    RevisionId,
    Namespace,
    Name,
    XmlValue,
}

#[derive(DeriveIden)]
enum Files {
    Table,
    Id,
    BlobId,
    Size,
    MimeType,
    CreatedByUserId,
    CreatedByUsername,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum FileVersions {
    Table,
    Id,
    FileId,
    BlobId,
    Version,
    Size,
    CreatedAt,
}

#[derive(DeriveIden)]
enum FileBlobs {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum EntityProperties {
    Table,
    EntityType,
    EntityId,
    Namespace,
    Name,
    Value,
}
