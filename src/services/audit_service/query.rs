use chrono::{Duration, Utc};
use std::collections::{HashMap, HashSet};

use crate::api::pagination::{AdminAuditLogSortBy, OffsetPage, SortOrder, load_offset_page};
use crate::db::repository::audit_log_repo;
use crate::entities::audit_log;
use crate::errors::Result;
use crate::runtime::PrimaryAppState;
use crate::services::{profile_service, user_service};
use crate::types::TeamMemberRole;

use super::filters::AuditLogFilters;
use super::manager::flush_global_audit_log_manager;
use super::models::{AuditLogEntry, TeamAuditEntryInfo};
use super::presentation::build_audit_presentation;

const DEFAULT_RETENTION_DAYS: i64 = 90;

async fn query_models(
    state: &PrimaryAppState,
    filters: AuditLogFilters,
    limit: u64,
    offset: u64,
    sort_by: AdminAuditLogSortBy,
    sort_order: SortOrder,
) -> Result<OffsetPage<audit_log::Model>> {
    flush_global_audit_log_manager().await;
    load_offset_page(limit, offset, 200, |limit, offset| async move {
        audit_log_repo::find_with_filters(
            state.reader_db(),
            audit_log_repo::AuditLogQuery {
                user_id: filters.user_id,
                action: filters.action.as_deref(),
                entity_type: filters.entity_type.map(|entity_type| entity_type.as_str()),
                entity_id: filters.entity_id,
                after: filters.after,
                before: filters.before,
                limit,
                offset,
                sort_by,
                sort_order,
            },
        )
        .await
    })
    .await
}

async fn build_audit_entries(
    state: &PrimaryAppState,
    entries: Vec<audit_log::Model>,
) -> Result<Vec<AuditLogEntry>> {
    let user_ids: Vec<i64> = entries
        .iter()
        .map(|entry| entry.user_id)
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    let users = user_service::user_summaries_by_ids(
        state,
        &user_ids,
        profile_service::AvatarAudience::AdminUser,
    )
    .await?;

    let mut items = Vec::with_capacity(entries.len());

    for model in entries {
        let Some(entity_type) = crate::types::AuditEntityType::from_str_name(&model.entity_type)
        else {
            tracing::warn!(
                audit_log_id = model.id,
                entity_type = %model.entity_type,
                "skipping audit log with unsupported entity_type"
            );
            continue;
        };

        let presentation = build_audit_presentation(
            model.action,
            entity_type,
            model.entity_id,
            model.entity_name.as_deref(),
            model.details.as_deref(),
        );

        items.push(AuditLogEntry {
            id: model.id,
            user: users.get(&model.user_id).cloned(),
            action: model.action,
            entity_type,
            entity_id: model.entity_id,
            entity_name: model.entity_name,
            details: model.details,
            presentation,
            ip_address: model.ip_address,
            user_agent: model.user_agent,
            created_at: model.created_at,
        });
    }

    Ok(items)
}

pub async fn query(
    state: &PrimaryAppState,
    filters: AuditLogFilters,
    limit: u64,
    offset: u64,
    sort_by: AdminAuditLogSortBy,
    sort_order: SortOrder,
) -> Result<OffsetPage<AuditLogEntry>> {
    let page = query_models(state, filters, limit, offset, sort_by, sort_order).await?;
    let items = build_audit_entries(state, page.items).await?;
    Ok(OffsetPage::new(items, page.total, page.limit, page.offset))
}

fn parse_team_member_role(value: Option<&serde_json::Value>) -> Option<TeamMemberRole> {
    serde_json::from_value(value?.clone()).ok()
}

fn parse_i64_field(details: &serde_json::Value, key: &str) -> Option<i64> {
    details.get(key)?.as_i64()
}

fn build_team_audit_entry(
    entry: audit_log::Model,
    users: &HashMap<i64, user_service::UserSummary>,
) -> TeamAuditEntryInfo {
    let entity_type = crate::types::AuditEntityType::from_str_name(&entry.entity_type);
    let parsed_details = entry
        .details
        .as_deref()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok());

    let member_user_id = parsed_details
        .as_ref()
        .and_then(|details| parse_i64_field(details, "member_user_id"));
    let role = parsed_details
        .as_ref()
        .and_then(|details| parse_team_member_role(details.get("role")))
        .or_else(|| {
            parsed_details
                .as_ref()
                .and_then(|details| parse_team_member_role(details.get("removed_role")))
        });
    let previous_role = parsed_details
        .as_ref()
        .and_then(|details| parse_team_member_role(details.get("previous_role")));
    let next_role = parsed_details
        .as_ref()
        .and_then(|details| parse_team_member_role(details.get("next_role")));

    TeamAuditEntryInfo {
        id: entry.id,
        action: entry.action,
        presentation: entity_type.and_then(|entity_type| {
            build_audit_presentation(
                entry.action,
                entity_type,
                entry.entity_id,
                entry.entity_name.as_deref(),
                entry.details.as_deref(),
            )
        }),
        actor: users.get(&entry.user_id).cloned(),
        created_at: entry.created_at,
        member: member_user_id.and_then(|member_user_id| users.get(&member_user_id).cloned()),
        role,
        previous_role,
        next_role,
    }
}

pub async fn query_team_entries(
    state: &PrimaryAppState,
    filters: AuditLogFilters,
    limit: u64,
    offset: u64,
) -> Result<OffsetPage<TeamAuditEntryInfo>> {
    let page = query_models(
        state,
        filters,
        limit,
        offset,
        AdminAuditLogSortBy::CreatedAt,
        SortOrder::Desc,
    )
    .await?;
    let mut user_ids = HashSet::new();
    for entry in &page.items {
        user_ids.insert(entry.user_id);
        if let Some(member_user_id) = entry
            .details
            .as_deref()
            .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
            .as_ref()
            .and_then(|details| parse_i64_field(details, "member_user_id"))
        {
            user_ids.insert(member_user_id);
        }
    }
    let user_ids: Vec<i64> = user_ids.into_iter().collect();
    let users = user_service::user_summaries_by_ids(
        state,
        &user_ids,
        profile_service::AvatarAudience::AdminUser,
    )
    .await?;
    let items = page
        .items
        .into_iter()
        .map(|entry| build_team_audit_entry(entry, &users))
        .collect();

    Ok(OffsetPage::new(items, page.total, page.limit, page.offset))
}

/// 清理过期审计日志
pub async fn cleanup_expired(state: &PrimaryAppState) -> Result<u64> {
    flush_global_audit_log_manager().await;
    let retention_days = state
        .runtime_config
        .get_i64("audit_log_retention_days")
        .unwrap_or_else(|| {
            if let Some(raw) = state.runtime_config.get("audit_log_retention_days") {
                tracing::warn!(
                    "invalid audit_log_retention_days value '{}', using default",
                    raw
                );
            }
            DEFAULT_RETENTION_DAYS
        });

    let cutoff = Utc::now() - Duration::days(retention_days);
    let deleted = audit_log_repo::delete_before(state.writer_db(), cutoff).await?;
    if deleted > 0 {
        tracing::info!("cleaned up {deleted} expired audit log entries");
    }
    Ok(deleted)
}
