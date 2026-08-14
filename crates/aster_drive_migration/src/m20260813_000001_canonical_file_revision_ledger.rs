//! Replace mutable file history with an immutable canonical revision ledger.

use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::{
    ConnectionTrait, DbBackend, TransactionTrait, prelude::DateTimeUtc,
};

const FILE_BACKFILL_BATCH_SIZE: u64 = 500;
const LEGACY_REVISION_BATCH_SIZE: u64 = 1_000;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        create_revision_histories(manager).await?;
        create_revisions(manager).await?;
        create_revision_properties(manager).await?;
        create_indexes(manager).await?;
        configure_mysql_case_sensitive_property_namespaces(manager).await?;
        create_legacy_backfill_index(manager).await?;
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

async fn create_legacy_backfill_index(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_index(
            Index::create()
                .name("idx_file_versions_backfill_cursor")
                .table(FileVersions::Table)
                .col(FileVersions::FileId)
                .col(FileVersions::Version)
                .col(FileVersions::Id)
                .to_owned(),
        )
        .await
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
    let mut table = Table::create();
    table
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
        );
    if manager.get_database_backend() == DbBackend::MySql {
        table.collate("utf8mb4_bin");
    }
    manager.create_table(table.to_owned()).await
}

async fn configure_mysql_case_sensitive_property_namespaces(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    if manager.get_database_backend() != DbBackend::MySql {
        return Ok(());
    }

    // MySQL's default collation makes XML QName identity case-insensitive. Build binary
    // projections and the replacement unique index online so large property tables avoid COPY.
    // Each step probes current schema state so interrupted runs and down/re-up cycles converge.
    if !manager
        .has_column("entity_properties", "namespace_case_key")
        .await?
    {
        manager
            .get_connection()
            .execute_unprepared(
                "ALTER TABLE entity_properties \
                 ADD COLUMN namespace_case_key VARCHAR(256) CHARACTER SET utf8mb4 \
                 COLLATE utf8mb4_bin GENERATED ALWAYS AS (namespace) VIRTUAL INVISIBLE, \
                 ALGORITHM=INSTANT",
            )
            .await?;
    }
    if !manager
        .has_column("entity_properties", "name_case_key")
        .await?
    {
        manager
            .get_connection()
            .execute_unprepared(
                "ALTER TABLE entity_properties \
                 ADD COLUMN name_case_key VARCHAR(255) CHARACTER SET utf8mb4 \
                 COLLATE utf8mb4_bin GENERATED ALWAYS AS (name) VIRTUAL INVISIBLE, \
                 ALGORITHM=INSTANT",
            )
            .await?;
    }
    if !manager
        .has_index("entity_properties", "idx_entity_properties_namespace_case")
        .await?
    {
        manager
            .get_connection()
            .execute_unprepared(
                "ALTER TABLE entity_properties \
                 ADD UNIQUE INDEX idx_entity_properties_namespace_case \
                 (entity_type, entity_id, namespace_case_key, name_case_key), \
                 ALGORITHM=INPLACE, LOCK=NONE",
            )
            .await?;
    }
    if manager
        .has_index("entity_properties", "idx_entity_properties_unique")
        .await?
    {
        manager
            .get_connection()
            .execute_unprepared(
                "ALTER TABLE entity_properties \
                 DROP INDEX idx_entity_properties_unique, ALGORITHM=INPLACE, LOCK=NONE",
            )
            .await?;
    }
    if manager
        .has_index("entity_properties", "idx_entity_properties_namespace_case")
        .await?
    {
        manager
            .get_connection()
            .execute_unprepared(
                "ALTER TABLE entity_properties \
                 RENAME INDEX idx_entity_properties_namespace_case \
                 TO idx_entity_properties_unique, ALGORITHM=INPLACE, LOCK=NONE",
            )
            .await?;
    }
    Ok(())
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
    let mut next_revision_id = find_max_legacy_revision_id(db)
        .await?
        .checked_add(1)
        .ok_or_else(|| DbErr::Migration("file revision id space exhausted".to_string()))?;
    let txn = db.begin().await?;
    let mut after_file_id = None;

    loop {
        let files = load_files_page(&txn, after_file_id, FILE_BACKFILL_BATCH_SIZE).await?;
        if files.is_empty() {
            break;
        }
        after_file_id = files.last().map(|file| file.id);

        for file in files {
            let history_id = file.id;
            let history_public_id = uuid::Uuid::new_v4().hyphenated().to_string();
            let mut predecessor = None;
            let mut sequence = 1_i64;
            let mut legacy_cursor = None;

            insert_history(&txn, history_id, &history_public_id, &file).await?;
            loop {
                let legacy_rows = load_legacy_revisions_page(
                    &txn,
                    file.id,
                    legacy_cursor,
                    LEGACY_REVISION_BATCH_SIZE,
                )
                .await?;
                if legacy_rows.is_empty() {
                    break;
                }
                legacy_cursor = legacy_rows
                    .last()
                    .map(|revision| (revision.version, revision.id));

                for legacy in legacy_rows {
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
                    sequence = sequence.checked_add(1).ok_or_else(|| {
                        DbErr::Migration("file revision sequence space exhausted".to_string())
                    })?;
                }
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
            update_history_head(
                &txn,
                history_id,
                current_revision_id,
                sequence.checked_add(1).ok_or_else(|| {
                    DbErr::Migration("file revision sequence space exhausted".to_string())
                })?,
            )
            .await?;
        }
    }

    txn.commit().await
}

async fn find_max_legacy_revision_id<C: ConnectionTrait>(db: &C) -> Result<i64, DbErr> {
    let mut select = Query::select();
    select
        .column(FileVersions::Id)
        .from(FileVersions::Table)
        .order_by(FileVersions::Id, Order::Desc)
        .limit(1);
    Ok(db
        .query_one(&select)
        .await?
        .map(|row| row.try_get_by_index::<i64>(0))
        .transpose()?
        .unwrap_or(0))
}

async fn load_legacy_revisions_page<C: ConnectionTrait>(
    db: &C,
    file_id: i64,
    after: Option<(i32, i64)>,
    limit: u64,
) -> Result<Vec<LegacyRevision>, DbErr> {
    let mut select = Query::select();
    select
        .columns([
            FileVersions::Id,
            FileVersions::Version,
            FileVersions::BlobId,
            FileVersions::Size,
            FileVersions::CreatedAt,
        ])
        .from(FileVersions::Table)
        .and_where(Expr::col(FileVersions::FileId).eq(file_id));
    if let Some((version, id)) = after {
        select.and_where(
            Condition::any()
                .add(Expr::col(FileVersions::Version).gt(version))
                .add(
                    Condition::all()
                        .add(Expr::col(FileVersions::Version).eq(version))
                        .add(Expr::col(FileVersions::Id).gt(id)),
                )
                .into(),
        );
    }
    select
        .order_by(FileVersions::Version, Order::Asc)
        .order_by(FileVersions::Id, Order::Asc)
        .limit(limit);
    db.query_all(&select)
        .await?
        .into_iter()
        .map(|row| {
            Ok(LegacyRevision {
                id: row.try_get_by_index(0)?,
                version: row.try_get_by_index(1)?,
                blob_id: row.try_get_by_index(2)?,
                size: row.try_get_by_index(3)?,
                created_at: row.try_get_by_index(4)?,
            })
        })
        .collect()
}

async fn load_files_page<C: ConnectionTrait>(
    db: &C,
    after_id: Option<i64>,
    limit: u64,
) -> Result<Vec<LegacyFile>, DbErr> {
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
        .from(Files::Table);
    if let Some(id) = after_id {
        select.and_where(Expr::col(Files::Id).gt(id));
    }
    select.order_by(Files::Id, Order::Asc).limit(limit);
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
            Expr::col((
                FileRevisionHistories::Table,
                FileRevisionHistories::CurrentRevisionId,
            ))
            .is_null()
            .or(
                Expr::col((FileRevisions::Table, FileRevisions::Id)).ne(Expr::col((
                    FileRevisionHistories::Table,
                    FileRevisionHistories::CurrentRevisionId,
                ))),
            ),
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
    version: i32,
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

#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm_migration::sea_orm::{Database, DatabaseConnection};

    async fn pagination_fixture() -> DatabaseConnection {
        let db = Database::connect("sqlite::memory:")
            .await
            .expect("SQLite fixture should connect");
        db.execute_unprepared(
            "CREATE TABLE files (\
                id INTEGER PRIMARY KEY, blob_id INTEGER NOT NULL, size INTEGER NOT NULL, \
                mime_type TEXT NOT NULL, created_by_user_id INTEGER NULL, \
                created_by_username TEXT NOT NULL, created_at TEXT NOT NULL, \
                updated_at TEXT NOT NULL\
             ); \
             CREATE TABLE file_versions (\
                id INTEGER PRIMARY KEY, file_id INTEGER NOT NULL, blob_id INTEGER NOT NULL, \
                version INTEGER NOT NULL, size INTEGER NOT NULL, created_at TEXT NOT NULL\
             ); \
             CREATE INDEX idx_file_versions_backfill_cursor \
                ON file_versions (file_id, version, id);",
        )
        .await
        .expect("pagination fixture schema should apply");

        let files = (1..=501)
            .map(|id| {
                format!(
                    "({id}, 1, {id}, 'text/plain', NULL, 'migration', \
                     '2026-08-01T00:00:00Z', '2026-08-01T00:00:00Z')"
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        db.execute_unprepared(&format!(
            "INSERT INTO files (id, blob_id, size, mime_type, created_by_user_id, \
             created_by_username, created_at, updated_at) VALUES {files}"
        ))
        .await
        .expect("file pagination fixtures should insert");

        let revisions = (1..=1_001)
            .map(|id| {
                let version = (id + 1) / 2;
                format!("({id}, 1, 1, {version}, {id}, '2026-08-01T00:00:00Z')")
            })
            .collect::<Vec<_>>()
            .join(",");
        db.execute_unprepared(&format!(
            "INSERT INTO file_versions (id, file_id, blob_id, version, size, created_at) \
             VALUES {revisions}"
        ))
        .await
        .expect("revision pagination fixtures should insert");
        db
    }

    #[tokio::test]
    async fn backfill_readers_bound_memory_and_preserve_composite_cursor_order() {
        let db = pagination_fixture().await;

        let first_files = load_files_page(&db, None, FILE_BACKFILL_BATCH_SIZE)
            .await
            .expect("first file batch should load");
        assert_eq!(first_files.len(), 500);
        assert_eq!(first_files.first().map(|file| file.id), Some(1));
        assert_eq!(first_files.last().map(|file| file.id), Some(500));
        let final_files = load_files_page(
            &db,
            first_files.last().map(|file| file.id),
            FILE_BACKFILL_BATCH_SIZE,
        )
        .await
        .expect("final file batch should load");
        assert_eq!(final_files.len(), 1);
        assert_eq!(final_files[0].id, 501);

        let first_revisions = load_legacy_revisions_page(&db, 1, None, LEGACY_REVISION_BATCH_SIZE)
            .await
            .expect("first revision batch should load");
        assert_eq!(first_revisions.len(), 1_000);
        let cursor = first_revisions
            .last()
            .map(|revision| (revision.version, revision.id));
        assert_eq!(cursor, Some((500, 1_000)));
        let final_revisions =
            load_legacy_revisions_page(&db, 1, cursor, LEGACY_REVISION_BATCH_SIZE)
                .await
                .expect("final revision batch should load");
        assert_eq!(final_revisions.len(), 1);
        assert_eq!(final_revisions[0].id, 1_001);
        assert_eq!(final_revisions[0].version, 501);
    }

    #[tokio::test]
    async fn restore_legacy_history_keeps_rows_when_current_pointer_is_null() {
        let db = Database::connect("sqlite::memory:")
            .await
            .expect("SQLite fixture should connect");
        db.execute_unprepared(
            "CREATE TABLE file_blobs (id INTEGER PRIMARY KEY); \
             CREATE TABLE files (id INTEGER PRIMARY KEY); \
             CREATE TABLE file_revision_histories (id INTEGER PRIMARY KEY, file_id INTEGER, current_revision_id INTEGER); \
             CREATE TABLE file_revisions (id INTEGER PRIMARY KEY, history_id INTEGER NOT NULL, blob_id INTEGER, sequence INTEGER NOT NULL, logical_size INTEGER NOT NULL, created_at TEXT NOT NULL); \
             INSERT INTO file_blobs VALUES (11); \
             INSERT INTO files VALUES (7); \
             INSERT INTO file_revision_histories VALUES (7, 7, NULL); \
             INSERT INTO file_revisions VALUES (21, 7, 11, 1, 5, '2026-08-01T00:00:00Z');",
        )
        .await
        .expect("legacy restore fixture should insert");
        let manager = SchemaManager::new(&db);
        create_legacy_file_versions(&manager)
            .await
            .expect("legacy history table should create");
        restore_legacy_history(&manager)
            .await
            .expect("legacy history should restore");

        let row = db
            .query_one_raw(sea_orm_migration::sea_orm::Statement::from_string(
                DbBackend::Sqlite,
                "SELECT id, file_id, version FROM file_versions",
            ))
            .await
            .expect("legacy row should query")
            .expect("NULL current pointer must not filter the revision");
        assert_eq!(row.try_get_by_index::<i64>(0).unwrap(), 21);
        assert_eq!(row.try_get_by_index::<i64>(1).unwrap(), 7);
        assert_eq!(row.try_get_by_index::<i32>(2).unwrap(), 1);
    }
}
