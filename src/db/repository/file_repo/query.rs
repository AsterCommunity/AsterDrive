//! `file_repo` 仓储子模块：`query`。

use std::{borrow::Cow, collections::HashSet};

use sea_orm::{
    ColumnTrait, Condition, ConnectionTrait, DbBackend, EntityTrait, ExprTrait, PaginatorTrait,
    QueryFilter, QueryOrder, QuerySelect, sea_query::Expr,
};
use unicode_normalization::{UnicodeNormalization, is_nfc, is_nfd};

use crate::api::pagination::{SortBy, SortOrder};
use crate::entities::file::{self, Entity as File};
use crate::errors::{AsterError, MapAsterErr, Result};

use super::common::{FileScope, active_scope_condition, apply_folder_condition, scope_condition};

const UNIQUE_FILENAME_CANDIDATE_BATCH_SIZE: usize = 32;

fn sum_as_i64_expr(
    backend: DbBackend,
    column: impl sea_orm::sea_query::IntoColumnRef + Copy,
) -> sea_orm::sea_query::SimpleExpr {
    let type_name = match backend {
        DbBackend::Postgres => "bigint",
        DbBackend::MySql => "signed",
        _ => "integer",
    };
    Expr::col(column).sum().cast_as(type_name)
}

/// 统计未删除文件总数
pub async fn count_live_files<C: ConnectionTrait>(db: &C) -> Result<u64> {
    File::find()
        .filter(file::Column::DeletedAt.is_null())
        .count(db)
        .await
        .map_err(AsterError::from)
}

/// 统计未删除文件总字节数
pub async fn sum_live_file_bytes<C: ConnectionTrait>(db: &C) -> Result<i64> {
    Ok(File::find()
        .select_only()
        .column_as(
            sum_as_i64_expr(db.get_database_backend(), file::Column::Size),
            "sum",
        )
        .filter(file::Column::DeletedAt.is_null())
        .into_tuple::<Option<i64>>()
        .one(db)
        .await?
        .flatten()
        .unwrap_or(0))
}

async fn find_by_folders_in_scope<C: ConnectionTrait>(
    db: &C,
    scope: FileScope,
    folder_ids: &[i64],
) -> Result<Vec<file::Model>> {
    if folder_ids.is_empty() {
        return Ok(vec![]);
    }
    File::find()
        .filter(active_scope_condition(scope))
        .filter(file::Column::FolderId.is_in(folder_ids.iter().copied()))
        .all(db)
        .await
        .map_err(AsterError::from)
}

async fn find_by_folder_in_scope<C: ConnectionTrait>(
    db: &C,
    scope: FileScope,
    folder_id: Option<i64>,
) -> Result<Vec<file::Model>> {
    File::find()
        .filter(apply_folder_condition(
            active_scope_condition(scope),
            folder_id,
        ))
        .order_by_asc(file::Column::Name)
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_by_id<C: ConnectionTrait>(db: &C, id: i64) -> Result<file::Model> {
    File::find_by_id(id)
        .one(db)
        .await
        .map_err(AsterError::from)?
        .ok_or_else(|| AsterError::file_not_found(format!("file #{id}")))
}

/// 以排他锁读取文件记录，用于防止并发操作同一文件时的竞态。
///
/// - Postgres/MySQL：使用 `SELECT ... FOR UPDATE`，有真正的行锁保障。
/// - SQLite：`FOR UPDATE` 不被支持，fallback 到普通读。SQLite 的写操作本身依赖 WAL 写锁，
///   对于 AsterDrive 的写入场景（覆盖上传等）已有 blob ref_count 原子操作兜底，
///   此函数在 SQLite 上的并发保护能力有限，设计上接受这一限制。
pub async fn lock_by_id<C: ConnectionTrait>(db: &C, id: i64) -> Result<file::Model> {
    match db.get_database_backend() {
        DbBackend::Postgres | DbBackend::MySql => File::find_by_id(id)
            .lock_exclusive()
            .one(db)
            .await
            .map_err(AsterError::from)?
            .ok_or_else(|| AsterError::file_not_found(format!("file #{id}"))),
        DbBackend::Sqlite => find_by_id(db, id).await,
        _ => find_by_id(db, id).await,
    }
}

pub async fn find_by_ids<C: ConnectionTrait>(db: &C, ids: &[i64]) -> Result<Vec<file::Model>> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    File::find()
        .filter(file::Column::Id.is_in(ids.iter().copied()))
        .all(db)
        .await
        .map_err(AsterError::from)
}

async fn find_by_ids_in_scope<C: ConnectionTrait>(
    db: &C,
    scope: FileScope,
    ids: &[i64],
) -> Result<Vec<file::Model>> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    File::find()
        .filter(scope_condition(scope))
        .filter(file::Column::Id.is_in(ids.iter().copied()))
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_by_ids_in_personal_scope<C: ConnectionTrait>(
    db: &C,
    user_id: i64,
    ids: &[i64],
) -> Result<Vec<file::Model>> {
    find_by_ids_in_scope(db, FileScope::Personal { user_id }, ids).await
}

pub async fn find_by_ids_in_team_scope<C: ConnectionTrait>(
    db: &C,
    team_id: i64,
    ids: &[i64],
) -> Result<Vec<file::Model>> {
    find_by_ids_in_scope(db, FileScope::Team { team_id }, ids).await
}

/// 批量查询多个文件夹下的未删除文件
pub async fn find_by_folders<C: ConnectionTrait>(
    db: &C,
    user_id: i64,
    folder_ids: &[i64],
) -> Result<Vec<file::Model>> {
    find_by_folders_in_scope(db, FileScope::Personal { user_id }, folder_ids).await
}

pub async fn find_by_team_folders<C: ConnectionTrait>(
    db: &C,
    team_id: i64,
    folder_ids: &[i64],
) -> Result<Vec<file::Model>> {
    find_by_folders_in_scope(db, FileScope::Team { team_id }, folder_ids).await
}

/// 批量查询多个文件夹下的文件（含已删除）
pub async fn find_all_in_folders<C: ConnectionTrait>(
    db: &C,
    folder_ids: &[i64],
) -> Result<Vec<file::Model>> {
    if folder_ids.is_empty() {
        return Ok(vec![]);
    }
    File::find()
        .filter(file::Column::FolderId.is_in(folder_ids.iter().copied()))
        .all(db)
        .await
        .map_err(AsterError::from)
}

/// 查询文件夹下的文件（排除已删除）
pub async fn find_by_folder<C: ConnectionTrait>(
    db: &C,
    user_id: i64,
    folder_id: Option<i64>,
) -> Result<Vec<file::Model>> {
    find_by_folder_in_scope(db, FileScope::Personal { user_id }, folder_id).await
}

pub async fn find_by_team_folder<C: ConnectionTrait>(
    db: &C,
    team_id: i64,
    folder_id: Option<i64>,
) -> Result<Vec<file::Model>> {
    find_by_folder_in_scope(db, FileScope::Team { team_id }, folder_id).await
}

/// 查询文件夹下的文件（排除已删除，cursor 分页，支持多字段排序）
async fn find_by_folder_cursor_in_scope<C: ConnectionTrait>(
    db: &C,
    scope: FileScope,
    folder_id: Option<i64>,
    limit: u64,
    after: Option<(String, i64)>,
    sort_by: SortBy,
    sort_order: SortOrder,
) -> Result<(Vec<file::Model>, u64)> {
    let base = File::find().filter(apply_folder_condition(
        active_scope_condition(scope),
        folder_id,
    ));
    let total = base.clone().count(db).await.map_err(AsterError::from)?;

    if total == 0 || limit == 0 {
        return Ok((vec![], total));
    }

    let is_asc = matches!(sort_order, SortOrder::Asc);

    let mut q = base;
    if let Some((after_value, after_id)) = after {
        let cursor_cond = build_cursor_condition(sort_by, is_asc, &after_value, after_id)?;
        q = q.filter(cursor_cond);
    }

    let primary_col = match sort_by {
        SortBy::Name => file::Column::Name,
        SortBy::Size => file::Column::Size,
        SortBy::CreatedAt => file::Column::CreatedAt,
        SortBy::UpdatedAt => file::Column::UpdatedAt,
        SortBy::Type => file::Column::MimeType,
    };

    q = if is_asc {
        q.order_by_asc(primary_col).order_by_asc(file::Column::Id)
    } else {
        q.order_by_desc(primary_col).order_by_desc(file::Column::Id)
    };

    let items = q.limit(limit).all(db).await.map_err(AsterError::from)?;
    Ok((items, total))
}

pub async fn find_by_folder_cursor<C: ConnectionTrait>(
    db: &C,
    user_id: i64,
    folder_id: Option<i64>,
    limit: u64,
    after: Option<(String, i64)>,
    sort_by: SortBy,
    sort_order: SortOrder,
) -> Result<(Vec<file::Model>, u64)> {
    find_by_folder_cursor_in_scope(
        db,
        FileScope::Personal { user_id },
        folder_id,
        limit,
        after,
        sort_by,
        sort_order,
    )
    .await
}

pub async fn find_by_team_folder_cursor<C: ConnectionTrait>(
    db: &C,
    team_id: i64,
    folder_id: Option<i64>,
    limit: u64,
    after: Option<(String, i64)>,
    sort_by: SortBy,
    sort_order: SortOrder,
) -> Result<(Vec<file::Model>, u64)> {
    find_by_folder_cursor_in_scope(
        db,
        FileScope::Team { team_id },
        folder_id,
        limit,
        after,
        sort_by,
        sort_order,
    )
    .await
}

/// 构建 cursor WHERE 条件
/// ASC:  (col > val) OR (col = val AND id > after_id)
/// DESC: (col < val) OR (col = val AND id < after_id)
fn build_cursor_condition(
    sort_by: SortBy,
    is_asc: bool,
    after_value: &str,
    after_id: i64,
) -> Result<Condition> {
    let id_cond = if is_asc {
        file::Column::Id.gt(after_id)
    } else {
        file::Column::Id.lt(after_id)
    };

    match sort_by {
        SortBy::Name => {
            let val = after_value.to_string();
            let (gt, eq) = if is_asc {
                (
                    file::Column::Name.gt(val.clone()),
                    file::Column::Name.eq(val),
                )
            } else {
                (
                    file::Column::Name.lt(val.clone()),
                    file::Column::Name.eq(val),
                )
            };
            Ok(Condition::any()
                .add(gt)
                .add(Condition::all().add(eq).add(id_cond)))
        }
        SortBy::Size => {
            let val: i64 = after_value.parse().map_aster_err_with(|| {
                AsterError::validation_error("invalid cursor value for size sort")
            })?;
            let (gt, eq) = if is_asc {
                (file::Column::Size.gt(val), file::Column::Size.eq(val))
            } else {
                (file::Column::Size.lt(val), file::Column::Size.eq(val))
            };
            Ok(Condition::any()
                .add(gt)
                .add(Condition::all().add(eq).add(id_cond)))
        }
        SortBy::CreatedAt => {
            let val: chrono::DateTime<chrono::Utc> =
                after_value.parse().map_aster_err_with(|| {
                    AsterError::validation_error("invalid cursor value for created_at sort")
                })?;
            let (gt, eq) = if is_asc {
                (
                    file::Column::CreatedAt.gt(val),
                    file::Column::CreatedAt.eq(val),
                )
            } else {
                (
                    file::Column::CreatedAt.lt(val),
                    file::Column::CreatedAt.eq(val),
                )
            };
            Ok(Condition::any()
                .add(gt)
                .add(Condition::all().add(eq).add(id_cond)))
        }
        SortBy::UpdatedAt => {
            let val: chrono::DateTime<chrono::Utc> =
                after_value.parse().map_aster_err_with(|| {
                    AsterError::validation_error("invalid cursor value for updated_at sort")
                })?;
            let (gt, eq) = if is_asc {
                (
                    file::Column::UpdatedAt.gt(val),
                    file::Column::UpdatedAt.eq(val),
                )
            } else {
                (
                    file::Column::UpdatedAt.lt(val),
                    file::Column::UpdatedAt.eq(val),
                )
            };
            Ok(Condition::any()
                .add(gt)
                .add(Condition::all().add(eq).add(id_cond)))
        }
        SortBy::Type => {
            let val = after_value.to_string();
            let (gt, eq) = if is_asc {
                (
                    file::Column::MimeType.gt(val.clone()),
                    file::Column::MimeType.eq(val),
                )
            } else {
                (
                    file::Column::MimeType.lt(val.clone()),
                    file::Column::MimeType.eq(val),
                )
            };
            Ok(Condition::any()
                .add(gt)
                .add(Condition::all().add(eq).add(id_cond)))
        }
    }
}

/// 按名称查文件（排除已删除）
async fn find_by_name_in_folder_in_scope<C: ConnectionTrait>(
    db: &C,
    scope: FileScope,
    folder_id: Option<i64>,
    name: &str,
) -> Result<Option<file::Model>> {
    let exact = File::find()
        .filter(apply_folder_condition(
            active_scope_condition(scope),
            folder_id,
        ))
        .filter(file::Column::Name.eq(name))
        .one(db)
        .await
        .map_err(AsterError::from)?;
    if exact.is_some() {
        return Ok(exact);
    }

    let normalized_name = crate::utils::normalize_name(name);
    Ok(find_by_folder_in_scope(db, scope, folder_id)
        .await?
        .into_iter()
        .find(|file| crate::utils::normalize_name(&file.name) == normalized_name))
}

async fn find_by_names_in_folder_in_scope<C: ConnectionTrait>(
    db: &C,
    scope: FileScope,
    folder_id: Option<i64>,
    names: &[String],
) -> Result<Vec<file::Model>> {
    if names.is_empty() {
        return Ok(vec![]);
    }

    File::find()
        .filter(apply_folder_condition(
            active_scope_condition(scope),
            folder_id,
        ))
        .filter(file::Column::Name.is_in(names.iter().cloned()))
        .all(db)
        .await
        .map_err(AsterError::from)
}

async fn find_names_by_names_in_folder_in_scope<C: ConnectionTrait>(
    db: &C,
    scope: FileScope,
    folder_id: Option<i64>,
    names: &[String],
) -> Result<Vec<String>> {
    if names.is_empty() {
        return Ok(vec![]);
    }

    File::find()
        .select_only()
        .column(file::Column::Name)
        .filter(apply_folder_condition(
            active_scope_condition(scope),
            folder_id,
        ))
        .filter(file::Column::Name.is_in(names.iter().cloned()))
        .into_tuple::<String>()
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_by_name_in_folder<C: ConnectionTrait>(
    db: &C,
    user_id: i64,
    folder_id: Option<i64>,
    name: &str,
) -> Result<Option<file::Model>> {
    find_by_name_in_folder_in_scope(db, FileScope::Personal { user_id }, folder_id, name).await
}

pub async fn find_by_name_in_team_folder<C: ConnectionTrait>(
    db: &C,
    team_id: i64,
    folder_id: Option<i64>,
    name: &str,
) -> Result<Option<file::Model>> {
    find_by_name_in_folder_in_scope(db, FileScope::Team { team_id }, folder_id, name).await
}

pub async fn find_by_names_in_folder<C: ConnectionTrait>(
    db: &C,
    user_id: i64,
    folder_id: Option<i64>,
    names: &[String],
) -> Result<Vec<file::Model>> {
    find_by_names_in_folder_in_scope(db, FileScope::Personal { user_id }, folder_id, names).await
}

pub async fn find_by_names_in_team_folder<C: ConnectionTrait>(
    db: &C,
    team_id: i64,
    folder_id: Option<i64>,
    names: &[String],
) -> Result<Vec<file::Model>> {
    find_by_names_in_folder_in_scope(db, FileScope::Team { team_id }, folder_id, names).await
}

fn unique_filename_candidate_error(name: &str) -> AsterError {
    AsterError::validation_error(format!(
        "failed to resolve a unique file name candidate for '{name}'"
    ))
}

fn checked_candidate_copy_number(
    normalized_name: &str,
    start_copy_number: u32,
    offset: usize,
) -> Result<u32> {
    let offset =
        u32::try_from(offset).map_err(|_| unique_filename_candidate_error(normalized_name))?;
    start_copy_number
        .checked_add(offset)
        .ok_or_else(|| unique_filename_candidate_error(normalized_name))
}

fn build_copy_filename_candidate_batch(
    template: &crate::utils::CopyNameTemplate,
    normalized_name: &str,
    start_copy_number: u32,
    count: usize,
) -> Result<Vec<String>> {
    let mut candidates = Vec::with_capacity(count);
    for offset in 0..count {
        let copy_number =
            checked_candidate_copy_number(normalized_name, start_copy_number, offset)?;
        candidates.push(crate::utils::format_copy_name(template, copy_number));
    }
    Ok(candidates)
}

fn build_unique_filename_candidates(normalized_name: &str) -> Result<Vec<String>> {
    let template = crate::utils::copy_name_template(normalized_name);
    let mut candidates = Vec::with_capacity(UNIQUE_FILENAME_CANDIDATE_BATCH_SIZE);
    candidates.push(normalized_name.to_string());
    candidates.extend(build_copy_filename_candidate_batch(
        &template,
        normalized_name,
        template.next_copy_number,
        UNIQUE_FILENAME_CANDIDATE_BATCH_SIZE - 1,
    )?);

    Ok(candidates)
}

fn push_unique_normalization_variant(variants: &mut Vec<String>, variant: &str) {
    if variants.iter().all(|existing| existing.as_str() != variant) {
        variants.push(variant.to_string());
    }
}

fn push_unique_owned_normalization_variant(variants: &mut Vec<String>, variant: String) {
    if variants
        .iter()
        .all(|existing| existing.as_str() != variant.as_str())
    {
        variants.push(variant);
    }
}

fn add_normalization_query_variants(names: &[String]) -> Cow<'_, [String]> {
    if names.iter().all(|name| name.is_ascii()) {
        return Cow::Borrowed(names);
    }

    let mut variants = Vec::with_capacity(names.len());
    for name in names {
        push_unique_normalization_variant(&mut variants, name);
        if name.is_ascii() {
            continue;
        }
        if !is_nfc(name) {
            push_unique_owned_normalization_variant(&mut variants, name.nfc().collect());
        }
        if !is_nfd(name) {
            push_unique_owned_normalization_variant(&mut variants, name.nfd().collect());
        }
    }
    Cow::Owned(variants)
}

fn normalize_existing_filename(name: String) -> String {
    if name.is_ascii() || is_nfc(&name) {
        name
    } else {
        name.nfc().collect()
    }
}

/// 基于当前目录快照建议一个不冲突的文件名：
/// 如果 `name` 已存在则递增 " (1)", " (2)" ...
///
/// 注意：这里故意只做“读当前快照并给出候选名”，不承诺并发写入下该名字
/// 在后续 `INSERT` 时仍然可用。真正创建文件时，调用方必须继续依赖数据库
/// live-name 唯一索引兜底，并在唯一约束冲突时自动推进到下一个副本名。
async fn resolve_unique_filename_in_scope<C: ConnectionTrait>(
    db: &C,
    scope: FileScope,
    folder_id: Option<i64>,
    name: &str,
) -> Result<String> {
    let normalized_name = crate::utils::normalize_validate_name(name)?;
    let candidates = build_unique_filename_candidates(&normalized_name)?;
    let query_names = add_normalization_query_variants(&candidates);
    let existing_candidate_names: HashSet<String> =
        find_names_by_names_in_folder_in_scope(db, scope, folder_id, query_names.as_ref())
            .await?
            .into_iter()
            .map(normalize_existing_filename)
            .collect();

    if let Some(candidate) = candidates
        .into_iter()
        .find(|candidate| !existing_candidate_names.contains(candidate.as_str()))
    {
        return Ok(candidate);
    }

    let template = crate::utils::copy_name_template(&normalized_name);
    let mut next_copy_number = checked_candidate_copy_number(
        &normalized_name,
        template.next_copy_number,
        UNIQUE_FILENAME_CANDIDATE_BATCH_SIZE - 1,
    )?;
    loop {
        let candidates = build_copy_filename_candidate_batch(
            &template,
            &normalized_name,
            next_copy_number,
            UNIQUE_FILENAME_CANDIDATE_BATCH_SIZE,
        )?;
        let query_names = add_normalization_query_variants(&candidates);
        let existing_names: HashSet<String> =
            find_names_by_names_in_folder_in_scope(db, scope, folder_id, query_names.as_ref())
                .await?
                .into_iter()
                .map(normalize_existing_filename)
                .collect();

        if let Some(candidate) = candidates
            .into_iter()
            .find(|candidate| !existing_names.contains(candidate.as_str()))
        {
            return Ok(candidate);
        }

        next_copy_number = checked_candidate_copy_number(
            &normalized_name,
            next_copy_number,
            UNIQUE_FILENAME_CANDIDATE_BATCH_SIZE,
        )?;
    }
}

/// 基于当前目录快照建议一个可用文件名。
///
/// 这个 helper 不持锁，也不保证调用方随后立刻 `INSERT` 就一定成功；
/// 并发写入场景仍然必须依赖 live-name 唯一索引兜底，并在唯一键冲突时
/// 自动推进到下一个副本名重试。
pub async fn resolve_unique_filename<C: ConnectionTrait>(
    db: &C,
    user_id: i64,
    folder_id: Option<i64>,
    name: &str,
) -> Result<String> {
    resolve_unique_filename_in_scope(db, FileScope::Personal { user_id }, folder_id, name).await
}

/// 团队空间版本的 `resolve_unique_filename()`。
pub async fn resolve_unique_team_filename<C: ConnectionTrait>(
    db: &C,
    team_id: i64,
    folder_id: Option<i64>,
    name: &str,
) -> Result<String> {
    resolve_unique_filename_in_scope(db, FileScope::Team { team_id }, folder_id, name).await
}

#[cfg(test)]
mod tests {
    use super::{add_normalization_query_variants, normalize_existing_filename};
    use std::borrow::Cow;

    #[test]
    fn normalization_query_variants_borrow_ascii_candidates() {
        let names = vec!["report.txt".to_string(), "report (1).txt".to_string()];
        let variants = add_normalization_query_variants(&names);

        assert!(matches!(variants, Cow::Borrowed(_)));
        assert_eq!(variants.as_ref(), names.as_slice());
    }

    #[test]
    fn normalization_query_variants_add_unicode_forms_only_when_needed() {
        let names = vec![
            "caf\u{00e9}.txt".to_string(),
            "cafe\u{0301}.txt".to_string(),
        ];
        let variants = add_normalization_query_variants(&names);

        assert!(matches!(variants, Cow::Owned(_)));
        assert_eq!(variants.as_ref().len(), 2);
        assert!(variants.as_ref().contains(&"caf\u{00e9}.txt".to_string()));
        assert!(variants.as_ref().contains(&"cafe\u{0301}.txt".to_string()));
    }

    #[test]
    fn normalize_existing_filename_reuses_ascii_and_nfc_names() {
        assert_eq!(
            normalize_existing_filename("report.txt".to_string()),
            "report.txt"
        );
        assert_eq!(
            normalize_existing_filename("caf\u{00e9}.txt".to_string()),
            "caf\u{00e9}.txt"
        );
        assert_eq!(
            normalize_existing_filename("cafe\u{0301}.txt".to_string()),
            "caf\u{00e9}.txt"
        );
    }
}
