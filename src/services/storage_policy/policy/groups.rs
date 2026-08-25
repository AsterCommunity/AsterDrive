//! 存储策略服务子模块：`groups`。

use aster_forge_db::transaction;
use chrono::Utc;
use sea_orm::{Set, TransactionTrait};

use super::placement::{PlacementMatcher, PlacementPayloadEnvelope};
use crate::api::pagination::{AdminPolicyGroupSortBy, load_offset_page};
use crate::db::repository::policy_placement_repo;
use crate::db::repository::{policy_group_repo, policy_repo, team_repo, user_repo};
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::runtime::SharedRuntimeState;
use aster_drive_model::entities::{
    storage_policy_group, storage_policy_group_rule, storage_policy_group_rule_target,
};
use aster_forge_api::{OffsetPage, SortOrder};

use super::models::{
    CreateStoragePolicyGroupInput, PolicyGroupAssignmentMigrationResult, StoragePolicyGroupInfo,
    UpdateStoragePolicyGroupInput,
};
use super::shared::{
    build_group_info, format_group_assignment_blocker, lock_default_group_assignment,
    replace_placement_rules, serialize_admission, validate_placement_rules,
};

pub async fn ensure_policy_groups_seeded<C>(db: &C) -> Result<()>
where
    C: sea_orm::ConnectionTrait + TransactionTrait,
{
    if policy_repo::find_default(db).await?.is_none() {
        return Ok(());
    }

    let txn = transaction::begin(db).await?;
    let result: Result<()> = async {
        // Serialize the complete default-policy/group reconciliation before
        // reading or creating a group. Multiple Primaries run this same startup
        // path, so locking only after group creation would still allow them to
        // create competing default groups.
        lock_default_group_assignment(&txn).await?;
        let default_policy = policy_repo::find_default(&txn).await?.ok_or_else(|| {
            AsterError::internal_error(
                "default storage policy disappeared while reconciling its policy group",
            )
        })?;
        let default_group = match policy_group_repo::find_default_group(&txn).await? {
            Some(group) => {
                let rules = policy_placement_repo::find_all_rules(&txn).await?;
                if !rules.iter().any(|rule| rule.group_id == group.id) {
                    create_default_placement_rule(&txn, group.id, default_policy.id, Utc::now())
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
                        admission_config: Set(serde_json::to_string(
                            &PlacementPayloadEnvelope::new(
                                super::placement::StorageAdmissionConstraints::default(),
                            ),
                        )
                        .map_err(|error| {
                            AsterError::internal_error(format!(
                                "serialize placement admission: {error}"
                            ))
                        })?),
                        upload_execution_preference: Set("automatic".to_string()),
                        routing_revision: Set(1),
                        created_at: Set(now),
                        updated_at: Set(now),
                        ..Default::default()
                    },
                )
                .await?;
                create_default_placement_rule(&txn, group.id, default_policy.id, now).await?;
                group
            }
        };
        policy_group_repo::set_only_default_group(&txn, default_group.id).await?;

        user_repo::assign_policy_group_to_unassigned(&txn, default_group.id, Utc::now())
            .await
            .map_aster_err(AsterError::database_operation)?;

        Ok(())
    }
    .await;

    result?;
    transaction::commit(txn).await.map_err(Into::into)
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
        admission,
        execution_preference,
        rules,
    } = input;
    if is_default && !is_enabled {
        return Err(AsterError::validation_error(
            "default storage policy group must be enabled",
        ));
    }

    let rules =
        rules.ok_or_else(|| AsterError::validation_error("placement rules are required"))?;
    validate_placement_rules(state.writer_db(), &rules).await?;
    let admission_config = serialize_admission(admission.unwrap_or_default())?;
    let execution_preference = execution_preference
        .unwrap_or_default()
        .as_str()
        .to_string();

    let txn = transaction::begin(state.writer_db()).await?;
    let now = Utc::now();
    let group = policy_group_repo::create_group(
        &txn,
        storage_policy_group::ActiveModel {
            name: Set(name),
            description: Set(description.unwrap_or_default()),
            is_enabled: Set(is_enabled),
            is_default: Set(false),
            admission_config: Set(admission_config),
            upload_execution_preference: Set(execution_preference),
            routing_revision: Set(1),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        },
    )
    .await?;
    replace_placement_rules(&txn, group.id, &rules).await?;
    if is_default {
        lock_default_group_assignment(&txn).await?;
        policy_group_repo::set_only_default_group(&txn, group.id).await?;
    }
    transaction::commit(txn).await?;
    state
        .driver_registry()
        .reload_policy_snapshot(state.policy_snapshot(), state.writer_db())
        .await?;
    crate::services::ops::config::runtime::publish_storage_topology_reload_after_commit(
        state,
        "create",
        "storage_policy_group",
        group.id,
    )
    .await;
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
        admission,
        execution_preference,
        rules,
    } = input;
    let txn = transaction::begin(state.writer_db()).await?;
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
            let team_assignment_count = team_repo::count_by_policy_group(&txn, id).await?;
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

    let rules = rules;
    if let Some(ref updated_rules) = rules {
        validate_placement_rules(&txn, updated_rules).await?;
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
    if let Some(value) = admission {
        active.admission_config = Set(serialize_admission(value)?);
    }
    if let Some(value) = execution_preference {
        active.upload_execution_preference = Set(value.as_str().to_string());
    }
    active.updated_at = Set(Utc::now());
    // Every profile mutation invalidates compiled placement snapshot entries.
    active.routing_revision = Set(active
        .routing_revision
        .take()
        .unwrap_or(1)
        .saturating_add(1));
    let group = policy_group_repo::update_group(&txn, active).await?;

    if let Some(updated_rules) = rules {
        replace_placement_rules(&txn, group.id, &updated_rules).await?;
    }

    if is_default == Some(true) {
        lock_default_group_assignment(&txn).await?;
        policy_group_repo::set_only_default_group(&txn, group.id).await?;
    }

    transaction::commit(txn).await?;
    state
        .driver_registry()
        .reload_policy_snapshot(state.policy_snapshot(), state.writer_db())
        .await?;
    crate::services::ops::config::runtime::publish_storage_topology_reload_after_commit(
        state,
        "update",
        "storage_policy_group",
        group.id,
    )
    .await;
    let group = policy_group_repo::find_group_by_id(state.writer_db(), group.id).await?;
    Ok(build_group_info(state, &group))
}

pub async fn delete_group(state: &impl SharedRuntimeState, id: i64) -> Result<()> {
    let txn = transaction::begin(state.writer_db()).await?;
    lock_default_group_assignment(&txn).await?;
    let group = policy_group_repo::find_group_by_id(&txn, id).await?;
    tracing::debug!(
        policy_group_id = id,
        policy_group_name = %group.name,
        is_default = group.is_default,
        "deleting storage policy group"
    );

    let user_assignment_count = policy_group_repo::count_user_group_assignments(&txn, id).await?;
    let team_assignment_count = team_repo::count_by_policy_group(&txn, id).await?;
    if let Some(message) =
        format_group_assignment_blocker("delete", user_assignment_count, team_assignment_count)
    {
        return Err(AsterError::validation_error(message));
    }

    policy_group_repo::delete_group(&txn, id).await?;
    transaction::commit(txn).await?;
    state
        .driver_registry()
        .reload_policy_snapshot(state.policy_snapshot(), state.writer_db())
        .await?;
    crate::services::ops::config::runtime::publish_storage_topology_reload_after_commit(
        state,
        "delete",
        "storage_policy_group",
        id,
    )
    .await;
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
    if !policy_placement_repo::find_all_rules(state.writer_db())
        .await?
        .iter()
        .any(|rule| rule.group_id == target_group_id)
    {
        return Err(AsterError::validation_error(
            "cannot migrate assignments to a storage policy group without policies",
        ));
    }

    let now = Utc::now();
    let txn = transaction::begin(state.writer_db()).await?;
    let affected_users =
        user_repo::migrate_policy_group_assignments(&txn, source_group_id, target_group_id, now)
            .await
            .map_aster_err(AsterError::database_operation)?;
    let affected_teams =
        team_repo::migrate_policy_group_assignments(&txn, source_group_id, target_group_id, now)
            .await
            .map_aster_err(AsterError::database_operation)?;

    transaction::commit(txn).await?;
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
    state
        .driver_registry()
        .reload_policy_snapshot(state.policy_snapshot(), state.writer_db())
        .await?;
    crate::services::ops::config::runtime::publish_storage_topology_reload_after_commit(
        state,
        "migrate_assignments",
        "storage_policy_group",
        source_group_id,
    )
    .await;

    Ok(PolicyGroupAssignmentMigrationResult {
        source_group_id,
        target_group_id,
        affected_users,
        affected_teams,
        migrated_assignments,
    })
}

async fn create_default_placement_rule<C: sea_orm::ConnectionTrait>(
    db: &C,
    group_id: i64,
    policy_id: i64,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<()> {
    let rule = policy_placement_repo::create_rule(
        db,
        storage_policy_group_rule::ActiveModel {
            group_id: Set(group_id),
            name: Set("Default placement rule".to_string()),
            description: Set("Automatic default target".to_string()),
            priority: Set(1),
            is_enabled: Set(true),
            matcher: Set(serde_json::to_string(&PlacementPayloadEnvelope::new(
                PlacementMatcher::default(),
            ))
            .map_err(|error| {
                AsterError::internal_error(format!("serialize placement matcher: {error}"))
            })?),
            selection_mode: Set("first_available".to_string()),
            unavailable_behavior: Set("reject".to_string()),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        },
    )
    .await?;
    policy_placement_repo::create_target(
        db,
        storage_policy_group_rule_target::ActiveModel {
            rule_id: Set(rule.id),
            policy_id: Set(policy_id),
            weight: Set(100),
            is_enabled: Set(true),
            accepting_new_writes: Set(true),
            stable_order: Set(1),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        },
    )
    .await?;
    Ok(())
}
