//! 配置子模块：`runtime_config`。

use aster_forge_config::{SyncConfigSnapshot, SyncRuntimeConfig};
use parking_lot::RwLock;
use sea_orm::ConnectionTrait;

use crate::config::audit::{self, AuditLogRuntimeSettings};
use crate::db::repository::config_repo;
use crate::errors::Result;
use aster_forge_db::system_config;

pub struct RuntimeConfig {
    snapshot: SyncRuntimeConfig<system_config::Model>,
    audit_log_settings: RwLock<AuditLogRuntimeSettings>,
}

impl RuntimeConfig {
    pub fn new() -> Self {
        Self {
            snapshot: SyncRuntimeConfig::new(),
            audit_log_settings: RwLock::new(AuditLogRuntimeSettings::default()),
        }
    }

    pub async fn reload<C: ConnectionTrait>(&self, db: &C) -> Result<()> {
        let configs = config_repo::find_all(db).await?;
        let next_snapshot = SyncConfigSnapshot::from_configs(configs.clone());
        let audit_log_settings = build_audit_log_settings(&next_snapshot);
        self.snapshot.replace(configs);
        *self.audit_log_settings.write() = audit_log_settings;
        Ok(())
    }

    pub fn get_model(&self, key: &str) -> Option<system_config::Model> {
        self.snapshot.get_model(key)
    }

    pub fn get(&self, key: &str) -> Option<String> {
        self.snapshot.get(key)
    }

    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.snapshot.get_bool(key)
    }

    pub fn get_i64(&self, key: &str) -> Option<i64> {
        self.snapshot.get_i64(key)
    }

    pub fn get_u64(&self, key: &str) -> Option<u64> {
        self.snapshot.get_u64(key)
    }

    pub fn get_string_or(&self, key: &str, default: &str) -> String {
        self.snapshot.get_string_or(key, default)
    }

    pub fn get_bool_or(&self, key: &str, default: bool) -> bool {
        self.snapshot.get_bool_or(key, default)
    }

    pub fn should_record_audit_action(&self, action: crate::types::AuditAction) -> bool {
        self.audit_log_settings.read().should_record(action)
    }

    pub fn get_i64_or(&self, key: &str, default: i64) -> i64 {
        self.snapshot.get_i64_or(key, default)
    }

    pub fn get_u64_or(&self, key: &str, default: u64) -> u64 {
        self.snapshot.get_u64_or(key, default)
    }

    pub fn apply(&self, config: system_config::Model) {
        let is_audit_runtime_key = audit::is_audit_runtime_key(&config.key);
        let changed = self.snapshot.apply(config).is_some();
        if changed && is_audit_runtime_key {
            *self.audit_log_settings.write() = build_audit_log_settings(&self.snapshot.snapshot());
        }
    }

    pub fn remove(&self, key: &str) {
        let removed = self.snapshot.remove(key).is_some();
        if removed && audit::is_audit_runtime_key(key) {
            *self.audit_log_settings.write() = build_audit_log_settings(&self.snapshot.snapshot());
        }
    }
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self::new()
    }
}

fn build_audit_log_settings(
    snapshot: &SyncConfigSnapshot<system_config::Model>,
) -> AuditLogRuntimeSettings {
    let enabled = snapshot.get(audit::AUDIT_LOG_ENABLED_KEY);
    let actions = snapshot.get(audit::AUDIT_LOG_RECORDED_ACTIONS_KEY);
    AuditLogRuntimeSettings::from_raw_values(enabled, actions)
}

#[cfg(test)]
mod tests {
    use super::RuntimeConfig;
    use crate::config::DatabaseConfig;
    use crate::config::definitions::CONFIG_CATEGORY_SITE;
    use crate::db;
    use crate::db::repository::config_repo;
    use crate::types::{ConfigSource, ConfigValueType};
    use aster_forge_db::system_config;
    use chrono::Utc;
    use migration::Migrator;

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
            value_type: ConfigValueType::String,
            requires_restart,
            is_sensitive: false,
            source: ConfigSource::System,
            visibility: crate::types::ConfigVisibility::Private,
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
