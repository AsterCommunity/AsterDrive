//! Typed config command results, independent from execution and rendering.
use super::transfer::ConfigBackup;
use crate::services::ops::config::SystemConfig;
use serde::Serialize;
#[derive(Debug, Serialize)]
pub(super) struct ConfigListOutput {
    pub(super) count: usize,
    pub(super) configs: Vec<SystemConfig>,
}

#[derive(Debug, Serialize)]
pub(super) struct DeleteOutput {
    pub(super) key: String,
    pub(super) deleted: bool,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum ConfigCommandKind {
    List,
    Get,
    Set,
    Delete,
    Validate,
    Export,
    Import,
    Unchanged,
    Preview,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub(super) enum ConfigCommandPayload {
    List(ConfigListOutput),
    Backup(ConfigBackup),
    Config(SystemConfig),
    Delete(DeleteOutput),
}

#[derive(Debug)]
pub(in crate::cli) struct ConfigCommandReport {
    pub(super) kind: ConfigCommandKind,
    pub(super) payload: ConfigCommandPayload,
    pub(super) changes: Vec<ConfigChange>,
}

#[derive(Debug, Serialize)]
pub(super) struct ConfigChange {
    pub(super) action: &'static str,
    pub(super) key: String,
}

impl ConfigCommandReport {
    pub(super) fn list(kind: ConfigCommandKind, configs: Vec<SystemConfig>) -> Self {
        Self {
            kind,
            payload: ConfigCommandPayload::List(ConfigListOutput {
                count: configs.len(),
                configs,
            }),
            changes: Vec::new(),
        }
    }

    pub(super) fn config(kind: ConfigCommandKind, config: SystemConfig) -> Self {
        Self {
            kind,
            payload: ConfigCommandPayload::Config(config),
            changes: Vec::new(),
        }
    }

    pub(super) fn delete(key: String) -> Self {
        Self {
            kind: ConfigCommandKind::Delete,
            payload: ConfigCommandPayload::Delete(DeleteOutput { key, deleted: true }),
            changes: Vec::new(),
        }
    }

    pub(super) fn preview_delete(key: String) -> Self {
        Self {
            kind: ConfigCommandKind::Preview,
            payload: ConfigCommandPayload::Delete(DeleteOutput {
                key: key.clone(),
                deleted: false,
            }),
            changes: vec![ConfigChange {
                action: "would_delete",
                key,
            }],
        }
    }

    pub(super) fn preview_config(config: SystemConfig, action: &'static str) -> Self {
        let key = config.key.clone();
        Self {
            kind: ConfigCommandKind::Preview,
            payload: ConfigCommandPayload::Config(config),
            changes: vec![ConfigChange { action, key }],
        }
    }

    pub(super) fn preview_list(configs: Vec<SystemConfig>, changes: Vec<ConfigChange>) -> Self {
        Self {
            kind: ConfigCommandKind::Preview,
            payload: ConfigCommandPayload::List(ConfigListOutput {
                count: configs.len(),
                configs,
            }),
            changes,
        }
    }
}
