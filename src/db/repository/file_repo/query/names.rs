use std::{borrow::Cow, collections::HashSet};

use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QuerySelect};
use unicode_normalization::{UnicodeNormalization, is_nfc, is_nfd};

use crate::entities::file::{self, Entity as File};
use crate::errors::{AsterError, Result};

use crate::db::repository::file_repo::common::FileScope;
use crate::db::repository::file_repo::query::basic::find_by_folder_in_scope;

const UNIQUE_FILENAME_CANDIDATE_BATCH_SIZE: usize = 32;

/// 按名称查文件（排除已删除）
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

pub(super) async fn find_by_name_in_folder_in_scope<C: ConnectionTrait>(
    db: &C,
    scope: FileScope,
    folder_id: Option<i64>,
    name: &str,
) -> Result<Option<file::Model>> {
    let exact = File::find()
        .filter(
            crate::db::repository::file_repo::common::apply_folder_condition(
                crate::db::repository::file_repo::common::active_scope_condition(scope),
                folder_id,
            ),
        )
        .filter(file::Column::Name.eq(name))
        .one(db)
        .await
        .map_err(AsterError::from)?;
    if exact.is_some() {
        return Ok(exact);
    }

    let normalized_name = aster_forge_validation::filename::normalize_name(name);
    Ok(find_by_folder_in_scope(db, scope, folder_id)
        .await?
        .into_iter()
        .find(|file| {
            aster_forge_validation::filename::normalize_name(&file.name) == normalized_name
        }))
}

pub(super) async fn find_by_names_in_folder_in_scope<C: ConnectionTrait>(
    db: &C,
    scope: FileScope,
    folder_id: Option<i64>,
    names: &[String],
) -> Result<Vec<file::Model>> {
    if names.is_empty() {
        return Ok(vec![]);
    }
    let query_names = add_normalization_query_variants(names);
    let normalized_names = normalized_non_ascii_names(names);

    let mut files = File::find()
        .filter(
            crate::db::repository::file_repo::common::apply_folder_condition(
                crate::db::repository::file_repo::common::active_scope_condition(scope),
                folder_id,
            ),
        )
        .filter(file::Column::Name.is_in(query_names.iter().cloned()))
        .all(db)
        .await
        .map_err(AsterError::from)?;

    if !normalized_names.is_empty() {
        let existing_ids: HashSet<i64> = files.iter().map(|file| file.id).collect();
        files.extend(
            find_by_folder_in_scope(db, scope, folder_id)
                .await?
                .into_iter()
                .filter(|file| !existing_ids.contains(&file.id))
                .filter(|file| {
                    normalized_names.contains(&aster_forge_validation::filename::normalize_name(
                        &file.name,
                    ))
                }),
        );
    }

    Ok(files)
}

pub(super) async fn find_names_by_names_in_folder_in_scope<C: ConnectionTrait>(
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
        .filter(
            crate::db::repository::file_repo::common::apply_folder_condition(
                crate::db::repository::file_repo::common::active_scope_condition(scope),
                folder_id,
            ),
        )
        .filter(file::Column::Name.is_in(names.iter().cloned()))
        .into_tuple::<String>()
        .all(db)
        .await
        .map_err(AsterError::from)
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
    template: &aster_forge_validation::filename::CopyNameTemplate,
    normalized_name: &str,
    start_copy_number: u32,
    count: usize,
) -> Result<Vec<String>> {
    let mut candidates = Vec::with_capacity(count);
    for offset in 0..count {
        let copy_number =
            checked_candidate_copy_number(normalized_name, start_copy_number, offset)?;
        candidates.push(aster_forge_validation::filename::format_copy_name(
            template,
            copy_number,
        ));
    }
    Ok(candidates)
}

fn build_unique_filename_candidates(normalized_name: &str) -> Result<Vec<String>> {
    let template = aster_forge_validation::filename::copy_name_template(normalized_name);
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

pub(super) fn add_normalization_query_variants(names: &[String]) -> Cow<'_, [String]> {
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

pub(super) fn normalize_existing_filename(name: String) -> String {
    if name.is_ascii() || is_nfc(&name) {
        name
    } else {
        name.nfc().collect()
    }
}

fn normalized_non_ascii_names(names: &[String]) -> HashSet<String> {
    names
        .iter()
        .filter(|name| !name.is_ascii())
        .map(|name| aster_forge_validation::filename::normalize_name(name))
        .collect()
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
    let normalized_name = aster_forge_validation::filename::normalize_validate_name(name)?;
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

    let template = aster_forge_validation::filename::copy_name_template(&normalized_name);
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
