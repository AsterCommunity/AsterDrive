//! AsterDrive-specific path defaults and configuration path adapters.

use std::path::Path;

use crate::errors::{AsterError, Result};

pub const DEFAULT_CONFIG_PATH: &str = "data/config.toml";
#[cfg(test)]
pub const DEFAULT_SQLITE_DATABASE_PATH: &str = "data/asterdrive.db";
pub const DEFAULT_CONFIG_SQLITE_DATABASE_URL: &str = "sqlite://asterdrive.db?mode=rwc";
#[cfg(test)]
pub const DEFAULT_SQLITE_DATABASE_URL: &str = "sqlite://data/asterdrive.db?mode=rwc";
pub const DEFAULT_CONFIG_TEMP_DIR: &str = ".tmp";
pub const DEFAULT_CONFIG_UPLOAD_TEMP_DIR: &str = ".uploads";
#[cfg(test)]
pub const DEFAULT_TEMP_DIR: &str = "data/.tmp";
#[cfg(test)]
pub const DEFAULT_UPLOAD_TEMP_DIR: &str = "data/.uploads";

fn map_config_path_error(error: aster_forge_utils::UtilsError) -> AsterError {
    AsterError::config_error(error.to_string())
}

pub fn resolve_config_relative_path(
    base_dir: &Path,
    config_dir: &Path,
    value: &str,
) -> Result<String> {
    aster_forge_utils::paths::resolve_config_relative_path(base_dir, config_dir, value)
        .map_err(map_config_path_error)
}

pub fn resolve_config_relative_sqlite_url(
    base_dir: &Path,
    config_dir: &Path,
    value: &str,
) -> Result<String> {
    aster_forge_utils::paths::resolve_config_relative_sqlite_url(base_dir, config_dir, value)
        .map_err(map_config_path_error)
}

pub fn resolve_config_relative_database_url(
    base_dir: &Path,
    config_dir: &Path,
    value: &mut aster_forge_db::DatabaseUrl,
) -> Result<()> {
    if let aster_forge_db::DatabaseUrl::Url(url) = value {
        *url = resolve_config_relative_sqlite_url(base_dir, config_dir, url)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{resolve_config_relative_path, resolve_config_relative_sqlite_url};

    #[test]
    fn relative_paths_use_drive_data_layout() {
        let base_dir = Path::new("/srv/asterdrive");
        let config_dir = Path::new("/srv/asterdrive/data");

        assert_eq!(
            resolve_config_relative_path(base_dir, config_dir, ".tmp").unwrap(),
            "data/.tmp"
        );
        assert_eq!(
            resolve_config_relative_sqlite_url(
                base_dir,
                config_dir,
                "sqlite://asterdrive.db?mode=rwc",
            )
            .unwrap(),
            "sqlite://data/asterdrive.db?mode=rwc"
        );
    }

    #[test]
    fn paths_outside_data_root_are_config_errors() {
        let error = resolve_config_relative_path(
            Path::new("/srv/asterdrive"),
            Path::new("/srv/asterdrive/data"),
            "../../shared",
        )
        .expect_err("path outside the Drive data root should fail");

        assert_eq!(error.code(), "E003");
        assert!(error.message().contains("outside data base_dir"));
    }
}
