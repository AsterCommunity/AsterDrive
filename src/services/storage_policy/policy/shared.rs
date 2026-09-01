//! 存储策略服务子模块：`shared`。

use chrono::Utc;
use sea_orm::Set;

use crate::db::repository::{
    policy_group_repo, policy_placement_repo, policy_repo, system_initialization_repo, user_repo,
};
use crate::errors::{AsterError, Result};
use crate::runtime::SharedRuntimeState;
use aster_drive_model::entities::{
    storage_policy_group, storage_policy_group_rule, storage_policy_group_rule_target,
};
use aster_drive_model::types::{
    StoredStoragePolicyAllowedTypes, serialize_storage_policy_allowed_types,
};

use super::models::{
    StoragePlacementRuleInfo, StoragePlacementRuleInput, StoragePlacementTargetInfo,
    StoragePolicyGroupInfo, StoragePolicySummaryInfo,
};
use super::placement::{
    MAX_PLACEMENT_PAYLOAD_BYTES, PlacementMatcher, PlacementPayloadEnvelope,
    StorageAdmissionConstraints, compile_admission, compile_matcher,
};

const MAX_PLACEMENT_RULE_NAME_CHARS: usize = 128;
const MAX_PLACEMENT_RULE_DESCRIPTION_CHARS: usize = 512;

pub(super) fn serialize_allowed_types(
    allowed_types: &[String],
) -> Result<StoredStoragePolicyAllowedTypes> {
    serialize_storage_policy_allowed_types(allowed_types).map_err(|error| {
        AsterError::internal_error(format!("serialize storage policy allowed_types: {error}"))
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
    let profile = state.policy_snapshot().get_placement_profile(group.id);
    let (admission, execution_preference, routing_revision, rules) = profile
        .map(|profile| {
            let rules = profile
                .rules
                .iter()
                .map(|rule| StoragePlacementRuleInfo {
                    id: rule.id,
                    name: rule.name.clone(),
                    description: rule.description.clone(),
                    priority: rule.priority,
                    is_enabled: rule.is_enabled,
                    matcher: rule.matcher.clone(),
                    selection_mode: rule.selection_mode,
                    unavailable_behavior: rule.unavailable_behavior,
                    targets: rule
                        .targets
                        .iter()
                        .filter_map(|target| {
                            let Some(policy) = state.policy_snapshot().get_policy(target.policy_id)
                            else {
                                tracing::warn!(
                                    target_id = target.id,
                                    policy_id = target.policy_id,
                                    "placement target references a missing policy"
                                );
                                return None;
                            };
                            Some(StoragePlacementTargetInfo {
                                id: target.id,
                                policy_id: target.policy_id,
                                weight: i32::try_from(target.weight).ok()?,
                                is_enabled: target.is_enabled,
                                accepting_new_writes: target.accepting_new_writes,
                                stable_order: i32::try_from(target.stable_order).ok()?,
                                policy: StoragePolicySummaryInfo {
                                    id: policy.id,
                                    name: policy.name,
                                    connector_id: policy.connector_id,
                                },
                            })
                        })
                        .collect(),
                })
                .collect();
            (
                profile.admission,
                profile.execution_preference,
                profile.revision,
                rules,
            )
        })
        .unwrap_or_else(|| {
            (
                StorageAdmissionConstraints::default(),
                super::placement::parse_execution_preference(&group.upload_execution_preference)
                    .unwrap_or(super::placement::UploadExecutionPreference::Automatic),
                group.routing_revision,
                Vec::new(),
            )
        });

    StoragePolicyGroupInfo {
        id: group.id,
        name: group.name.clone(),
        description: group.description.clone(),
        is_enabled: group.is_enabled,
        is_default: group.is_default,
        created_at: group.created_at,
        updated_at: group.updated_at,
        admission,
        execution_preference,
        routing_revision,
        rules,
    }
}

pub(super) async fn validate_placement_rules<C: sea_orm::ConnectionTrait>(
    db: &C,
    rules: &[StoragePlacementRuleInput],
) -> Result<()> {
    if rules.is_empty() {
        return Err(AsterError::validation_error(
            "placement profile must contain at least one rule",
        ));
    }
    let mut priorities = std::collections::HashSet::new();
    for rule in rules {
        validate_rule_name(&rule.name)?;
        validate_rule_description(rule.description.as_deref())?;
        if rule.priority <= 0 || !priorities.insert(rule.priority) {
            return Err(AsterError::validation_error(
                "placement rule priorities must be unique positive integers",
            ));
        }
        serialize_matcher(rule.matcher.clone())?;
        if rule.targets.is_empty() {
            return Err(AsterError::validation_error(
                "placement rule must contain at least one target",
            ));
        }
        let mut policies = std::collections::HashSet::new();
        for target in &rule.targets {
            if target.weight <= 0 || target.stable_order < 0 {
                return Err(AsterError::validation_error(
                    "placement target weight must be positive and stable_order non-negative",
                ));
            }
            if !policies.insert(target.policy_id) {
                return Err(AsterError::validation_error(
                    "placement rule targets must reference distinct policies",
                ));
            }
            policy_repo::find_by_id(db, target.policy_id).await?;
        }
    }
    Ok(())
}

fn validate_rule_name(name: &str) -> Result<()> {
    if name.trim().is_empty() {
        return Err(AsterError::validation_error(
            "placement rule name must not be empty",
        ));
    }
    if name.trim().chars().count() > MAX_PLACEMENT_RULE_NAME_CHARS {
        return Err(AsterError::validation_error(
            "placement rule name must be at most 128 characters",
        ));
    }
    Ok(())
}

fn validate_rule_description(description: Option<&str>) -> Result<()> {
    if description.is_some_and(|value| value.chars().count() > MAX_PLACEMENT_RULE_DESCRIPTION_CHARS)
    {
        return Err(AsterError::validation_error(
            "placement rule description must be at most 512 characters",
        ));
    }
    Ok(())
}

fn serialize_matcher(matcher: PlacementMatcher) -> Result<String> {
    let matcher = compile_matcher(matcher).map_err(AsterError::validation_error)?;
    let payload =
        serde_json::to_string(&PlacementPayloadEnvelope::new(matcher)).map_err(|error| {
            AsterError::internal_error(format!("serialize placement matcher: {error}"))
        })?;
    if payload.len() > MAX_PLACEMENT_PAYLOAD_BYTES {
        return Err(AsterError::validation_error(
            "placement matcher payload exceeds 4000 bytes",
        ));
    }
    Ok(payload)
}

pub(super) async fn replace_placement_rules<C: sea_orm::ConnectionTrait>(
    db: &C,
    group_id: i64,
    rules: &[StoragePlacementRuleInput],
) -> Result<()> {
    validate_placement_rules(db, rules).await?;
    policy_placement_repo::delete_rules_by_group(db, group_id).await?;
    let now = Utc::now();
    for rule in rules {
        let matcher = serialize_matcher(rule.matcher.clone())?;
        let created = policy_placement_repo::create_rule(
            db,
            storage_policy_group_rule::ActiveModel {
                group_id: Set(group_id),
                name: Set(rule.name.trim().to_string()),
                description: Set(rule.description.clone().unwrap_or_default()),
                priority: Set(rule.priority),
                is_enabled: Set(rule.is_enabled),
                matcher: Set(matcher),
                selection_mode: Set(match rule.selection_mode {
                    super::placement::PlacementSelectionMode::FirstAvailable => "first_available",
                    super::placement::PlacementSelectionMode::WeightedRandom => "weighted_random",
                }
                .to_string()),
                unavailable_behavior: Set(match rule.unavailable_behavior {
                    super::placement::PlacementUnavailableBehavior::NextRule => "next_rule",
                    super::placement::PlacementUnavailableBehavior::Reject => "reject",
                }
                .to_string()),
                created_at: Set(now),
                updated_at: Set(now),
                ..Default::default()
            },
        )
        .await?;
        for target in &rule.targets {
            policy_placement_repo::create_target(
                db,
                storage_policy_group_rule_target::ActiveModel {
                    rule_id: Set(created.id),
                    policy_id: Set(target.policy_id),
                    weight: Set(target.weight),
                    is_enabled: Set(target.is_enabled),
                    accepting_new_writes: Set(target.accepting_new_writes),
                    stable_order: Set(target.stable_order),
                    created_at: Set(now),
                    updated_at: Set(now),
                    ..Default::default()
                },
            )
            .await?;
        }
    }
    Ok(())
}

pub(super) fn serialize_admission(admission: StorageAdmissionConstraints) -> Result<String> {
    let admission = compile_admission(admission).map_err(AsterError::validation_error)?;
    let payload =
        serde_json::to_string(&PlacementPayloadEnvelope::new(admission)).map_err(|error| {
            AsterError::internal_error(format!("serialize placement admission: {error}"))
        })?;
    if payload.len() > MAX_PLACEMENT_PAYLOAD_BYTES {
        return Err(AsterError::validation_error(
            "placement admission payload exceeds 4000 bytes",
        ));
    }
    Ok(payload)
}

pub(super) async fn lock_default_group_assignment<C: sea_orm::ConnectionTrait>(
    db: &C,
) -> Result<()> {
    system_initialization_repo::acquire_storage_topology_lock(db).await
}

#[cfg(test)]
mod validation_tests {
    use super::*;

    #[test]
    fn placement_rule_text_limits_count_unicode_characters() {
        assert!(validate_rule_name(&"界".repeat(128)).is_ok());
        assert!(validate_rule_name(&"界".repeat(129)).is_err());
        assert!(validate_rule_name(" \t ").is_err());

        assert!(validate_rule_description(Some(&"述".repeat(512))).is_ok());
        assert!(validate_rule_description(Some(&"述".repeat(513))).is_err());
        assert!(validate_rule_description(None).is_ok());
    }

    #[test]
    fn placement_matcher_payload_enforces_database_byte_limit() {
        let small = PlacementMatcher {
            extensions: vec!["txt".to_string(), "tar.gz".to_string()],
            ..Default::default()
        };
        let serialized = serialize_matcher(small).expect("small matcher should serialize");
        assert!(serialized.len() <= MAX_PLACEMENT_PAYLOAD_BYTES);

        let oversized = PlacementMatcher {
            extensions: (0..700).map(|index| format!("ext{index:04}")).collect(),
            ..Default::default()
        };
        let error = serialize_matcher(oversized).expect_err("oversized matcher must be rejected");
        assert_eq!(
            error.message(),
            "placement matcher payload exceeds 4000 bytes"
        );
    }
}

pub(super) async fn ensure_singleton_group_for_policy<C: sea_orm::ConnectionTrait>(
    db: &C,
    policy_id: i64,
) -> Result<i64> {
    let singleton_description = format!(
        "Compatibility singleton profile for storage policy #{}",
        policy_id
    );
    let groups = policy_group_repo::find_all_groups(db).await?;
    for group in groups {
        if group.description != singleton_description || !group.is_enabled {
            continue;
        }
        let group_rules = policy_placement_repo::find_rules_by_group_id(db, group.id).await?;
        let matching_targets = if group_rules.len() == 1 {
            policy_placement_repo::find_targets_by_rule_id(db, group_rules[0].id).await?
        } else {
            Vec::new()
        };
        if group_rules.len() == 1
            && matching_targets.len() == 1
            && matching_targets[0].policy_id == policy_id
        {
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
            admission_config: Set(serialize_admission(StorageAdmissionConstraints::default())?),
            upload_execution_preference: Set("automatic".to_string()),
            routing_revision: Set(1),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        },
    )
    .await?;
    let rule = policy_placement_repo::create_rule(
        db,
        storage_policy_group_rule::ActiveModel {
            group_id: Set(group.id),
            name: Set("Default placement rule".to_string()),
            description: Set("Automatic singleton target".to_string()),
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
            policy_id: Set(policy.id),
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
    Ok(group.id)
}

pub(super) async fn set_default_policy_and_group<C: sea_orm::ConnectionTrait>(
    db: &C,
    policy_id: i64,
) -> Result<i64> {
    lock_default_group_assignment(db).await?;
    policy_repo::set_only_default(db, policy_id).await?;
    let default_group_id = ensure_singleton_group_for_policy(db, policy_id).await?;
    policy_group_repo::set_only_default_group(db, default_group_id).await?;
    user_repo::assign_policy_group_to_unassigned(db, default_group_id, Utc::now()).await?;
    Ok(default_group_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rule_name_rejects_blank_values() {
        assert!(validate_rule_name("").is_err());
        assert!(validate_rule_name("  \t").is_err());
        assert!(validate_rule_name("Images").is_ok());
    }

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
