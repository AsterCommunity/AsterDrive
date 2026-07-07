//! 存储策略服务子模块：`shared`。

use chrono::Utc;
use sea_orm::Set;
use validator::Validate;

use crate::db::repository::{policy_group_repo, policy_repo};
use crate::entities::{storage_policy_group, storage_policy_group_item};
use crate::errors::{AsterError, Result};
use crate::runtime::SharedRuntimeState;
use crate::types::{
    StoragePolicyOptions, StoredStoragePolicyAllowedTypes, StoredStoragePolicyOptions,
    serialize_storage_policy_allowed_types, serialize_storage_policy_options,
};

use super::models::{
    StoragePolicyGroupInfo, StoragePolicyGroupItemInfo, StoragePolicyGroupItemInput,
    StoragePolicySummaryInfo,
};

pub(super) const SYSTEM_STORAGE_POLICY_ID: i64 = 1;

pub(super) fn serialize_allowed_types(
    allowed_types: &[String],
) -> Result<StoredStoragePolicyAllowedTypes> {
    serialize_storage_policy_allowed_types(allowed_types).map_err(|error| {
        AsterError::internal_error(format!("serialize storage policy allowed_types: {error}"))
    })
}

pub(super) fn serialize_options(
    options: &StoragePolicyOptions,
) -> Result<StoredStoragePolicyOptions> {
    let options = options.clone().normalized();
    options
        .validate()
        .map_err(|error| AsterError::validation_error(error.to_string()))?;
    serialize_storage_policy_options(&options).map_err(|error| {
        AsterError::internal_error(format!("serialize storage policy options: {error}"))
    })
}

pub(super) fn format_group_assignment_blocker(
    action: &str,
    user_assignment_count: u64,
    team_assignment_count: u64,
) -> Option<String> {
    let mut refs = Vec::new();
    if user_assignment_count > 0 {
        refs.push(format!(
            "{user_assignment_count} user assignment(s) still reference it"
        ));
    }
    if team_assignment_count > 0 {
        refs.push(format!(
            "{team_assignment_count} team assignment(s) still reference it"
        ));
    }

    if refs.is_empty() {
        return None;
    }

    Some(format!(
        "cannot {action} policy group: {}",
        refs.join(" and ")
    ))
}

pub(super) fn build_group_info(
    state: &impl SharedRuntimeState,
    group: &storage_policy_group::Model,
) -> StoragePolicyGroupInfo {
    let items = state
        .policy_snapshot()
        .get_policy_group_items(group.id)
        .into_iter()
        .map(|resolved| {
            let policy = resolved.policy;
            StoragePolicyGroupItemInfo {
                id: resolved.item.id,
                policy_id: resolved.item.policy_id,
                priority: resolved.item.priority,
                min_file_size: resolved.item.min_file_size,
                max_file_size: resolved.item.max_file_size,
                policy: StoragePolicySummaryInfo {
                    id: policy.id,
                    name: policy.name,
                    driver_type: policy.driver_type,
                },
            }
        })
        .collect();

    StoragePolicyGroupInfo {
        id: group.id,
        name: group.name.clone(),
        description: group.description.clone(),
        is_enabled: group.is_enabled,
        is_default: group.is_default,
        created_at: group.created_at,
        updated_at: group.updated_at,
        items,
    }
}

pub(super) async fn validate_group_items<C: sea_orm::ConnectionTrait>(
    db: &C,
    items: &[StoragePolicyGroupItemInput],
) -> Result<()> {
    if items.is_empty() {
        return Err(AsterError::validation_error(
            "storage policy group must contain at least one policy",
        ));
    }

    let mut seen_policies = std::collections::HashSet::new();
    let mut seen_priorities = std::collections::HashSet::new();
    for item in items {
        if item.priority <= 0 {
            return Err(AsterError::validation_error(
                "group item priority must be greater than 0",
            ));
        }
        if item.min_file_size < 0 || item.max_file_size < 0 {
            return Err(AsterError::validation_error(
                "file size rules must be non-negative",
            ));
        }
        if item.max_file_size != 0 && item.max_file_size <= item.min_file_size {
            return Err(AsterError::validation_error(
                "max_file_size must be greater than min_file_size",
            ));
        }
        if !seen_policies.insert(item.policy_id) {
            return Err(AsterError::validation_error(
                "duplicate policy_id in storage policy group items",
            ));
        }
        if !seen_priorities.insert(item.priority) {
            return Err(AsterError::validation_error(
                "duplicate priority in storage policy group items",
            ));
        }
        policy_repo::find_by_id(db, item.policy_id).await?;
    }

    Ok(())
}

pub(super) async fn replace_group_items<C: sea_orm::ConnectionTrait>(
    db: &C,
    group_id: i64,
    items: &[StoragePolicyGroupItemInput],
) -> Result<()> {
    policy_group_repo::delete_group_items_by_group(db, group_id).await?;
    let now = Utc::now();
    for item in items {
        policy_group_repo::create_group_item(
            db,
            storage_policy_group_item::ActiveModel {
                group_id: Set(group_id),
                policy_id: Set(item.policy_id),
                priority: Set(item.priority),
                min_file_size: Set(item.min_file_size),
                max_file_size: Set(item.max_file_size),
                created_at: Set(now),
                ..Default::default()
            },
        )
        .await?;
    }
    Ok(())
}

pub(super) async fn lock_default_group_assignment<C: sea_orm::ConnectionTrait>(
    db: &C,
) -> Result<()> {
    policy_repo::lock_by_id(db, SYSTEM_STORAGE_POLICY_ID).await?;
    Ok(())
}

pub(super) async fn ensure_singleton_group_for_policy<C: sea_orm::ConnectionTrait>(
    db: &C,
    policy_id: i64,
) -> Result<i64> {
    let singleton_description = format!(
        "Compatibility singleton group for storage policy #{}",
        policy_id
    );
    let groups = policy_group_repo::find_all_groups(db).await?;
    let items = policy_group_repo::find_all_group_items(db).await?;
    let mut items_by_group_id =
        std::collections::HashMap::<i64, Vec<storage_policy_group_item::Model>>::new();
    for item in items {
        items_by_group_id
            .entry(item.group_id)
            .or_default()
            .push(item);
    }
    for group in groups {
        if group.description != singleton_description || !group.is_enabled {
            continue;
        }
        let Some(group_items) = items_by_group_id.get(&group.id) else {
            continue;
        };
        if group_items.len() == 1 && group_items[0].policy_id == policy_id {
            return Ok(group.id);
        }
    }

    let now = Utc::now();
    let policy = policy_repo::find_by_id(db, policy_id).await?;
    let group = policy_group_repo::create_group(
        db,
        storage_policy_group::ActiveModel {
            name: Set(format!("Singleton · {}", policy.name)),
            description: Set(singleton_description),
            is_enabled: Set(true),
            is_default: Set(false),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        },
    )
    .await?;
    policy_group_repo::create_group_item(
        db,
        storage_policy_group_item::ActiveModel {
            group_id: Set(group.id),
            policy_id: Set(policy.id),
            priority: Set(1),
            min_file_size: Set(0),
            max_file_size: Set(0),
            created_at: Set(now),
            ..Default::default()
        },
    )
    .await?;
    Ok(group.id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_group_assignment_blocker_empty_returns_none() {
        assert_eq!(format_group_assignment_blocker("delete", 0, 0), None);
    }

    #[test]
    fn format_group_assignment_blocker_users_only() {
        let msg = format_group_assignment_blocker("delete", 5, 0).unwrap();
        assert!(msg.contains("delete"));
        assert!(msg.contains("5 user"));
        assert!(!msg.contains("team"));
    }

    #[test]
    fn format_group_assignment_blocker_teams_only() {
        let msg = format_group_assignment_blocker("disable", 0, 3).unwrap();
        assert!(msg.contains("disable"));
        assert!(msg.contains("3 team"));
    }

    #[test]
    fn format_group_assignment_blocker_both() {
        let msg = format_group_assignment_blocker("delete", 2, 4).unwrap();
        assert!(msg.contains("2 user") || msg.contains("4 team"));
        assert!(msg.contains("and"));
    }
}
