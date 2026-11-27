use sea_orm::entity::prelude::*;
use serde::de::{self, Visitor};
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use super::EntityType;

macro_rules! define_audit_action_list {
    ($macro:ident) => {
        $macro! {
            AdminCreateUser,
            AdminForceDeleteUser,
            AdminCreateTeam,
            AdminCreatePolicyGroup,
            AdminArchiveTeam,
            AdminRestoreTeam,
            AdminRevokeUserSessions,
            AdminResetUserPassword,
            AdminResetUserMfa,
            AdminUpdateTeam,
            AdminUpdateUser,
            AdminDeletePolicyGroup,
            AdminMigratePolicyGroupUsers,
            AdminUpdatePolicyGroup,
            AdminCreatePolicy,
            AdminUpdatePolicy,
            AdminDeletePolicy,
            AdminTriggerStorageAction,
            AdminDeleteConfig,
            AdminDeleteShare,
            AdminForceUnlock,
            AdminCleanupExpiredLocks,
            AdminCleanupTasks,
            AdminCreateBlobMaintenanceTask,
            AdminCreateRemoteNode,
            AdminUpdateRemoteNode,
            AdminDeleteRemoteNode,
            AdminTestRemoteNode,
            AdminCreateRemoteNodeEnrollmentToken,
            // Action names use storage-target terminology; numeric bitmask IDs remain stable.
            AdminCreateRemoteStorageTarget,
            AdminUpdateRemoteStorageTarget,
            AdminDeleteRemoteStorageTarget,
            AdminCreateExternalAuthProvider,
            AdminUpdateExternalAuthProvider,
            AdminDeleteExternalAuthProvider,
            AdminTestExternalAuthProvider,
            BatchCopy,
            BatchDelete,
            BatchMove,
            ConfigActionExecute,
            ConfigUpdate,
            FileCopy,
            FileCreate,
            FileDelete,
            FileDownload,
            FileDirectLinkCreate,
            FileEdit,
            FileMove,
            FileRename,
            FileUpload,
            FilePreviewLinkCreate,
            FileWopiOpen,
            FileUploadCancel,
            FileRestore,
            FilePurge,
            FileLock,
            FileUnlock,
            FileVersionRestore,
            FileVersionDelete,
            FolderCopy,
            FolderCreate,
            FolderDelete,
            FolderMove,
            FolderPolicyChange,
            FolderRename,
            FolderRestore,
            FolderPurge,
            FolderLock,
            FolderUnlock,
            PropertySet,
            PropertyDelete,
            ShareBatchDelete,
            ShareCreate,
            ShareDelete,
            ShareUpdate,
            SystemSetup,
            ServerStart,
            ServerShutdown,
            TeamArchive,
            TeamCleanupExpired,
            TeamCreate,
            TeamMemberAdd,
            TeamMemberRemove,
            TeamMemberUpdate,
            TeamRestore,
            TeamUpdate,
            TaskRetry,
            ArchiveCompress,
            ArchiveExtract,
            ArchiveDownload,
            OfflineDownload,
            TrashPurgeAll,
            RemoteEnrollmentRedeem,
            RemoteEnrollmentAck,
            UserRevokeOtherSessions,
            UserRevokeSession,
            UserUpdatePreferences,
            UserUpdateProfile,
            UserUploadAvatar,
            UserSetAvatarSource,
            UserUpdateWopiInfo,
            WebdavAccountCreate,
            WebdavAccountDelete,
            WebdavAccountToggle,
            WebdavAccountAccessModeUpdate,
            TeamWebdavAccountCreate,
            TeamWebdavAccountDelete,
            TeamWebdavAccountToggle,
            TeamWebdavAccountAccessModeUpdate,
            UserChangePassword,
            UserConfirmPasswordReset,
            UserConfirmEmailChange,
            UserConfirmRegistration,
            UserLogin,
            UserLogout,
            UserMfaEnable,
            UserMfaDisable,
            UserMfaRecoveryCodesRegenerate,
            UserMfaEmailCodeSend,
            UserMfaChallengeSuccess,
            UserMfaChallengeFailed,
            UserPasskeyDelete,
            UserPasskeyLogin,
            UserPasskeyRegister,
            UserPasskeyRename,
            UserExternalAuthLogin,
            UserExternalAuthLink,
            UserExternalAuthUnlink,
            UserRefreshTokenReuseDetected,
            UserRequestEmailChange,
            UserRequestPasswordReset,
            UserRegister,
            UserResendEmailChange,
            UserResendRegistration,
            // Follower actions are written to the follower node's own audit table.
            // The primary admin audit page will not see them unless follower logs are
            // queried directly or aggregated later, so keep them distinct from
            // primary-side Admin* remote management actions.
            FollowerBindingSync,
            FollowerObjectRead,
            FollowerObjectWrite,
            FollowerObjectDelete,
            FollowerObjectCompose,
            // Action names use storage-target terminology; numeric bitmask IDs remain stable.
            FollowerStorageTargetCreate,
            FollowerStorageTargetUpdate,
            FollowerStorageTargetDelete,
            MailSend,
            MailDeliveryFailed,
            AdminCreateInvitation,
            AdminRevokeInvitation,
            TagCreate,
            TagUpdate,
            TagDelete,
            TagAttach,
            TagDetach,
            RemoteNodeConnected,
            RemoteNodeGracefulDisconnect,
            RemoteNodeUnexpectedDisconnect,
            RemoteNodeHeartbeatTimeout,
            AdminCreateStoragePolicyForcedPurgeTask,
            FolderIconChange,
            AdminWorkspaceInspect,
            AdminFileDownload,
            AdminRevisionDownload,
            TaskCancelRequested,
        }
    };
}

macro_rules! audit_action_count {
    ($($variant:ident),+ $(,)?) => {
        <[()]>::len(&[$(audit_action_count!(@unit $variant)),+])
    };
    (@unit $variant:ident) => {
        ()
    };
}

macro_rules! audit_action_all {
    ($($variant:ident),+ $(,)?) => {
        [$(AuditAction::$variant,)+]
    };
}

macro_rules! define_audit_entity_type {
    ($($variant:ident => $name:literal),+ $(,)?) => {
        /// 审计日志实体类型
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
        pub enum AuditEntityType {
            $(
                /// 被审计的资源实体类型。
                #[serde(rename = $name)]
                $variant,
            )+
        }

        impl AuditEntityType {
            /// Number of distinct audit entity types.
            pub const COUNT: usize = <[()]>::len(&[$(define_audit_entity_type!(@unit $variant)),+]);
            /// Every audit entity type, in declaration order.
            pub const ALL: [Self; Self::COUNT] = [$(Self::$variant,)+];

            /// Canonical snake_case wire name persisted in audit rows and accepted by filters.
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $name,)+
                }
            }

            /// Parses a wire name produced by [`Self::as_str`]; returns `None` for unknown names.
            pub fn from_str_name(value: &str) -> Option<Self> {
                match value {
                    $($name => Some(Self::$variant),)+
                    _ => None,
                }
            }

            /// Maps a general entity type to its audit counterpart; only file and folder are audited.
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
    Mail => "mail",
    Invitation => "invitation",
    MfaFactor => "mfa_factor",
    Passkey => "passkey",
    PolicyGroup => "policy_group",
    // Historical storage/API values are normalized by the audit terminology migration.
    RemoteStorageTarget => "remote_storage_target",
    RemoteNode => "remote_node",
    ResourceLock => "resource_lock",
    Share => "share",
    StoragePolicy => "storage_policy",
    StreamTicket => "stream_ticket",
    SystemConfig => "system_config",
    Tag => "tag",
    Task => "task",
    Team => "team",
    Trash => "trash",
    UploadSession => "upload_session",
    User => "user",
    WebdavAccount => "webdav_account",
}

// Derive cannot resolve serde rename literals inside the entity-list macro.
// Build the API enum from the same wire values used by persistence.
#[cfg(all(debug_assertions, feature = "openapi"))]
impl utoipa::PartialSchema for AuditEntityType {
    /// Generate entity-type schema values from the same identifiers accepted by persistence and filters.
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        utoipa::openapi::ObjectBuilder::new()
            .schema_type(utoipa::openapi::schema::Type::String)
            .description(Some("审计日志实体类型"))
            .enum_values(Some(AUDIT_ENTITY_TYPE_NAMES.iter().copied()))
            .into()
    }
}

#[cfg(all(debug_assertions, feature = "openapi"))]
impl ToSchema for AuditEntityType {}

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

/// Error returned when a string does not match any known audit entity type.
#[derive(Debug, Clone, Copy)]
pub struct ParseAuditEntityTypeError;

impl fmt::Display for ParseAuditEntityTypeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid audit entity type")
    }
}

impl std::error::Error for ParseAuditEntityTypeError {}

/// 审计日志动作
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(64))")]
#[serde(rename_all = "snake_case")]
pub enum AuditAction {
    /// Administrator created a user account.
    #[sea_orm(string_value = "admin_create_user")]
    AdminCreateUser,
    /// Administrator permanently deleted a user account together with its owned resources.
    #[sea_orm(string_value = "admin_force_delete_user")]
    AdminForceDeleteUser,
    /// Administrator created a team.
    #[sea_orm(string_value = "admin_create_team")]
    AdminCreateTeam,
    /// Administrator created a policy group.
    #[sea_orm(string_value = "admin_create_policy_group")]
    AdminCreatePolicyGroup,
    /// Administrator archived a team.
    #[sea_orm(string_value = "admin_archive_team")]
    AdminArchiveTeam,
    /// Administrator restored a previously archived team.
    #[sea_orm(string_value = "admin_restore_team")]
    AdminRestoreTeam,
    /// Administrator revoked all active sessions of a user.
    #[sea_orm(string_value = "admin_revoke_user_sessions")]
    AdminRevokeUserSessions,
    /// Administrator reset a user's password.
    #[sea_orm(string_value = "admin_reset_user_password")]
    AdminResetUserPassword,
    /// Administrator reset or disabled a user's MFA enrollment.
    #[sea_orm(string_value = "admin_reset_user_mfa")]
    AdminResetUserMfa,
    /// Administrator updated team settings.
    #[sea_orm(string_value = "admin_update_team")]
    AdminUpdateTeam,
    /// Administrator updated a user account.
    #[sea_orm(string_value = "admin_update_user")]
    AdminUpdateUser,
    /// Administrator deleted a policy group.
    #[sea_orm(string_value = "admin_delete_policy_group")]
    AdminDeletePolicyGroup,
    /// Administrator moved users from one policy group to another.
    #[sea_orm(string_value = "admin_migrate_policy_group_users")]
    AdminMigratePolicyGroupUsers,
    /// Administrator updated a policy group.
    #[sea_orm(string_value = "admin_update_policy_group")]
    AdminUpdatePolicyGroup,
    /// Administrator created a storage policy.
    #[sea_orm(string_value = "admin_create_policy")]
    AdminCreatePolicy,
    /// Administrator updated a storage policy.
    #[sea_orm(string_value = "admin_update_policy")]
    AdminUpdatePolicy,
    /// Administrator deleted a storage policy.
    #[sea_orm(string_value = "admin_delete_policy")]
    AdminDeletePolicy,
    /// Administrator executed a connector-defined action on a storage policy or its draft.
    #[sea_orm(string_value = "admin_trigger_storage_action")]
    AdminTriggerStorageAction,
    /// Administrator deleted a system configuration entry.
    #[sea_orm(string_value = "admin_delete_config")]
    AdminDeleteConfig,
    /// Administrator deleted a share owned by another user.
    #[sea_orm(string_value = "admin_delete_share")]
    AdminDeleteShare,
    /// Administrator force-released a resource lock regardless of its owner.
    #[sea_orm(string_value = "admin_force_unlock")]
    AdminForceUnlock,
    /// Administrator triggered cleanup of expired resource locks.
    #[sea_orm(string_value = "admin_cleanup_expired_locks")]
    AdminCleanupExpiredLocks,
    /// Administrator triggered cleanup of finished or stale background tasks.
    #[sea_orm(string_value = "admin_cleanup_tasks")]
    AdminCleanupTasks,
    /// Administrator created a blob maintenance (garbage collection / repair) task.
    #[sea_orm(string_value = "admin_create_blob_maintenance_task")]
    AdminCreateBlobMaintenanceTask,
    /// Administrator registered a remote node.
    #[sea_orm(string_value = "admin_create_remote_node")]
    AdminCreateRemoteNode,
    /// Administrator updated a remote node's configuration.
    #[sea_orm(string_value = "admin_update_remote_node")]
    AdminUpdateRemoteNode,
    /// Administrator removed a remote node.
    #[sea_orm(string_value = "admin_delete_remote_node")]
    AdminDeleteRemoteNode,
    /// Administrator ran a connectivity test against a remote node.
    #[sea_orm(string_value = "admin_test_remote_node")]
    AdminTestRemoteNode,
    /// Administrator issued an enrollment token for joining a remote node.
    #[sea_orm(string_value = "admin_create_remote_node_enrollment_token")]
    AdminCreateRemoteNodeEnrollmentToken,
    /// Administrator created a remote storage target on a remote node.
    #[sea_orm(string_value = "admin_create_remote_storage_target")]
    AdminCreateRemoteStorageTarget,
    /// Administrator updated a remote storage target on a remote node.
    #[sea_orm(string_value = "admin_update_remote_storage_target")]
    AdminUpdateRemoteStorageTarget,
    /// Administrator deleted a remote storage target on a remote node.
    #[sea_orm(string_value = "admin_delete_remote_storage_target")]
    AdminDeleteRemoteStorageTarget,
    /// Administrator registered an external authentication provider.
    #[sea_orm(string_value = "admin_create_external_auth_provider")]
    AdminCreateExternalAuthProvider,
    /// Administrator updated an external authentication provider.
    #[sea_orm(string_value = "admin_update_external_auth_provider")]
    AdminUpdateExternalAuthProvider,
    /// Administrator removed an external authentication provider.
    #[sea_orm(string_value = "admin_delete_external_auth_provider")]
    AdminDeleteExternalAuthProvider,
    /// Administrator ran a connectivity test against an external authentication provider.
    #[sea_orm(string_value = "admin_test_external_auth_provider")]
    AdminTestExternalAuthProvider,
    /// A multi-file batch copy operation completed.
    #[sea_orm(string_value = "batch_copy")]
    BatchCopy,
    /// A multi-file batch delete operation completed.
    #[sea_orm(string_value = "batch_delete")]
    BatchDelete,
    /// A multi-file batch move operation completed.
    #[sea_orm(string_value = "batch_move")]
    BatchMove,
    /// A stored system configuration action was executed.
    #[sea_orm(string_value = "config_action_execute")]
    ConfigActionExecute,
    /// A system configuration value was created or changed.
    #[sea_orm(string_value = "config_update")]
    ConfigUpdate,
    /// A file was copied within or across workspaces.
    #[sea_orm(string_value = "file_copy")]
    FileCopy,
    /// A file was created.
    #[sea_orm(string_value = "file_create")]
    FileCreate,
    /// A file was moved to trash.
    #[sea_orm(string_value = "file_delete")]
    FileDelete,
    /// A file's content was downloaded.
    #[sea_orm(string_value = "file_download")]
    FileDownload,
    /// A direct-download access token was issued for a file.
    #[sea_orm(string_value = "file_direct_link_create")]
    FileDirectLinkCreate,
    /// A file's content was modified in place.
    #[sea_orm(string_value = "file_edit")]
    FileEdit,
    /// A file was moved to another folder.
    #[sea_orm(string_value = "file_move")]
    FileMove,
    /// A file was renamed.
    #[sea_orm(string_value = "file_rename")]
    FileRename,
    /// A file upload completed.
    #[sea_orm(string_value = "file_upload")]
    FileUpload,
    /// An in-browser preview access token was issued for a file.
    #[sea_orm(string_value = "file_preview_link_create")]
    FilePreviewLinkCreate,
    /// A file was opened for editing through the WOPI protocol.
    #[sea_orm(string_value = "file_wopi_open")]
    FileWopiOpen,
    /// An in-progress upload was cancelled by the client.
    #[sea_orm(string_value = "file_upload_cancel")]
    FileUploadCancel,
    /// A trashed file was restored.
    #[sea_orm(string_value = "file_restore")]
    FileRestore,
    /// A trashed file was permanently purged.
    #[sea_orm(string_value = "file_purge")]
    FilePurge,
    /// A lock was placed on a file.
    #[sea_orm(string_value = "file_lock")]
    FileLock,
    /// A file's lock was released.
    #[sea_orm(string_value = "file_unlock")]
    FileUnlock,
    /// A file was rolled back to a previous version.
    #[sea_orm(string_value = "file_version_restore")]
    FileVersionRestore,
    /// A stored file version was deleted.
    #[sea_orm(string_value = "file_version_delete")]
    FileVersionDelete,
    /// A folder was copied within or across workspaces.
    #[sea_orm(string_value = "folder_copy")]
    FolderCopy,
    /// A folder was created.
    #[sea_orm(string_value = "folder_create")]
    FolderCreate,
    /// A folder was moved to trash.
    #[sea_orm(string_value = "folder_delete")]
    FolderDelete,
    /// A folder was moved to another parent.
    #[sea_orm(string_value = "folder_move")]
    FolderMove,
    /// A folder's storage policy assignment was changed.
    #[sea_orm(string_value = "folder_policy_change")]
    FolderPolicyChange,
    /// A folder was renamed.
    #[sea_orm(string_value = "folder_rename")]
    FolderRename,
    /// A trashed folder was restored.
    #[sea_orm(string_value = "folder_restore")]
    FolderRestore,
    /// A trashed folder was permanently purged.
    #[sea_orm(string_value = "folder_purge")]
    FolderPurge,
    /// A lock was placed on a folder.
    #[sea_orm(string_value = "folder_lock")]
    FolderLock,
    /// A folder's lock was released.
    #[sea_orm(string_value = "folder_unlock")]
    FolderUnlock,
    /// A WebDAV metadata property was set on a resource.
    #[sea_orm(string_value = "property_set")]
    PropertySet,
    /// A WebDAV metadata property was removed from a resource.
    #[sea_orm(string_value = "property_delete")]
    PropertyDelete,
    /// Multiple shares were deleted in one batch.
    #[sea_orm(string_value = "share_batch_delete")]
    ShareBatchDelete,
    /// A share was created.
    #[sea_orm(string_value = "share_create")]
    ShareCreate,
    /// A share was deleted.
    #[sea_orm(string_value = "share_delete")]
    ShareDelete,
    /// A share's settings were updated.
    #[sea_orm(string_value = "share_update")]
    ShareUpdate,
    /// Initial instance setup completed and the system became usable.
    #[sea_orm(string_value = "system_setup")]
    SystemSetup,
    /// The server process started.
    #[sea_orm(string_value = "server_start")]
    ServerStart,
    /// The server process shut down.
    #[sea_orm(string_value = "server_shutdown")]
    ServerShutdown,
    /// A team was archived.
    #[sea_orm(string_value = "team_archive")]
    TeamArchive,
    /// Expired archived teams were cleaned up by the retention sweep.
    #[sea_orm(string_value = "team_cleanup_expired")]
    TeamCleanupExpired,
    /// A team was created.
    #[sea_orm(string_value = "team_create")]
    TeamCreate,
    /// A member was added to a team.
    #[sea_orm(string_value = "team_member_add")]
    TeamMemberAdd,
    /// A member was removed from a team.
    #[sea_orm(string_value = "team_member_remove")]
    TeamMemberRemove,
    /// A team member's role or membership was updated.
    #[sea_orm(string_value = "team_member_update")]
    TeamMemberUpdate,
    /// A previously archived team was restored.
    #[sea_orm(string_value = "team_restore")]
    TeamRestore,
    /// A team's settings were updated.
    #[sea_orm(string_value = "team_update")]
    TeamUpdate,
    /// A background task was retried.
    #[sea_orm(string_value = "task_retry")]
    TaskRetry,
    /// Cancellation was requested for a running background task.
    #[sea_orm(string_value = "task_cancel_requested")]
    TaskCancelRequested,
    /// An archive file was created by compressing selected resources.
    #[sea_orm(string_value = "archive_compress")]
    ArchiveCompress,
    /// An archive file was extracted into the workspace.
    #[sea_orm(string_value = "archive_extract")]
    ArchiveExtract,
    /// An archive file was downloaded.
    #[sea_orm(string_value = "archive_download")]
    ArchiveDownload,
    /// A remote-URL download task was created to fetch content into the workspace.
    #[sea_orm(string_value = "offline_download")]
    OfflineDownload,
    /// The entire trash was emptied.
    #[sea_orm(string_value = "trash_purge_all")]
    TrashPurgeAll,
    /// A remote node redeemed an enrollment token to join the primary.
    #[sea_orm(string_value = "remote_enrollment_redeem")]
    RemoteEnrollmentRedeem,
    /// The primary acknowledged a remote node's enrollment completion.
    #[sea_orm(string_value = "remote_enrollment_ack")]
    RemoteEnrollmentAck,
    /// A user revoked all sessions except the current one.
    #[sea_orm(string_value = "user_revoke_other_sessions")]
    UserRevokeOtherSessions,
    /// A user revoked a specific session.
    #[sea_orm(string_value = "user_revoke_session")]
    UserRevokeSession,
    /// A user updated their preferences.
    #[sea_orm(string_value = "user_update_preferences")]
    UserUpdatePreferences,
    /// A user updated their profile information.
    #[sea_orm(string_value = "user_update_profile")]
    UserUpdateProfile,
    /// A user uploaded a new avatar image.
    #[sea_orm(string_value = "user_upload_avatar")]
    UserUploadAvatar,
    /// A user switched their avatar source (none, gravatar, or upload).
    #[sea_orm(string_value = "user_set_avatar_source")]
    UserSetAvatarSource,
    /// A user updated their WOPI integration identity information.
    #[sea_orm(string_value = "user_update_wopi_info")]
    UserUpdateWopiInfo,
    /// A personal WebDAV account credential was created.
    #[sea_orm(string_value = "webdav_account_create")]
    WebdavAccountCreate,
    /// A personal WebDAV account was deleted.
    #[sea_orm(string_value = "webdav_account_delete")]
    WebdavAccountDelete,
    /// A personal WebDAV account was enabled or disabled.
    #[sea_orm(string_value = "webdav_account_toggle")]
    WebdavAccountToggle,
    /// A personal WebDAV account's access mode was changed.
    #[sea_orm(string_value = "webdav_account_access_mode_update")]
    WebdavAccountAccessModeUpdate,

    /// A team-scoped WebDAV account credential was created.
    #[sea_orm(string_value = "team_webdav_account_create")]
    TeamWebdavAccountCreate,
    /// A team-scoped WebDAV account was deleted.
    #[sea_orm(string_value = "team_webdav_account_delete")]
    TeamWebdavAccountDelete,
    /// A team-scoped WebDAV account was enabled or disabled.
    #[sea_orm(string_value = "team_webdav_account_toggle")]
    TeamWebdavAccountToggle,
    /// A team-scoped WebDAV account's access mode was changed.
    #[sea_orm(string_value = "team_webdav_account_access_mode_update")]
    TeamWebdavAccountAccessModeUpdate,
    /// A user changed their own password.
    #[sea_orm(string_value = "user_change_password")]
    UserChangePassword,
    /// A user completed a password reset with a reset token.
    #[sea_orm(string_value = "user_confirm_password_reset")]
    UserConfirmPasswordReset,
    /// A user confirmed an email change with the confirmation token.
    #[sea_orm(string_value = "user_confirm_email_change")]
    UserConfirmEmailChange,
    /// A user confirmed registration with the confirmation token.
    #[sea_orm(string_value = "user_confirm_registration")]
    UserConfirmRegistration,
    /// A user logged in.
    #[sea_orm(string_value = "user_login")]
    UserLogin,
    /// A user logged out.
    #[sea_orm(string_value = "user_logout")]
    UserLogout,
    /// A user enabled MFA.
    #[sea_orm(string_value = "user_mfa_enable")]
    UserMfaEnable,
    /// A user disabled MFA.
    #[sea_orm(string_value = "user_mfa_disable")]
    UserMfaDisable,
    /// A user regenerated their MFA recovery codes.
    #[sea_orm(string_value = "user_mfa_recovery_codes_regenerate")]
    UserMfaRecoveryCodesRegenerate,
    /// An MFA email verification code was sent to a user.
    #[sea_orm(string_value = "user_mfa_email_code_send")]
    UserMfaEmailCodeSend,
    /// An MFA challenge was satisfied.
    #[sea_orm(string_value = "user_mfa_challenge_success")]
    UserMfaChallengeSuccess,
    /// An MFA challenge attempt failed.
    #[sea_orm(string_value = "user_mfa_challenge_failed")]
    UserMfaChallengeFailed,
    /// A registered passkey was deleted.
    #[sea_orm(string_value = "user_passkey_delete")]
    UserPasskeyDelete,
    /// A user authenticated with a passkey.
    #[sea_orm(string_value = "user_passkey_login")]
    UserPasskeyLogin,
    /// A new passkey was registered for a user.
    #[sea_orm(string_value = "user_passkey_register")]
    UserPasskeyRegister,
    /// A registered passkey was renamed.
    #[sea_orm(string_value = "user_passkey_rename")]
    UserPasskeyRename,
    /// A user logged in through an external authentication provider.
    #[sea_orm(string_value = "user_external_auth_login")]
    UserExternalAuthLogin,
    /// A user linked an external authentication identity to their account.
    #[sea_orm(string_value = "user_external_auth_link")]
    UserExternalAuthLink,
    /// A user unlinked an external authentication identity from their account.
    #[sea_orm(string_value = "user_external_auth_unlink")]
    UserExternalAuthUnlink,
    /// Refresh-token reuse was detected and the token family was invalidated.
    #[sea_orm(string_value = "user_refresh_token_reuse_detected")]
    UserRefreshTokenReuseDetected,
    /// A user requested an email change.
    #[sea_orm(string_value = "user_request_email_change")]
    UserRequestEmailChange,
    /// A user requested a password reset email.
    #[sea_orm(string_value = "user_request_password_reset")]
    UserRequestPasswordReset,
    /// A new user registered an account.
    #[sea_orm(string_value = "user_register")]
    UserRegister,
    /// The email-change confirmation email was resent.
    #[sea_orm(string_value = "user_resend_email_change")]
    UserResendEmailChange,
    /// The registration confirmation email was resent.
    #[sea_orm(string_value = "user_resend_registration")]
    UserResendRegistration,
    /// Binding information was synced from the primary to a follower node.
    #[sea_orm(string_value = "follower_binding_sync")]
    FollowerBindingSync,
    /// A follower node served an object read to the internal storage protocol.
    #[sea_orm(string_value = "follower_object_read")]
    FollowerObjectRead,
    /// A follower node stored an object through the internal storage protocol.
    #[sea_orm(string_value = "follower_object_write")]
    FollowerObjectWrite,
    /// A follower node deleted an object through the internal storage protocol.
    #[sea_orm(string_value = "follower_object_delete")]
    FollowerObjectDelete,
    /// A follower node assembled uploaded parts into a complete object.
    #[sea_orm(string_value = "follower_object_compose")]
    FollowerObjectCompose,
    /// A follower node created a remote storage target.
    #[sea_orm(string_value = "follower_storage_target_create")]
    FollowerStorageTargetCreate,
    /// A follower node updated a remote storage target.
    #[sea_orm(string_value = "follower_storage_target_update")]
    FollowerStorageTargetUpdate,
    /// A follower node deleted a remote storage target.
    #[sea_orm(string_value = "follower_storage_target_delete")]
    FollowerStorageTargetDelete,
    /// An email was sent.
    #[sea_orm(string_value = "mail_send")]
    MailSend,
    /// An email delivery attempt failed.
    #[sea_orm(string_value = "mail_delivery_failed")]
    MailDeliveryFailed,
    /// Administrator created an invitation.
    #[sea_orm(string_value = "admin_create_invitation")]
    AdminCreateInvitation,
    /// Administrator revoked an invitation.
    #[sea_orm(string_value = "admin_revoke_invitation")]
    AdminRevokeInvitation,
    /// A tag was created.
    #[sea_orm(string_value = "tag_create")]
    TagCreate,
    /// A tag was updated.
    #[sea_orm(string_value = "tag_update")]
    TagUpdate,
    /// A tag was deleted.
    #[sea_orm(string_value = "tag_delete")]
    TagDelete,
    /// A tag was attached to a resource.
    #[sea_orm(string_value = "tag_attach")]
    TagAttach,
    /// A tag was detached from a resource.
    #[sea_orm(string_value = "tag_detach")]
    TagDetach,
    /// A remote node established a connection to the primary.
    #[sea_orm(string_value = "remote_node_connected")]
    RemoteNodeConnected,
    /// A remote node disconnected in an orderly shutdown.
    #[sea_orm(string_value = "remote_node_graceful_disconnect")]
    RemoteNodeGracefulDisconnect,
    /// A remote node dropped its connection without notice.
    #[sea_orm(string_value = "remote_node_unexpected_disconnect")]
    RemoteNodeUnexpectedDisconnect,
    /// A remote node was marked offline after missing heartbeats.
    #[sea_orm(string_value = "remote_node_heartbeat_timeout")]
    RemoteNodeHeartbeatTimeout,
    /// Administrator confirmed creation of a destructive policy-purge task.
    #[sea_orm(string_value = "admin_create_storage_policy_forced_purge_task")]
    AdminCreateStoragePolicyForcedPurgeTask,
    /// A folder's display icon was changed.
    #[sea_orm(string_value = "folder_icon_change")]
    FolderIconChange,
    /// Administrator inspected another user's workspace for support or moderation.
    #[sea_orm(string_value = "admin_workspace_inspect")]
    AdminWorkspaceInspect,
    /// Administrator downloaded a file from a user's workspace.
    #[sea_orm(string_value = "admin_file_download")]
    AdminFileDownload,
    /// Administrator downloaded a specific file revision.
    #[sea_orm(string_value = "admin_revision_download")]
    AdminRevisionDownload,
}

impl AuditAction {
    /// Whether the action mutates workspace content, which should be audited unconditionally.
    pub const fn is_workspace_mutation(self) -> bool {
        matches!(
            self,
            Self::FileCreate
                | Self::FileCopy
                | Self::FileMove
                | Self::FileRename
                | Self::FileEdit
                | Self::FileUpload
                | Self::FileDelete
                | Self::FileRestore
                | Self::FileVersionRestore
                | Self::FolderCreate
                | Self::FolderCopy
                | Self::FolderMove
                | Self::FolderRename
                | Self::FolderDelete
                | Self::FolderRestore
                | Self::FolderIconChange
                | Self::TagAttach
                | Self::TagDetach
                | Self::PropertySet
                | Self::PropertyDelete
        )
    }

    /// Number of distinct audit actions.
    pub const COUNT: usize = define_audit_action_list!(audit_action_count);
    /// Every audit action, in the order matching its stable bit index.
    pub const ALL: [Self; Self::COUNT] = define_audit_action_list!(audit_action_all);

    /// Return the stable action bit index used by runtime audit-selection masks.
    pub const fn index(self) -> usize {
        match self {
            Self::AdminCreateUser => 0,
            Self::AdminForceDeleteUser => 1,
            Self::AdminCreateTeam => 2,
            Self::AdminCreatePolicyGroup => 3,
            Self::AdminArchiveTeam => 4,
            Self::AdminRestoreTeam => 5,
            Self::AdminRevokeUserSessions => 6,
            Self::AdminResetUserPassword => 7,
            Self::AdminResetUserMfa => 8,
            Self::AdminUpdateTeam => 9,
            Self::AdminUpdateUser => 10,
            Self::AdminDeletePolicyGroup => 11,
            Self::AdminMigratePolicyGroupUsers => 12,
            Self::AdminUpdatePolicyGroup => 13,
            Self::AdminCreatePolicy => 14,
            Self::AdminUpdatePolicy => 15,
            Self::AdminDeletePolicy => 16,
            Self::AdminTriggerStorageAction => 17,
            Self::AdminDeleteConfig => 18,
            Self::AdminDeleteShare => 19,
            Self::AdminForceUnlock => 20,
            Self::AdminCleanupExpiredLocks => 21,
            Self::AdminCleanupTasks => 22,
            Self::AdminCreateBlobMaintenanceTask => 23,
            Self::AdminCreateRemoteNode => 24,
            Self::AdminUpdateRemoteNode => 25,
            Self::AdminDeleteRemoteNode => 26,
            Self::AdminTestRemoteNode => 27,
            Self::AdminCreateRemoteNodeEnrollmentToken => 28,
            Self::AdminCreateRemoteStorageTarget => 29,
            Self::AdminUpdateRemoteStorageTarget => 30,
            Self::AdminDeleteRemoteStorageTarget => 31,
            Self::AdminCreateExternalAuthProvider => 32,
            Self::AdminUpdateExternalAuthProvider => 33,
            Self::AdminDeleteExternalAuthProvider => 34,
            Self::AdminTestExternalAuthProvider => 35,
            Self::BatchCopy => 36,
            Self::BatchDelete => 37,
            Self::BatchMove => 38,
            Self::ConfigActionExecute => 39,
            Self::ConfigUpdate => 40,
            Self::FileCopy => 41,
            Self::FileCreate => 42,
            Self::FileDelete => 43,
            Self::FileDownload => 44,
            Self::FileDirectLinkCreate => 45,
            Self::FileEdit => 46,
            Self::FileMove => 47,
            Self::FileRename => 48,
            Self::FileUpload => 49,
            Self::FilePreviewLinkCreate => 50,
            Self::FileWopiOpen => 51,
            Self::FileUploadCancel => 52,
            Self::FileRestore => 53,
            Self::FilePurge => 54,
            Self::FileLock => 55,
            Self::FileUnlock => 56,
            Self::FileVersionRestore => 57,
            Self::FileVersionDelete => 58,
            Self::FolderCopy => 59,
            Self::FolderCreate => 60,
            Self::FolderDelete => 61,
            Self::FolderMove => 62,
            Self::FolderPolicyChange => 63,
            Self::FolderRename => 64,
            Self::FolderRestore => 65,
            Self::FolderPurge => 66,
            Self::FolderLock => 67,
            Self::FolderUnlock => 68,
            Self::PropertySet => 69,
            Self::PropertyDelete => 70,
            Self::ShareBatchDelete => 71,
            Self::ShareCreate => 72,
            Self::ShareDelete => 73,
            Self::ShareUpdate => 74,
            Self::SystemSetup => 75,
            Self::ServerStart => 76,
            Self::ServerShutdown => 77,
            Self::TeamArchive => 78,
            Self::TeamCleanupExpired => 79,
            Self::TeamCreate => 80,
            Self::TeamMemberAdd => 81,
            Self::TeamMemberRemove => 82,
            Self::TeamMemberUpdate => 83,
            Self::TeamRestore => 84,
            Self::TeamUpdate => 85,
            Self::TaskRetry => 86,
            Self::TaskCancelRequested => 158,
            Self::ArchiveCompress => 87,
            Self::ArchiveExtract => 88,
            Self::ArchiveDownload => 89,
            Self::OfflineDownload => 90,
            Self::TrashPurgeAll => 91,
            Self::RemoteEnrollmentRedeem => 92,
            Self::RemoteEnrollmentAck => 93,
            Self::UserRevokeOtherSessions => 94,
            Self::UserRevokeSession => 95,
            Self::UserUpdatePreferences => 96,
            Self::UserUpdateProfile => 97,
            Self::UserUploadAvatar => 98,
            Self::UserSetAvatarSource => 99,
            Self::UserUpdateWopiInfo => 100,
            Self::WebdavAccountCreate => 101,
            Self::WebdavAccountDelete => 102,
            Self::WebdavAccountToggle => 103,
            Self::TeamWebdavAccountCreate => 104,
            Self::TeamWebdavAccountDelete => 105,
            Self::TeamWebdavAccountToggle => 106,
            Self::UserChangePassword => 107,
            Self::UserConfirmPasswordReset => 108,
            Self::UserConfirmEmailChange => 109,
            Self::UserConfirmRegistration => 110,
            Self::UserLogin => 111,
            Self::UserLogout => 112,
            Self::UserMfaEnable => 113,
            Self::UserMfaDisable => 114,
            Self::UserMfaRecoveryCodesRegenerate => 115,
            Self::UserMfaEmailCodeSend => 116,
            Self::UserMfaChallengeSuccess => 117,
            Self::UserMfaChallengeFailed => 118,
            Self::UserPasskeyDelete => 119,
            Self::UserPasskeyLogin => 120,
            Self::UserPasskeyRegister => 121,
            Self::UserPasskeyRename => 122,
            Self::UserExternalAuthLogin => 123,
            Self::UserExternalAuthLink => 124,
            Self::UserExternalAuthUnlink => 125,
            Self::UserRefreshTokenReuseDetected => 126,
            Self::UserRequestEmailChange => 127,
            Self::UserRequestPasswordReset => 128,
            Self::UserRegister => 129,
            Self::UserResendEmailChange => 130,
            Self::UserResendRegistration => 131,
            Self::FollowerBindingSync => 132,
            Self::FollowerObjectRead => 133,
            Self::FollowerObjectWrite => 134,
            Self::FollowerObjectDelete => 135,
            Self::FollowerObjectCompose => 136,
            Self::FollowerStorageTargetCreate => 137,
            Self::FollowerStorageTargetUpdate => 138,
            Self::FollowerStorageTargetDelete => 139,
            Self::MailSend => 140,
            Self::MailDeliveryFailed => 141,
            Self::AdminCreateInvitation => 142,
            Self::AdminRevokeInvitation => 143,
            Self::TagCreate => 144,
            Self::TagUpdate => 145,
            Self::TagDelete => 146,
            Self::TagAttach => 147,
            Self::TagDetach => 148,
            Self::RemoteNodeConnected => 149,
            Self::RemoteNodeGracefulDisconnect => 150,
            Self::RemoteNodeUnexpectedDisconnect => 151,
            Self::RemoteNodeHeartbeatTimeout => 152,
            Self::AdminCreateStoragePolicyForcedPurgeTask => 153,
            Self::FolderIconChange => 154,
            Self::AdminWorkspaceInspect => 155,
            Self::AdminFileDownload => 156,
            Self::AdminRevisionDownload => 157,
            Self::WebdavAccountAccessModeUpdate => 159,
            Self::TeamWebdavAccountAccessModeUpdate => 160,
        }
    }

    /// Return the canonical action identifier used by persistence, API responses and configuration.
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
            Self::AdminResetUserMfa => "admin_reset_user_mfa",
            Self::AdminUpdateTeam => "admin_update_team",
            Self::AdminUpdateUser => "admin_update_user",
            Self::AdminDeletePolicyGroup => "admin_delete_policy_group",
            Self::AdminMigratePolicyGroupUsers => "admin_migrate_policy_group_users",
            Self::AdminUpdatePolicyGroup => "admin_update_policy_group",
            Self::AdminCreatePolicy => "admin_create_policy",
            Self::AdminUpdatePolicy => "admin_update_policy",
            Self::AdminDeletePolicy => "admin_delete_policy",
            Self::AdminTriggerStorageAction => "admin_trigger_storage_action",
            Self::AdminDeleteConfig => "admin_delete_config",
            Self::AdminDeleteShare => "admin_delete_share",
            Self::AdminForceUnlock => "admin_force_unlock",
            Self::AdminCleanupExpiredLocks => "admin_cleanup_expired_locks",
            Self::AdminCleanupTasks => "admin_cleanup_tasks",
            Self::AdminCreateBlobMaintenanceTask => "admin_create_blob_maintenance_task",
            Self::AdminCreateRemoteNode => "admin_create_remote_node",
            Self::AdminUpdateRemoteNode => "admin_update_remote_node",
            Self::AdminDeleteRemoteNode => "admin_delete_remote_node",
            Self::AdminTestRemoteNode => "admin_test_remote_node",
            Self::AdminCreateRemoteNodeEnrollmentToken => {
                "admin_create_remote_node_enrollment_token"
            }
            Self::AdminCreateRemoteStorageTarget => "admin_create_remote_storage_target",
            Self::AdminUpdateRemoteStorageTarget => "admin_update_remote_storage_target",
            Self::AdminDeleteRemoteStorageTarget => "admin_delete_remote_storage_target",
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
            Self::ServerStart => "server_start",
            Self::ServerShutdown => "server_shutdown",
            Self::TeamArchive => "team_archive",
            Self::TeamCleanupExpired => "team_cleanup_expired",
            Self::TeamCreate => "team_create",
            Self::TeamMemberAdd => "team_member_add",
            Self::TeamMemberRemove => "team_member_remove",
            Self::TeamMemberUpdate => "team_member_update",
            Self::TeamRestore => "team_restore",
            Self::TeamUpdate => "team_update",
            Self::TaskRetry => "task_retry",
            Self::TaskCancelRequested => "task_cancel_requested",
            Self::ArchiveCompress => "archive_compress",
            Self::ArchiveExtract => "archive_extract",
            Self::ArchiveDownload => "archive_download",
            Self::OfflineDownload => "offline_download",
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
            Self::TeamWebdavAccountCreate => "team_webdav_account_create",
            Self::TeamWebdavAccountDelete => "team_webdav_account_delete",
            Self::TeamWebdavAccountToggle => "team_webdav_account_toggle",
            Self::WebdavAccountAccessModeUpdate => "webdav_account_access_mode_update",
            Self::TeamWebdavAccountAccessModeUpdate => "team_webdav_account_access_mode_update",
            Self::UserChangePassword => "user_change_password",
            Self::UserConfirmPasswordReset => "user_confirm_password_reset",
            Self::UserConfirmEmailChange => "user_confirm_email_change",
            Self::UserConfirmRegistration => "user_confirm_registration",
            Self::UserLogin => "user_login",
            Self::UserLogout => "user_logout",
            Self::UserMfaEnable => "user_mfa_enable",
            Self::UserMfaDisable => "user_mfa_disable",
            Self::UserMfaRecoveryCodesRegenerate => "user_mfa_recovery_codes_regenerate",
            Self::UserMfaEmailCodeSend => "user_mfa_email_code_send",
            Self::UserMfaChallengeSuccess => "user_mfa_challenge_success",
            Self::UserMfaChallengeFailed => "user_mfa_challenge_failed",
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
            Self::FollowerBindingSync => "follower_binding_sync",
            Self::FollowerObjectRead => "follower_object_read",
            Self::FollowerObjectWrite => "follower_object_write",
            Self::FollowerObjectDelete => "follower_object_delete",
            Self::FollowerObjectCompose => "follower_object_compose",
            Self::FollowerStorageTargetCreate => "follower_storage_target_create",
            Self::FollowerStorageTargetUpdate => "follower_storage_target_update",
            Self::FollowerStorageTargetDelete => "follower_storage_target_delete",
            Self::MailSend => "mail_send",
            Self::MailDeliveryFailed => "mail_delivery_failed",
            Self::AdminCreateInvitation => "admin_create_invitation",
            Self::AdminRevokeInvitation => "admin_revoke_invitation",
            Self::TagCreate => "tag_create",
            Self::TagUpdate => "tag_update",
            Self::TagDelete => "tag_delete",
            Self::TagAttach => "tag_attach",
            Self::TagDetach => "tag_detach",
            Self::RemoteNodeConnected => "remote_node_connected",
            Self::RemoteNodeGracefulDisconnect => "remote_node_graceful_disconnect",
            Self::RemoteNodeUnexpectedDisconnect => "remote_node_unexpected_disconnect",
            Self::RemoteNodeHeartbeatTimeout => "remote_node_heartbeat_timeout",
            Self::AdminCreateStoragePolicyForcedPurgeTask => {
                "admin_create_storage_policy_forced_purge_task"
            }
            Self::FolderIconChange => "folder_icon_change",
            Self::AdminWorkspaceInspect => "admin_workspace_inspect",
            Self::AdminFileDownload => "admin_file_download",
            Self::AdminRevisionDownload => "admin_revision_download",
        }
    }

    /// Parse current audit identifiers without accepting retired ingress aliases.
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
            "admin_reset_user_mfa" => Some(Self::AdminResetUserMfa),
            "admin_update_team" => Some(Self::AdminUpdateTeam),
            "admin_update_user" => Some(Self::AdminUpdateUser),
            "admin_delete_policy_group" => Some(Self::AdminDeletePolicyGroup),
            "admin_migrate_policy_group_users" => Some(Self::AdminMigratePolicyGroupUsers),
            "admin_update_policy_group" => Some(Self::AdminUpdatePolicyGroup),
            "admin_create_policy" => Some(Self::AdminCreatePolicy),
            "admin_update_policy" => Some(Self::AdminUpdatePolicy),
            "admin_delete_policy" => Some(Self::AdminDeletePolicy),
            "admin_trigger_storage_action" => Some(Self::AdminTriggerStorageAction),
            "admin_delete_config" => Some(Self::AdminDeleteConfig),
            "admin_delete_share" => Some(Self::AdminDeleteShare),
            "admin_force_unlock" => Some(Self::AdminForceUnlock),
            "admin_cleanup_expired_locks" => Some(Self::AdminCleanupExpiredLocks),
            "admin_cleanup_tasks" => Some(Self::AdminCleanupTasks),
            "admin_create_blob_maintenance_task" => Some(Self::AdminCreateBlobMaintenanceTask),
            "admin_create_remote_node" => Some(Self::AdminCreateRemoteNode),
            "admin_update_remote_node" => Some(Self::AdminUpdateRemoteNode),
            "admin_delete_remote_node" => Some(Self::AdminDeleteRemoteNode),
            "admin_test_remote_node" => Some(Self::AdminTestRemoteNode),
            "admin_create_remote_node_enrollment_token" => {
                Some(Self::AdminCreateRemoteNodeEnrollmentToken)
            }
            "admin_create_remote_storage_target" => Some(Self::AdminCreateRemoteStorageTarget),
            "admin_update_remote_storage_target" => Some(Self::AdminUpdateRemoteStorageTarget),
            "admin_delete_remote_storage_target" => Some(Self::AdminDeleteRemoteStorageTarget),
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
            "server_start" => Some(Self::ServerStart),
            "server_shutdown" => Some(Self::ServerShutdown),
            "team_archive" => Some(Self::TeamArchive),
            "team_cleanup_expired" => Some(Self::TeamCleanupExpired),
            "team_create" => Some(Self::TeamCreate),
            "team_member_add" => Some(Self::TeamMemberAdd),
            "team_member_remove" => Some(Self::TeamMemberRemove),
            "team_member_update" => Some(Self::TeamMemberUpdate),
            "team_restore" => Some(Self::TeamRestore),
            "team_update" => Some(Self::TeamUpdate),
            "task_retry" => Some(Self::TaskRetry),
            "task_cancel_requested" => Some(Self::TaskCancelRequested),
            "archive_compress" => Some(Self::ArchiveCompress),
            "archive_extract" => Some(Self::ArchiveExtract),
            "archive_download" => Some(Self::ArchiveDownload),
            "offline_download" => Some(Self::OfflineDownload),
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
            "team_webdav_account_create" => Some(Self::TeamWebdavAccountCreate),
            "team_webdav_account_delete" => Some(Self::TeamWebdavAccountDelete),
            "team_webdav_account_toggle" => Some(Self::TeamWebdavAccountToggle),
            "webdav_account_access_mode_update" => Some(Self::WebdavAccountAccessModeUpdate),
            "team_webdav_account_access_mode_update" => {
                Some(Self::TeamWebdavAccountAccessModeUpdate)
            }
            "user_change_password" => Some(Self::UserChangePassword),
            "user_confirm_password_reset" => Some(Self::UserConfirmPasswordReset),
            "user_confirm_email_change" => Some(Self::UserConfirmEmailChange),
            "user_confirm_registration" => Some(Self::UserConfirmRegistration),
            "user_login" => Some(Self::UserLogin),
            "user_logout" => Some(Self::UserLogout),
            "user_mfa_enable" => Some(Self::UserMfaEnable),
            "user_mfa_disable" => Some(Self::UserMfaDisable),
            "user_mfa_recovery_codes_regenerate" => Some(Self::UserMfaRecoveryCodesRegenerate),
            "user_mfa_email_code_send" => Some(Self::UserMfaEmailCodeSend),
            "user_mfa_challenge_success" => Some(Self::UserMfaChallengeSuccess),
            "user_mfa_challenge_failed" => Some(Self::UserMfaChallengeFailed),
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
            "follower_binding_sync" => Some(Self::FollowerBindingSync),
            "follower_object_read" => Some(Self::FollowerObjectRead),
            "follower_object_write" => Some(Self::FollowerObjectWrite),
            "follower_object_delete" => Some(Self::FollowerObjectDelete),
            "follower_object_compose" => Some(Self::FollowerObjectCompose),
            "follower_storage_target_create" => Some(Self::FollowerStorageTargetCreate),
            "follower_storage_target_update" => Some(Self::FollowerStorageTargetUpdate),
            "follower_storage_target_delete" => Some(Self::FollowerStorageTargetDelete),
            "mail_send" => Some(Self::MailSend),
            "mail_delivery_failed" => Some(Self::MailDeliveryFailed),
            "admin_create_invitation" => Some(Self::AdminCreateInvitation),
            "admin_revoke_invitation" => Some(Self::AdminRevokeInvitation),
            "tag_create" => Some(Self::TagCreate),
            "tag_update" => Some(Self::TagUpdate),
            "tag_delete" => Some(Self::TagDelete),
            "tag_attach" => Some(Self::TagAttach),
            "tag_detach" => Some(Self::TagDetach),
            "remote_node_connected" => Some(Self::RemoteNodeConnected),
            "remote_node_graceful_disconnect" => Some(Self::RemoteNodeGracefulDisconnect),
            "remote_node_unexpected_disconnect" => Some(Self::RemoteNodeUnexpectedDisconnect),
            "remote_node_heartbeat_timeout" => Some(Self::RemoteNodeHeartbeatTimeout),
            "admin_create_storage_policy_forced_purge_task" => {
                Some(Self::AdminCreateStoragePolicyForcedPurgeTask)
            }
            "folder_icon_change" => Some(Self::FolderIconChange),
            "admin_workspace_inspect" => Some(Self::AdminWorkspaceInspect),
            "admin_file_download" => Some(Self::AdminFileDownload),
            "admin_revision_download" => Some(Self::AdminRevisionDownload),

            _ => None,
        }
    }

    /// Group actions for the administrator configuration selector and its translated labels.
    pub const fn group(self) -> &'static str {
        match self {
            Self::AdminWorkspaceInspect
            | Self::AdminFileDownload
            | Self::AdminRevisionDownload
            | Self::AdminCreateUser
            | Self::AdminForceDeleteUser
            | Self::AdminCreateTeam
            | Self::AdminCreatePolicyGroup
            | Self::AdminArchiveTeam
            | Self::AdminRestoreTeam
            | Self::AdminRevokeUserSessions
            | Self::AdminResetUserPassword
            | Self::AdminResetUserMfa
            | Self::AdminUpdateTeam
            | Self::AdminUpdateUser
            | Self::AdminDeletePolicyGroup
            | Self::AdminMigratePolicyGroupUsers
            | Self::AdminUpdatePolicyGroup
            | Self::AdminCreatePolicy
            | Self::AdminUpdatePolicy
            | Self::AdminDeletePolicy
            | Self::AdminTriggerStorageAction
            | Self::AdminDeleteConfig
            | Self::AdminDeleteShare
            | Self::AdminForceUnlock
            | Self::AdminCleanupExpiredLocks
            | Self::AdminCleanupTasks
            | Self::AdminCreateBlobMaintenanceTask
            | Self::AdminCreateInvitation
            | Self::AdminRevokeInvitation
            | Self::AdminCreateStoragePolicyForcedPurgeTask => "admin",
            Self::AdminCreateRemoteNode
            | Self::AdminUpdateRemoteNode
            | Self::AdminDeleteRemoteNode
            | Self::AdminTestRemoteNode
            | Self::AdminCreateRemoteNodeEnrollmentToken
            | Self::RemoteEnrollmentRedeem
            | Self::RemoteEnrollmentAck => "remote",
            Self::AdminCreateRemoteStorageTarget
            | Self::AdminUpdateRemoteStorageTarget
            | Self::AdminDeleteRemoteStorageTarget => "remote_storage_target",
            Self::AdminCreateExternalAuthProvider
            | Self::AdminUpdateExternalAuthProvider
            | Self::AdminDeleteExternalAuthProvider
            | Self::AdminTestExternalAuthProvider
            | Self::UserExternalAuthLogin
            | Self::UserExternalAuthLink
            | Self::UserExternalAuthUnlink => "external_auth",
            Self::BatchCopy | Self::BatchDelete | Self::BatchMove => "batch",
            Self::ConfigActionExecute | Self::ConfigUpdate => "config",
            Self::FileCopy
            | Self::FileCreate
            | Self::FileDelete
            | Self::FileDownload
            | Self::FileDirectLinkCreate
            | Self::FileEdit
            | Self::FileMove
            | Self::FileRename
            | Self::FileUpload
            | Self::FilePreviewLinkCreate
            | Self::FileWopiOpen
            | Self::FileUploadCancel
            | Self::FileRestore
            | Self::FilePurge
            | Self::FileLock
            | Self::FileUnlock
            | Self::FileVersionRestore
            | Self::FileVersionDelete => "file",
            Self::FolderCopy
            | Self::FolderCreate
            | Self::FolderDelete
            | Self::FolderMove
            | Self::FolderPolicyChange
            | Self::FolderRename
            | Self::FolderRestore
            | Self::FolderPurge
            | Self::FolderLock
            | Self::FolderUnlock
            | Self::FolderIconChange => "folder",
            Self::PropertySet | Self::PropertyDelete => "property",
            Self::ShareBatchDelete | Self::ShareCreate | Self::ShareDelete | Self::ShareUpdate => {
                "share"
            }
            Self::SystemSetup | Self::ServerStart | Self::ServerShutdown => "system",
            Self::TeamArchive
            | Self::TeamCleanupExpired
            | Self::TeamCreate
            | Self::TeamMemberAdd
            | Self::TeamMemberRemove
            | Self::TeamMemberUpdate
            | Self::TeamRestore
            | Self::TeamUpdate => "team",
            Self::TaskRetry | Self::TaskCancelRequested => "task",
            Self::ArchiveCompress | Self::ArchiveExtract | Self::ArchiveDownload => "archive",
            Self::OfflineDownload => "task",
            Self::TrashPurgeAll => "trash",
            Self::UserRevokeOtherSessions
            | Self::UserRevokeSession
            | Self::UserUpdatePreferences
            | Self::UserUpdateProfile
            | Self::UserUploadAvatar
            | Self::UserSetAvatarSource
            | Self::UserUpdateWopiInfo => "user",
            Self::WebdavAccountCreate
            | Self::WebdavAccountDelete
            | Self::WebdavAccountToggle
            | Self::TeamWebdavAccountCreate
            | Self::TeamWebdavAccountDelete
            | Self::TeamWebdavAccountToggle
            | Self::WebdavAccountAccessModeUpdate
            | Self::TeamWebdavAccountAccessModeUpdate => "webdav",
            Self::UserChangePassword
            | Self::UserConfirmPasswordReset
            | Self::UserConfirmEmailChange
            | Self::UserConfirmRegistration
            | Self::UserLogin
            | Self::UserLogout
            | Self::UserMfaEnable
            | Self::UserMfaDisable
            | Self::UserMfaRecoveryCodesRegenerate
            | Self::UserMfaEmailCodeSend
            | Self::UserMfaChallengeSuccess
            | Self::UserMfaChallengeFailed
            | Self::UserPasskeyDelete
            | Self::UserPasskeyLogin
            | Self::UserPasskeyRegister
            | Self::UserPasskeyRename
            | Self::UserRefreshTokenReuseDetected
            | Self::UserRequestEmailChange
            | Self::UserRequestPasswordReset
            | Self::UserRegister
            | Self::UserResendEmailChange
            | Self::UserResendRegistration => "auth",
            Self::FollowerBindingSync => "remote",
            Self::FollowerObjectRead
            | Self::FollowerObjectWrite
            | Self::FollowerObjectDelete
            | Self::FollowerObjectCompose => "remote_storage",
            Self::FollowerStorageTargetCreate
            | Self::FollowerStorageTargetUpdate
            | Self::FollowerStorageTargetDelete => "remote_storage_target",
            Self::MailSend | Self::MailDeliveryFailed => "mail",
            Self::TagCreate
            | Self::TagUpdate
            | Self::TagDelete
            | Self::TagAttach
            | Self::TagDetach => "tag",
            Self::RemoteNodeConnected
            | Self::RemoteNodeGracefulDisconnect
            | Self::RemoteNodeUnexpectedDisconnect
            | Self::RemoteNodeHeartbeatTimeout => "remote",
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

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::{AUDIT_ENTITY_TYPE_NAMES, AuditAction, AuditEntityType};

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

    /// Verify remote target audit contract uses storage target wire values.
    #[test]
    fn remote_target_audit_contract_uses_storage_target_wire_values() {
        use sea_orm::ActiveEnum;
        for (action, index, name) in [
            (
                AuditAction::AdminCreateRemoteStorageTarget,
                29,
                "admin_create_remote_storage_target",
            ),
            (
                AuditAction::AdminUpdateRemoteStorageTarget,
                30,
                "admin_update_remote_storage_target",
            ),
            (
                AuditAction::AdminDeleteRemoteStorageTarget,
                31,
                "admin_delete_remote_storage_target",
            ),
            (
                AuditAction::FollowerStorageTargetCreate,
                137,
                "follower_storage_target_create",
            ),
            (
                AuditAction::FollowerStorageTargetUpdate,
                138,
                "follower_storage_target_update",
            ),
            (
                AuditAction::FollowerStorageTargetDelete,
                139,
                "follower_storage_target_delete",
            ),
        ] {
            assert_eq!(action.index(), index);
            assert_eq!(action.as_str(), name);
            assert_eq!(action.to_value(), name);
            assert_eq!(
                serde_json::to_value(action).unwrap(),
                serde_json::json!(name)
            );
            assert_eq!(
                serde_json::from_str::<AuditAction>(&format!("\"{name}\"")).unwrap(),
                action
            );
        }
        assert_eq!(
            AuditEntityType::RemoteStorageTarget.as_str(),
            "remote_storage_target"
        );
        assert_eq!(
            serde_json::to_value(AuditEntityType::RemoteStorageTarget).unwrap(),
            serde_json::json!("remote_storage_target")
        );
    }

    /// Verify retired ingress names are not runtime aliases.
    #[test]
    fn retired_ingress_names_are_not_runtime_aliases() {
        for old in [
            "admin_create_remote_ingress_profile",
            "admin_update_remote_ingress_profile",
            "admin_delete_remote_ingress_profile",
            "follower_ingress_profile_create",
            "follower_ingress_profile_update",
            "follower_ingress_profile_delete",
        ] {
            assert!(AuditAction::from_str_name(old).is_none());
            assert!(serde_json::from_value::<AuditAction>(serde_json::json!(old)).is_err());
        }
        assert!(AuditEntityType::from_str_name("remote_ingress_profile").is_none());
    }

    #[test]
    fn audit_action_all_covers_every_stable_index() {
        assert_eq!(AuditAction::COUNT, AuditAction::ALL.len());
        let mut indexes = HashSet::new();
        let mut names = HashSet::new();

        for (expected_index, action) in AuditAction::ALL.iter().copied().enumerate() {
            assert_eq!(action.index(), expected_index);
            assert!(indexes.insert(action.index()));
            assert!(names.insert(action.as_str()));
            assert_eq!(AuditAction::from_str_name(action.as_str()), Some(action));
            assert_eq!(
                serde_json::to_value(action).expect("audit action serializes"),
                serde_json::json!(action.as_str())
            );
            assert_eq!(
                serde_json::from_value::<AuditAction>(serde_json::json!(action.as_str()))
                    .expect("audit action deserializes"),
                action
            );
        }

        assert_eq!(indexes.len(), AuditAction::COUNT);
        assert_eq!(names.len(), AuditAction::COUNT);
        assert_eq!(AuditAction::from_str_name("unknown"), None);
        assert!(serde_json::from_value::<AuditAction>(serde_json::json!("unknown")).is_err());
    }
}
