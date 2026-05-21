use crate::config::{mail, media_processing};
use crate::db::repository::user_repo;
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::runtime::PrimaryAppState;
use crate::services::{
    audit_service::{self, AuditContext},
    mail_service, media_processing_service, preview_app_service, wopi_service,
};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use super::system::set;

pub const MAIL_CONFIG_ACTION_KEY: &str = "mail";
const PREVIEW_APP_DISCOVERY_GENERATED_SEGMENT: &str = "__wopi_discovery__";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub enum ConfigActionType {
    BuildWopiDiscoveryPreviewConfig,
    SendTestEmail,
    TestVipsCli,
    TestFfmpegCli,
    TestFfprobeCli,
}

impl ConfigActionType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BuildWopiDiscoveryPreviewConfig => "build_wopi_discovery_preview_config",
            Self::SendTestEmail => "send_test_email",
            Self::TestVipsCli => "test_vips_cli",
            Self::TestFfmpegCli => "test_ffmpeg_cli",
            Self::TestFfprobeCli => "test_ffprobe_cli",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigActionResult {
    pub message: String,
    pub target_email: Option<String>,
    pub value: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct ExecuteConfigActionInput<'a> {
    pub key: &'a str,
    pub action: ConfigActionType,
    pub actor_user_id: i64,
    pub target_email: Option<&'a str>,
    pub value: Option<&'a str>,
    pub discovery_url: Option<&'a str>,
}

pub async fn execute_action(
    state: &PrimaryAppState,
    input: ExecuteConfigActionInput<'_>,
) -> Result<ConfigActionResult> {
    let ExecuteConfigActionInput {
        key,
        action,
        actor_user_id,
        target_email,
        value,
        discovery_url,
    } = input;
    match key {
        MAIL_CONFIG_ACTION_KEY => {
            execute_mail_action(state, action, actor_user_id, target_email).await
        }
        preview_app_service::PREVIEW_APPS_CONFIG_KEY => {
            execute_preview_app_action(state, action, actor_user_id, value, discovery_url).await
        }
        media_processing::MEDIA_PROCESSING_REGISTRY_JSON_KEY => {
            execute_media_processing_action(state, action, actor_user_id, value).await
        }
        _ => Err(AsterError::record_not_found(format!(
            "config action target '{key}'"
        ))),
    }
}

pub async fn execute_action_with_audit(
    state: &PrimaryAppState,
    input: ExecuteConfigActionInput<'_>,
    audit_ctx: &AuditContext,
) -> Result<ConfigActionResult> {
    let action_result = execute_action(state, input).await?;
    audit_service::log(
        state,
        audit_ctx,
        audit_service::AuditAction::ConfigActionExecute,
        audit_service::AuditEntityType::SystemConfig,
        None,
        Some(input.key),
        audit_service::details(audit_service::ConfigActionDetails {
            action: input.action.as_str(),
            target_email: action_result.target_email.as_deref(),
        }),
    )
    .await;
    Ok(action_result)
}

async fn execute_mail_action(
    state: &PrimaryAppState,
    action: ConfigActionType,
    actor_user_id: i64,
    target_email: Option<&str>,
) -> Result<ConfigActionResult> {
    match action {
        ConfigActionType::SendTestEmail => {
            let actor = user_repo::find_by_id(&state.db, actor_user_id).await?;
            let requested_target = target_email.unwrap_or(&actor.email);
            let normalized_target = mail::normalize_mail_address_config_value(requested_target)?;
            if normalized_target.is_empty() {
                return Err(AsterError::validation_error("target_email is required"));
            }

            tracing::debug!(
                actor_user_id,
                actor_username = %actor.username,
                target_email = %normalized_target,
                action = %action.as_str(),
                "config: executing mail action"
            );

            mail_service::send_test_email(state, &normalized_target, Some(&actor.username)).await?;

            Ok(ConfigActionResult {
                message: format!("Test email sent to {normalized_target}"),
                target_email: Some(normalized_target),
                value: None,
            })
        }
        _ => Err(AsterError::validation_error(format!(
            "action '{}' is not supported for '{MAIL_CONFIG_ACTION_KEY}'",
            action.as_str()
        ))),
    }
}

async fn execute_preview_app_action(
    state: &PrimaryAppState,
    action: ConfigActionType,
    actor_user_id: i64,
    value: Option<&str>,
    discovery_url: Option<&str>,
) -> Result<ConfigActionResult> {
    match action {
        ConfigActionType::BuildWopiDiscoveryPreviewConfig => {
            let raw_value = value.map(str::to_string).unwrap_or_else(|| {
                state
                    .runtime_config
                    .get(preview_app_service::PREVIEW_APPS_CONFIG_KEY)
                    .unwrap_or_else(preview_app_service::default_public_preview_apps_json)
            });
            let normalized =
                preview_app_service::normalize_public_preview_apps_config_value(&raw_value)?;
            let mut config: preview_app_service::PublicPreviewAppsConfig =
                serde_json::from_str(&normalized).map_aster_err_ctx(
                    "failed to parse normalized preview apps config",
                    AsterError::internal_error,
                )?;

            let requested_discovery_url =
                discovery_url.map(str::trim).filter(|url| !url.is_empty());
            let Some(discovery_url) = requested_discovery_url else {
                return Err(AsterError::validation_error("discovery_url is required"));
            };
            build_wopi_discovery_preview_apps_into_config(state, &mut config, discovery_url)
                .await?;
            let serialized = serde_json::to_string_pretty(&config).map_aster_err_ctx(
                "failed to serialize imported preview apps config",
                AsterError::internal_error,
            )?;

            if value.is_none() {
                set(
                    state,
                    preview_app_service::PREVIEW_APPS_CONFIG_KEY,
                    &serialized,
                    actor_user_id,
                )
                .await?;
            }

            Ok(ConfigActionResult {
                message: format!(
                    "Built WOPI preview apps from {discovery_url} into the preview app draft"
                ),
                target_email: None,
                value: Some(serialized),
            })
        }
        _ => Err(AsterError::validation_error(format!(
            "action '{}' is not supported for '{}'",
            action.as_str(),
            preview_app_service::PREVIEW_APPS_CONFIG_KEY
        ))),
    }
}

async fn execute_media_processing_action(
    state: &PrimaryAppState,
    action: ConfigActionType,
    actor_user_id: i64,
    value: Option<&str>,
) -> Result<ConfigActionResult> {
    match action {
        ConfigActionType::TestVipsCli => {
            let raw_value = value.map(str::to_string).unwrap_or_else(|| {
                state
                    .runtime_config
                    .get(media_processing::MEDIA_PROCESSING_REGISTRY_JSON_KEY)
                    .unwrap_or_else(media_processing::default_media_processing_registry_json)
            });
            let command = media_processing::vips_command_from_registry_value(&raw_value)?;

            tracing::debug!(
                actor_user_id,
                action = %action.as_str(),
                command = %command,
                "config: executing media processing action"
            );

            let message = media_processing_service::probe_vips_cli_command(&command).await?;

            Ok(ConfigActionResult {
                message,
                target_email: None,
                value: None,
            })
        }
        ConfigActionType::TestFfmpegCli => {
            let raw_value = value.map(str::to_string).unwrap_or_else(|| {
                state
                    .runtime_config
                    .get(media_processing::MEDIA_PROCESSING_REGISTRY_JSON_KEY)
                    .unwrap_or_else(media_processing::default_media_processing_registry_json)
            });
            let command = media_processing::ffmpeg_command_from_registry_value(&raw_value)?;

            tracing::debug!(
                actor_user_id,
                action = %action.as_str(),
                command = %command,
                "config: executing media processing action"
            );

            let message = media_processing_service::probe_ffmpeg_cli_command(&command).await?;

            Ok(ConfigActionResult {
                message,
                target_email: None,
                value: None,
            })
        }
        ConfigActionType::TestFfprobeCli => {
            let raw_value = value.map(str::to_string).unwrap_or_else(|| {
                state
                    .runtime_config
                    .get(media_processing::MEDIA_PROCESSING_REGISTRY_JSON_KEY)
                    .unwrap_or_else(media_processing::default_media_processing_registry_json)
            });
            let command = media_processing::ffprobe_command_from_registry_value(&raw_value)?;

            tracing::debug!(
                actor_user_id,
                action = %action.as_str(),
                command = %command,
                "config: executing media processing action"
            );

            let message =
                crate::services::media_metadata_service::probe_ffprobe_cli_command(&command)
                    .await?;

            Ok(ConfigActionResult {
                message,
                target_email: None,
                value: None,
            })
        }
        _ => Err(AsterError::validation_error(format!(
            "action '{}' is not supported for '{}'",
            action.as_str(),
            media_processing::MEDIA_PROCESSING_REGISTRY_JSON_KEY
        ))),
    }
}

async fn build_wopi_discovery_preview_apps_into_config(
    state: &PrimaryAppState,
    config: &mut preview_app_service::PublicPreviewAppsConfig,
    discovery_url: &str,
) -> Result<()> {
    let discovery_url = discovery_url.trim();
    if discovery_url.is_empty() {
        return Err(AsterError::validation_error("discovery_url is required"));
    }

    let discovered_apps = wopi_service::discover_preview_apps(state, discovery_url).await?;
    let existing_generated_apps = config
        .apps
        .iter()
        .filter(|app| is_generated_wopi_discovery_app(app, discovery_url))
        .filter_map(|app| {
            generated_preview_app_suffix(&app.key).map(|suffix| (suffix.to_string(), app.clone()))
        })
        .collect::<BTreeMap<_, _>>();
    let imported_apps =
        build_imported_wopi_preview_apps(discovery_url, &existing_generated_apps, discovered_apps)?;

    let mut next_apps = Vec::with_capacity(config.apps.len() + imported_apps.len());
    for app in &config.apps {
        if is_generated_wopi_discovery_app(app, discovery_url) {
            continue;
        }
        next_apps.push(app.clone());
    }

    next_apps.extend(imported_apps);
    config.apps = next_apps;
    Ok(())
}

fn build_imported_wopi_preview_apps(
    discovery_url: &str,
    existing_generated_apps: &BTreeMap<String, preview_app_service::PublicPreviewAppDefinition>,
    discovered_apps: Vec<wopi_service::DiscoveredWopiPreviewApp>,
) -> Result<Vec<preview_app_service::PublicPreviewAppDefinition>> {
    let mut imported = Vec::new();

    for discovered_app in discovered_apps {
        let key = format!(
            "{}{}",
            generated_preview_app_key_prefix(discovery_url),
            discovered_app.key_suffix
        );
        let enabled = existing_generated_apps
            .get(&discovered_app.key_suffix)
            .map(|app| app.enabled)
            .unwrap_or(true);

        imported.push(preview_app_service::PublicPreviewAppDefinition {
            key,
            provider: preview_app_service::PreviewAppProvider::Wopi,
            icon: discovered_app
                .icon_url
                .unwrap_or_else(|| "/static/preview-apps/file.svg".to_string()),
            enabled,
            labels: BTreeMap::from([
                ("en".to_string(), discovered_app.label.clone()),
                ("zh".to_string(), discovered_app.label.clone()),
            ]),
            extensions: discovered_app.extensions,
            config: preview_app_service::PublicPreviewAppConfig {
                delimiter: None,
                mode: Some(preview_app_service::PreviewOpenMode::Iframe),
                url_template: None,
                allowed_origins: Vec::new(),
                action: Some(discovered_app.action),
                action_url: None,
                action_url_template: None,
                discovery_url: Some(discovery_url.to_string()),
                form_fields: BTreeMap::new(),
            },
        });
    }

    if imported.is_empty() {
        return Err(AsterError::validation_error(format!(
            "WOPI discovery '{discovery_url}' did not produce any importable apps"
        )));
    }

    Ok(imported)
}

fn is_generated_wopi_discovery_app(
    app: &preview_app_service::PublicPreviewAppDefinition,
    discovery_url: &str,
) -> bool {
    app.provider == preview_app_service::PreviewAppProvider::Wopi
        && app.key.contains(PREVIEW_APP_DISCOVERY_GENERATED_SEGMENT)
        && app.config.discovery_url.as_deref() == Some(discovery_url)
}

fn generated_preview_app_key_prefix(discovery_url: &str) -> String {
    format!(
        "custom.wopi.{}{PREVIEW_APP_DISCOVERY_GENERATED_SEGMENT}",
        discovery_key_segment(discovery_url)
    )
}

fn generated_preview_app_suffix(key: &str) -> Option<&str> {
    key.split_once(PREVIEW_APP_DISCOVERY_GENERATED_SEGMENT)
        .map(|(_, suffix)| suffix)
        .filter(|suffix| !suffix.trim().is_empty())
}

fn discovery_key_segment(discovery_url: &str) -> String {
    let value = Url::parse(discovery_url)
        .ok()
        .map(|url| {
            let mut next = url.host_str().unwrap_or_default().to_string();
            if let Some(port) = url.port() {
                if !next.is_empty() {
                    next.push('.');
                }
                next.push_str(&port.to_string());
            }
            let path = url
                .path_segments()
                .map(|segments| {
                    segments
                        .filter(|segment| !segment.trim().is_empty())
                        .collect::<Vec<_>>()
                        .join(".")
                })
                .unwrap_or_default();
            if !path.is_empty() {
                if !next.is_empty() {
                    next.push('.');
                }
                next.push_str(&path);
            }
            next
        })
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| discovery_url.trim().to_string());
    slugify_preview_app_key_segment(&value)
}

fn slugify_preview_app_key_segment(value: &str) -> String {
    let mut slug = String::new();
    let mut last_was_separator = false;

    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            last_was_separator = false;
            continue;
        }

        if !last_was_separator {
            slug.push('.');
            last_was_separator = true;
        }
    }

    let trimmed = slug.trim_matches('.');
    if trimmed.is_empty() {
        "discovery".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        PREVIEW_APP_DISCOVERY_GENERATED_SEGMENT, build_imported_wopi_preview_apps,
        generated_preview_app_key_prefix, generated_preview_app_suffix,
        is_generated_wopi_discovery_app,
    };
    use crate::services::{preview_app_service, wopi_service};
    use std::collections::BTreeMap;

    #[test]
    fn generated_preview_app_key_prefix_uses_reserved_segment() {
        assert_eq!(
            generated_preview_app_key_prefix("https://office.esaps.net/hosting/discovery"),
            format!(
                "custom.wopi.office.esaps.net.hosting.discovery{PREVIEW_APP_DISCOVERY_GENERATED_SEGMENT}"
            )
        );
    }

    #[test]
    fn build_imported_wopi_preview_apps_preserves_existing_enabled_state() {
        let discovery_url = "http://localhost:8080/hosting/discovery";
        let existing_key =
            "custom.wopi.localhost.8080.hosting.discovery__wopi_discovery__word".to_string();
        let existing_generated = BTreeMap::from([(
            "word".to_string(),
            preview_app_service::PublicPreviewAppDefinition {
                key: existing_key,
                provider: preview_app_service::PreviewAppProvider::Wopi,
                icon: "http://localhost:8080/word.ico".to_string(),
                enabled: false,
                labels: BTreeMap::from([("en".to_string(), "Word".to_string())]),
                extensions: vec!["docx".to_string()],
                config: preview_app_service::PublicPreviewAppConfig {
                    mode: Some(preview_app_service::PreviewOpenMode::Iframe),
                    action: Some("view".to_string()),
                    discovery_url: Some(discovery_url.to_string()),
                    ..Default::default()
                },
            },
        )]);

        let imported = build_imported_wopi_preview_apps(
            discovery_url,
            &existing_generated,
            vec![wopi_service::DiscoveredWopiPreviewApp {
                action: "view".to_string(),
                extensions: vec!["doc".to_string(), "docx".to_string()],
                icon_url: Some("http://localhost:8080/word.ico".to_string()),
                key_suffix: "word".to_string(),
                label: "Word".to_string(),
            }],
        )
        .unwrap();

        assert_eq!(imported.len(), 1);
        assert_eq!(
            imported[0].key,
            "custom.wopi.localhost.8080.hosting.discovery__wopi_discovery__word"
        );
        assert!(!imported[0].enabled);
        assert_eq!(imported[0].config.action.as_deref(), Some("view"));
        assert_eq!(
            imported[0].config.discovery_url.as_deref(),
            Some(discovery_url)
        );
        assert_eq!(imported[0].extensions, vec!["doc", "docx"]);
    }

    #[test]
    fn build_imported_wopi_preview_apps_enables_new_entries_by_default() {
        let imported = build_imported_wopi_preview_apps(
            "http://localhost:8080/hosting/discovery",
            &BTreeMap::new(),
            vec![wopi_service::DiscoveredWopiPreviewApp {
                action: "view".to_string(),
                extensions: vec!["docx".to_string()],
                icon_url: Some("http://localhost:8080/word.ico".to_string()),
                key_suffix: "word".to_string(),
                label: "Word".to_string(),
            }],
        )
        .unwrap();

        assert_eq!(imported.len(), 1);
        assert!(imported[0].enabled);
    }

    #[test]
    fn generated_preview_app_suffix_extracts_suffix() {
        assert_eq!(
            generated_preview_app_suffix("custom.wopi.office.esaps.net__wopi_discovery__word"),
            Some("word")
        );
    }

    #[test]
    fn legacy_wopi_seed_is_not_treated_as_generated_app() {
        let discovery_url = "http://localhost:8080/hosting/discovery";
        let legacy_seed = preview_app_service::PublicPreviewAppDefinition {
            key: "custom.wopi.word".to_string(),
            provider: preview_app_service::PreviewAppProvider::Wopi,
            icon: "http://localhost:8080/word.ico".to_string(),
            enabled: true,
            labels: BTreeMap::from([("en".to_string(), "Word".to_string())]),
            extensions: Vec::new(),
            config: preview_app_service::PublicPreviewAppConfig {
                discovery_url: Some(discovery_url.to_string()),
                ..Default::default()
            },
        };

        assert!(!is_generated_wopi_discovery_app(
            &legacy_seed,
            discovery_url
        ));
    }
}
