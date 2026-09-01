//! 存储子模块：`policy_snapshot`。

use std::collections::{HashMap, HashSet};

use parking_lot::RwLock;
use sea_orm::{DatabaseConnection, EntityTrait, QueryOrder};

use crate::db::repository::{
    managed_follower_repo, policy_group_repo, policy_repo, team_repo, user_repo,
};
use crate::errors::{AsterError, Result};
use crate::services::storage_policy::policy::placement::{
    CompiledPlacementProfile, PLACEMENT_PAYLOAD_FORMAT_VERSION, PLACEMENT_PAYLOAD_SCHEMA_VERSION,
    PlacementMatcher, PlacementPayloadEnvelope, PlacementRule, PlacementSelectionMode,
    PlacementTarget, PlacementUnavailableBehavior, StorageAdmissionConstraints, compile_admission,
    compile_matcher, parse_execution_preference,
};
use aster_drive_model::entities::{
    storage_policy, storage_policy_group, storage_policy_group_rule,
    storage_policy_group_rule_target,
};

#[derive(Default)]
struct PolicySnapshotData {
    policies_by_id: HashMap<i64, storage_policy::Model>,
    policy_groups_by_id: HashMap<i64, storage_policy_group::Model>,
    placement_profiles_by_id: HashMap<i64, CompiledPlacementProfile>,
    user_policy_group_by_user_id: HashMap<i64, i64>,
    team_policy_group_by_team_id: HashMap<i64, i64>,
    enabled_remote_node_ids: HashSet<i64>,
    remote_node_id_by_policy_id: HashMap<i64, Option<i64>>,
    system_default_policy_group_id: Option<i64>,
    system_default_policy_id: Option<i64>,
}

pub struct PolicySnapshot {
    snapshot: RwLock<PolicySnapshotData>,
}

impl PolicySnapshot {
    pub fn new() -> Self {
        Self {
            snapshot: RwLock::new(PolicySnapshotData::default()),
        }
    }

    pub(crate) async fn reload(
        &self,
        db: &DatabaseConnection,
        connectors: &crate::storage::connectors::StorageConnectorRegistry,
    ) -> Result<()> {
        let policies = policy_repo::find_all(db).await?;
        let policy_groups = policy_group_repo::find_all_groups(db).await?;
        let placement_rules = storage_policy_group_rule::Entity::find()
            .order_by_asc(storage_policy_group_rule::Column::Priority)
            .order_by_asc(storage_policy_group_rule::Column::Id)
            .all(db)
            .await
            .map_err(AsterError::from)?;
        let placement_targets = storage_policy_group_rule_target::Entity::find()
            .order_by_asc(storage_policy_group_rule_target::Column::StableOrder)
            .order_by_asc(storage_policy_group_rule_target::Column::Id)
            .all(db)
            .await
            .map_err(AsterError::from)?;
        let managed_followers = managed_follower_repo::find_all(db).await?;
        let users = user_repo::find_all(db).await?;
        let teams = team_repo::find_all(db).await?;
        let enabled_remote_node_ids = managed_followers
            .iter()
            .filter(|node| node.is_enabled)
            .map(|node| node.id)
            .collect::<HashSet<_>>();

        let system_default_policy_id = policies
            .iter()
            .find(|policy| policy.is_default)
            .map(|policy| policy.id);
        let mut remote_node_id_by_policy_id = HashMap::new();
        for policy in &policies {
            if let Some(binding) =
                crate::storage::connectors::resolve_remote_policy_binding(connectors, policy)?
            {
                remote_node_id_by_policy_id.insert(policy.id, binding.remote_node_id);
            }
        }
        let policies_by_id = policies
            .into_iter()
            .map(|policy| (policy.id, policy))
            .collect::<HashMap<_, _>>();
        let system_default_policy_group_id = policy_groups
            .iter()
            .find(|group| group.is_default)
            .map(|group| group.id);
        let policy_groups_by_id = policy_groups
            .iter()
            .cloned()
            .map(|group| (group.id, group))
            .collect::<HashMap<_, _>>();

        let mut targets_by_rule_id: HashMap<i64, Vec<PlacementTarget>> = HashMap::new();
        for target in placement_targets {
            let Some(policy) = policies_by_id.get(&target.policy_id) else {
                tracing::warn!(
                    rule_id = target.rule_id,
                    policy_id = target.policy_id,
                    "placement target references a missing policy"
                );
                continue;
            };
            let capability_exclusion = crate::storage::connectors::resolve_policy_upload_transport(
                connectors,
                policy,
            )
            .err()
            .map(|_| {
                crate::services::storage_policy::policy::placement::TargetExclusionReason::Incompatible
            });
            let exclusion = capability_exclusion.or_else(|| match remote_node_id_by_policy_id.get(&policy.id) {
                None => None,
                Some(None) => Some(crate::services::storage_policy::policy::placement::TargetExclusionReason::Unavailable),
                Some(Some(node_id)) if !enabled_remote_node_ids.contains(node_id) => {
                    Some(crate::services::storage_policy::policy::placement::TargetExclusionReason::Unavailable)
                }
                Some(Some(_)) => None,
            });
            targets_by_rule_id
                .entry(target.rule_id)
                .or_default()
                .push(PlacementTarget {
                    id: target.id,
                    policy_id: target.policy_id,
                    weight: match u32::try_from(target.weight) {
                        Ok(value) => value,
                        Err(error) => {
                            tracing::error!(target_id = target.id, policy_id = target.policy_id, error = %error, "excluding placement target with invalid weight");
                            continue;
                        }
                    },
                    stable_order: match u32::try_from(target.stable_order) {
                        Ok(value) => value,
                        Err(error) => {
                            tracing::error!(target_id = target.id, policy_id = target.policy_id, error = %error, "excluding placement target with invalid stable order");
                            continue;
                        }
                    },
                    is_enabled: target.is_enabled,
                    accepting_new_writes: target.accepting_new_writes,
                    policy_max_file_size: policy.max_file_size,
                    exclusion,
                });
        }

        let mut rules_by_group_id: HashMap<i64, Vec<PlacementRule>> = HashMap::new();
        for rule in placement_rules {
            let envelope: PlacementPayloadEnvelope<PlacementMatcher> = match serde_json::from_str(
                &rule.matcher,
            ) {
                Ok(value) => value,
                Err(error) => {
                    tracing::error!(rule_id = rule.id, error = %error, "skipping placement rule with invalid matcher payload");
                    continue;
                }
            };
            if envelope.format_version != PLACEMENT_PAYLOAD_FORMAT_VERSION
                || envelope.schema_version != PLACEMENT_PAYLOAD_SCHEMA_VERSION
            {
                tracing::error!(
                    rule_id = rule.id,
                    "skipping placement rule with unsupported matcher version"
                );
                continue;
            }
            let matcher = match compile_matcher(envelope.values) {
                Ok(value) => value,
                Err(error) => {
                    tracing::error!(rule_id = rule.id, error = %error, "skipping placement rule with invalid matcher");
                    continue;
                }
            };
            let Some(selection_mode) = parse_selection_mode(&rule.selection_mode) else {
                tracing::error!(
                    rule_id = rule.id,
                    "skipping placement rule with invalid selection mode"
                );
                continue;
            };
            let Some(unavailable_behavior) = parse_unavailable_behavior(&rule.unavailable_behavior)
            else {
                tracing::error!(
                    rule_id = rule.id,
                    "skipping placement rule with invalid unavailable behavior"
                );
                continue;
            };
            rules_by_group_id
                .entry(rule.group_id)
                .or_default()
                .push(PlacementRule {
                    id: rule.id,
                    name: rule.name,
                    description: rule.description,
                    priority: rule.priority,
                    is_enabled: rule.is_enabled,
                    matcher,
                    selection_mode,
                    unavailable_behavior,
                    targets: targets_by_rule_id.remove(&rule.id).unwrap_or_default(),
                });
        }

        let mut placement_profiles_by_id = HashMap::new();
        for group in &policy_groups {
            let admission_envelope: PlacementPayloadEnvelope<StorageAdmissionConstraints> =
                match serde_json::from_str(&group.admission_config) {
                    Ok(value) => value,
                    Err(error) => {
                        tracing::error!(profile_id = group.id, error = %error, "skipping placement profile with invalid admission payload");
                        continue;
                    }
                };
            if admission_envelope.format_version != PLACEMENT_PAYLOAD_FORMAT_VERSION
                || admission_envelope.schema_version != PLACEMENT_PAYLOAD_SCHEMA_VERSION
            {
                tracing::error!(
                    profile_id = group.id,
                    "skipping placement profile with unsupported admission version"
                );
                continue;
            }
            let admission = match compile_admission(admission_envelope.values) {
                Ok(value) => value,
                Err(error) => {
                    tracing::error!(profile_id = group.id, error = %error, "skipping placement profile with invalid admission");
                    continue;
                }
            };
            let Some(execution_preference) =
                parse_execution_preference(&group.upload_execution_preference)
            else {
                tracing::error!(
                    profile_id = group.id,
                    "skipping placement profile with invalid execution preference"
                );
                continue;
            };
            placement_profiles_by_id.insert(
                group.id,
                CompiledPlacementProfile {
                    id: group.id,
                    revision: group.routing_revision,
                    is_enabled: group.is_enabled,
                    admission,
                    execution_preference,
                    rules: rules_by_group_id.remove(&group.id).unwrap_or_default(),
                },
            );
        }

        let user_policy_group_by_user_id = users
            .into_iter()
            .filter_map(|user| user.policy_group_id.map(|group_id| (user.id, group_id)))
            .collect();
        let team_policy_group_by_team_id = teams
            .into_iter()
            .filter_map(|team| team.policy_group_id.map(|group_id| (team.id, group_id)))
            .collect();
        *self.snapshot.write() = PolicySnapshotData {
            policies_by_id,
            policy_groups_by_id,
            placement_profiles_by_id,
            user_policy_group_by_user_id,
            team_policy_group_by_team_id,
            enabled_remote_node_ids,
            remote_node_id_by_policy_id,
            system_default_policy_group_id,
            system_default_policy_id,
        };

        Ok(())
    }

    pub fn get_policy(&self, policy_id: i64) -> Option<storage_policy::Model> {
        self.snapshot.read().policies_by_id.get(&policy_id).cloned()
    }

    pub fn all_policies(&self) -> Vec<storage_policy::Model> {
        self.snapshot
            .read()
            .policies_by_id
            .values()
            .cloned()
            .collect()
    }

    pub fn get_policy_or_err(&self, policy_id: i64) -> Result<storage_policy::Model> {
        self.get_policy(policy_id)
            .ok_or_else(|| AsterError::storage_policy_not_found(format!("policy #{policy_id}")))
    }

    pub fn is_policy_available_for_outbound(&self, policy: &storage_policy::Model) -> bool {
        self.policy_available_for_outbound(policy)
    }

    pub fn describe_policy_outbound_availability(
        &self,
        policy: &storage_policy::Model,
    ) -> Option<String> {
        let snapshot = self.snapshot.read();
        let remote_node_id = snapshot
            .remote_node_id_by_policy_id
            .get(&policy.id)
            .copied()?;
        let Some(remote_node_id) = remote_node_id else {
            return Some("remote policy has no bound remote node".to_string());
        };

        if snapshot.enabled_remote_node_ids.contains(&remote_node_id) {
            None
        } else {
            Some(format!(
                "remote node #{remote_node_id} is disabled or unavailable"
            ))
        }
    }

    pub fn get_policy_group(&self, group_id: i64) -> Option<storage_policy_group::Model> {
        self.snapshot
            .read()
            .policy_groups_by_id
            .get(&group_id)
            .cloned()
    }

    pub fn get_policy_group_or_err(&self, group_id: i64) -> Result<storage_policy_group::Model> {
        self.get_policy_group(group_id).ok_or_else(|| {
            AsterError::record_not_found(format!("storage_policy_group #{group_id}"))
        })
    }

    pub fn get_placement_profile(&self, profile_id: i64) -> Option<CompiledPlacementProfile> {
        self.snapshot
            .read()
            .placement_profiles_by_id
            .get(&profile_id)
            .cloned()
    }

    pub fn resolve_placement(
        &self,
        profile_id: i64,
        context: &crate::services::storage_policy::policy::placement::StoragePlacementContext,
        folder_override: Option<
            &crate::services::storage_policy::policy::placement::FolderPlacementOverride,
        >,
    ) -> Result<crate::services::storage_policy::policy::placement::StorageRoutingDecision> {
        let profile = self.get_placement_profile(profile_id).ok_or_else(|| {
            AsterError::storage_policy_not_found(format!("placement profile #{profile_id}"))
        })?;
        crate::services::storage_policy::policy::placement::resolve_placement(
            &profile,
            context,
            folder_override,
        )
        .map_err(|rejection| {
            AsterError::validation_error(format!("{}: {}", rejection.code(), profile_id))
        })
    }

    pub fn resolve_team_policy_group_id(&self, team_id: i64) -> Option<i64> {
        self.snapshot
            .read()
            .team_policy_group_by_team_id
            .get(&team_id)
            .copied()
    }

    pub fn resolve_default_policy_group_id(&self, user_id: i64) -> Option<i64> {
        self.snapshot
            .read()
            .user_policy_group_by_user_id
            .get(&user_id)
            .copied()
    }

    pub fn resolve_default_policy_group(
        &self,
        user_id: i64,
    ) -> Option<storage_policy_group::Model> {
        let group_id = self.resolve_default_policy_group_id(user_id)?;
        self.get_policy_group(group_id)
    }

    pub fn require_user_policy_group_id(&self, user_id: i64) -> Result<i64> {
        self.resolve_default_policy_group_id(user_id)
            .ok_or_else(|| {
                AsterError::storage_policy_not_found(format!(
                    "no storage policy group assigned to user #{user_id}"
                ))
            })
    }

    pub fn system_default_policy(&self) -> Option<storage_policy::Model> {
        let policy_id = self.snapshot.read().system_default_policy_id?;
        self.get_policy(policy_id)
    }

    pub fn system_default_policy_group(&self) -> Option<storage_policy_group::Model> {
        let group_id = self.snapshot.read().system_default_policy_group_id?;
        self.get_policy_group(group_id)
    }

    pub fn set_user_policy_group(&self, user_id: i64, group_id: i64) {
        self.snapshot
            .write()
            .user_policy_group_by_user_id
            .insert(user_id, group_id);
    }

    pub fn set_team_policy_group(&self, team_id: i64, group_id: i64) {
        self.snapshot
            .write()
            .team_policy_group_by_team_id
            .insert(team_id, group_id);
    }

    pub fn remove_user_policy_group(&self, user_id: i64) {
        self.snapshot
            .write()
            .user_policy_group_by_user_id
            .remove(&user_id);
    }

    fn policy_available_for_outbound(&self, policy: &storage_policy::Model) -> bool {
        let snapshot = self.snapshot.read();
        let Some(remote_node_id) = snapshot
            .remote_node_id_by_policy_id
            .get(&policy.id)
            .copied()
        else {
            return true;
        };
        let Some(remote_node_id) = remote_node_id else {
            return false;
        };

        snapshot.enabled_remote_node_ids.contains(&remote_node_id)
    }
}

fn parse_selection_mode(value: &str) -> Option<PlacementSelectionMode> {
    match value {
        "first_available" => Some(PlacementSelectionMode::FirstAvailable),
        "weighted_random" => Some(PlacementSelectionMode::WeightedRandom),
        _ => None,
    }
}

fn parse_unavailable_behavior(value: &str) -> Option<PlacementUnavailableBehavior> {
    match value {
        "next_rule" => Some(PlacementUnavailableBehavior::NextRule),
        "reject" => Some(PlacementUnavailableBehavior::Reject),
        _ => None,
    }
}

impl Default for PolicySnapshot {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
impl PolicySnapshot {
    fn resolve_compiled_profile_for_test(
        &self,
        group_id: i64,
        file_size: i64,
    ) -> Result<storage_policy::Model> {
        let profile = self.get_placement_profile(group_id).ok_or_else(|| {
            AsterError::storage_policy_not_found(format!("placement profile #{group_id}"))
        })?;
        let context = crate::services::storage_policy::policy::placement::StoragePlacementContext {
            profile_id: group_id,
            filename: "file.bin".to_string(),
            file_size,
            extension: "bin".to_string(),
            compound_extension: None,
            category: aster_forge_file_classification::FileCategory::Other,
        };
        let decision = crate::services::storage_policy::policy::placement::resolve_placement(
            &profile, &context, None,
        )
        .map_err(|error| AsterError::validation_error(error.code()))?;
        self.get_policy_or_err(decision.policy_id)
    }

    fn resolve_default_policy_for_size_for_test(
        &self,
        user_id: i64,
        file_size: i64,
    ) -> Option<storage_policy::Model> {
        let group_id = self.resolve_default_policy_group_id(user_id)?;
        self.resolve_compiled_profile_for_test(group_id, file_size)
            .ok()
    }
}

#[cfg(test)]
mod tests {
    use super::PolicySnapshot;
    use crate::config::DatabaseConfig;
    use crate::db;
    use crate::db::repository::{managed_follower_repo, policy_group_repo, policy_repo, user_repo};
    use crate::services::storage_policy::policy::placement::{
        PlacementMatcher, PlacementPayloadEnvelope, StorageAdmissionConstraints,
        StoragePlacementContext,
    };
    use crate::storage::connectors::builtin_storage_connector_registry;
    use aster_drive_model::types::{
        RemoteDownloadStrategy, RemoteUploadStrategy, UserRole, UserStatus,
    };
    use chrono::Utc;
    use sea_orm::{ActiveModelTrait, Set};

    async fn setup_db() -> sea_orm::DatabaseConnection {
        let db = db::connect_with_metrics(
            &DatabaseConfig {
                url: "sqlite::memory:".into(),
                pool_size: 1,
                retry_count: 0,
            },
            aster_drive_metrics::NoopMetrics::arc(),
        )
        .await
        .unwrap();
        crate::storage::connectors::test_support::migrate_current_storage_test_schema(&db).await;
        db
    }

    async fn create_policy(
        db: &sea_orm::DatabaseConnection,
        name: &str,
        base_path: &str,
        is_default: bool,
    ) -> aster_drive_model::entities::storage_policy::Model {
        let now = Utc::now();
        let fixture = crate::storage::connectors::test_support::local_policy(base_path);
        policy_repo::create(
            db,
            aster_drive_model::entities::storage_policy::ActiveModel {
                name: Set(name.to_string()),
                connector_id: Set(fixture.connector_id),
                storage_config: Set(fixture.storage_config),
                max_file_size: Set(0),
                allowed_types: Set(
                    aster_drive_model::types::StoredStoragePolicyAllowedTypes::empty(),
                ),
                is_default: Set(is_default),
                chunk_size: Set(5_242_880),
                created_at: Set(now),
                updated_at: Set(now),
                ..Default::default()
            },
        )
        .await
        .unwrap()
    }

    async fn create_remote_node(
        db: &sea_orm::DatabaseConnection,
        name: &str,
        is_enabled: bool,
    ) -> aster_drive_model::entities::managed_follower::Model {
        let now = Utc::now();
        managed_follower_repo::create(
            db,
            aster_drive_model::entities::managed_follower::ActiveModel {
                name: Set(name.to_string()),
                base_url: Set("https://remote.example.com".to_string()),
                access_key: Set(format!("ak_{name}")),
                secret_key: Set(format!("sk_{name}")),
                is_enabled: Set(is_enabled),
                last_capabilities: Set("{}".to_string()),
                last_probe_error: Set(String::new()),
                last_probe_at: Set(None),
                created_at: Set(now),
                updated_at: Set(now),
                ..Default::default()
            },
        )
        .await
        .unwrap()
    }

    async fn create_remote_policy(
        db: &sea_orm::DatabaseConnection,
        name: &str,
        remote_node_id: i64,
    ) -> aster_drive_model::entities::storage_policy::Model {
        let now = Utc::now();
        let fixture = crate::storage::connectors::test_support::remote_policy(
            "",
            Some(remote_node_id),
            RemoteDownloadStrategy::RelayStream,
            RemoteUploadStrategy::RelayStream,
        );
        policy_repo::create(
            db,
            aster_drive_model::entities::storage_policy::ActiveModel {
                name: Set(name.to_string()),
                connector_id: Set(fixture.connector_id),
                storage_config: Set(fixture.storage_config),
                max_file_size: Set(0),
                allowed_types: Set(
                    aster_drive_model::types::StoredStoragePolicyAllowedTypes::empty(),
                ),
                is_default: Set(false),
                chunk_size: Set(5_242_880),
                created_at: Set(now),
                updated_at: Set(now),
                ..Default::default()
            },
        )
        .await
        .unwrap()
    }

    async fn create_group(
        db: &sea_orm::DatabaseConnection,
        name: &str,
        policy_id: i64,
        is_default: bool,
        min_file_size: i64,
        max_file_size: i64,
    ) -> aster_drive_model::entities::storage_policy_group::Model {
        let now = Utc::now();
        let group = policy_group_repo::create_group(
            db,
            aster_drive_model::entities::storage_policy_group::ActiveModel {
                name: Set(name.to_string()),
                description: Set(String::new()),
                is_enabled: Set(true),
                is_default: Set(is_default),
                admission_config: Set(serde_json::to_string(&PlacementPayloadEnvelope::new(
                    StorageAdmissionConstraints::default(),
                ))
                .unwrap()),
                upload_execution_preference: Set("automatic".to_string()),
                routing_revision: Set(1),
                created_at: Set(now),
                updated_at: Set(now),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let matcher = serde_json::to_string(&PlacementPayloadEnvelope::new(PlacementMatcher {
            min_file_size,
            max_file_size,
            ..Default::default()
        }))
        .unwrap();
        let rule = aster_drive_model::entities::storage_policy_group_rule::ActiveModel {
            group_id: Set(group.id),
            name: Set("Test Rule".to_string()),
            description: Set(String::new()),
            priority: Set(1),
            is_enabled: Set(true),
            matcher: Set(matcher),
            selection_mode: Set("first_available".to_string()),
            unavailable_behavior: Set("next_rule".to_string()),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(db)
        .await
        .unwrap();
        aster_drive_model::entities::storage_policy_group_rule_target::ActiveModel {
            rule_id: Set(rule.id),
            policy_id: Set(policy_id),
            weight: Set(100),
            is_enabled: Set(true),
            accepting_new_writes: Set(true),
            stable_order: Set(1),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(db)
        .await
        .unwrap();
        group
    }

    async fn create_user(
        db: &sea_orm::DatabaseConnection,
        username: &str,
        email: &str,
    ) -> aster_drive_model::entities::user::Model {
        let now = Utc::now();
        user_repo::create(
            db,
            aster_drive_model::entities::user::ActiveModel {
                username: Set(username.to_string()),
                email: Set(email.to_string()),
                password_hash: Set("hashed-password".to_string()),
                role: Set(UserRole::User),
                status: Set(UserStatus::Active),
                session_version: Set(1),
                storage_used: Set(0),
                storage_quota: Set(0),
                created_at: Set(now),
                updated_at: Set(now),
                config: Set(None),
                ..Default::default()
            },
        )
        .await
        .unwrap()
    }

    async fn create_test_rule(
        db: &sea_orm::DatabaseConnection,
        group_id: i64,
        policy_id: i64,
        priority: i32,
        min_file_size: i64,
        max_file_size: i64,
    ) {
        let now = Utc::now();
        let rule = aster_drive_model::entities::storage_policy_group_rule::ActiveModel {
            group_id: Set(group_id),
            name: Set(format!("Rule {priority}")),
            description: Set(String::new()),
            priority: Set(priority),
            is_enabled: Set(true),
            matcher: Set(
                serde_json::to_string(&PlacementPayloadEnvelope::new(PlacementMatcher {
                    min_file_size,
                    max_file_size,
                    ..Default::default()
                }))
                .unwrap(),
            ),
            selection_mode: Set("first_available".to_string()),
            unavailable_behavior: Set("next_rule".to_string()),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(db)
        .await
        .unwrap();
        aster_drive_model::entities::storage_policy_group_rule_target::ActiveModel {
            rule_id: Set(rule.id),
            policy_id: Set(policy_id),
            weight: Set(100),
            is_enabled: Set(true),
            accepting_new_writes: Set(true),
            stable_order: Set(priority),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(db)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn reload_compiles_rule_targets_and_resolves_without_legacy_items() {
        let db = setup_db().await;
        let policy = create_policy(&db, "Placement Policy", "/tmp/placement-policy", true).await;
        let now = Utc::now();
        let group = policy_group_repo::create_group(
            &db,
            aster_drive_model::entities::storage_policy_group::ActiveModel {
                name: Set("Placement Profile".to_string()),
                description: Set(String::new()),
                is_enabled: Set(true),
                is_default: Set(true),
                admission_config: Set(serde_json::to_string(&PlacementPayloadEnvelope::new(
                    StorageAdmissionConstraints::default(),
                ))
                .unwrap()),
                upload_execution_preference: Set("automatic".to_string()),
                routing_revision: Set(9),
                created_at: Set(now),
                updated_at: Set(now),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let rule = aster_drive_model::entities::storage_policy_group_rule::ActiveModel {
            group_id: Set(group.id),
            name: Set("Images".to_string()),
            description: Set(String::new()),
            priority: Set(1),
            is_enabled: Set(true),
            matcher: Set(
                serde_json::to_string(&PlacementPayloadEnvelope::new(PlacementMatcher {
                    extensions: vec!["jpg".to_string()],
                    ..Default::default()
                }))
                .unwrap(),
            ),
            selection_mode: Set("first_available".to_string()),
            unavailable_behavior: Set("reject".to_string()),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();
        aster_drive_model::entities::storage_policy_group_rule_target::ActiveModel {
            rule_id: Set(rule.id),
            policy_id: Set(policy.id),
            weight: Set(100),
            is_enabled: Set(true),
            accepting_new_writes: Set(true),
            stable_order: Set(1),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();

        let snapshot = PolicySnapshot::new();
        let connectors = builtin_storage_connector_registry().unwrap();
        snapshot.reload(&db, &connectors).await.unwrap();
        let profile = snapshot.get_placement_profile(group.id).unwrap();
        let context =
            StoragePlacementContext::from_filename(group.id, "photo.jpg", 10, "image/jpeg");
        let decision = crate::services::storage_policy::policy::placement::resolve_placement(
            &profile, &context, None,
        )
        .unwrap();
        assert_eq!(decision.policy_id, policy.id);
        assert_eq!(decision.revision, 9);
        assert_eq!(decision.rule_id, Some(rule.id));
    }

    #[tokio::test]
    async fn reload_exposes_policies_and_system_default_group() {
        let db = setup_db().await;
        let system_default =
            create_policy(&db, "System Default", "/tmp/policy-snap-default", true).await;
        let secondary = create_policy(&db, "Secondary", "/tmp/policy-snap-secondary", false).await;
        let default_group = create_group(&db, "Default Group", system_default.id, true, 0, 0).await;
        let snapshot = PolicySnapshot::new();
        let connectors = builtin_storage_connector_registry().unwrap();

        snapshot.reload(&db, &connectors).await.unwrap();

        assert_eq!(
            snapshot.system_default_policy_group().unwrap().id,
            default_group.id
        );
        assert_eq!(snapshot.get_policy(secondary.id).unwrap().name, "Secondary");
    }

    #[tokio::test]
    async fn resolve_default_policy_uses_assigned_group_and_does_not_fall_back() {
        let db = setup_db().await;
        let system_default =
            create_policy(&db, "System Default", "/tmp/policy-snap-fallback", true).await;
        let user_default = create_policy(&db, "User Default", "/tmp/policy-snap-user", false).await;
        create_group(&db, "System Default Group", system_default.id, true, 0, 0).await;
        let user_default_group =
            create_group(&db, "User Default Group", user_default.id, false, 0, 0).await;

        let user = create_user(
            &db,
            "policy_snapshot_user",
            "policy_snapshot_user@example.com",
        )
        .await;
        let mut user_active: aster_drive_model::entities::user::ActiveModel = user.clone().into();
        user_active.policy_group_id = Set(Some(user_default_group.id));
        user_active.update(&db).await.unwrap();

        let snapshot = PolicySnapshot::new();
        let connectors = builtin_storage_connector_registry().unwrap();
        snapshot.reload(&db, &connectors).await.unwrap();

        assert_eq!(
            snapshot.resolve_default_policy_group_id(user.id),
            Some(user_default_group.id)
        );
        assert_eq!(
            snapshot
                .resolve_default_policy_for_size_for_test(user.id, 16)
                .unwrap()
                .id,
            user_default.id
        );
        assert!(
            snapshot
                .resolve_default_policy_for_size_for_test(9999, 16)
                .is_none()
        );
    }

    #[tokio::test]
    async fn resolve_policy_in_group_uses_size_rules() {
        let db = setup_db().await;
        let small = create_policy(&db, "Small", "/tmp/policy-snap-small", true).await;
        let large = create_policy(&db, "Large", "/tmp/policy-snap-large", false).await;
        let now = Utc::now();
        let group = policy_group_repo::create_group(
            &db,
            aster_drive_model::entities::storage_policy_group::ActiveModel {
                name: Set("Tiered".to_string()),
                description: Set(String::new()),
                is_enabled: Set(true),
                is_default: Set(true),
                created_at: Set(now),
                updated_at: Set(now),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        for (priority, policy_id, min_file_size, max_file_size) in
            [(1, small.id, 0, 10), (2, large.id, 10, 0)]
        {
            create_test_rule(
                &db,
                group.id,
                policy_id,
                priority,
                min_file_size,
                max_file_size,
            )
            .await;
        }

        let snapshot = PolicySnapshot::new();
        let connectors = builtin_storage_connector_registry().unwrap();
        snapshot.reload(&db, &connectors).await.unwrap();

        assert_eq!(
            snapshot
                .resolve_compiled_profile_for_test(group.id, 5)
                .unwrap()
                .id,
            small.id
        );
        assert_eq!(
            snapshot
                .resolve_compiled_profile_for_test(group.id, 1024)
                .unwrap()
                .id,
            large.id
        );
    }

    #[tokio::test]
    async fn resolve_policy_in_group_errors_when_no_rule_matches() {
        let db = setup_db().await;
        let small = create_policy(&db, "Small", "/tmp/policy-snap-gap-small", true).await;
        let large = create_policy(&db, "Large", "/tmp/policy-snap-gap-large", false).await;
        let now = Utc::now();
        let group = policy_group_repo::create_group(
            &db,
            aster_drive_model::entities::storage_policy_group::ActiveModel {
                name: Set("Gap".to_string()),
                description: Set(String::new()),
                is_enabled: Set(true),
                is_default: Set(true),
                created_at: Set(now),
                updated_at: Set(now),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        for (priority, policy_id, min_file_size, max_file_size) in
            [(1, small.id, 0, 10), (2, large.id, 20, 0)]
        {
            create_test_rule(
                &db,
                group.id,
                policy_id,
                priority,
                min_file_size,
                max_file_size,
            )
            .await;
        }

        let snapshot = PolicySnapshot::new();
        let connectors = builtin_storage_connector_registry().unwrap();
        snapshot.reload(&db, &connectors).await.unwrap();

        let err = snapshot
            .resolve_compiled_profile_for_test(group.id, 15)
            .unwrap_err();
        assert_eq!(err.code(), "E005");
        assert!(err.message().contains("placement_no_matching_rule"));
    }

    #[tokio::test]
    async fn resolve_policy_in_group_skips_disabled_remote_nodes() {
        let db = setup_db().await;
        let disabled_remote_node = create_remote_node(&db, "disabled-node", false).await;
        let remote_policy =
            create_remote_policy(&db, "Disabled Remote", disabled_remote_node.id).await;
        let fallback_policy =
            create_policy(&db, "Fallback Local", "/tmp/policy-snap-fallback", false).await;
        let now = Utc::now();
        let group = policy_group_repo::create_group(
            &db,
            aster_drive_model::entities::storage_policy_group::ActiveModel {
                name: Set("Remote Fallback".to_string()),
                description: Set(String::new()),
                is_enabled: Set(true),
                is_default: Set(true),
                created_at: Set(now),
                updated_at: Set(now),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        for (priority, policy_id) in [(1, remote_policy.id), (2, fallback_policy.id)] {
            create_test_rule(&db, group.id, policy_id, priority, 0, 0).await;
        }

        let snapshot = PolicySnapshot::new();
        let connectors = builtin_storage_connector_registry().unwrap();
        snapshot.reload(&db, &connectors).await.unwrap();

        assert_eq!(
            snapshot
                .resolve_compiled_profile_for_test(group.id, 5)
                .unwrap()
                .id,
            fallback_policy.id
        );
    }
}
