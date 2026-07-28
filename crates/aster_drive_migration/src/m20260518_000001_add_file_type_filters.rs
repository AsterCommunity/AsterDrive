//! 数据库迁移：为文件搜索增加后缀和分类派生字段。

use aster_forge_file_classification::{FileClassification, classify_file};
use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::{ConnectionTrait, TransactionTrait};

#[derive(DeriveMigrationName)]
pub struct Migration;

const INDEXES: &[&str] = &[
    "idx_files_owner_deleted_category_ext",
    "idx_files_owner_deleted_compound_ext",
    "idx_files_team_deleted_category_ext",
    "idx_files_team_deleted_compound_ext",
];
const BACKFILL_BATCH_SIZE: u64 = 1_000;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for column in [
            ColumnDef::new(Files::Extension)
                .string_len(32)
                .not_null()
                .default("")
                .to_owned(),
            ColumnDef::new(Files::CompoundExtension)
                .string_len(32)
                .null()
                .to_owned(),
            ColumnDef::new(Files::FileCategory)
                .string_len(32)
                .not_null()
                .default("other")
                .to_owned(),
        ] {
            manager
                .alter_table(
                    Table::alter()
                        .table(Files::Table)
                        .add_column(column)
                        .to_owned(),
                )
                .await?;
        }

        backfill_file_type_fields(manager).await?;
        create_indexes(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for index in INDEXES {
            manager
                .drop_index(
                    Index::drop()
                        .name(*index)
                        .table(Files::Table)
                        .if_exists()
                        .to_owned(),
                )
                .await?;
        }

        manager
            .alter_table(
                Table::alter()
                    .table(Files::Table)
                    .drop_column(Files::FileCategory)
                    .drop_column(Files::CompoundExtension)
                    .drop_column(Files::Extension)
                    .to_owned(),
            )
            .await
    }
}

async fn create_indexes(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    for index in [
        Index::create()
            .name("idx_files_owner_deleted_category_ext")
            .table(Files::Table)
            .col(Files::OwnerUserId)
            .col(Files::TeamId)
            .col(Files::DeletedAt)
            .col(Files::FileCategory)
            .col(Files::Extension)
            .to_owned(),
        Index::create()
            .name("idx_files_owner_deleted_compound_ext")
            .table(Files::Table)
            .col(Files::OwnerUserId)
            .col(Files::TeamId)
            .col(Files::DeletedAt)
            .col(Files::CompoundExtension)
            .to_owned(),
        Index::create()
            .name("idx_files_team_deleted_category_ext")
            .table(Files::Table)
            .col(Files::TeamId)
            .col(Files::DeletedAt)
            .col(Files::FileCategory)
            .col(Files::Extension)
            .to_owned(),
        Index::create()
            .name("idx_files_team_deleted_compound_ext")
            .table(Files::Table)
            .col(Files::TeamId)
            .col(Files::DeletedAt)
            .col(Files::CompoundExtension)
            .to_owned(),
    ] {
        manager.create_index(index).await?;
    }

    Ok(())
}

async fn backfill_file_type_fields(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let db = manager.get_connection();
    let mut last_processed_id = 0_i64;

    loop {
        let mut select = Query::select();
        select
            .columns([Files::Id, Files::Name, Files::MimeType])
            .from(Files::Table)
            .and_where(Expr::col(Files::Id).gt(last_processed_id))
            .order_by(Files::Id, Order::Asc)
            .limit(BACKFILL_BATCH_SIZE);

        let rows = db.query_all(&select).await?;
        if rows.is_empty() {
            break;
        }

        let mut updates = Vec::with_capacity(rows.len());
        for row in rows {
            let id = row.try_get_by_index::<i64>(0)?;
            let name = row.try_get_by_index::<String>(1)?;
            let mime_type = row.try_get_by_index::<String>(2)?;
            updates.push(FileTypeBackfill {
                id,
                classification: classify_file(&name, &mime_type),
            });
            last_processed_id = id;
        }

        let ids: Vec<i64> = updates.iter().map(|update| update.id).collect();
        let mut update = Query::update();
        update
            .table(Files::Table)
            .values([
                (Files::Extension, extension_case(&updates)),
                (Files::CompoundExtension, compound_extension_case(&updates)),
                (Files::FileCategory, file_category_case(&updates)),
            ])
            .and_where(Expr::col(Files::Id).is_in(ids));

        let txn = db.begin().await?;
        txn.execute(&update).await?;
        txn.commit().await?;
    }

    Ok(())
}

struct FileTypeBackfill {
    id: i64,
    classification: FileClassification,
}

fn extension_case(updates: &[FileTypeBackfill]) -> SimpleExpr {
    let first = &updates[0];
    let mut expr = Expr::case(
        Expr::col(Files::Id).eq(first.id),
        first.classification.extension.clone(),
    );
    for update in &updates[1..] {
        expr = expr.case(
            Expr::col(Files::Id).eq(update.id),
            update.classification.extension.clone(),
        );
    }
    expr.finally(Expr::col(Files::Extension)).into()
}

fn compound_extension_case(updates: &[FileTypeBackfill]) -> SimpleExpr {
    let first = &updates[0];
    let mut expr = Expr::case(
        Expr::col(Files::Id).eq(first.id),
        first.classification.compound_extension.clone(),
    );
    for update in &updates[1..] {
        expr = expr.case(
            Expr::col(Files::Id).eq(update.id),
            update.classification.compound_extension.clone(),
        );
    }
    expr.finally(Expr::col(Files::CompoundExtension)).into()
}

fn file_category_case(updates: &[FileTypeBackfill]) -> SimpleExpr {
    let first = &updates[0];
    let mut expr = Expr::case(
        Expr::col(Files::Id).eq(first.id),
        first.classification.category.as_str(),
    );
    for update in &updates[1..] {
        expr = expr.case(
            Expr::col(Files::Id).eq(update.id),
            update.classification.category.as_str(),
        );
    }
    expr.finally(Expr::col(Files::FileCategory)).into()
}

#[derive(DeriveIden)]
enum Files {
    Table,
    Id,
    Name,
    MimeType,
    OwnerUserId,
    TeamId,
    DeletedAt,
    Extension,
    CompoundExtension,
    FileCategory,
}
