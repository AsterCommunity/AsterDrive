//! 仓储模块：`upload_session_part_repo`。

use chrono::Utc;
use sea_orm::{
    ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect, Set,
    TryInsertResult, sea_query::Expr,
};

use crate::errors::{AsterError, Result};
use aster_drive_model::entities::upload_session_part::{self, Entity as UploadSessionPart};

/// Outcome of a part upsert, distinguishing a fresh claim from an overwrite.
pub struct UpsertPartResult {
    /// Reloaded row state after the upsert.
    pub model: upload_session_part::Model,
    /// `true` when this call created the row instead of updating an existing one.
    pub inserted: bool,
}

/// Atomically reserves a part slot with an empty etag. Returns `true` when this
/// call created the claim and `false` when the slot is already claimed; the
/// claim row is left untouched on conflict.
pub async fn try_claim_part<C: ConnectionTrait>(
    db: &C,
    upload_id: &str,
    part_number: i32,
) -> Result<bool> {
    let now = Utc::now();
    match UploadSessionPart::insert(upload_session_part::ActiveModel {
        upload_id: Set(upload_id.to_string()),
        part_number: Set(part_number),
        etag: Set(String::new()),
        size: Set(0),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    })
    .on_conflict_do_nothing_on([
        upload_session_part::Column::UploadId,
        upload_session_part::Column::PartNumber,
    ])
    .exec(db)
    .await
    .map_err(AsterError::from)?
    {
        TryInsertResult::Inserted(_) => Ok(true),
        TryInsertResult::Conflicted => Ok(false),
        TryInsertResult::Empty => Err(AsterError::internal_error(
            "try_claim_part produced empty insert result",
        )),
    }
}

/// Records a completed part, inserting the claim row when missing and otherwise
/// overwriting its etag, size and update time. Returns the reloaded row plus
/// whether this call performed the insert; errors if a conflicting row cannot
/// be updated or reloaded afterwards.
pub async fn upsert_part<C: ConnectionTrait>(
    db: &C,
    upload_id: &str,
    part_number: i32,
    etag: &str,
    size: i64,
) -> Result<UpsertPartResult> {
    let now = Utc::now();
    let upload_id_owned = upload_id.to_string();
    let etag_owned = etag.to_string();

    let inserted = match UploadSessionPart::insert(upload_session_part::ActiveModel {
        upload_id: Set(upload_id_owned.clone()),
        part_number: Set(part_number),
        etag: Set(etag_owned.clone()),
        size: Set(size),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    })
    .on_conflict_do_nothing_on([
        upload_session_part::Column::UploadId,
        upload_session_part::Column::PartNumber,
    ])
    .exec(db)
    .await
    .map_err(AsterError::from)?
    {
        TryInsertResult::Inserted(_) => true,
        TryInsertResult::Conflicted => false,
        TryInsertResult::Empty => {
            return Err(AsterError::internal_error(
                "upsert_part produced empty insert result",
            ));
        }
    };

    if !inserted {
        let result = UploadSessionPart::update_many()
            .col_expr(upload_session_part::Column::Etag, Expr::value(etag_owned))
            .col_expr(upload_session_part::Column::Size, Expr::value(size))
            .col_expr(upload_session_part::Column::UpdatedAt, Expr::value(now))
            .filter(upload_session_part::Column::UploadId.eq(upload_id))
            .filter(upload_session_part::Column::PartNumber.eq(part_number))
            .exec(db)
            .await
            .map_err(AsterError::from)?;

        if result.rows_affected == 0 {
            return Err(AsterError::internal_error(format!(
                "upsert_part update affected 0 rows for upload_id={upload_id}, part_number={part_number}"
            )));
        }
    }

    let model = UploadSessionPart::find()
        .filter(upload_session_part::Column::UploadId.eq(upload_id))
        .filter(upload_session_part::Column::PartNumber.eq(part_number))
        .one(db)
        .await
        .map_err(AsterError::from)?
        .ok_or_else(|| {
            AsterError::internal_error(format!(
                "upsert_part could not reload row for upload_id={upload_id}, part_number={part_number}"
            ))
        })?;

    Ok(UpsertPartResult { model, inserted })
}

/// Loads a single part row (claimed or not) for the given upload. Returns `None`
/// when the slot has no row.
pub async fn find_by_upload_and_part<C: ConnectionTrait>(
    db: &C,
    upload_id: &str,
    part_number: i32,
) -> Result<Option<upload_session_part::Model>> {
    UploadSessionPart::find()
        .filter(upload_session_part::Column::UploadId.eq(upload_id))
        .filter(upload_session_part::Column::PartNumber.eq(part_number))
        .one(db)
        .await
        .map_err(AsterError::from)
}

/// Fills in etag and size on an unclaimed (empty-etag) part row only. Returns
/// `true` when this call finalized the claim and `false` when the row is absent
/// or already finalized; the row is never overwritten twice.
pub async fn finalize_claimed_part<C: ConnectionTrait>(
    db: &C,
    upload_id: &str,
    part_number: i32,
    etag: &str,
    size: i64,
) -> Result<bool> {
    let result = UploadSessionPart::update_many()
        .col_expr(
            upload_session_part::Column::Etag,
            Expr::value(etag.to_string()),
        )
        .col_expr(upload_session_part::Column::Size, Expr::value(size))
        .col_expr(
            upload_session_part::Column::UpdatedAt,
            Expr::value(Utc::now()),
        )
        .filter(upload_session_part::Column::UploadId.eq(upload_id))
        .filter(upload_session_part::Column::PartNumber.eq(part_number))
        .filter(upload_session_part::Column::Etag.eq(""))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}

/// Refreshes `updated_at` on an unclaimed (empty-etag) part row to keep the
/// claim out of stale-claim cleanup. Returns `false` when the claim no longer
/// exists, leaving the row untouched.
pub async fn touch_claimed_part<C: ConnectionTrait>(
    db: &C,
    upload_id: &str,
    part_number: i32,
) -> Result<bool> {
    let claim_exists = UploadSessionPart::find()
        .filter(upload_session_part::Column::UploadId.eq(upload_id))
        .filter(upload_session_part::Column::PartNumber.eq(part_number))
        .filter(upload_session_part::Column::Etag.eq(""))
        .one(db)
        .await
        .map_err(AsterError::from)?
        .is_some();
    if !claim_exists {
        return Ok(false);
    }

    UploadSessionPart::update_many()
        .col_expr(
            upload_session_part::Column::UpdatedAt,
            Expr::value(Utc::now()),
        )
        .filter(upload_session_part::Column::UploadId.eq(upload_id))
        .filter(upload_session_part::Column::PartNumber.eq(part_number))
        .filter(upload_session_part::Column::Etag.eq(""))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(true)
}

/// Removes an unclaimed (empty-etag) part row whose `updated_at` is older than
/// `stale_before`. Returns `true` when this call deleted the claim; finalized
/// rows are never removed here.
pub async fn delete_stale_claim<C: ConnectionTrait>(
    db: &C,
    upload_id: &str,
    part_number: i32,
    stale_before: chrono::DateTime<Utc>,
) -> Result<bool> {
    let result = UploadSessionPart::delete_many()
        .filter(upload_session_part::Column::UploadId.eq(upload_id))
        .filter(upload_session_part::Column::PartNumber.eq(part_number))
        .filter(upload_session_part::Column::Etag.eq(""))
        .filter(upload_session_part::Column::UpdatedAt.lt(stale_before))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}

/// Lists the finalized (non-empty etag) parts of an upload, ordered by part number.
pub async fn list_by_upload<C: ConnectionTrait>(
    db: &C,
    upload_id: &str,
) -> Result<Vec<upload_session_part::Model>> {
    UploadSessionPart::find()
        .filter(upload_session_part::Column::UploadId.eq(upload_id))
        .filter(upload_session_part::Column::Etag.ne(""))
        .order_by_asc(upload_session_part::Column::PartNumber)
        .all(db)
        .await
        .map_err(AsterError::from)
}

/// Lists all part rows of an upload, including unfinished (empty-etag) claims,
/// ordered by part number.
pub async fn list_all_by_upload<C: ConnectionTrait>(
    db: &C,
    upload_id: &str,
) -> Result<Vec<upload_session_part::Model>> {
    UploadSessionPart::find()
        .filter(upload_session_part::Column::UploadId.eq(upload_id))
        .order_by_asc(upload_session_part::Column::PartNumber)
        .all(db)
        .await
        .map_err(AsterError::from)
}

/// Lists the finalized (non-empty etag) part numbers of an upload in ascending order.
pub async fn list_part_numbers<C: ConnectionTrait>(db: &C, upload_id: &str) -> Result<Vec<i32>> {
    UploadSessionPart::find()
        .select_only()
        .column(upload_session_part::Column::PartNumber)
        .filter(upload_session_part::Column::UploadId.eq(upload_id))
        .filter(upload_session_part::Column::Etag.ne(""))
        .order_by_asc(upload_session_part::Column::PartNumber)
        .into_tuple::<i32>()
        .all(db)
        .await
        .map_err(AsterError::from)
}

/// Inserts a completed local offset-staging receipt without replacing an existing row.
///
/// A local receipt is the durable completion index for the staging range. Keeping this
/// operation insert-only prevents a retry from changing a provider multipart ETag or
/// accidentally turning a corrupted local row into a valid one.
pub async fn insert_part_if_missing<C: ConnectionTrait>(
    db: &C,
    upload_id: &str,
    part_number: i32,
    etag: &str,
    size: i64,
) -> Result<bool> {
    let now = Utc::now();
    match UploadSessionPart::insert(upload_session_part::ActiveModel {
        upload_id: Set(upload_id.to_string()),
        part_number: Set(part_number),
        etag: Set(etag.to_string()),
        size: Set(size),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    })
    .on_conflict_do_nothing_on([
        upload_session_part::Column::UploadId,
        upload_session_part::Column::PartNumber,
    ])
    .exec(db)
    .await
    .map_err(AsterError::from)?
    {
        TryInsertResult::Inserted(_) => Ok(true),
        TryInsertResult::Conflicted => Ok(false),
        TryInsertResult::Empty => Err(AsterError::internal_error(
            "insert_part_if_missing produced empty insert result",
        )),
    }
}

/// Deletes every part row of an upload, returning the number of removed rows.
pub async fn delete_by_upload<C: ConnectionTrait>(db: &C, upload_id: &str) -> Result<u64> {
    let res = UploadSessionPart::delete_many()
        .filter(upload_session_part::Column::UploadId.eq(upload_id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(res.rows_affected)
}

/// Deletes one part row of an upload, returning the number of removed rows
/// (0 when the slot has no row).
pub async fn delete_by_upload_and_part<C: ConnectionTrait>(
    db: &C,
    upload_id: &str,
    part_number: i32,
) -> Result<u64> {
    let res = UploadSessionPart::delete_many()
        .filter(upload_session_part::Column::UploadId.eq(upload_id))
        .filter(upload_session_part::Column::PartNumber.eq(part_number))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(res.rows_affected)
}
