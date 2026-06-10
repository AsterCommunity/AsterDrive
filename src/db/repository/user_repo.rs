//! 仓储模块：`user_repo`。

use crate::api::pagination::{AdminUserSortBy, SortOrder};
use crate::db::repository::pagination_repo::fetch_offset_page;
use crate::db::repository::search_query::{
    escape_like_query, lower_like_condition, mysql_boolean_mode_query, sqlite_fts_match_condition,
    sqlite_match_query,
};
use crate::db::repository::sort::{order_by_column_with_id, order_by_id};
use crate::entities::user::{self, Entity as User};
use crate::errors::{AsterError, Result};
use crate::types::{UserRole, UserStatus};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, ConnectionTrait, DatabaseConnection, DbBackend,
    EntityTrait, ExprTrait, PaginatorTrait, QueryFilter, QueryOrder, Select,
    sea_query::{Expr, extension::postgres::PgExpr},
};

const SQLITE_USERS_FTS_TABLE: &str = "users_search_fts";

#[derive(Debug, Clone, Copy)]
pub struct AdminUserListFilters<'a> {
    pub keyword: Option<&'a str>,
    pub role: Option<UserRole>,
    pub status: Option<UserStatus>,
    pub sort_by: AdminUserSortBy,
    pub sort_order: SortOrder,
}

fn user_keyword_like_condition(query: &str) -> Condition {
    Condition::any()
        .add(lower_like_condition(user::Column::Username, query))
        .add(lower_like_condition(user::Column::Email, query))
}

fn user_keyword_condition(backend: DbBackend, query: &str) -> Condition {
    match backend {
        DbBackend::Postgres => {
            let pattern = format!("%{}%", escape_like_query(query));
            Condition::any()
                .add(Expr::col(user::Column::Username).ilike(pattern.clone()))
                .add(Expr::col(user::Column::Email).ilike(pattern))
        }
        DbBackend::MySql => mysql_boolean_mode_query(query)
            .map(|boolean_query| {
                Condition::all().add(Expr::cust_with_exprs(
                    "MATCH(?, ?) AGAINST (? IN BOOLEAN MODE)",
                    [
                        Expr::col(user::Column::Username),
                        Expr::col(user::Column::Email),
                        Expr::val(boolean_query),
                    ],
                ))
            })
            .unwrap_or_else(|| user_keyword_like_condition(query)),
        DbBackend::Sqlite => sqlite_match_query(query)
            .map(|match_query| {
                Condition::all().add(sqlite_fts_match_condition(
                    user::Column::Id,
                    SQLITE_USERS_FTS_TABLE,
                    &match_query,
                ))
            })
            .unwrap_or_else(|| user_keyword_like_condition(query)),
        _ => user_keyword_like_condition(query),
    }
}

pub async fn find_by_id<C: ConnectionTrait>(db: &C, id: i64) -> Result<user::Model> {
    User::find_by_id(id)
        .one(db)
        .await
        .map_err(AsterError::from)?
        .ok_or_else(|| AsterError::record_not_found(format!("user #{id}")))
}

pub async fn find_by_ids<C: ConnectionTrait>(db: &C, ids: &[i64]) -> Result<Vec<user::Model>> {
    if ids.is_empty() {
        return Ok(vec![]);
    }

    User::find()
        .filter(user::Column::Id.is_in(ids.iter().copied()))
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_by_username<C: ConnectionTrait>(
    db: &C,
    username: &str,
) -> Result<Option<user::Model>> {
    User::find()
        .filter(user::Column::Username.eq(username))
        .one(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_by_email<C: ConnectionTrait>(db: &C, email: &str) -> Result<Option<user::Model>> {
    User::find()
        .filter(user::Column::Email.eq(email))
        .one(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_by_pending_email<C: ConnectionTrait>(
    db: &C,
    email: &str,
) -> Result<Option<user::Model>> {
    User::find()
        .filter(user::Column::PendingEmail.eq(email))
        .one(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_all<C: ConnectionTrait>(db: &C) -> Result<Vec<user::Model>> {
    User::find()
        .order_by_asc(user::Column::Id)
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_by_policy_group<C: ConnectionTrait>(
    db: &C,
    policy_group_id: i64,
) -> Result<Vec<user::Model>> {
    User::find()
        .filter(user::Column::PolicyGroupId.eq(policy_group_id))
        .order_by_asc(user::Column::Id)
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn count_by_policy_group<C: ConnectionTrait>(
    db: &C,
    policy_group_id: i64,
) -> Result<u64> {
    User::find()
        .filter(user::Column::PolicyGroupId.eq(policy_group_id))
        .count(db)
        .await
        .map_err(AsterError::from)
}

pub async fn assign_policy_group_to_unassigned<C: ConnectionTrait>(
    db: &C,
    policy_group_id: i64,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<u64> {
    let result = User::update_many()
        .col_expr(
            user::Column::PolicyGroupId,
            Expr::value(Some(policy_group_id)),
        )
        .col_expr(user::Column::UpdatedAt, Expr::value(now))
        .filter(user::Column::PolicyGroupId.is_null())
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected)
}

pub async fn migrate_policy_group_assignments<C: ConnectionTrait>(
    db: &C,
    source_group_id: i64,
    target_group_id: i64,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<u64> {
    let result = User::update_many()
        .col_expr(
            user::Column::PolicyGroupId,
            Expr::value(Some(target_group_id)),
        )
        .col_expr(user::Column::UpdatedAt, Expr::value(now))
        .filter(user::Column::PolicyGroupId.eq(source_group_id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected)
}

pub async fn find_paginated(
    db: &DatabaseConnection,
    limit: u64,
    offset: u64,
    filters: &AdminUserListFilters<'_>,
) -> Result<(Vec<user::Model>, u64)> {
    let backend = db.get_database_backend();
    let keyword = filters
        .keyword
        .map(str::trim)
        .filter(|keyword| !keyword.is_empty());

    let mut q = apply_admin_user_sort(User::find(), filters.sort_by, filters.sort_order);

    if let Some(keyword) = keyword {
        q = q.filter(user_keyword_condition(backend, keyword));
    }
    if let Some(role) = filters.role {
        q = q.filter(user::Column::Role.eq(role));
    }
    if let Some(status) = filters.status {
        q = q.filter(user::Column::Status.eq(status));
    }

    fetch_offset_page(db, q, limit, offset).await
}

fn apply_admin_user_sort(
    query: Select<User>,
    sort_by: AdminUserSortBy,
    sort_order: SortOrder,
) -> Select<User> {
    match sort_by {
        AdminUserSortBy::Id => order_by_id(query, user::Column::Id, sort_order),
        AdminUserSortBy::Username => {
            order_by_column_with_id(query, user::Column::Username, sort_order, user::Column::Id)
        }
        AdminUserSortBy::Email => {
            order_by_column_with_id(query, user::Column::Email, sort_order, user::Column::Id)
        }
        AdminUserSortBy::Role => {
            order_by_column_with_id(query, user::Column::Role, sort_order, user::Column::Id)
        }
        AdminUserSortBy::Status => {
            order_by_column_with_id(query, user::Column::Status, sort_order, user::Column::Id)
        }
        AdminUserSortBy::StorageUsed => order_by_column_with_id(
            query,
            user::Column::StorageUsed,
            sort_order,
            user::Column::Id,
        ),
        AdminUserSortBy::StorageQuota => order_by_column_with_id(
            query,
            user::Column::StorageQuota,
            sort_order,
            user::Column::Id,
        ),
        AdminUserSortBy::CreatedAt => {
            order_by_column_with_id(query, user::Column::CreatedAt, sort_order, user::Column::Id)
        }
        AdminUserSortBy::UpdatedAt => {
            order_by_column_with_id(query, user::Column::UpdatedAt, sort_order, user::Column::Id)
        }
    }
}

pub async fn count_all(db: &DatabaseConnection) -> Result<u64> {
    User::find().count(db).await.map_err(AsterError::from)
}

/// 按状态统计用户数
pub async fn count_by_status(db: &DatabaseConnection, status: UserStatus) -> Result<u64> {
    User::find()
        .filter(user::Column::Status.eq(status))
        .count(db)
        .await
        .map_err(AsterError::from)
}

pub async fn create<C: ConnectionTrait>(db: &C, model: user::ActiveModel) -> Result<user::Model> {
    model.insert(db).await.map_err(AsterError::from)
}

pub async fn delete<C: ConnectionTrait>(db: &C, id: i64) -> Result<()> {
    let result = User::delete_by_id(id)
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    if result.rows_affected == 0 {
        return Err(AsterError::record_not_found(format!("user #{id}")));
    }
    Ok(())
}

/// 检查用户配额是否足够。quota=0 表示不限。
///
/// 注意：这只是 fast-fail 预检，并发场景下两个请求可能同时通过此检查后超额。
/// 真正的原子保证在 [`update_storage_used`] 内通过 SQL CAS 完成。
pub async fn check_quota<C: ConnectionTrait>(db: &C, user_id: i64, needed_size: i64) -> Result<()> {
    let user = find_by_id(db, user_id).await?;
    let projected_storage_used = user.storage_used.checked_add(needed_size).ok_or_else(|| {
        AsterError::internal_error(format!(
            "user storage usage overflow: used {}, delta {}",
            user.storage_used, needed_size
        ))
    })?;
    if user.storage_quota > 0 && projected_storage_used > user.storage_quota {
        return Err(AsterError::storage_quota_exceeded(format!(
            "quota {}, used {}, need {}",
            user.storage_quota, user.storage_used, needed_size
        )));
    }
    Ok(())
}

/// 原子更新用户已用配额。
///
/// 正向增量（delta > 0）受配额上限保护：SQL 层用 `WHERE storage_used + delta <= storage_quota`
/// 做 compare-and-swap，并发请求中只有不超额的才会成功提交。
///
/// 配额超额时返回 [`AsterError::storage_quota_exceeded`]，记录不存在时返回
/// [`AsterError::record_not_found`]，调用方需要根据错误类型区分处理。
pub async fn update_storage_used<C: ConnectionTrait>(db: &C, id: i64, delta: i64) -> Result<()> {
    let expr = if delta >= 0 {
        Expr::col(user::Column::StorageUsed).add(delta)
    } else {
        let decrement_by = -delta;
        Expr::case(Expr::col(user::Column::StorageUsed).lt(decrement_by), 0)
            .finally(Expr::col(user::Column::StorageUsed).sub(decrement_by))
            .into()
    };

    let mut query = User::update_many()
        .col_expr(user::Column::StorageUsed, expr)
        .filter(user::Column::Id.eq(id));

    if delta >= 0 {
        // 正向增量必须满足：quota=0（不限）或 used + delta <= quota
        query = query.filter(
            Condition::any().add(user::Column::StorageQuota.eq(0)).add(
                Expr::col(user::Column::StorageUsed)
                    .add(delta)
                    .lte(Expr::col(user::Column::StorageQuota)),
            ),
        );
    }

    let result = query.exec(db).await.map_err(AsterError::from)?;

    if result.rows_affected == 0 {
        // 0 行受影响有两种可能：用户不存在，或者并发场景下 CAS 失败（超配额）
        if delta >= 0 {
            let user = find_by_id(db, id).await?;
            let projected_storage_used = user.storage_used.checked_add(delta).ok_or_else(|| {
                AsterError::internal_error(format!(
                    "user storage usage overflow: used {}, delta {}",
                    user.storage_used, delta
                ))
            })?;
            if user.storage_quota > 0 && projected_storage_used > user.storage_quota {
                return Err(AsterError::storage_quota_exceeded(format!(
                    "quota {}, used {}, need {}",
                    user.storage_quota, user.storage_used, delta
                )));
            }
        }
        return Err(AsterError::record_not_found(format!("user #{id}")));
    }

    Ok(())
}

pub async fn set_storage_used<C: ConnectionTrait>(db: &C, id: i64, value: i64) -> Result<()> {
    let result = User::update_many()
        .col_expr(user::Column::StorageUsed, Expr::value(value))
        .filter(user::Column::Id.eq(id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;

    if result.rows_affected == 0 {
        return Err(AsterError::record_not_found(format!("user #{id}")));
    }

    Ok(())
}

pub async fn bump_session_version<C: ConnectionTrait>(db: &C, id: i64) -> Result<()> {
    let result = User::update_many()
        .col_expr(
            user::Column::SessionVersion,
            Expr::col(user::Column::SessionVersion).add(1i64),
        )
        .filter(user::Column::Id.eq(id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;

    if result.rows_affected == 0 {
        return Err(AsterError::record_not_found(format!("user #{id}")));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm::{DbBackend, QueryTrait};

    #[test]
    fn postgres_user_keyword_condition_uses_ilike() {
        let sql: String = format!(
            "{}",
            User::find()
                .filter(user_keyword_condition(DbBackend::Postgres, "alice"))
                .build(DbBackend::Postgres)
        );

        assert!(
            sql.as_str().contains(r#""username" ILIKE '%alice%'"#),
            "{sql}"
        );
        assert!(sql.as_str().contains(r#""email" ILIKE '%alice%'"#), "{sql}");
    }

    #[test]
    fn mysql_user_keyword_condition_uses_match_against() {
        let sql: String = format!(
            "{}",
            User::find()
                .filter(user_keyword_condition(DbBackend::MySql, "alice"))
                .build(DbBackend::MySql)
        );

        assert!(sql.as_str().contains("MATCH("), "{sql}");
        assert!(sql.as_str().contains("`username`"), "{sql}");
        assert!(sql.as_str().contains("`email`"), "{sql}");
        assert!(
            sql.as_str()
                .contains(r#"AGAINST ('\"alice\"' IN BOOLEAN MODE)"#),
            "{sql}"
        );
    }

    #[test]
    fn mysql_user_keyword_condition_falls_back_to_like_for_punctuation() {
        let sql: String = format!(
            "{}",
            User::find()
                .filter(user_keyword_condition(DbBackend::MySql, "end-u"))
                .build(DbBackend::MySql)
        );

        assert!(!sql.as_str().contains("MATCH("), "{sql}");
        assert!(
            sql.as_str().contains("LOWER(`username`) LIKE '%end-u%'"),
            "{sql}"
        );
        assert!(
            sql.as_str().contains("LOWER(`email`) LIKE '%end-u%'"),
            "{sql}"
        );
    }

    #[test]
    fn postgres_update_storage_used_sql_is_valid() {
        let sql: String = format!(
            "{}",
            User::update_many()
                .col_expr(
                    user::Column::StorageUsed,
                    Expr::col(user::Column::StorageUsed).add(1i64),
                )
                .filter(user::Column::Id.eq(7))
                .build(DbBackend::Postgres)
        );

        assert!(
            sql.as_str()
                .contains(r#""storage_used" = "storage_used" + 1"#),
            "{sql}"
        );
        assert!(sql.as_str().contains(r#"WHERE "users"."id" = 7"#), "{sql}");
    }

    #[test]
    fn postgres_bump_session_version_sql_is_valid() {
        let sql: String = format!(
            "{}",
            User::update_many()
                .col_expr(
                    user::Column::SessionVersion,
                    Expr::col(user::Column::SessionVersion).add(1i64),
                )
                .filter(user::Column::Id.eq(9))
                .build(DbBackend::Postgres)
        );

        assert!(
            sql.as_str()
                .contains(r#""session_version" = "session_version" + 1"#),
            "{sql}"
        );
        assert!(sql.as_str().contains(r#"WHERE "users"."id" = 9"#), "{sql}");
    }
}
