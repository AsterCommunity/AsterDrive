//! 存储子模块：`policy_snapshot`。

use std::collections::{HashMap, HashSet};

use parking_lot::RwLock;
use sea_orm::DatabaseConnection;

use crate::db::repository::{managed_follower_repo, policy_group_repo, policy_repo, user_repo};
use crate::entities::{storage_policy, storage_policy_group, storage_policy_group_item};
use crate::errors::{AsterError, Result};

#[derive(Clone, Debug)]
pub struct ResolvedPolicyGroupItem {
    pub item: storage_policy_group_item::Model,
    pub policy: storage_policy::Model,
}

#[derive(Default)]
struct PolicySnapshotData {
    policies_by_id: HashMap<i64, storage_policy::Model>,
    policy_groups_by_id: HashMap<i64, storage_policy_group::Model>,
    policy_group_items_by_group_id: HashMap<i64, Vec<ResolvedPolicyGroupItem>>,
    user_policy_group_by_user_id: HashMap<i64, i64>,
    enabled_remote_node_ids: HashSet<i64>,
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

    pub async fn reload(&self, db: &DatabaseConnection) -> Result<()> {
        let policies = policy_repo::find_all(db).await?;
        let policy_groups = policy_group_repo::find_all_groups(db).await?;
        let policy_group_items = policy_group_repo::find_all_group_items(db).await?;
        let managed_followers = managed_follower_repo::find_all(db).await?;
        let users = user_repo::find_all(db).await?;

        let system_default_policy_id = policies
            .iter()
            .find(|policy| policy.is_default)
            .map(|policy| policy.id);
        let policies_by_id = policies
            .into_iter()
            .map(|policy| (policy.id, policy))
            .collect::<HashMap<_, _>>();
        let system_default_policy_group_id = policy_groups
            .iter()
            .find(|group| group.is_default)
            .map(|group| group.id);
        let policy_groups_by_id = policy_groups
            .into_iter()
            .map(|group| (group.id, group))
            .collect::<HashMap<_, _>>();

        let mut policy_group_items_by_group_id: HashMap<i64, Vec<ResolvedPolicyGroupItem>> =
            HashMap::new();
        for item in policy_group_items {
            let Some(policy) = policies_by_id.get(&item.policy_id).cloned() else {
                continue;
            };
            policy_group_items_by_group_id
                .entry(item.group_id)
                .or_default()
                .push(ResolvedPolicyGroupItem { item, policy });
        }
        for items in policy_group_items_by_group_id.values_mut() {
            items.sort_by_key(|resolved| (resolved.item.priority, resolved.item.id));
        }

        let user_policy_group_by_user_id = users
            .into_iter()
            .filter_map(|user| user.policy_group_id.map(|group_id| (user.id, group_id)))
            .collect();
        let enabled_remote_node_ids = managed_followers
            .into_iter()
            .filter(|node| node.is_enabled)
            .map(|node| node.id)
            .collect();

        *self.snapshot.write() = PolicySnapshotData {
            policies_by_id,
            policy_groups_by_id,
            policy_group_items_by_group_id,
            user_policy_group_by_user_id,
            enabled_remote_node_ids,
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
        if policy.driver_type != crate::types::DriverType::Remote {
            return None;
        }

        let Some(remote_node_id) = policy.remote_node_id else {
            return Some("remote policy has no bound remote node".to_string());
        };

        if self
            .snapshot
            .read()
            .enabled_remote_node_ids
            .contains(&remote_node_id)
        {
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

    pub fn get_policy_group_items(&self, group_id: i64) -> Vec<ResolvedPolicyGroupItem> {
        self.snapshot
            .read()
            .policy_group_items_by_group_id
            .get(&group_id)
            .cloned()
            .unwrap_or_default()
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

    pub fn resolve_policy_in_group(
        &self,
        group_id: i64,
        file_size: i64,
    ) -> Result<storage_policy::Model> {
        let group = self.get_policy_group_or_err(group_id)?;
        if !group.is_enabled {
            return Err(AsterError::validation_error(format!(
                "storage policy group #{} is disabled",
                group.id
            )));
        }

        let items = self.get_policy_group_items(group_id);
        if items.is_empty() {
            return Err(AsterError::storage_policy_not_found(format!(
                "policy group #{} has no policies",
                group_id
            )));
        }

        let mut skipped_disabled_remote = false;
        for resolved in &items {
            if matches_size_rule(&resolved.item, file_size)
                && self.policy_available_for_outbound(&resolved.policy)
            {
                return Ok(resolved.policy.clone());
            }
            if matches_size_rule(&resolved.item, file_size)
                && resolved.policy.driver_type == crate::types::DriverType::Remote
            {
                skipped_disabled_remote = true;
            }
        }

        if skipped_disabled_remote {
            return Err(AsterError::validation_error(format!(
                "no enabled storage policy rule in group #{} matches file size {}",
                group_id, file_size
            )));
        }

        Err(AsterError::validation_error(format!(
            "no storage policy rule in group #{} matches file size {}",
            group_id, file_size
        )))
    }

    pub fn resolve_user_policy_id_for_size(&self, user_id: i64, file_size: i64) -> Result<i64> {
        let group_id = self.require_user_policy_group_id(user_id)?;
        self.resolve_policy_in_group(group_id, file_size)
            .map(|policy| policy.id)
    }

    pub fn resolve_user_policy_for_size(
        &self,
        user_id: i64,
        file_size: i64,
    ) -> Result<storage_policy::Model> {
        let group_id = self.require_user_policy_group_id(user_id)?;
        self.resolve_policy_in_group(group_id, file_size)
    }

    pub fn resolve_default_policy_id(&self, user_id: i64) -> Option<i64> {
        self.resolve_default_policy_id_for_size(user_id, 0)
    }

    pub fn resolve_default_policy_id_for_size(&self, user_id: i64, file_size: i64) -> Option<i64> {
        self.resolve_user_policy_id_for_size(user_id, file_size)
            .ok()
    }

    pub fn resolve_default_policy(&self, user_id: i64) -> Option<storage_policy::Model> {
        self.resolve_default_policy_for_size(user_id, 0)
    }

    pub fn resolve_default_policy_for_size(
        &self,
        user_id: i64,
        file_size: i64,
    ) -> Option<storage_policy::Model> {
        self.resolve_user_policy_for_size(user_id, file_size).ok()
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

    pub fn remove_user_policy_group(&self, user_id: i64) {
        self.snapshot
            .write()
            .user_policy_group_by_user_id
            .remove(&user_id);
    }

    fn policy_available_for_outbound(&self, policy: &storage_policy::Model) -> bool {
        if policy.driver_type != crate::types::DriverType::Remote {
            return true;
        }

        let Some(remote_node_id) = policy.remote_node_id else {
            return false;
        };

        self.snapshot
            .read()
            .enabled_remote_node_ids
            .contains(&remote_node_id)
    }
}

fn matches_size_rule(item: &storage_policy_group_item::Model, file_size: i64) -> bool {
    if file_size < item.min_file_size {
        return false;
    }
    item.max_file_size == 0 || file_size < item.max_file_size
}

impl Default for PolicySnapshot {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::PolicySnapshot;
    use crate::config::DatabaseConfig;
    use crate::db;
    use crate::db::repository::{managed_follower_repo, policy_group_repo, policy_repo, user_repo};
    use crate::types::{DriverType, UserRole, UserStatus};
    use chrono::Utc;
    use migration::Migrator;
    use sea_orm::{ActiveModelTrait, Set};

    async fn setup_db() -> sea_orm::DatabaseConnection {
        let db = db::connect_with_metrics(
            &DatabaseConfig {
                url: "sqlite::memory:".to_string(),
                pool_size: 1,
                retry_count: 0,
            },
            crate::metrics::NoopMetrics::arc(),
        )
        .await
        .unwrap();
        Migrator::up(&db, None).await.unwrap();
        db
    }

    async fn create_policy(
        db: &sea_orm::DatabaseConnection,
        name: &str,
        base_path: &str,
        is_default: bool,
    ) -> crate::entities::storage_policy::Model {
        let now = Utc::now();
        policy_repo::create(
            db,
            crate::entities::storage_policy::ActiveModel {
                name: Set(name.to_string()),
                driver_type: Set(DriverType::Local),
                endpoint: Set(String::new()),
                bucket: Set(String::new()),
                access_key: Set(String::new()),
                secret_key: Set(String::new()),
                base_path: Set(base_path.to_string()),
                max_file_size: Set(0),
                allowed_types: Set(crate::types::StoredStoragePolicyAllowedTypes::empty()),
                options: Set(crate::types::StoredStoragePolicyOptions::empty()),
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
    ) -> crate::entities::managed_follower::Model {
        let now = Utc::now();
        managed_follower_repo::create(
            db,
            crate::entities::managed_follower::ActiveModel {
                name: Set(name.to_string()),
                base_url: Set("https://remote.example.com".to_string()),
                access_key: Set(format!("ak_{name}")),
                secret_key: Set(format!("sk_{name}")),
                is_enabled: Set(is_enabled),
                last_capabilities: Set("{}".to_string()),
                last_error: Set(String::new()),
                last_checked_at: Set(None),
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
    ) -> crate::entities::storage_policy::Model {
        let now = Utc::now();
        policy_repo::create(
            db,
            crate::entities::storage_policy::ActiveModel {
                name: Set(name.to_string()),
                driver_type: Set(DriverType::Remote),
                endpoint: Set(String::new()),
                bucket: Set(String::new()),
                access_key: Set(String::new()),
                secret_key: Set(String::new()),
                base_path: Set(String::new()),
                remote_node_id: Set(Some(remote_node_id)),
                max_file_size: Set(0),
                allowed_types: Set(crate::types::StoredStoragePolicyAllowedTypes::empty()),
                options: Set(crate::types::StoredStoragePolicyOptions::empty()),
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
    ) -> crate::entities::storage_policy_group::Model {
        let now = Utc::now();
        let group = policy_group_repo::create_group(
            db,
            crate::entities::storage_policy_group::ActiveModel {
                name: Set(name.to_string()),
                description: Set(String::new()),
                is_enabled: Set(true),
                is_default: Set(is_default),
                created_at: Set(now),
                updated_at: Set(now),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        policy_group_repo::create_group_item(
            db,
            crate::entities::storage_policy_group_item::ActiveModel {
                group_id: Set(group.id),
                policy_id: Set(policy_id),
                priority: Set(1),
                min_file_size: Set(min_file_size),
                max_file_size: Set(max_file_size),
                created_at: Set(now),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        group
    }

    async fn create_user(
        db: &sea_orm::DatabaseConnection,
        username: &str,
        email: &str,
    ) -> crate::entities::user::Model {
        let now = Utc::now();
        user_repo::create(
            db,
            crate::entities::user::ActiveModel {
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

    #[tokio::test]
    async fn reload_exposes_policies_and_system_default_group() {
        let db = setup_db().await;
        let system_default =
            create_policy(&db, "System Default", "/tmp/policy-snap-default", true).await;
        let secondary = create_policy(&db, "Secondary", "/tmp/policy-snap-secondary", false).await;
        let default_group = create_group(&db, "Default Group", system_default.id, true, 0, 0).await;
        let snapshot = PolicySnapshot::new();

        snapshot.reload(&db).await.unwrap();

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
        let mut user_active: crate::entities::user::ActiveModel = user.clone().into();
        user_active.policy_group_id = Set(Some(user_default_group.id));
        user_active.update(&db).await.unwrap();

        let snapshot = PolicySnapshot::new();
        snapshot.reload(&db).await.unwrap();

        assert_eq!(
            snapshot.resolve_default_policy_group_id(user.id),
            Some(user_default_group.id)
        );
        assert_eq!(
            snapshot
                .resolve_default_policy_for_size(user.id, 16)
                .unwrap()
                .id,
            user_default.id
        );
        assert!(snapshot.resolve_default_policy_for_size(9999, 16).is_none());
    }

    #[tokio::test]
    async fn resolve_policy_in_group_uses_size_rules() {
        let db = setup_db().await;
        let small = create_policy(&db, "Small", "/tmp/policy-snap-small", true).await;
        let large = create_policy(&db, "Large", "/tmp/policy-snap-large", false).await;
        let now = Utc::now();
        let group = policy_group_repo::create_group(
            &db,
            crate::entities::storage_policy_group::ActiveModel {
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
            policy_group_repo::create_group_item(
                &db,
                crate::entities::storage_policy_group_item::ActiveModel {
                    group_id: Set(group.id),
                    policy_id: Set(policy_id),
                    priority: Set(priority),
                    min_file_size: Set(min_file_size),
                    max_file_size: Set(max_file_size),
                    created_at: Set(now),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        }

        let snapshot = PolicySnapshot::new();
        snapshot.reload(&db).await.unwrap();

        assert_eq!(
            snapshot.resolve_policy_in_group(group.id, 5).unwrap().id,
            small.id
        );
        assert_eq!(
            snapshot.resolve_policy_in_group(group.id, 1024).unwrap().id,
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
            crate::entities::storage_policy_group::ActiveModel {
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
            policy_group_repo::create_group_item(
                &db,
                crate::entities::storage_policy_group_item::ActiveModel {
                    group_id: Set(group.id),
                    policy_id: Set(policy_id),
                    priority: Set(priority),
                    min_file_size: Set(min_file_size),
                    max_file_size: Set(max_file_size),
                    created_at: Set(now),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        }

        let snapshot = PolicySnapshot::new();
        snapshot.reload(&db).await.unwrap();

        let err = snapshot.resolve_policy_in_group(group.id, 15).unwrap_err();
        assert_eq!(err.code(), "E005");
        assert!(err.message().contains("no storage policy rule"));
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
            crate::entities::storage_policy_group::ActiveModel {
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
            policy_group_repo::create_group_item(
                &db,
                crate::entities::storage_policy_group_item::ActiveModel {
                    group_id: Set(group.id),
                    policy_id: Set(policy_id),
                    priority: Set(priority),
                    min_file_size: Set(0),
                    max_file_size: Set(0),
                    created_at: Set(now),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        }

        let snapshot = PolicySnapshot::new();
        snapshot.reload(&db).await.unwrap();

        assert_eq!(
            snapshot.resolve_policy_in_group(group.id, 5).unwrap().id,
            fallback_policy.id
        );
    }
}
