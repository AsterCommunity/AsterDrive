use crate::services::wopi_service::proof::WopiProofKeySet;
use crate::services::wopi_service::types::DiscoveredWopiApp;

use super::types::{WopiDiscovery, WopiDiscoveryAction};

const EDIT_FALLBACK_ACTION_PRIORITY: &[&str] = &[
    "edit",
    "embededit",
    "mobileedit",
    "view",
    "embedview",
    "mobileview",
];

const EMBED_EDIT_FALLBACK_ACTION_PRIORITY: &[&str] = &[
    "embededit",
    "edit",
    "mobileedit",
    "embedview",
    "view",
    "mobileview",
];

const MOBILE_EDIT_FALLBACK_ACTION_PRIORITY: &[&str] = &[
    "mobileedit",
    "edit",
    "embededit",
    "mobileview",
    "view",
    "embedview",
];

const DISCOVERY_ACTION_PRIORITY: &[&str] = EDIT_FALLBACK_ACTION_PRIORITY;

impl WopiDiscovery {
    pub(crate) fn find_action_url(
        &self,
        action: &str,
        extension: Option<&str>,
        mime_type: &str,
    ) -> Option<String> {
        let action = action.to_ascii_lowercase();
        let extension = extension.map(|value| value.to_ascii_lowercase());
        let mime_type = mime_type.trim().to_ascii_lowercase();

        self.actions
            .iter()
            .find(|item| item.action == action && item.ext.as_deref() == extension.as_deref())
            .or_else(|| {
                self.actions.iter().find(|item| {
                    item.action == action && item.mime.as_deref() == Some(mime_type.as_str())
                })
            })
            .or_else(|| {
                self.actions
                    .iter()
                    .find(|item| item.action == action && item.ext.as_deref() == Some("*"))
            })
            .map(|item| item.urlsrc.clone())
    }

    pub(crate) fn proof_keys(&self) -> Option<&WopiProofKeySet> {
        self.proof_keys.as_ref()
    }
}

pub(crate) fn resolve_discovery_action_url(
    discovery: &WopiDiscovery,
    requested_action: &str,
    extension: Option<&str>,
    mime_type: &str,
) -> Option<String> {
    let preferred_actions = preferred_discovery_actions(requested_action);

    preferred_actions
        .iter()
        .find_map(|action| discovery.find_action_url(action, extension, mime_type))
}

fn preferred_discovery_actions(requested_action: &str) -> Vec<String> {
    let normalized = requested_action.trim().to_ascii_lowercase();
    let mut actions = Vec::new();

    let fallback_priority = match normalized.as_str() {
        "embededit" => EMBED_EDIT_FALLBACK_ACTION_PRIORITY,
        "mobileedit" => MOBILE_EDIT_FALLBACK_ACTION_PRIORITY,
        _ => EDIT_FALLBACK_ACTION_PRIORITY,
    };

    if !normalized.is_empty() && !is_known_discovery_action(&normalized) {
        actions.push(normalized.clone());
    }

    for candidate in fallback_priority {
        if actions.iter().any(|existing| existing == candidate) {
            continue;
        }
        actions.push((*candidate).to_string());
    }

    actions
}

fn is_known_discovery_action(action: &str) -> bool {
    DISCOVERY_ACTION_PRIORITY.contains(&action)
}

pub(crate) fn build_discovered_apps(discovery: &WopiDiscovery) -> Vec<DiscoveredWopiApp> {
    #[derive(Debug, Clone)]
    struct DiscoveryGroup {
        icon_url: Option<String>,
        label: String,
        actions: Vec<WopiDiscoveryAction>,
    }

    let mut groups = Vec::<DiscoveryGroup>::new();
    for action in &discovery.actions {
        let label = action
            .app_name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("WOPI");

        if let Some(group) = groups.iter_mut().find(|group| group.label == label) {
            group.actions.push(action.clone());
            if group.icon_url.is_none() {
                group.icon_url = action.app_icon_url.clone();
            }
            continue;
        }

        groups.push(DiscoveryGroup {
            icon_url: action.app_icon_url.clone(),
            label: label.to_string(),
            actions: vec![action.clone()],
        });
    }

    let mut results = Vec::new();
    let mut used_suffixes = std::collections::HashSet::new();

    for group in groups {
        let action_name = DISCOVERY_ACTION_PRIORITY
            .iter()
            .find_map(|candidate| {
                let has_extensions = group.actions.iter().any(|action| {
                    action.action == *candidate
                        && action
                            .ext
                            .as_deref()
                            .is_some_and(|ext| !ext.is_empty() && ext != "*")
                });
                has_extensions.then_some((*candidate).to_string())
            })
            .or_else(|| {
                group.actions.iter().find_map(|action| {
                    action
                        .ext
                        .as_deref()
                        .is_some_and(|ext| !ext.is_empty() && ext != "*")
                        .then(|| action.action.clone())
                })
            });

        let Some(action_name) = action_name else {
            continue;
        };

        let mut extensions = Vec::new();
        for action in &group.actions {
            let should_collect_extension = if is_known_discovery_action(&action_name) {
                is_known_discovery_action(&action.action)
            } else {
                action.action == action_name
            };

            if !should_collect_extension {
                continue;
            }
            if let Some(ext) = action.ext.as_deref()
                && !ext.is_empty()
                && ext != "*"
            {
                push_unique(&mut extensions, ext.to_string());
            }
        }

        if extensions.is_empty() {
            continue;
        }

        let mut key_suffix = slugify_discovery_app_name(&group.label);
        if key_suffix.is_empty() {
            key_suffix = "app".to_string();
        }

        if !used_suffixes.insert(key_suffix.clone()) {
            let base = key_suffix.clone();
            let mut index = 2;
            loop {
                let candidate = format!("{base}_{index}");
                if used_suffixes.insert(candidate.clone()) {
                    key_suffix = candidate;
                    break;
                }
                index += 1;
            }
        }

        results.push(DiscoveredWopiApp {
            action: action_name,
            extensions,
            icon_url: group.icon_url,
            key_suffix,
            label: group.label,
        });
    }

    results
}

fn slugify_discovery_app_name(value: &str) -> String {
    let mut slug = String::new();
    let mut previous_was_separator = false;

    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            previous_was_separator = false;
            continue;
        }

        if !previous_was_separator && !slug.is_empty() {
            slug.push('_');
            previous_was_separator = true;
        }
    }

    slug.trim_matches('_').to_string()
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.iter().any(|existing| existing == &value) {
        values.push(value);
    }
}
