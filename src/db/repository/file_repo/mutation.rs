//! `file_repo` 仓储子模块：`mutation`。

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, Set, sea_query::Expr,
};

use crate::errors::{AsterError, Result};
use aster_drive_model::entities::file::{self, Entity as File};

use super::common::{map_bulk_name_db_err, map_name_db_err};

/// Inserts a file row as-is; unique-name violations map to the duplicate-name error.
pub async fn create<C: ConnectionTrait>(db: &C, model: file::ActiveModel) -> Result<file::Model> {
    model.insert(db).await.map_err(AsterError::from)
}

/// Fully resolved fields for [`create_with_blob`]; the caller owns validation and blob
/// publication, this only carries what the file row needs.
#[derive(Debug, Clone)]
pub struct CreateFileWithBlobInput<'a> {
    /// File name as provided; classification derives the extension fields from it.
    pub name: &'a str,
    /// Containing folder; `None` places the file at the space root.
    pub folder_id: Option<i64>,
    /// Owning team workspace; `None` marks a personal-space file.
    pub team_id: Option<i64>,
    /// Blob row backing the file's current content.
    pub blob_id: i64,
    /// Logical file size in bytes, expected to match the blob size.
    pub size: i64,
    /// Owning user id for personal files; nullable per the caller's workspace semantics.
    pub owner_user_id: Option<i64>,
    /// User credited with the creation; resolved by the service layer.
    pub created_by_user_id: Option<i64>,
    /// Username snapshot stored alongside the creator id.
    pub created_by_username: &'a str,
    /// MIME type recorded for content handling and the `Type` sort key.
    pub mime_type: &'a str,
    /// Timestamp applied to `created_at`/`updated_at` and attribution fields.
    pub now: chrono::DateTime<Utc>,
}

/// Inserts a file row from a resolved [`CreateFileWithBlobInput`], deriving extension,
/// compound extension, and category from `name`/`mime_type`, and initializing the
/// attribution fields. Unique-name violations map to the duplicate-name error.
pub async fn create_with_blob<C: ConnectionTrait>(
    db: &C,
    input: CreateFileWithBlobInput<'_>,
) -> Result<file::Model> {
    let CreateFileWithBlobInput {
        name,
        folder_id,
        team_id,
        blob_id,
        size,
        owner_user_id,
        created_by_user_id,
        created_by_username,
        mime_type,
        now,
    } = input;
    let classification = aster_forge_file_classification::classify_file(name, mime_type);

    let mut model = file::ActiveModel {
        name: Set(name.to_string()),
        folder_id: Set(folder_id),
        team_id: Set(team_id),
        blob_id: Set(blob_id),
        size: Set(size),
        owner_user_id: Set(owner_user_id),
        created_by_user_id: Set(created_by_user_id),
        created_by_username: Set(created_by_username.to_string()),
        mime_type: Set(mime_type.to_string()),
        extension: Set(classification.extension),
        compound_extension: Set(classification.compound_extension),
        file_category: Set(classification.category),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    model.initialize_attribution();
    File::insert(model)
        .exec_with_returning(db)
        .await
        .map_err(|err| map_name_db_err(err, name))
}

/// 批量插入文件记录（不返回创建的 Model，批量复制用）
pub async fn create_many<C: ConnectionTrait>(db: &C, models: Vec<file::ActiveModel>) -> Result<()> {
    if models.is_empty() {
        return Ok(());
    }
    let models = models
        .into_iter()
        .map(|mut model| {
            model.initialize_attribution();
            model
        })
        .collect::<Vec<_>>();
    File::insert_many(models).exec(db).await.map_err(|err| {
        map_bulk_name_db_err(err, "one or more files already exist in this folder")
    })?;
    Ok(())
}

/// 批量移动文件到同一文件夹
pub async fn move_many_to_folder<C: ConnectionTrait>(
    db: &C,
    ids: &[i64],
    folder_id: Option<i64>,
    now: chrono::DateTime<Utc>,
) -> Result<()> {
    if ids.is_empty() {
        return Ok(());
    }
    File::update_many()
        .col_expr(file::Column::FolderId, Expr::value(folder_id))
        .col_expr(file::Column::UpdatedAt, Expr::value(now))
        .filter(file::Column::Id.is_in(ids.iter().copied()))
        .exec(db)
        .await
        .map_err(|err| {
            map_bulk_name_db_err(err, "one or more files already exist in target folder")
        })?;
    Ok(())
}

/// Repoints every file row backed by `old_blob_id` to `new_blob_id` (blob merge during
/// migration/dedup) and refreshes their `updated_at`. Returns the number of rows moved;
/// no-op returning 0 when the two ids are equal.
pub async fn replace_file_blob_refs<C: ConnectionTrait>(
    db: &C,
    old_blob_id: i64,
    new_blob_id: i64,
) -> Result<u64> {
    if old_blob_id == new_blob_id {
        return Ok(0);
    }

    let result = File::update_many()
        .col_expr(file::Column::BlobId, Expr::value(new_blob_id))
        .col_expr(file::Column::UpdatedAt, Expr::value(Utc::now()))
        .filter(file::Column::BlobId.eq(old_blob_id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected)
}
