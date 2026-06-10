//! 配置子模块：`runtime_config`。

use std::collections::HashMap;

use parking_lot::RwLock;
use sea_orm::DatabaseConnection;

use crate::config::audit::{self, AuditLogRuntimeSettings};
use crate::db::repository::config_repo;
use crate::entities::system_config;
use crate::errors::Result;

pub struct RuntimeConfig {
    snapshot: RwLock<HashMap<String, system_config::Model>>,
    audit_log_settings: RwLock<AuditLogRuntimeSettings>,
}

impl RuntimeConfig {
    pub fn new() -> Self {
        Self {
            snapshot: RwLock::new(HashMap::new()),
            audit_log_settings: RwLock::new(AuditLogRuntimeSettings::default()),
        }
    }

    pub async fn reload(&self, db: &DatabaseConnection) -> Result<()> {
        let configs = config_repo::find_all(db).await?;
        let snapshot = configs
            .into_iter()
            .map(|config| (config.key.clone(), config))
            .collect();
        let audit_log_settings = build_audit_log_settings(&snapshot);
        *self.snapshot.write() = snapshot;
        *self.audit_log_settings.write() = audit_log_settings;
        Ok(())
    }

    pub fn get_model(&self, key: &str) -> Option<system_config::Model> {
        self.snapshot.read().get(key).cloned()
    }

    pub fn get(&self, key: &str) -> Option<String> {
        self.get_model(key).map(|config| config.value)
    }

    pub fn get_bool(&self, key: &str) -> Option<bool> {
        let value = self.get(key)?;
        parse_bool(&value)
    }

    pub fn get_i64(&self, key: &str) -> Option<i64> {
        self.get(key)?.trim().parse().ok()
    }

    pub fn get_u64(&self, key: &str) -> Option<u64> {
        self.get(key)?.trim().parse().ok()
    }

    pub fn get_string_or(&self, key: &str, default: &str) -> String {
        self.get(key).unwrap_or_else(|| default.to_string())
    }

    pub fn get_bool_or(&self, key: &str, default: bool) -> bool {
        self.get_bool(key).unwrap_or(default)
    }

    pub fn should_record_audit_action(&self, action: crate::types::AuditAction) -> bool {
        self.audit_log_settings.read().should_record(action)
    }

    pub fn get_i64_or(&self, key: &str, default: i64) -> i64 {
        self.get_i64(key).unwrap_or(default)
    }

    pub fn get_u64_or(&self, key: &str, default: u64) -> u64 {
        self.get_u64(key).unwrap_or(default)
    }

    pub fn apply(&self, config: system_config::Model) {
        let mut snapshot = self.snapshot.write();

        if config.requires_restart && snapshot.contains_key(&config.key) {
            return;
        }

        let is_audit_runtime_key = audit::is_audit_runtime_key(&config.key);
        snapshot.insert(config.key.clone(), config);
        if is_audit_runtime_key {
            *self.audit_log_settings.write() = build_audit_log_settings(&snapshot);
        }
    }

    pub fn remove(&self, key: &str) {
        let mut snapshot = self.snapshot.write();
        snapshot.remove(key);
        if audit::is_audit_runtime_key(key) {
            *self.audit_log_settings.write() = build_audit_log_settings(&snapshot);
        }
    }
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self::new()
    }
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Some(true),
        "false" | "0" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn build_audit_log_settings(
    snapshot: &HashMap<String, system_config::Model>,
) -> AuditLogRuntimeSettings {
    let enabled = snapshot
        .get(audit::AUDIT_LOG_ENABLED_KEY)
        .map(|config| config.value.as_str());
    let actions = snapshot
        .get(audit::AUDIT_LOG_RECORDED_ACTIONS_KEY)
        .map(|config| config.value.as_str());
    AuditLogRuntimeSettings::from_raw_values(enabled, actions)
}

#[cfg(test)]
mod tests {
    use super::RuntimeConfig;
    use crate::config::DatabaseConfig;
    use crate::config::definitions::CONFIG_CATEGORY_SITE;
    use crate::db;
    use crate::db::repository::config_repo;
    use crate::entities::system_config;
    use crate::types::{SystemConfigSource, SystemConfigValueType};
    use chrono::Utc;
    use migration::Migrator;

    async fn setup_db() -> sea_orm::DatabaseConnection {
        let db = db::connect_with_metrics(
            &DatabaseConfig {
                url: "sqlite::memory:".to_string(),
                pool_size: 1,
                retry_count: 0,
            },
            crate::metrics_core::NoopMetrics::arc(),
        )
        .await
        .unwrap();
        Migrator::up(&db, None).await.unwrap();
        config_repo::ensure_defaults_with_env(&db, &|_| None)
            .await
            .unwrap();
        db
    }

    fn model(key: &str, value: &str, requires_restart: bool) -> system_config::Model {
        system_config::Model {
            id: 1,
            key: key.to_string(),
            value: value.to_string(),
            value_type: SystemConfigValueType::String,
            requires_restart,
            is_sensitive: false,
            source: SystemConfigSource::System,
            visibility: crate::types::SystemConfigVisibility::Private,
            namespace: String::new(),
            category: CONFIG_CATEGORY_SITE.to_string(),
            description: "test".to_string(),
            updated_at: Utc::now(),
            updated_by: None,
        }
    }

    #[tokio::test]
    async fn reload_loads_defaults_and_remove_hides_values() {
        let db = setup_db().await;
        let runtime_config = RuntimeConfig::new();

        runtime_config.reload(&db).await.unwrap();
        assert_eq!(runtime_config.get_bool("webdav_enabled"), Some(true));
        assert_eq!(runtime_config.get_i64("max_versions_per_file"), Some(10));

        runtime_config.remove("webdav_enabled");
        assert_eq!(runtime_config.get("webdav_enabled"), None);
    }

    #[tokio::test]
    async fn apply_updates_existing_runtime_values() {
        let db = setup_db().await;
        let runtime_config = RuntimeConfig::new();
        runtime_config.reload(&db).await.unwrap();

        let mut updated = config_repo::find_by_key(&db, "gravatar_base_url")
            .await
            .unwrap()
            .unwrap();
        updated.value = "https://mirror.example.com/avatar".to_string();

        runtime_config.apply(updated);

        assert_eq!(
            runtime_config.get("gravatar_base_url").as_deref(),
            Some("https://mirror.example.com/avatar")
        );
    }

    #[tokio::test]
    async fn reload_and_apply_keep_precompiled_audit_scope_current() {
        let db = setup_db().await;
        let runtime_config = RuntimeConfig::new();
        runtime_config.reload(&db).await.unwrap();

        assert!(runtime_config.should_record_audit_action(crate::types::AuditAction::FileDownload));

        runtime_config.apply(model(
            "audit_log_recorded_actions",
            r#"["user_login"]"#,
            false,
        ));
        assert!(runtime_config.should_record_audit_action(crate::types::AuditAction::UserLogin));
        assert!(
            !runtime_config.should_record_audit_action(crate::types::AuditAction::FileDownload)
        );

        runtime_config.apply(model("audit_log_enabled", "false", false));
        assert!(!runtime_config.should_record_audit_action(crate::types::AuditAction::UserLogin));
    }

    #[tokio::test]
    async fn apply_keeps_existing_value_when_config_requires_restart() {
        let runtime_config = RuntimeConfig::new();
        runtime_config.apply(model("test.requires_restart", "old", false));
        runtime_config.apply(model("test.requires_restart", "new", true));

        assert_eq!(
            runtime_config.get("test.requires_restart").as_deref(),
            Some("old")
        );
    }
}
