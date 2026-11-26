//! 审计日志运行时配置：记录开关、可配置的审计动作范围及其预编译位掩码。

use crate::config::bool_like::parse_bool_like;
pub use crate::config::definitions::{AUDIT_LOG_ENABLED_KEY, AUDIT_LOG_RECORDED_ACTIONS_KEY};
use crate::errors::{AsterError, Result};
use aster_drive_model::types::AuditAction;

/// Default value of [`AUDIT_LOG_ENABLED_KEY`] when the runtime config is
/// missing or unparseable: audit logging is on.
pub const DEFAULT_AUDIT_LOG_ENABLED: bool = true;
// Ceiling division: AuditAction::COUNT is the number of action bits, and
// u64::BITS is each word's capacity, yielding the required u64 word count.
/// Number of `u64` words in the per-action bitmask; one bit per [`AuditAction`].
pub const AUDIT_ACTION_MASK_WORDS: usize = AuditAction::COUNT.div_ceil(u64::BITS as usize);

/// Precompiled audit recording scope for the request hot path.
///
/// Holds the enabled flag plus a bitmask over [`AuditAction`] values, so
/// `should_record` is a cheap mask test instead of re-parsing config strings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditLogRuntimeSettings {
    enabled: bool,
    action_mask: [u64; AUDIT_ACTION_MASK_WORDS],
}

impl AuditLogRuntimeSettings {
    // Precompute the scope once so the audit hot path only does a bitmask check.
    /// Builds the recording scope from raw runtime config values.
    ///
    /// `enabled_raw` falls back to [`DEFAULT_AUDIT_LOG_ENABLED`] when missing or
    /// invalid; `actions_raw` falls back to recording all non-workspace actions.
    pub fn from_raw_values(enabled_raw: Option<&str>, actions_raw: Option<&str>) -> Self {
        let enabled = enabled_raw
            .and_then(parse_bool_like)
            .unwrap_or(DEFAULT_AUDIT_LOG_ENABLED);
        let action_mask = actions_raw
            .map(parse_action_mask_for_runtime)
            .unwrap_or_else(all_actions_mask);

        Self {
            enabled,
            action_mask,
        }
    }

    /// Whether an audit entry should be written for `action`.
    ///
    /// Workspace-mutation actions are always recorded regardless of the mask;
    /// every other action requires both `enabled` and mask membership.
    pub fn should_record(&self, action: AuditAction) -> bool {
        self.enabled && !action.is_workspace_mutation() && mask_contains(self.action_mask, action)
    }
}

impl Default for AuditLogRuntimeSettings {
    fn default() -> Self {
        Self {
            enabled: DEFAULT_AUDIT_LOG_ENABLED,
            action_mask: all_actions_mask(),
        }
    }
}

/// JSON array stored as the default value of [`AUDIT_LOG_RECORDED_ACTIONS_KEY`].
///
/// Contains every non-workspace audit action in authoritative [`AuditAction::ALL`]
/// order so schema and storage stay stable.
pub fn default_recorded_actions_value() -> String {
    // Keep the stored default in authoritative action order so schema and storage stay stable.
    let values: Vec<&'static str> = AuditAction::ALL
        .iter()
        .filter(|action| !action.is_workspace_mutation())
        .map(|action| action.as_str())
        .collect();
    serde_json::to_string(&values).unwrap_or_else(|error| {
        tracing::error!(
            error = %error,
            "failed to serialize default audit_log_recorded_actions config; using full audit action fallback"
        );
        full_recorded_actions_json_fallback()
    })
}

fn full_recorded_actions_json_fallback() -> String {
    let values: Vec<&'static str> = AuditAction::ALL
        .iter()
        .filter(|action| !action.is_workspace_mutation())
        .map(|action| action.as_str())
        .collect();
    serde_json::json!(values).to_string()
}

/// Normalizes a stored [`AUDIT_LOG_RECORDED_ACTIONS_KEY`] value.
///
/// Validates and deduplicates the action list, rejects workspace-mutation
/// actions (they are always recorded and not configurable), and re-serializes
/// the selection in authoritative [`AuditAction::ALL`] order.
pub fn normalize_recorded_actions_config_value(value: &str) -> Result<String> {
    let actions = parse_recorded_actions_config_value(value)?;
    let mut selected = [false; AuditAction::COUNT];
    for action in actions {
        if action.is_workspace_mutation() {
            return Err(AsterError::validation_error(format!(
                "{} is a required workspace activity, not a configurable audit action",
                action.as_str()
            )));
        }
        selected[action.index()] = true;
    }

    let normalized: Vec<&'static str> = AuditAction::ALL
        .iter()
        .copied()
        .filter(|action| selected[action.index()])
        .filter(|action| !action.is_workspace_mutation())
        .map(|action| action.as_str())
        .collect();
    serde_json::to_string(&normalized).map_err(|error| {
        AsterError::internal_error(format!(
            "failed to serialize audit_log_recorded_actions config: {error}"
        ))
    })
}

/// Parses a recorded-actions JSON value into [`AuditAction`] entries.
///
/// Rejects unknown names and duplicates, but does not filter workspace-mutation
/// actions; callers that normalize stored values must reject those themselves.
pub fn parse_recorded_actions_config_value(value: &str) -> Result<Vec<AuditAction>> {
    let values = parse_string_array(value, "string_enum_set")?;
    let mut seen = [false; AuditAction::COUNT];
    let mut actions = Vec::with_capacity(values.len());

    for raw in values {
        let Some(action) = AuditAction::from_str_name(&raw) else {
            return Err(AsterError::validation_error(format!(
                "unknown audit action '{raw}' in {AUDIT_LOG_RECORDED_ACTIONS_KEY}"
            )));
        };
        let index = action.index();
        if seen[index] {
            return Err(AsterError::validation_error(format!(
                "duplicate audit action '{raw}' in {AUDIT_LOG_RECORDED_ACTIONS_KEY}"
            )));
        }
        seen[index] = true;
        actions.push(action);
    }

    Ok(actions)
}

/// Whether `key` is a runtime audit setting whose changes must rebuild the
/// precompiled recording scope.
pub fn is_audit_runtime_key(key: &str) -> bool {
    matches!(key, AUDIT_LOG_ENABLED_KEY | AUDIT_LOG_RECORDED_ACTIONS_KEY)
}

fn parse_action_mask_for_runtime(value: &str) -> [u64; AUDIT_ACTION_MASK_WORDS] {
    match parse_recorded_actions_config_value(value) {
        Ok(actions) => actions
            .into_iter()
            .fold(empty_action_mask(), |mut mask, action| {
                set_mask_bit(&mut mask, action);
                mask
            }),
        Err(error) => {
            // Bad runtime config should never block logging; fall back to recording everything.
            tracing::warn!(
                error = %error,
                "invalid audit_log_recorded_actions runtime config; recording all audit actions"
            );
            all_actions_mask()
        }
    }
}

fn parse_string_array(value: &str, value_type: &str) -> Result<Vec<String>> {
    serde_json::from_str::<Vec<String>>(value.trim()).map_err(|err| {
        AsterError::validation_error(format!(
            "{value_type} config must be a JSON array of strings: {err}"
        ))
    })
}

fn empty_action_mask() -> [u64; AUDIT_ACTION_MASK_WORDS] {
    [0; AUDIT_ACTION_MASK_WORDS]
}

fn all_actions_mask() -> [u64; AUDIT_ACTION_MASK_WORDS] {
    let mut mask = empty_action_mask();
    for action in AuditAction::ALL
        .into_iter()
        .filter(|action| !action.is_workspace_mutation())
    {
        set_mask_bit(&mut mask, action);
    }
    mask
}

fn set_mask_bit(mask: &mut [u64; AUDIT_ACTION_MASK_WORDS], action: AuditAction) {
    let index = action.index();
    debug_assert!(index < AuditAction::COUNT);
    let word = index / u64::BITS as usize;
    let bit = index % u64::BITS as usize;
    mask[word] |= 1_u64 << bit;
}

fn mask_contains(mask: [u64; AUDIT_ACTION_MASK_WORDS], action: AuditAction) -> bool {
    let index = action.index();
    debug_assert!(index < AuditAction::COUNT);
    let word = index / u64::BITS as usize;
    let bit = index % u64::BITS as usize;
    (mask[word] & (1_u64 << bit)) != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audit_action_mask_word_count_matches_action_count() {
        assert_eq!(
            AuditAction::COUNT.div_ceil(u64::BITS as usize),
            AUDIT_ACTION_MASK_WORDS
        );
    }

    #[test]
    fn default_recorded_actions_contains_only_system_audit_actions() {
        let actions = parse_recorded_actions_config_value(&default_recorded_actions_value())
            .expect("default audit action scope should parse");
        assert_eq!(
            actions,
            AuditAction::ALL
                .into_iter()
                .filter(|action| !action.is_workspace_mutation())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn full_recorded_actions_json_fallback_matches_serialized_all_actions() {
        let values: Vec<&'static str> = AuditAction::ALL
            .iter()
            .filter(|action| !action.is_workspace_mutation())
            .map(|action| action.as_str())
            .collect();
        assert_eq!(
            full_recorded_actions_json_fallback(),
            serde_json::to_string(&values).expect("all audit actions should serialize")
        );
    }

    /// Verify remote target action selection uses current names.
    #[test]
    fn remote_target_action_selection_uses_current_names() {
        let settings = AuditLogRuntimeSettings::from_raw_values(
            Some("true"),
            Some(r#"["admin_create_remote_storage_target","follower_storage_target_delete"]"#),
        );
        assert!(settings.should_record(AuditAction::AdminCreateRemoteStorageTarget));
        assert!(settings.should_record(AuditAction::FollowerStorageTargetDelete));
        assert!(!settings.should_record(AuditAction::AdminDeleteRemoteStorageTarget));
        assert!(
            normalize_recorded_actions_config_value(r#"["admin_create_remote_ingress_profile"]"#)
                .is_err()
        );
    }

    #[test]
    fn normalize_recorded_actions_rejects_unknown_and_duplicate_values() {
        assert!(normalize_recorded_actions_config_value(r#"["user_login","unknown"]"#).is_err());
        assert!(normalize_recorded_actions_config_value(r#"["file_upload"]"#).is_err());
        assert!(normalize_recorded_actions_config_value(r#"["user_login","user_login"]"#).is_err());
    }

    #[test]
    fn normalize_recorded_actions_preserves_authoritative_order_and_empty_set() {
        assert_eq!(
            normalize_recorded_actions_config_value(r#"["user_login","file_download"]"#).unwrap(),
            r#"["file_download","user_login"]"#
        );
        assert_eq!(normalize_recorded_actions_config_value("[]").unwrap(), "[]");
    }

    #[test]
    fn legacy_workspace_selection_does_not_expand_system_audit_scope() {
        let settings = AuditLogRuntimeSettings::from_raw_values(
            Some("true"),
            Some(r#"["file_upload","user_login"]"#),
        );
        assert!(settings.should_record(AuditAction::UserLogin));
        assert!(!settings.should_record(AuditAction::FileUpload));
        assert!(!settings.should_record(AuditAction::FileDownload));
    }

    #[test]
    fn runtime_settings_use_precompiled_action_mask() {
        let settings = AuditLogRuntimeSettings::from_raw_values(
            Some("true"),
            Some(r#"["user_login","config_update"]"#),
        );
        assert!(settings.should_record(AuditAction::UserLogin));
        assert!(settings.should_record(AuditAction::ConfigUpdate));
        assert!(!settings.should_record(AuditAction::FileDownload));

        let disabled =
            AuditLogRuntimeSettings::from_raw_values(Some("false"), Some(r#"["user_login"]"#));
        assert!(!disabled.should_record(AuditAction::UserLogin));
    }

    #[test]
    fn runtime_settings_preserve_full_scope_missing_mail_actions() {
        let legacy_values: Vec<&'static str> = AuditAction::ALL
            .iter()
            .copied()
            .filter(|action| {
                !matches!(
                    action,
                    AuditAction::MailSend | AuditAction::MailDeliveryFailed
                )
            })
            .map(|action| action.as_str())
            .collect();
        let legacy_raw =
            serde_json::to_string(&legacy_values).expect("legacy values should serialize");

        let settings = AuditLogRuntimeSettings::from_raw_values(Some("true"), Some(&legacy_raw));

        assert!(!settings.should_record(AuditAction::MailSend));
        assert!(!settings.should_record(AuditAction::MailDeliveryFailed));
    }

    #[test]
    fn runtime_settings_do_not_expand_custom_action_scope_missing_mail_actions() {
        let settings = AuditLogRuntimeSettings::from_raw_values(
            Some("true"),
            Some(r#"["user_login","config_update"]"#),
        );

        assert!(settings.should_record(AuditAction::UserLogin));
        assert!(settings.should_record(AuditAction::ConfigUpdate));
        assert!(!settings.should_record(AuditAction::MailSend));
        assert!(!settings.should_record(AuditAction::MailDeliveryFailed));
    }

    #[test]
    fn runtime_settings_do_not_treat_partial_legacy_scope_as_full_scope() {
        let values: Vec<&'static str> = AuditAction::ALL
            .iter()
            .copied()
            .filter(|action| {
                !matches!(
                    action,
                    AuditAction::UserLogin
                        | AuditAction::MailSend
                        | AuditAction::MailDeliveryFailed
                )
            })
            .map(|action| action.as_str())
            .collect();
        let raw = serde_json::to_string(&values).expect("values should serialize");

        let settings = AuditLogRuntimeSettings::from_raw_values(Some("true"), Some(&raw));

        assert!(!settings.should_record(AuditAction::UserLogin));
        assert!(!settings.should_record(AuditAction::MailSend));
        assert!(!settings.should_record(AuditAction::MailDeliveryFailed));
        assert!(!settings.should_record(AuditAction::FileUpload));
    }

    #[test]
    fn runtime_settings_do_not_expand_when_one_new_mail_action_is_selected() {
        let values: Vec<&'static str> = AuditAction::ALL
            .iter()
            .copied()
            .filter(|action| !matches!(action, AuditAction::MailDeliveryFailed))
            .map(|action| action.as_str())
            .collect();
        let raw = serde_json::to_string(&values).expect("values should serialize");

        let settings = AuditLogRuntimeSettings::from_raw_values(Some("true"), Some(&raw));

        assert!(settings.should_record(AuditAction::MailSend));
        assert!(!settings.should_record(AuditAction::MailDeliveryFailed));
    }
}
