use std::collections::BTreeMap;

use serde_json::Value;

use crate::types::{AuditAction, AuditEntityType};

use super::models::{AuditPresentation, AuditPresentationMessage};

pub fn build_audit_presentation(
    action: AuditAction,
    entity_type: AuditEntityType,
    entity_id: Option<i64>,
    entity_name: Option<&str>,
    details: Option<&str>,
) -> Option<AuditPresentation> {
    // Presentation is additive API metadata. Malformed legacy details must degrade to
    // summary/target fallback instead of making the audit entry unreadable.
    let parsed_details = details.and_then(parse_details);
    let summary = Some(summary_message(
        action,
        entity_name,
        parsed_details.as_ref(),
    ));
    let target = match action {
        AuditAction::ServerStart | AuditAction::ServerShutdown => Some(server_target()),
        _ => target_message(entity_type, entity_id, entity_name),
    };
    let detail = detail_message(action, parsed_details.as_ref());

    Some(AuditPresentation {
        summary,
        target,
        detail,
    })
}

fn parse_details(raw: &str) -> Option<Value> {
    serde_json::from_str(raw).ok()
}

fn summary_message(
    action: AuditAction,
    entity_name: Option<&str>,
    details: Option<&Value>,
) -> AuditPresentationMessage {
    let mut params = BTreeMap::new();
    if let Some(name) = entity_name {
        params.insert("name".to_string(), Value::String(name.to_string()));
    }

    match action {
        AuditAction::ConfigUpdate | AuditAction::AdminDeleteConfig => {
            if let Some(name) = entity_name {
                params.insert("key".to_string(), Value::String(name.to_string()));
            }
        }
        AuditAction::TeamMemberAdd
        | AuditAction::TeamMemberRemove
        | AuditAction::TeamMemberUpdate => {
            copy_string_param(details, &mut params, "member_username");
        }
        _ => {}
    }

    AuditPresentationMessage {
        code: action.as_str().to_string(),
        params,
    }
}

fn target_message(
    entity_type: AuditEntityType,
    entity_id: Option<i64>,
    entity_name: Option<&str>,
) -> Option<AuditPresentationMessage> {
    if entity_id.is_none() && entity_name.is_none() {
        return None;
    }

    let mut params = BTreeMap::new();
    if let Some(id) = entity_id {
        params.insert("id".to_string(), Value::Number(id.into()));
    }
    if let Some(name) = entity_name {
        params.insert("name".to_string(), Value::String(name.to_string()));
    }

    Some(AuditPresentationMessage {
        code: entity_type.as_str().to_string(),
        params,
    })
}

fn server_target() -> AuditPresentationMessage {
    AuditPresentationMessage {
        code: "server".to_string(),
        params: BTreeMap::new(),
    }
}

fn detail_message(
    action: AuditAction,
    details: Option<&Value>,
) -> Option<AuditPresentationMessage> {
    let details = details?;
    let mut params = BTreeMap::new();

    match action {
        AuditAction::ConfigUpdate => {
            copy_param(details, &mut params, "value");
            Some(message("config_value_updated", params))
        }
        AuditAction::ConfigActionExecute => {
            copy_param(details, &mut params, "action");
            copy_param(details, &mut params, "target_email");
            Some(message("config_action_executed", params))
        }
        AuditAction::MailSend => {
            copy_param(details, &mut params, "to_address");
            copy_param(details, &mut params, "template_code");
            copy_param(details, &mut params, "outbox_id");
            Some(message("mail_sent", params))
        }
        AuditAction::MailDeliveryFailed => {
            copy_param(details, &mut params, "to_address");
            copy_param(details, &mut params, "template_code");
            copy_param(details, &mut params, "outbox_id");
            copy_param(details, &mut params, "attempt_count");
            copy_param(details, &mut params, "error");
            Some(message("mail_delivery_failed", params))
        }
        AuditAction::TeamMemberAdd => {
            copy_param(details, &mut params, "member_user_id");
            copy_param(details, &mut params, "member_username");
            copy_param(details, &mut params, "role");
            Some(message("team_member_added", params))
        }
        AuditAction::TeamMemberUpdate => {
            copy_param(details, &mut params, "member_user_id");
            copy_param(details, &mut params, "member_username");
            copy_param(details, &mut params, "previous_role");
            copy_param(details, &mut params, "next_role");
            Some(message("team_member_updated", params))
        }
        AuditAction::TeamMemberRemove => {
            copy_param(details, &mut params, "member_user_id");
            copy_param(details, &mut params, "member_username");
            copy_param(details, &mut params, "removed_role");
            Some(message("team_member_removed", params))
        }
        AuditAction::AdminCleanupExpiredLocks => {
            copy_param(details, &mut params, "removed");
            Some(message("locks_cleanup_finished", params))
        }
        AuditAction::AdminCleanupTasks => {
            copy_param(details, &mut params, "removed");
            copy_param(details, &mut params, "finished_before");
            copy_param(details, &mut params, "kind");
            copy_param(details, &mut params, "status");
            Some(message("tasks_cleanup_finished", params))
        }
        AuditAction::TaskRetry => {
            copy_param(details, &mut params, "kind");
            copy_param(details, &mut params, "previous_attempt_count");
            Some(message("task_retry_scheduled", params))
        }
        AuditAction::FileUploadCancel => {
            copy_param(details, &mut params, "upload_id");
            Some(message("upload_cancelled", params))
        }
        AuditAction::FileDirectLinkCreate | AuditAction::FilePreviewLinkCreate => {
            copy_param(details, &mut params, "source");
            copy_param(details, &mut params, "app_key");
            Some(message("file_access_token_created", params))
        }
        AuditAction::FileVersionRestore | AuditAction::FileVersionDelete => {
            copy_param(details, &mut params, "version_id");
            Some(message("file_version_changed", params))
        }
        AuditAction::FolderPolicyChange => {
            copy_param(details, &mut params, "previous_policy_id");
            copy_param(details, &mut params, "policy_id");
            Some(message("folder_policy_changed", params))
        }
        AuditAction::BatchDelete => {
            copy_param(details, &mut params, "succeeded");
            copy_param(details, &mut params, "failed");
            Some(message("batch_delete_finished", params))
        }
        AuditAction::BatchCopy | AuditAction::BatchMove => {
            copy_param(details, &mut params, "target_folder_id");
            copy_param(details, &mut params, "succeeded");
            copy_param(details, &mut params, "failed");
            Some(message("batch_transfer_finished", params))
        }
        AuditAction::ShareBatchDelete => {
            copy_param(details, &mut params, "succeeded");
            copy_param(details, &mut params, "failed");
            Some(message("share_batch_delete_finished", params))
        }
        AuditAction::ShareUpdate => {
            copy_param(details, &mut params, "has_password");
            copy_param(details, &mut params, "expires_at");
            copy_param(details, &mut params, "max_downloads");
            Some(message("share_updated", params))
        }
        AuditAction::PropertySet | AuditAction::PropertyDelete => {
            copy_param(details, &mut params, "entity_type");
            copy_param(details, &mut params, "namespace");
            copy_param(details, &mut params, "name");
            Some(message("property_changed", params))
        }
        AuditAction::TrashPurgeAll => {
            copy_param(details, &mut params, "purged");
            Some(message("trash_purge_finished", params))
        }
        AuditAction::ArchiveCompress
        | AuditAction::ArchiveExtract
        | AuditAction::ArchiveDownload => {
            copy_param(details, &mut params, "archive_name");
            copy_param(details, &mut params, "target_folder_id");
            Some(message("archive_selection_created", params))
        }
        AuditAction::OfflineDownload => {
            copy_param(details, &mut params, "source");
            copy_param(details, &mut params, "target_folder_id");
            Some(message("offline_download_created", params))
        }
        _ => None,
    }
}

fn message(code: &str, params: BTreeMap<String, Value>) -> AuditPresentationMessage {
    AuditPresentationMessage {
        code: code.to_string(),
        params,
    }
}

fn copy_string_param(source: Option<&Value>, params: &mut BTreeMap<String, Value>, key: &str) {
    let Some(value) = source
        .and_then(|source| source.get(key))
        .and_then(Value::as_str)
    else {
        return;
    };
    params.insert(key.to_string(), Value::String(value.to_string()));
}

fn copy_param(source: &Value, params: &mut BTreeMap<String, Value>, key: &str) {
    let Some(value) = source.get(key) else {
        return;
    };
    if value.is_null() {
        return;
    }
    params.insert(key.to_string(), value.clone());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presentation_includes_config_key_and_value_detail() {
        let presentation = build_audit_presentation(
            AuditAction::ConfigUpdate,
            AuditEntityType::SystemConfig,
            Some(42),
            Some("audit_log_recorded_actions"),
            Some(r#"{"value":"[\"user_login\"]"}"#),
        )
        .expect("presentation should be built");

        assert_eq!(presentation.summary.as_ref().unwrap().code, "config_update");
        assert_eq!(
            presentation.summary.as_ref().unwrap().params.get("key"),
            Some(&Value::String("audit_log_recorded_actions".to_string()))
        );
        assert_eq!(
            presentation.detail.as_ref().unwrap().code,
            "config_value_updated"
        );
    }

    #[test]
    fn presentation_handles_malformed_details_with_safe_fallback_fields() {
        let presentation = build_audit_presentation(
            AuditAction::FileDownload,
            AuditEntityType::File,
            Some(7),
            Some("report.txt"),
            Some("not json"),
        )
        .expect("presentation should be built");

        assert_eq!(presentation.summary.as_ref().unwrap().code, "file_download");
        assert!(presentation.detail.is_none());
        assert_eq!(presentation.target.as_ref().unwrap().code, "file");
    }

    #[test]
    fn presentation_includes_folder_policy_change_detail() {
        let presentation = build_audit_presentation(
            AuditAction::FolderPolicyChange,
            AuditEntityType::Folder,
            Some(7),
            Some("Projects"),
            Some(r#"{"previous_policy_id":2,"policy_id":5}"#),
        )
        .expect("presentation should be built");

        let detail = presentation.detail.as_ref().unwrap();
        assert_eq!(detail.code, "folder_policy_changed");
        assert_eq!(detail.params.get("previous_policy_id"), Some(&2.into()));
        assert_eq!(detail.params.get("policy_id"), Some(&5.into()));
    }

    #[test]
    fn presentation_uses_server_target_for_server_lifecycle_actions() {
        let presentation = build_audit_presentation(
            AuditAction::ServerStart,
            AuditEntityType::SystemConfig,
            None,
            None,
            None,
        )
        .expect("presentation should be built");

        assert_eq!(presentation.summary.as_ref().unwrap().code, "server_start");
        assert_eq!(presentation.target.as_ref().unwrap().code, "server");
        assert!(presentation.detail.is_none());
    }
}
