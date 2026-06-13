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
        AuditAction::AdminCreateUser => {
            copy_params(
                details,
                &mut params,
                &[
                    "email",
                    "email_verified",
                    "role",
                    "status",
                    "must_change_password",
                    "temporary_password_generated",
                    "storage_quota",
                    "policy_group_id",
                ],
            );
            Some(message("admin_user_created_snapshot", params))
        }
        AuditAction::AdminUpdateUser => {
            copy_params(
                details,
                &mut params,
                &[
                    "changed_fields",
                    "email_verified",
                    "role",
                    "status",
                    "must_change_password",
                    "storage_quota",
                    "policy_group_id",
                    "previous_email_verified",
                    "previous_role",
                    "previous_status",
                    "previous_must_change_password",
                    "previous_storage_quota",
                    "previous_policy_group_id",
                ],
            );
            Some(message("admin_user_updated_diff", params))
        }
        AuditAction::AdminForceDeleteUser => {
            copy_params(
                details,
                &mut params,
                &[
                    "file_count",
                    "folder_count",
                    "share_count",
                    "webdav_account_count",
                    "upload_session_count",
                    "lock_count",
                ],
            );
            Some(message("admin_force_delete_user_finished", params))
        }
        AuditAction::AdminCreatePolicy
        | AuditAction::AdminUpdatePolicy
        | AuditAction::AdminDeletePolicy => {
            copy_params(
                details,
                &mut params,
                &[
                    "driver_type",
                    "remote_node_id",
                    "max_file_size",
                    "chunk_size",
                    "is_default",
                ],
            );
            Some(message("storage_policy_snapshot", params))
        }
        AuditAction::AdminCreatePolicyGroup
        | AuditAction::AdminUpdatePolicyGroup
        | AuditAction::AdminDeletePolicyGroup => {
            copy_params(
                details,
                &mut params,
                &["is_default", "is_enabled", "item_count"],
            );
            Some(message("policy_group_snapshot", params))
        }
        AuditAction::AdminMigratePolicyGroupUsers => {
            copy_params(
                details,
                &mut params,
                &[
                    "source_group_id",
                    "source_group_name",
                    "target_group_id",
                    "target_group_name",
                    "affected_users",
                    "affected_teams",
                    "migrated_assignments",
                ],
            );
            Some(message("policy_group_migration_finished", params))
        }
        AuditAction::AdminCreateTeam
        | AuditAction::AdminUpdateTeam
        | AuditAction::AdminArchiveTeam
        | AuditAction::AdminRestoreTeam
        | AuditAction::TeamCreate
        | AuditAction::TeamUpdate
        | AuditAction::TeamArchive
        | AuditAction::TeamRestore => {
            copy_params(
                details,
                &mut params,
                &[
                    "description",
                    "member_count",
                    "storage_quota",
                    "policy_group_id",
                    "archived_at",
                    "actor_role",
                ],
            );
            Some(message("team_snapshot", params))
        }
        AuditAction::TeamCleanupExpired => {
            copy_params(details, &mut params, &["archived_at", "retention_days"]);
            Some(message("team_cleanup_expired_finished", params))
        }
        AuditAction::TeamMemberAdd => {
            copy_param(details, &mut params, "member_user_id");
            copy_param(details, &mut params, "member_username");
            copy_param(details, &mut params, "role");
            copy_param(details, &mut params, "actor_role");
            Some(message("team_member_added", params))
        }
        AuditAction::TeamMemberUpdate => {
            copy_param(details, &mut params, "member_user_id");
            copy_param(details, &mut params, "member_username");
            copy_param(details, &mut params, "previous_role");
            copy_param(details, &mut params, "next_role");
            copy_param(details, &mut params, "actor_role");
            Some(message("team_member_updated", params))
        }
        AuditAction::TeamMemberRemove => {
            copy_param(details, &mut params, "member_user_id");
            copy_param(details, &mut params, "member_username");
            copy_param(details, &mut params, "removed_role");
            copy_param(details, &mut params, "actor_role");
            Some(message("team_member_removed", params))
        }
        AuditAction::UserRevokeSession => {
            copy_params(details, &mut params, &["session_id", "revoked_current"]);
            Some(message("auth_session_revoked", params))
        }
        AuditAction::UserRevokeOtherSessions => {
            copy_params(
                details,
                &mut params,
                &["session_id", "removed", "revoked_current"],
            );
            Some(message("other_auth_sessions_revoked", params))
        }
        AuditAction::UserUpdateProfile => {
            copy_param(details, &mut params, "display_name");
            Some(message("user_profile_updated", params))
        }
        AuditAction::UserUpdatePreferences => {
            copy_params(
                details,
                &mut params,
                &[
                    "changed_fields",
                    "custom_upsert_count",
                    "custom_remove_count",
                ],
            );
            Some(message("user_preferences_updated", params))
        }
        AuditAction::UserUploadAvatar => {
            copy_params(details, &mut params, &["source", "version"]);
            Some(message("user_avatar_uploaded", params))
        }
        AuditAction::UserSetAvatarSource => {
            copy_param(details, &mut params, "source");
            Some(message("user_avatar_source_changed", params))
        }
        AuditAction::UserLogin => {
            copy_param(details, &mut params, "mfa_required");
            if details
                .get("mfa_required")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                Some(message("user_login_mfa_required", params))
            } else {
                copy_param(details, &mut params, "password_change_required");
                Some(message("user_login_completed", params))
            }
        }
        AuditAction::UserMfaChallengeSuccess => {
            copy_params(
                details,
                &mut params,
                &[
                    "method",
                    "flow_id",
                    "attempt_count",
                    "password_change_required",
                ],
            );
            Some(message("mfa_challenge_completed", params))
        }
        AuditAction::UserMfaChallengeFailed => {
            copy_params(
                details,
                &mut params,
                &["method", "flow_id", "attempt_count", "failure_reason"],
            );
            Some(message("mfa_challenge_failed", params))
        }
        AuditAction::UserMfaEnable
        | AuditAction::UserMfaDisable
        | AuditAction::UserMfaRecoveryCodesRegenerate
        | AuditAction::AdminResetUserMfa => {
            copy_params(
                details,
                &mut params,
                &[
                    "method",
                    "factor_id",
                    "factor_name",
                    "factor_count",
                    "recovery_code_count",
                ],
            );
            Some(message("mfa_management_changed", params))
        }
        AuditAction::UserMfaEmailCodeSend => {
            copy_params(
                details,
                &mut params,
                &["method", "flow_id", "expires_in", "resend_after"],
            );
            Some(message("mfa_email_code_sent", params))
        }
        AuditAction::UserPasskeyRegister | AuditAction::UserPasskeyDelete => {
            copy_params(
                details,
                &mut params,
                &[
                    "passkey_id",
                    "name",
                    "backup_eligible",
                    "backed_up",
                    "sign_count",
                    "last_used_at",
                ],
            );
            Some(message("passkey_snapshot", params))
        }
        AuditAction::UserPasskeyRename => {
            copy_params(
                details,
                &mut params,
                &[
                    "passkey_id",
                    "previous_name",
                    "next_name",
                    "backup_eligible",
                    "backed_up",
                ],
            );
            Some(message("passkey_renamed", params))
        }
        AuditAction::UserPasskeyLogin => {
            copy_params(
                details,
                &mut params,
                &["passkey_id", "name", "password_change_required"],
            );
            Some(message("passkey_login_completed", params))
        }
        AuditAction::AdminCreateRemoteNode
        | AuditAction::AdminUpdateRemoteNode
        | AuditAction::AdminDeleteRemoteNode => {
            copy_params(
                details,
                &mut params,
                &["base_url", "is_enabled", "enrollment_status"],
            );
            Some(message("remote_node_snapshot", params))
        }
        AuditAction::AdminTestRemoteNode => {
            copy_params(
                details,
                &mut params,
                &[
                    "base_url",
                    "is_enabled",
                    "enrollment_status",
                    "success",
                    "protocol_version",
                    "server_version",
                    "supports_list",
                    "supports_range_read",
                    "supports_stream_upload",
                    "supports_capacity",
                ],
            );
            let code = if details.get("success").is_some() {
                "remote_node_connection_tested"
            } else {
                "remote_node_snapshot"
            };
            Some(message(code, params))
        }
        AuditAction::AdminCreateRemoteNodeEnrollmentToken => {
            copy_param(details, &mut params, "expires_at");
            Some(message("remote_node_enrollment_token_created", params))
        }
        AuditAction::AdminCreateRemoteIngressProfile
        | AuditAction::AdminUpdateRemoteIngressProfile
        | AuditAction::AdminDeleteRemoteIngressProfile => {
            copy_params(
                details,
                &mut params,
                &["profile_key", "driver_type", "is_default"],
            );
            Some(message("remote_ingress_profile_snapshot", params))
        }
        AuditAction::AdminCreateExternalAuthProvider
        | AuditAction::AdminUpdateExternalAuthProvider
        | AuditAction::AdminDeleteExternalAuthProvider => {
            copy_params(
                details,
                &mut params,
                &[
                    "key",
                    "issuer_url",
                    "enabled",
                    "auto_provision_enabled",
                    "auto_link_verified_email_enabled",
                    "require_email_verified",
                ],
            );
            Some(message("external_auth_provider_snapshot", params))
        }
        AuditAction::AdminTestExternalAuthProvider => {
            copy_params(
                details,
                &mut params,
                &["provider_kind", "key", "success", "issuer_url", "enabled"],
            );
            Some(message("external_auth_provider_tested", params))
        }
        AuditAction::UserExternalAuthLogin | AuditAction::UserExternalAuthLink => {
            copy_params(
                details,
                &mut params,
                &[
                    "provider_key",
                    "issuer",
                    "subject",
                    "linked",
                    "auto_provisioned",
                ],
            );
            let code = match action {
                AuditAction::UserExternalAuthLink => "external_auth_linked",
                _ => "external_auth_login_completed",
            };
            Some(message(code, params))
        }
        AuditAction::UserExternalAuthUnlink => {
            copy_params(details, &mut params, &["provider_key", "issuer", "subject"]);
            Some(message("external_auth_unlinked", params))
        }
        AuditAction::UserUpdateWopiInfo => {
            copy_params(
                details,
                &mut params,
                &["file_id", "app_key", "user_info_len"],
            );
            Some(message("wopi_user_info_updated", params))
        }
        AuditAction::WebdavAccountToggle => {
            copy_param(details, &mut params, "is_active");
            Some(message("webdav_account_status_changed", params))
        }
        AuditAction::WebdavAccountCreate | AuditAction::WebdavAccountDelete => {
            copy_params(
                details,
                &mut params,
                &["username", "root_folder_id", "is_active"],
            );
            Some(message("webdav_account_changed", params))
        }
        AuditAction::TeamWebdavAccountToggle => {
            copy_params(details, &mut params, &["team_id", "is_active"]);
            Some(message("team_webdav_account_status_changed", params))
        }
        AuditAction::TeamWebdavAccountCreate | AuditAction::TeamWebdavAccountDelete => {
            copy_params(
                details,
                &mut params,
                &["username", "team_id", "root_folder_id", "is_active"],
            );
            Some(message("team_webdav_account_changed", params))
        }
        AuditAction::AdminForceUnlock => {
            copy_params(details, &mut params, &["entity_type", "entity_id"]);
            Some(message("resource_force_unlocked", params))
        }
        AuditAction::AdminCreateBlobMaintenanceTask => {
            copy_param(details, &mut params, "action");
            if let Some(count) = array_len_value(details, "blob_ids") {
                params.insert("blob_count".to_string(), count);
            }
            Some(message("blob_maintenance_task_created", params))
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
        AuditAction::FileDelete
        | AuditAction::FileDownload
        | AuditAction::FileCreate
        | AuditAction::FileEdit
        | AuditAction::FileUpload
        | AuditAction::FileLock
        | AuditAction::FileUnlock
        | AuditAction::FileRestore
        | AuditAction::FilePurge => {
            copy_params(details, &mut params, &["folder_id", "path", "team_id"]);
            Some(message("file_location", params))
        }
        AuditAction::FileMove | AuditAction::FileRename | AuditAction::FileCopy => {
            copy_params(
                details,
                &mut params,
                &[
                    "source_folder_id",
                    "source_path",
                    "target_folder_id",
                    "target_path",
                    "previous_name",
                    "next_name",
                    "team_id",
                ],
            );
            Some(message("file_transfer", params))
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
        AuditAction::FolderCreate
        | AuditAction::FolderDelete
        | AuditAction::FolderLock
        | AuditAction::FolderUnlock
        | AuditAction::FolderRestore
        | AuditAction::FolderPurge => {
            copy_params(details, &mut params, &["parent_id", "path", "team_id"]);
            Some(message("folder_location", params))
        }
        AuditAction::FolderMove | AuditAction::FolderRename | AuditAction::FolderCopy => {
            copy_params(
                details,
                &mut params,
                &[
                    "source_parent_id",
                    "source_path",
                    "target_parent_id",
                    "target_path",
                    "previous_name",
                    "next_name",
                    "team_id",
                ],
            );
            Some(message("folder_transfer", params))
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
        AuditAction::ShareCreate | AuditAction::ShareDelete | AuditAction::AdminDeleteShare => {
            copy_params(
                details,
                &mut params,
                &[
                    "token",
                    "target_type",
                    "target_id",
                    "team_id",
                    "has_password",
                    "expires_at",
                    "max_downloads",
                ],
            );
            let code = match action {
                AuditAction::ShareCreate => "share_created",
                _ => "share_deleted",
            };
            Some(message(code, params))
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
        AuditAction::TagCreate | AuditAction::TagDelete => {
            copy_params(details, &mut params, &["name", "color", "team_id"]);
            Some(message("tag_snapshot", params))
        }
        AuditAction::TagUpdate => {
            copy_params(
                details,
                &mut params,
                &[
                    "name",
                    "color",
                    "previous_name",
                    "next_name",
                    "previous_color",
                    "next_color",
                    "team_id",
                ],
            );
            Some(message("tag_updated", params))
        }
        AuditAction::TagAttach | AuditAction::TagDetach => {
            copy_params(
                details,
                &mut params,
                &[
                    "operation",
                    "tag_id",
                    "tag_name",
                    "tag_color",
                    "entity_type",
                    "entity_id",
                    "file_count",
                    "folder_count",
                    "tag_count",
                    "team_id",
                ],
            );
            let code = match details.get("operation").and_then(Value::as_str) {
                Some("attach") | Some("detach") => "tag_assignment_changed",
                Some("replace") => "tag_assignment_replaced",
                Some("batch_attach") | Some("batch_detach") => "tag_assignment_batch_changed",
                _ => "tag_assignment_changed",
            };
            Some(message(code, params))
        }
        AuditAction::TrashPurgeAll => {
            copy_params(details, &mut params, &["phase", "purged", "team_id"]);
            let code = match details.get("phase").and_then(Value::as_str) {
                Some("requested") => "trash_purge_requested",
                _ => "trash_purge_finished",
            };
            Some(message(code, params))
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
        AuditAction::RemoteEnrollmentRedeem | AuditAction::RemoteEnrollmentAck => {
            copy_params(
                details,
                &mut params,
                &["phase", "remote_node_id", "remote_node_name", "is_enabled"],
            );
            Some(message("remote_enrollment_changed", params))
        }
        AuditAction::AdminCreateInvitation | AuditAction::AdminRevokeInvitation => {
            copy_params(
                details,
                &mut params,
                &[
                    "email",
                    "status",
                    "invited_by",
                    "accepted_user_id",
                    "expires_at",
                    "mail_queued",
                ],
            );
            Some(message("invitation_snapshot", params))
        }
        AuditAction::FollowerBindingSync => {
            copy_params(details, &mut params, &["binding_id", "name", "is_enabled"]);
            Some(message("follower_binding_synced", params))
        }
        AuditAction::FollowerObjectRead
        | AuditAction::FollowerObjectWrite
        | AuditAction::FollowerObjectDelete
        | AuditAction::FollowerObjectCompose => {
            copy_params(
                details,
                &mut params,
                &[
                    "binding_id",
                    "object_key",
                    "storage_path",
                    "size",
                    "bytes_written",
                    "partial",
                    "parts",
                ],
            );
            Some(message("follower_object_changed", params))
        }
        AuditAction::FollowerIngressProfileCreate
        | AuditAction::FollowerIngressProfileUpdate
        | AuditAction::FollowerIngressProfileDelete => {
            copy_params(
                details,
                &mut params,
                &["binding_id", "profile_key", "driver_type", "is_default"],
            );
            Some(message("follower_ingress_profile_changed", params))
        }
        AuditAction::AdminRevokeUserSessions
        | AuditAction::AdminResetUserPassword
        | AuditAction::AdminDeleteConfig
        | AuditAction::FileWopiOpen
        | AuditAction::SystemSetup
        | AuditAction::ServerStart
        | AuditAction::ServerShutdown
        | AuditAction::UserChangePassword
        | AuditAction::UserConfirmPasswordReset
        | AuditAction::UserConfirmEmailChange
        | AuditAction::UserConfirmRegistration
        | AuditAction::UserLogout
        | AuditAction::UserRefreshTokenReuseDetected
        | AuditAction::UserRequestEmailChange
        | AuditAction::UserRequestPasswordReset
        | AuditAction::UserRegister
        | AuditAction::UserResendEmailChange
        | AuditAction::UserResendRegistration => None,
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

fn copy_params(source: &Value, params: &mut BTreeMap<String, Value>, keys: &[&str]) {
    for key in keys {
        copy_param(source, params, key);
    }
}

fn array_len_value(source: &Value, key: &str) -> Option<Value> {
    let len = source.get(key)?.as_array()?.len();
    Some(Value::Number(u64::try_from(len).ok()?.into()))
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
    fn presentation_includes_file_location_detail() {
        let presentation = build_audit_presentation(
            AuditAction::FileDelete,
            AuditEntityType::File,
            Some(7),
            Some("report.txt"),
            Some(r#"{"folder_id":3,"path":"/Projects/report.txt"}"#),
        )
        .expect("presentation should be built");

        let detail = presentation.detail.as_ref().unwrap();
        assert_eq!(detail.code, "file_location");
        assert_eq!(
            detail.params.get("path"),
            Some(&Value::String("/Projects/report.txt".to_string()))
        );
        assert_eq!(detail.params.get("folder_id"), Some(&3.into()));
    }

    #[test]
    fn presentation_includes_file_transfer_detail() {
        let presentation = build_audit_presentation(
            AuditAction::FileMove,
            AuditEntityType::File,
            Some(7),
            Some("report.txt"),
            Some(
                r#"{"source_folder_id":2,"source_path":"/Inbox/report.txt","target_folder_id":3,"target_path":"/Projects/report.txt","previous_name":"report.txt","next_name":"report.txt"}"#,
            ),
        )
        .expect("presentation should be built");

        let detail = presentation.detail.as_ref().unwrap();
        assert_eq!(detail.code, "file_transfer");
        assert_eq!(
            detail.params.get("source_path"),
            Some(&Value::String("/Inbox/report.txt".to_string()))
        );
        assert_eq!(
            detail.params.get("target_path"),
            Some(&Value::String("/Projects/report.txt".to_string()))
        );
    }

    #[test]
    fn presentation_includes_admin_user_snapshot_detail() {
        let presentation = build_audit_presentation(
            AuditAction::AdminCreateUser,
            AuditEntityType::User,
            Some(7),
            Some("alice"),
            Some(
                r#"{"email":"alice@example.com","email_verified":true,"role":"admin","status":"active","must_change_password":false,"temporary_password_generated":true,"storage_quota":1073741824,"policy_group_id":3}"#,
            ),
        )
        .expect("presentation should be built");

        let detail = presentation.detail.as_ref().unwrap();
        assert_eq!(detail.code, "admin_user_created_snapshot");
        assert_eq!(
            detail.params.get("email"),
            Some(&Value::String("alice@example.com".to_string()))
        );
        assert_eq!(detail.params.get("policy_group_id"), Some(&3.into()));
    }

    #[test]
    fn presentation_includes_admin_user_update_diff_detail() {
        let presentation = build_audit_presentation(
            AuditAction::AdminUpdateUser,
            AuditEntityType::User,
            Some(7),
            Some("alice"),
            Some(
                r#"{"changed_fields":["role","storage_quota"],"previous_role":"user","role":"admin","previous_status":"active","status":"active","previous_email_verified":false,"email_verified":false,"previous_must_change_password":false,"must_change_password":false,"previous_storage_quota":1024,"storage_quota":2048}"#,
            ),
        )
        .expect("presentation should be built");

        let detail = presentation.detail.as_ref().unwrap();
        assert_eq!(detail.code, "admin_user_updated_diff");
        assert_eq!(
            detail.params.get("previous_role"),
            Some(&Value::String("user".to_string()))
        );
        assert_eq!(
            detail.params.get("role"),
            Some(&Value::String("admin".to_string()))
        );
    }

    #[test]
    fn presentation_includes_policy_group_migration_detail() {
        let presentation = build_audit_presentation(
            AuditAction::AdminMigratePolicyGroupUsers,
            AuditEntityType::PolicyGroup,
            Some(1),
            Some("Default"),
            Some(
                r#"{"source_group_id":1,"source_group_name":"Default","target_group_id":2,"target_group_name":"Archive","affected_users":4,"affected_teams":2,"migrated_assignments":6}"#,
            ),
        )
        .expect("presentation should be built");

        let detail = presentation.detail.as_ref().unwrap();
        assert_eq!(detail.code, "policy_group_migration_finished");
        assert_eq!(
            detail.params.get("target_group_name"),
            Some(&Value::String("Archive".to_string()))
        );
        assert_eq!(detail.params.get("migrated_assignments"), Some(&6.into()));
    }

    #[test]
    fn presentation_includes_external_auth_login_detail() {
        let presentation = build_audit_presentation(
            AuditAction::UserExternalAuthLogin,
            AuditEntityType::ExternalAuthIdentity,
            Some(9),
            Some("oidc"),
            Some(
                r#"{"provider_key":"oidc","issuer":"https://idp.example.com","subject":"sub-1","linked":true,"auto_provisioned":false}"#,
            ),
        )
        .expect("presentation should be built");

        let detail = presentation.detail.as_ref().unwrap();
        assert_eq!(detail.code, "external_auth_login_completed");
        assert_eq!(
            detail.params.get("provider_key"),
            Some(&Value::String("oidc".to_string()))
        );
        assert_eq!(detail.params.get("linked"), Some(&Value::Bool(true)));
    }

    #[test]
    fn presentation_includes_external_auth_link_detail() {
        let presentation = build_audit_presentation(
            AuditAction::UserExternalAuthLink,
            AuditEntityType::ExternalAuthIdentity,
            None,
            Some("oidc"),
            Some(
                r#"{"provider_key":"oidc","issuer":"https://idp.example.com","subject":"sub-1","linked":true,"auto_provisioned":false}"#,
            ),
        )
        .expect("presentation should be built");

        let detail = presentation.detail.as_ref().unwrap();
        assert_eq!(detail.code, "external_auth_linked");
        assert_eq!(
            detail.params.get("subject"),
            Some(&Value::String("sub-1".to_string()))
        );
    }

    #[test]
    fn presentation_includes_preferences_update_detail() {
        let presentation = build_audit_presentation(
            AuditAction::UserUpdatePreferences,
            AuditEntityType::User,
            Some(9),
            Some("alice"),
            Some(
                r#"{"changed_fields":["theme_mode","language"],"custom_upsert_count":2,"custom_remove_count":1}"#,
            ),
        )
        .expect("presentation should be built");

        let detail = presentation.detail.as_ref().unwrap();
        assert_eq!(detail.code, "user_preferences_updated");
        assert_eq!(detail.params.get("custom_upsert_count"), Some(&2.into()));
        assert_eq!(detail.params.get("custom_remove_count"), Some(&1.into()));
    }

    #[test]
    fn presentation_includes_avatar_upload_detail() {
        let presentation = build_audit_presentation(
            AuditAction::UserUploadAvatar,
            AuditEntityType::User,
            Some(9),
            Some("alice"),
            Some(r#"{"source":"upload","version":3}"#),
        )
        .expect("presentation should be built");

        let detail = presentation.detail.as_ref().unwrap();
        assert_eq!(detail.code, "user_avatar_uploaded");
        assert_eq!(
            detail.params.get("source"),
            Some(&Value::String("upload".to_string()))
        );
        assert_eq!(detail.params.get("version"), Some(&3.into()));
    }

    #[test]
    fn presentation_includes_remote_node_param_test_detail() {
        let presentation = build_audit_presentation(
            AuditAction::AdminTestRemoteNode,
            AuditEntityType::RemoteNode,
            None,
            Some("https://follower.example.com"),
            Some(
                r#"{"base_url":"https://follower.example.com","success":true,"protocol_version":"1.0","supports_list":true,"supports_range_read":true,"supports_stream_upload":true,"supports_capacity":false}"#,
            ),
        )
        .expect("presentation should be built");

        let detail = presentation.detail.as_ref().unwrap();
        assert_eq!(detail.code, "remote_node_connection_tested");
        assert_eq!(
            detail.params.get("protocol_version"),
            Some(&Value::String("1.0".to_string()))
        );
    }

    #[test]
    fn presentation_includes_webdav_account_detail() {
        let presentation = build_audit_presentation(
            AuditAction::TeamWebdavAccountCreate,
            AuditEntityType::WebdavAccount,
            Some(9),
            Some("dav-ci"),
            Some(r#"{"username":"dav-ci","team_id":4,"root_folder_id":8,"is_active":true}"#),
        )
        .expect("presentation should be built");

        let detail = presentation.detail.as_ref().unwrap();
        assert_eq!(detail.code, "team_webdav_account_changed");
        assert_eq!(
            detail.params.get("username"),
            Some(&Value::String("dav-ci".to_string()))
        );
        assert_eq!(detail.params.get("team_id"), Some(&4.into()));
        assert_eq!(detail.params.get("root_folder_id"), Some(&8.into()));
    }

    #[test]
    fn presentation_includes_passkey_login_detail() {
        let presentation = build_audit_presentation(
            AuditAction::UserPasskeyLogin,
            AuditEntityType::Passkey,
            Some(9),
            Some("MacBook Touch ID"),
            Some(r#"{"passkey_id":9,"name":"MacBook Touch ID","password_change_required":false}"#),
        )
        .expect("presentation should be built");

        let detail = presentation.detail.as_ref().unwrap();
        assert_eq!(detail.code, "passkey_login_completed");
        assert_eq!(detail.params.get("passkey_id"), Some(&9.into()));
        assert_eq!(
            detail.params.get("name"),
            Some(&Value::String("MacBook Touch ID".to_string()))
        );
    }

    #[test]
    fn presentation_distinguishes_mfa_pending_login_detail() {
        let presentation = build_audit_presentation(
            AuditAction::UserLogin,
            AuditEntityType::AuthSession,
            None,
            Some("alice"),
            Some(r#"{"mfa_required":true,"available_methods":["totp","recovery_code"]}"#),
        )
        .expect("presentation should be built");

        let detail = presentation.detail.as_ref().unwrap();
        assert_eq!(detail.code, "user_login_mfa_required");
        assert_eq!(detail.params.get("mfa_required"), Some(&Value::Bool(true)));
    }

    #[test]
    fn presentation_includes_mfa_failure_detail() {
        let presentation = build_audit_presentation(
            AuditAction::UserMfaChallengeFailed,
            AuditEntityType::MfaFactor,
            Some(9),
            Some("totp"),
            Some(
                r#"{"method":"totp","flow_id":9,"attempt_count":2,"failure_reason":"auth.mfa_code_invalid"}"#,
            ),
        )
        .expect("presentation should be built");

        let detail = presentation.detail.as_ref().unwrap();
        assert_eq!(detail.code, "mfa_challenge_failed");
        assert_eq!(
            detail.params.get("method"),
            Some(&Value::String("totp".to_string()))
        );
        assert_eq!(detail.params.get("attempt_count"), Some(&2.into()));
        assert_eq!(
            detail.params.get("failure_reason"),
            Some(&Value::String("auth.mfa_code_invalid".to_string()))
        );
    }

    #[test]
    fn presentation_includes_mfa_management_detail() {
        let presentation = build_audit_presentation(
            AuditAction::UserMfaEnable,
            AuditEntityType::MfaFactor,
            Some(9),
            Some("Authenticator app"),
            Some(
                r#"{"method":"totp","factor_id":9,"factor_name":"Authenticator app","recovery_code_count":10}"#,
            ),
        )
        .expect("presentation should be built");

        let detail = presentation.detail.as_ref().unwrap();
        assert_eq!(detail.code, "mfa_management_changed");
        assert_eq!(
            detail.params.get("method"),
            Some(&Value::String("totp".to_string()))
        );
        assert_eq!(detail.params.get("recovery_code_count"), Some(&10.into()));
    }

    #[test]
    fn presentation_includes_external_auth_unlink_detail() {
        let presentation = build_audit_presentation(
            AuditAction::UserExternalAuthUnlink,
            AuditEntityType::ExternalAuthIdentity,
            Some(9),
            Some("oidc"),
            Some(r#"{"provider_key":"oidc","issuer":"https://idp.example.com","subject":"sub-1"}"#),
        )
        .expect("presentation should be built");

        let detail = presentation.detail.as_ref().unwrap();
        assert_eq!(detail.code, "external_auth_unlinked");
        assert_eq!(
            detail.params.get("provider_key"),
            Some(&Value::String("oidc".to_string()))
        );
        assert_eq!(
            detail.params.get("subject"),
            Some(&Value::String("sub-1".to_string()))
        );
    }

    #[test]
    fn presentation_includes_share_delete_detail() {
        let presentation = build_audit_presentation(
            AuditAction::ShareDelete,
            AuditEntityType::Share,
            Some(12),
            Some("shr_test"),
            Some(
                r#"{"token":"shr_test","target_type":"file","target_id":44,"team_id":3,"has_password":true,"max_downloads":5}"#,
            ),
        )
        .expect("presentation should be built");

        let detail = presentation.detail.as_ref().unwrap();
        assert_eq!(detail.code, "share_deleted");
        assert_eq!(
            detail.params.get("target_type"),
            Some(&Value::String("file".to_string()))
        );
        assert_eq!(detail.params.get("target_id"), Some(&44.into()));
    }

    #[test]
    fn presentation_includes_share_create_detail() {
        let presentation = build_audit_presentation(
            AuditAction::ShareCreate,
            AuditEntityType::Share,
            Some(12),
            Some("shr_test"),
            Some(
                r#"{"token":"shr_test","target_type":"folder","target_id":44,"has_password":false,"max_downloads":0}"#,
            ),
        )
        .expect("presentation should be built");

        let detail = presentation.detail.as_ref().unwrap();
        assert_eq!(detail.code, "share_created");
        assert_eq!(
            detail.params.get("target_type"),
            Some(&Value::String("folder".to_string()))
        );
    }

    #[test]
    fn presentation_includes_remote_enrollment_detail() {
        let presentation = build_audit_presentation(
            AuditAction::RemoteEnrollmentAck,
            AuditEntityType::RemoteNode,
            Some(8),
            Some("edge-1"),
            Some(
                r#"{"phase":"acked","remote_node_id":8,"remote_node_name":"edge-1","is_enabled":true}"#,
            ),
        )
        .expect("presentation should be built");

        let detail = presentation.detail.as_ref().unwrap();
        assert_eq!(detail.code, "remote_enrollment_changed");
        assert_eq!(
            detail.params.get("phase"),
            Some(&Value::String("acked".to_string()))
        );
    }

    #[test]
    fn presentation_includes_invitation_detail() {
        let presentation = build_audit_presentation(
            AuditAction::AdminCreateInvitation,
            AuditEntityType::Invitation,
            Some(8),
            Some("dev@example.com"),
            Some(
                r#"{"email":"dev@example.com","status":"pending","invited_by":1,"expires_at":"2026-06-14T00:00:00Z","mail_queued":true}"#,
            ),
        )
        .expect("presentation should be built");

        let detail = presentation.detail.as_ref().unwrap();
        assert_eq!(detail.code, "invitation_snapshot");
        assert_eq!(
            detail.params.get("email"),
            Some(&Value::String("dev@example.com".to_string()))
        );
    }

    #[test]
    fn presentation_includes_follower_object_detail() {
        let presentation = build_audit_presentation(
            AuditAction::FollowerObjectWrite,
            AuditEntityType::File,
            None,
            Some("ab/cd/object"),
            Some(
                r#"{"binding_id":2,"object_key":"ab/cd/object","storage_path":"ab/cd/object","size":128,"bytes_written":128,"partial":false}"#,
            ),
        )
        .expect("presentation should be built");

        let detail = presentation.detail.as_ref().unwrap();
        assert_eq!(detail.code, "follower_object_changed");
        assert_eq!(detail.params.get("binding_id"), Some(&2.into()));
        assert_eq!(
            detail.params.get("object_key"),
            Some(&Value::String("ab/cd/object".to_string()))
        );
    }

    #[test]
    fn presentation_includes_tag_update_detail() {
        let presentation = build_audit_presentation(
            AuditAction::TagUpdate,
            AuditEntityType::Tag,
            Some(5),
            Some("backend"),
            Some(
                r##"{"name":"backend","color":"#3b82f6","previous_name":"api","next_name":"backend","previous_color":"#ef4444","next_color":"#3b82f6"}"##,
            ),
        )
        .expect("presentation should be built");

        let detail = presentation.detail.as_ref().unwrap();
        assert_eq!(detail.code, "tag_updated");
        assert_eq!(
            detail.params.get("previous_name"),
            Some(&Value::String("api".to_string()))
        );
        assert_eq!(
            detail.params.get("next_color"),
            Some(&Value::String("#3b82f6".to_string()))
        );
    }

    #[test]
    fn presentation_includes_batch_tag_assignment_detail() {
        let presentation = build_audit_presentation(
            AuditAction::TagAttach,
            AuditEntityType::Tag,
            Some(5),
            Some("backend"),
            Some(
                r#"{"operation":"batch_attach","tag_id":5,"tag_name":"backend","file_count":3,"folder_count":1}"#,
            ),
        )
        .expect("presentation should be built");

        let detail = presentation.detail.as_ref().unwrap();
        assert_eq!(detail.code, "tag_assignment_batch_changed");
        assert_eq!(detail.params.get("file_count"), Some(&3.into()));
        assert_eq!(detail.params.get("folder_count"), Some(&1.into()));
    }

    #[test]
    fn presentation_distinguishes_requested_trash_purge_detail() {
        let presentation = build_audit_presentation(
            AuditAction::TrashPurgeAll,
            AuditEntityType::Trash,
            Some(12),
            Some("Empty trash"),
            Some(r#"{"phase":"requested","team_id":4}"#),
        )
        .expect("presentation should be built");

        let detail = presentation.detail.as_ref().unwrap();
        assert_eq!(detail.code, "trash_purge_requested");
        assert_eq!(
            detail.params.get("phase"),
            Some(&Value::String("requested".to_string()))
        );
        assert_eq!(detail.params.get("team_id"), Some(&4.into()));
    }

    #[test]
    fn presentation_counts_blob_ids_for_blob_maintenance_detail() {
        let presentation = build_audit_presentation(
            AuditAction::AdminCreateBlobMaintenanceTask,
            AuditEntityType::Task,
            Some(10),
            Some("blob maintenance"),
            Some(r#"{"action":"verify","blob_ids":[1,2,3]}"#),
        )
        .expect("presentation should be built");

        let detail = presentation.detail.as_ref().unwrap();
        assert_eq!(detail.code, "blob_maintenance_task_created");
        assert_eq!(
            detail.params.get("action"),
            Some(&Value::String("verify".to_string()))
        );
        assert_eq!(detail.params.get("blob_count"), Some(&3.into()));
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
