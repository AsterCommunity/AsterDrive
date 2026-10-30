use sea_orm::{
    ColumnTrait, Condition, ConnectionTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
    QuerySelect,
};

use crate::api::pagination::SortBy;
use crate::errors::{AsterError, MapAsterErr, Result};
use aster_drive_model::entities::file::{self, Entity as File};
use aster_forge_api::SortOrder;

use crate::db::repository::file_repo::common::{
    FileScope, apply_folder_condition, scope_condition,
};

/// Resource-scoped cursor selection shared by ordinary and governed administrator reads.
pub(crate) struct FileCursorQuery {
    pub folder_id: Option<i64>,
    pub limit: u64,
    pub after: Option<(String, i64)>,
    pub sort_by: SortBy,
    pub sort_order: SortOrder,
    pub deleted: bool,
}
pub(crate) async fn find_by_folder_cursor_in_scope<C: ConnectionTrait>(
    db: &C,
    scope: FileScope,
    query: FileCursorQuery,
) -> Result<(Vec<file::Model>, u64)> {
    let FileCursorQuery {
        folder_id,
        limit,
        after,
        sort_by,
        sort_order,
        deleted,
    } = query;
    let base = File::find().filter(apply_folder_condition(
        scope_condition(scope).add(if deleted {
            file::Column::DeletedAt.is_not_null()
        } else {
            file::Column::DeletedAt.is_null()
        }),
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
        FileCursorQuery {
            folder_id,
            limit,
            after,
            sort_by,
            sort_order,
            deleted: false,
        },
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
        FileCursorQuery {
            folder_id,
            limit,
            after,
            sort_by,
            sort_order,
            deleted: false,
        },
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
