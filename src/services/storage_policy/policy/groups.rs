//! 存储策略服务子模块：`groups`。

use chrono::Utc;
use sea_orm::{Set, TransactionTrait};

use crate::api::pagination::{AdminPolicyGroupSortBy, OffsetPage, SortOrder, load_offset_page};
use crate::db::repository::{policy_group_repo, policy_repo, team_repo, user_repo};
use crate::entities::{storage_policy_group, storage_policy_group_item};
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::runtime::SharedRuntimeState;

use super::models::{
    CreateStoragePolicyGroupInput, PolicyGroupAssignmentMigrationResult, StoragePolicyGroupInfo,
    UpdateStoragePolicyGroupInput,
};
use super::shared::{
    build_group_info, format_group_assignment_blocker, lock_default_group_assignment,
    replace_group_items, validate_group_items,
};

pub async fn ensure_policy_groups_seeded<C>(db: &C) -> Result<()>
where
    C: sea_orm::ConnectionTrait + TransactionTrait,
{
    let default_policy = match policy_repo::find_default(db).await? {
        Some(policy) => policy,
        None => return Ok(()),
    };

    let txn = crate::db::transaction::begin(db).await?;
    let result: Result<()> = async {
        let default_group = match policy_group_repo::find_default_group(&txn).await? {
            Some(group) => {
                let items = policy_group_repo::find_group_items(&txn, group.id).await?;
                if items.is_empty() {
                    policy_group_repo::create_group_item(
                        &txn,
                        storage_policy_group_item::ActiveModel {
                            group_id: Set(group.id),
                            policy_id: Set(default_policy.id),
                            priority: Set(1),
                            min_file_size: Set(0),
                            max_file_size: Set(0),
                            created_at: Set(Utc::now()),
                            ..Default::default()
                        },
                    )
                    .await?;
                }
                group
            }
            None => {
                let now = Utc::now();
                let group = policy_group_repo::create_group(
                    &txn,
                    storage_policy_group::ActiveModel {
                        name: Set("Default Policy Group".to_string()),
                        description: Set(
                            "System default storage policy group created automatically".to_string(),
                        ),
                        is_enabled: Set(true),
                        is_default: Set(false),
                        created_at: Set(now),
                        updated_at: Set(now),
                        ..Default::default()
                    },
                )
                .await?;
                policy_group_repo::create_group_item(
                    &txn,
                    storage_policy_group_item::ActiveModel {
                        group_id: Set(group.id),
                        policy_id: Set(default_policy.id),
                        priority: Set(1),
                        min_file_size: Set(0),
                        max_file_size: Set(0),
                        created_at: Set(now),
                        ..Default::default()
                    },
                )
                .await?;
                group
            }
        };
        lock_default_group_assignment(&txn).await?;
        policy_group_repo::set_only_default_group(&txn, default_group.id).await?;

        user_repo::assign_policy_group_to_unassigned(&txn, default_group.id, Utc::now())
            .await
            .map_aster_err(AsterError::database_operation)?;

        Ok(())
    }
    .await;

    result?;
    crate::db::transaction::commit(txn).await
}

pub async fn list_groups_paginated(
    state: &impl SharedRuntimeState,
    limit: u64,
    offset: u64,
    sort_by: AdminPolicyGroupSortBy,
    sort_order: SortOrder,
) -> Result<OffsetPage<StoragePolicyGroupInfo>> {
    let page = load_offset_page(limit, offset, 100, |limit, offset| async move {
        policy_group_repo::find_groups_paginated(
            state.reader_db(),
            limit,
            offset,
            sort_by,
            sort_order,
        )
        .await
    })
    .await?;
    Ok(OffsetPage {
        items: page
            .items
            .iter()
            .map(|group| build_group_info(state, group))
            .collect(),
        total: page.total,
        limit: page.limit,
        offset: page.offset,
    })
}

pub async fn get_group(state: &impl SharedRuntimeState, id: i64) -> Result<StoragePolicyGroupInfo> {
    let group = policy_group_repo::find_group_by_id(state.reader_db(), id).await?;
    Ok(build_group_info(state, &group))
}

pub async fn create_group(
    state: &impl SharedRuntimeState,
    input: CreateStoragePolicyGroupInput,
) -> Result<StoragePolicyGroupInfo> {
    let CreateStoragePolicyGroupInput {
        name,
        description,
        is_enabled,
        is_default,
        items,
    } = input;
    if is_default && !is_enabled {
        return Err(AsterError::validation_error(
            "default storage policy group must be enabled",
        ));
    }

    validate_group_items(state.writer_db(), &items).await?;

    let txn = crate::db::transaction::begin(state.writer_db()).await?;
    let now = Utc::now();
    let group = policy_group_repo::create_group(
        &txn,
        storage_policy_group::ActiveModel {
            name: Set(name),
            description: Set(description.unwrap_or_default()),
            is_enabled: Set(is_enabled),
            is_default: Set(false),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        },
    )
    .await?;
    replace_group_items(&txn, group.id, &items).await?;
    if is_default {
        lock_default_group_assignment(&txn).await?;
        policy_group_repo::set_only_default_group(&txn, group.id).await?;
    }
    crate::db::transaction::commit(txn).await?;
    state.policy_snapshot().reload(state.writer_db()).await?;
    let group = policy_group_repo::find_group_by_id(state.writer_db(), group.id).await?;
    Ok(build_group_info(state, &group))
}

pub async fn update_group(
    state: &impl SharedRuntimeState,
    id: i64,
    input: UpdateStoragePolicyGroupInput,
) -> Result<StoragePolicyGroupInfo> {
    let UpdateStoragePolicyGroupInput {
        name,
        description,
        is_enabled,
        is_default,
        items,
    } = input;
    let txn = crate::db::transaction::begin(state.writer_db()).await?;
    let existing = policy_group_repo::find_group_by_id(&txn, id).await?;
    let next_is_enabled = is_enabled.unwrap_or(existing.is_enabled);
    let next_is_default = is_default.unwrap_or(existing.is_default);

    if let Some(false) = is_enabled {
        if next_is_default {
            return Err(AsterError::validation_error(
                "cannot disable the default storage policy group; set another group as default first",
            ));
        }

        if existing.is_enabled {
            let user_assignment_count =
                policy_group_repo::count_user_group_assignments(&txn, id).await?;
            let team_assignment_count = team_repo::count_active_by_policy_group(&txn, id).await?;
            if let Some(message) = format_group_assignment_blocker(
                "disable",
                user_assignment_count,
                team_assignment_count,
            ) {
                return Err(AsterError::validation_error(message));
            }
        }
    }

    if let Some(true) = is_default
        && !next_is_enabled
    {
        return Err(AsterError::validation_error(
            "default storage policy group must be enabled",
        ));
    }

    if let Some(false) = is_default
        && existing.is_default
    {
        let all = policy_group_repo::find_all_groups(&txn).await?;
        let default_count = all.iter().filter(|group| group.is_default).count();
        if default_count <= 1 {
            return Err(AsterError::validation_error(
                "cannot unset the only default storage policy group",
            ));
        }
    }

    if let Some(ref updated_items) = items {
        validate_group_items(&txn, updated_items).await?;
    }

    let mut active: storage_policy_group::ActiveModel = existing.into();
    if let Some(value) = name {
        active.name = Set(value);
    }
    if let Some(value) = description {
        active.description = Set(value);
    }
    if let Some(value) = is_enabled {
        active.is_enabled = Set(value);
    }
    if let Some(value) = is_default {
        active.is_default = Set(value);
    }
    active.updated_at = Set(Utc::now());
    let group = policy_group_repo::update_group(&txn, active).await?;

    if let Some(updated_items) = items {
        replace_group_items(&txn, group.id, &updated_items).await?;
    }

    if is_default == Some(true) {
        lock_default_group_assignment(&txn).await?;
        policy_group_repo::set_only_default_group(&txn, group.id).await?;
    }

    crate::db::transaction::commit(txn).await?;
    state.policy_snapshot().reload(state.writer_db()).await?;
    let group = policy_group_repo::find_group_by_id(state.writer_db(), group.id).await?;
    Ok(build_group_info(state, &group))
}

pub async fn delete_group(state: &impl SharedRuntimeState, id: i64) -> Result<()> {
    let group = policy_group_repo::find_group_by_id(state.writer_db(), id).await?;
    tracing::debug!(
        policy_group_id = id,
        policy_group_name = %group.name,
        is_default = group.is_default,
        "deleting storage policy group"
    );

    if group.is_default {
        let all = policy_group_repo::find_all_groups(state.writer_db()).await?;
        let default_count = all.iter().filter(|item| item.is_default).count();
        if default_count <= 1 {
            return Err(AsterError::validation_error(
                "cannot delete the only default storage policy group",
            ));
        }
    }

    let user_assignment_count =
        policy_group_repo::count_user_group_assignments(state.writer_db(), id).await?;
    let team_assignment_count =
        team_repo::count_active_by_policy_group(state.writer_db(), id).await?;
    if let Some(message) =
        format_group_assignment_blocker("delete", user_assignment_count, team_assignment_count)
    {
        return Err(AsterError::validation_error(message));
    }

    policy_group_repo::delete_group(state.writer_db(), id).await?;
    state.policy_snapshot().reload(state.writer_db()).await?;
    tracing::info!(
        policy_group_id = id,
        policy_group_name = %group.name,
        "deleted storage policy group"
    );
    Ok(())
}

pub async fn migrate_group_assignments(
    state: &impl SharedRuntimeState,
    source_group_id: i64,
    target_group_id: i64,
) -> Result<PolicyGroupAssignmentMigrationResult> {
    if source_group_id == target_group_id {
        return Err(AsterError::validation_error(
            "source and target storage policy groups must be different",
        ));
    }

    policy_group_repo::find_group_by_id(state.writer_db(), source_group_id).await?;
    let target_group =
        policy_group_repo::find_group_by_id(state.writer_db(), target_group_id).await?;
    if !target_group.is_enabled {
        return Err(AsterError::validation_error(
            "cannot migrate assignments to a disabled storage policy group",
        ));
    }
    if policy_group_repo::find_group_items(state.writer_db(), target_group_id)
        .await?
        .is_empty()
    {
        return Err(AsterError::validation_error(
            "cannot migrate assignments to a storage policy group without policies",
        ));
    }

    let now = Utc::now();
    let txn = crate::db::transaction::begin(state.writer_db()).await?;
    let affected_users =
        user_repo::migrate_policy_group_assignments(&txn, source_group_id, target_group_id, now)
            .await
            .map_aster_err(AsterError::database_operation)?;
    let affected_teams =
        team_repo::migrate_policy_group_assignments(&txn, source_group_id, target_group_id, now)
            .await
            .map_aster_err(AsterError::database_operation)?;

    crate::db::transaction::commit(txn).await?;
    let migrated_assignments = affected_users.checked_add(affected_teams).ok_or_else(|| {
        AsterError::internal_error("policy group migration assignment count overflow")
    })?;
    if migrated_assignments == 0 {
        return Ok(PolicyGroupAssignmentMigrationResult {
            source_group_id,
            target_group_id,
            affected_users: 0,
            affected_teams: 0,
            migrated_assignments: 0,
        });
    }
    state.policy_snapshot().reload(state.writer_db()).await?;

    Ok(PolicyGroupAssignmentMigrationResult {
        source_group_id,
        target_group_id,
        affected_users,
        affected_teams,
        migrated_assignments,
    })
}
