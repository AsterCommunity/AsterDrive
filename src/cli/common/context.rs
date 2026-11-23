//! Resolve the instance once; operational reads must never bootstrap a deployment.
use crate::config::{Config, ConfigLocation, DatabaseConfig};
use crate::errors::{AsterError, Result};

pub struct CliContext {
    pub config: Config,
    pub database: DatabaseConfig,
}

impl CliContext {
    pub fn existing(location: &ConfigLocation, database_url: Option<&str>) -> Result<Self> {
        let require_existing =
            database_url.is_none() && std::env::var_os("ASTER__DATABASE__URL").is_none();
        let mut config = location.load_read_only(require_existing)?;
        let mut database = config.database.clone();
        if let Some(url) = database_url {
            if url.trim().is_empty() {
                return Err(AsterError::validation_error(
                    "--database-url must not be empty",
                ));
            }
            database.url = url.into();
        }
        config.database.url = database.url.clone();
        aster_forge_config::ValidatableConfig::validate(&config)
            .map_err(|error| AsterError::config_error(error.to_string()))?;
        database.pool_size = 1;
        database.retry_count = 0;
        Ok(Self { config, database })
    }
}

#[cfg(test)]
mod tests {
    use super::CliContext;
    use crate::config::ConfigLocation;

    #[test]
    fn database_url_override_precedes_cross_section_deployment_validation() {
        let directory = aster_forge_test::temp::TestTempDir::new("cli-config-database-override");
        let path = directory.path().join("config.toml");
        std::fs::write(
            &path,
            "[deployment]\nprofile = 'cluster'\n\
             [database]\nurl = 'sqlite::memory:'\n\
             [cache]\nbackend = 'redis'\nendpoint = 'redis://cache:6379/0'\n\
             [config_sync]\nbackend = 'redis'\nendpoint = 'redis://config:6379/0'\n",
        )
        .unwrap();
        let location = ConfigLocation::resolve(Some(&path), None).unwrap();

        let context = CliContext::existing(
            &location,
            Some("postgres://user:password@database:5432/asterdrive"),
        )
        .unwrap();

        let expected: crate::config::DatabaseConfig = crate::config::DatabaseConfig {
            url: "postgres://user:password@database:5432/asterdrive".into(),
            pool_size: 1,
            retry_count: 0,
        };
        assert_eq!(context.database.url, expected.url);
        assert_eq!(context.config.database.url, context.database.url);
    }
}
