//! 仓储模块：`audit_log_repo`。

use chrono::{DateTime, Utc};
use sea_orm::{
    ColumnTrait, Condition, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter,
    QueryOrder, QuerySelect, Select,
};

use crate::api::pagination::AdminAuditLogSortBy;
use crate::errors::{AsterError, Result};
use aster_drive_model::entities::audit_log as product_audit_log;
use aster_drive_model::types::AuditAction;
use aster_forge_api::SortOrder;
use aster_forge_db::audit_log::{self, Entity as AuditLog};
use aster_forge_db::sort::{order_by_column_with_id, order_by_id};

/// Filters, ordering, and paging for admin audit log queries.
pub struct AuditLogQuery<'a> {
    /// Restricts results to actions performed by this user.
    pub user_id: Option<i64>,
    /// Exact action name to match.
    pub action: Option<&'a str>,
    /// Exact entity type name to match.
    pub entity_type: Option<&'a str>,
    /// Restricts results to this entity id.
    pub entity_id: Option<i64>,
    /// Inclusive lower bound on the event creation time.
    pub after: Option<DateTime<Utc>>,
    /// Inclusive upper bound on the event creation time.
    pub before: Option<DateTime<Utc>>,
    /// Maximum number of rows returned.
    pub limit: u64,
    /// Number of rows skipped before the first returned row.
    pub offset: u64,
    /// Result ordering key.
    pub sort_by: AdminAuditLogSortBy,
    /// Result ordering direction.
    pub sort_order: SortOrder,
}

/// Filters and ordering for audit log export runs.
#[derive(Debug, Clone)]
pub struct AuditLogExportQuery {
    /// Restricts results to actions performed by this user.
    pub user_id: Option<i64>,
    /// Exact action name to match.
    pub action: Option<String>,
    /// Exact entity type name to match.
    pub entity_type: Option<String>,
    /// Restricts results to this entity id.
    pub entity_id: Option<i64>,
    /// Inclusive lower bound on the event creation time.
    pub after: Option<DateTime<Utc>>,
    /// Inclusive upper bound on the event creation time.
    pub before: Option<DateTime<Utc>>,
    /// Result ordering key.
    pub sort_by: AdminAuditLogSortBy,
    /// Result ordering direction.
    pub sort_order: SortOrder,
}

/// Consistency boundary for one audit log export run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuditLogExportSnapshot {
    /// Largest audit log id included in the export; later rows are excluded.
    pub max_id: i64,
    /// Total number of rows matching the export filters up to `max_id`.
    pub total: u64,
}

/// 带过滤条件的分页查询
pub async fn find_with_filters(
    db: &DatabaseConnection,
    query: AuditLogQuery<'_>,
) -> Result<(Vec<product_audit_log::Model>, u64)> {
    let mut q = apply_admin_audit_log_sort(AuditLog::find(), query.sort_by, query.sort_order);

    if let Some(uid) = query.user_id {
        q = q.filter(audit_log::Column::UserId.eq(uid));
    }
    if let Some(act) = query.action {
        q = q.filter(audit_log::Column::Action.eq(act));
    }
    if let Some(et) = query.entity_type {
        q = q.filter(audit_log::Column::EntityType.eq(et));
    }
    if let Some(eid) = query.entity_id {
        q = q.filter(audit_log::Column::EntityId.eq(eid));
    }
    if let Some(after) = query.after {
        q = q.filter(audit_log::Column::CreatedAt.gte(after));
    }
    if let Some(before) = query.before {
        q = q.filter(audit_log::Column::CreatedAt.lte(before));
    }

    let total = q.clone().count(db).await.map_err(AsterError::from)?;
    let items = q
        .limit(query.limit)
        .offset(query.offset)
        .all(db)
        .await
        .map_err(AsterError::from)?;

    let items = items
        .into_iter()
        .map(product_audit_log_from_forge)
        .collect::<Result<Vec<_>>>()?;

    Ok((items, total))
}

fn product_audit_log_from_forge(
    value: aster_forge_db::audit_log::Model,
) -> Result<product_audit_log::Model> {
    let action = AuditAction::from_str_name(&value.action).ok_or_else(|| {
        AsterError::database_operation(format!(
            "unsupported audit action in audit log row {}: {}",
            value.id, value.action
        ))
    })?;

    Ok(product_audit_log::Model {
        id: value.id,
        user_id: value.user_id,
        action,
        entity_type: value.entity_type,
        entity_id: value.entity_id,
        entity_name: value.entity_name,
        details: value.details,
        ip_address: value.ip_address,
        user_agent: value.user_agent,
        created_at: value.created_at,
    })
}

fn apply_admin_audit_log_sort(
    query: Select<AuditLog>,
    sort_by: AdminAuditLogSortBy,
    sort_order: SortOrder,
) -> Select<AuditLog> {
    match sort_by {
        AdminAuditLogSortBy::Id => order_by_id(query, audit_log::Column::Id, sort_order),
        AdminAuditLogSortBy::CreatedAt => order_by_column_with_id(
            query,
            audit_log::Column::CreatedAt,
            sort_order,
            audit_log::Column::Id,
        ),
        AdminAuditLogSortBy::UserId => order_by_column_with_id(
            query,
            audit_log::Column::UserId,
            sort_order,
            audit_log::Column::Id,
        ),
        AdminAuditLogSortBy::Action => order_by_column_with_id(
            query,
            audit_log::Column::Action,
            sort_order,
            audit_log::Column::Id,
        ),
        AdminAuditLogSortBy::EntityType => order_by_column_with_id(
            query,
            audit_log::Column::EntityType,
            sort_order,
            audit_log::Column::Id,
        ),
        AdminAuditLogSortBy::EntityName => order_by_column_with_id(
            query,
            audit_log::Column::EntityName,
            sort_order,
            audit_log::Column::Id,
        ),
        AdminAuditLogSortBy::IpAddress => order_by_column_with_id(
            query,
            audit_log::Column::IpAddress,
            sort_order,
            audit_log::Column::Id,
        ),
    }
}

fn apply_export_filters(
    mut query: Select<AuditLog>,
    filters: &AuditLogExportQuery,
) -> Select<AuditLog> {
    if let Some(user_id) = filters.user_id {
        query = query.filter(audit_log::Column::UserId.eq(user_id));
    }
    if let Some(action) = filters.action.as_deref() {
        query = query.filter(audit_log::Column::Action.eq(action));
    }
    if let Some(entity_type) = filters.entity_type.as_deref() {
        query = query.filter(audit_log::Column::EntityType.eq(entity_type));
    }
    if let Some(entity_id) = filters.entity_id {
        query = query.filter(audit_log::Column::EntityId.eq(entity_id));
    }
    if let Some(after) = filters.after {
        query = query.filter(audit_log::Column::CreatedAt.gte(after));
    }
    if let Some(before) = filters.before {
        query = query.filter(audit_log::Column::CreatedAt.lte(before));
    }
    query
}

/// Captures the id boundary and row total for a filtered export run.
///
/// Returns `None` when no rows match the filters; otherwise the snapshot bounds every
/// later page so rows inserted during the export are not included.
pub async fn export_snapshot(
    db: &DatabaseConnection,
    filters: &AuditLogExportQuery,
) -> Result<Option<AuditLogExportSnapshot>> {
    let max_id = apply_export_filters(AuditLog::find(), filters)
        .select_only()
        .column(audit_log::Column::Id)
        .order_by_desc(audit_log::Column::Id)
        .limit(1)
        .into_tuple::<i64>()
        .one(db)
        .await
        .map_err(AsterError::from)?;
    let Some(max_id) = max_id else {
        return Ok(None);
    };

    let total = apply_export_filters(AuditLog::find(), filters)
        .filter(audit_log::Column::Id.lte(max_id))
        .count(db)
        .await
        .map_err(AsterError::from)?;

    Ok(Some(AuditLogExportSnapshot { max_id, total }))
}

fn id_cursor_condition(id: i64, sort_order: SortOrder) -> Condition {
    match sort_order {
        SortOrder::Asc => Condition::all().add(audit_log::Column::Id.gt(id)),
        SortOrder::Desc => Condition::all().add(audit_log::Column::Id.lt(id)),
    }
}

fn non_null_cursor_condition<V>(
    column: audit_log::Column,
    value: V,
    id: i64,
    sort_order: SortOrder,
) -> Condition
where
    V: Clone + Into<sea_orm::Value>,
{
    match sort_order {
        SortOrder::Asc => Condition::any().add(column.gt(value.clone())).add(
            Condition::all()
                .add(column.eq(value))
                .add(audit_log::Column::Id.gt(id)),
        ),
        SortOrder::Desc => Condition::any().add(column.lt(value.clone())).add(
            Condition::all()
                .add(column.eq(value))
                .add(audit_log::Column::Id.lt(id)),
        ),
    }
}

fn nullable_string_cursor_condition(
    column: audit_log::Column,
    value: Option<&str>,
    id: i64,
    sort_order: SortOrder,
) -> Condition {
    let id_condition = match sort_order {
        SortOrder::Asc => audit_log::Column::Id.gt(id),
        SortOrder::Desc => audit_log::Column::Id.lt(id),
    };
    let Some(value) = value else {
        return Condition::all().add(column.is_null()).add(id_condition);
    };

    let ordered_after = match sort_order {
        SortOrder::Asc => column.gt(value),
        SortOrder::Desc => column.lt(value),
    };
    Condition::any()
        .add(column.is_null())
        .add(ordered_after)
        .add(Condition::all().add(column.eq(value)).add(id_condition))
}

fn apply_export_sort(
    mut query: Select<AuditLog>,
    sort_by: AdminAuditLogSortBy,
    sort_order: SortOrder,
) -> Select<AuditLog> {
    let column = match sort_by {
        AdminAuditLogSortBy::Id => return order_by_id(query, audit_log::Column::Id, sort_order),
        AdminAuditLogSortBy::CreatedAt => audit_log::Column::CreatedAt,
        AdminAuditLogSortBy::UserId => audit_log::Column::UserId,
        AdminAuditLogSortBy::Action => audit_log::Column::Action,
        AdminAuditLogSortBy::EntityType => audit_log::Column::EntityType,
        AdminAuditLogSortBy::EntityName => {
            query = query.order_by_asc(audit_log::Column::EntityName.is_null());
            audit_log::Column::EntityName
        }
        AdminAuditLogSortBy::IpAddress => {
            query = query.order_by_asc(audit_log::Column::IpAddress.is_null());
            audit_log::Column::IpAddress
        }
    };
    order_by_column_with_id(query, column, sort_order, audit_log::Column::Id)
}

fn export_cursor_condition(
    cursor: &product_audit_log::Model,
    sort_by: AdminAuditLogSortBy,
    sort_order: SortOrder,
) -> Condition {
    match sort_by {
        AdminAuditLogSortBy::Id => id_cursor_condition(cursor.id, sort_order),
        AdminAuditLogSortBy::CreatedAt => non_null_cursor_condition(
            audit_log::Column::CreatedAt,
            cursor.created_at,
            cursor.id,
            sort_order,
        ),
        AdminAuditLogSortBy::UserId => non_null_cursor_condition(
            audit_log::Column::UserId,
            cursor.user_id,
            cursor.id,
            sort_order,
        ),
        AdminAuditLogSortBy::Action => non_null_cursor_condition(
            audit_log::Column::Action,
            cursor.action.as_str(),
            cursor.id,
            sort_order,
        ),
        AdminAuditLogSortBy::EntityType => non_null_cursor_condition(
            audit_log::Column::EntityType,
            cursor.entity_type.as_str(),
            cursor.id,
            sort_order,
        ),
        AdminAuditLogSortBy::EntityName => nullable_string_cursor_condition(
            audit_log::Column::EntityName,
            cursor.entity_name.as_deref(),
            cursor.id,
            sort_order,
        ),
        AdminAuditLogSortBy::IpAddress => nullable_string_cursor_condition(
            audit_log::Column::IpAddress,
            cursor.ip_address.as_deref(),
            cursor.id,
            sort_order,
        ),
    }
}

/// Returns one cursor page of an export run bounded by the given snapshot.
///
/// When `cursor` is `Some`, only rows strictly after the cursor row in the export sort
/// order are returned; the snapshot's `max_id` always bounds the page.
pub async fn find_export_page(
    db: &DatabaseConnection,
    filters: &AuditLogExportQuery,
    snapshot: AuditLogExportSnapshot,
    cursor: Option<&product_audit_log::Model>,
    limit: u64,
) -> Result<Vec<product_audit_log::Model>> {
    let mut query = apply_export_filters(AuditLog::find(), filters)
        .filter(audit_log::Column::Id.lte(snapshot.max_id));
    if let Some(cursor) = cursor {
        query = query.filter(export_cursor_condition(
            cursor,
            filters.sort_by,
            filters.sort_order,
        ));
    }
    let rows = apply_export_sort(query, filters.sort_by, filters.sort_order)
        .limit(limit)
        .all(db)
        .await
        .map_err(AsterError::from)?;
    rows.into_iter().map(product_audit_log_from_forge).collect()
}

/// Cursor page for admin overview daily aggregation.
///
/// Overview reads `action` and `created_at`, plus batch-delete details for operation
/// correlation (other potentially large payloads stay in the database). The cursor carries
/// `id` so rows sharing the same timestamp are scanned exactly once without
/// offset pagination. This keeps memory bounded even when the audit retention
/// window contains a large number of events.
pub async fn find_action_page_in_range(
    db: &DatabaseConnection,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    after: Option<(DateTime<Utc>, i64)>,
    limit: u64,
) -> Result<Vec<(i64, String, DateTime<Utc>, Option<String>)>> {
    let mut query = AuditLog::find()
        .select_only()
        .column(audit_log::Column::Id)
        .column(audit_log::Column::Action)
        .column(audit_log::Column::CreatedAt)
        .expr_as(
            sea_orm::sea_query::Expr::case(
                audit_log::Column::Action.eq(AuditAction::BatchDelete.as_str()),
                sea_orm::sea_query::Expr::col(audit_log::Column::Details),
            )
            .finally(Option::<String>::None),
            "details",
        )
        .filter(audit_log::Column::CreatedAt.gte(start))
        .filter(audit_log::Column::CreatedAt.lt(end))
        .order_by_asc(audit_log::Column::CreatedAt)
        .order_by_asc(audit_log::Column::Id)
        .limit(limit);

    if let Some((created_at, id)) = after {
        query = query.filter(
            Condition::any()
                .add(audit_log::Column::CreatedAt.gt(created_at))
                .add(
                    Condition::all()
                        .add(audit_log::Column::CreatedAt.eq(created_at))
                        .add(audit_log::Column::Id.gt(id)),
                ),
        );
    }

    query
        .into_tuple::<(i64, String, DateTime<Utc>, Option<String>)>()
        .all(db)
        .await
        .map_err(AsterError::from)
}
