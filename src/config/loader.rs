//! 配置子模块：`loader`。

use super::schema::Config;
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::utils::paths::{
    DEFAULT_CONFIG_PATH, resolve_config_relative_path, resolve_config_relative_sqlite_url,
};
use config::{Config as RawConfig, Environment, File, FileFormat};
use std::path::{Path, PathBuf};
use toml_edit::{DocumentMut, Item, Table, value};

pub fn load() -> Result<Config> {
    let base_dir = std::env::current_dir()
        .map_aster_err_ctx("failed to resolve current dir", AsterError::config_error)?;
    let env_database_url = std::env::var("ASTER__DATABASE__URL").ok();
    load_from_dir(&base_dir, env_database_url.as_deref(), true)
}

pub fn ensure_default_config_for_current_dir(default: &Config) -> Result<PathBuf> {
    let base_dir = std::env::current_dir()
        .map_aster_err_ctx("failed to resolve current dir", AsterError::config_error)?;
    let config_path = base_dir.join(DEFAULT_CONFIG_PATH);
    ensure_default_config_exists(&config_path, default)?;
    Ok(config_path)
}

fn load_from_dir(
    base_dir: &Path,
    env_database_url: Option<&str>,
    include_env: bool,
) -> Result<Config> {
    let config_path = base_dir.join(DEFAULT_CONFIG_PATH);

    ensure_default_config_exists(&config_path, &Config::default())?;
    let migrated_config = migrate_legacy_config_file(&config_path)?;
    let stable_defaults_config =
        ensure_stable_default_config_keys(&config_path, migrated_config.as_deref())?;
    let config_content = stable_defaults_config.or(migrated_config);

    let mut builder = RawConfig::builder();
    builder = match config_content {
        Some(config_content) => {
            builder.add_source(File::from_str(&config_content, FileFormat::Toml))
        }
        None => builder.add_source(File::from(config_path.as_path()).required(false)),
    };

    if include_env {
        builder = builder.add_source(
            Environment::with_prefix("ASTER")
                .separator("__")
                .try_parsing(true),
        );
    } else if let Some(database_url) = env_database_url {
        builder = builder
            .set_override("database.url", database_url)
            .map_aster_err(AsterError::config_error)?;
    }

    let raw = builder.build().map_aster_err(AsterError::config_error)?;
    reject_deprecated_config_keys(&raw)?;

    let mut cfg = raw
        .try_deserialize::<Config>()
        .map_aster_err(AsterError::config_error)?;

    resolve_loaded_paths(base_dir, &config_path, &mut cfg)?;

    eprintln!(
        "[INFO] Configuration loaded from: {}",
        config_path.display()
    );
    Ok(cfg)
}

fn reject_deprecated_config_keys(raw: &RawConfig) -> Result<()> {
    if raw.get_string("server.managed_ingress_local_root").is_ok() {
        return Err(AsterError::config_error(
            "server.managed_ingress_local_root has moved to server.follower.managed_ingress_local_root",
        ));
    }
    Ok(())
}

// TODO: 0.3.0 版本需要删除这段兼容迁移；旧配置应只保留 [network_trust].trusted_proxies。
fn migrate_legacy_config_file(config_path: &Path) -> Result<Option<String>> {
    let content = match std::fs::read_to_string(config_path) {
        Ok(content) => content,
        Err(error) => {
            return Err(AsterError::config_error(format!(
                "failed to read {}: {error}",
                config_path.display()
            )));
        }
    };

    let mut doc = content.parse::<DocumentMut>().map_err(|error| {
        AsterError::config_error(format!(
            "failed to parse {}: {error}",
            config_path.display()
        ))
    })?;

    let legacy_trusted_proxies = {
        let Some(rate_limit_item) = doc.get("rate_limit") else {
            return Ok(None);
        };
        let Some(rate_limit_table) = rate_limit_item.as_table() else {
            return Ok(None);
        };
        rate_limit_table.get("trusted_proxies").cloned()
    };

    let Some(legacy_trusted_proxies) = legacy_trusted_proxies else {
        return Ok(None);
    };

    if let Some(network_trust_item) = doc.get("network_trust")
        && network_trust_item.as_table().is_none()
    {
        return Err(AsterError::config_error(
            "failed to migrate legacy trusted_proxies: network_trust must be a table",
        ));
    }

    let network_trust_item = doc
        .as_table_mut()
        .entry("network_trust")
        .or_insert(Item::Table(Table::new()));
    if let Some(network_trust_table) = network_trust_item.as_table_mut()
        && !network_trust_table.contains_key("trusted_proxies")
    {
        network_trust_table.insert("trusted_proxies", legacy_trusted_proxies);
    }

    if let Some(rate_limit_item) = doc.as_table_mut().get_mut("rate_limit")
        && let Some(rate_limit_table) = rate_limit_item.as_table_mut()
    {
        rate_limit_table.remove("trusted_proxies");
    }

    let migrated_content = doc.to_string();

    if let Err(error) = std::fs::write(config_path, &migrated_content) {
        eprintln!(
            "[WARN] Failed to write migrated configuration to {}: {error}. Using migrated config in memory.",
            config_path.display()
        );
    } else {
        eprintln!(
            "[INFO] Migrated legacy configuration in: {}",
            config_path.display()
        );
    }
    Ok(Some(migrated_content))
}

fn ensure_default_config_exists(config_path: &Path, default: &Config) -> Result<()> {
    if config_path.exists() {
        return Ok(());
    }

    create_default_config(config_path, default)
}

fn create_default_config(config_path: &Path, default: &Config) -> Result<()> {
    let toml_str = toml::to_string_pretty(default).map_aster_err(AsterError::config_error)?;

    let content = format!(
        "# AsterDrive configuration file\n\
         # Generated on first startup; edit as needed.\n\
         # Relative paths are resolved against the directory containing this file (default: ./data).\n\
         # Docs: https://asterdrive.docs.esap.cc/config/\n\n\
         {toml_str}"
    );

    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent).map_aster_err_ctx(
            &format!("failed to create config dir '{}'", parent.display()),
            AsterError::config_error,
        )?;
    }

    std::fs::write(config_path, &content).map_aster_err_ctx(
        &format!("failed to write {}", config_path.display()),
        AsterError::config_error,
    )?;

    eprintln!(
        "[INFO] Default configuration written to: {}",
        config_path.display()
    );
    eprintln!("[INFO] Please review and modify it as needed.");
    Ok(())
}

fn ensure_stable_default_config_keys(
    config_path: &Path,
    current_content: Option<&str>,
) -> Result<Option<String>> {
    let content = match current_content {
        Some(content) => content.to_string(),
        None => std::fs::read_to_string(config_path).map_err(|error| {
            AsterError::config_error(format!("failed to read {}: {error}", config_path.display()))
        })?,
    };

    let mut doc = content.parse::<DocumentMut>().map_err(|error| {
        AsterError::config_error(format!(
            "failed to parse {}: {error}",
            config_path.display()
        ))
    })?;

    let mut changed = false;
    let auth_item = doc
        .as_table_mut()
        .entry("auth")
        .or_insert(Item::Table(Table::new()));
    let Some(auth_table) = auth_item.as_table_mut() else {
        return Err(AsterError::config_error("auth must be a table"));
    };
    if !auth_table.contains_key("mfa_secret_key") {
        auth_table.insert(
            "mfa_secret_key",
            value(crate::config::AuthConfig::default().mfa_secret_key),
        );
        changed = true;
    }

    if !changed {
        return Ok(None);
    }

    let updated = doc.to_string();
    if let Err(error) = std::fs::write(config_path, &updated) {
        eprintln!(
            "[ERROR] Failed to write generated stable configuration keys to {}: {error}. Fix config file permissions before starting.",
            config_path.display()
        );
        return Err(AsterError::config_error(format!(
            "failed to persist generated stable configuration keys to {}: {error}",
            config_path.display()
        )));
    } else {
        eprintln!(
            "[INFO] Added generated stable configuration keys to: {}",
            config_path.display()
        );
    }
    Ok(Some(updated))
}

fn resolve_loaded_paths(base_dir: &Path, config_path: &Path, cfg: &mut Config) -> Result<()> {
    let config_dir = config_path.parent().unwrap_or(base_dir);

    cfg.server.temp_dir = resolve_config_relative_path(base_dir, config_dir, &cfg.server.temp_dir)?;
    cfg.server.upload_temp_dir =
        resolve_config_relative_path(base_dir, config_dir, &cfg.server.upload_temp_dir)?;
    cfg.server.follower.managed_ingress_local_root = resolve_config_relative_path(
        base_dir,
        config_dir,
        &cfg.server.follower.managed_ingress_local_root,
    )?;
    cfg.database.url = resolve_config_relative_sqlite_url(base_dir, config_dir, &cfg.database.url)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{ensure_default_config_exists, load_from_dir};
    use crate::config::{Config, node_mode::NodeRuntimeMode};
    use crate::utils::paths::{
        DEFAULT_CONFIG_PATH, DEFAULT_SQLITE_DATABASE_PATH, DEFAULT_SQLITE_DATABASE_URL,
        DEFAULT_TEMP_DIR, DEFAULT_UPLOAD_TEMP_DIR,
    };
    use std::path::{Path, PathBuf};

    fn make_temp_dir(test_name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "asterdrive-config-loader-{test_name}-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(path: &Path, content: &[u8]) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, content).unwrap();
    }

    #[cfg(unix)]
    fn make_read_only(path: &Path) {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = std::fs::metadata(path).unwrap().permissions();
        permissions.set_mode(0o444);
        std::fs::set_permissions(path, permissions).unwrap();
    }

    #[test]
    fn load_creates_default_config_under_data_dir() {
        let dir = make_temp_dir("create-default");

        let cfg = load_from_dir(&dir, None, false).unwrap();
        let generated = std::fs::read_to_string(dir.join(DEFAULT_CONFIG_PATH)).unwrap();

        assert_eq!(cfg.database.url, DEFAULT_SQLITE_DATABASE_URL);
        assert_eq!(cfg.server.start_mode, NodeRuntimeMode::Primary);
        assert_eq!(cfg.server.temp_dir, DEFAULT_TEMP_DIR);
        assert_eq!(cfg.server.upload_temp_dir, DEFAULT_UPLOAD_TEMP_DIR);
        assert_eq!(
            cfg.server.follower.managed_ingress_local_root,
            "data/managed-ingress"
        );
        assert!(cfg.network_trust.trusted_proxies.is_empty());
        assert!(dir.join(DEFAULT_CONFIG_PATH).exists());
        assert!(generated.contains("[server]"));
        assert!(generated.contains(r#"start_mode = "primary""#));
        assert!(generated.contains(r#"url = "sqlite://asterdrive.db?mode=rwc""#));
        assert!(generated.contains(r#"temp_dir = ".tmp""#));
        assert!(generated.contains(r#"upload_temp_dir = ".uploads""#));
        assert!(generated.contains("[server.follower]"));
        assert!(generated.contains(r#"managed_ingress_local_root = "managed-ingress""#));
        assert!(generated.contains("[network_trust]"));
        assert!(generated.contains(r#"trusted_proxies = []"#));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn ensure_default_config_exists_can_seed_follower_mode() {
        let dir = make_temp_dir("create-follower-default");
        let config_path = dir.join(DEFAULT_CONFIG_PATH);
        let mut default = Config::default();
        default.server.start_mode = NodeRuntimeMode::Follower;

        ensure_default_config_exists(&config_path, &default).unwrap();

        let generated = std::fs::read_to_string(&config_path).unwrap();
        assert!(generated.contains(r#"start_mode = "follower""#));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn load_ignores_root_config_file_and_creates_data_config() {
        let dir = make_temp_dir("legacy-config");
        write(
            &dir.join("config.toml"),
            br#"[database]
url = "sqlite://custom.db?mode=rwc"
"#,
        );

        let cfg = load_from_dir(&dir, None, false).unwrap();

        assert_eq!(cfg.database.url, DEFAULT_SQLITE_DATABASE_URL);
        assert!(dir.join("config.toml").exists());
        assert!(dir.join(DEFAULT_CONFIG_PATH).exists());

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn load_migrates_legacy_rate_limit_trusted_proxies_to_network_trust() {
        let dir = make_temp_dir("legacy-trusted-proxies");
        write(
            &dir.join(DEFAULT_CONFIG_PATH),
            br#"[rate_limit]
enabled = true
trusted_proxies = ["10.0.0.0/8", "192.168.0.0/16"]

[rate_limit.auth]
seconds_per_request = 1
burst_size = 5
"#,
        );

        let cfg = load_from_dir(&dir, None, false).unwrap();
        let migrated = std::fs::read_to_string(dir.join(DEFAULT_CONFIG_PATH)).unwrap();

        assert_eq!(
            cfg.network_trust.trusted_proxies,
            vec!["10.0.0.0/8".to_string(), "192.168.0.0/16".to_string()]
        );
        assert!(!migrated.contains("rate_limit.trusted_proxies"));
        assert!(migrated.contains("[network_trust]"));
        assert!(migrated.contains(r#"trusted_proxies = ["10.0.0.0/8", "192.168.0.0/16"]"#));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[cfg(unix)]
    #[test]
    fn load_uses_migrated_legacy_trusted_proxies_in_memory_when_write_fails() {
        let dir = make_temp_dir("legacy-trusted-proxies-read-only");
        let config_path = dir.join(DEFAULT_CONFIG_PATH);
        write(
            &config_path,
            br#"[rate_limit]
enabled = true
trusted_proxies = ["10.0.0.0/8"]

[rate_limit.auth]
seconds_per_request = 1
burst_size = 5
"#,
        );
        make_read_only(&config_path);

        let cfg = load_from_dir(&dir, None, false).unwrap();
        let unchanged = std::fs::read_to_string(&config_path).unwrap();

        assert_eq!(
            cfg.network_trust.trusted_proxies,
            vec!["10.0.0.0/8".to_string()]
        );
        assert!(unchanged.contains("trusted_proxies = [\"10.0.0.0/8\"]"));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn load_ignores_root_sqlite_database_file_for_default_layout() {
        let dir = make_temp_dir("legacy-db");
        write(&dir.join("asterdrive.db"), b"legacy");

        let cfg = load_from_dir(&dir, None, false).unwrap();

        assert_eq!(cfg.database.url, DEFAULT_SQLITE_DATABASE_URL);
        assert!(dir.join("asterdrive.db").exists());
        assert!(!dir.join(DEFAULT_SQLITE_DATABASE_PATH).exists());

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn load_keeps_existing_data_prefixed_paths_without_double_data() {
        let dir = make_temp_dir("legacy-data-prefixed-values");
        write(
            &dir.join(DEFAULT_CONFIG_PATH),
            br#"[database]
url = "sqlite://data/asterdrive.db?mode=rwc"

[server]
temp_dir = "data/.tmp"
upload_temp_dir = "data/.uploads"

[server.follower]
managed_ingress_local_root = "data/managed-ingress"
"#,
        );

        let cfg = load_from_dir(&dir, None, false).unwrap();

        assert_eq!(cfg.database.url, DEFAULT_SQLITE_DATABASE_URL);
        assert_eq!(cfg.server.temp_dir, DEFAULT_TEMP_DIR);
        assert_eq!(cfg.server.upload_temp_dir, DEFAULT_UPLOAD_TEMP_DIR);
        assert_eq!(
            cfg.server.follower.managed_ingress_local_root,
            "data/managed-ingress"
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn load_rejects_deprecated_server_managed_ingress_root() {
        let dir = make_temp_dir("deprecated-managed-ingress-root");
        write(
            &dir.join(DEFAULT_CONFIG_PATH),
            br#"[server]
managed_ingress_local_root = "custom-managed-ingress"
"#,
        );

        let error = load_from_dir(&dir, None, false).unwrap_err();
        assert!(
            error
                .message()
                .contains("server.follower.managed_ingress_local_root")
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn load_resolves_relative_database_override_under_data_dir() {
        let dir = make_temp_dir("env-db-url-relative");
        write(&dir.join("asterdrive.db"), b"legacy");

        let cfg = load_from_dir(&dir, Some("sqlite://custom.db?mode=rwc"), false).unwrap();

        assert_eq!(cfg.database.url, "sqlite://data/custom.db?mode=rwc");
        assert!(dir.join(DEFAULT_CONFIG_PATH).exists());
        assert!(dir.join("asterdrive.db").exists());
        assert!(!dir.join(DEFAULT_SQLITE_DATABASE_PATH).exists());

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn load_keeps_data_prefixed_database_override_without_double_data() {
        let dir = make_temp_dir("env-db-url-legacy-root-relative");

        let cfg = load_from_dir(&dir, Some("sqlite://data/custom.db?mode=rwc"), false).unwrap();

        assert_eq!(cfg.database.url, "sqlite://data/custom.db?mode=rwc");

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn load_accepts_root_sqlite_database_for_relative_default_override() {
        let dir = make_temp_dir("env-db-url-relative-default");
        write(&dir.join("asterdrive.db"), b"legacy");

        let cfg = load_from_dir(&dir, Some("sqlite://asterdrive.db?mode=rwc"), false).unwrap();

        assert_eq!(cfg.database.url, DEFAULT_SQLITE_DATABASE_URL);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn load_accepts_root_sqlite_database_for_data_prefixed_default_override() {
        let dir = make_temp_dir("env-db-url-data-prefixed-default");
        write(&dir.join("asterdrive.db"), b"legacy");

        let cfg = load_from_dir(&dir, Some("sqlite://data/asterdrive.db?mode=rwc"), false).unwrap();

        assert_eq!(cfg.database.url, DEFAULT_SQLITE_DATABASE_URL);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn load_rejects_relative_paths_outside_base_dir() {
        let dir = make_temp_dir("path-outside-base-dir");
        write(
            &dir.join(DEFAULT_CONFIG_PATH),
            br#"[server]
temp_dir = "../../outside/.tmp"
"#,
        );

        let err = load_from_dir(&dir, None, false).unwrap_err();
        assert!(err.to_string().contains("outside data base_dir"));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn load_rejects_sqlite_url_outside_base_dir() {
        let dir = make_temp_dir("sqlite-url-outside-base-dir");
        write(
            &dir.join(DEFAULT_CONFIG_PATH),
            br#"[database]
url = "sqlite://../../outside/asterdrive.db?mode=rwc"
"#,
        );

        let err = load_from_dir(&dir, None, false).unwrap_err();
        assert!(err.to_string().contains("outside data base_dir"));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn load_rejects_zero_rate_limit_values() {
        let dir = make_temp_dir("invalid-rate-limit-zero");
        write(
            &dir.join(DEFAULT_CONFIG_PATH),
            br#"[rate_limit]
enabled = true

[rate_limit.auth]
seconds_per_request = 0
burst_size = 5
"#,
        );

        let err = load_from_dir(&dir, None, false).unwrap_err();
        assert!(err.to_string().contains("invalid value: integer `0`"));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn load_rejects_zero_rate_limit_burst_size() {
        let dir = make_temp_dir("invalid-rate-limit-burst-zero");
        write(
            &dir.join(DEFAULT_CONFIG_PATH),
            br#"[rate_limit]
enabled = true

[rate_limit.auth]
seconds_per_request = 1
burst_size = 0
"#,
        );

        let err = load_from_dir(&dir, None, false).unwrap_err();
        assert!(err.to_string().contains("invalid value: integer `0`"));

        let _ = std::fs::remove_dir_all(dir);
    }
}
