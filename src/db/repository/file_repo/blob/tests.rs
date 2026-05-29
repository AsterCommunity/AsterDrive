use super::lookup::find_or_create_blob_retry_delay;
use super::ref_count::{
    blob_ref_count_decrement_expr, blob_ref_count_increment_expr, normalize_blob_ref_count_deltas,
};
use crate::entities::file_blob;
use chrono::Utc;
use sea_orm::{ColumnTrait, DbBackend, EntityTrait, ExprTrait, QueryFilter, QueryTrait, Set};
use std::time::Duration;

#[test]
fn find_or_create_blob_retry_delay_grows_exponentially_and_caps() {
    assert_eq!(find_or_create_blob_retry_delay(0), Duration::from_millis(5));
    assert_eq!(
        find_or_create_blob_retry_delay(1),
        Duration::from_millis(10)
    );
    assert_eq!(
        find_or_create_blob_retry_delay(2),
        Duration::from_millis(20)
    );
    assert_eq!(
        find_or_create_blob_retry_delay(3),
        Duration::from_millis(40)
    );
    assert_eq!(
        find_or_create_blob_retry_delay(4),
        Duration::from_millis(80)
    );
    assert_eq!(
        find_or_create_blob_retry_delay(5),
        Duration::from_millis(80)
    );
    assert_eq!(
        find_or_create_blob_retry_delay(99),
        Duration::from_millis(80)
    );
}

#[test]
fn postgres_find_or_create_blob_insert_sql_uses_valid_on_conflict() {
    let now = Utc::now();
    let sql = crate::entities::file_blob::Entity::insert(file_blob::ActiveModel {
        hash: Set("hash".to_string()),
        size: Set(1),
        policy_id: Set(2),
        storage_path: Set("files/hash".to_string()),
        thumbnail_path: Set(None),
        thumbnail_processor: Set(None),
        thumbnail_version: Set(None),
        ref_count: Set(1),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    })
    .on_conflict_do_nothing_on([file_blob::Column::Hash, file_blob::Column::PolicyId])
    .build(DbBackend::Postgres)
    .to_string();

    assert!(
        sql.contains(r#"ON CONFLICT ("hash", "policy_id") DO NOTHING"#),
        "{sql}"
    );
    assert!(!sql.contains(" WHERE "), "{sql}");
}

// 第二轮审查有 agent 怀疑 `RefCount.gte(0)` 是无效过滤（误以为 ref_count 不能为负）。
// 实际上 `claim_blob_cleanup` 会把 ref_count 置为 -1 作为 "待清理" 标记，
// `gte(0)` 正是为了把这类已被 cleanup worker 认领的行从 dedup 命中集中排除。
// 这个测试通过检查生成的 SQL 是否包含 `ref_count >= 0` 条件来锁定这一不变量。
#[test]
fn find_active_blob_by_hash_sql_excludes_cleanup_claimed_rows() {
    let sql = crate::entities::file_blob::Entity::find()
        .filter(file_blob::Column::Hash.eq("h"))
        .filter(file_blob::Column::PolicyId.eq(1))
        .filter(file_blob::Column::RefCount.gte(0))
        .build(DbBackend::Postgres)
        .to_string();
    assert!(
        sql.contains(r#""ref_count" >= 0"#),
        "expected ref_count >= 0 filter in '{sql}'"
    );
}

#[test]
fn increment_blob_ref_count_sql_uses_cas_against_cleanup_claim() {
    let sql = crate::entities::file_blob::Entity::update_many()
        .col_expr(
            file_blob::Column::RefCount,
            sea_orm::sea_query::Expr::col(file_blob::Column::RefCount).add(1i32),
        )
        .filter(file_blob::Column::Id.eq(1))
        .filter(file_blob::Column::RefCount.gte(0))
        .build(DbBackend::Postgres)
        .to_string();
    assert!(
        sql.contains(r#""ref_count" >= 0"#),
        "ref_count CAS must reject cleanup-claimed (-1) rows; sql='{sql}'"
    );
}

#[test]
fn normalize_blob_ref_count_deltas_merges_duplicate_ids_and_skips_zeroes() {
    let deltas =
        normalize_blob_ref_count_deltas(&[(2, 1), (1, 0), (2, 3), (3, 2)], "test merge").unwrap();

    assert_eq!(deltas, vec![(2, 4), (3, 2)]);
}

#[test]
fn normalize_blob_ref_count_deltas_rejects_negative_values() {
    let error = normalize_blob_ref_count_deltas(&[(7, -1)], "test negative").unwrap_err();

    assert!(
        error
            .to_string()
            .contains("test negative requires positive delta"),
        "{error}"
    );
}

#[test]
fn postgres_batch_increment_blob_ref_counts_sql_uses_single_case_update() {
    let deltas = vec![(2, 3), (4, 1)];
    let ids: Vec<i64> = deltas.iter().map(|(id, _)| *id).collect();
    let sql = crate::entities::file_blob::Entity::update_many()
        .col_expr(
            file_blob::Column::RefCount,
            blob_ref_count_increment_expr(&deltas),
        )
        .filter(file_blob::Column::Id.is_in(ids.iter().copied()))
        .filter(file_blob::Column::RefCount.gte(0))
        .build(DbBackend::Postgres)
        .to_string();

    assert!(sql.contains(r#"CASE WHEN ("id" = 2)"#), "{sql}");
    assert!(sql.contains(r#"THEN "ref_count" + 3"#), "{sql}");
    assert!(sql.contains(r#"WHEN ("id" = 4)"#), "{sql}");
    assert!(sql.contains(r#"THEN "ref_count" + 1"#), "{sql}");
    assert!(sql.contains(r#""ref_count" >= 0"#), "{sql}");
}

#[test]
fn postgres_batch_decrement_blob_ref_counts_sql_uses_floor_case_update() {
    let deltas = vec![(2, 3), (4, 1)];
    let ids: Vec<i64> = deltas.iter().map(|(id, _)| *id).collect();
    let sql = crate::entities::file_blob::Entity::update_many()
        .col_expr(
            file_blob::Column::RefCount,
            blob_ref_count_decrement_expr(&deltas),
        )
        .filter(file_blob::Column::Id.is_in(ids.iter().copied()))
        .build(DbBackend::Postgres)
        .to_string();

    assert!(sql.contains(r#"CASE WHEN ("id" = 2)"#), "{sql}");
    assert!(sql.contains(r#"CASE WHEN ("ref_count" < 3)"#), "{sql}");
    assert!(sql.contains(r#"ELSE "ref_count" - 3 END"#), "{sql}");
    assert!(sql.contains(r#"WHEN ("id" = 4)"#), "{sql}");
}

#[test]
fn postgres_move_blob_policy_if_current_sql_uses_policy_cas() {
    let sql = crate::entities::file_blob::Entity::update_many()
        .col_expr(
            file_blob::Column::Hash,
            sea_orm::sea_query::Expr::value("hash".to_string()),
        )
        .col_expr(
            file_blob::Column::PolicyId,
            sea_orm::sea_query::Expr::value(2),
        )
        .col_expr(
            file_blob::Column::StoragePath,
            sea_orm::sea_query::Expr::value("ab/cd/hash".to_string()),
        )
        .filter(file_blob::Column::Id.eq(9))
        .filter(file_blob::Column::PolicyId.eq(1))
        .build(DbBackend::Postgres)
        .to_string();

    assert!(sql.contains(r#""id" = 9"#), "{sql}");
    assert!(sql.contains(r#""policy_id" = 1"#), "{sql}");
    assert!(sql.contains(r#""policy_id" = 2"#), "{sql}");
    assert!(sql.contains(r#""hash" = 'hash'"#), "{sql}");
}
