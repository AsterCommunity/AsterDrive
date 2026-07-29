use chrono::Utc;
use sea_orm::{
    ColumnTrait, ConnectionTrait, EntityTrait, ExprTrait, QueryFilter, sea_query::CaseStatement,
    sea_query::Expr,
};

use crate::errors::{AsterError, Result};
use aster_drive_model::entities::file_blob::{self, Entity as FileBlob};
use aster_forge_utils::numbers::usize_to_u64;

pub async fn find_active_blob_by_hash<C: ConnectionTrait>(
    db: &C,
    hash: &str,
    policy_id: i64,
) -> Result<Option<file_blob::Model>> {
    FileBlob::find()
        .filter(file_blob::Column::Hash.eq(hash))
        .filter(file_blob::Column::PolicyId.eq(policy_id))
        .filter(file_blob::Column::RefCount.gte(0))
        .one(db)
        .await
        .map_err(AsterError::from)
}

/// 原子递增 blob ref_count（防止并发丢更新）
pub async fn increment_blob_ref_count<C: ConnectionTrait>(db: &C, id: i64) -> Result<()> {
    let result = FileBlob::update_many()
        .col_expr(
            file_blob::Column::RefCount,
            Expr::col(file_blob::Column::RefCount).add(1i32),
        )
        .col_expr(file_blob::Column::UpdatedAt, Expr::value(Utc::now()))
        .filter(file_blob::Column::Id.eq(id))
        .filter(file_blob::Column::RefCount.gte(0))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    if result.rows_affected == 0 {
        return Err(AsterError::record_not_found(format!("file_blob #{id}")));
    }
    Ok(())
}

/// 原子增加 blob ref_count（可变增量，批量复制用）
pub async fn increment_blob_ref_count_by<C: ConnectionTrait>(
    db: &C,
    id: i64,
    delta: i32,
) -> Result<()> {
    if delta < 0 {
        return Err(AsterError::internal_error(format!(
            "increment_blob_ref_count_by requires positive delta, got {delta}"
        )));
    }
    if delta == 0 {
        return Ok(());
    }
    let result = FileBlob::update_many()
        .col_expr(
            file_blob::Column::RefCount,
            Expr::col(file_blob::Column::RefCount).add(delta),
        )
        .col_expr(file_blob::Column::UpdatedAt, Expr::value(Utc::now()))
        .filter(file_blob::Column::Id.eq(id))
        .filter(file_blob::Column::RefCount.gte(0))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    if result.rows_affected == 0 {
        return Err(AsterError::record_not_found(format!("file_blob #{id}")));
    }
    Ok(())
}

/// 批量原子增加多个 blob 的 ref_count（每个 blob 可有不同增量）。
pub async fn increment_blob_ref_counts_by<C: ConnectionTrait>(
    db: &C,
    deltas: &[(i64, i32)],
) -> Result<()> {
    let deltas = normalize_blob_ref_count_deltas(deltas, "increment_blob_ref_counts_by")?;
    if deltas.is_empty() {
        return Ok(());
    }

    let ids: Vec<i64> = deltas.iter().map(|(id, _)| *id).collect();
    let expected_rows = usize_to_u64(deltas.len(), "blob ref_count increment row count")?;
    let result = FileBlob::update_many()
        .col_expr(
            file_blob::Column::RefCount,
            blob_ref_count_increment_expr(&deltas),
        )
        .col_expr(file_blob::Column::UpdatedAt, Expr::value(Utc::now()))
        .filter(file_blob::Column::Id.is_in(ids.iter().copied()))
        .filter(file_blob::Column::RefCount.gte(0))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    if result.rows_affected != expected_rows {
        return Err(AsterError::record_not_found(
            "one or more file_blob rows were missing or cleanup-claimed during ref_count increment",
        ));
    }
    Ok(())
}

/// 原子递减 blob ref_count（floor 0，防止并发丢更新）
pub async fn decrement_blob_ref_count<C: ConnectionTrait>(db: &C, id: i64) -> Result<()> {
    FileBlob::update_many()
        .col_expr(
            file_blob::Column::RefCount,
            Expr::case(Expr::col(file_blob::Column::RefCount).lt(1i32), 0)
                .finally(Expr::col(file_blob::Column::RefCount).sub(1i32))
                .into(),
        )
        .col_expr(file_blob::Column::UpdatedAt, Expr::value(Utc::now()))
        .filter(file_blob::Column::Id.eq(id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(())
}

/// 原子递减 blob ref_count（可变减量，floor 0）
pub async fn decrement_blob_ref_count_by<C: ConnectionTrait>(
    db: &C,
    id: i64,
    delta: i32,
) -> Result<()> {
    if delta < 0 {
        return Err(AsterError::internal_error(format!(
            "decrement_blob_ref_count_by requires positive delta, got {delta}"
        )));
    }
    if delta == 0 {
        return Ok(());
    }
    FileBlob::update_many()
        .col_expr(
            file_blob::Column::RefCount,
            Expr::case(Expr::col(file_blob::Column::RefCount).lt(delta), 0)
                .finally(Expr::col(file_blob::Column::RefCount).sub(delta))
                .into(),
        )
        .col_expr(file_blob::Column::UpdatedAt, Expr::value(Utc::now()))
        .filter(file_blob::Column::Id.eq(id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(())
}

/// 批量原子递减多个 blob 的 ref_count（每个 blob 可有不同减量，floor 0）。
pub async fn decrement_blob_ref_counts_by<C: ConnectionTrait>(
    db: &C,
    deltas: &[(i64, i32)],
) -> Result<()> {
    let deltas = normalize_blob_ref_count_deltas(deltas, "decrement_blob_ref_counts_by")?;
    if deltas.is_empty() {
        return Ok(());
    }

    let ids: Vec<i64> = deltas.iter().map(|(id, _)| *id).collect();
    FileBlob::update_many()
        .col_expr(
            file_blob::Column::RefCount,
            blob_ref_count_decrement_expr(&deltas),
        )
        .col_expr(file_blob::Column::UpdatedAt, Expr::value(Utc::now()))
        .filter(file_blob::Column::Id.is_in(ids.iter().copied()))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(())
}

pub(super) fn normalize_blob_ref_count_deltas(
    deltas: &[(i64, i32)],
    context: &str,
) -> Result<Vec<(i64, i32)>> {
    let mut merged = std::collections::BTreeMap::<i64, i32>::new();
    for &(id, delta) in deltas {
        if delta < 0 {
            return Err(AsterError::internal_error(format!(
                "{context} requires positive delta for blob {id}, got {delta}"
            )));
        }
        if delta == 0 {
            continue;
        }
        let entry = merged.entry(id).or_default();
        *entry = entry.checked_add(delta).ok_or_else(|| {
            AsterError::internal_error(format!("{context} delta overflow while merging blob {id}"))
        })?;
    }
    Ok(merged.into_iter().collect())
}

pub(super) fn blob_ref_count_increment_expr(
    deltas: &[(i64, i32)],
) -> sea_orm::sea_query::SimpleExpr {
    let mut case = CaseStatement::new();
    for &(id, delta) in deltas {
        case = case.case(
            Expr::col(file_blob::Column::Id).eq(id),
            Expr::col(file_blob::Column::RefCount).add(delta),
        );
    }
    case.finally(Expr::col(file_blob::Column::RefCount)).into()
}

pub(super) fn blob_ref_count_decrement_expr(
    deltas: &[(i64, i32)],
) -> sea_orm::sea_query::SimpleExpr {
    let mut case = CaseStatement::new();
    for &(id, delta) in deltas {
        let decrement = Expr::case(Expr::col(file_blob::Column::RefCount).lt(delta), 0)
            .finally(Expr::col(file_blob::Column::RefCount).sub(delta));
        case = case.case(Expr::col(file_blob::Column::Id).eq(id), decrement);
    }
    case.finally(Expr::col(file_blob::Column::RefCount)).into()
}
