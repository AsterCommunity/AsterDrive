use actix_web::test as actix_test;
use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter};
use std::sync::atomic::{AtomicUsize, Ordering};

use super::AuditAction;
use super::context::{
    AuditContext, AuditRequestInfo, MAX_AUDIT_IP_ADDRESS_LEN, MAX_AUDIT_USER_AGENT_LEN,
    bounded_audit_value,
};
use crate::entities::audit_log;

async fn in_memory_db() -> sea_orm::DatabaseConnection {
    let db = sea_orm::Database::connect("sqlite::memory:")
        .await
        .expect("in-memory db should connect");
    migration::Migrator::up(&db, None)
        .await
        .expect("migrations should run");
    db
}

#[test]
fn bounded_audit_value_truncates_without_splitting_utf8() {
    assert_eq!(bounded_audit_value("abcdef", 3), "abc");
    assert_eq!(bounded_audit_value("猫猫猫", 4), "猫");
}

#[test]
fn request_audit_info_truncates_user_controlled_headers() {
    let long_ip = "1".repeat(MAX_AUDIT_IP_ADDRESS_LEN + 32);
    let long_user_agent = "a".repeat(MAX_AUDIT_USER_AGENT_LEN + 32);
    let req = actix_test::TestRequest::default()
        .peer_addr("127.0.0.1:12345".parse().unwrap())
        .insert_header(("X-Forwarded-For", long_ip.as_str()))
        .insert_header(("User-Agent", long_user_agent.as_str()))
        .to_http_request();

    let info = AuditRequestInfo::from_request(&req);

    assert_eq!(
        info.ip_address.as_deref(),
        Some(&long_ip[..MAX_AUDIT_IP_ADDRESS_LEN])
    );
    assert_eq!(
        info.user_agent.as_deref(),
        Some(&long_user_agent[..MAX_AUDIT_USER_AGENT_LEN])
    );
}

#[test]
fn trusted_proxy_audit_info_uses_x_forwarded_for_only_from_trusted_peer() {
    let req = actix_test::TestRequest::default()
        .peer_addr("10.0.0.10:12345".parse().unwrap())
        .insert_header((
            "X-Forwarded-For",
            "203.0.113.7:54321, [2001:db8::1]:443, 10.0.0.10",
        ))
        .to_http_request();

    let info =
        AuditRequestInfo::from_request_with_trusted_proxies(&req, &["10.0.0.0/8".to_string()]);

    assert_eq!(info.ip_address.as_deref(), Some("203.0.113.7"));
}

#[test]
fn trusted_proxy_audit_info_accepts_bracketed_ipv6_with_port() {
    let req = actix_test::TestRequest::default()
        .peer_addr("10.0.0.10:12345".parse().unwrap())
        .insert_header(("X-Forwarded-For", "[2001:db8::1]:443, 10.0.0.10"))
        .to_http_request();

    let info =
        AuditRequestInfo::from_request_with_trusted_proxies(&req, &["10.0.0.0/8".to_string()]);

    assert_eq!(info.ip_address.as_deref(), Some("2001:db8::1"));
}

#[test]
fn trusted_proxy_audit_info_ignores_spoofed_x_forwarded_for_from_untrusted_peer() {
    let req = actix_test::TestRequest::default()
        .peer_addr("198.51.100.4:12345".parse().unwrap())
        .insert_header(("X-Forwarded-For", "[2001:db8::1]:443, 203.0.113.7:54321"))
        .to_http_request();

    let info =
        AuditRequestInfo::from_request_with_trusted_proxies(&req, &["10.0.0.0/8".to_string()]);

    assert_eq!(info.ip_address.as_deref(), Some("198.51.100.4"));
}

#[test]
fn audit_action_strings_match_existing_contract() {
    let cases = [
        (AuditAction::AdminCreateUser, "admin_create_user"),
        (AuditAction::AdminForceDeleteUser, "admin_force_delete_user"),
        (AuditAction::AdminCreateTeam, "admin_create_team"),
        (AuditAction::AdminCreatePolicy, "admin_create_policy"),
        (AuditAction::AdminUpdatePolicy, "admin_update_policy"),
        (AuditAction::AdminDeletePolicy, "admin_delete_policy"),
        (
            AuditAction::AdminTriggerStorageAction,
            "admin_trigger_storage_action",
        ),
        (
            AuditAction::AdminCreatePolicyGroup,
            "admin_create_policy_group",
        ),
        (AuditAction::AdminArchiveTeam, "admin_archive_team"),
        (AuditAction::AdminRestoreTeam, "admin_restore_team"),
        (
            AuditAction::AdminDeletePolicyGroup,
            "admin_delete_policy_group",
        ),
        (
            AuditAction::AdminMigratePolicyGroupUsers,
            "admin_migrate_policy_group_users",
        ),
        (
            AuditAction::AdminRevokeUserSessions,
            "admin_revoke_user_sessions",
        ),
        (
            AuditAction::AdminResetUserPassword,
            "admin_reset_user_password",
        ),
        (AuditAction::AdminUpdateTeam, "admin_update_team"),
        (
            AuditAction::AdminUpdatePolicyGroup,
            "admin_update_policy_group",
        ),
        (AuditAction::AdminUpdateUser, "admin_update_user"),
        (AuditAction::AdminDeleteConfig, "admin_delete_config"),
        (AuditAction::AdminDeleteShare, "admin_delete_share"),
        (AuditAction::AdminForceUnlock, "admin_force_unlock"),
        (
            AuditAction::AdminCleanupExpiredLocks,
            "admin_cleanup_expired_locks",
        ),
        (AuditAction::AdminCleanupTasks, "admin_cleanup_tasks"),
        (
            AuditAction::AdminCreateBlobMaintenanceTask,
            "admin_create_blob_maintenance_task",
        ),
        (
            AuditAction::AdminCreateRemoteNode,
            "admin_create_remote_node",
        ),
        (
            AuditAction::AdminUpdateRemoteNode,
            "admin_update_remote_node",
        ),
        (
            AuditAction::AdminDeleteRemoteNode,
            "admin_delete_remote_node",
        ),
        (AuditAction::AdminTestRemoteNode, "admin_test_remote_node"),
        (
            AuditAction::AdminCreateRemoteNodeEnrollmentToken,
            "admin_create_remote_node_enrollment_token",
        ),
        (
            AuditAction::AdminCreateRemoteIngressProfile,
            "admin_create_remote_ingress_profile",
        ),
        (
            AuditAction::AdminUpdateRemoteIngressProfile,
            "admin_update_remote_ingress_profile",
        ),
        (
            AuditAction::AdminDeleteRemoteIngressProfile,
            "admin_delete_remote_ingress_profile",
        ),
        (
            AuditAction::AdminCreateExternalAuthProvider,
            "admin_create_external_auth_provider",
        ),
        (
            AuditAction::AdminUpdateExternalAuthProvider,
            "admin_update_external_auth_provider",
        ),
        (
            AuditAction::AdminDeleteExternalAuthProvider,
            "admin_delete_external_auth_provider",
        ),
        (
            AuditAction::AdminTestExternalAuthProvider,
            "admin_test_external_auth_provider",
        ),
        (AuditAction::BatchCopy, "batch_copy"),
        (AuditAction::BatchDelete, "batch_delete"),
        (AuditAction::BatchMove, "batch_move"),
        (AuditAction::ConfigActionExecute, "config_action_execute"),
        (AuditAction::ConfigUpdate, "config_update"),
        (AuditAction::FileCopy, "file_copy"),
        (AuditAction::FileCreate, "file_create"),
        (AuditAction::FileDelete, "file_delete"),
        (AuditAction::FileDownload, "file_download"),
        (AuditAction::FileDirectLinkCreate, "file_direct_link_create"),
        (AuditAction::FileEdit, "file_edit"),
        (AuditAction::FileMove, "file_move"),
        (AuditAction::FileRename, "file_rename"),
        (AuditAction::FileUpload, "file_upload"),
        (
            AuditAction::FilePreviewLinkCreate,
            "file_preview_link_create",
        ),
        (AuditAction::FileWopiOpen, "file_wopi_open"),
        (AuditAction::FileUploadCancel, "file_upload_cancel"),
        (AuditAction::FileRestore, "file_restore"),
        (AuditAction::FilePurge, "file_purge"),
        (AuditAction::FileLock, "file_lock"),
        (AuditAction::FileUnlock, "file_unlock"),
        (AuditAction::FileVersionRestore, "file_version_restore"),
        (AuditAction::FileVersionDelete, "file_version_delete"),
        (AuditAction::FolderCopy, "folder_copy"),
        (AuditAction::FolderCreate, "folder_create"),
        (AuditAction::FolderDelete, "folder_delete"),
        (AuditAction::FolderMove, "folder_move"),
        (AuditAction::FolderPolicyChange, "folder_policy_change"),
        (AuditAction::FolderRename, "folder_rename"),
        (AuditAction::FolderRestore, "folder_restore"),
        (AuditAction::FolderPurge, "folder_purge"),
        (AuditAction::FolderLock, "folder_lock"),
        (AuditAction::FolderUnlock, "folder_unlock"),
        (AuditAction::PropertySet, "property_set"),
        (AuditAction::PropertyDelete, "property_delete"),
        (AuditAction::ShareBatchDelete, "share_batch_delete"),
        (AuditAction::ShareCreate, "share_create"),
        (AuditAction::ShareDelete, "share_delete"),
        (AuditAction::ShareUpdate, "share_update"),
        (AuditAction::SystemSetup, "system_setup"),
        (AuditAction::ServerStart, "server_start"),
        (AuditAction::ServerShutdown, "server_shutdown"),
        (AuditAction::TeamArchive, "team_archive"),
        (AuditAction::TeamCleanupExpired, "team_cleanup_expired"),
        (AuditAction::TeamCreate, "team_create"),
        (AuditAction::TeamMemberAdd, "team_member_add"),
        (AuditAction::TeamMemberRemove, "team_member_remove"),
        (AuditAction::TeamMemberUpdate, "team_member_update"),
        (AuditAction::TeamRestore, "team_restore"),
        (AuditAction::TeamUpdate, "team_update"),
        (AuditAction::TaskRetry, "task_retry"),
        (AuditAction::ArchiveCompress, "archive_compress"),
        (AuditAction::ArchiveExtract, "archive_extract"),
        (AuditAction::ArchiveDownload, "archive_download"),
        (AuditAction::OfflineDownload, "offline_download"),
        (AuditAction::TrashPurgeAll, "trash_purge_all"),
        (
            AuditAction::RemoteEnrollmentRedeem,
            "remote_enrollment_redeem",
        ),
        (AuditAction::RemoteEnrollmentAck, "remote_enrollment_ack"),
        (
            AuditAction::UserRevokeOtherSessions,
            "user_revoke_other_sessions",
        ),
        (AuditAction::UserRevokeSession, "user_revoke_session"),
        (
            AuditAction::UserUpdatePreferences,
            "user_update_preferences",
        ),
        (AuditAction::UserUpdateProfile, "user_update_profile"),
        (AuditAction::UserUploadAvatar, "user_upload_avatar"),
        (AuditAction::UserSetAvatarSource, "user_set_avatar_source"),
        (AuditAction::UserUpdateWopiInfo, "user_update_wopi_info"),
        (AuditAction::WebdavAccountCreate, "webdav_account_create"),
        (AuditAction::WebdavAccountDelete, "webdav_account_delete"),
        (AuditAction::WebdavAccountToggle, "webdav_account_toggle"),
        (
            AuditAction::TeamWebdavAccountCreate,
            "team_webdav_account_create",
        ),
        (
            AuditAction::TeamWebdavAccountDelete,
            "team_webdav_account_delete",
        ),
        (
            AuditAction::TeamWebdavAccountToggle,
            "team_webdav_account_toggle",
        ),
        (AuditAction::UserChangePassword, "user_change_password"),
        (
            AuditAction::UserConfirmPasswordReset,
            "user_confirm_password_reset",
        ),
        (
            AuditAction::UserConfirmEmailChange,
            "user_confirm_email_change",
        ),
        (
            AuditAction::UserConfirmRegistration,
            "user_confirm_registration",
        ),
        (AuditAction::UserLogin, "user_login"),
        (AuditAction::UserLogout, "user_logout"),
        (
            AuditAction::UserExternalAuthLogin,
            "user_external_auth_login",
        ),
        (AuditAction::UserExternalAuthLink, "user_external_auth_link"),
        (
            AuditAction::UserExternalAuthUnlink,
            "user_external_auth_unlink",
        ),
        (
            AuditAction::UserRefreshTokenReuseDetected,
            "user_refresh_token_reuse_detected",
        ),
        (
            AuditAction::UserRequestEmailChange,
            "user_request_email_change",
        ),
        (
            AuditAction::UserRequestPasswordReset,
            "user_request_password_reset",
        ),
        (AuditAction::UserRegister, "user_register"),
        (
            AuditAction::UserResendEmailChange,
            "user_resend_email_change",
        ),
        (
            AuditAction::UserResendRegistration,
            "user_resend_registration",
        ),
        (AuditAction::FollowerBindingSync, "follower_binding_sync"),
        (AuditAction::FollowerObjectRead, "follower_object_read"),
        (AuditAction::FollowerObjectWrite, "follower_object_write"),
        (AuditAction::FollowerObjectDelete, "follower_object_delete"),
        (
            AuditAction::FollowerObjectCompose,
            "follower_object_compose",
        ),
        (
            AuditAction::FollowerIngressProfileCreate,
            "follower_ingress_profile_create",
        ),
        (
            AuditAction::FollowerIngressProfileUpdate,
            "follower_ingress_profile_update",
        ),
        (
            AuditAction::FollowerIngressProfileDelete,
            "follower_ingress_profile_delete",
        ),
        (AuditAction::MailSend, "mail_send"),
        (AuditAction::MailDeliveryFailed, "mail_delivery_failed"),
    ];

    for (action, expected) in cases {
        assert_eq!(action.as_str(), expected);
        assert_eq!(action.as_ref(), expected);
        assert_eq!(action.to_string(), expected);
        assert_eq!(AuditAction::from_str_name(expected), Some(action));
    }
}

#[tokio::test]
async fn log_writes_synchronously_without_global_manager() {
    let db = in_memory_db().await;

    let runtime_config = std::sync::Arc::new(crate::config::RuntimeConfig::new());
    runtime_config
        .reload(&db)
        .await
        .expect("runtime config should load");
    let cache = aster_forge_cache::create_cache(&aster_forge_cache::CacheConfig {
        ..Default::default()
    })
    .await;
    let storage_change_bus = crate::services::events::storage_change::StorageChangeBus::new(
        crate::services::events::storage_change::STORAGE_CHANGE_CHANNEL_CAPACITY,
    );
    let share_download_rollback =
        crate::services::share::spawn_detached_share_download_rollback_queue(
            db.clone(),
            crate::config::operations::DEFAULT_SHARE_DOWNLOAD_ROLLBACK_QUEUE_CAPACITY,
        );
    let state = crate::runtime::PrimaryAppState {
        db_handles: aster_forge_db::DbHandles::single(db.clone()),
        driver_registry: std::sync::Arc::new(crate::storage::DriverRegistry::noop()),
        runtime_config,
        policy_snapshot: std::sync::Arc::new(crate::storage::PolicySnapshot::new()),
        config: std::sync::Arc::new(crate::config::Config::default()),
        cache,
        config_sync: aster_forge_config::ConfigSyncRuntime::disabled_for_test("aster_drive"),
        metrics: crate::metrics::NoopMetrics::arc(),
        mail_sender: aster_forge_mail::memory_sender(),
        storage_change_bus,
        share_download_rollback,
        background_task_dispatch_wakeup:
            crate::runtime::PrimaryAppState::new_background_task_dispatch_wakeup(),
        remote_protocol: crate::runtime::PrimaryAppState::new_remote_protocol(),
        upload_runtime: crate::runtime::PrimaryAppState::new_upload_runtime(),
    };

    super::log(
        &state,
        &AuditContext {
            user_id: 42,
            ip_address: None,
            user_agent: None,
        },
        AuditAction::FileUpload,
        crate::services::ops::audit::AuditEntityType::File,
        Some(7),
        Some("report.txt"),
        None,
    )
    .await;

    let count = audit_log::Entity::find()
        .filter(audit_log::Column::Action.eq(AuditAction::FileUpload))
        .count(&db)
        .await
        .expect("audit query should succeed");
    assert_eq!(count, 1);
}

#[tokio::test]
async fn follower_state_can_record_allowed_audit_log() {
    let db = in_memory_db().await;

    let runtime_config = std::sync::Arc::new(crate::config::RuntimeConfig::new());
    runtime_config
        .reload(&db)
        .await
        .expect("runtime config should load");
    let cache = aster_forge_cache::create_cache(&aster_forge_cache::CacheConfig {
        ..Default::default()
    })
    .await;
    let state = crate::runtime::FollowerAppState {
        db_handles: aster_forge_db::DbHandles::single(db.clone()),
        driver_registry: std::sync::Arc::new(crate::storage::DriverRegistry::noop()),
        runtime_config,
        policy_snapshot: std::sync::Arc::new(crate::storage::PolicySnapshot::new()),
        config: std::sync::Arc::new(crate::config::Config::default()),
        cache,
        config_sync: aster_forge_config::ConfigSyncRuntime::disabled_for_test("aster_drive"),
        metrics: crate::metrics::NoopMetrics::arc(),
    };

    super::log(
        &state,
        &AuditContext::system(),
        AuditAction::FollowerObjectWrite,
        crate::services::ops::audit::AuditEntityType::File,
        None,
        Some("remote/object.bin"),
        None,
    )
    .await;

    let count = audit_log::Entity::find()
        .filter(audit_log::Column::Action.eq(AuditAction::FollowerObjectWrite))
        .count(&db)
        .await
        .expect("audit query should succeed");
    assert_eq!(count, 1);
}

#[tokio::test]
async fn log_with_details_skips_details_when_action_scope_excludes_action() {
    let db = in_memory_db().await;

    let runtime_config = std::sync::Arc::new(crate::config::RuntimeConfig::new());
    runtime_config.apply(aster_forge_db::system_config::Model {
        id: 1,
        key: crate::config::audit::AUDIT_LOG_RECORDED_ACTIONS_KEY.to_string(),
        value: r#"["user_login"]"#.to_string(),
        value_type: aster_forge_config::ConfigValueType::StringEnumSet,
        requires_restart: false,
        is_sensitive: false,
        source: aster_forge_config::ConfigSource::System,
        visibility: aster_forge_config::ConfigVisibility::Private,
        namespace: String::new(),
        category: crate::config::definitions::CONFIG_CATEGORY_AUDIT.to_string(),
        description: String::new(),
        updated_at: chrono::Utc::now(),
        updated_by: None,
    });
    let cache = aster_forge_cache::create_cache(&aster_forge_cache::CacheConfig {
        ..Default::default()
    })
    .await;
    let storage_change_bus = crate::services::events::storage_change::StorageChangeBus::new(
        crate::services::events::storage_change::STORAGE_CHANGE_CHANNEL_CAPACITY,
    );
    let share_download_rollback =
        crate::services::share::spawn_detached_share_download_rollback_queue(
            db.clone(),
            crate::config::operations::DEFAULT_SHARE_DOWNLOAD_ROLLBACK_QUEUE_CAPACITY,
        );
    let state = crate::runtime::PrimaryAppState {
        db_handles: aster_forge_db::DbHandles::single(db.clone()),
        driver_registry: std::sync::Arc::new(crate::storage::DriverRegistry::noop()),
        runtime_config,
        policy_snapshot: std::sync::Arc::new(crate::storage::PolicySnapshot::new()),
        config: std::sync::Arc::new(crate::config::Config::default()),
        cache,
        config_sync: aster_forge_config::ConfigSyncRuntime::disabled_for_test("aster_drive"),
        metrics: crate::metrics::NoopMetrics::arc(),
        mail_sender: aster_forge_mail::memory_sender(),
        storage_change_bus,
        share_download_rollback,
        background_task_dispatch_wakeup:
            crate::runtime::PrimaryAppState::new_background_task_dispatch_wakeup(),
        remote_protocol: crate::runtime::PrimaryAppState::new_remote_protocol(),
        upload_runtime: crate::runtime::PrimaryAppState::new_upload_runtime(),
    };
    let calls = AtomicUsize::new(0);

    super::log_with_details(
        &state,
        &AuditContext {
            user_id: 42,
            ip_address: None,
            user_agent: None,
        },
        AuditAction::FileUpload,
        crate::services::ops::audit::AuditEntityType::File,
        Some(7),
        Some("report.txt"),
        || {
            calls.fetch_add(1, Ordering::SeqCst);
            Some(serde_json::json!({"should": "not happen"}))
        },
    )
    .await;

    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let count = audit_log::Entity::find()
        .filter(audit_log::Column::Action.eq(AuditAction::FileUpload))
        .count(&db)
        .await
        .expect("audit query should succeed");
    assert_eq!(count, 0);
}
