use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::{DbBackend, Statement};

const BLOB_BACKING_CHECK_NAME: &str = "ck_file_blobs_backing";
const BLOB_BACKING_PREDICATE: &str = "(backing = 'stored' AND storage_path IS NOT NULL) OR \
     (backing = 'virtual_empty' AND storage_path IS NULL AND size = 0 AND \
      hash = 'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855')";

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if manager.get_database_backend() == DbBackend::Sqlite {
            rebuild_sqlite_file_blobs(manager).await?;
        } else {
            manager
                .alter_table(
                    Table::alter()
                        .table(FileBlobs::Table)
                        .add_column(
                            ColumnDef::new(FileBlobs::Backing)
                                .string_len(16)
                                .not_null()
                                .default("stored"),
                        )
                        .to_owned(),
                )
                .await?;
            manager
                .alter_table(
                    Table::alter()
                        .table(FileBlobs::Table)
                        .modify_column(
                            ColumnDef::new(FileBlobs::StoragePath)
                                .string_len(1024)
                                .null(),
                        )
                        .to_owned(),
                )
                .await?;
            manager
                .get_connection()
                .execute_unprepared(&format!(
                    "ALTER TABLE file_blobs ADD CONSTRAINT {BLOB_BACKING_CHECK_NAME} CHECK ({BLOB_BACKING_PREDICATE})"
                ))
                .await?;
            aster_forge_db_migration::drop_index_if_exists(
                manager.get_connection(),
                "file_blobs",
                "idx_file_blobs_hash_policy",
            )
            .await?;
        }
        manager
            .create_index(
                Index::create()
                    .name("idx_file_blobs_hash_policy_backing")
                    .table(FileBlobs::Table)
                    .col(FileBlobs::Hash)
                    .col(FileBlobs::PolicyId)
                    .col(FileBlobs::Backing)
                    .unique()
                    .to_owned(),
            )
            .await?;
        manager
            .create_table(
                Table::create()
                    .table(FileCreateIdempotencies::Table)
                    .if_not_exists()
                    .col(aster_forge_db_migration::big_integer_primary_key(
                        FileCreateIdempotencies::Id,
                    ))
                    .col(
                        ColumnDef::new(FileCreateIdempotencies::ActorUserId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(FileCreateIdempotencies::WorkspaceKind)
                            .string_len(16)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(FileCreateIdempotencies::WorkspaceId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(FileCreateIdempotencies::KeyHash)
                            .string_len(64)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(FileCreateIdempotencies::RequestFingerprint)
                            .string_len(64)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(FileCreateIdempotencies::ResultFileId)
                            .big_integer()
                            .null(),
                    )
                    .col(
                        aster_forge_db_migration::utc_date_time_column(
                            manager,
                            FileCreateIdempotencies::CreatedAt,
                        )
                        .not_null(),
                    )
                    .col(
                        aster_forge_db_migration::utc_date_time_column(
                            manager,
                            FileCreateIdempotencies::ExpiresAt,
                        )
                        .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(
                                FileCreateIdempotencies::Table,
                                FileCreateIdempotencies::ResultFileId,
                            )
                            .to(Files::Table, Files::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_file_create_idempotencies_scope_key")
                    .table(FileCreateIdempotencies::Table)
                    .col(FileCreateIdempotencies::ActorUserId)
                    .col(FileCreateIdempotencies::WorkspaceKind)
                    .col(FileCreateIdempotencies::WorkspaceId)
                    .col(FileCreateIdempotencies::KeyHash)
                    .unique()
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_file_create_idempotencies_expiry")
                    .table(FileCreateIdempotencies::Table)
                    .col(FileCreateIdempotencies::ExpiresAt)
                    .col(FileCreateIdempotencies::Id)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        ensure_no_virtual_or_pathless_blobs(manager).await?;
        manager
            .drop_table(
                Table::drop()
                    .table(FileCreateIdempotencies::Table)
                    .to_owned(),
            )
            .await?;
        if manager.get_database_backend() == DbBackend::Sqlite {
            return rebuild_sqlite_file_blobs_down(manager).await;
        }
        manager
            .alter_table(
                Table::alter()
                    .table(FileBlobs::Table)
                    .drop_constraint(Alias::new(BLOB_BACKING_CHECK_NAME))
                    .to_owned(),
            )
            .await?;
        manager
            .drop_index(
                Index::drop()
                    .name("idx_file_blobs_hash_policy_backing")
                    .table(FileBlobs::Table)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_file_blobs_hash_policy")
                    .table(FileBlobs::Table)
                    .col(FileBlobs::Hash)
                    .col(FileBlobs::PolicyId)
                    .unique()
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(FileBlobs::Table)
                    .modify_column(
                        ColumnDef::new(FileBlobs::StoragePath)
                            .string_len(1024)
                            .not_null(),
                    )
                    .drop_column(FileBlobs::Backing)
                    .to_owned(),
            )
            .await
    }
}

async fn rebuild_sqlite_file_blobs_down(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    const REBUILT_TABLE: &str = "file_blobs_stored_rebuild";
    rebuild_sqlite_file_blobs_down_unchecked(manager, REBUILT_TABLE).await
}

async fn ensure_no_virtual_or_pathless_blobs(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let backend = manager.get_database_backend();
    let virtual_count = manager
        .get_connection()
        .query_one_raw(Statement::from_string(
            backend,
            "SELECT COUNT(*) AS count FROM file_blobs WHERE backing <> 'stored' OR storage_path IS NULL",
        ))
        .await?
        .and_then(|row| row.try_get_by_index::<i64>(0).ok())
        .unwrap_or(0);
    if virtual_count > 0 {
        return Err(DbErr::Migration(format!(
            "cannot roll back virtual-empty blob schema while {virtual_count} virtual or pathless blob rows exist"
        )));
    }
    Ok(())
}

async fn rebuild_sqlite_file_blobs_down_unchecked(
    manager: &SchemaManager<'_>,
    rebuilt_table: &str,
) -> Result<(), DbErr> {
    manager
        .drop_table(
            Table::drop()
                .table(Alias::new(rebuilt_table))
                .if_exists()
                .to_owned(),
        )
        .await?;
    manager
        .create_table(
            Table::create()
                .table(Alias::new(rebuilt_table))
                .col(aster_forge_db_migration::big_integer_primary_key(
                    FileBlobs::Id,
                ))
                .col(ColumnDef::new(FileBlobs::Hash).string_len(64).not_null())
                .col(ColumnDef::new(FileBlobs::Size).big_integer().not_null())
                .col(ColumnDef::new(FileBlobs::PolicyId).big_integer().not_null())
                .col(
                    ColumnDef::new(FileBlobs::StoragePath)
                        .string_len(1024)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(FileBlobs::ThumbnailPath)
                        .string_len(1024)
                        .null(),
                )
                .col(
                    ColumnDef::new(FileBlobs::ThumbnailProcessor)
                        .string_len(32)
                        .null(),
                )
                .col(
                    ColumnDef::new(FileBlobs::ThumbnailVersion)
                        .string_len(32)
                        .null(),
                )
                .col(
                    ColumnDef::new(FileBlobs::RefCount)
                        .integer()
                        .not_null()
                        .default(1),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(manager, FileBlobs::CreatedAt)
                        .not_null(),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(manager, FileBlobs::UpdatedAt)
                        .not_null(),
                )
                .foreign_key(
                    ForeignKey::create()
                        .from(Alias::new(rebuilt_table), FileBlobs::PolicyId)
                        .to(StoragePolicies::Table, StoragePolicies::Id),
                )
                .to_owned(),
        )
        .await?;
    manager
        .get_connection()
        .execute_unprepared(
            "INSERT INTO file_blobs_stored_rebuild \
             (id, hash, size, policy_id, storage_path, thumbnail_path, thumbnail_processor, thumbnail_version, ref_count, created_at, updated_at) \
             SELECT id, hash, size, policy_id, storage_path, thumbnail_path, thumbnail_processor, thumbnail_version, ref_count, created_at, updated_at \
             FROM file_blobs",
        )
        .await?;
    manager
        .drop_table(Table::drop().table(FileBlobs::Table).to_owned())
        .await?;
    manager
        .rename_table(
            Table::rename()
                .table(Alias::new(rebuilt_table), FileBlobs::Table)
                .to_owned(),
        )
        .await?;
    manager
        .create_index(
            Index::create()
                .name("idx_file_blobs_storage_path")
                .table(FileBlobs::Table)
                .col(FileBlobs::StoragePath)
                .to_owned(),
        )
        .await?;
    manager
        .create_index(
            Index::create()
                .name("idx_file_blobs_hash_policy")
                .table(FileBlobs::Table)
                .col(FileBlobs::Hash)
                .col(FileBlobs::PolicyId)
                .unique()
                .to_owned(),
        )
        .await
}

async fn rebuild_sqlite_file_blobs(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    const REBUILT_TABLE: &str = "file_blobs_virtual_empty_rebuild";
    manager
        .drop_table(
            Table::drop()
                .table(Alias::new(REBUILT_TABLE))
                .if_exists()
                .to_owned(),
        )
        .await?;
    manager
        .create_table(
            Table::create()
                .table(Alias::new(REBUILT_TABLE))
                .col(aster_forge_db_migration::big_integer_primary_key(
                    FileBlobs::Id,
                ))
                .col(ColumnDef::new(FileBlobs::Hash).string_len(64).not_null())
                .col(ColumnDef::new(FileBlobs::Size).big_integer().not_null())
                .col(ColumnDef::new(FileBlobs::PolicyId).big_integer().not_null())
                .col(
                    ColumnDef::new(FileBlobs::StoragePath)
                        .string_len(1024)
                        .null(),
                )
                .col(
                    ColumnDef::new(FileBlobs::Backing)
                        .string_len(16)
                        .not_null()
                        .default("stored"),
                )
                .col(
                    ColumnDef::new(FileBlobs::ThumbnailPath)
                        .string_len(1024)
                        .null(),
                )
                .col(
                    ColumnDef::new(FileBlobs::ThumbnailProcessor)
                        .string_len(32)
                        .null(),
                )
                .col(
                    ColumnDef::new(FileBlobs::ThumbnailVersion)
                        .string_len(32)
                        .null(),
                )
                .col(
                    ColumnDef::new(FileBlobs::RefCount)
                        .integer()
                        .not_null()
                        .default(1),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(manager, FileBlobs::CreatedAt)
                        .not_null(),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(manager, FileBlobs::UpdatedAt)
                        .not_null(),
                )
                .check(blob_backing_check())
                .foreign_key(
                    ForeignKey::create()
                        .from(Alias::new(REBUILT_TABLE), FileBlobs::PolicyId)
                        .to(StoragePolicies::Table, StoragePolicies::Id),
                )
                .to_owned(),
        )
        .await?;
    manager
        .get_connection()
        .execute_unprepared(
            "INSERT INTO file_blobs_virtual_empty_rebuild \
             (id, hash, size, policy_id, storage_path, backing, thumbnail_path, thumbnail_processor, thumbnail_version, ref_count, created_at, updated_at) \
             SELECT id, hash, size, policy_id, storage_path, 'stored', thumbnail_path, thumbnail_processor, thumbnail_version, ref_count, created_at, updated_at FROM file_blobs",
        )
        .await?;
    manager
        .drop_table(Table::drop().table(FileBlobs::Table).to_owned())
        .await?;
    manager
        .rename_table(
            Table::rename()
                .table(Alias::new(REBUILT_TABLE), FileBlobs::Table)
                .to_owned(),
        )
        .await?;
    manager
        .create_index(
            Index::create()
                .name("idx_file_blobs_storage_path")
                .table(FileBlobs::Table)
                .col(FileBlobs::StoragePath)
                .to_owned(),
        )
        .await
}

fn blob_backing_check() -> (Alias, SimpleExpr) {
    (
        Alias::new(BLOB_BACKING_CHECK_NAME),
        Expr::cust(BLOB_BACKING_PREDICATE),
    )
}

#[derive(DeriveIden)]
enum FileBlobs {
    Table,
    Id,
    Hash,
    Size,
    PolicyId,
    StoragePath,
    Backing,
    ThumbnailPath,
    ThumbnailProcessor,
    ThumbnailVersion,
    RefCount,
    CreatedAt,
    UpdatedAt,
}
#[derive(DeriveIden)]
enum Files {
    Table,
    Id,
}
#[derive(DeriveIden)]
enum StoragePolicies {
    Table,
    Id,
}
#[derive(DeriveIden)]
enum FileCreateIdempotencies {
    Table,
    Id,
    ActorUserId,
    WorkspaceKind,
    WorkspaceId,
    KeyHash,
    RequestFingerprint,
    ResultFileId,
    CreatedAt,
    ExpiresAt,
}
