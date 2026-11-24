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
        if let Some(url) = database_url {
            if url.trim().is_empty() {
                return Err(AsterError::validation_error(
                    "--database-url must not be empty",
                ));
            }
            config.database.url = url.into();
        }
        let database = DatabaseConfig {
            pool_size: 1,
            retry_count: 0,
            ..config.database.clone()
        };
        Ok(Self { config, database })
    }
}
