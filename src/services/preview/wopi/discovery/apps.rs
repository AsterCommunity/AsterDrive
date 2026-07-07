use crate::entities::file;
use crate::errors::{AsterError, Result};
use crate::runtime::SharedRuntimeState;
use crate::services::preview::apps;
use crate::services::preview::wopi::targets::file_extension;
use crate::services::preview::wopi::types::DiscoveredWopiApp;

use super::actions::{build_discovered_apps, resolve_discovery_action_url};
use super::cache::load_discovery;
use super::types::WopiAppConfig;
use super::url::{append_wopi_src, expand_action_url, trusted_origins_for_app};

pub fn allowed_origins(state: &impl SharedRuntimeState) -> Vec<String> {
    let mut origins = Vec::new();

    for app in apps::get_public_preview_apps(state).apps {
        if app.provider != apps::PreviewAppProvider::Wopi {
            continue;
        }
        for origin in trusted_origins_for_app(&app) {
            push_unique(&mut origins, origin);
        }
    }

    origins
}

pub async fn discover_apps(
    state: &impl SharedRuntimeState,
    discovery_url: &str,
) -> Result<Vec<DiscoveredWopiApp>> {
    let discovery = load_discovery(state, discovery_url).await?;
    let apps = build_discovered_apps(&discovery);
    if apps.is_empty() {
        return Err(AsterError::validation_error(
            "WOPI discovery did not expose any importable apps",
        ));
    }
    Ok(apps)
}

pub(crate) fn parse_wopi_app_config(
    app: &apps::PublicPreviewAppDefinition,
) -> Result<WopiAppConfig> {
    if app.provider != apps::PreviewAppProvider::Wopi {
        return Err(AsterError::validation_error(format!(
            "app '{}' is not a WOPI provider",
            app.key
        )));
    }

    let mode = app.config.mode.ok_or_else(|| {
        AsterError::validation_error(format!("WOPI app '{}' requires config.mode", app.key))
    })?;

    let action = app
        .config
        .action
        .as_deref()
        .unwrap_or("edit")
        .to_ascii_lowercase();

    let action_url = app
        .config
        .action_url
        .clone()
        .or_else(|| app.config.action_url_template.clone());
    let discovery_url = app.config.discovery_url.clone();
    if action_url.is_none() && discovery_url.is_none() {
        return Err(AsterError::validation_error(format!(
            "WOPI app '{}' requires config.action_url or config.discovery_url",
            app.key
        )));
    }

    Ok(WopiAppConfig {
        action,
        action_url,
        discovery_url,
        form_fields: app.config.form_fields.clone(),
        mode,
    })
}

pub(crate) async fn resolve_action_url(
    state: &impl SharedRuntimeState,
    app_config: &WopiAppConfig,
    file: &file::Model,
    wopi_src: &str,
) -> Result<String> {
    if let Some(action_url) = app_config.action_url.as_deref() {
        return expand_action_url(action_url, wopi_src);
    }

    let discovery_url = app_config
        .discovery_url
        .as_deref()
        .ok_or_else(|| AsterError::validation_error("missing WOPI discovery URL"))?;
    let discovery = load_discovery(state, discovery_url).await?;
    let extension = file_extension(&file.name);
    let urlsrc = resolve_discovery_action_url(
        &discovery,
        &app_config.action,
        extension.as_deref(),
        &file.mime_type,
    )
    .ok_or_else(|| {
        AsterError::validation_error(format!(
            "WOPI discovery has no compatible action for '{}' (preferred action '{}')",
            file.name, app_config.action
        ))
    })?;
    append_wopi_src(&urlsrc, wopi_src)
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.iter().any(|existing| existing == &value) {
        values.push(value);
    }
}
