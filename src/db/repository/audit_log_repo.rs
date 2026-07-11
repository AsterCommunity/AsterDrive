//! 仓储模块：`audit_log_repo`。

use chrono::{DateTime, Utc};
use sea_orm::{
    ColumnTrait, Condition, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter,
    QueryOrder, QuerySelect, Select,
};

use crate::api::pagination::{AdminAuditLogSortBy, SortOrder};
use crate::entities::audit_log as product_audit_log;
use crate::errors::{AsterError, Result};
use crate::types::AuditAction;
use aster_forge_db::audit_log::{self, Entity as AuditLog};
use aster_forge_db::sort::{order_by_column_with_id, order_by_id};

pub struct AuditLogQuery<'a> {
    pub user_id: Option<i64>,
    pub action: Option<&'a str>,
    pub entity_type: Option<&'a str>,
    pub entity_id: Option<i64>,
    pub after: Option<DateTime<Utc>>,
    pub before: Option<DateTime<Utc>>,
    pub limit: u64,
    pub offset: u64,
    pub sort_by: AdminAuditLogSortBy,
    pub sort_order: SortOrder,
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
        .map(product_audit_log::Model::try_from)
        .collect::<Result<Vec<_>>>()?;

    Ok((items, total))
}

impl TryFrom<aster_forge_db::audit_log::Model> for product_audit_log::Model {
    type Error = AsterError;

    fn try_from(value: aster_forge_db::audit_log::Model) -> Result<Self> {
        let action = AuditAction::from_str_name(&value.action).ok_or_else(|| {
            AsterError::database_operation(format!(
                "unsupported audit action in audit log row {}: {}",
                value.id, value.action
            ))
        })?;

        Ok(Self {
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

/// Cursor page for admin overview daily aggregation.
///
/// Overview only needs `action` and `created_at`, but the cursor also carries
/// `id` so rows sharing the same timestamp are scanned exactly once without
/// offset pagination. This keeps memory bounded even when the audit retention
/// window contains a large number of events.
pub async fn find_action_page_in_range(
    db: &DatabaseConnection,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    after: Option<(DateTime<Utc>, i64)>,
    limit: u64,
) -> Result<Vec<(i64, String, DateTime<Utc>)>> {
    let mut query = AuditLog::find()
        .select_only()
        .column(audit_log::Column::Id)
        .column(audit_log::Column::Action)
        .column(audit_log::Column::CreatedAt)
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
        .into_tuple::<(i64, String, DateTime<Utc>)>()
        .all(db)
        .await
        .map_err(AsterError::from)
}
