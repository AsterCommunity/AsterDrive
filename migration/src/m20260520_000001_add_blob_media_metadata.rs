//! 数据库迁移：新增 blob 级媒体元数据缓存表。

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(BlobMediaMetadata::Table)
                    .if_not_exists()
                    .col(aster_forge_db_migration::big_integer_primary_key(
                        BlobMediaMetadata::Id,
                    ))
                    .col(
                        ColumnDef::new(BlobMediaMetadata::BlobId)
                            .big_integer()
                            .not_null()
                            .unique_key(),
                    )
                    .col(
                        ColumnDef::new(BlobMediaMetadata::BlobHash)
                            .string_len(64)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BlobMediaMetadata::Kind)
                            .string_len(16)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BlobMediaMetadata::Status)
                            .string_len(16)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BlobMediaMetadata::MetadataJson)
                            .text()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(BlobMediaMetadata::ErrorMessage)
                            .text()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(BlobMediaMetadata::Parser)
                            .string_len(64)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BlobMediaMetadata::ParserVersion)
                            .string_len(32)
                            .not_null(),
                    )
                    .col(
                        aster_forge_db_migration::utc_date_time_column(
                            manager,
                            BlobMediaMetadata::CreatedAt,
                        )
                        .not_null(),
                    )
                    .col(
                        aster_forge_db_migration::utc_date_time_column(
                            manager,
                            BlobMediaMetadata::UpdatedAt,
                        )
                        .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(BlobMediaMetadata::Table, BlobMediaMetadata::BlobId)
                            .to(FileBlobs::Table, FileBlobs::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_blob_media_metadata_blob_id")
                    .table(BlobMediaMetadata::Table)
                    .col(BlobMediaMetadata::BlobId)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(BlobMediaMetadata::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum BlobMediaMetadata {
    Table,
    Id,
    BlobId,
    BlobHash,
    Kind,
    Status,
    MetadataJson,
    ErrorMessage,
    Parser,
    ParserVersion,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum FileBlobs {
    Table,
    Id,
}
