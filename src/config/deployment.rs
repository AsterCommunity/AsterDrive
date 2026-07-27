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

    /// Whether this profile may run more than one Primary against shared state.
    ///
    /// Runtime and product code should depend on this capability instead of
    /// matching profile variants independently. This keeps the profile mapping
    /// in one place and prevents single/cluster behavior from drifting.
    pub const fn requires_shared_runtime(self) -> bool {
        matches!(self, Self::Cluster)
    }

    /// Whether state owned by one process or Pod may be used as durable data.
    pub const fn allows_instance_local_state(self) -> bool {
        !self.requires_shared_runtime()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct DeploymentConfig {
    #[serde(default)]
    pub profile: DeploymentProfile,
    /// Per-instance HTTP endpoint used by other primaries for internal data-plane proxying.
    #[serde(default)]
    pub internal_endpoint: String,
    /// Cluster-wide secret used to authenticate internal primary-to-primary proxy requests.
    #[serde(default)]
    pub internal_proxy_secret: String,
}

const INTERNAL_PROXY_SECRET_MIN_LENGTH: usize = 32;

impl DeploymentConfig {
    pub const fn requires_shared_runtime(&self) -> bool {
        self.profile.requires_shared_runtime()
    }

    pub const fn allows_instance_local_state(&self) -> bool {
        self.profile.allows_instance_local_state()
    }

    pub fn internal_proxy_enabled(&self) -> bool {
        let endpoint = self.internal_endpoint.trim();
        self.requires_shared_runtime()
            && self.internal_proxy_secret.trim().len() >= INTERNAL_PROXY_SECRET_MIN_LENGTH
            && matches!(
                url::Url::parse(endpoint),
                Ok(url)
                    if matches!(url.scheme(), "http" | "https")
                        && url.query().is_none()
                        && url.fragment().is_none()
            )
    }
}

pub fn static_issues(config: &Config, database_url_override: Option<&str>) -> Vec<String> {
    if !config.deployment.requires_shared_runtime() {
        return Vec::new();
    }

    let database_url = database_url_override.unwrap_or_else(|| match &config.database.url {
        aster_forge_db::DatabaseUrl::Url(url) => url,
        aster_forge_db::DatabaseUrl::Credentials { base_url, .. } => base_url,
    });
    let mut issues = Vec::new();
    if database_url
        .trim_start()
        .to_ascii_lowercase()
        .starts_with("sqlite:")
    {
        issues.push("cluster profile requires a shared PostgreSQL or MySQL database".to_string());
    }

    if config.cache.normalized_backend() != "redis" {
        issues.push("cluster profile requires cache.backend = \"redis\"".to_string());
    } else if cache_endpoint_is_blank(&config.cache.endpoint) {
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
    } else if config_sync_endpoint_is_blank(&config.config_sync.endpoint) {
        issues.push(
            "cluster profile requires config_sync.endpoint when config_sync.backend is redis"
                .to_string(),
        );
    }

    let internal_endpoint = config.deployment.internal_endpoint.trim();
    let internal_proxy_secret = config.deployment.internal_proxy_secret.trim();
    match (
        internal_endpoint.is_empty(),
        internal_proxy_secret.is_empty(),
    ) {
        (true, false) => issues.push(
            "cluster deployment.internal_endpoint is required when internal_proxy_secret is set"
                .to_string(),
        ),
        (false, true) => issues.push(
            "cluster deployment.internal_proxy_secret is required when internal_endpoint is set"
                .to_string(),
        ),
        (false, false) => {
            match url::Url::parse(internal_endpoint) {
                Ok(url)
                    if matches!(url.scheme(), "http" | "https")
                        && url.query().is_none()
                        && url.fragment().is_none() => {}
                _ => issues.push(
                    "cluster deployment.internal_endpoint must be an absolute http/https URL without query or fragment"
                        .to_string(),
                ),
            }
            if internal_proxy_secret.len() < INTERNAL_PROXY_SECRET_MIN_LENGTH {
                issues.push(format!(
                    "cluster deployment.internal_proxy_secret must contain at least {INTERNAL_PROXY_SECRET_MIN_LENGTH} characters"
                ));
            }
        }
        (true, true) => {}
    }

    issues
}

fn cache_endpoint_is_blank(endpoint: &aster_forge_cache::CacheEndpoint) -> bool {
    match endpoint {
        aster_forge_cache::CacheEndpoint::Url(url) => url.trim().is_empty(),
        aster_forge_cache::CacheEndpoint::Credentials { base_url, .. } => {
            base_url.trim().is_empty()
        }
    }
}

fn config_sync_endpoint_is_blank(endpoint: &aster_forge_config::ConfigSyncEndpoint) -> bool {
    match endpoint {
        aster_forge_config::ConfigSyncEndpoint::Url(url) => url.trim().is_empty(),
        aster_forge_config::ConfigSyncEndpoint::Credentials { base_url, .. } => {
            base_url.trim().is_empty()
        }
    }
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
    fn profile_capabilities_are_mapped_in_one_place() {
        assert!(!DeploymentProfile::Single.requires_shared_runtime());
        assert!(DeploymentProfile::Single.allows_instance_local_state());
        assert!(DeploymentProfile::Cluster.requires_shared_runtime());
        assert!(!DeploymentProfile::Cluster.allows_instance_local_state());
    }

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
        config.database.url = "postgres://aster:secret@db/asterdrive".into();
        config.cache.backend = "redis".to_string();
        config.cache.endpoint = "redis://redis:6379/0".into();
        config.config_sync.backend = "redis".to_string();
        config.config_sync.endpoint = "redis://redis:6379/0".into();

        validate_static(&config).expect("cluster profile should accept shared dependencies");
    }

    #[test]
    fn cluster_internal_proxy_requires_a_complete_valid_pair() {
        let mut config = Config::default();
        config.deployment.profile = DeploymentProfile::Cluster;
        config.database.url = "postgres://aster:secret@db/asterdrive".into();
        config.cache.backend = "redis".to_string();
        config.cache.endpoint = "redis://redis:6379/0".into();
        config.config_sync.backend = "redis".to_string();
        config.config_sync.endpoint = "redis://redis:6379/0".into();

        config.deployment.internal_endpoint = "http://primary-a:3000".to_string();
        let issues = static_issues(&config, None);
        assert_eq!(
            issues,
            vec![
                "cluster deployment.internal_proxy_secret is required when internal_endpoint is set"
            ]
        );

        config.deployment.internal_proxy_secret =
            "cluster-proxy-secret-at-least-32-bytes".to_string();
        assert!(static_issues(&config, None).is_empty());
        assert!(config.deployment.internal_proxy_enabled());

        config.deployment.internal_endpoint = "redis://primary-a:6379/0".to_string();
        assert_eq!(static_issues(&config, None).len(), 1);
        assert!(!config.deployment.internal_proxy_enabled());
    }

    #[test]
    fn cluster_internal_proxy_rejects_short_shared_secret() {
        let mut config = Config::default();
        config.deployment.profile = DeploymentProfile::Cluster;
        config.database.url = "postgres://aster:secret@db/asterdrive".into();
        config.cache.backend = "redis".to_string();
        config.cache.endpoint = "redis://redis:6379/0".into();
        config.config_sync.backend = "redis".to_string();
        config.config_sync.endpoint = "redis://redis:6379/0".into();
        config.deployment.internal_endpoint = "http://primary-a:3000".to_string();
        config.deployment.internal_proxy_secret = "short".to_string();

        assert!(
            static_issues(&config, None)
                .iter()
                .any(|issue| issue.contains("at least 32 characters"))
        );
        assert!(!config.deployment.internal_proxy_enabled());
    }

    #[test]
    fn doctor_can_validate_an_explicit_database_url() {
        let mut config = Config::default();
        config.deployment.profile = DeploymentProfile::Cluster;
        config.cache.backend = "redis".to_string();
        config.cache.endpoint = "redis://redis:6379/0".into();
        config.config_sync.backend = "redis".to_string();
        config.config_sync.endpoint = "redis://redis:6379/0".into();

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
        config.database.url = " \nSQLITE://data/aster.db".into();
        config.cache.backend = " ReDiS ".to_string();
        config.cache.endpoint = "redis://cache:6379/0".into();
        config.config_sync.backend = " REDIS ".to_string();
        config.config_sync.endpoint = "redis://config-sync:6379/0".into();

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
        config.database.url = "mysql://aster:secret@db/asterdrive".into();
        config.cache.backend = "redis".to_string();
        config.cache.endpoint = " \n\t".into();
        config.config_sync.backend = "redis".to_string();
        config.config_sync.endpoint = " ".into();

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
        config.database.url = "sqlite::memory:".into();
        config.cache.backend = "redis".to_string();
        config.cache.endpoint = "redis://cache:6379/0".into();
        config.config_sync.backend = "redis".to_string();
        config.config_sync.endpoint = "redis://config-sync:6379/0".into();

        assert!(static_issues(&config, Some("postgres://aster:secret@db/asterdrive")).is_empty());

        config.database.url = "postgres://aster:secret@db/asterdrive".into();
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
