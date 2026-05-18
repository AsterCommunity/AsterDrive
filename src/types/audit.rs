use sea_orm::entity::prelude::*;
use serde::de::{self, Visitor};
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use super::EntityType;

macro_rules! define_audit_entity_type {
    ($($variant:ident => $name:literal),+ $(,)?) => {
        /// 审计日志实体类型
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
        #[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
        #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(rename_all = "snake_case"))]
        pub enum AuditEntityType {
            $(
                #[serde(rename = $name)]
                $variant,
            )+
        }

        impl AuditEntityType {
            pub const COUNT: usize = <[()]>::len(&[$(define_audit_entity_type!(@unit $variant)),+]);
            pub const ALL: [Self; Self::COUNT] = [$(Self::$variant,)+];

            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $name,)+
                }
            }

            pub fn from_str_name(value: &str) -> Option<Self> {
                match value {
                    $($name => Some(Self::$variant),)+
                    _ => None,
                }
            }

            pub const fn from_entity_type(entity_type: EntityType) -> Self {
                match entity_type {
                    EntityType::File => Self::File,
                    EntityType::Folder => Self::Folder,
                }
            }
        }

        const AUDIT_ENTITY_TYPE_NAMES: &'static [&'static str] = &[$($name,)+];
    };
    (@unit $variant:ident) => {
        ()
    };
}

define_audit_entity_type! {
    AuthSession => "auth_session",
    Batch => "batch",
    ExternalAuthIdentity => "external_auth_identity",
    ExternalAuthProvider => "external_auth_provider",
    File => "file",
    Folder => "folder",
    Passkey => "passkey",
    PolicyGroup => "policy_group",
    RemoteIngressProfile => "remote_ingress_profile",
    RemoteNode => "remote_node",
    ResourceLock => "resource_lock",
    Share => "share",
    StoragePolicy => "storage_policy",
    StreamTicket => "stream_ticket",
    SystemConfig => "system_config",
    Task => "task",
    Team => "team",
    Trash => "trash",
    UploadSession => "upload_session",
    User => "user",
    WebdavAccount => "webdav_account",
}

impl AsRef<str> for AuditEntityType {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for AuditEntityType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for AuditEntityType {
    type Err = ParseAuditEntityTypeError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::from_str_name(value).ok_or(ParseAuditEntityTypeError)
    }
}

impl<'de> Deserialize<'de> for AuditEntityType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct AuditEntityTypeVisitor;

        impl Visitor<'_> for AuditEntityTypeVisitor {
            type Value = AuditEntityType;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a supported audit entity type")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                AuditEntityType::from_str_name(value)
                    .ok_or_else(|| E::unknown_variant(value, AUDIT_ENTITY_TYPE_NAMES))
            }
        }

        deserializer.deserialize_str(AuditEntityTypeVisitor)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ParseAuditEntityTypeError;

impl fmt::Display for ParseAuditEntityTypeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid audit entity type")
    }
}

impl std::error::Error for ParseAuditEntityTypeError {}

#[cfg(test)]
mod tests {
    use super::{AUDIT_ENTITY_TYPE_NAMES, AuditEntityType};

    #[test]
    fn audit_entity_type_round_trips_string_names() {
        let names: Vec<_> = AuditEntityType::ALL
            .iter()
            .map(|entity_type| entity_type.as_str())
            .collect();
        assert_eq!(AUDIT_ENTITY_TYPE_NAMES, names.as_slice());

        for entity_type in AuditEntityType::ALL {
            let name = entity_type.as_str();

            assert_eq!(entity_type.as_ref(), name);
            assert_eq!(entity_type.to_string(), name);
            assert_eq!(AuditEntityType::from_str_name(name), Some(entity_type));
            assert_eq!(
                serde_json::to_value(entity_type).expect("audit entity type serializes"),
                serde_json::json!(name)
            );
            assert_eq!(
                serde_json::from_value::<AuditEntityType>(serde_json::json!(name))
                    .expect("audit entity type deserializes"),
                entity_type
            );
        }

        assert_eq!(AuditEntityType::from_str_name("unknown"), None);
        assert!(serde_json::from_value::<AuditEntityType>(serde_json::json!("unknown")).is_err());
    }
}

/// 审计日志动作
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(64))")]
#[serde(rename_all = "snake_case")]
pub enum AuditAction {
    #[sea_orm(string_value = "admin_create_user")]
    AdminCreateUser,
    #[sea_orm(string_value = "admin_force_delete_user")]
    AdminForceDeleteUser,
    #[sea_orm(string_value = "admin_create_team")]
    AdminCreateTeam,
    #[sea_orm(string_value = "admin_create_policy_group")]
    AdminCreatePolicyGroup,
    #[sea_orm(string_value = "admin_archive_team")]
    AdminArchiveTeam,
    #[sea_orm(string_value = "admin_restore_team")]
    AdminRestoreTeam,
    #[sea_orm(string_value = "admin_revoke_user_sessions")]
    AdminRevokeUserSessions,
    #[sea_orm(string_value = "admin_reset_user_password")]
    AdminResetUserPassword,
    #[sea_orm(string_value = "admin_update_team")]
    AdminUpdateTeam,
    #[sea_orm(string_value = "admin_update_user")]
    AdminUpdateUser,
    #[sea_orm(string_value = "admin_delete_policy_group")]
    AdminDeletePolicyGroup,
    #[sea_orm(string_value = "admin_migrate_policy_group_users")]
    AdminMigratePolicyGroupUsers,
    #[sea_orm(string_value = "admin_update_policy_group")]
    AdminUpdatePolicyGroup,
    #[sea_orm(string_value = "admin_create_policy")]
    AdminCreatePolicy,
    #[sea_orm(string_value = "admin_update_policy")]
    AdminUpdatePolicy,
    #[sea_orm(string_value = "admin_delete_policy")]
    AdminDeletePolicy,
    #[sea_orm(string_value = "admin_delete_config")]
    AdminDeleteConfig,
    #[sea_orm(string_value = "admin_delete_share")]
    AdminDeleteShare,
    #[sea_orm(string_value = "admin_force_unlock")]
    AdminForceUnlock,
    #[sea_orm(string_value = "admin_cleanup_expired_locks")]
    AdminCleanupExpiredLocks,
    #[sea_orm(string_value = "admin_cleanup_tasks")]
    AdminCleanupTasks,
    #[sea_orm(string_value = "admin_create_remote_node")]
    AdminCreateRemoteNode,
    #[sea_orm(string_value = "admin_update_remote_node")]
    AdminUpdateRemoteNode,
    #[sea_orm(string_value = "admin_delete_remote_node")]
    AdminDeleteRemoteNode,
    #[sea_orm(string_value = "admin_test_remote_node")]
    AdminTestRemoteNode,
    #[sea_orm(string_value = "admin_create_remote_node_enrollment_token")]
    AdminCreateRemoteNodeEnrollmentToken,
    #[sea_orm(string_value = "admin_create_remote_ingress_profile")]
    AdminCreateRemoteIngressProfile,
    #[sea_orm(string_value = "admin_update_remote_ingress_profile")]
    AdminUpdateRemoteIngressProfile,
    #[sea_orm(string_value = "admin_delete_remote_ingress_profile")]
    AdminDeleteRemoteIngressProfile,
    #[sea_orm(string_value = "admin_create_external_auth_provider")]
    AdminCreateExternalAuthProvider,
    #[sea_orm(string_value = "admin_update_external_auth_provider")]
    AdminUpdateExternalAuthProvider,
    #[sea_orm(string_value = "admin_delete_external_auth_provider")]
    AdminDeleteExternalAuthProvider,
    #[sea_orm(string_value = "admin_test_external_auth_provider")]
    AdminTestExternalAuthProvider,
    #[sea_orm(string_value = "batch_copy")]
    BatchCopy,
    #[sea_orm(string_value = "batch_delete")]
    BatchDelete,
    #[sea_orm(string_value = "batch_move")]
    BatchMove,
    #[sea_orm(string_value = "config_action_execute")]
    ConfigActionExecute,
    #[sea_orm(string_value = "config_update")]
    ConfigUpdate,
    #[sea_orm(string_value = "file_copy")]
    FileCopy,
    #[sea_orm(string_value = "file_create")]
    FileCreate,
    #[sea_orm(string_value = "file_delete")]
    FileDelete,
    #[sea_orm(string_value = "file_download")]
    FileDownload,
    #[sea_orm(string_value = "file_direct_link_create")]
    FileDirectLinkCreate,
    #[sea_orm(string_value = "file_edit")]
    FileEdit,
    #[sea_orm(string_value = "file_move")]
    FileMove,
    #[sea_orm(string_value = "file_rename")]
    FileRename,
    #[sea_orm(string_value = "file_upload")]
    FileUpload,
    #[sea_orm(string_value = "file_preview_link_create")]
    FilePreviewLinkCreate,
    #[sea_orm(string_value = "file_wopi_open")]
    FileWopiOpen,
    #[sea_orm(string_value = "file_upload_cancel")]
    FileUploadCancel,
    #[sea_orm(string_value = "file_restore")]
    FileRestore,
    #[sea_orm(string_value = "file_purge")]
    FilePurge,
    #[sea_orm(string_value = "file_lock")]
    FileLock,
    #[sea_orm(string_value = "file_unlock")]
    FileUnlock,
    #[sea_orm(string_value = "file_version_restore")]
    FileVersionRestore,
    #[sea_orm(string_value = "file_version_delete")]
    FileVersionDelete,
    #[sea_orm(string_value = "folder_copy")]
    FolderCopy,
    #[sea_orm(string_value = "folder_create")]
    FolderCreate,
    #[sea_orm(string_value = "folder_delete")]
    FolderDelete,
    #[sea_orm(string_value = "folder_move")]
    FolderMove,
    #[sea_orm(string_value = "folder_policy_change")]
    FolderPolicyChange,
    #[sea_orm(string_value = "folder_rename")]
    FolderRename,
    #[sea_orm(string_value = "folder_restore")]
    FolderRestore,
    #[sea_orm(string_value = "folder_purge")]
    FolderPurge,
    #[sea_orm(string_value = "folder_lock")]
    FolderLock,
    #[sea_orm(string_value = "folder_unlock")]
    FolderUnlock,
    #[sea_orm(string_value = "property_set")]
    PropertySet,
    #[sea_orm(string_value = "property_delete")]
    PropertyDelete,
    #[sea_orm(string_value = "share_batch_delete")]
    ShareBatchDelete,
    #[sea_orm(string_value = "share_create")]
    ShareCreate,
    #[sea_orm(string_value = "share_delete")]
    ShareDelete,
    #[sea_orm(string_value = "share_update")]
    ShareUpdate,
    #[sea_orm(string_value = "system_setup")]
    SystemSetup,
    #[sea_orm(string_value = "team_archive")]
    TeamArchive,
    #[sea_orm(string_value = "team_cleanup_expired")]
    TeamCleanupExpired,
    #[sea_orm(string_value = "team_create")]
    TeamCreate,
    #[sea_orm(string_value = "team_member_add")]
    TeamMemberAdd,
    #[sea_orm(string_value = "team_member_remove")]
    TeamMemberRemove,
    #[sea_orm(string_value = "team_member_update")]
    TeamMemberUpdate,
    #[sea_orm(string_value = "team_restore")]
    TeamRestore,
    #[sea_orm(string_value = "team_update")]
    TeamUpdate,
    #[sea_orm(string_value = "task_retry")]
    TaskRetry,
    #[sea_orm(string_value = "archive_compress")]
    ArchiveCompress,
    #[sea_orm(string_value = "archive_extract")]
    ArchiveExtract,
    #[sea_orm(string_value = "archive_download")]
    ArchiveDownload,
    #[sea_orm(string_value = "trash_purge_all")]
    TrashPurgeAll,
    #[sea_orm(string_value = "remote_enrollment_redeem")]
    RemoteEnrollmentRedeem,
    #[sea_orm(string_value = "remote_enrollment_ack")]
    RemoteEnrollmentAck,
    #[sea_orm(string_value = "user_revoke_other_sessions")]
    UserRevokeOtherSessions,
    #[sea_orm(string_value = "user_revoke_session")]
    UserRevokeSession,
    #[sea_orm(string_value = "user_update_preferences")]
    UserUpdatePreferences,
    #[sea_orm(string_value = "user_update_profile")]
    UserUpdateProfile,
    #[sea_orm(string_value = "user_upload_avatar")]
    UserUploadAvatar,
    #[sea_orm(string_value = "user_set_avatar_source")]
    UserSetAvatarSource,
    #[sea_orm(string_value = "user_update_wopi_info")]
    UserUpdateWopiInfo,
    #[sea_orm(string_value = "webdav_account_create")]
    WebdavAccountCreate,
    #[sea_orm(string_value = "webdav_account_delete")]
    WebdavAccountDelete,
    #[sea_orm(string_value = "webdav_account_toggle")]
    WebdavAccountToggle,
    #[sea_orm(string_value = "user_change_password")]
    UserChangePassword,
    #[sea_orm(string_value = "user_confirm_password_reset")]
    UserConfirmPasswordReset,
    #[sea_orm(string_value = "user_confirm_email_change")]
    UserConfirmEmailChange,
    #[sea_orm(string_value = "user_confirm_registration")]
    UserConfirmRegistration,
    #[sea_orm(string_value = "user_login")]
    UserLogin,
    #[sea_orm(string_value = "user_logout")]
    UserLogout,
    #[sea_orm(string_value = "user_passkey_delete")]
    UserPasskeyDelete,
    #[sea_orm(string_value = "user_passkey_login")]
    UserPasskeyLogin,
    #[sea_orm(string_value = "user_passkey_register")]
    UserPasskeyRegister,
    #[sea_orm(string_value = "user_passkey_rename")]
    UserPasskeyRename,
    #[sea_orm(string_value = "user_external_auth_login")]
    UserExternalAuthLogin,
    #[sea_orm(string_value = "user_external_auth_link")]
    UserExternalAuthLink,
    #[sea_orm(string_value = "user_external_auth_unlink")]
    UserExternalAuthUnlink,
    #[sea_orm(string_value = "user_refresh_token_reuse_detected")]
    UserRefreshTokenReuseDetected,
    #[sea_orm(string_value = "user_request_email_change")]
    UserRequestEmailChange,
    #[sea_orm(string_value = "user_request_password_reset")]
    UserRequestPasswordReset,
    #[sea_orm(string_value = "user_register")]
    UserRegister,
    #[sea_orm(string_value = "user_resend_email_change")]
    UserResendEmailChange,
    #[sea_orm(string_value = "user_resend_registration")]
    UserResendRegistration,
}

impl AuditAction {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AdminCreateUser => "admin_create_user",
            Self::AdminForceDeleteUser => "admin_force_delete_user",
            Self::AdminCreateTeam => "admin_create_team",
            Self::AdminCreatePolicyGroup => "admin_create_policy_group",
            Self::AdminArchiveTeam => "admin_archive_team",
            Self::AdminRestoreTeam => "admin_restore_team",
            Self::AdminRevokeUserSessions => "admin_revoke_user_sessions",
            Self::AdminResetUserPassword => "admin_reset_user_password",
            Self::AdminUpdateTeam => "admin_update_team",
            Self::AdminUpdateUser => "admin_update_user",
            Self::AdminDeletePolicyGroup => "admin_delete_policy_group",
            Self::AdminMigratePolicyGroupUsers => "admin_migrate_policy_group_users",
            Self::AdminUpdatePolicyGroup => "admin_update_policy_group",
            Self::AdminCreatePolicy => "admin_create_policy",
            Self::AdminUpdatePolicy => "admin_update_policy",
            Self::AdminDeletePolicy => "admin_delete_policy",
            Self::AdminDeleteConfig => "admin_delete_config",
            Self::AdminDeleteShare => "admin_delete_share",
            Self::AdminForceUnlock => "admin_force_unlock",
            Self::AdminCleanupExpiredLocks => "admin_cleanup_expired_locks",
            Self::AdminCleanupTasks => "admin_cleanup_tasks",
            Self::AdminCreateRemoteNode => "admin_create_remote_node",
            Self::AdminUpdateRemoteNode => "admin_update_remote_node",
            Self::AdminDeleteRemoteNode => "admin_delete_remote_node",
            Self::AdminTestRemoteNode => "admin_test_remote_node",
            Self::AdminCreateRemoteNodeEnrollmentToken => {
                "admin_create_remote_node_enrollment_token"
            }
            Self::AdminCreateRemoteIngressProfile => "admin_create_remote_ingress_profile",
            Self::AdminUpdateRemoteIngressProfile => "admin_update_remote_ingress_profile",
            Self::AdminDeleteRemoteIngressProfile => "admin_delete_remote_ingress_profile",
            Self::AdminCreateExternalAuthProvider => "admin_create_external_auth_provider",
            Self::AdminUpdateExternalAuthProvider => "admin_update_external_auth_provider",
            Self::AdminDeleteExternalAuthProvider => "admin_delete_external_auth_provider",
            Self::AdminTestExternalAuthProvider => "admin_test_external_auth_provider",
            Self::BatchCopy => "batch_copy",
            Self::BatchDelete => "batch_delete",
            Self::BatchMove => "batch_move",
            Self::ConfigActionExecute => "config_action_execute",
            Self::ConfigUpdate => "config_update",
            Self::FileCopy => "file_copy",
            Self::FileCreate => "file_create",
            Self::FileDelete => "file_delete",
            Self::FileDownload => "file_download",
            Self::FileDirectLinkCreate => "file_direct_link_create",
            Self::FileEdit => "file_edit",
            Self::FileMove => "file_move",
            Self::FileRename => "file_rename",
            Self::FileUpload => "file_upload",
            Self::FilePreviewLinkCreate => "file_preview_link_create",
            Self::FileWopiOpen => "file_wopi_open",
            Self::FileUploadCancel => "file_upload_cancel",
            Self::FileRestore => "file_restore",
            Self::FilePurge => "file_purge",
            Self::FileLock => "file_lock",
            Self::FileUnlock => "file_unlock",
            Self::FileVersionRestore => "file_version_restore",
            Self::FileVersionDelete => "file_version_delete",
            Self::FolderCopy => "folder_copy",
            Self::FolderCreate => "folder_create",
            Self::FolderDelete => "folder_delete",
            Self::FolderMove => "folder_move",
            Self::FolderPolicyChange => "folder_policy_change",
            Self::FolderRename => "folder_rename",
            Self::FolderRestore => "folder_restore",
            Self::FolderPurge => "folder_purge",
            Self::FolderLock => "folder_lock",
            Self::FolderUnlock => "folder_unlock",
            Self::PropertySet => "property_set",
            Self::PropertyDelete => "property_delete",
            Self::ShareBatchDelete => "share_batch_delete",
            Self::ShareCreate => "share_create",
            Self::ShareDelete => "share_delete",
            Self::ShareUpdate => "share_update",
            Self::SystemSetup => "system_setup",
            Self::TeamArchive => "team_archive",
            Self::TeamCleanupExpired => "team_cleanup_expired",
            Self::TeamCreate => "team_create",
            Self::TeamMemberAdd => "team_member_add",
            Self::TeamMemberRemove => "team_member_remove",
            Self::TeamMemberUpdate => "team_member_update",
            Self::TeamRestore => "team_restore",
            Self::TeamUpdate => "team_update",
            Self::TaskRetry => "task_retry",
            Self::ArchiveCompress => "archive_compress",
            Self::ArchiveExtract => "archive_extract",
            Self::ArchiveDownload => "archive_download",
            Self::TrashPurgeAll => "trash_purge_all",
            Self::RemoteEnrollmentRedeem => "remote_enrollment_redeem",
            Self::RemoteEnrollmentAck => "remote_enrollment_ack",
            Self::UserRevokeOtherSessions => "user_revoke_other_sessions",
            Self::UserRevokeSession => "user_revoke_session",
            Self::UserUpdatePreferences => "user_update_preferences",
            Self::UserUpdateProfile => "user_update_profile",
            Self::UserUploadAvatar => "user_upload_avatar",
            Self::UserSetAvatarSource => "user_set_avatar_source",
            Self::UserUpdateWopiInfo => "user_update_wopi_info",
            Self::WebdavAccountCreate => "webdav_account_create",
            Self::WebdavAccountDelete => "webdav_account_delete",
            Self::WebdavAccountToggle => "webdav_account_toggle",
            Self::UserChangePassword => "user_change_password",
            Self::UserConfirmPasswordReset => "user_confirm_password_reset",
            Self::UserConfirmEmailChange => "user_confirm_email_change",
            Self::UserConfirmRegistration => "user_confirm_registration",
            Self::UserLogin => "user_login",
            Self::UserLogout => "user_logout",
            Self::UserPasskeyDelete => "user_passkey_delete",
            Self::UserPasskeyLogin => "user_passkey_login",
            Self::UserPasskeyRegister => "user_passkey_register",
            Self::UserPasskeyRename => "user_passkey_rename",
            Self::UserExternalAuthLogin => "user_external_auth_login",
            Self::UserExternalAuthLink => "user_external_auth_link",
            Self::UserExternalAuthUnlink => "user_external_auth_unlink",
            Self::UserRefreshTokenReuseDetected => "user_refresh_token_reuse_detected",
            Self::UserRequestEmailChange => "user_request_email_change",
            Self::UserRequestPasswordReset => "user_request_password_reset",
            Self::UserRegister => "user_register",
            Self::UserResendEmailChange => "user_resend_email_change",
            Self::UserResendRegistration => "user_resend_registration",
        }
    }

    pub fn from_str_name(value: &str) -> Option<Self> {
        match value {
            "admin_create_user" => Some(Self::AdminCreateUser),
            "admin_force_delete_user" => Some(Self::AdminForceDeleteUser),
            "admin_create_team" => Some(Self::AdminCreateTeam),
            "admin_create_policy_group" => Some(Self::AdminCreatePolicyGroup),
            "admin_archive_team" => Some(Self::AdminArchiveTeam),
            "admin_restore_team" => Some(Self::AdminRestoreTeam),
            "admin_revoke_user_sessions" => Some(Self::AdminRevokeUserSessions),
            "admin_reset_user_password" => Some(Self::AdminResetUserPassword),
            "admin_update_team" => Some(Self::AdminUpdateTeam),
            "admin_update_user" => Some(Self::AdminUpdateUser),
            "admin_delete_policy_group" => Some(Self::AdminDeletePolicyGroup),
            "admin_migrate_policy_group_users" => Some(Self::AdminMigratePolicyGroupUsers),
            "admin_update_policy_group" => Some(Self::AdminUpdatePolicyGroup),
            "admin_create_policy" => Some(Self::AdminCreatePolicy),
            "admin_update_policy" => Some(Self::AdminUpdatePolicy),
            "admin_delete_policy" => Some(Self::AdminDeletePolicy),
            "admin_delete_config" => Some(Self::AdminDeleteConfig),
            "admin_delete_share" => Some(Self::AdminDeleteShare),
            "admin_force_unlock" => Some(Self::AdminForceUnlock),
            "admin_cleanup_expired_locks" => Some(Self::AdminCleanupExpiredLocks),
            "admin_cleanup_tasks" => Some(Self::AdminCleanupTasks),
            "admin_create_remote_node" => Some(Self::AdminCreateRemoteNode),
            "admin_update_remote_node" => Some(Self::AdminUpdateRemoteNode),
            "admin_delete_remote_node" => Some(Self::AdminDeleteRemoteNode),
            "admin_test_remote_node" => Some(Self::AdminTestRemoteNode),
            "admin_create_remote_node_enrollment_token" => {
                Some(Self::AdminCreateRemoteNodeEnrollmentToken)
            }
            "admin_create_remote_ingress_profile" => Some(Self::AdminCreateRemoteIngressProfile),
            "admin_update_remote_ingress_profile" => Some(Self::AdminUpdateRemoteIngressProfile),
            "admin_delete_remote_ingress_profile" => Some(Self::AdminDeleteRemoteIngressProfile),
            "admin_create_external_auth_provider" => Some(Self::AdminCreateExternalAuthProvider),
            "admin_update_external_auth_provider" => Some(Self::AdminUpdateExternalAuthProvider),
            "admin_delete_external_auth_provider" => Some(Self::AdminDeleteExternalAuthProvider),
            "admin_test_external_auth_provider" => Some(Self::AdminTestExternalAuthProvider),
            "batch_copy" => Some(Self::BatchCopy),
            "batch_delete" => Some(Self::BatchDelete),
            "batch_move" => Some(Self::BatchMove),
            "config_action_execute" => Some(Self::ConfigActionExecute),
            "config_update" => Some(Self::ConfigUpdate),
            "file_copy" => Some(Self::FileCopy),
            "file_create" => Some(Self::FileCreate),
            "file_delete" => Some(Self::FileDelete),
            "file_download" => Some(Self::FileDownload),
            "file_direct_link_create" => Some(Self::FileDirectLinkCreate),
            "file_edit" => Some(Self::FileEdit),
            "file_move" => Some(Self::FileMove),
            "file_rename" => Some(Self::FileRename),
            "file_upload" => Some(Self::FileUpload),
            "file_preview_link_create" => Some(Self::FilePreviewLinkCreate),
            "file_wopi_open" => Some(Self::FileWopiOpen),
            "file_upload_cancel" => Some(Self::FileUploadCancel),
            "file_restore" => Some(Self::FileRestore),
            "file_purge" => Some(Self::FilePurge),
            "file_lock" => Some(Self::FileLock),
            "file_unlock" => Some(Self::FileUnlock),
            "file_version_restore" => Some(Self::FileVersionRestore),
            "file_version_delete" => Some(Self::FileVersionDelete),
            "folder_copy" => Some(Self::FolderCopy),
            "folder_create" => Some(Self::FolderCreate),
            "folder_delete" => Some(Self::FolderDelete),
            "folder_move" => Some(Self::FolderMove),
            "folder_policy_change" => Some(Self::FolderPolicyChange),
            "folder_rename" => Some(Self::FolderRename),
            "folder_restore" => Some(Self::FolderRestore),
            "folder_purge" => Some(Self::FolderPurge),
            "folder_lock" => Some(Self::FolderLock),
            "folder_unlock" => Some(Self::FolderUnlock),
            "property_set" => Some(Self::PropertySet),
            "property_delete" => Some(Self::PropertyDelete),
            "share_batch_delete" => Some(Self::ShareBatchDelete),
            "share_create" => Some(Self::ShareCreate),
            "share_delete" => Some(Self::ShareDelete),
            "share_update" => Some(Self::ShareUpdate),
            "system_setup" => Some(Self::SystemSetup),
            "team_archive" => Some(Self::TeamArchive),
            "team_cleanup_expired" => Some(Self::TeamCleanupExpired),
            "team_create" => Some(Self::TeamCreate),
            "team_member_add" => Some(Self::TeamMemberAdd),
            "team_member_remove" => Some(Self::TeamMemberRemove),
            "team_member_update" => Some(Self::TeamMemberUpdate),
            "team_restore" => Some(Self::TeamRestore),
            "team_update" => Some(Self::TeamUpdate),
            "task_retry" => Some(Self::TaskRetry),
            "archive_compress" => Some(Self::ArchiveCompress),
            "archive_extract" => Some(Self::ArchiveExtract),
            "archive_download" => Some(Self::ArchiveDownload),
            "trash_purge_all" => Some(Self::TrashPurgeAll),
            "remote_enrollment_redeem" => Some(Self::RemoteEnrollmentRedeem),
            "remote_enrollment_ack" => Some(Self::RemoteEnrollmentAck),
            "user_revoke_other_sessions" => Some(Self::UserRevokeOtherSessions),
            "user_revoke_session" => Some(Self::UserRevokeSession),
            "user_update_preferences" => Some(Self::UserUpdatePreferences),
            "user_update_profile" => Some(Self::UserUpdateProfile),
            "user_upload_avatar" => Some(Self::UserUploadAvatar),
            "user_set_avatar_source" => Some(Self::UserSetAvatarSource),
            "user_update_wopi_info" => Some(Self::UserUpdateWopiInfo),
            "webdav_account_create" => Some(Self::WebdavAccountCreate),
            "webdav_account_delete" => Some(Self::WebdavAccountDelete),
            "webdav_account_toggle" => Some(Self::WebdavAccountToggle),
            "user_change_password" => Some(Self::UserChangePassword),
            "user_confirm_password_reset" => Some(Self::UserConfirmPasswordReset),
            "user_confirm_email_change" => Some(Self::UserConfirmEmailChange),
            "user_confirm_registration" => Some(Self::UserConfirmRegistration),
            "user_login" => Some(Self::UserLogin),
            "user_logout" => Some(Self::UserLogout),
            "user_passkey_delete" => Some(Self::UserPasskeyDelete),
            "user_passkey_login" => Some(Self::UserPasskeyLogin),
            "user_passkey_register" => Some(Self::UserPasskeyRegister),
            "user_passkey_rename" => Some(Self::UserPasskeyRename),
            "user_external_auth_login" => Some(Self::UserExternalAuthLogin),
            "user_external_auth_link" => Some(Self::UserExternalAuthLink),
            "user_external_auth_unlink" => Some(Self::UserExternalAuthUnlink),
            "user_refresh_token_reuse_detected" => Some(Self::UserRefreshTokenReuseDetected),
            "user_request_email_change" => Some(Self::UserRequestEmailChange),
            "user_request_password_reset" => Some(Self::UserRequestPasswordReset),
            "user_register" => Some(Self::UserRegister),
            "user_resend_email_change" => Some(Self::UserResendEmailChange),
            "user_resend_registration" => Some(Self::UserResendRegistration),
            _ => None,
        }
    }
}

impl AsRef<str> for AuditAction {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for AuditAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
