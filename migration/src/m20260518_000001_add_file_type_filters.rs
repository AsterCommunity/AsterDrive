//! 数据库迁移：为文件搜索增加后缀和分类派生字段。

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

struct FileClassification {
    extension: String,
    compound_extension: Option<String>,
    category: &'static str,
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
        first.classification.category,
    );
    for update in &updates[1..] {
        expr = expr.case(
            Expr::col(Files::Id).eq(update.id),
            update.classification.category,
        );
    }
    expr.finally(Expr::col(Files::FileCategory)).into()
}

fn classify_file(name: &str, mime_type: &str) -> FileClassification {
    let extension = extension_from_name(name).unwrap_or_default();
    let compound_extension = compound_extension_from_name(name);
    let category =
        classify_extension_and_mime(&extension, compound_extension.as_deref(), mime_type);
    FileClassification {
        extension,
        compound_extension,
        category,
    }
}

fn extension_from_name(name: &str) -> Option<String> {
    let trimmed = name.trim();
    let dot = trimmed.rfind('.')?;
    if dot == 0 || dot + 1 >= trimmed.len() {
        return None;
    }
    Some(trimmed[dot + 1..].to_ascii_lowercase())
}

fn compound_extension_from_name(name: &str) -> Option<String> {
    let normalized = name.trim().to_ascii_lowercase();
    [
        "tar.gz", "tar.bz2", "tar.xz", "tar.zst", "tar.br", "tar.lz", "tar.lzma", "tar.lzo",
    ]
    .iter()
    .find(|extension| normalized.ends_with(&format!(".{extension}")))
    .map(|extension| (*extension).to_string())
}

fn classify_extension_and_mime(
    extension: &str,
    compound_extension: Option<&str>,
    mime_type: &str,
) -> &'static str {
    if compound_extension.is_some()
        || matches!(
            extension,
            "zip"
                | "rar"
                | "7z"
                | "tar"
                | "gz"
                | "bz2"
                | "xz"
                | "zst"
                | "br"
                | "tgz"
                | "tbz"
                | "tbz2"
                | "txz"
                | "lz"
                | "lzma"
                | "lzo"
                | "cab"
                | "iso"
                | "dmg"
        )
    {
        return "archive";
    }
    if matches!(
        extension,
        "xls" | "xlsx" | "ods" | "csv" | "tsv" | "numbers"
    ) {
        return "spreadsheet";
    }
    if matches!(extension, "ppt" | "pptx" | "odp" | "key") {
        return "presentation";
    }
    if matches!(
        extension,
        "jpg"
            | "jpeg"
            | "png"
            | "gif"
            | "webp"
            | "bmp"
            | "tif"
            | "tiff"
            | "svg"
            | "ico"
            | "avif"
            | "heic"
            | "heif"
            | "raw"
            | "cr2"
            | "nef"
            | "orf"
            | "rw2"
    ) {
        return "image";
    }
    if matches!(
        extension,
        "mp4"
            | "m4v"
            | "mov"
            | "avi"
            | "mkv"
            | "webm"
            | "flv"
            | "wmv"
            | "mpeg"
            | "mpg"
            | "3gp"
            | "ts"
            | "m2ts"
            | "ogv"
    ) {
        return "video";
    }
    if matches!(
        extension,
        "mp3"
            | "wav"
            | "flac"
            | "aac"
            | "m4a"
            | "ogg"
            | "oga"
            | "opus"
            | "wma"
            | "aiff"
            | "alac"
            | "mid"
            | "midi"
    ) {
        return "audio";
    }
    if matches!(
        extension,
        "pdf"
            | "txt"
            | "md"
            | "markdown"
            | "rtf"
            | "doc"
            | "docx"
            | "odt"
            | "pages"
            | "epub"
            | "tex"
    ) {
        return "document";
    }
    if matches!(
        extension,
        "rs" | "ts"
            | "tsx"
            | "js"
            | "jsx"
            | "mjs"
            | "cjs"
            | "json"
            | "jsonc"
            | "yaml"
            | "yml"
            | "toml"
            | "xml"
            | "html"
            | "htm"
            | "css"
            | "scss"
            | "sass"
            | "less"
            | "sql"
            | "sh"
            | "bash"
            | "zsh"
            | "fish"
            | "ps1"
            | "py"
            | "rb"
            | "go"
            | "java"
            | "kt"
            | "kts"
            | "swift"
            | "c"
            | "h"
            | "cpp"
            | "cc"
            | "cxx"
            | "hpp"
            | "cs"
            | "php"
            | "lua"
            | "dart"
            | "vue"
            | "svelte"
            | "lock"
            | "ini"
            | "conf"
            | "dockerfile"
            | "makefile"
    ) {
        return "code";
    }

    classify_mime(mime_type)
}

fn classify_mime(mime_type: &str) -> &'static str {
    let mime = mime_type.trim().to_ascii_lowercase();
    if mime.starts_with("image/") {
        "image"
    } else if mime.starts_with("video/") {
        "video"
    } else if mime.starts_with("audio/") {
        "audio"
    } else if mime == "application/pdf" || mime.starts_with("text/") {
        "document"
    } else if mime.contains("spreadsheet") || mime.contains("excel") || mime.ends_with("/csv") {
        "spreadsheet"
    } else if mime.contains("presentation") || mime.contains("powerpoint") {
        "presentation"
    } else if mime.contains("zip")
        || mime.contains("compressed")
        || mime.contains("x-tar")
        || mime.contains("x-7z")
        || mime.contains("x-rar")
    {
        "archive"
    } else if mime.contains("json") || mime.contains("xml") {
        "code"
    } else {
        "other"
    }
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
