//! 配置子模块：`loader`。

use super::paths::{
    DEFAULT_CONFIG_PATH, resolve_config_relative_path, resolve_config_relative_sqlite_url,
};
use super::schema::Config;
use crate::errors::{AsterError, MapAsterErr, Result};
use config::{Config as RawConfig, Environment, File, FileFormat};
use std::path::{Path, PathBuf};
use toml_edit::{DocumentMut, Item, Table, value};

#[derive(Debug)]
pub struct LoadedConfig {
    pub config: Config,
    pub report: ConfigLoadReport,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigLoadReport {
    config_path: PathBuf,
    default_created: bool,
    stable_defaults_added: bool,
}

impl ConfigLoadReport {
    pub fn config_path(&self) -> &Path {
        &self.config_path
    }

    pub fn messages(&self) -> Vec<String> {
        let mut messages = Vec::with_capacity(4);
        if self.default_created {
            messages.push(format!(
                "[INFO] Default configuration written to: {}",
                self.config_path.display()
            ));
            messages.push("[INFO] Please review and modify it as needed.".to_string());
        }
        if self.stable_defaults_added {
            messages.push(format!(
                "[INFO] Added generated stable configuration keys to: {}",
                self.config_path.display()
            ));
        }
        messages.push(format!(
            "[INFO] Configuration loaded from: {}",
            self.config_path.display()
        ));
        messages
    }
}

pub fn load() -> Result<LoadedConfig> {
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
) -> Result<LoadedConfig> {
    let config_path = base_dir.join(DEFAULT_CONFIG_PATH);

    let default_created = ensure_default_config_exists(&config_path, &Config::default())?;
    let stable_defaults_config = ensure_stable_default_config_keys(&config_path, None)?;
    let stable_defaults_added = stable_defaults_config.is_some();

    let mut builder = RawConfig::builder();
    builder = match stable_defaults_config {
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

    let mut cfg = builder
        .build()
        .map_aster_err(AsterError::config_error)?
        .try_deserialize::<Config>()
        .map_aster_err(AsterError::config_error)?;

    resolve_loaded_paths(base_dir, &config_path, &mut cfg)?;

    Ok(LoadedConfig {
        config: cfg,
        report: ConfigLoadReport {
            config_path,
            default_created,
            stable_defaults_added,
        },
    })
}

fn ensure_default_config_exists(config_path: &Path, default: &Config) -> Result<bool> {
    if config_path.exists() {
        return Ok(false);
    }

    create_default_config(config_path, default)?;
    Ok(true)
}

fn create_default_config(config_path: &Path, default: &Config) -> Result<()> {
    let toml_str = toml::to_string_pretty(default).map_aster_err(AsterError::config_error)?;

    let content = format!(
        "# AsterDrive configuration file\n\
         # Generated on first startup; edit as needed.\n\
         # Relative paths are resolved against the directory containing this file (default: ./data).\n\
         # Docs: https://drive.astercosm.com/config/\n\n\
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
    let auth_defaults = crate::config::AuthConfig::default();
    for (key, secret) in [
        ("jwt_secret", auth_defaults.jwt_secret),
        ("share_cookie_secret", auth_defaults.share_cookie_secret),
        ("direct_link_secret", auth_defaults.direct_link_secret),
        ("mfa_secret_key", auth_defaults.mfa_secret_key),
        (
            "storage_credential_secret_key",
            auth_defaults.storage_credential_secret_key,
        ),
    ] {
        if !auth_table.contains_key(key) && std::env::var_os(auth_env_name(key)).is_none() {
            auth_table.insert(key, value(secret));
            changed = true;
        }
    }

    if !changed {
        return Ok(None);
    }

    let updated = doc.to_string();
    if let Err(error) = std::fs::write(config_path, &updated) {
        return Err(AsterError::config_error(format!(
            "failed to persist generated stable configuration keys to {}: {error}. Fix config file permissions before starting",
            config_path.display()
        )));
    }
    Ok(Some(updated))
}

fn auth_env_name(key: &str) -> String {
    format!("ASTER__AUTH__{}", key.to_ascii_uppercase())
}

fn resolve_loaded_paths(base_dir: &Path, config_path: &Path, cfg: &mut Config) -> Result<()> {
    let config_dir = config_path.parent().unwrap_or(base_dir);

    cfg.server.temp_dir = resolve_config_relative_path(base_dir, config_dir, &cfg.server.temp_dir)?;
    cfg.server.upload_temp_dir =
        resolve_config_relative_path(base_dir, config_dir, &cfg.server.upload_temp_dir)?;
    cfg.server.follower.remote_storage_target_local_root = resolve_config_relative_path(
        base_dir,
        config_dir,
        &cfg.server.follower.remote_storage_target_local_root,
    )?;
    cfg.database.url = resolve_config_relative_sqlite_url(base_dir, config_dir, &cfg.database.url)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{ensure_default_config_exists, load_from_dir};
    use crate::config::paths::{
        DEFAULT_CONFIG_PATH, DEFAULT_SQLITE_DATABASE_PATH, DEFAULT_SQLITE_DATABASE_URL,
        DEFAULT_TEMP_DIR, DEFAULT_UPLOAD_TEMP_DIR,
    };
    use crate::config::{Config, node_mode::NodeRuntimeMode};
    use std::path::{Path, PathBuf};
    use std::sync::{Mutex, OnceLock};

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

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    struct EnvVarGuard {
        name: &'static str,
        old: Option<std::ffi::OsString>,
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            unsafe {
                match &self.old {
                    Some(old) => std::env::set_var(self.name, old),
                    None => std::env::remove_var(self.name),
                }
            }
        }
    }

    fn with_env_var<T>(name: &'static str, value: &str, run: impl FnOnce() -> T) -> T {
        let _lock = env_lock()
            .lock()
            .expect("config loader env test lock should not be poisoned");
        let _env = EnvVarGuard {
            name,
            old: std::env::var_os(name),
        };
        unsafe {
            std::env::set_var(name, value);
        }
        run()
    }

    fn clear_auth_secret_env_vars<T>(run: impl FnOnce() -> T) -> T {
        let names = [
            "ASTER__AUTH__JWT_SECRET",
            "ASTER__AUTH__SHARE_COOKIE_SECRET",
            "ASTER__AUTH__DIRECT_LINK_SECRET",
            "ASTER__AUTH__MFA_SECRET_KEY",
            "ASTER__AUTH__STORAGE_CREDENTIAL_SECRET_KEY",
        ];
        let guards: Vec<_> = names
            .into_iter()
            .map(|name| {
                let guard = EnvVarGuard {
                    name,
                    old: std::env::var_os(name),
                };
                unsafe {
                    std::env::remove_var(name);
                }
                guard
            })
            .collect();
        let result = run();
        drop(guards);
        result
    }

    #[test]
    fn load_creates_default_config_under_data_dir() {
        let dir = make_temp_dir("create-default");

        let loaded = load_from_dir(&dir, None, false).unwrap();
        let messages = loaded.report.messages();
        let cfg = loaded.config;
        let generated = std::fs::read_to_string(dir.join(DEFAULT_CONFIG_PATH)).unwrap();

        assert_eq!(cfg.database.url, DEFAULT_SQLITE_DATABASE_URL);
        assert_eq!(cfg.server.start_mode, NodeRuntimeMode::Primary);
        assert_eq!(cfg.server.temp_dir, DEFAULT_TEMP_DIR);
        assert_eq!(cfg.server.upload_temp_dir, DEFAULT_UPLOAD_TEMP_DIR);
        assert_eq!(
            cfg.server.follower.remote_storage_target_local_root,
            "data/remote-storage-targets"
        );
        assert!(cfg.network_trust.trusted_proxies.is_empty());
        assert!(dir.join(DEFAULT_CONFIG_PATH).exists());
        assert!(generated.contains("[server]"));
        assert!(generated.contains(r#"start_mode = "primary""#));
        assert!(generated.contains(r#"url = "sqlite://asterdrive.db?mode=rwc""#));
        assert!(generated.contains(r#"temp_dir = ".tmp""#));
        assert!(generated.contains(r#"upload_temp_dir = ".uploads""#));
        assert!(generated.contains("[server.follower]"));
        assert!(
            generated.contains(r#"remote_storage_target_local_root = "remote-storage-targets""#)
        );
        assert!(generated.contains("[network_trust]"));
        assert!(generated.contains(r#"trusted_proxies = []"#));
        assert!(generated.contains("jwt_secret"));
        assert!(generated.contains("share_cookie_secret"));
        assert!(generated.contains("direct_link_secret"));
        assert!(generated.contains("mfa_secret_key"));
        assert!(generated.contains("storage_credential_secret_key"));
        assert!(
            messages
                .iter()
                .any(|message| { message.starts_with("[INFO] Default configuration written to:") })
        );
        assert!(
            messages
                .iter()
                .any(|message| { message.starts_with("[INFO] Configuration loaded from:") })
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn load_backfills_missing_auth_secrets_without_reusing_existing_jwt_secret() {
        let _lock = env_lock()
            .lock()
            .expect("config loader env test lock should not be poisoned");
        clear_auth_secret_env_vars(|| {
            let dir = make_temp_dir("backfill-auth-secrets");
            let legacy_jwt_secret = "legacy-jwt-secret";
            write(
                &dir.join(DEFAULT_CONFIG_PATH),
                format!(
                    r#"[auth]
jwt_secret = "{legacy_jwt_secret}"
"#
                )
                .as_bytes(),
            );

            let cfg = load_from_dir(&dir, None, false).unwrap().config;
            let updated = std::fs::read_to_string(dir.join(DEFAULT_CONFIG_PATH)).unwrap();

            assert_eq!(cfg.auth.jwt_secret, legacy_jwt_secret);
            assert!(!cfg.auth.share_cookie_secret.is_empty());
            assert!(!cfg.auth.direct_link_secret.is_empty());
            assert!(!cfg.auth.mfa_secret_key.is_empty());
            assert!(!cfg.auth.storage_credential_secret_key.is_empty());
            assert_ne!(cfg.auth.share_cookie_secret, legacy_jwt_secret);
            assert_ne!(cfg.auth.direct_link_secret, legacy_jwt_secret);
            assert_ne!(cfg.auth.mfa_secret_key, legacy_jwt_secret);
            assert_ne!(cfg.auth.storage_credential_secret_key, legacy_jwt_secret);
            assert!(updated.contains("share_cookie_secret"));
            assert!(updated.contains("direct_link_secret"));
            assert!(updated.contains("mfa_secret_key"));
            assert!(updated.contains("storage_credential_secret_key"));

            let _ = std::fs::remove_dir_all(dir);
        });
    }

    #[test]
    fn load_backfills_jwt_secret_when_auth_table_is_missing() {
        let _lock = env_lock()
            .lock()
            .expect("config loader env test lock should not be poisoned");
        clear_auth_secret_env_vars(|| {
            let dir = make_temp_dir("backfill-missing-auth-table");
            write(
                &dir.join(DEFAULT_CONFIG_PATH),
                br#"[database]
url = "sqlite://asterdrive.db?mode=rwc"
"#,
            );

            let cfg = load_from_dir(&dir, None, false).unwrap().config;
            let updated = std::fs::read_to_string(dir.join(DEFAULT_CONFIG_PATH)).unwrap();

            assert!(!cfg.auth.jwt_secret.is_empty());
            assert!(!cfg.auth.share_cookie_secret.is_empty());
            assert!(!cfg.auth.direct_link_secret.is_empty());
            assert!(!cfg.auth.mfa_secret_key.is_empty());
            assert!(!cfg.auth.storage_credential_secret_key.is_empty());
            assert!(updated.contains("[auth]"));
            assert!(updated.contains("jwt_secret"));
            assert!(updated.contains("share_cookie_secret"));
            assert!(updated.contains("direct_link_secret"));
            assert!(updated.contains("mfa_secret_key"));
            assert!(updated.contains("storage_credential_secret_key"));

            let _ = std::fs::remove_dir_all(dir);
        });
    }

    #[test]
    fn load_does_not_backfill_secret_supplied_by_environment() {
        with_env_var(
            "ASTER__AUTH__DIRECT_LINK_SECRET",
            "env-direct-link-secret",
            || {
                let dir = make_temp_dir("skip-env-auth-secret-backfill");
                write(
                    &dir.join(DEFAULT_CONFIG_PATH),
                    br#"[auth]
jwt_secret = "file-jwt-secret"
share_cookie_secret = "file-share-secret"
mfa_secret_key = "file-mfa-secret"
"#,
                );

                let cfg = load_from_dir(&dir, None, true).unwrap().config;
                let updated = std::fs::read_to_string(dir.join(DEFAULT_CONFIG_PATH)).unwrap();

                assert_eq!(cfg.auth.direct_link_secret, "env-direct-link-secret");
                assert!(!updated.contains("direct_link_secret"));

                let _ = std::fs::remove_dir_all(dir);
            },
        );
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

        let cfg = load_from_dir(&dir, None, false).unwrap().config;

        assert_eq!(cfg.database.url, DEFAULT_SQLITE_DATABASE_URL);
        assert!(dir.join("config.toml").exists());
        assert!(dir.join(DEFAULT_CONFIG_PATH).exists());

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn load_ignores_root_sqlite_database_file_for_default_layout() {
        let dir = make_temp_dir("legacy-db");
        write(&dir.join("asterdrive.db"), b"legacy");

        let cfg = load_from_dir(&dir, None, false).unwrap().config;

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
remote_storage_target_local_root = "data/remote-storage-targets"
"#,
        );

        let cfg = load_from_dir(&dir, None, false).unwrap().config;

        assert_eq!(cfg.database.url, DEFAULT_SQLITE_DATABASE_URL);
        assert_eq!(cfg.server.temp_dir, DEFAULT_TEMP_DIR);
        assert_eq!(cfg.server.upload_temp_dir, DEFAULT_UPLOAD_TEMP_DIR);
        assert_eq!(
            cfg.server.follower.remote_storage_target_local_root,
            "data/remote-storage-targets"
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn load_accepts_legacy_nested_managed_ingress_root_alias() {
        let dir = make_temp_dir("legacy-nested-managed-ingress-root");
        write(
            &dir.join(DEFAULT_CONFIG_PATH),
            br#"[server.follower]
managed_ingress_local_root = "data/remote-storage-targets"
"#,
        );

        let cfg = load_from_dir(&dir, None, false).unwrap().config;

        assert_eq!(
            cfg.server.follower.remote_storage_target_local_root,
            "data/remote-storage-targets"
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn load_resolves_relative_database_override_under_data_dir() {
        let dir = make_temp_dir("env-db-url-relative");
        write(&dir.join("asterdrive.db"), b"legacy");

        let cfg = load_from_dir(&dir, Some("sqlite://custom.db?mode=rwc"), false)
            .unwrap()
            .config;

        assert_eq!(cfg.database.url, "sqlite://data/custom.db?mode=rwc");
        assert!(dir.join(DEFAULT_CONFIG_PATH).exists());
        assert!(dir.join("asterdrive.db").exists());
        assert!(!dir.join(DEFAULT_SQLITE_DATABASE_PATH).exists());

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn load_keeps_data_prefixed_database_override_without_double_data() {
        let dir = make_temp_dir("env-db-url-legacy-root-relative");

        let cfg = load_from_dir(&dir, Some("sqlite://data/custom.db?mode=rwc"), false)
            .unwrap()
            .config;

        assert_eq!(cfg.database.url, "sqlite://data/custom.db?mode=rwc");

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn load_accepts_root_sqlite_database_for_relative_default_override() {
        let dir = make_temp_dir("env-db-url-relative-default");
        write(&dir.join("asterdrive.db"), b"legacy");

        let cfg = load_from_dir(&dir, Some("sqlite://asterdrive.db?mode=rwc"), false)
            .unwrap()
            .config;

        assert_eq!(cfg.database.url, DEFAULT_SQLITE_DATABASE_URL);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn load_accepts_root_sqlite_database_for_data_prefixed_default_override() {
        let dir = make_temp_dir("env-db-url-data-prefixed-default");
        write(&dir.join("asterdrive.db"), b"legacy");

        let cfg = load_from_dir(&dir, Some("sqlite://data/asterdrive.db?mode=rwc"), false)
            .unwrap()
            .config;

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
