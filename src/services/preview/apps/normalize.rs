//! 预览应用服务子模块：`normalize`。

use std::collections::{BTreeMap, BTreeSet, HashSet};

use crate::errors::{AsterError, Result};
use crate::runtime::SharedRuntimeState;

use super::{
    DEFAULT_TABLE_PREVIEW_DELIMITER, PREVIEW_APPS_CONFIG_KEY, PREVIEW_APPS_VERSION,
    PreviewAppProvider, PublicPreviewAppDefinition, PublicPreviewAppsConfig,
    REQUIRED_BUILTIN_PREVIEW_APP_KEYS, default_public_preview_apps,
    is_required_builtin_preview_app_key, is_table_preview_app_key,
};

pub fn normalize_public_preview_apps_config_value(value: &str) -> Result<String> {
    let mut config: PublicPreviewAppsConfig = serde_json::from_str(value).map_err(|error| {
        AsterError::validation_error(format!("preview apps config must be valid JSON: {error}"))
    })?;
    validate_preview_apps_config(&mut config)?;
    serde_json::to_string_pretty(&config).map_err(|error| {
        AsterError::internal_error(format!("failed to serialize preview apps config: {error}"))
    })
}

pub fn public_preview_apps_config_has_missing_required_builtins(value: &str) -> Result<bool> {
    let config: PublicPreviewAppsConfig = serde_json::from_str(value).map_err(|error| {
        AsterError::validation_error(format!("preview apps config must be valid JSON: {error}"))
    })?;
    let keys = config
        .apps
        .iter()
        .map(|app| app.key.trim())
        .collect::<HashSet<_>>();

    Ok(REQUIRED_BUILTIN_PREVIEW_APP_KEYS
        .iter()
        .any(|key| !keys.contains(*key)))
}

pub fn get_public_preview_apps(state: &impl SharedRuntimeState) -> PublicPreviewAppsConfig {
    let Some(raw) = state.runtime_config().get(PREVIEW_APPS_CONFIG_KEY) else {
        return default_public_preview_apps();
    };

    match parse_public_preview_apps_config(&raw) {
        Ok(config) => build_public_preview_apps(config),
        Err(error) => {
            tracing::warn!("failed to parse preview apps config: {error}");
            default_public_preview_apps()
        }
    }
}

pub(super) fn parse_public_preview_apps_config(value: &str) -> Result<PublicPreviewAppsConfig> {
    let mut config: PublicPreviewAppsConfig = serde_json::from_str(value).map_err(|error| {
        AsterError::validation_error(format!("preview apps config must be valid JSON: {error}"))
    })?;
    validate_preview_apps_config(&mut config)?;
    Ok(config)
}

fn build_public_preview_apps(config: PublicPreviewAppsConfig) -> PublicPreviewAppsConfig {
    PublicPreviewAppsConfig {
        version: config.version,
        apps: config.apps.into_iter().filter(|app| app.enabled).collect(),
    }
}

fn validate_preview_apps_config(config: &mut PublicPreviewAppsConfig) -> Result<()> {
    if config.version != PREVIEW_APPS_VERSION {
        return Err(AsterError::validation_error(format!(
            "preview apps config version must be {PREVIEW_APPS_VERSION}",
        )));
    }

    let mut seen_keys = HashSet::new();
    for app in &mut config.apps {
        app.key = normalize_non_empty("app key", &app.key)?;
        app.icon = app.icon.trim().to_string();
        app.labels = normalize_locale_labels(std::mem::take(&mut app.labels))?;
        if app.labels.is_empty() {
            return Err(AsterError::validation_error(format!(
                "preview app '{}' must provide localized labels",
                app.key
            )));
        }

        if !seen_keys.insert(app.key.clone()) {
            return Err(AsterError::validation_error(format!(
                "duplicate preview app key '{}'",
                app.key
            )));
        }

        validate_preview_app_config(app)?;
    }

    append_missing_required_builtin_preview_apps(config, &seen_keys)?;

    Ok(())
}

fn append_missing_required_builtin_preview_apps(
    config: &mut PublicPreviewAppsConfig,
    seen_keys: &HashSet<String>,
) -> Result<()> {
    let default_apps = default_public_preview_apps();
    for builtin_key in REQUIRED_BUILTIN_PREVIEW_APP_KEYS {
        if seen_keys.contains(*builtin_key) {
            continue;
        }

        let default_app = default_apps
            .apps
            .iter()
            .find(|app| app.key == *builtin_key)
            .cloned()
            .ok_or_else(|| {
                AsterError::internal_error(format!(
                    "required built-in preview app '{builtin_key}' is missing from defaults"
                ))
            })?;
        config.apps.push(default_app);
    }

    Ok(())
}

fn validate_preview_app_config(app: &mut PublicPreviewAppDefinition) -> Result<()> {
    let provider = app.provider;
    ensure_supported_provider(app, provider)?;

    normalize_match_list(&mut app.extensions, normalize_extension)?;

    if is_table_preview_app_key(&app.key) {
        let delimiter = app
            .config
            .delimiter
            .take()
            .unwrap_or_else(|| DEFAULT_TABLE_PREVIEW_DELIMITER.to_string());
        app.config.delimiter = Some(normalize_table_delimiter(&delimiter)?);

        return Ok(());
    }

    normalize_allowed_origins(&mut app.config.allowed_origins)?;
    normalize_form_fields(&mut app.config.form_fields);

    match provider {
        PreviewAppProvider::Builtin => {}
        PreviewAppProvider::UrlTemplate => {
            if app.config.mode.is_none() {
                return Err(AsterError::validation_error(format!(
                    "preview app '{}' url_template provider requires config.mode",
                    app.key
                )));
            }

            let url_template = app.config.url_template.take().ok_or_else(|| {
                AsterError::validation_error(format!(
                    "preview app '{}' url_template provider requires config.url_template",
                    app.key
                ))
            })?;
            app.config.url_template = Some(normalize_non_empty("url_template", &url_template)?);
        }
        PreviewAppProvider::Wopi => {
            if app.config.mode.is_none() {
                return Err(AsterError::validation_error(format!(
                    "preview app '{}' wopi provider requires config.mode",
                    app.key
                )));
            }

            app.config.action = normalize_optional_non_empty("action", app.config.action.take())
                .map(|action| action.to_ascii_lowercase());
            app.config.action_url =
                normalize_optional_non_empty("action_url", app.config.action_url.take());
            app.config.action_url_template = normalize_optional_non_empty(
                "action_url_template",
                app.config.action_url_template.take(),
            );
            app.config.discovery_url =
                normalize_optional_non_empty("discovery_url", app.config.discovery_url.take());
        }
    }

    Ok(())
}

fn ensure_supported_provider(
    app: &PublicPreviewAppDefinition,
    provider: PreviewAppProvider,
) -> Result<()> {
    if provider == PreviewAppProvider::Builtin && !is_required_builtin_preview_app_key(&app.key) {
        return Err(AsterError::validation_error(format!(
            "preview app '{}' cannot use builtin provider",
            app.key
        )));
    }

    if is_required_builtin_preview_app_key(&app.key) && provider != PreviewAppProvider::Builtin {
        return Err(AsterError::validation_error(format!(
            "preview app '{}' must use provider 'builtin'",
            app.key
        )));
    }

    Ok(())
}

fn normalize_match_list<F>(items: &mut Vec<String>, normalize: F) -> Result<()>
where
    F: Fn(&str) -> Result<String>,
{
    let mut unique = BTreeSet::new();
    for item in std::mem::take(items) {
        unique.insert(normalize(&item)?);
    }
    *items = unique.into_iter().collect();
    Ok(())
}

fn normalize_locale_labels(labels: BTreeMap<String, String>) -> Result<BTreeMap<String, String>> {
    let mut normalized = BTreeMap::new();

    for (locale, label) in labels {
        normalized.insert(
            normalize_locale_key(&locale)?,
            normalize_non_empty("label", &label)?,
        );
    }

    Ok(normalized)
}

fn normalize_locale_key(value: &str) -> Result<String> {
    let locale = normalize_non_empty("label locale", value)?
        .to_ascii_lowercase()
        .replace('_', "-");

    if !locale
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
    {
        return Err(AsterError::validation_error(format!(
            "unsupported label locale '{locale}'",
        )));
    }

    Ok(locale)
}

fn normalize_non_empty(field: &str, value: &str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(AsterError::validation_error(format!(
            "{field} must not be empty"
        )));
    }
    Ok(trimmed.to_string())
}

fn normalize_extension(value: &str) -> Result<String> {
    let normalized = normalize_non_empty("extension", value)?;
    Ok(normalized.trim_start_matches('.').to_ascii_lowercase())
}

fn normalize_table_delimiter(value: &str) -> Result<String> {
    match value.trim() {
        "auto" => Ok("auto".to_string()),
        "," => Ok(",".to_string()),
        "\t" => Ok("\t".to_string()),
        ";" => Ok(";".to_string()),
        "|" => Ok("|".to_string()),
        _ => Err(AsterError::validation_error(
            "table delimiter must be one of: auto, ',', '\\t', ';', '|'",
        )),
    }
}

fn normalize_optional_non_empty(field: &str, value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(normalize_non_empty(field, trimmed).ok()?)
        }
    })
}

fn normalize_allowed_origins(origins: &mut Vec<String>) -> Result<()> {
    let mut normalized = Vec::new();
    for origin in std::mem::take(origins) {
        let origin = normalize_non_empty("allowed_origin", &origin)?;
        if !normalized.contains(&origin) {
            normalized.push(origin);
        }
    }
    *origins = normalized;
    Ok(())
}

fn normalize_form_fields(form_fields: &mut BTreeMap<String, String>) {
    let mut normalized = BTreeMap::new();
    for (key, value) in std::mem::take(form_fields) {
        let key = key.trim();
        let value = value.trim();
        if key.is_empty() || value.is_empty() {
            continue;
        }
        normalized.insert(key.to_string(), value.to_string());
    }
    *form_fields = normalized;
}
