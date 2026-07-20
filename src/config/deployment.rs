//! Static deployment topology configuration.

use serde::{Deserialize, Serialize};

use super::schema::Config;
use crate::errors::{AsterError, Result};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DeploymentProfile {
    #[default]
    Single,
    Cluster,
}

impl DeploymentProfile {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Single => "single",
            Self::Cluster => "cluster",
        }
    }

    pub const fn is_cluster(self) -> bool {
        matches!(self, Self::Cluster)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct DeploymentConfig {
    #[serde(default)]
    pub profile: DeploymentProfile,
}

pub fn static_issues(config: &Config, database_url_override: Option<&str>) -> Vec<String> {
    if !config.deployment.profile.is_cluster() {
        return Vec::new();
    }

    let database_url = database_url_override.unwrap_or(&config.database.url);
    let mut issues = Vec::new();
    if database_url
        .trim_start()
        .to_ascii_lowercase()
        .starts_with("sqlite:")
    {
        issues.push("cluster profile requires a shared PostgreSQL or MySQL database".to_string());
    }

    if !config.cache.backend.trim().eq_ignore_ascii_case("redis") {
        issues.push("cluster profile requires cache.backend = \"redis\"".to_string());
    } else if config.cache.endpoint.trim().is_empty() {
        issues.push(
            "cluster profile requires cache.endpoint when cache.backend is redis".to_string(),
        );
    }

    if !config
        .config_sync
        .backend
        .trim()
        .eq_ignore_ascii_case("redis")
    {
        issues.push("cluster profile requires config_sync.backend = \"redis\"".to_string());
    } else if config.config_sync.endpoint.trim().is_empty() {
        issues.push(
            "cluster profile requires config_sync.endpoint when config_sync.backend is redis"
                .to_string(),
        );
    }

    issues
}

pub fn validate_static(config: &Config) -> Result<()> {
    let issues = static_issues(config, None);
    if issues.is_empty() {
        return Ok(());
    }

    Err(AsterError::config_error(format!(
        "invalid deployment profile '{}': {}",
        config.deployment.profile.as_str(),
        issues.join("; ")
    )))
}

#[cfg(test)]
mod tests {
    use super::{DeploymentProfile, static_issues, validate_static};
    use crate::config::Config;

    #[test]
    fn single_profile_keeps_default_single_node_dependencies() {
        let config = Config::default();

        assert!(static_issues(&config, None).is_empty());
        validate_static(&config).expect("single profile should accept default dependencies");
    }

    #[test]
    fn cluster_profile_requires_shared_dependencies() {
        let mut config = Config::default();
        config.deployment.profile = DeploymentProfile::Cluster;

        let issues = static_issues(&config, None);
        assert_eq!(issues.len(), 3);
        assert!(
            issues
                .iter()
                .any(|issue| issue.contains("shared PostgreSQL"))
        );
        assert!(issues.iter().any(|issue| issue.contains("cache.backend")));
        assert!(
            issues
                .iter()
                .any(|issue| issue.contains("config_sync.backend"))
        );
        assert!(validate_static(&config).is_err());
    }

    #[test]
    fn cluster_profile_accepts_shared_dependencies() {
        let mut config = Config::default();
        config.deployment.profile = DeploymentProfile::Cluster;
        config.database.url = "postgres://aster:secret@db/asterdrive".to_string();
        config.cache.backend = "redis".to_string();
        config.cache.endpoint = "redis://redis:6379/0".to_string();
        config.config_sync.backend = "redis".to_string();
        config.config_sync.endpoint = "redis://redis:6379/0".to_string();

        validate_static(&config).expect("cluster profile should accept shared dependencies");
    }

    #[test]
    fn doctor_can_validate_an_explicit_database_url() {
        let mut config = Config::default();
        config.deployment.profile = DeploymentProfile::Cluster;
        config.cache.backend = "redis".to_string();
        config.cache.endpoint = "redis://redis:6379/0".to_string();
        config.config_sync.backend = "redis".to_string();
        config.config_sync.endpoint = "redis://redis:6379/0".to_string();

        let issues = static_issues(&config, Some("postgres://aster:secret@db/asterdrive"));
        assert!(issues.is_empty());
    }

    #[test]
    fn single_profile_ignores_cluster_only_dependency_rules() {
        let config = Config::default();

        assert!(static_issues(&config, Some("  sqlite::memory:")).is_empty());
    }

    #[test]
    fn cluster_profile_matches_sqlite_and_redis_case_insensitively() {
        let mut config = Config::default();
        config.deployment.profile = DeploymentProfile::Cluster;
        config.database.url = " \nSQLITE://data/aster.db".to_string();
        config.cache.backend = " ReDiS ".to_string();
        config.cache.endpoint = "redis://cache:6379/0".to_string();
        config.config_sync.backend = " REDIS ".to_string();
        config.config_sync.endpoint = "redis://config-sync:6379/0".to_string();

        let issues = static_issues(&config, None);
        assert_eq!(
            issues,
            vec!["cluster profile requires a shared PostgreSQL or MySQL database"]
        );
    }

    #[test]
    fn cluster_profile_requires_non_blank_redis_endpoints() {
        let mut config = Config::default();
        config.deployment.profile = DeploymentProfile::Cluster;
        config.database.url = "mysql://aster:secret@db/asterdrive".to_string();
        config.cache.backend = "redis".to_string();
        config.cache.endpoint = " \n\t".to_string();
        config.config_sync.backend = "redis".to_string();
        config.config_sync.endpoint = " ".to_string();

        assert_eq!(
            static_issues(&config, None),
            vec![
                "cluster profile requires cache.endpoint when cache.backend is redis",
                "cluster profile requires config_sync.endpoint when config_sync.backend is redis",
            ]
        );
    }

    #[test]
    fn explicit_database_url_override_is_authoritative() {
        let mut config = Config::default();
        config.deployment.profile = DeploymentProfile::Cluster;
        config.database.url = "sqlite::memory:".to_string();
        config.cache.backend = "redis".to_string();
        config.cache.endpoint = "redis://cache:6379/0".to_string();
        config.config_sync.backend = "redis".to_string();
        config.config_sync.endpoint = "redis://config-sync:6379/0".to_string();

        assert!(static_issues(&config, Some("postgres://aster:secret@db/asterdrive")).is_empty());

        config.database.url = "postgres://aster:secret@db/asterdrive".to_string();
        assert_eq!(
            static_issues(&config, Some(" sqlite::memory:")),
            vec!["cluster profile requires a shared PostgreSQL or MySQL database"]
        );
    }

    #[test]
    fn validate_static_aggregates_all_cluster_issues_in_stable_order() {
        let mut config = Config::default();
        config.deployment.profile = DeploymentProfile::Cluster;

        let error = validate_static(&config).expect_err("invalid cluster config should fail");
        assert_eq!(
            error.to_string(),
            "Configuration Error: invalid deployment profile 'cluster': cluster profile requires a shared PostgreSQL or MySQL database; cluster profile requires cache.backend = \"redis\"; cluster profile requires config_sync.backend = \"redis\""
        );
    }
}
