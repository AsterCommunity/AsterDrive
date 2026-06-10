//! 仓储模块：`upload_session_part_repo`。

use chrono::Utc;
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder,
    QuerySelect, Set, TryInsertResult, sea_query::Expr,
};

use crate::entities::upload_session_part::{self, Entity as UploadSessionPart};
use crate::errors::{AsterError, Result};

pub struct UpsertPartResult {
    pub model: upload_session_part::Model,
    pub inserted: bool,
}

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

pub async fn find_by_upload_and_part(
    db: &DatabaseConnection,
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

pub async fn list_by_upload(
    db: &DatabaseConnection,
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

pub async fn list_part_numbers(db: &DatabaseConnection, upload_id: &str) -> Result<Vec<i32>> {
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

pub async fn delete_by_upload<C: ConnectionTrait>(db: &C, upload_id: &str) -> Result<u64> {
    let res = UploadSessionPart::delete_many()
        .filter(upload_session_part::Column::UploadId.eq(upload_id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(res.rows_affected)
}

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
