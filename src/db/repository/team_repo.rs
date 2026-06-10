//! 仓储模块：`team_repo`。

use crate::api::pagination::{AdminTeamSortBy, SortOrder};
use crate::db::repository::pagination_repo::fetch_offset_page;
use crate::db::repository::search_query::{
    escape_like_query, lower_like_condition, mysql_boolean_mode_query, sqlite_fts_match_condition,
    sqlite_match_query,
};
use crate::db::repository::sort::{order_by_column_with_id, order_by_id};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, ConnectionTrait, DatabaseConnection, DbBackend,
    EntityTrait, ExprTrait, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Select,
    sea_query::{Expr, extension::postgres::PgExpr},
};

use crate::entities::team::{self, Entity as Team};
use crate::errors::{AsterError, Result};

const SQLITE_TEAMS_FTS_TABLE: &str = "teams_search_fts";

fn team_keyword_like_condition(query: &str) -> Condition {
    Condition::any()
        .add(lower_like_condition(
            (team::Entity, team::Column::Name),
            query,
        ))
        .add(lower_like_condition(
            (team::Entity, team::Column::Description),
            query,
        ))
}

pub(crate) fn team_keyword_condition(backend: DbBackend, query: &str) -> Condition {
    match backend {
        DbBackend::Postgres => {
            let pattern = format!("%{}%", escape_like_query(query));
            Condition::any()
                .add(Expr::col((team::Entity, team::Column::Name)).ilike(pattern.clone()))
                .add(Expr::col((team::Entity, team::Column::Description)).ilike(pattern))
        }
        DbBackend::MySql => mysql_boolean_mode_query(query)
            .map(|boolean_query| {
                Condition::all().add(Expr::cust_with_exprs(
                    "MATCH(?, ?) AGAINST (? IN BOOLEAN MODE)",
                    [
                        Expr::col((team::Entity, team::Column::Name)),
                        Expr::col((team::Entity, team::Column::Description)),
                        Expr::val(boolean_query),
                    ],
                ))
            })
            .unwrap_or_else(|| team_keyword_like_condition(query)),
        DbBackend::Sqlite => sqlite_match_query(query)
            .map(|match_query| {
                Condition::all().add(sqlite_fts_match_condition(
                    (team::Entity, team::Column::Id),
                    SQLITE_TEAMS_FTS_TABLE,
                    &match_query,
                ))
            })
            .unwrap_or_else(|| team_keyword_like_condition(query)),
        _ => team_keyword_like_condition(query),
    }
}

pub async fn create<C: ConnectionTrait>(db: &C, model: team::ActiveModel) -> Result<team::Model> {
    model.insert(db).await.map_err(AsterError::from)
}

pub async fn update<C: ConnectionTrait>(db: &C, model: team::ActiveModel) -> Result<team::Model> {
    model.update(db).await.map_err(AsterError::from)
}

pub async fn find_by_id<C: ConnectionTrait>(db: &C, id: i64) -> Result<team::Model> {
    Team::find_by_id(id)
        .one(db)
        .await
        .map_err(AsterError::from)?
        .ok_or_else(|| AsterError::record_not_found(format!("team #{id}")))
}

pub async fn find_active_by_id<C: ConnectionTrait>(db: &C, id: i64) -> Result<team::Model> {
    Team::find()
        .filter(team::Column::Id.eq(id))
        .filter(team::Column::ArchivedAt.is_null())
        .one(db)
        .await
        .map_err(AsterError::from)?
        .ok_or_else(|| AsterError::record_not_found(format!("team #{id}")))
}

pub async fn find_archived_by_id<C: ConnectionTrait>(db: &C, id: i64) -> Result<team::Model> {
    Team::find()
        .filter(team::Column::Id.eq(id))
        .filter(team::Column::ArchivedAt.is_not_null())
        .one(db)
        .await
        .map_err(AsterError::from)?
        .ok_or_else(|| AsterError::record_not_found(format!("team #{id}")))
}

pub async fn lock_active_by_id<C: ConnectionTrait>(db: &C, id: i64) -> Result<team::Model> {
    match db.get_database_backend() {
        DbBackend::Postgres | DbBackend::MySql => Team::find()
            .filter(team::Column::Id.eq(id))
            .filter(team::Column::ArchivedAt.is_null())
            .lock_exclusive()
            .one(db)
            .await
            .map_err(AsterError::from)?
            .ok_or_else(|| AsterError::record_not_found(format!("team #{id}"))),
        DbBackend::Sqlite => find_active_by_id(db, id).await,
        _ => find_active_by_id(db, id).await,
    }
}

pub async fn lock_archived_by_id<C: ConnectionTrait>(db: &C, id: i64) -> Result<team::Model> {
    match db.get_database_backend() {
        DbBackend::Postgres | DbBackend::MySql => Team::find()
            .filter(team::Column::Id.eq(id))
            .filter(team::Column::ArchivedAt.is_not_null())
            .lock_exclusive()
            .one(db)
            .await
            .map_err(AsterError::from)?
            .ok_or_else(|| AsterError::record_not_found(format!("team #{id}"))),
        DbBackend::Sqlite => find_archived_by_id(db, id).await,
        _ => find_archived_by_id(db, id).await,
    }
}

pub async fn find_all<C: ConnectionTrait>(db: &C) -> Result<Vec<team::Model>> {
    Team::find()
        .order_by_asc(team::Column::Id)
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_active_paginated(
    db: &DatabaseConnection,
    limit: u64,
    offset: u64,
    keyword: Option<&str>,
    sort_by: AdminTeamSortBy,
    sort_order: SortOrder,
) -> Result<(Vec<team::Model>, u64)> {
    find_paginated_by_archived_state(db, limit, offset, keyword, false, sort_by, sort_order).await
}

pub async fn find_archived_paginated(
    db: &DatabaseConnection,
    limit: u64,
    offset: u64,
    keyword: Option<&str>,
    sort_by: AdminTeamSortBy,
    sort_order: SortOrder,
) -> Result<(Vec<team::Model>, u64)> {
    find_paginated_by_archived_state(db, limit, offset, keyword, true, sort_by, sort_order).await
}

async fn find_paginated_by_archived_state(
    db: &DatabaseConnection,
    limit: u64,
    offset: u64,
    keyword: Option<&str>,
    archived: bool,
    sort_by: AdminTeamSortBy,
    sort_order: SortOrder,
) -> Result<(Vec<team::Model>, u64)> {
    let backend = db.get_database_backend();
    let keyword = keyword.map(str::trim).filter(|keyword| !keyword.is_empty());
    let mut q = apply_admin_team_sort(Team::find(), sort_by, sort_order);

    q = if archived {
        q.filter(team::Column::ArchivedAt.is_not_null())
    } else {
        q.filter(team::Column::ArchivedAt.is_null())
    };

    if let Some(keyword) = keyword {
        q = q.filter(team_keyword_condition(backend, keyword));
    }

    fetch_offset_page(db, q, limit, offset).await
}

fn apply_admin_team_sort(
    query: Select<Team>,
    sort_by: AdminTeamSortBy,
    sort_order: SortOrder,
) -> Select<Team> {
    match sort_by {
        AdminTeamSortBy::Id => order_by_id(query, team::Column::Id, sort_order),
        AdminTeamSortBy::Name => {
            order_by_column_with_id(query, team::Column::Name, sort_order, team::Column::Id)
        }
        AdminTeamSortBy::StorageUsed => order_by_column_with_id(
            query,
            team::Column::StorageUsed,
            sort_order,
            team::Column::Id,
        ),
        AdminTeamSortBy::StorageQuota => order_by_column_with_id(
            query,
            team::Column::StorageQuota,
            sort_order,
            team::Column::Id,
        ),
        AdminTeamSortBy::CreatedAt => {
            order_by_column_with_id(query, team::Column::CreatedAt, sort_order, team::Column::Id)
        }
        AdminTeamSortBy::UpdatedAt => {
            order_by_column_with_id(query, team::Column::UpdatedAt, sort_order, team::Column::Id)
        }
        AdminTeamSortBy::ArchivedAt => order_by_column_with_id(
            query,
            team::Column::ArchivedAt,
            sort_order,
            team::Column::Id,
        ),
    }
}

pub async fn find_archived_before<C: ConnectionTrait>(
    db: &C,
    before: chrono::DateTime<chrono::Utc>,
) -> Result<Vec<team::Model>> {
    Team::find()
        .filter(team::Column::ArchivedAt.is_not_null())
        .filter(team::Column::ArchivedAt.lt(before))
        .order_by_asc(team::Column::ArchivedAt)
        .order_by_asc(team::Column::Id)
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn delete<C: ConnectionTrait>(db: &C, id: i64) -> Result<()> {
    let result = Team::delete_by_id(id)
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    if result.rows_affected == 0 {
        return Err(AsterError::record_not_found(format!("team #{id}")));
    }
    Ok(())
}

pub async fn count_active_by_policy_group<C: ConnectionTrait>(
    db: &C,
    policy_group_id: i64,
) -> Result<u64> {
    Team::find()
        .filter(team::Column::ArchivedAt.is_null())
        .filter(team::Column::PolicyGroupId.eq(policy_group_id))
        .count(db)
        .await
        .map_err(AsterError::from)
}

pub async fn migrate_policy_group_assignments<C: ConnectionTrait>(
    db: &C,
    source_group_id: i64,
    target_group_id: i64,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<u64> {
    // Include archived teams as well: they can later be restored, and restoring
    // a team should not resurrect an obsolete or deleted policy group binding.
    let result = Team::update_many()
        .col_expr(
            team::Column::PolicyGroupId,
            Expr::value(Some(target_group_id)),
        )
        .col_expr(team::Column::UpdatedAt, Expr::value(now))
        .filter(team::Column::PolicyGroupId.eq(source_group_id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected)
}

pub async fn check_quota<C: ConnectionTrait>(db: &C, team_id: i64, needed_size: i64) -> Result<()> {
    let team = find_active_by_id(db, team_id).await?;
    let projected_storage_used = team.storage_used.checked_add(needed_size).ok_or_else(|| {
        AsterError::internal_error(format!(
            "team storage usage overflow: used {}, delta {}",
            team.storage_used, needed_size
        ))
    })?;
    if team.storage_quota > 0 && projected_storage_used > team.storage_quota {
        return Err(AsterError::storage_quota_exceeded(format!(
            "team quota {}, used {}, need {}",
            team.storage_quota, team.storage_used, needed_size
        )));
    }
    Ok(())
}

pub async fn update_storage_used<C: ConnectionTrait>(db: &C, id: i64, delta: i64) -> Result<()> {
    let expr = if delta >= 0 {
        Expr::col(team::Column::StorageUsed).add(delta)
    } else {
        let decrement_by = -delta;
        Expr::case(Expr::col(team::Column::StorageUsed).lt(decrement_by), 0)
            .finally(Expr::col(team::Column::StorageUsed).sub(decrement_by))
            .into()
    };

    let mut query = Team::update_many()
        .col_expr(team::Column::StorageUsed, expr)
        .filter(team::Column::Id.eq(id));

    if delta >= 0 {
        query = query.filter(
            Condition::any().add(team::Column::StorageQuota.eq(0)).add(
                Expr::col(team::Column::StorageUsed)
                    .add(delta)
                    .lte(Expr::col(team::Column::StorageQuota)),
            ),
        );
    }

    let result = query.exec(db).await.map_err(AsterError::from)?;

    if result.rows_affected == 0 {
        if delta >= 0 {
            let team = find_by_id(db, id).await?;
            let projected_storage_used = team.storage_used.checked_add(delta).ok_or_else(|| {
                AsterError::internal_error(format!(
                    "team storage usage overflow: used {}, delta {}",
                    team.storage_used, delta
                ))
            })?;
            if team.storage_quota > 0 && projected_storage_used > team.storage_quota {
                return Err(AsterError::storage_quota_exceeded(format!(
                    "team quota {}, used {}, need {}",
                    team.storage_quota, team.storage_used, delta
                )));
            }
        }
        return Err(AsterError::record_not_found(format!("team #{id}")));
    }

    Ok(())
}

pub async fn set_storage_used<C: ConnectionTrait>(db: &C, id: i64, value: i64) -> Result<()> {
    let result = Team::update_many()
        .col_expr(team::Column::StorageUsed, Expr::value(value))
        .filter(team::Column::Id.eq(id))
        .exec(db)
        .await
        .map_err(AsterError::from)?;

    if result.rows_affected == 0 {
        return Err(AsterError::record_not_found(format!("team #{id}")));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm::{DbBackend, QueryTrait};

    #[test]
    fn postgres_team_keyword_condition_uses_ilike() {
        let sql: String = format!(
            "{}",
            Team::find()
                .filter(team_keyword_condition(DbBackend::Postgres, "ops"))
                .build(DbBackend::Postgres)
        );

        assert!(
            sql.as_str().contains(r#""teams"."name" ILIKE '%ops%'"#),
            "{sql}"
        );
        assert!(
            sql.as_str()
                .contains(r#""teams"."description" ILIKE '%ops%'"#),
            "{sql}"
        );
    }

    #[test]
    fn mysql_team_keyword_condition_uses_match_against() {
        let sql: String = format!(
            "{}",
            Team::find()
                .filter(team_keyword_condition(DbBackend::MySql, "ops"))
                .build(DbBackend::MySql)
        );

        assert!(sql.as_str().contains("MATCH("), "{sql}");
        assert!(sql.as_str().contains("`teams`.`name`"), "{sql}");
        assert!(sql.as_str().contains("`teams`.`description`"), "{sql}");
        assert!(
            sql.as_str()
                .contains(r#"AGAINST ('\"ops\"' IN BOOLEAN MODE)"#),
            "{sql}"
        );
    }

    #[test]
    fn mysql_team_keyword_condition_falls_back_to_like_for_punctuation() {
        let sql: String = format!(
            "{}",
            Team::find()
                .filter(team_keyword_condition(DbBackend::MySql, "ops-core"))
                .build(DbBackend::MySql)
        );

        assert!(!sql.as_str().contains("MATCH("), "{sql}");
        assert!(
            sql.as_str()
                .contains("LOWER(`teams`.`name`) LIKE '%ops-core%'"),
            "{sql}"
        );
        assert!(
            sql.as_str()
                .contains("LOWER(`teams`.`description`) LIKE '%ops-core%'"),
            "{sql}"
        );
    }
}
