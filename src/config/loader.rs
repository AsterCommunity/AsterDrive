//! 配置子模块：`loader`。

use super::paths::{
    DEFAULT_CONFIG_PATH, resolve_config_relative_database_url, resolve_config_relative_path,
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

fn map_static_config_error(error: config::ConfigError) -> AsterError {
    if let config::ConfigError::FileParse { uri, cause } = &error {
        let position = cause
            .downcast_ref::<toml::de::Error>()
            .and_then(toml::de::Error::span)
            .map(|span| format!("bytes {}..{}", span.start, span.end))
            .unwrap_or_else(|| "position unavailable".into());
        return AsterError::config_error(format!(
            "invalid static configuration TOML in {} at {position} (values redacted)",
            uri.as_deref().unwrap_or("<inline>"),
        ));
    }
    // Unknown-field diagnostics contain only a schema key. Keep a bounded,
    // conventional key name without echoing arbitrary parser text or values.
    let mut cause = &error;
    while let config::ConfigError::At { error, .. } = cause {
        cause = error;
    }
    if let config::ConfigError::Message(message) = cause
        && let Some((field, _)) = message
            .strip_prefix("unknown field `")
            .and_then(|message| message.split_once('`'))
    {
        let field = if !field.is_empty()
            && field.len() <= 128
            && field
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            field
        } else {
            "[REDACTED]"
        };
        return AsterError::config_error(format!(
            "unknown static configuration field `{field}` (values redacted)"
        ));
    }
    if error.to_string().contains("observability") {
        AsterError::config_error("invalid observability configuration (values redacted)")
    } else {
        AsterError::config_error(
            "invalid static configuration; check field types and required values (values redacted)",
        )
    }
}

pub fn load() -> Result<LoadedConfig> {
    let base_dir = std::env::current_dir()
        .map_aster_err_ctx("failed to resolve current dir", AsterError::config_error)?;
    let env_database_url = std::env::var("ASTER__DATABASE__URL").ok();
    load_from_path(
        &base_dir,
        &base_dir.join(DEFAULT_CONFIG_PATH),
        env_database_url.as_deref(),
        true,
    )
}

fn load_from_path(
    base_dir: &Path,
    config_path: &Path,
    env_database_url: Option<&str>,
    include_env: bool,
) -> Result<LoadedConfig> {
    let default_created = ensure_default_config_exists(config_path, &Config::default())?;
    let stable_defaults_config = ensure_stable_default_config_keys(config_path, None)?;
    let stable_defaults_added = stable_defaults_config.is_some();

    let mut builder = RawConfig::builder();
    builder = match stable_defaults_config {
        Some(config_content) => {
            builder.add_source(File::from_str(&config_content, FileFormat::Toml))
        }
        None => builder.add_source(File::from(config_path).required(false)),
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
            .map_err(map_static_config_error)?;
    }

    let mut cfg = builder
        .build()
        .map_err(map_static_config_error)?
        .try_deserialize::<Config>()
        .map_err(map_static_config_error)?;

    resolve_loaded_paths(base_dir, config_path, &mut cfg)?;
    cfg.auth.validate()?;
    cfg.observability.validate()?;

    Ok(LoadedConfig {
        config: cfg,
        report: ConfigLoadReport {
            config_path: config_path.to_path_buf(),
            default_created,
            stable_defaults_added,
        },
    })
}

fn load_read_only_from_path(
    base_dir: &Path,
    config_path: &Path,
    include_env: bool,
) -> Result<Config> {
    let mut builder = RawConfig::builder().add_source(File::from(config_path).required(false));
    if include_env {
        builder = builder.add_source(
            Environment::with_prefix("ASTER")
                .separator("__")
                .try_parsing(true),
        );
    }

    let mut config = builder
        .build()
        .map_err(map_static_config_error)?
        .try_deserialize::<Config>()
        .map_err(map_static_config_error)?;
    resolve_loaded_paths(base_dir, config_path, &mut config)?;
    config.auth.validate()?;
    config.observability.validate()?;
    Ok(config)
}

fn ensure_default_config_exists(config_path: &Path, default: &Config) -> Result<bool> {
    if config_path.exists() {
        return Ok(false);
    }

    create_default_config(config_path, default)?;
    Ok(true)
}

fn create_default_config(config_path: &Path, default: &Config) -> Result<()> {
    let toml_str = toml::to_string_pretty(default)
        .map_err(|_| AsterError::config_error("failed to serialize static configuration"))?;

    let content = format!(
        "# AsterDrive configuration file\n\
         # Generated on first startup; edit as needed.\n\
         # Relative paths are resolved against the directory containing this file (default: ./data).\n\
         # Docs: https://drive.docs.astercosm.com/config/\n\n\
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
        let position = error
            .span()
            .map(|span| format!("bytes {}..{}", span.start, span.end))
            .unwrap_or_else(|| "position unavailable".into());
        AsterError::config_error(format!(
            "failed to parse {} at {position}: invalid TOML (values redacted)",
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
        (
            "webdav_auth_cache_secret",
            auth_defaults.webdav_auth_cache_secret,
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
    cfg.logging.file = resolve_config_relative_path(base_dir, config_dir, &cfg.logging.file)?;
    cfg.server.follower.remote_storage_target_local_root = resolve_config_relative_path(
        base_dir,
        config_dir,
        &cfg.server.follower.remote_storage_target_local_root,
    )?;
    resolve_config_relative_database_url(base_dir, config_dir, &mut cfg.database.url)?;
    Ok(())
}

/// An explicit location resolves every runtime path independently of the caller's cwd.
#[derive(Debug, Clone)]
pub struct ConfigLocation {
    path: PathBuf,
    explicit: bool,
}

impl ConfigLocation {
    pub fn resolve(config_file: Option<&Path>, data_dir: Option<&Path>) -> Result<Self> {
        let cwd = std::env::current_dir()
            .map_aster_err_ctx("failed to resolve current dir", AsterError::config_error)?;
        let (path, explicit) = match (config_file, data_dir) {
            (Some(_), Some(_)) => {
                return Err(AsterError::validation_error(
                    "use either --config-file or --data-dir",
                ));
            }
            (Some(path), None) => (path.to_path_buf(), true),
            (None, Some(dir)) => (dir.join("config.toml"), true),
            (None, None) => (PathBuf::from(DEFAULT_CONFIG_PATH), false),
        };
        Ok(Self {
            path: if path.is_absolute() {
                path
            } else {
                cwd.join(path)
            },
            explicit,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn base_dir(&self) -> &Path {
        let parent = self.path.parent().unwrap_or(Path::new("/"));
        if parent.file_name().is_some_and(|name| name == "data") {
            parent.parent().unwrap_or(parent)
        } else {
            parent
        }
    }

    fn absolutize(&self, config: &mut Config) {
        for path in [
            &mut config.server.temp_dir,
            &mut config.server.upload_temp_dir,
            &mut config.server.follower.remote_storage_target_local_root,
            &mut config.logging.file,
        ] {
            if !path.is_empty() && Path::new(path).is_relative() {
                *path = self.base_dir().join(&*path).to_string_lossy().into_owned();
            }
        }
        if let aster_forge_db::DatabaseUrl::Url(url) = &mut config.database.url
            && let Some(rest) = url.strip_prefix("sqlite://")
        {
            let (path, query) = rest.split_once('?').map_or((rest, ""), |(p, q)| (p, q));
            if !path.is_empty() && path != ":memory:" && Path::new(path).is_relative() {
                *url = format!(
                    "sqlite://{}{}{}",
                    self.base_dir().join(path).display(),
                    if query.is_empty() { "" } else { "?" },
                    query
                );
            }
        }
    }

    pub fn load_read_only(&self, require_existing: bool) -> Result<Config> {
        if (self.explicit || require_existing) && !self.path.is_file() {
            return Err(AsterError::config_error(format!(
                "configuration file '{}' does not exist; specify --config-file/--data-dir or --database-url",
                self.path.display()
            )));
        }
        let mut config = load_read_only_from_path(self.base_dir(), &self.path, true)?;
        self.absolutize(&mut config);
        Ok(config)
    }

    pub fn ensure_default(&self, default: &Config) -> Result<PathBuf> {
        ensure_default_config_exists(&self.path, default)?;
        Ok(self.path.clone())
    }

    pub fn initialize(&self) -> Result<LoadedConfig> {
        let mut loaded = load_from_path(self.base_dir(), &self.path, None, true)?;
        self.absolutize(&mut loaded.config);
        Ok(loaded)
    }
}

#[cfg(test)]
mod tests {
    use super::{ensure_default_config_exists, load_from_path, load_read_only_from_path};
    use crate::config::paths::{
        DEFAULT_CONFIG_PATH, DEFAULT_SQLITE_DATABASE_PATH, DEFAULT_SQLITE_DATABASE_URL,
        DEFAULT_TEMP_DIR, DEFAULT_UPLOAD_TEMP_DIR,
    };
    use crate::config::{Config, DeploymentProfile, node_mode::NodeRuntimeMode};
    use std::path::{Path, PathBuf};
    use std::process::Command;

    const ISOLATED_ENV_CHILD: &str = "ASTER_CONFIG_LOADER_ISOLATED_ENV_CHILD";
    const AUTH_SECRET_ENV_NAMES: [&str; 6] = [
        "ASTER__AUTH__JWT_SECRET",
        "ASTER__AUTH__SHARE_COOKIE_SECRET",
        "ASTER__AUTH__DIRECT_LINK_SECRET",
        "ASTER__AUTH__MFA_SECRET_KEY",
        "ASTER__AUTH__STORAGE_CREDENTIAL_SECRET_KEY",
        "ASTER__AUTH__WEBDAV_AUTH_CACHE_SECRET",
    ];

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

    fn is_isolated_environment_child() -> bool {
        std::env::var_os(ISOLATED_ENV_CHILD).is_some()
    }

    fn run_in_isolated_environment(test_name: &str, removed: &[&str], values: &[(&str, &str)]) {
        let mut command = Command::new(
            std::env::current_exe().expect("current config loader test executable should resolve"),
        );
        command
            .args(["--exact", test_name, "--nocapture"])
            .env(ISOLATED_ENV_CHILD, "1");
        for name in removed {
            command.env_remove(name);
        }
        for (name, value) in values {
            command.env(name, value);
        }
        let output = command
            .output()
            .expect("isolated config loader test process should start");
        assert!(
            output.status.success(),
            "isolated config loader test failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn load_creates_default_config_under_data_dir() {
        let dir = make_temp_dir("create-default");

        let loaded = load_from_path(&dir, &dir.join(DEFAULT_CONFIG_PATH), None, false).unwrap();
        let messages = loaded.report.messages();
        let cfg = loaded.config;
        let generated = std::fs::read_to_string(dir.join(DEFAULT_CONFIG_PATH)).unwrap();

        assert_eq!(cfg.database.url.as_url(), Some(DEFAULT_SQLITE_DATABASE_URL));
        assert_eq!(cfg.deployment.profile, DeploymentProfile::Single);
        assert_eq!(cfg.server.start_mode, NodeRuntimeMode::Primary);
        assert_eq!(cfg.server.temp_dir, DEFAULT_TEMP_DIR);
        assert_eq!(cfg.server.upload_temp_dir, DEFAULT_UPLOAD_TEMP_DIR);
        assert_eq!(cfg.server.upload_temp_min_free_bytes, 256 * 1024 * 1024);
        assert_eq!(
            cfg.server.follower.remote_storage_target_local_root,
            "data/remote-storage-targets"
        );
        assert!(cfg.network_trust.trusted_proxies.is_empty());
        assert!(dir.join(DEFAULT_CONFIG_PATH).exists());
        assert!(generated.contains("[server]"));
        assert!(generated.contains("[deployment]"));
        assert!(generated.contains(r#"profile = "single""#));
        assert!(generated.contains(r#"start_mode = "primary""#));
        assert!(generated.contains(r#"url = "sqlite://asterdrive.db?mode=rwc""#));
        assert!(generated.contains(r#"temp_dir = ".tmp""#));
        assert!(generated.contains(r#"upload_temp_dir = ".uploads""#));
        assert!(generated.contains("upload_temp_min_free_bytes = 268435456"));
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
        assert!(generated.contains("webdav_auth_cache_secret"));
        assert!(generated.contains("password_hash_max_concurrency = 2"));
        assert_eq!(cfg.auth.password_hash_max_concurrency, 2);
        assert!(cfg.auth.bootstrap_insecure_cookies);
        assert!(generated.contains("bootstrap_insecure_cookies = true"));
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
    fn read_only_load_uses_defaults_without_creating_config() {
        let dir = make_temp_dir("read-only-default");

        let config = load_read_only_from_path(&dir, &dir.join(DEFAULT_CONFIG_PATH), false).unwrap();

        assert_eq!(config.deployment.profile, DeploymentProfile::Single);
        assert!(!dir.join(DEFAULT_CONFIG_PATH).exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn jwt_secret_file_validation_uses_bytes_and_preserves_explicit_values() {
        for secret in [
            String::new(),
            "short-private-key".to_string(),
            "x".repeat(31),
            "x".repeat(32),
            "x".repeat(33),
            "猫".repeat(10),
            format!("{}ab", "猫".repeat(10)),
            "猫".repeat(11),
            "0123456789abcdef0123456789abcdef".to_string(),
        ] {
            let dir = make_temp_dir("jwt-secret-file-validation");
            let path = dir.join(DEFAULT_CONFIG_PATH);
            let mut config = Config::default();
            config.auth.jwt_secret = secret.clone();
            let original = toml::to_string_pretty(&config).unwrap();
            write(&path, original.as_bytes());

            for result in [
                load_from_path(&dir, &dir.join(DEFAULT_CONFIG_PATH), None, false)
                    .map(|loaded| loaded.config),
                load_read_only_from_path(&dir, &dir.join(DEFAULT_CONFIG_PATH), false),
            ] {
                if secret.len() < 32 {
                    let error = result.expect_err("short JWT secret must fail").to_string();
                    assert!(error.contains("auth.jwt_secret"));
                    assert!(error.contains("at least 32 bytes for HS256"));
                    assert!(error.contains(&format!("got {} bytes", secret.len())));
                    if !secret.is_empty() {
                        assert!(!error.contains(&secret));
                    }
                } else {
                    assert_eq!(result.unwrap().auth.jwt_secret, secret);
                }
            }
            assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
            std::fs::remove_dir_all(dir).unwrap();
        }
    }

    #[test]
    fn jwt_secret_environment_is_validated_after_overriding_file() {
        if !is_isolated_environment_child() {
            for secret in ["", "short-private-key", "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"] {
                run_in_isolated_environment(
                    "config::loader::tests::jwt_secret_environment_is_validated_after_overriding_file",
                    &AUTH_SECRET_ENV_NAMES,
                    &[("ASTER__AUTH__JWT_SECRET", secret)],
                );
            }
            return;
        }
        let dir = make_temp_dir("jwt-secret-environment-validation");
        let path = dir.join(DEFAULT_CONFIG_PATH);
        let original = toml::to_string_pretty(&Config::default()).unwrap();
        write(&path, original.as_bytes());
        let secret = std::env::var("ASTER__AUTH__JWT_SECRET").unwrap();
        for result in [
            load_from_path(&dir, &dir.join(DEFAULT_CONFIG_PATH), None, true)
                .map(|loaded| loaded.config),
            load_read_only_from_path(&dir, &dir.join(DEFAULT_CONFIG_PATH), true),
        ] {
            let error = result
                .expect_err("short environment secret must fail")
                .to_string();
            assert!(error.contains("auth.jwt_secret"));
            assert!(error.contains(&format!("got {} bytes", secret.len())));
            if !secret.is_empty() {
                assert!(!error.contains(&secret));
            }
        }
        assert_eq!(std::fs::read_to_string(path).unwrap(), original);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn valid_jwt_secret_environment_override_accepts_short_file_value() {
        const SECRET: &str = "0123456789abcdef0123456789abcdef";
        if !is_isolated_environment_child() {
            run_in_isolated_environment(
                "config::loader::tests::valid_jwt_secret_environment_override_accepts_short_file_value",
                &AUTH_SECRET_ENV_NAMES,
                &[("ASTER__AUTH__JWT_SECRET", SECRET)],
            );
            return;
        }
        let dir = make_temp_dir("valid-jwt-secret-environment-override");
        let path = dir.join(DEFAULT_CONFIG_PATH);
        let mut config = Config::default();
        config.auth.jwt_secret = "short-file-secret".into();
        let original = toml::to_string_pretty(&config).unwrap();
        write(&path, original.as_bytes());
        assert_eq!(
            load_from_path(&dir, &dir.join(DEFAULT_CONFIG_PATH), None, true)
                .unwrap()
                .config
                .auth
                .jwt_secret,
            SECRET
        );
        assert_eq!(
            load_read_only_from_path(&dir, &dir.join(DEFAULT_CONFIG_PATH), true)
                .unwrap()
                .auth
                .jwt_secret,
            SECRET
        );
        assert_eq!(std::fs::read_to_string(path).unwrap(), original);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn load_backfills_missing_auth_secrets_without_reusing_existing_jwt_secret() {
        if !is_isolated_environment_child() {
            run_in_isolated_environment(
                "config::loader::tests::load_backfills_missing_auth_secrets_without_reusing_existing_jwt_secret",
                &AUTH_SECRET_ENV_NAMES,
                &[],
            );
            return;
        }

        let dir = make_temp_dir("backfill-auth-secrets");
        let legacy_jwt_secret = "legacy-jwt-secret-at-least-32-bytes";
        write(
            &dir.join(DEFAULT_CONFIG_PATH),
            format!(
                r#"[auth]
jwt_secret = "{legacy_jwt_secret}"
"#
            )
            .as_bytes(),
        );

        let cfg = load_from_path(&dir, &dir.join(DEFAULT_CONFIG_PATH), None, false)
            .unwrap()
            .config;
        let updated = std::fs::read_to_string(dir.join(DEFAULT_CONFIG_PATH)).unwrap();

        assert_eq!(cfg.auth.jwt_secret, legacy_jwt_secret);
        assert!(!cfg.auth.share_cookie_secret.is_empty());
        assert!(!cfg.auth.direct_link_secret.is_empty());
        assert!(!cfg.auth.mfa_secret_key.is_empty());
        assert!(!cfg.auth.storage_credential_secret_key.is_empty());
        assert!(!cfg.auth.webdav_auth_cache_secret.is_empty());
        assert_ne!(cfg.auth.share_cookie_secret, legacy_jwt_secret);
        assert_ne!(cfg.auth.direct_link_secret, legacy_jwt_secret);
        assert_ne!(cfg.auth.mfa_secret_key, legacy_jwt_secret);
        assert_ne!(cfg.auth.storage_credential_secret_key, legacy_jwt_secret);
        assert_ne!(cfg.auth.webdav_auth_cache_secret, legacy_jwt_secret);
        assert!(updated.contains("share_cookie_secret"));
        assert!(updated.contains("direct_link_secret"));
        assert!(updated.contains("mfa_secret_key"));
        assert!(updated.contains("storage_credential_secret_key"));
        assert!(updated.contains("webdav_auth_cache_secret"));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn load_backfills_jwt_secret_when_auth_table_is_missing() {
        if !is_isolated_environment_child() {
            run_in_isolated_environment(
                "config::loader::tests::load_backfills_jwt_secret_when_auth_table_is_missing",
                &AUTH_SECRET_ENV_NAMES,
                &[],
            );
            return;
        }

        let dir = make_temp_dir("backfill-missing-auth-table");
        write(
            &dir.join(DEFAULT_CONFIG_PATH),
            br#"[database]
url = "sqlite://asterdrive.db?mode=rwc"
"#,
        );

        let cfg = load_from_path(&dir, &dir.join(DEFAULT_CONFIG_PATH), None, false)
            .unwrap()
            .config;
        let updated = std::fs::read_to_string(dir.join(DEFAULT_CONFIG_PATH)).unwrap();

        assert!(!cfg.auth.jwt_secret.is_empty());
        assert!(!cfg.auth.share_cookie_secret.is_empty());
        assert!(!cfg.auth.direct_link_secret.is_empty());
        assert!(!cfg.auth.mfa_secret_key.is_empty());
        assert!(!cfg.auth.storage_credential_secret_key.is_empty());
        assert!(!cfg.auth.webdav_auth_cache_secret.is_empty());
        assert!(updated.contains("[auth]"));
        assert!(updated.contains("jwt_secret"));
        assert!(updated.contains("share_cookie_secret"));
        assert!(updated.contains("direct_link_secret"));
        assert!(updated.contains("mfa_secret_key"));
        assert!(updated.contains("storage_credential_secret_key"));
        assert!(updated.contains("webdav_auth_cache_secret"));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn load_does_not_backfill_secret_supplied_by_environment() {
        if !is_isolated_environment_child() {
            run_in_isolated_environment(
                "config::loader::tests::load_does_not_backfill_secret_supplied_by_environment",
                &AUTH_SECRET_ENV_NAMES,
                &[("ASTER__AUTH__DIRECT_LINK_SECRET", "env-direct-link-secret")],
            );
            return;
        }

        let dir = make_temp_dir("skip-env-auth-secret-backfill");
        write(
            &dir.join(DEFAULT_CONFIG_PATH),
            br#"[auth]
jwt_secret = "file-jwt-secret-at-least-32-bytes"
share_cookie_secret = "file-share-secret"
mfa_secret_key = "file-mfa-secret"
"#,
        );

        let cfg = load_from_path(&dir, &dir.join(DEFAULT_CONFIG_PATH), None, true)
            .unwrap()
            .config;
        let updated = std::fs::read_to_string(dir.join(DEFAULT_CONFIG_PATH)).unwrap();

        assert_eq!(cfg.auth.direct_link_secret, "env-direct-link-secret");
        assert!(!updated.contains("direct_link_secret"));

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

        let cfg = load_from_path(&dir, &dir.join(DEFAULT_CONFIG_PATH), None, false)
            .unwrap()
            .config;

        assert_eq!(cfg.database.url.as_url(), Some(DEFAULT_SQLITE_DATABASE_URL));
        assert!(dir.join("config.toml").exists());
        assert!(dir.join(DEFAULT_CONFIG_PATH).exists());

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn load_ignores_root_sqlite_database_file_for_default_layout() {
        let dir = make_temp_dir("legacy-db");
        write(&dir.join("asterdrive.db"), b"legacy");

        let cfg = load_from_path(&dir, &dir.join(DEFAULT_CONFIG_PATH), None, false)
            .unwrap()
            .config;

        assert_eq!(cfg.database.url.as_url(), Some(DEFAULT_SQLITE_DATABASE_URL));
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

        let cfg = load_from_path(&dir, &dir.join(DEFAULT_CONFIG_PATH), None, false)
            .unwrap()
            .config;

        assert_eq!(cfg.database.url.as_url(), Some(DEFAULT_SQLITE_DATABASE_URL));
        assert_eq!(cfg.server.temp_dir, DEFAULT_TEMP_DIR);
        assert_eq!(cfg.server.upload_temp_dir, DEFAULT_UPLOAD_TEMP_DIR);
        assert_eq!(
            cfg.server.follower.remote_storage_target_local_root,
            "data/remote-storage-targets"
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn load_accepts_structured_infrastructure_credentials_without_exposing_secrets() {
        let dir = make_temp_dir("structured-infrastructure-credentials");
        let raw_username = "drive user";
        let raw_password = "raw#[]{}^+=*@:/?%secret";
        write(
            &dir.join(DEFAULT_CONFIG_PATH),
            format!(
                r#"[database]
url = {{ base_url = "postgres://database.internal:5432/asterdrive", username = "{raw_username}", password = "{raw_password}" }}

[cache]
backend = "redis"
endpoint = {{ base_url = "redis://cache.internal:6379/0", username = "{raw_username}", password = "{raw_password}" }}

[config_sync]
backend = "redis"
endpoint = {{ base_url = "redis://events.internal:6379/0", username = "{raw_username}", password = "{raw_password}" }}
topic = "aster_drive.config_reload"
"#
            )
            .as_bytes(),
        );

        let cfg = load_from_path(&dir, &dir.join(DEFAULT_CONFIG_PATH), None, false)
            .unwrap()
            .config;
        assert_eq!(
            cfg.database.url,
            aster_forge_db::DatabaseUrl::credentials(
                "postgres://database.internal:5432/asterdrive",
                Some(raw_username.to_string()),
                Some(raw_password.to_string()),
            )
        );
        assert_eq!(
            cfg.cache.endpoint,
            aster_forge_cache::CacheEndpoint::credentials(
                "redis://cache.internal:6379/0",
                Some(raw_username.to_string()),
                Some(raw_password.to_string()),
            )
        );
        assert_eq!(
            cfg.config_sync.endpoint,
            aster_forge_config::ConfigSyncEndpoint::credentials(
                "redis://events.internal:6379/0",
                Some(raw_username.to_string()),
                Some(raw_password.to_string()),
            )
        );

        let debug = format!("{cfg:?}");
        assert!(!debug.contains(raw_username));
        assert!(!debug.contains(raw_password));
        let serialized = toml::to_string(&cfg).unwrap();
        assert!(!serialized.contains(raw_username));
        assert!(!serialized.contains(raw_password));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn load_keeps_legacy_string_infrastructure_endpoints() {
        let dir = make_temp_dir("legacy-string-infrastructure-endpoints");
        write(
            &dir.join(DEFAULT_CONFIG_PATH),
            br#"[database]
url = "postgres://encoded-user:encoded-password@database.internal:5432/asterdrive"

[cache]
backend = "redis"
endpoint = "redis://encoded-user:encoded-password@cache.internal:6379/0"

[config_sync]
backend = "redis"
endpoint = "redis://encoded-user:encoded-password@events.internal:6379/0"
topic = "aster_drive.config_reload"
"#,
        );

        let cfg = load_from_path(&dir, &dir.join(DEFAULT_CONFIG_PATH), None, false)
            .unwrap()
            .config;
        assert_eq!(
            cfg.database.url.as_url(),
            Some("postgres://encoded-user:encoded-password@database.internal:5432/asterdrive")
        );
        assert_eq!(
            cfg.cache.endpoint,
            aster_forge_cache::CacheEndpoint::url(
                "redis://encoded-user:encoded-password@cache.internal:6379/0"
            )
        );
        assert_eq!(
            cfg.config_sync.endpoint,
            aster_forge_config::ConfigSyncEndpoint::url(
                "redis://encoded-user:encoded-password@events.internal:6379/0"
            )
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn load_accepts_structured_infrastructure_credentials_from_environment() {
        let raw_username = "env user";
        let raw_password = "env#[]{}^+=*@:/?%secret";
        let values = [
            (
                "ASTER__DATABASE__URL__BASE_URL",
                "postgres://database.internal:5432/asterdrive",
            ),
            ("ASTER__DATABASE__URL__USERNAME", raw_username),
            ("ASTER__DATABASE__URL__PASSWORD", raw_password),
            ("ASTER__CACHE__BACKEND", "redis"),
            (
                "ASTER__CACHE__ENDPOINT__BASE_URL",
                "redis://cache.internal:6379/0",
            ),
            ("ASTER__CACHE__ENDPOINT__USERNAME", raw_username),
            ("ASTER__CACHE__ENDPOINT__PASSWORD", raw_password),
            ("ASTER__CONFIG_SYNC__BACKEND", "redis"),
            (
                "ASTER__CONFIG_SYNC__ENDPOINT__BASE_URL",
                "redis://events.internal:6379/0",
            ),
            ("ASTER__CONFIG_SYNC__ENDPOINT__USERNAME", raw_username),
            ("ASTER__CONFIG_SYNC__ENDPOINT__PASSWORD", raw_password),
        ];
        if !is_isolated_environment_child() {
            let removed: Vec<_> = values.iter().map(|(name, _)| *name).collect();
            run_in_isolated_environment(
                "config::loader::tests::load_accepts_structured_infrastructure_credentials_from_environment",
                &removed,
                &values,
            );
            return;
        }

        let dir = make_temp_dir("structured-infrastructure-env");
        let cfg = load_from_path(&dir, &dir.join(DEFAULT_CONFIG_PATH), None, true)
            .unwrap()
            .config;
        assert_eq!(
            cfg.database.url,
            aster_forge_db::DatabaseUrl::credentials(
                "postgres://database.internal:5432/asterdrive",
                Some(raw_username.to_string()),
                Some(raw_password.to_string()),
            )
        );
        assert_eq!(
            cfg.cache.endpoint,
            aster_forge_cache::CacheEndpoint::credentials(
                "redis://cache.internal:6379/0",
                Some(raw_username.to_string()),
                Some(raw_password.to_string()),
            )
        );
        assert_eq!(
            cfg.config_sync.endpoint,
            aster_forge_config::ConfigSyncEndpoint::credentials(
                "redis://events.internal:6379/0",
                Some(raw_username.to_string()),
                Some(raw_password.to_string()),
            )
        );
        let debug = format!("{cfg:?}");
        assert!(!debug.contains(raw_username));
        assert!(!debug.contains(raw_password));

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

        let cfg = load_from_path(&dir, &dir.join(DEFAULT_CONFIG_PATH), None, false)
            .unwrap()
            .config;

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

        let cfg = load_from_path(
            &dir,
            &dir.join(DEFAULT_CONFIG_PATH),
            Some("sqlite://custom.db?mode=rwc"),
            false,
        )
        .unwrap()
        .config;

        assert_eq!(
            cfg.database.url.as_url(),
            Some("sqlite://data/custom.db?mode=rwc")
        );
        assert!(dir.join(DEFAULT_CONFIG_PATH).exists());
        assert!(dir.join("asterdrive.db").exists());
        assert!(!dir.join(DEFAULT_SQLITE_DATABASE_PATH).exists());

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn load_keeps_data_prefixed_database_override_without_double_data() {
        let dir = make_temp_dir("env-db-url-legacy-root-relative");

        let cfg = load_from_path(
            &dir,
            &dir.join(DEFAULT_CONFIG_PATH),
            Some("sqlite://data/custom.db?mode=rwc"),
            false,
        )
        .unwrap()
        .config;

        assert_eq!(
            cfg.database.url.as_url(),
            Some("sqlite://data/custom.db?mode=rwc")
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn load_accepts_root_sqlite_database_for_relative_default_override() {
        let dir = make_temp_dir("env-db-url-relative-default");
        write(&dir.join("asterdrive.db"), b"legacy");

        let cfg = load_from_path(
            &dir,
            &dir.join(DEFAULT_CONFIG_PATH),
            Some("sqlite://asterdrive.db?mode=rwc"),
            false,
        )
        .unwrap()
        .config;

        assert_eq!(cfg.database.url.as_url(), Some(DEFAULT_SQLITE_DATABASE_URL));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn load_accepts_root_sqlite_database_for_data_prefixed_default_override() {
        let dir = make_temp_dir("env-db-url-data-prefixed-default");
        write(&dir.join("asterdrive.db"), b"legacy");

        let cfg = load_from_path(
            &dir,
            &dir.join(DEFAULT_CONFIG_PATH),
            Some("sqlite://data/asterdrive.db?mode=rwc"),
            false,
        )
        .unwrap()
        .config;

        assert_eq!(cfg.database.url.as_url(), Some(DEFAULT_SQLITE_DATABASE_URL));

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

        let err = load_from_path(&dir, &dir.join(DEFAULT_CONFIG_PATH), None, false).unwrap_err();
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

        let err = load_from_path(&dir, &dir.join(DEFAULT_CONFIG_PATH), None, false).unwrap_err();
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

        let err = load_from_path(&dir, &dir.join(DEFAULT_CONFIG_PATH), None, false).unwrap_err();
        assert!(err.to_string().contains("invalid static configuration"));

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

        let err = load_from_path(&dir, &dir.join(DEFAULT_CONFIG_PATH), None, false).unwrap_err();
        assert!(err.to_string().contains("invalid static configuration"));

        let _ = std::fs::remove_dir_all(dir);
    }
    #[cfg(feature = "metrics")]
    #[test]
    fn metrics_credentials_survive_default_backfill_and_reload() {
        let dir = aster_forge_utils::raii::TempDirGuard::new(
            make_temp_dir("observability"),
            "loader observability test",
        );
        let hash = aster_forge_crypto::hash_password("scrape:secret").unwrap();
        let content = format!(
            "[observability.prometheus]\nenabled = true\n[observability.prometheus.basic_auth]\nenabled = true\nusername = 'scraper'\npassword_hash = '{hash}'\n"
        );
        write(&dir.path().join(DEFAULT_CONFIG_PATH), content.as_bytes());
        let first = load_from_path(
            dir.path(),
            &dir.path().join(DEFAULT_CONFIG_PATH),
            None,
            false,
        )
        .unwrap();
        assert!(first.report.stable_defaults_added);
        assert_eq!(
            first
                .config
                .observability
                .prometheus
                .basic_auth
                .password_hash,
            hash
        );
        let persisted = std::fs::read_to_string(dir.path().join(DEFAULT_CONFIG_PATH)).unwrap();
        assert!(persisted.contains(&hash));
        let second = load_from_path(
            dir.path(),
            &dir.path().join(DEFAULT_CONFIG_PATH),
            None,
            false,
        )
        .unwrap();
        assert!(!second.report.stable_defaults_added);
        assert_eq!(
            second.config.observability.prometheus.basic_auth.username,
            "scraper"
        );
        assert_eq!(
            second
                .config
                .observability
                .prometheus
                .basic_auth
                .password_hash,
            hash
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join(DEFAULT_CONFIG_PATH)).unwrap(),
            persisted
        );
    }

    #[test]
    fn malformed_observability_values_are_redacted_in_both_load_paths() {
        for content in [
            "[observability.prometheus.basic_auth]\nenabled = 'credential-marker'\n",
            "[observability.otlp_metrics]\nheaders = 'credential-marker'\n",
            "[observability.prometheus.basic_auth]\npassword_hash = \"credential-marker\n",
            "[observability.prometheus.basic_auth]\npassword_has = 'credential-marker'\n",
        ] {
            let dir = aster_forge_utils::raii::TempDirGuard::new(
                make_temp_dir("observability"),
                "loader observability test",
            );
            write(&dir.path().join(DEFAULT_CONFIG_PATH), content.as_bytes());
            let error = load_from_path(
                dir.path(),
                &dir.path().join(DEFAULT_CONFIG_PATH),
                None,
                false,
            )
            .unwrap_err()
            .to_string();
            assert!(!error.contains("credential-marker"), "{error}");
            let error =
                load_read_only_from_path(dir.path(), &dir.path().join(DEFAULT_CONFIG_PATH), false)
                    .unwrap_err()
                    .to_string();
            assert!(!error.contains("credential-marker"), "{error}");
        }
    }
    #[test]
    fn strict_unknown_fields_keep_safe_keys_and_parse_errors_keep_positions_without_values() {
        for (content, field) in [
            ("legacy_unused = 'credential-marker'\n", "legacy_unused"),
            (
                "[observability.prometheus.basic_auth]\npassword_has = 'credential-marker'\n",
                "password_has",
            ),
            (
                "[metrics.basic_auth]\nenabled = true\npassword_hash = 'credential-marker'\n",
                "metrics",
            ),
        ] {
            let dir = aster_forge_utils::raii::TempDirGuard::new(
                make_temp_dir("strict-fields"),
                "loader strict fields test",
            );
            write(&dir.path().join(DEFAULT_CONFIG_PATH), content.as_bytes());
            for error in [
                load_from_path(
                    dir.path(),
                    &dir.path().join(DEFAULT_CONFIG_PATH),
                    None,
                    false,
                )
                .unwrap_err()
                .to_string(),
                load_read_only_from_path(dir.path(), &dir.path().join(DEFAULT_CONFIG_PATH), false)
                    .unwrap_err()
                    .to_string(),
            ] {
                assert!(error.contains(field), "{error}");
                assert!(!error.contains("credential-marker"), "{error}");
            }
        }
        let dir = aster_forge_utils::raii::TempDirGuard::new(
            make_temp_dir("parse-position"),
            "loader parse position test",
        );
        write(
            &dir.path().join(DEFAULT_CONFIG_PATH),
            b"[observability]\nenvironment = \"credential-marker\n",
        );
        for error in [
            load_from_path(
                dir.path(),
                &dir.path().join(DEFAULT_CONFIG_PATH),
                None,
                false,
            )
            .unwrap_err()
            .to_string(),
            load_read_only_from_path(dir.path(), &dir.path().join(DEFAULT_CONFIG_PATH), false)
                .unwrap_err()
                .to_string(),
        ] {
            assert!(error.contains("bytes "), "{error}");
            assert!(!error.contains("credential-marker"), "{error}");
        }
        let error = super::map_static_config_error(config::ConfigError::FileParse {
            uri: None,
            cause: Box::new(std::io::Error::other("credential-marker")),
        })
        .to_string();
        assert!(error.contains("<inline>") && error.contains("position unavailable"));
        assert!(!error.contains("credential-marker"));
    }
}
