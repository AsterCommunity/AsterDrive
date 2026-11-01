//! 集成测试：运维 CLI。

#![cfg(feature = "operations-cli")]

use crate::common;

use actix_web::test as actix_test;
use std::process::Command;

use aster_drive::config::DatabaseConfig;
use aster_drive::db;
use aster_drive::db::repository::{
    contact_verification_token_repo, file_create_idempotency_repo, file_repo,
    follower_enrollment_session_repo, managed_follower_repo, master_binding_repo, mfa_factor_repo,
    mfa_login_flow_repo, mfa_recovery_code_repo, mfa_totp_setup_flow_repo, policy_repo,
    property_repo, storage_policy_connector_credential_repo, tag_repo, user_repo,
};
use aster_drive_migration::{CurrentMigrator, Migrator, MigratorTrait};
use aster_drive_model::entities::{
    admin_content_access, auth_session, contact_verification_token, follower_enrollment_session,
    managed_follower, master_binding, mfa_factor, mfa_login_flow, mfa_recovery_code,
    mfa_totp_setup_flow, passkey, storage_policy, user_invitation,
};
use aster_drive_model::types::{
    EntityType, MfaFirstFactor, MfaPersistentFactorMethod, RemoteDownloadStrategy,
    RemoteUploadStrategy, StoredPasskeyCredential, StoredStoragePolicyAllowedTypes, TagScopeType,
    UserInvitationStatus, VerificationChannel, VerificationPurpose,
};
use chrono::{Duration, Utc};
use sea_orm::{
    ActiveModelTrait, ConnectionTrait, DatabaseConnection, DbBackend, EntityTrait, Set, Statement,
};
use serde_json::Value;

fn aster_drive_bin() -> &'static str {
    env!("CARGO_BIN_EXE_aster_drive")
}

const MIGRATION_REMOTE_NODE_NAME: &str = "MigratedRemoteNode";
const MIGRATION_REMOTE_POLICY_NAME: &str = "MigratedRemotePolicy";
const MIGRATION_REMOTE_POLICY_CIPHERTEXT: &str = "encrypted-migration-credential";
const MIGRATION_MASTER_BINDING_NAME: &str = "MigratedMasterBinding";
const MIGRATION_MASTER_STORAGE_NAMESPACE: &str = "mb_migrate_remote_space";
const MIGRATION_TAG_NAME: &str = "Migrated Tag";
const MIGRATION_TAG_NORMALIZED_NAME: &str = "migrated tag";
const MIGRATION_TAG_COLOR: &str = "#2563eb";
const MIGRATION_TAG_PROPERTY_NAMESPACE: &str = "system.tags";
const MIGRATION_SCHEDULED_TASK_ID: &str = "aster_drive:migration_fixture";
const MIGRATION_RUNTIME_LEASE_ID: &str = "aster_drive.migration_fixture";
const MIGRATION_FILE_CREATE_KEY_HASH: &str =
    "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const MIGRATION_FILE_CREATE_FINGERPRINT: &str =
    "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";

async fn setup_database_url() -> String {
    let db_path =
        std::env::temp_dir().join(format!("asterdrive-cli-test-{}.db", uuid::Uuid::new_v4()));
    let url = format!("sqlite://{}?mode=rwc", db_path.display());
    let db = db::connect_with_metrics(
        &DatabaseConfig {
            url: url.clone().into(),
            pool_size: 1,
            retry_count: 0,
        },
        aster_drive_metrics::NoopMetrics::arc(),
    )
    .await
    .unwrap();
    Migrator::up(&db, None).await.unwrap();
    aster_drive::db::repository::config_repo::ensure_defaults_with_env(&db, &|_| None)
        .await
        .unwrap();
    db.close().await.unwrap();
    url
}

async fn setup_empty_database_url(prefix: &str) -> String {
    let db_path = std::env::temp_dir().join(format!("{prefix}-{}.db", uuid::Uuid::new_v4()));
    let url = format!("sqlite://{}?mode=rwc", db_path.display());
    let db = db::connect_with_metrics(
        &DatabaseConfig {
            url: url.clone().into(),
            pool_size: 1,
            retry_count: 0,
        },
        aster_drive_metrics::NoopMetrics::arc(),
    )
    .await
    .unwrap();
    db.close().await.unwrap();
    url
}

async fn setup_ready_database_url() -> String {
    let db_path = std::env::temp_dir().join(format!(
        "asterdrive-cli-ready-test-{}.db",
        uuid::Uuid::new_v4()
    ));
    let url = format!("sqlite://{}?mode=rwc", db_path.display());
    let _state = common::setup_with_database_url(&url).await;
    url
}

fn run_aster_drive(args: &[&str]) -> std::process::Output {
    run_aster_drive_with_env(args, &[])
}

#[test]
fn role_help_and_invalid_arguments_have_no_bootstrap_side_effects() {
    let dir = std::env::temp_dir().join(format!("asterdrive-role-cli-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    for (args, succeeds) in [
        (vec!["serve", "--help"], true),
        (vec!["--help"], true),
        (vec!["--version"], true),
        (vec!["serve", "invalid"], false),
        (vec!["serve", "api", "worker"], false),
    ] {
        let output = run_aster_drive_with_env_in_dir(&args, &[], &dir);
        assert_eq!(output.status.success(), succeeds, "{output:?}");
        assert!(
            !dir.join("data").exists(),
            "CLI parsing must precede configuration/database initialization"
        );
    }
    std::fs::remove_dir(&dir).unwrap();
}

fn run_aster_drive_with_env(args: &[&str], envs: &[(&str, &str)]) -> std::process::Output {
    Command::new(aster_drive_bin())
        .args(args)
        .envs(envs.iter().copied())
        .output()
        .expect("aster_drive binary should run")
}

fn run_aster_drive_with_env_in_dir(
    args: &[&str],
    envs: &[(&str, &str)],
    current_dir: &std::path::Path,
) -> std::process::Output {
    Command::new(aster_drive_bin())
        .args(args)
        .envs(envs.iter().copied())
        .current_dir(current_dir)
        .output()
        .expect("aster_drive binary should run")
}

fn redact_database_url(database_url: &str) -> String {
    if database_url == "sqlite::memory:" {
        return database_url.to_string();
    }

    if database_url.starts_with("sqlite:") {
        let Some(path_and_query) = database_url.strip_prefix("sqlite://") else {
            return database_url.to_string();
        };
        let (path, query) = path_and_query
            .split_once('?')
            .map_or((path_and_query, None), |(path, query)| (path, Some(query)));
        let filename = std::path::Path::new(path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(path);
        return match query {
            Some(query) => format!("sqlite:///.../{filename}?{query}"),
            None => format!("sqlite:///.../{filename}"),
        };
    }

    let Some((scheme, rest)) = database_url.split_once("://") else {
        return database_url.to_string();
    };
    let Some((_authority, suffix)) = rest.rsplit_once('@') else {
        return database_url.to_string();
    };

    format!("{scheme}://***@{suffix}")
}

async fn scalar_i64(db: &DatabaseConnection, backend: DbBackend, sql: &str) -> i64 {
    db.query_one_raw(Statement::from_string(backend, sql))
        .await
        .unwrap()
        .unwrap()
        .try_get_by_index(0)
        .unwrap()
}

async fn scalar_string(db: &DatabaseConnection, backend: DbBackend, sql: &str) -> String {
    db.query_one_raw(Statement::from_string(backend, sql))
        .await
        .unwrap()
        .unwrap()
        .try_get_by_index(0)
        .unwrap()
}

async fn column_exists(
    db: &DatabaseConnection,
    backend: DbBackend,
    table: &str,
    column: &str,
) -> bool {
    let sql = match backend {
        DbBackend::Sqlite => {
            format!("SELECT COUNT(*) FROM pragma_table_info('{table}') WHERE name = '{column}'")
        }
        DbBackend::Postgres => {
            format!(
                "SELECT COUNT(*) FROM information_schema.columns \
                 WHERE table_schema = 'public' \
                   AND table_name = '{table}' \
                   AND column_name = '{column}'"
            )
        }
        DbBackend::MySql => {
            format!(
                "SELECT COUNT(*) FROM information_schema.columns \
                 WHERE table_schema = DATABASE() \
                   AND table_name = '{table}' \
                   AND column_name = '{column}'"
            )
        }
        backend => panic!("unsupported test database backend: {backend:?}"),
    };
    scalar_i64(db, backend, &sql).await > 0
}

async fn applied_migration_versions(db: &DatabaseConnection, backend: DbBackend) -> Vec<String> {
    db.query_all_raw(Statement::from_string(
        backend,
        "SELECT version FROM seaql_migrations ORDER BY version",
    ))
    .await
    .unwrap()
    .into_iter()
    .map(|row| row.try_get_by_index::<String>(0).unwrap())
    .collect()
}

async fn seed_migration_fixture(database_url: &str) -> i64 {
    let state = common::setup_with_database_url(database_url).await;
    let app = create_test_app!(state.clone());
    let (token, _) = register_and_login!(app);

    let folder_req = actix_test::TestRequest::post()
        .uri("/api/v1/folders")
        .insert_header(("Cookie", common::access_cookie_header(&token)))
        .insert_header(common::csrf_header_for(&token))
        .set_json(serde_json::json!({
            "name": "Migrated Folder",
            "parent_id": null
        }))
        .to_request();
    let folder_resp = actix_test::call_service(&app, folder_req).await;
    assert_eq!(folder_resp.status(), 201);
    let folder_body: Value = actix_test::read_body_json(folder_resp).await;
    let folder_id = folder_body["data"]["id"]
        .as_i64()
        .expect("folder id should exist");

    let file_id = upload_test_file_to_folder!(app, token, folder_id);
    seed_file_create_idempotency_fixture(state.writer_db(), file_id).await;
    seed_passkey_fixture(state.writer_db()).await;
    seed_mfa_fixture(state.writer_db()).await;
    seed_remote_node_fixture(state.writer_db()).await;
    seed_user_invitation_fixture(state.writer_db()).await;
    seed_tag_fixture(state.writer_db(), file_id).await;
    seed_runtime_coordination_fixture(state.writer_db()).await;
    seed_admin_content_access_fixture(state.writer_db(), file_id).await;
    // A nonempty receipt proves migration preserves recovery state, not just its table.
    let now = Utc::now();
    let task = aster_drive_model::entities::background_task::ActiveModel {
        kind: Set(aster_drive_model::types::BackgroundTaskKind::ArchiveCompress),
        status: Set(aster_drive_model::types::BackgroundTaskStatus::Canceled),
        display_name: Set("migration publication fixture".to_string()),
        payload_json: Set(aster_drive_model::types::StoredTaskPayload(
            "{}".to_string(),
        )),
        progress_current: Set(0),
        progress_total: Set(0),
        attempt_count: Set(1),
        max_attempts: Set(3),
        processing_token: Set(2),
        next_run_at: Set(now),
        expires_at: Set(now + Duration::hours(24)),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(state.writer_db())
    .await
    .unwrap();
    aster_drive_model::entities::task_resource_publication::ActiveModel {
        task_id: Set(task.id),
        entry_key: Set("a".repeat(64)),
        resource_snapshot: Set(
            serde_json::json!({"version":1,"fixture_file_id":file_id,"unicode":"迁移产物"})
                .to_string(),
        ),
    }
    .insert(state.writer_db())
    .await
    .unwrap();

    file_id
}

async fn seed_admin_content_access_fixture(db: &DatabaseConnection, file_id: i64) {
    let session = auth_session::Entity::find().one(db).await.unwrap().unwrap();
    let expiry = chrono::DateTime::parse_from_rfc3339("2026-10-30T01:02:03Z")
        .unwrap()
        .with_timezone(&Utc);
    // Preserve both payload variants, session binding, and expired rows without interpretation.
    for (id, payload) in [
        (
            "migration-workspace-access",
            serde_json::json!({
                "kind": "workspace",
                "scope": {"scope_kind": "personal", "target_id": session.user_id},
                "reason": "security_investigation",
                "remark": "迁移保真 <case-628>"
            }),
        ),
        (
            "migration-download-access",
            serde_json::json!({
                "kind": "download",
                "scope": {"scope_kind": "personal", "target_id": session.user_id},
                "file_id": file_id,
                "revision_id": 628
            }),
        ),
    ] {
        admin_content_access::ActiveModel {
            id: Set(id.to_string()),
            actor_user_id: Set(session.user_id),
            session_id: Set(session.id.clone()),
            payload: Set(payload.to_string()),
            expires_at: Set(expiry),
        }
        .insert(db)
        .await
        .unwrap();
    }
}

async fn seed_file_create_idempotency_fixture(db: &DatabaseConnection, file_id: i64) {
    let user = user_repo::find_by_email(db, "test@example.com")
        .await
        .unwrap()
        .expect("seed user should exist");
    let now = Utc::now();
    let claim = file_create_idempotency_repo::create_claim(
        db,
        file_create_idempotency_repo::FileCreateIdempotencyScope {
            actor_user_id: user.id,
            workspace_kind: "personal",
            workspace_id: user.id,
        },
        MIGRATION_FILE_CREATE_KEY_HASH,
        MIGRATION_FILE_CREATE_FINGERPRINT,
        now,
        now + Duration::hours(24),
    )
    .await
    .expect("file-create idempotency claim should insert");
    file_create_idempotency_repo::complete(db, claim, file_id)
        .await
        .expect("file-create idempotency claim should complete");
}

async fn seed_runtime_coordination_fixture(db: &DatabaseConnection) {
    let created_at = chrono::DateTime::parse_from_rfc3339("2026-07-13T01:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let last_claimed_at = created_at + Duration::minutes(15);
    let last_finished_at = created_at + Duration::minutes(20);
    let next_run_at = created_at + Duration::hours(1);

    aster_forge_db::scheduled_task::ActiveModel {
        task_id: Set(MIGRATION_SCHEDULED_TASK_ID.to_string()),
        namespace: Set("aster_drive".to_string()),
        task_name: Set("migration_fixture".to_string()),
        display_name: Set("Migration fixture".to_string()),
        next_run_at: Set(next_run_at),
        claim_owner_id: Set(None),
        claim_expires_at: Set(None),
        last_claimed_at: Set(Some(last_claimed_at)),
        last_finished_at: Set(Some(last_finished_at)),
        created_at: Set(created_at),
        updated_at: Set(last_finished_at),
    }
    .insert(db)
    .await
    .unwrap();

    aster_forge_db::runtime_lease::ActiveModel {
        lease_id: Set(MIGRATION_RUNTIME_LEASE_ID.to_string()),
        owner_id: Set("old-runtime".to_string()),
        expires_at: Set(created_at + Duration::hours(2)),
        last_renewed_at: Set(created_at + Duration::minutes(30)),
        created_at: Set(created_at),
        updated_at: Set(created_at + Duration::minutes(30)),
    }
    .insert(db)
    .await
    .unwrap();
}

async fn seed_passkey_fixture(db: &DatabaseConnection) {
    let user = user_repo::find_by_email(db, "test@example.com")
        .await
        .unwrap()
        .expect("seed user should exist");
    let now = Utc::now();

    passkey::ActiveModel {
        user_id: Set(user.id),
        credential_id: Set("migrate-passkey-credential".to_string()),
        user_handle: Set("8f7cb5ac-d850-4706-94ee-2b062765697a".to_string()),
        credential: Set(StoredPasskeyCredential(
            serde_json::json!({
            "cred_id": "migrate-passkey-credential",
            "counter": 7,
            "transports": ["internal"]
            })
            .to_string(),
        )),
        name: Set("Migrated Passkey".to_string()),
        transports: Set(Some("internal".to_string())),
        backup_eligible: Set(true),
        backed_up: Set(false),
        sign_count: Set(7),
        created_at: Set(now),
        updated_at: Set(now),
        last_used_at: Set(Some(now)),
        ..Default::default()
    }
    .insert(db)
    .await
    .unwrap();
}

async fn seed_mfa_fixture(db: &DatabaseConnection) {
    let user = user_repo::find_by_email(db, "test@example.com")
        .await
        .unwrap()
        .expect("seed user should exist");
    let now = Utc::now();

    mfa_factor_repo::create(
        db,
        mfa_factor::ActiveModel {
            user_id: Set(user.id),
            method: Set(MfaPersistentFactorMethod::Totp),
            name: Set("Migrated TOTP".to_string()),
            secret_ciphertext: Set("migrate-factor-secret-ciphertext".to_string()),
            secret_version: Set(1),
            enabled_at: Set(now - Duration::minutes(20)),
            last_used_at: Set(Some(now - Duration::minutes(5))),
            created_at: Set(now - Duration::minutes(25)),
            updated_at: Set(now - Duration::minutes(5)),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    mfa_recovery_code_repo::create_many(
        db,
        vec![
            mfa_recovery_code::ActiveModel {
                user_id: Set(user.id),
                code_hash: Set("migrate-recovery-code-unused".to_string()),
                used_at: Set(None),
                created_at: Set(now - Duration::minutes(20)),
                ..Default::default()
            },
            mfa_recovery_code::ActiveModel {
                user_id: Set(user.id),
                code_hash: Set("migrate-recovery-code-used".to_string()),
                used_at: Set(Some(now - Duration::minutes(10))),
                created_at: Set(now - Duration::minutes(30)),
                ..Default::default()
            },
        ],
    )
    .await
    .unwrap();

    mfa_login_flow_repo::create(
        db,
        mfa_login_flow::ActiveModel {
            flow_token_hash: Set("migrate-login-flow-token-hash".to_string()),
            user_id: Set(user.id),
            user_session_version: Set(user.session_version),
            first_factor: Set(MfaFirstFactor::Password),
            return_path: Set(Some("/files".to_string())),
            ip_address: Set(Some("127.0.0.1".to_string())),
            user_agent: Set(Some("migration-test".to_string())),
            attempt_count: Set(1),
            expires_at: Set(now + Duration::minutes(10)),
            consumed_at: Set(None),
            created_at: Set(now - Duration::minutes(1)),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    mfa_totp_setup_flow_repo::create(
        db,
        mfa_totp_setup_flow::ActiveModel {
            flow_token_hash: Set("migrate-totp-setup-token-hash".to_string()),
            user_id: Set(user.id),
            secret_ciphertext: Set("migrate-setup-secret-ciphertext".to_string()),
            secret_version: Set(1),
            expires_at: Set(now + Duration::minutes(10)),
            consumed_at: Set(Some(now - Duration::minutes(1))),
            created_at: Set(now - Duration::minutes(5)),
            ..Default::default()
        },
    )
    .await
    .unwrap();
}

async fn seed_remote_node_fixture(db: &DatabaseConnection) {
    let now = Utc::now();

    let remote_node = managed_follower_repo::create(
        db,
        managed_follower::ActiveModel {
            name: Set(MIGRATION_REMOTE_NODE_NAME.to_string()),
            base_url: Set("https://remote.example.com".to_string()),
            access_key: Set("migrate-remote-ak".to_string()),
            secret_key: Set("migrate-remote-sk".to_string()),
            is_enabled: Set(true),
            last_capabilities: Set(serde_json::to_string(
                &aster_drive::storage::remote_protocol::RemoteStorageCapabilities::current(),
            )
            .expect("current remote capabilities should serialize")),
            last_probe_error: Set(String::new()),
            last_probe_at: Set(Some(now)),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let remote_policy = policy_repo::create(
        db,
        storage_policy::ActiveModel {
            name: Set(MIGRATION_REMOTE_POLICY_NAME.to_string()),
            connector_id: Set("asterdrive.storage.remote".to_string()),
            storage_config: Set(common::encoded_policy_config(
                "asterdrive.storage.remote",
                common::TestRemoteConnectorConfigV1 {
                    base_path: "remote-ingress".to_string(),
                    remote_node_id: Some(remote_node.id),
                    remote_storage_target_key: Some("migration-target".to_string()),
                    remote_download_strategy: RemoteDownloadStrategy::RelayStream,
                    remote_upload_strategy: RemoteUploadStrategy::RelayStream,
                    capacity_probe_timeout_secs: 10,
                },
                aster_drive_storage::StoragePolicyBehaviorConfig::default(),
            )),
            max_file_size: Set(0),
            allowed_types: Set(StoredStoragePolicyAllowedTypes::empty()),
            is_default: Set(false),
            chunk_size: Set(5_242_880),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    storage_policy_connector_credential_repo::upsert(
        db,
        remote_policy.id,
        "asterdrive.storage.remote".to_string(),
        1,
        MIGRATION_REMOTE_POLICY_CIPHERTEXT.to_string(),
    )
    .await
    .expect("connector credential migration fixture should be stored");

    follower_enrollment_session_repo::create(
        db,
        follower_enrollment_session::ActiveModel {
            managed_follower_id: Set(remote_node.id),
            token_hash: Set("migrate-enrollment-token-hash".to_string()),
            ack_token_hash: Set("migrate-enrollment-ack-token-hash".to_string()),
            expires_at: Set(now + Duration::minutes(30)),
            redeemed_at: Set(Some(now - Duration::minutes(5))),
            acked_at: Set(None),
            invalidated_at: Set(None),
            created_at: Set(now - Duration::minutes(10)),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    master_binding_repo::create(
        db,
        master_binding::ActiveModel {
            name: Set(MIGRATION_MASTER_BINDING_NAME.to_string()),
            master_url: Set("https://primary.example.com".to_string()),
            access_key: Set("migrate-master-ak".to_string()),
            secret_key: Set("migrate-master-sk".to_string()),
            storage_namespace: Set(MIGRATION_MASTER_STORAGE_NAMESPACE.to_string()),
            is_enabled: Set(true),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        },
    )
    .await
    .unwrap();
}

async fn seed_user_invitation_fixture(db: &DatabaseConnection) {
    let user = user_repo::find_by_email(db, "test@example.com")
        .await
        .unwrap()
        .expect("seed user should exist");
    let now = Utc::now();

    user_invitation::ActiveModel {
        email: Set("migrated-invitee@example.com".to_string()),
        token_hash: Set("migrate-user-invitation-token-hash".to_string()),
        status: Set(UserInvitationStatus::Pending),
        invited_by: Set(user.id),
        accepted_user_id: Set(None),
        expires_at: Set(now + Duration::days(7)),
        created_at: Set(now - Duration::minutes(15)),
        updated_at: Set(now - Duration::minutes(15)),
        accepted_at: Set(None),
        revoked_at: Set(None),
        ..Default::default()
    }
    .insert(db)
    .await
    .unwrap();
}

async fn seed_tag_fixture(db: &DatabaseConnection, file_id: i64) {
    let user = user_repo::find_by_email(db, "test@example.com")
        .await
        .unwrap()
        .expect("seed user should exist");
    let tag = tag_repo::create(
        db,
        TagScopeType::Personal,
        Some(user.id),
        None,
        MIGRATION_TAG_NAME,
        MIGRATION_TAG_NORMALIZED_NAME,
        MIGRATION_TAG_COLOR,
    )
    .await
    .unwrap();

    property_repo::upsert(
        db,
        EntityType::File,
        file_id,
        MIGRATION_TAG_PROPERTY_NAMESPACE,
        &tag.id.to_string(),
        None,
    )
    .await
    .unwrap();
}

async fn seed_contact_verification_history(database_url: &str) {
    let db = db::connect_with_metrics(
        &DatabaseConfig {
            url: database_url.into(),
            pool_size: 1,
            retry_count: 0,
        },
        aster_drive_metrics::NoopMetrics::arc(),
    )
    .await
    .unwrap();
    let user = user_repo::find_by_email(&db, "test@example.com")
        .await
        .unwrap()
        .expect("seed user should exist");
    let now = Utc::now();

    for (index, consumed_at) in [
        (1, Some(now - Duration::minutes(30))),
        (2, Some(now - Duration::minutes(20))),
        (3, None),
    ] {
        contact_verification_token_repo::create(
            &db,
            contact_verification_token::ActiveModel {
                user_id: Set(user.id),
                channel: Set(VerificationChannel::Email),
                purpose: Set(VerificationPurpose::PasswordReset),
                target: Set(user.email.clone()),
                token_hash: Set(format!("password-reset-history-{index}")),
                expires_at: Set(now + Duration::minutes(30)),
                consumed_at: Set(consumed_at),
                created_at: Set(now - Duration::minutes(40 - index as i64)),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    }

    for (index, consumed_at) in [
        (1, Some(now - Duration::minutes(50))),
        (2, Some(now - Duration::minutes(45))),
    ] {
        contact_verification_token_repo::create(
            &db,
            contact_verification_token::ActiveModel {
                user_id: Set(user.id),
                channel: Set(VerificationChannel::Email),
                purpose: Set(VerificationPurpose::ContactChange),
                target: Set(user.email.clone()),
                token_hash: Set(format!("contact-change-history-{index}")),
                expires_at: Set(now + Duration::minutes(30)),
                consumed_at: Set(consumed_at),
                created_at: Set(now - Duration::minutes(60 - index as i64)),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    }
}

async fn assert_migrated_fixture(
    target_database_url: &str,
    target_backend: DbBackend,
    file_id: i64,
) {
    let target_db = db::connect_with_metrics(
        &DatabaseConfig {
            url: target_database_url.into(),
            pool_size: 1,
            retry_count: 0,
        },
        aster_drive_metrics::NoopMetrics::arc(),
    )
    .await
    .unwrap();
    let publication = aster_drive_model::entities::task_resource_publication::Entity::find()
        .one(&target_db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(publication.entry_key, "a".repeat(64));
    assert_eq!(
        serde_json::from_str::<Value>(&publication.resource_snapshot).unwrap(),
        serde_json::json!({"version":1,"fixture_file_id":file_id,"unicode":"迁移产物"})
    );
    let producer =
        aster_drive_model::entities::background_task::Entity::find_by_id(publication.task_id)
            .one(&target_db)
            .await
            .unwrap()
            .unwrap();
    assert_eq!(
        producer.status,
        aster_drive_model::types::BackgroundTaskStatus::Canceled
    );
    assert_eq!(producer.processing_token, 2);
    let users = scalar_i64(&target_db, target_backend, "SELECT COUNT(*) FROM users").await;
    let folders = scalar_i64(&target_db, target_backend, "SELECT COUNT(*) FROM folders").await;
    let files = scalar_i64(&target_db, target_backend, "SELECT COUNT(*) FROM files").await;
    let accesses = admin_content_access::Entity::find()
        .all(&target_db)
        .await
        .unwrap();
    assert_eq!(accesses.len(), 2);
    for access in accesses {
        let session = auth_session::Entity::find_by_id(access.session_id.clone())
            .one(&target_db)
            .await
            .unwrap()
            .expect("bound authentication session should migrate");
        assert_eq!(access.actor_user_id, session.user_id);
        assert_eq!(access.expires_at.to_rfc3339(), "2026-10-30T01:02:03+00:00");
        let expected = match access.id.as_str() {
            "migration-workspace-access" => serde_json::json!({
                "kind": "workspace",
                "scope": {"scope_kind": "personal", "target_id": session.user_id},
                "reason": "security_investigation",
                "remark": "迁移保真 <case-628>"
            }),
            "migration-download-access" => serde_json::json!({
                "kind": "download",
                "scope": {"scope_kind": "personal", "target_id": session.user_id},
                "file_id": file_id,
                "revision_id": 628
            }),
            other => panic!("unexpected migrated access: {other}"),
        };
        assert_eq!(access.payload, expected.to_string());
    }
    let file_create_idempotencies = scalar_i64(
        &target_db,
        target_backend,
        &format!(
            "SELECT COUNT(*) FROM file_create_idempotencies \
             WHERE key_hash = '{MIGRATION_FILE_CREATE_KEY_HASH}' \
               AND request_fingerprint = '{MIGRATION_FILE_CREATE_FINGERPRINT}' \
               AND result_file_id = {file_id}"
        ),
    )
    .await;
    let managed_followers = scalar_i64(
        &target_db,
        target_backend,
        "SELECT COUNT(*) FROM managed_followers",
    )
    .await;
    let passkeys = scalar_i64(&target_db, target_backend, "SELECT COUNT(*) FROM passkeys").await;
    let mfa_factors = scalar_i64(
        &target_db,
        target_backend,
        "SELECT COUNT(*) FROM mfa_factors",
    )
    .await;
    let mfa_recovery_codes = scalar_i64(
        &target_db,
        target_backend,
        "SELECT COUNT(*) FROM mfa_recovery_codes",
    )
    .await;
    let mfa_login_flows = scalar_i64(
        &target_db,
        target_backend,
        "SELECT COUNT(*) FROM mfa_login_flows",
    )
    .await;
    let mfa_totp_setup_flows = scalar_i64(
        &target_db,
        target_backend,
        "SELECT COUNT(*) FROM mfa_totp_setup_flows",
    )
    .await;
    let used_mfa_recovery_codes = scalar_i64(
        &target_db,
        target_backend,
        "SELECT COUNT(*) FROM mfa_recovery_codes WHERE used_at IS NOT NULL",
    )
    .await;
    let mfa_login_flow_with_attempt_count = scalar_i64(
        &target_db,
        target_backend,
        "SELECT COUNT(*) FROM mfa_login_flows \
         WHERE flow_token_hash = 'migrate-login-flow-token-hash' AND attempt_count = 1",
    )
    .await;
    let enrollment_sessions = scalar_i64(
        &target_db,
        target_backend,
        "SELECT COUNT(*) FROM follower_enrollment_sessions",
    )
    .await;
    let master_bindings = scalar_i64(
        &target_db,
        target_backend,
        "SELECT COUNT(*) FROM master_bindings",
    )
    .await;
    let user_invitations = scalar_i64(
        &target_db,
        target_backend,
        "SELECT COUNT(*) FROM user_invitations",
    )
    .await;
    let tags = scalar_i64(&target_db, target_backend, "SELECT COUNT(*) FROM tags").await;
    let migrated_file_tag_bindings = scalar_i64(
        &target_db,
        target_backend,
        &format!(
            "SELECT COUNT(*) FROM entity_properties \
             WHERE entity_type = 'file' \
               AND entity_id = {file_id} \
               AND namespace = '{MIGRATION_TAG_PROPERTY_NAMESPACE}'"
        ),
    )
    .await;
    let remote_policy = policy_repo::find_all(&target_db)
        .await
        .unwrap()
        .into_iter()
        .find(|policy| policy.name == MIGRATION_REMOTE_POLICY_NAME)
        .expect("migrated remote policy should exist");
    let remote_policy_config = common::remote_policy_config(&remote_policy);
    let remote_policy_credential =
        storage_policy_connector_credential_repo::find_by_policy(&target_db, remote_policy.id)
            .await
            .unwrap()
            .expect("migrated connector credential should exist");
    let file_name = scalar_string(
        &target_db,
        target_backend,
        &format!("SELECT name FROM files WHERE id = {file_id}"),
    )
    .await;
    let passkey = aster_drive_model::entities::passkey::Entity::find()
        .one(&target_db)
        .await
        .unwrap()
        .expect("migrated passkey should exist");
    let managed_followers_has_namespace =
        column_exists(&target_db, target_backend, "managed_followers", "namespace").await;
    let master_bindings_has_namespace =
        column_exists(&target_db, target_backend, "master_bindings", "namespace").await;
    let master_bindings_has_storage_namespace = column_exists(
        &target_db,
        target_backend,
        "master_bindings",
        "storage_namespace",
    )
    .await;
    let master_binding_storage_namespace = scalar_string(
        &target_db,
        target_backend,
        &format!(
            "SELECT storage_namespace FROM master_bindings WHERE name = '{MIGRATION_MASTER_BINDING_NAME}'"
        ),
    )
    .await;
    let migrated_tag_color = scalar_string(
        &target_db,
        target_backend,
        &format!("SELECT color FROM tags WHERE name = '{MIGRATION_TAG_NAME}'"),
    )
    .await;
    let scheduled_task =
        aster_forge_db::scheduled_task::Entity::find_by_id(MIGRATION_SCHEDULED_TASK_ID.to_string())
            .one(&target_db)
            .await
            .unwrap()
            .expect("scheduled task catalog row should migrate");
    let runtime_lease =
        aster_forge_db::runtime_lease::Entity::find_by_id(MIGRATION_RUNTIME_LEASE_ID.to_string())
            .one(&target_db)
            .await
            .unwrap()
            .expect("runtime lease row should migrate");

    assert_eq!(users, 1);
    assert_eq!(folders, 1);
    assert_eq!(files, 1);
    assert_eq!(file_create_idempotencies, 1);
    assert_eq!(passkeys, 1);
    assert_eq!(mfa_factors, 1);
    assert_eq!(mfa_recovery_codes, 2);
    assert_eq!(mfa_login_flows, 1);
    assert_eq!(mfa_totp_setup_flows, 1);
    assert_eq!(used_mfa_recovery_codes, 1);
    assert_eq!(mfa_login_flow_with_attempt_count, 1);
    assert_eq!(managed_followers, 1);
    assert_eq!(enrollment_sessions, 1);
    assert_eq!(master_bindings, 1);
    assert_eq!(user_invitations, 1);
    assert_eq!(tags, 1);
    assert_eq!(migrated_file_tag_bindings, 1);
    assert_eq!(remote_policy.connector_id, "asterdrive.storage.remote");
    assert_eq!(remote_policy_config.remote_node_id, Some(1));
    assert_eq!(
        remote_policy_config.remote_storage_target_key.as_deref(),
        Some("migration-target")
    );
    assert_eq!(
        remote_policy_credential.connector_id,
        "asterdrive.storage.remote"
    );
    assert_eq!(remote_policy_credential.schema_version, 1);
    assert_eq!(remote_policy_credential.revision, 1);
    assert_eq!(
        remote_policy_credential.ciphertext,
        MIGRATION_REMOTE_POLICY_CIPHERTEXT
    );
    assert_eq!(file_name, "test-in-folder.txt");
    assert_eq!(passkey.name, "Migrated Passkey");
    let credential = passkey
        .credential
        .parse()
        .expect("passkey credential should be valid JSON");
    assert_eq!(credential["counter"], 7);
    assert!(!managed_followers_has_namespace);
    assert!(!master_bindings_has_namespace);
    assert!(master_bindings_has_storage_namespace);
    assert_eq!(
        master_binding_storage_namespace,
        MIGRATION_MASTER_STORAGE_NAMESPACE
    );
    assert_eq!(migrated_tag_color, MIGRATION_TAG_COLOR);
    assert_eq!(scheduled_task.namespace, "aster_drive");
    assert_eq!(scheduled_task.task_name, "migration_fixture");
    assert_eq!(
        scheduled_task.next_run_at,
        chrono::DateTime::parse_from_rfc3339("2026-07-13T02:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    );
    assert_eq!(
        scheduled_task.last_finished_at,
        Some(
            chrono::DateTime::parse_from_rfc3339("2026-07-13T01:20:00Z")
                .unwrap()
                .with_timezone(&Utc)
        )
    );
    assert_eq!(runtime_lease.owner_id, "old-runtime");
    assert_eq!(
        runtime_lease.expires_at,
        chrono::DateTime::parse_from_rfc3339("2026-07-13T03:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    );
    assert_eq!(
        runtime_lease.last_renewed_at,
        chrono::DateTime::parse_from_rfc3339("2026-07-13T01:30:00Z")
            .unwrap()
            .with_timezone(&Utc)
    );
}

#[test]
fn test_root_binary_help_lists_config_subcommand() {
    let output = run_aster_drive(&["--help"]);
    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("help stdout should be utf-8");
    assert_eq!(
        stdout.lines().next(),
        Some(concat!("AsterDrive ", env!("CARGO_PKG_VERSION")))
    );
    assert!(stdout.contains(env!("CARGO_PKG_DESCRIPTION")));
    assert!(stdout.contains("serve"));
    assert!(stdout.contains("Run the server with all, API, or worker responsibilities"));
    assert!(stdout.contains("config"));
    assert!(stdout.contains("View and manage runtime settings"));
    assert!(stdout.contains("doctor"));
    assert!(stdout.contains("Check instance health and configuration"));
    assert!(stdout.contains("database-migrate"));
    assert!(stdout.contains("Migrate data between database backends"));
}

#[test]
fn test_root_binary_config_help_lists_runtime_config_commands() {
    let output = run_aster_drive(&["config", "--help"]);
    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("config help stdout should be utf-8");
    for command in [
        "list", "get", "set", "delete", "validate", "export", "import",
    ] {
        assert!(
            stdout.contains(command),
            "config help should mention '{command}', got: {stdout}"
        );
    }
}

#[tokio::test]
async fn test_root_binary_serve_help_is_available() {
    let output = run_aster_drive(&["serve", "--help"]);
    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("serve help stdout should be utf-8");
    assert!(stdout.contains("Run the server with all, API, or worker responsibilities"));
}

#[tokio::test]
async fn test_root_binary_database_migrate_help_is_available() {
    let output = run_aster_drive(&["database-migrate", "--help"]);
    assert!(output.status.success());

    let stdout =
        String::from_utf8(output.stdout).expect("database-migrate help stdout should be utf-8");
    assert!(stdout.contains("Migrate data between database backends"));
    assert!(stdout.contains("--source-database-url"));
    assert!(stdout.contains("--target-database-url"));
    assert!(stdout.contains("--dry-run"));
    assert!(stdout.contains("--verify-only"));
}

#[tokio::test]
async fn test_root_binary_doctor_help_is_available() {
    let output = run_aster_drive(&["doctor", "--help"]);
    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("doctor help stdout should be utf-8");
    assert!(stdout.contains("Check instance health and configuration"));
    assert!(stdout.contains("--database-url"));
    assert!(stdout.contains("--output-format"));
    assert!(stdout.contains("--strict"));
    assert!(stdout.contains("--deep"));
    assert!(stdout.contains("--fix"));
    assert!(stdout.contains("--scope"));
    assert!(stdout.contains("--policy-id"));
}

#[tokio::test]
async fn test_root_binary_config_set_and_get_round_trip() {
    let database_url = setup_database_url().await;

    let set_output = run_aster_drive(&[
        "config",
        "--database-url",
        &database_url,
        "set",
        "--key",
        "public_site_url",
        "--value",
        r#"[" HTTPS://Drive.EXAMPLE.com/ "]"#,
    ]);
    assert!(
        set_output.status.success(),
        "set stderr: {}",
        String::from_utf8_lossy(&set_output.stderr)
    );
    let set_json: Value = serde_json::from_slice(&set_output.stdout).expect("set output json");
    assert!(
        set_output.stderr.is_empty(),
        "structured config success stderr should be empty: {}",
        String::from_utf8_lossy(&set_output.stderr)
    );
    assert_eq!(set_json["ok"], true);
    assert_eq!(
        set_json["data"]["value"],
        serde_json::json!(["https://drive.example.com"])
    );

    let get_output = run_aster_drive(&[
        "config",
        "--database-url",
        &database_url,
        "get",
        "--key",
        "public_site_url",
    ]);
    assert!(
        get_output.status.success(),
        "get stderr: {}",
        String::from_utf8_lossy(&get_output.stderr)
    );
    let get_json: Value = serde_json::from_slice(&get_output.stdout).expect("get output json");
    assert_eq!(get_json["ok"], true);
    assert_eq!(get_json["data"]["key"], "public_site_url");
    assert_eq!(
        get_json["data"]["value"],
        serde_json::json!(["https://drive.example.com"])
    );
}

#[tokio::test]
async fn test_root_binary_config_get_human_output_is_readable() {
    let database_url = setup_database_url().await;

    let set_output = run_aster_drive(&[
        "config",
        "--database-url",
        &database_url,
        "set",
        "--key",
        "public_site_url",
        "--value",
        r#"[" HTTPS://Drive.EXAMPLE.com/ "]"#,
    ]);
    assert!(
        set_output.status.success(),
        "set stderr: {}",
        String::from_utf8_lossy(&set_output.stderr)
    );

    let output = run_aster_drive(&[
        "config",
        "--database-url",
        &database_url,
        "--output-format",
        "human",
        "get",
        "--key",
        "public_site_url",
    ]);
    assert!(
        output.status.success(),
        "get stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("human stdout should be utf-8");
    assert!(stdout.contains("Configuration value"));
    assert!(stdout.contains("Key:"));
    assert!(stdout.contains("Value:"));
    assert!(stdout.contains("Source:"));
    assert!(stdout.contains("public_site_url"));
    assert!(stdout.contains("https://drive.example.com"));
    assert!(stdout.contains("[system]"));
    assert!(serde_json::from_str::<Value>(&stdout).is_err());
}

#[tokio::test]
async fn test_root_binary_config_list_human_summarizes_multiline_and_masks_sensitive() {
    let database_url = setup_database_url().await;

    let set_output = run_aster_drive(&[
        "config",
        "--database-url",
        &database_url,
        "set",
        "--key",
        "mail_smtp_password",
        "--value",
        "super-secret-password",
    ]);
    assert!(
        set_output.status.success(),
        "set stderr: {}",
        String::from_utf8_lossy(&set_output.stderr)
    );

    let output = run_aster_drive(&[
        "config",
        "--database-url",
        &database_url,
        "--output-format",
        "human",
        "list",
    ]);
    assert!(
        output.status.success(),
        "list stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("human list stdout should be utf-8");
    assert!(stdout.contains("mail_smtp_password"));
    assert!(stdout.contains("[hidden sensitive value]"));
    assert!(!stdout.contains("super-secret-password"));
    assert!(stdout.contains("mail_template_register_activation_html"));
    assert!(stdout.contains("<html template:"));
    assert!(!stdout.contains("mail_template_register_activation_html = <!doctype html>"));
    assert!(stdout.contains("frontend_preview_apps_json"));
    assert!(stdout.contains("<json value:"));
    assert!(!stdout.contains("frontend_preview_apps_json = {"));
}

#[cfg(unix)]
#[tokio::test]
async fn config_backup_round_trips_values_and_redacts_secrets_by_default() {
    use std::os::unix::fs::PermissionsExt;

    let source = setup_database_url().await;
    let target_without_secrets = setup_database_url().await;
    let target_with_secrets = setup_database_url().await;
    for (key, value) in [
        ("custom.backup_roundtrip", "line one\nline two"),
        ("mail_smtp_password", "backup-secret-marker"),
    ] {
        let output = run_aster_drive(&[
            "config",
            "--database-url",
            &source,
            "set",
            "--key",
            key,
            "--value",
            value,
        ]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        if key == "mail_smtp_password" {
            assert!(!String::from_utf8_lossy(&output.stdout).contains(value));
            assert!(!String::from_utf8_lossy(&output.stderr).contains(value));
        }
    }

    let sanitized = run_aster_drive(&["config", "--database-url", &source, "export"]);
    assert!(sanitized.status.success());
    let backup: Value = serde_json::from_slice(&sanitized.stdout).unwrap();
    assert_eq!(backup["format"], "asterdrive.runtime-config");
    assert_eq!(backup["version"], 1);
    assert!(
        backup["omitted_sensitive_keys"]
            .as_array()
            .unwrap()
            .iter()
            .any(|key| key == "mail_smtp_password")
    );
    assert!(!String::from_utf8_lossy(&sanitized.stdout).contains("backup-secret-marker"));

    let dir = aster_forge_utils::raii::TempDirGuard::new(
        std::env::temp_dir().join(format!("aster-cli-backup-{}", uuid::Uuid::new_v4())),
        "config backup fixture",
    );
    std::fs::create_dir_all(dir.path()).unwrap();
    let sanitized_path = dir.path().join("sanitized.json");
    std::fs::write(&sanitized_path, &sanitized.stdout).unwrap();
    std::fs::set_permissions(&sanitized_path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let imported = run_aster_drive(&[
        "config",
        "--database-url",
        &target_without_secrets,
        "import",
        "--input-file",
        sanitized_path.to_str().unwrap(),
    ]);
    assert!(
        imported.status.success(),
        "{}",
        String::from_utf8_lossy(&imported.stderr)
    );
    assert!(!String::from_utf8_lossy(&imported.stdout).contains("backup-secret-marker"));
    let value = run_aster_drive(&[
        "config",
        "--database-url",
        &target_without_secrets,
        "get",
        "--key",
        "custom.backup_roundtrip",
    ]);
    let value: Value = serde_json::from_slice(&value.stdout).unwrap();
    assert_eq!(value["data"]["value"], "line one\nline two");

    let complete = run_aster_drive(&[
        "config",
        "--database-url",
        &source,
        "export",
        "--include-sensitive",
    ]);
    assert!(String::from_utf8_lossy(&complete.stdout).contains("backup-secret-marker"));
    let complete_path = dir.path().join("complete.json");
    std::fs::write(&complete_path, &complete.stdout).unwrap();
    std::fs::set_permissions(&complete_path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let imported = run_aster_drive(&[
        "config",
        "--database-url",
        &target_with_secrets,
        "import",
        "--input-file",
        complete_path.to_str().unwrap(),
    ]);
    assert!(
        imported.status.success(),
        "{}",
        String::from_utf8_lossy(&imported.stderr)
    );
    assert!(!String::from_utf8_lossy(&imported.stdout).contains("backup-secret-marker"));
    let restored = run_aster_drive(&[
        "config",
        "--database-url",
        &target_with_secrets,
        "export",
        "--include-sensitive",
    ]);
    let before: Value = serde_json::from_slice(&complete.stdout).unwrap();
    let after: Value = serde_json::from_slice(&restored.stdout).unwrap();
    let before = before["configs"].as_array().unwrap();
    let after = after["configs"].as_array().unwrap();
    assert_eq!(before.len(), after.len());
    for (original, restored) in before.iter().zip(after) {
        assert_eq!(original["key"], restored["key"]);
        if original["key"] == "media_processing_registry_json" {
            // Processor extension order is a set; import canonicalizes that
            // order. Every other registry property must survive unchanged.
            let mut first: Value =
                serde_json::from_str(original["value"].as_str().unwrap()).unwrap();
            let mut second: Value =
                serde_json::from_str(restored["value"].as_str().unwrap()).unwrap();
            for registry in [&mut first, &mut second] {
                for processor in registry["processors"].as_array_mut().unwrap() {
                    if let Some(extensions) = processor
                        .get_mut("extensions")
                        .and_then(Value::as_array_mut)
                    {
                        extensions.sort_by(|a, b| a.as_str().cmp(&b.as_str()));
                    }
                }
            }
            assert!(
                first == second,
                "processor configuration changed during round trip"
            );
        } else {
            assert!(
                original == restored,
                "configuration changed during round trip: {}",
                original["key"]
            );
        }
    }
}

#[cfg(unix)]
#[tokio::test]
async fn config_import_rejects_invalid_batch_without_partial_writes() {
    use std::os::unix::fs::PermissionsExt;

    let database_url = setup_database_url().await;
    let dir = aster_forge_utils::raii::TempDirGuard::new(
        std::env::temp_dir().join(format!("aster-cli-invalid-backup-{}", uuid::Uuid::new_v4())),
        "invalid config backup fixture",
    );
    std::fs::create_dir_all(dir.path()).unwrap();
    let path = dir.path().join("invalid.json");
    std::fs::write(&path, r#"{"format":"asterdrive.runtime-config","version":1,"configs":[{"key":"custom.must_not_write","value":"first","source":"custom","visibility":"private","is_sensitive":false},{"key":"auth_cookie_secure","value":"yes","source":"system","visibility":"private","is_sensitive":false}],"omitted_sensitive_keys":[]}"#).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let output = run_aster_drive(&[
        "config",
        "--database-url",
        &database_url,
        "import",
        "--input-file",
        path.to_str().unwrap(),
    ]);
    assert!(!output.status.success());
    let get = run_aster_drive(&[
        "config",
        "--database-url",
        &database_url,
        "get",
        "--key",
        "custom.must_not_write",
    ]);
    assert!(!get.status.success());
    assert!(!String::from_utf8_lossy(&output.stderr).contains("first"));

    let legacy_path = dir.path().join("legacy.json");
    std::fs::write(
        &legacy_path,
        r#"[{"key":"custom.legacy","value":"ignored"}]"#,
    )
    .unwrap();
    std::fs::set_permissions(&legacy_path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let legacy = run_aster_drive(&[
        "config",
        "--database-url",
        &database_url,
        "import",
        "--input-file",
        legacy_path.to_str().unwrap(),
    ]);
    assert!(!legacy.status.success());
}

#[cfg(unix)]
#[tokio::test]
async fn config_secret_file_input_requires_private_permissions_and_never_echoes_value() {
    use std::os::unix::fs::PermissionsExt;

    let database_url = setup_database_url().await;
    let dir = aster_forge_utils::raii::TempDirGuard::new(
        std::env::temp_dir().join(format!("aster-cli-secret-input-{}", uuid::Uuid::new_v4())),
        "secret config input fixture",
    );
    std::fs::create_dir_all(dir.path()).unwrap();
    let secret = "secret-file-marker";
    let private_path = dir.path().join("private");
    std::fs::write(&private_path, secret).unwrap();
    std::fs::set_permissions(&private_path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let output = run_aster_drive(&[
        "config",
        "--database-url",
        &database_url,
        "set",
        "--key",
        "mail_smtp_password",
        "--value-file",
        private_path.to_str().unwrap(),
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains(secret));
    assert!(!String::from_utf8_lossy(&output.stderr).contains(secret));

    let public_path = dir.path().join("public");
    std::fs::write(&public_path, secret).unwrap();
    std::fs::set_permissions(&public_path, std::fs::Permissions::from_mode(0o644)).unwrap();
    let output = run_aster_drive(&[
        "config",
        "--database-url",
        &database_url,
        "set",
        "--key",
        "mail_smtp_password",
        "--value-file",
        public_path.to_str().unwrap(),
    ]);
    assert!(!output.status.success());
    assert!(!String::from_utf8_lossy(&output.stdout).contains(secret));
    assert!(!String::from_utf8_lossy(&output.stderr).contains(secret));
}

#[tokio::test]
async fn cli_database_target_precedence_is_argument_then_cli_env_then_instance_config() {
    let configured_database = setup_database_url().await;
    let environment_database = setup_database_url().await;
    let argument_database = setup_database_url().await;
    for (database, value) in [
        (&configured_database, "config-file"),
        (&environment_database, "cli-env"),
        (&argument_database, "argument"),
    ] {
        let output = run_aster_drive(&[
            "config",
            "--database-url",
            database,
            "set",
            "--key",
            "custom.target_precedence",
            "--value",
            value,
        ]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let data_dir =
        std::env::temp_dir().join(format!("aster-cli-config-target-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&data_dir).unwrap();
    std::fs::write(
        data_dir.join("config.toml"),
        format!("[database]\nurl = \"{configured_database}\"\n"),
    )
    .unwrap();
    let data_arg = data_dir.to_str().unwrap();

    let get = |database: Option<&str>, env: &[(&str, &str)]| {
        let mut args = vec!["--data-dir", data_arg, "config"];
        if let Some(database) = database {
            args.extend(["--database-url", database]);
        }
        args.extend(["get", "--key", "custom.target_precedence"]);
        run_aster_drive_with_env(&args, env)
    };
    let from_config = get(None, &[]);
    assert_eq!(
        serde_json::from_slice::<Value>(&from_config.stdout).unwrap()["data"]["value"],
        "config-file"
    );
    let from_environment = get(None, &[("ASTER_CLI_DATABASE_URL", &environment_database)]);
    assert_eq!(
        serde_json::from_slice::<Value>(&from_environment.stdout).unwrap()["data"]["value"],
        "cli-env"
    );
    let from_argument = get(
        Some(&argument_database),
        &[("ASTER_CLI_DATABASE_URL", &environment_database)],
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&from_argument.stdout).unwrap()["data"]["value"],
        "argument"
    );

    std::fs::remove_dir_all(data_dir).unwrap();
}

#[test]
fn cli_instance_location_works_from_another_directory_and_missing_instance_is_side_effect_free() {
    let root = aster_forge_utils::raii::TempDirGuard::new(
        std::env::temp_dir().join(format!("aster-cli-instance-{}", uuid::Uuid::new_v4())),
        "CLI instance location fixture",
    );
    std::fs::create_dir_all(root.path()).unwrap();
    let cwd = root.path().join("elsewhere");
    let data = root.path().join("instance-data");
    std::fs::create_dir_all(&cwd).unwrap();
    std::fs::create_dir_all(&data).unwrap();
    let output = run_aster_drive_with_env_in_dir(&["config", "list"], &[], &cwd);
    assert!(!output.status.success());
    assert!(!cwd.join("data").exists());

    let missing = run_aster_drive_with_env_in_dir(
        &["--data-dir", data.to_str().unwrap(), "config", "list"],
        &[],
        &cwd,
    );
    assert!(!missing.status.success());
    assert!(!data.join("config.toml").exists());
    assert!(!data.join("asterdrive.db").exists());
}

#[tokio::test]
async fn test_root_binary_config_human_output_supports_forced_color() {
    let database_url = setup_database_url().await;

    let output = run_aster_drive_with_env(
        &[
            "config",
            "--database-url",
            &database_url,
            "--output-format",
            "human",
            "list",
        ],
        &[("CLICOLOR_FORCE", "1")],
    );
    assert!(
        output.status.success(),
        "list stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("forced-color stdout should be utf-8");
    assert!(stdout.contains("\u{1b}["));
    assert!(stdout.contains("Configuration list"));
}

#[tokio::test]
async fn test_root_binary_doctor_defaults_to_json_output() {
    let database_url = setup_ready_database_url().await;

    let output = run_aster_drive(&["doctor", "--database-url", &database_url]);
    assert!(
        output.status.success(),
        "doctor stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("doctor stdout should be utf-8");
    let report: Value = serde_json::from_str(&stdout).expect("doctor output should be json");
    assert_eq!(report["ok"], true);
    let redacted_database_url = report["data"]["database_url"]
        .as_str()
        .expect("doctor database url should be a string");
    assert!(redacted_database_url.starts_with("sqlite:///.../asterdrive-cli-ready-test-"));
    assert!(redacted_database_url.ends_with(".db?mode=rwc"));
    assert_eq!(report["data"]["status"], "warn");
    assert_eq!(report["data"]["summary"]["fail"], 0);
    assert!(
        report["data"]["summary"]["warn"]
            .as_u64()
            .expect("warn count should exist")
            >= 1
    );
}

#[tokio::test]
async fn test_root_binary_doctor_redacts_database_url_query_credentials() {
    let database_path = std::env::temp_dir().join(format!(
        "asterdrive-cli-redaction-{}.db",
        uuid::Uuid::new_v4()
    ));
    let database_url = format!(
        "sqlite://{}?mode=rwc&password=secret%2Fvalue&DB_TOKEN=doctor-query-token-credential",
        database_path.display()
    );

    let output = run_aster_drive(&["doctor", "--database-url", &database_url]);
    let stdout = String::from_utf8(output.stdout).expect("doctor stdout should be utf-8");
    let report: Value = serde_json::from_str(&stdout).expect("doctor output should be json");
    assert_eq!(report["data"]["status"], "fail");
    assert!(stdout.contains("password=***"));
    assert!(stdout.contains("DB_TOKEN=***"));
    assert!(!stdout.contains("secret%2Fvalue"));
    assert!(!stdout.contains("doctor-query-token-credential"));
}

#[tokio::test]
async fn test_root_binary_doctor_human_output_redacts_database_url_query_credentials() {
    let database_path = std::env::temp_dir().join(format!(
        "asterdrive-cli-human-redaction-{}.db",
        uuid::Uuid::new_v4()
    ));
    let database_url = format!(
        "sqlite://{}?mode=rwc&password=human-secret%2Fvalue&ACCESS_TOKEN=human-token",
        database_path.display()
    );

    let output = run_aster_drive(&[
        "doctor",
        "--database-url",
        &database_url,
        "--output-format",
        "human",
    ]);
    let stdout = String::from_utf8(output.stdout).expect("doctor human stdout should be utf-8");
    assert!(stdout.contains("Database:"));
    assert!(stdout.contains("password=***"));
    assert!(stdout.contains("ACCESS_TOKEN=***"));
    assert!(!stdout.contains("human-secret%2Fvalue"));
    assert!(!stdout.contains("human-token"));
}

#[tokio::test]
async fn test_root_binary_doctor_human_output_is_readable() {
    let database_url = setup_ready_database_url().await;

    let output = run_aster_drive(&[
        "doctor",
        "--database-url",
        &database_url,
        "--output-format",
        "human",
    ]);
    assert!(
        output.status.success(),
        "doctor stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("doctor human stdout should be utf-8");
    assert!(stdout.contains("System doctor"));
    assert!(stdout.contains("Database:"));
    assert!(stdout.contains("Mode:"));
    assert!(stdout.contains("Status:"));
    assert!(stdout.contains("Checks:"));
    assert!(stdout.contains("Database connection"));
    assert!(stdout.contains("Mail configuration"));
    assert!(stdout.contains("hint:"));
    assert!(serde_json::from_str::<Value>(&stdout).is_err());
}

#[tokio::test]
async fn test_root_binary_doctor_human_output_supports_forced_color() {
    let database_url = setup_ready_database_url().await;

    let output = run_aster_drive_with_env(
        &[
            "doctor",
            "--database-url",
            &database_url,
            "--output-format",
            "human",
        ],
        &[("CLICOLOR_FORCE", "1")],
    );
    assert!(
        output.status.success(),
        "doctor stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout =
        String::from_utf8(output.stdout).expect("doctor forced-color stdout should be utf-8");
    assert!(stdout.contains("\u{1b}["));
    assert!(stdout.contains("System doctor"));
}

#[tokio::test]
async fn test_root_binary_doctor_strict_turns_warnings_into_nonzero_exit() {
    let database_url = setup_ready_database_url().await;

    let output = run_aster_drive(&["doctor", "--database-url", &database_url, "--strict"]);
    assert!(
        !output.status.success(),
        "doctor --strict should fail when warnings are present"
    );

    let stdout = String::from_utf8(output.stdout).expect("doctor stdout should be utf-8");
    let report: Value = serde_json::from_str(&stdout).expect("doctor output should stay json");
    assert_eq!(report["ok"], true);
    assert_eq!(report["data"]["strict"], true);
    assert_eq!(report["data"]["status"], "fail");
    assert_eq!(report["data"]["summary"]["fail"], 0);
    assert!(
        report["data"]["summary"]["warn"]
            .as_u64()
            .expect("warn count should exist")
            >= 1
    );
}

#[tokio::test]
async fn test_root_binary_doctor_warns_when_public_site_url_uses_http() {
    let database_url = setup_ready_database_url().await;

    let set_output = run_aster_drive(&[
        "config",
        "--database-url",
        &database_url,
        "set",
        "--key",
        "public_site_url",
        "--value",
        r#"["http://drive.example.com"]"#,
    ]);
    assert!(
        set_output.status.success(),
        "set stderr: {}",
        String::from_utf8_lossy(&set_output.stderr)
    );

    let output = run_aster_drive(&["doctor", "--database-url", &database_url]);
    assert!(
        output.status.success(),
        "doctor stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("doctor stdout should be utf-8");
    let report: Value = serde_json::from_str(&stdout).expect("doctor output should be json");
    let checks = report["data"]["checks"]
        .as_array()
        .expect("doctor checks should be an array");
    let public_site_url_check = checks
        .iter()
        .find(|check| check["name"] == "public_site_url")
        .expect("public_site_url check should exist");

    assert_eq!(public_site_url_check["status"], "warn");
    assert_eq!(
        public_site_url_check["summary"],
        "public_site_url uses insecure HTTP"
    );
    assert!(
        public_site_url_check["details"]
            .as_array()
            .is_some_and(|details| details.iter().any(
                |detail| detail.as_str() == Some(r#"configured=["http://drive.example.com"]"#)
            ))
    );
}

#[tokio::test]
async fn test_root_binary_doctor_reports_sqlite_search_acceleration_ready() {
    let database_url = setup_ready_database_url().await;

    let output = run_aster_drive(&["doctor", "--database-url", &database_url]);
    assert!(
        output.status.success(),
        "doctor stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("doctor stdout should be utf-8");
    let report: Value = serde_json::from_str(&stdout).expect("doctor output should be json");
    let checks = report["data"]["checks"]
        .as_array()
        .expect("doctor checks should be an array");
    let sqlite_search_check = checks
        .iter()
        .find(|check| check["name"] == "sqlite_search_acceleration")
        .expect("sqlite_search_acceleration check should exist");

    assert_eq!(sqlite_search_check["status"], "ok");
    assert_eq!(
        sqlite_search_check["summary"],
        "FTS5 trigram search acceleration ready"
    );
    assert!(
        sqlite_search_check["details"]
            .as_array()
            .is_some_and(|details| details.iter().any(|detail| detail
                .as_str()
                .is_some_and(|detail| detail.starts_with("sqlite_version="))))
    );
}

#[tokio::test]
async fn test_migrations_use_current_baseline_for_fresh_install() {
    let database_url = setup_empty_database_url("asterdrive-cli-fresh-baseline-test").await;
    let db = db::connect_with_metrics(
        &DatabaseConfig {
            url: database_url.clone().into(),
            pool_size: 1,
            retry_count: 0,
        },
        aster_drive_metrics::NoopMetrics::arc(),
    )
    .await
    .unwrap();
    Migrator::up(&db, None).await.unwrap();
    let versions = applied_migration_versions(&db, DbBackend::Sqlite).await;
    assert_eq!(
        versions,
        aster_drive_migration::current_migration_names(),
        "fresh install should stamp all current migrations"
    );
    for column in [
        "last_probe_error",
        "last_probe_at",
        "tunnel_runtime_error",
        "tunnel_last_handshake_at",
    ] {
        assert!(
            column_exists(&db, DbBackend::Sqlite, "managed_followers", column).await,
            "fresh schema should contain renamed remote-node telemetry column {column}"
        );
    }
    for column in [
        "last_error",
        "last_checked_at",
        "tunnel_last_error",
        "tunnel_last_seen_at",
    ] {
        assert!(
            !column_exists(&db, DbBackend::Sqlite, "managed_followers", column).await,
            "fresh schema should not retain legacy remote-node telemetry column {column}"
        );
    }
}

#[tokio::test]
async fn test_remote_node_telemetry_rename_preserves_values_on_sqlite_up_and_down() {
    let database_url =
        setup_empty_database_url("asterdrive-cli-remote-node-telemetry-rename-test").await;
    let db = db::connect_with_metrics(
        &DatabaseConfig {
            url: database_url.into(),
            pool_size: 1,
            retry_count: 0,
        },
        aster_drive_metrics::NoopMetrics::arc(),
    )
    .await
    .unwrap();
    let migrations = CurrentMigrator::migrations();
    let rename_index = migrations
        .iter()
        .position(|migration| migration.name() == "m20260821_000001_rename_remote_node_telemetry")
        .expect("remote-node telemetry rename migration should be registered");
    CurrentMigrator::up(
        &db,
        Some(u32::try_from(rename_index).expect("migration index should fit u32")),
    )
    .await
    .unwrap();

    db.execute_unprepared(
        "INSERT INTO managed_followers \
         (name, base_url, access_key, secret_key, is_enabled, last_capabilities, \
          last_error, last_checked_at, tunnel_last_error, tunnel_last_seen_at, created_at, updated_at) \
         VALUES ('legacy-node', '', 'legacy-access', 'legacy-secret', 1, '{\"v\":1}', \
                 'probe failed', '2026-08-20T01:02:03Z', 'tunnel failed', '2026-08-20T02:03:04Z', \
                 '2026-08-20T01:02:03Z', '2026-08-20T01:02:03Z')",
    )
    .await
    .unwrap();

    CurrentMigrator::up(&db, None).await.unwrap();
    assert_eq!(
        scalar_string(
            &db,
            DbBackend::Sqlite,
            "SELECT last_probe_error FROM managed_followers WHERE name = 'legacy-node'",
        )
        .await,
        "probe failed"
    );
    assert_eq!(
        scalar_string(
            &db,
            DbBackend::Sqlite,
            "SELECT tunnel_runtime_error FROM managed_followers WHERE name = 'legacy-node'",
        )
        .await,
        "tunnel failed"
    );
    assert_eq!(
        scalar_i64(
            &db,
            DbBackend::Sqlite,
            "SELECT COUNT(*) FROM managed_followers WHERE name = 'legacy-node' AND last_probe_at IS NOT NULL",
        )
        .await,
        1
    );
    assert_eq!(
        scalar_string(
            &db,
            DbBackend::Sqlite,
            "SELECT tunnel_last_handshake_at FROM managed_followers WHERE name = 'legacy-node'",
        )
        .await,
        "2026-08-20T02:03:04Z"
    );
    assert_eq!(
        scalar_i64(
            &db,
            DbBackend::Sqlite,
            "SELECT COUNT(*) FROM managed_followers WHERE name = 'legacy-node' AND tunnel_last_handshake_at IS NULL",
        )
        .await,
        0
    );

    let rollback_steps =
        u32::try_from(migrations.len() - rename_index).expect("rollback step count should fit u32");
    CurrentMigrator::down(&db, Some(rollback_steps))
        .await
        .unwrap();
    assert_eq!(
        scalar_string(
            &db,
            DbBackend::Sqlite,
            "SELECT last_error FROM managed_followers WHERE name = 'legacy-node'",
        )
        .await,
        "probe failed"
    );
    assert_eq!(
        scalar_string(
            &db,
            DbBackend::Sqlite,
            "SELECT tunnel_last_error FROM managed_followers WHERE name = 'legacy-node'",
        )
        .await,
        "tunnel failed"
    );
    assert_eq!(
        scalar_string(
            &db,
            DbBackend::Sqlite,
            "SELECT last_checked_at FROM managed_followers WHERE name = 'legacy-node'",
        )
        .await,
        "2026-08-20T01:02:03Z"
    );
    assert_eq!(
        scalar_string(
            &db,
            DbBackend::Sqlite,
            "SELECT tunnel_last_seen_at FROM managed_followers WHERE name = 'legacy-node'",
        )
        .await,
        "2026-08-20T02:03:04Z"
    );
    assert!(
        column_exists(
            &db,
            DbBackend::Sqlite,
            "managed_followers",
            "last_checked_at"
        )
        .await
    );
}

#[tokio::test]
async fn test_migration_backfills_storage_migration_result_renamed_opaque_count() {
    let database_url =
        setup_empty_database_url("asterdrive-cli-storage-migration-result-backfill-test").await;
    let db = db::connect_with_metrics(
        &DatabaseConfig {
            url: database_url.into(),
            pool_size: 1,
            retry_count: 0,
        },
        aster_drive_metrics::NoopMetrics::arc(),
    )
    .await
    .unwrap();

    let migration_count_before_opaque_backfill = u32::try_from(
        aster_drive_migration::current_migration_names()
            .iter()
            .position(|name| name.contains("add_storage_migration_opaque_rename_count"))
            .expect("opaque rename count migration should exist"),
    )
    .expect("migration count should fit u32");
    CurrentMigrator::up(&db, Some(migration_count_before_opaque_backfill))
        .await
        .unwrap();

    let old_result = serde_json::json!({
        "source_policy_id": 1,
        "target_policy_id": 2,
        "scanned_blobs": 3,
        "migrated_blobs": 2,
        "merged_blobs": 1,
        "skipped_blobs": 0,
        "failed_blobs": 0,
        "migrated_bytes": 4096
    });
    let now = Utc::now();
    let payload_json = serde_json::json!({
        "source_policy_id": 1,
        "target_policy_id": 2,
        "plan_hash": "0".repeat(64),
        "source_policy_updated_at": now,
        "target_policy_updated_at": now
    })
    .to_string();
    // This fixture intentionally targets the historical schema before
    // `runtime_json` existed, so it must not use the current ActiveModel.
    db.execute_raw(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "INSERT INTO background_tasks (
            kind, status, creator_user_id, team_id, share_id, display_name,
            payload_json, result_json, steps_json, progress_current, progress_total,
            status_text, attempt_count, max_attempts, next_run_at, processing_token,
            processing_started_at, last_heartbeat_at, lease_expires_at, started_at,
            finished_at, last_error, failure_can_retry, expires_at, created_at, updated_at
        ) VALUES (
            ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?
        )",
        vec![
            aster_drive_model::types::BackgroundTaskKind::StoragePolicyMigration
                .as_str()
                .into(),
            aster_drive_model::types::BackgroundTaskStatus::Succeeded
                .as_str()
                .into(),
            Option::<i64>::None.into(),
            Option::<i64>::None.into(),
            Option::<i64>::None.into(),
            "legacy storage migration".into(),
            payload_json.into(),
            Some(old_result.to_string()).into(),
            Option::<String>::None.into(),
            3_i64.into(),
            3_i64.into(),
            Option::<String>::None.into(),
            1_i64.into(),
            3_i64.into(),
            now.into(),
            0_i64.into(),
            Option::<chrono::DateTime<Utc>>::None.into(),
            Option::<chrono::DateTime<Utc>>::None.into(),
            Option::<chrono::DateTime<Utc>>::None.into(),
            Some(now).into(),
            Some(now).into(),
            Option::<String>::None.into(),
            Option::<bool>::None.into(),
            (now + Duration::hours(24)).into(),
            now.into(),
            now.into(),
        ],
    ))
    .await
    .unwrap();

    CurrentMigrator::up(&db, None).await.unwrap();

    let task = aster_drive_model::entities::background_task::Entity::find()
        .one(&db)
        .await
        .unwrap()
        .expect("legacy task should remain");
    let result = task
        .result_json
        .as_ref()
        .expect("legacy task result should remain");
    let parsed: aster_drive::services::task::types::StoragePolicyMigrationTaskResult =
        serde_json::from_str(result.as_ref())
            .expect("backfilled storage migration result should match current schema");
    assert_eq!(parsed.renamed_opaque_blobs, 0);
}

#[tokio::test]
async fn test_migrations_reject_unsupported_historical_migration_history() {
    let database_url = setup_empty_database_url("asterdrive-cli-old-migration-test").await;
    let db = db::connect_with_metrics(
        &DatabaseConfig {
            url: database_url.clone().into(),
            pool_size: 1,
            retry_count: 0,
        },
        aster_drive_metrics::NoopMetrics::arc(),
    )
    .await
    .unwrap();
    Migrator::up(&db, None).await.unwrap();
    db.execute_raw(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "INSERT INTO seaql_migrations (version, applied_at) VALUES (?, ?)",
        ["m20260502_000001_baseline_schema".into(), 1_i64.into()],
    ))
    .await
    .unwrap();
    db.close().await.unwrap();

    let db = db::connect_with_metrics(
        &DatabaseConfig {
            url: database_url.into(),
            pool_size: 1,
            retry_count: 0,
        },
        aster_drive_metrics::NoopMetrics::arc(),
    )
    .await
    .unwrap();
    let error = Migrator::up(&db, None)
        .await
        .expect_err("unsupported historical migration rows should be rejected");
    let stderr = error.to_string();
    assert!(
        stderr.contains("m20260502_000001_baseline_schema"),
        "error should include the unsupported migration name: {stderr}"
    );
    assert!(
        stderr.contains("unknown migration versions"),
        "error should explain that historical migration rows are no longer accepted: {stderr}"
    );
}

#[tokio::test]
async fn test_migrations_reject_non_prefix_current_migration_history() {
    let database_url = setup_empty_database_url("asterdrive-cli-non-prefix-migration-test").await;
    let db = db::connect_with_metrics(
        &DatabaseConfig {
            url: database_url.into(),
            pool_size: 1,
            retry_count: 0,
        },
        aster_drive_metrics::NoopMetrics::arc(),
    )
    .await
    .unwrap();
    Migrator::up(&db, None).await.unwrap();
    let current_names = aster_drive_migration::current_migration_names();
    db.execute_raw(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "DELETE FROM seaql_migrations WHERE version = ?",
        [current_names[0].clone().into()],
    ))
    .await
    .unwrap();

    let history = aster_drive_migration::inspect_migration_history(&db)
        .await
        .unwrap();
    assert_eq!(
        history.track,
        aster_drive_migration::MigrationTrack::Unknown
    );

    let error = Migrator::up(&db, None)
        .await
        .expect_err("non-prefix migration history should be rejected");
    let stderr = error.to_string();
    assert!(
        stderr.contains("non-prefix migration history"),
        "error should identify the non-prefix migration history: {stderr}"
    );
}

#[tokio::test]
async fn test_migrations_reject_existing_schema_with_empty_history() {
    let database_url = setup_empty_database_url("asterdrive-cli-empty-history-schema-test").await;
    let db = db::connect_with_metrics(
        &DatabaseConfig {
            url: database_url.into(),
            pool_size: 1,
            retry_count: 0,
        },
        aster_drive_metrics::NoopMetrics::arc(),
    )
    .await
    .unwrap();
    db.execute_raw(Statement::from_string(
        DbBackend::Sqlite,
        "CREATE TABLE users (id INTEGER PRIMARY KEY)".to_string(),
    ))
    .await
    .unwrap();

    let history = aster_drive_migration::inspect_migration_history(&db)
        .await
        .unwrap();
    assert_eq!(
        history.track,
        aster_drive_migration::MigrationTrack::Unknown
    );

    let error = Migrator::up(&db, None)
        .await
        .expect_err("existing schema with empty history should be rejected");
    let stderr = error.to_string();
    assert!(
        stderr.contains("empty migration history with existing schema objects"),
        "error should identify existing schema objects without migration history: {stderr}"
    );
}

#[tokio::test]
async fn test_root_binary_doctor_deep_fix_repairs_counters() {
    let database_url = setup_database_url().await;
    let state = common::setup_with_database_url(&database_url).await;
    let db = state.writer_db().clone();
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let file_id = upload_test_file!(app, token);

    let user = user_repo::find_by_email(&db, "test@example.com")
        .await
        .unwrap()
        .expect("test user should exist");
    let file = file_repo::find_by_id(&db, file_id).await.unwrap();
    let blob = file_repo::find_blob_by_id(&db, file.blob_id).await.unwrap();

    let mut user_active: aster_drive_model::entities::user::ActiveModel = user.into();
    user_active.storage_used = Set(0);
    user_active.update(&db).await.unwrap();

    let mut blob_active: aster_drive_model::entities::file_blob::ActiveModel = blob.into();
    blob_active.ref_count = Set(0);
    blob_active.updated_at = Set(Utc::now());
    blob_active.update(&db).await.unwrap();

    let output = run_aster_drive(&["doctor", "--database-url", &database_url, "--deep", "--fix"]);
    assert!(
        output.status.success(),
        "doctor --deep --fix stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("doctor stdout should be utf-8");
    let report: Value = serde_json::from_str(&stdout).expect("doctor output should be json");
    assert_eq!(report["ok"], true);
    assert_eq!(report["data"]["deep"], true);
    assert_eq!(report["data"]["fix"], true);

    let checks = report["data"]["checks"]
        .as_array()
        .expect("doctor checks should be an array");
    let usage_check = checks
        .iter()
        .find(|check| check["name"] == "storage_usage_consistency")
        .expect("storage usage check should exist");
    assert_eq!(usage_check["status"], "ok");
    assert!(
        usage_check["summary"]
            .as_str()
            .is_some_and(|summary| summary.contains("fixed 1"))
    );

    let ref_count_check = checks
        .iter()
        .find(|check| check["name"] == "blob_ref_counts")
        .expect("blob ref_count check should exist");
    assert_eq!(ref_count_check["status"], "ok");
    assert!(
        ref_count_check["summary"]
            .as_str()
            .is_some_and(|summary| summary.contains("fixed 1"))
    );
}

#[tokio::test]
async fn test_root_binary_doctor_scope_and_policy_filter_limit_deep_checks() {
    let database_url = setup_database_url().await;
    let state = common::setup_with_database_url(&database_url).await;
    let db = state.writer_db().clone();
    let policy = policy_repo::find_default(&db)
        .await
        .unwrap()
        .expect("default policy should exist");
    let app = create_test_app!(state);
    let (token, _) = register_and_login!(app);
    let file_id = upload_test_file!(app, token);

    let user = user_repo::find_by_email(&db, "test@example.com")
        .await
        .unwrap()
        .expect("test user should exist");
    let file = file_repo::find_by_id(&db, file_id).await.unwrap();
    let blob = file_repo::find_blob_by_id(&db, file.blob_id).await.unwrap();

    let mut user_active: aster_drive_model::entities::user::ActiveModel = user.into();
    user_active.storage_used = Set(0);
    user_active.update(&db).await.unwrap();

    let mut blob_active: aster_drive_model::entities::file_blob::ActiveModel = blob.into();
    blob_active.ref_count = Set(0);
    blob_active.updated_at = Set(Utc::now());
    blob_active.update(&db).await.unwrap();

    let output = run_aster_drive(&[
        "doctor",
        "--database-url",
        &database_url,
        "--scope",
        "blob-ref-counts",
        "--policy-id",
        &policy.id.to_string(),
    ]);
    assert!(
        output.status.success(),
        "doctor scoped stdout: {}\ndoctor scoped stderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("doctor stdout should be utf-8");
    let report: Value = serde_json::from_str(&stdout).expect("doctor output should be json");
    assert_eq!(report["data"]["deep"], true);
    assert_eq!(report["data"]["policy_id"], policy.id);
    assert!(
        report["data"]["scopes"]
            .as_array()
            .is_some_and(|scopes| scopes.len() == 1 && scopes[0] == "blob_ref_counts")
    );

    let checks = report["data"]["checks"]
        .as_array()
        .expect("doctor checks should be an array");
    assert!(
        checks
            .iter()
            .any(|check| check["name"] == "blob_ref_counts")
    );
    assert!(checks.iter().any(|check| check["name"] == "policy_filter"));
    assert!(
        !checks
            .iter()
            .any(|check| check["name"] == "storage_usage_consistency")
    );
    assert!(
        !checks
            .iter()
            .any(|check| check["name"] == "tracked_blob_objects")
    );
    assert!(
        !checks
            .iter()
            .any(|check| check["name"] == "folder_tree_integrity")
    );
}

#[tokio::test]
async fn test_root_binary_doctor_exits_nonzero_when_storage_policy_setup_is_missing() {
    let database_url = setup_database_url().await;

    let output = run_aster_drive(&["doctor", "--database-url", &database_url]);
    assert!(
        !output.status.success(),
        "doctor should fail on incomplete setup"
    );

    let stdout = String::from_utf8(output.stdout).expect("doctor stdout should be utf-8");
    let report: Value = serde_json::from_str(&stdout).expect("doctor output should stay json");
    assert_eq!(report["ok"], true);
    assert_eq!(report["data"]["status"], "fail");
    let checks = report["data"]["checks"]
        .as_array()
        .expect("doctor checks should be an array");
    assert!(
        checks
            .iter()
            .any(|check| { check["name"] == "storage_policies" && check["status"] == "fail" })
    );
}

#[tokio::test]
async fn test_root_binary_config_delete_rejects_system_config_key() {
    let database_url = setup_database_url().await;

    let output = run_aster_drive(&[
        "config",
        "--database-url",
        &database_url,
        "delete",
        "--key",
        "public_site_url",
    ]);
    assert!(
        !output.status.success(),
        "delete should fail for system config"
    );

    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf-8");
    let err_json: Value = serde_json::from_str(&stderr).expect("error output json");
    assert_eq!(err_json["ok"], false);
    assert_eq!(err_json["error"]["code"], "E013");
    assert!(
        err_json["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("cannot delete system configuration")
    );
}

#[tokio::test]
async fn test_root_binary_config_json_error_does_not_create_default_config() {
    let database_url = setup_database_url().await;
    let current_dir = std::env::temp_dir().join(format!(
        "asterdrive-cli-config-clean-json-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&current_dir).unwrap();

    let output = run_aster_drive_with_env_in_dir(
        &[
            "config",
            "--database-url",
            &database_url,
            "--output-format",
            "json",
            "delete",
            "--key",
            "public_site_url",
        ],
        &[],
        &current_dir,
    );

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let err_json: Value =
        serde_json::from_slice(&output.stderr).expect("stderr should be one JSON document");
    assert_eq!(err_json["ok"], false);
    assert_eq!(err_json["error"]["code"], "E013");
    assert!(!current_dir.join("data/config.toml").exists());
    assert!(!current_dir.join("data/asterdrive.db").exists());

    std::fs::remove_dir_all(current_dir).unwrap();
}

#[tokio::test]
async fn test_root_binary_database_migrate_rejects_current_history_with_missing_runtime_lease_table()
 {
    let source_database_url = setup_database_url().await;
    let target_database_url = setup_database_url().await;
    let target_db = db::connect_with_metrics(
        &DatabaseConfig {
            url: target_database_url.clone().into(),
            pool_size: 1,
            retry_count: 0,
        },
        aster_drive_metrics::NoopMetrics::arc(),
    )
    .await
    .unwrap();
    target_db
        .execute_raw(Statement::from_string(
            DbBackend::Sqlite,
            "DROP TABLE runtime_leases",
        ))
        .await
        .unwrap();
    target_db.close().await.unwrap();

    let output = run_aster_drive(&[
        "database-migrate",
        "--verify-only",
        "--source-database-url",
        &source_database_url,
        "--target-database-url",
        &target_database_url,
    ]);

    assert!(
        !output.status.success(),
        "verify-only must reject migration history/schema drift"
    );
    let error_json: Value = serde_json::from_slice(&output.stderr)
        .expect("schema drift failure should remain structured JSON");
    assert!(
        error_json["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains(aster_forge_db::RUNTIME_LEASES_TABLE)),
        "schema drift error should name the missing runtime lease table: {error_json}"
    );
}

#[tokio::test]
async fn test_root_binary_database_migrate_sqlite_to_postgres_happy_path() {
    let source_db_path = std::env::temp_dir().join(format!(
        "asterdrive-cli-migrate-{}.db",
        uuid::Uuid::new_v4()
    ));
    let source_database_url = format!("sqlite://{}?mode=rwc", source_db_path.display());
    let file_id = seed_migration_fixture(&source_database_url).await;

    let target_database_url = common::postgres_test_database_url().await;

    let output = run_aster_drive(&[
        "database-migrate",
        "--source-database-url",
        &source_database_url,
        "--target-database-url",
        &target_database_url,
    ]);
    assert!(
        output.status.success(),
        "database-migrate stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let output_json: Value =
        serde_json::from_slice(&output.stdout).expect("database-migrate output should be json");
    assert_eq!(output_json["ok"], true);
    assert_eq!(output_json["data"]["mode"], "apply");
    assert_eq!(output_json["data"]["ready_to_cutover"], true);
    assert_eq!(output_json["data"]["rolled_back"], false);
    assert_eq!(output_json["data"]["resume"]["enabled"], true);
    assert_eq!(output_json["data"]["resume"]["resumed"], false);
    let migrated_tables = output_json["data"]["tables"]
        .as_array()
        .expect("database-migrate tables should be an array");
    assert!(
        migrated_tables
            .iter()
            .any(|table| table["name"] == aster_forge_db::SCHEDULED_TASKS_TABLE)
    );
    assert!(
        migrated_tables
            .iter()
            .any(|table| table["name"] == aster_forge_db::RUNTIME_LEASES_TABLE)
    );
    assert_eq!(
        output_json["data"]["source"]["database_url"],
        format!(
            "sqlite:///.../{}?mode=rwc",
            source_db_path
                .file_name()
                .and_then(|name| name.to_str())
                .expect("source db file name should be valid utf-8")
        )
    );
    assert_eq!(
        output_json["data"]["target"]["database_url"],
        redact_database_url(&target_database_url)
    );

    let target_db = db::connect_with_metrics(
        &DatabaseConfig {
            url: target_database_url.clone().into(),
            pool_size: 1,
            retry_count: 0,
        },
        aster_drive_metrics::NoopMetrics::arc(),
    )
    .await
    .unwrap();
    let checkpoint_source_url = scalar_string(
        &target_db,
        DbBackend::Postgres,
        "SELECT source_database_url FROM aster_cli_database_migrations LIMIT 1",
    )
    .await;
    let checkpoint_target_url = scalar_string(
        &target_db,
        DbBackend::Postgres,
        "SELECT target_database_url FROM aster_cli_database_migrations LIMIT 1",
    )
    .await;
    assert_eq!(
        checkpoint_source_url,
        output_json["data"]["source"]["database_url"]
    );
    assert_eq!(
        checkpoint_target_url,
        output_json["data"]["target"]["database_url"]
    );

    assert_migrated_fixture(&target_database_url, DbBackend::Postgres, file_id).await;
}

#[tokio::test]
async fn test_root_binary_database_migrate_postgres_to_mysql_with_progress() {
    let source = async {
        let database_url = common::postgres_test_database_url().await;
        let file_id = seed_migration_fixture(&database_url).await;
        (database_url, file_id)
    };
    let ((source_database_url, file_id), target_database_url) =
        tokio::join!(source, common::mysql_test_database_url());

    let output = run_aster_drive_with_env(
        &[
            "database-migrate",
            "--source-database-url",
            &source_database_url,
            "--target-database-url",
            &target_database_url,
        ],
        &[
            ("ASTER_CLI_PROGRESS", "1"),
            ("ASTER_CLI_COPY_BATCH_SIZE", "1"),
        ],
    );
    assert!(
        output.status.success(),
        "database-migrate stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let output_json: Value =
        serde_json::from_slice(&output.stdout).expect("database-migrate output should be json");
    assert_eq!(output_json["ok"], true);
    assert_eq!(output_json["data"]["ready_to_cutover"], true);
    assert_eq!(output_json["data"]["resume"]["resumed"], false);

    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf-8");
    assert!(stderr.contains("[database-migrate] data_copy:"));

    assert_migrated_fixture(&target_database_url, DbBackend::MySql, file_id).await;
}

#[tokio::test]
async fn test_root_binary_database_migrate_mysql_to_sqlite_happy_path() {
    let source_database_url = common::mysql_test_database_url().await;
    let file_id = seed_migration_fixture(&source_database_url).await;

    let target_db_path = std::env::temp_dir().join(format!(
        "asterdrive-cli-migrate-target-{}.db",
        uuid::Uuid::new_v4()
    ));
    let target_database_url = format!("sqlite://{}?mode=rwc", target_db_path.display());

    let output = run_aster_drive(&[
        "database-migrate",
        "--source-database-url",
        &source_database_url,
        "--target-database-url",
        &target_database_url,
    ]);
    assert!(
        output.status.success(),
        "database-migrate stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let output_json: Value =
        serde_json::from_slice(&output.stdout).expect("database-migrate output should be json");
    assert_eq!(output_json["ok"], true);
    assert_eq!(output_json["data"]["ready_to_cutover"], true);

    assert_migrated_fixture(&target_database_url, DbBackend::Sqlite, file_id).await;
}

#[tokio::test]
async fn test_root_binary_database_migrate_sqlite_resume_from_checkpoint() {
    let source_db_path = std::env::temp_dir().join(format!(
        "asterdrive-cli-resume-source-{}.db",
        uuid::Uuid::new_v4()
    ));
    let source_database_url = format!("sqlite://{}?mode=rwc", source_db_path.display());
    let file_id = seed_migration_fixture(&source_database_url).await;

    let target_db_path = std::env::temp_dir().join(format!(
        "asterdrive-cli-resume-target-{}.db",
        uuid::Uuid::new_v4()
    ));
    let target_database_url = format!("sqlite://{}?mode=rwc", target_db_path.display());

    let first_output = run_aster_drive_with_env(
        &[
            "database-migrate",
            "--source-database-url",
            &source_database_url,
            "--target-database-url",
            &target_database_url,
        ],
        &[
            ("ASTER_CLI_COPY_BATCH_SIZE", "1"),
            ("ASTER_CLI_FAIL_AFTER_BATCHES", "1"),
        ],
    );
    assert!(
        !first_output.status.success(),
        "first migration should fail to exercise resume"
    );
    let error_json: Value =
        serde_json::from_slice(&first_output.stderr).expect("error stderr should stay json");
    assert_eq!(error_json["ok"], false);
    assert!(
        error_json["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("forced failure")
    );

    let target_db = db::connect_with_metrics(
        &DatabaseConfig {
            url: target_database_url.clone().into(),
            pool_size: 1,
            retry_count: 0,
        },
        aster_drive_metrics::NoopMetrics::arc(),
    )
    .await
    .unwrap();
    let checkpoint_rows = scalar_i64(
        &target_db,
        DbBackend::Sqlite,
        "SELECT COUNT(*) FROM aster_cli_database_migrations",
    )
    .await;
    assert_eq!(checkpoint_rows, 1);
    let checkpoint_source_url = scalar_string(
        &target_db,
        DbBackend::Sqlite,
        "SELECT source_database_url FROM aster_cli_database_migrations LIMIT 1",
    )
    .await;
    let checkpoint_target_url = scalar_string(
        &target_db,
        DbBackend::Sqlite,
        "SELECT target_database_url FROM aster_cli_database_migrations LIMIT 1",
    )
    .await;
    assert_ne!(checkpoint_source_url, source_database_url);
    assert_ne!(checkpoint_target_url, target_database_url);
    assert!(checkpoint_source_url.contains("/.../"));
    assert!(checkpoint_target_url.contains("/.../"));
    assert!(
        checkpoint_source_url.contains(
            source_db_path
                .file_name()
                .and_then(|name| name.to_str())
                .expect("source db file name should be valid utf-8")
        )
    );
    assert!(
        checkpoint_target_url.contains(
            target_db_path
                .file_name()
                .and_then(|name| name.to_str())
                .expect("target db file name should be valid utf-8")
        )
    );

    let second_output = run_aster_drive_with_env(
        &[
            "database-migrate",
            "--source-database-url",
            &source_database_url,
            "--target-database-url",
            &target_database_url,
        ],
        &[("ASTER_CLI_COPY_BATCH_SIZE", "1")],
    );
    assert!(
        second_output.status.success(),
        "resume stderr: {}",
        String::from_utf8_lossy(&second_output.stderr)
    );

    let output_json: Value =
        serde_json::from_slice(&second_output.stdout).expect("resume output should be json");
    assert_eq!(output_json["ok"], true);
    assert_eq!(output_json["data"]["ready_to_cutover"], true);
    assert_eq!(output_json["data"]["resume"]["enabled"], true);
    assert_eq!(output_json["data"]["resume"]["resumed"], true);

    assert_migrated_fixture(&target_database_url, DbBackend::Sqlite, file_id).await;
}

#[tokio::test]
async fn test_root_binary_database_migrate_sqlite_urls_without_mode_default_to_rwc() {
    let source_db_path = std::env::temp_dir().join(format!(
        "asterdrive-cli-source-no-mode-{}.db",
        uuid::Uuid::new_v4()
    ));
    let source_database_url_with_mode = format!("sqlite://{}?mode=rwc", source_db_path.display());
    let file_id = seed_migration_fixture(&source_database_url_with_mode).await;
    let source_database_url = format!("sqlite://{}", source_db_path.display());

    let target_db_path = std::env::temp_dir().join(format!(
        "asterdrive-cli-target-no-mode-{}.db",
        uuid::Uuid::new_v4()
    ));
    let target_database_url = format!("sqlite://{}", target_db_path.display());

    let output = run_aster_drive(&[
        "database-migrate",
        "--source-database-url",
        &source_database_url,
        "--target-database-url",
        &target_database_url,
    ]);
    assert!(
        output.status.success(),
        "database-migrate stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let output_json: Value =
        serde_json::from_slice(&output.stdout).expect("database-migrate output should be json");
    assert_eq!(output_json["ok"], true);
    assert_eq!(output_json["data"]["ready_to_cutover"], true);

    assert_migrated_fixture(
        &format!("{target_database_url}?mode=rwc"),
        DbBackend::Sqlite,
        file_id,
    )
    .await;
}

#[tokio::test]
async fn test_root_binary_database_migrate_allows_consumed_contact_verification_history() {
    let source_db_path = std::env::temp_dir().join(format!(
        "asterdrive-cli-contact-history-source-{}.db",
        uuid::Uuid::new_v4()
    ));
    let source_database_url = format!("sqlite://{}?mode=rwc", source_db_path.display());
    let file_id = seed_migration_fixture(&source_database_url).await;
    seed_contact_verification_history(&source_database_url).await;

    let target_db_path = std::env::temp_dir().join(format!(
        "asterdrive-cli-contact-history-target-{}.db",
        uuid::Uuid::new_v4()
    ));
    let target_database_url = format!("sqlite://{}?mode=rwc", target_db_path.display());

    let output = run_aster_drive(&[
        "database-migrate",
        "--source-database-url",
        &source_database_url,
        "--target-database-url",
        &target_database_url,
    ]);
    assert!(
        output.status.success(),
        "database-migrate stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let output_json: Value =
        serde_json::from_slice(&output.stdout).expect("database-migrate output should be json");
    assert_eq!(output_json["ok"], true);
    assert_eq!(output_json["data"]["ready_to_cutover"], true);
    assert_eq!(
        output_json["data"]["verification"]["unique_conflicts"]
            .as_array()
            .map(Vec::len),
        Some(0)
    );

    let target_db = db::connect_with_metrics(
        &DatabaseConfig {
            url: target_database_url.clone().into(),
            pool_size: 1,
            retry_count: 0,
        },
        aster_drive_metrics::NoopMetrics::arc(),
    )
    .await
    .unwrap();
    let active_password_reset_tokens = scalar_i64(
        &target_db,
        DbBackend::Sqlite,
        "SELECT COUNT(*) FROM contact_verification_tokens \
         WHERE channel = 'email' AND purpose = 'password_reset' AND consumed_at IS NULL",
    )
    .await;
    let historical_contact_change_tokens = scalar_i64(
        &target_db,
        DbBackend::Sqlite,
        "SELECT COUNT(*) FROM contact_verification_tokens \
         WHERE channel = 'email' AND purpose = 'contact_change' AND consumed_at IS NOT NULL",
    )
    .await;
    assert_eq!(active_password_reset_tokens, 1);
    assert_eq!(historical_contact_change_tokens, 2);

    assert_migrated_fixture(&target_database_url, DbBackend::Sqlite, file_id).await;
}

#[tokio::test]
async fn test_root_binary_database_migrate_human_output_is_readable() {
    let source_db_path = std::env::temp_dir().join(format!(
        "asterdrive-cli-human-source-{}.db",
        uuid::Uuid::new_v4()
    ));
    let source_database_url = format!("sqlite://{}?mode=rwc", source_db_path.display());
    let file_id = seed_migration_fixture(&source_database_url).await;

    let target_db_path = std::env::temp_dir().join(format!(
        "asterdrive-cli-human-target-{}.db",
        uuid::Uuid::new_v4()
    ));
    let target_database_url = format!("sqlite://{}?mode=rwc", target_db_path.display());

    let output = run_aster_drive_with_env(
        &[
            "database-migrate",
            "--output-format",
            "human",
            "--source-database-url",
            &source_database_url,
            "--target-database-url",
            &target_database_url,
        ],
        &[("ASTER_CLI_PROGRESS", "1")],
    );
    assert!(
        output.status.success(),
        "database-migrate stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("human stdout should be utf-8");
    assert!(stdout.contains("Database migration complete"));
    assert!(stdout.contains("Stages:"));
    assert!(stdout.contains("Source:"));
    assert!(stdout.contains("Target:"));
    assert!(stdout.contains("Cutover:"));
    assert!(stdout.contains("[OK] ready"));
    assert!(stdout.contains("Verification:"));
    assert!(serde_json::from_str::<Value>(&stdout).is_err());

    let stderr = String::from_utf8(output.stderr).expect("human stderr should be utf-8");
    assert!(stderr.contains("[database-migrate] data_copy: ["));

    assert_migrated_fixture(&target_database_url, DbBackend::Sqlite, file_id).await;
}

#[tokio::test]
async fn test_root_binary_database_migrate_human_output_supports_forced_color() {
    let source_db_path = std::env::temp_dir().join(format!(
        "asterdrive-cli-human-color-source-{}.db",
        uuid::Uuid::new_v4()
    ));
    let source_database_url = format!("sqlite://{}?mode=rwc", source_db_path.display());
    let _ = seed_migration_fixture(&source_database_url).await;

    let target_db_path = std::env::temp_dir().join(format!(
        "asterdrive-cli-human-color-target-{}.db",
        uuid::Uuid::new_v4()
    ));
    let target_database_url = format!("sqlite://{}?mode=rwc", target_db_path.display());

    let output = run_aster_drive_with_env(
        &[
            "database-migrate",
            "--output-format",
            "human",
            "--source-database-url",
            &source_database_url,
            "--target-database-url",
            &target_database_url,
        ],
        &[("ASTER_CLI_PROGRESS", "1"), ("CLICOLOR_FORCE", "1")],
    );
    assert!(
        output.status.success(),
        "database-migrate stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("forced-color stdout should be utf-8");
    let stderr = String::from_utf8(output.stderr).expect("forced-color stderr should be utf-8");
    assert!(stdout.contains("\u{1b}["));
    assert!(stderr.contains("\u{1b}["));
}

/// Pipe input must remain offline, bounded, and compatible with line endings.
#[test]
fn hash_password_reads_stdin_offline_and_rejects_invalid_input_without_echoing_it() {
    use std::io::Write;
    use std::process::Stdio;
    let dir = aster_forge_utils::raii::TempDirGuard::new(
        std::env::temp_dir().join(format!("aster-hash-cli-{}", uuid::Uuid::new_v4())),
        "hash CLI test",
    );
    std::fs::create_dir_all(dir.path()).unwrap();
    // An invalid server configuration proves this command never starts server
    // resources or logging, and never generates or repairs configuration files.
    std::fs::create_dir(dir.path().join("data")).unwrap();
    let invalid_config = "not valid TOML";
    std::fs::write(dir.path().join("data/config.toml"), invalid_config).unwrap();
    let run = |input: &[u8]| {
        let mut child = Command::new(aster_drive_bin())
            .arg("hash-password")
            .current_dir(dir.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(input).unwrap();
        child.wait_with_output().unwrap()
    };
    for (password, ending) in [
        ("scrape:secret".to_owned(), "\r\n"),
        ("x".repeat(4096), "\r\n"),
        ("  scrape:secret  ".to_owned(), ""),
        ("scrape:secret".to_owned(), "\n"),
        ("scrape:secret".to_owned(), "\r"),
    ] {
        let output = run(format!("{password}{ending}").as_bytes());
        assert!(
            output.status.success(),
            "hash command should succeed without exposing its input"
        );
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert_eq!(stdout.lines().count(), 1);
        assert!(stdout.starts_with("$argon2id$"));
        assert!(aster_forge_crypto::verify_password(&password, stdout.trim()).unwrap());
        assert!(output.stderr.is_empty());
    }
    for password in [
        Vec::new(),
        b"\n".to_vec(),
        b"credential-marker\nsecond-line".to_vec(),
        b"credential-marker\t".to_vec(),
        b"credential-marker\0".to_vec(),
        vec![b'x'; 4097],
        vec![0xff],
    ] {
        let output = run(&password);
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(!String::from_utf8_lossy(&output.stderr).contains("credential-marker"));
    }
    assert_eq!(
        std::fs::read_to_string(dir.path().join("data/config.toml")).unwrap(),
        invalid_config
    );
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}

/// Real PTYs prove hidden editing, mode restoration, and safe detached failures.
#[cfg(unix)]
#[test]
fn hash_password_hidden_terminal_input_is_strict_and_keeps_stdout_clean() {
    use std::io::Write;
    use std::process::Stdio;
    let root = aster_forge_utils::raii::TempDirGuard::new(
        std::env::temp_dir().join(format!("aster-hash-terminal-{}", uuid::Uuid::new_v4())),
        "hash controlling terminal fixture",
    );
    std::fs::create_dir_all(root.path().join("data")).unwrap();
    let invalid_config = "not valid TOML";
    std::fs::write(root.path().join("data/config.toml"), invalid_config).unwrap();
    let cases = [
        (
            "pty-hidden-credential-marker\r".to_owned(),
            Some("pty-hidden-credential-marker".to_owned()),
        ),
        ("  猫😀  \r".to_owned(), Some("  猫😀  ".to_owned())),
        ("猫😀\x7f\x08abc\r".to_owned(), Some("abc".to_owned())),
        (format!("{}\r", "x".repeat(4096)), Some("x".repeat(4096))),
        ("\r".to_owned(), None),
        ("pty-hidden-credential-marker\t\r".to_owned(), None),
        ("pty-hidden-credential-marker\0\r".to_owned(), None),
        (
            format!("{}pty-hidden-credential-marker\r", "x".repeat(4097)),
            None,
        ),
        ("\x03".to_owned(), None),
        ("\x04".to_owned(), None),
        ("\x1a".to_owned(), None),
    ];
    let inputs: Vec<_> = cases
        .iter()
        .map(
            |(input, expected)| serde_json::json!({"input": input, "succeeds": expected.is_some()}),
        )
        .collect();
    let mut command = Command::new("python3");
    for (key, _) in std::env::vars()
        .filter(|(key, _)| key.starts_with("ASTER__") || key.starts_with("ASTER_CLI_"))
    {
        command.env_remove(key);
    }
    let mut child = command
        .args(["-c", include_str!("cli_hash_input.py"), aster_drive_bin()])
        .arg(root.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Python 3 PTY test support must be available on Unix");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&inputs).unwrap())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "PTY harness failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let hashes: Vec<Option<String>> =
        serde_json::from_slice(&output.stdout).expect("PTY harness must return hash-only results");
    assert_eq!(hashes.len(), cases.len());
    for ((_, expected), hash) in cases.iter().zip(hashes) {
        match (expected, hash) {
            (Some(expected), Some(hash)) => {
                assert!(aster_forge_crypto::verify_password(expected, hash.trim()).unwrap());
            }
            (None, None) => {}
            _ => panic!("PTY result did not match the expected success state"),
        }
    }
    assert_eq!(
        std::fs::read_to_string(root.path().join("data/config.toml")).unwrap(),
        invalid_config
    );
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
}

#[cfg(unix)]
#[tokio::test]
async fn cli_hidden_terminal_input_restores_echo_and_keeps_credentials_out_of_output() {
    let database_url = setup_database_url().await;
    let root = aster_forge_utils::raii::TempDirGuard::new(
        std::env::temp_dir().join(format!("aster-cli-terminal-{}", uuid::Uuid::new_v4())),
        "CLI controlling terminal fixture",
    );
    std::fs::create_dir_all(root.path().join("data")).unwrap();
    std::fs::write(
        root.path().join("data/config.toml"),
        "[server]\nstart_mode = \"primary\"\n",
    )
    .unwrap();
    let output = Command::new("python3")
        .args([
            "-c",
            include_str!("cli_hidden_input.py"),
            aster_drive_bin(),
            &database_url,
        ])
        .arg(root.path())
        .output()
        .expect("Python 3 PTY test support must be available on Unix");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[tokio::test]
async fn cli_secret_stdin_modes_are_bounded_mutually_exclusive_and_redacted() {
    use std::io::Write;
    use std::process::Stdio;
    let database_url = setup_database_url().await;
    let run = |input: &[u8]| {
        let mut child = Command::new(aster_drive_bin())
            .args([
                "config",
                "--database-url",
                &database_url,
                "set",
                "--key",
                "mail_smtp_password",
                "--value-stdin",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let written = child.stdin.take().unwrap().write_all(input);
        if let Err(error) = written {
            assert_eq!(error.kind(), std::io::ErrorKind::BrokenPipe);
        }
        child.wait_with_output().unwrap()
    };
    let output = run(b"stdin-secret-marker\r\n");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("stdin-secret-marker"));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("stdin-secret-marker"));
    let complete = run_aster_drive(&[
        "config",
        "--database-url",
        &database_url,
        "export",
        "--include-sensitive",
    ]);
    let backup: Value = serde_json::from_slice(&complete.stdout).unwrap();
    let stored = backup["configs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["key"] == "mail_smtp_password")
        .unwrap();
    assert_eq!(stored["value"], "stdin-secret-marker");
    for bad_input in [vec![0xff], vec![b'x'; 1024 * 1024 + 1]] {
        let output = run(&bad_input);
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(!String::from_utf8_lossy(&output.stderr).contains("stdin-secret-marker"));
    }
    for args in [
        vec![
            "config",
            "--database-url",
            &database_url,
            "set",
            "--key",
            "mail_smtp_password",
            "--value",
            "conflict-secret-marker",
            "--value-stdin",
        ],
        vec![
            "node",
            "enroll",
            "--master-url",
            "https://example.invalid",
            "--token",
            "conflict-secret-marker",
            "--token-stdin",
        ],
    ] {
        let output = run_aster_drive(&args);
        assert!(!output.status.success());
        assert!(!String::from_utf8_lossy(&output.stdout).contains("conflict-secret-marker"));
        assert!(!String::from_utf8_lossy(&output.stderr).contains("conflict-secret-marker"));
    }
    for (args, variable) in [
        (vec!["config", "set", "--help"], "ASTER_CLI_CONFIG_VALUE"),
        (
            vec!["node", "enroll", "--help"],
            "ASTER_CLI_ENROLLMENT_TOKEN",
        ),
    ] {
        let output = run_aster_drive_with_env(&args, &[(variable, "environment-secret-marker")]);
        assert!(output.status.success());
        assert!(!String::from_utf8_lossy(&output.stdout).contains("environment-secret-marker"));
        assert!(!String::from_utf8_lossy(&output.stderr).contains("environment-secret-marker"));
    }
}

#[tokio::test]
async fn cli_config_location_resolves_relative_database_and_service_environment_from_other_cwd() {
    let root = aster_forge_utils::raii::TempDirGuard::new(
        std::env::temp_dir().join(format!("aster-cli-paths-{}", uuid::Uuid::new_v4())),
        "CLI relative paths fixture",
    );
    let data = root.path().join("custom-data");
    let cwd = root.path().join("elsewhere");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::create_dir_all(&cwd).unwrap();
    let seeded = setup_database_url().await;
    let seeded_path = seeded
        .strip_prefix("sqlite://")
        .unwrap()
        .split('?')
        .next()
        .unwrap();
    std::fs::copy(seeded_path, data.join("instance.db")).unwrap();
    let config_path = data.join("config.toml");
    let config_content = "[database]\nurl = \"sqlite://instance.db?mode=rwc\"\n";
    std::fs::write(&config_path, config_content).unwrap();
    for args in [
        vec![
            "--config-file",
            config_path.to_str().unwrap(),
            "config",
            "get",
            "--key",
            "auth_cookie_secure",
        ],
        vec![
            "config",
            "--data-dir",
            data.to_str().unwrap(),
            "get",
            "--key",
            "auth_cookie_secure",
        ],
    ] {
        let output = run_aster_drive_with_env_in_dir(&args, &[], &cwd);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!cwd.join("data").exists());
        assert_eq!(
            std::fs::read_to_string(&config_path).unwrap(),
            config_content
        );
    }
    let env = run_aster_drive_with_env_in_dir(
        &["config", "get", "--key", "auth_cookie_secure"],
        &[
            ("ASTER_CLI_CONFIG_FILE", config_path.to_str().unwrap()),
            ("ASTER__DATABASE__URL", &seeded),
        ],
        &cwd,
    );
    assert!(
        env.status.success(),
        "{}",
        String::from_utf8_lossy(&env.stderr)
    );
    let doctor = run_aster_drive_with_env_in_dir(
        &["--data-dir", data.to_str().unwrap(), "doctor"],
        &[],
        &cwd,
    );
    let report: Value = serde_json::from_slice(&doctor.stdout).unwrap();
    assert!(
        report["data"]["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|check| check["name"] == "database_connection" && check["status"] == "ok")
    );
    let missing_database = data.join("missing.db");
    std::fs::write(
        &config_path,
        "[database]\nurl = \"sqlite://missing.db?mode=rwc\"\n",
    )
    .unwrap();
    let output = run_aster_drive_with_env_in_dir(
        &["--data-dir", data.to_str().unwrap(), "config", "list"],
        &[],
        &cwd,
    );
    assert!(!output.status.success());
    assert!(!missing_database.exists());
}

#[test]
fn cli_help_banner_includes_version_at_every_command_depth_without_startup() {
    let root = aster_forge_utils::raii::TempDirGuard::new(
        std::env::temp_dir().join(format!("aster-cli-help-banner-{}", uuid::Uuid::new_v4())),
        "CLI help fixture",
    );
    std::fs::create_dir_all(root.path()).unwrap();
    for args in [
        vec!["-h"],
        vec!["--help"],
        vec!["serve", "--help"],
        vec!["doctor", "--help"],
        vec!["config", "--help"],
        vec!["config", "export", "--help"],
        vec!["node", "enroll", "--help"],
        vec!["database-migrate", "--help"],
    ] {
        let output = run_aster_drive_with_env_in_dir(&args, &[], root.path());
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let help = String::from_utf8(output.stdout).unwrap();
        assert_eq!(
            help.lines().next(),
            Some(concat!("AsterDrive ", env!("CARGO_PKG_VERSION")))
        );
        assert!(!help.contains("system_config"));
        assert!(!help.contains("[env:"));
        assert!(!root.path().join("data").exists());
    }
    let output = run_aster_drive_with_env_in_dir(&["--version"], &[], root.path());
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains(env!("CARGO_PKG_VERSION")));
}

#[tokio::test]
async fn offline_config_and_doctor_ignore_uncompiled_exporters() {
    let database_url = setup_database_url().await;
    let root = aster_forge_utils::raii::TempDirGuard::new(
        std::env::temp_dir().join(format!(
            "aster-cli-exporter-boundary-{}",
            uuid::Uuid::new_v4()
        )),
        "offline exporter validation boundary",
    );
    std::fs::create_dir_all(root.path()).unwrap();
    let path = root.path().join("config.toml");
    for section in ["prometheus", "otlp_metrics"] {
        let content = format!(
            "[database]\nurl = \"{database_url}\"\n[observability.{section}]\nenabled = true\n"
        );
        std::fs::write(&path, &content).unwrap();
        for action in [
            vec!["list"],
            vec!["get", "--key", "auth_cookie_secure"],
            vec!["export"],
            vec!["validate", "--key", "custom.preview", "--value", "preview"],
            vec!["set", "--key", "custom.offline", "--value", "updated"],
        ] {
            let mut args = vec!["config", "--config-file", path.to_str().unwrap()];
            args.extend(action);
            let output = run_aster_drive_with_env_in_dir(&args, &[], root.path());
            assert!(
                output.status.success(),
                "offline {section} config: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(output.stderr.is_empty());
        }
        assert_eq!(std::fs::read_to_string(&path).unwrap(), content);
        assert!(!root.path().join("data").exists());
        let output = run_aster_drive_with_env_in_dir(
            &["doctor", "--config-file", path.to_str().unwrap()],
            &[],
            root.path(),
        );
        let report: Value = serde_json::from_slice(&output.stdout)
            .expect("doctor must produce its diagnostic report");
        assert!(
            report["data"]["checks"]
                .as_array()
                .unwrap()
                .iter()
                .any(|check| check["name"] == "static_config" && check["status"] == "ok")
        );
        assert!(
            report["data"]["checks"]
                .as_array()
                .unwrap()
                .iter()
                .any(|check| check["name"] == "database_connection" && check["status"] == "ok")
        );
    }
}

#[cfg(all(unix, any(not(feature = "metrics"), not(feature = "otlp-metrics"))))]
#[tokio::test]
async fn server_starts_and_keeps_metrics_route_absent_for_uncompiled_exporters() {
    use aster_forge_test::process::{TestProcess, available_loopback_port};
    use std::time::Duration;
    for mode in ["primary", "follower"] {
        let port = available_loopback_port();
        let mut command = Command::new(aster_drive_bin());
        for (key, _) in std::env::vars_os() {
            if key.to_string_lossy().starts_with("ASTER_") {
                command.env_remove(key);
            }
        }
        command
            .env("ASTER__SERVER__START_MODE", mode)
            .env("ASTER__SERVER__HOST", "127.0.0.1")
            .env("ASTER__SERVER__PORT", port.to_string())
            .env("ASTER__SERVER__WORKERS", "1")
            .env("ASTER__DATABASE__URL", "sqlite://asterdrive.db?mode=rwc")
            .env("ASTER__DATABASE__POOL_SIZE", "2")
            .env("ASTER__AUTH__BOOTSTRAP_INSECURE_COOKIES", "true");
        if !cfg!(feature = "metrics") {
            command
                .env("ASTER__OBSERVABILITY__PROMETHEUS__ENABLED", "true")
                .env(
                    "ASTER__OBSERVABILITY__PROMETHEUS__BASIC_AUTH__ENABLED",
                    "true",
                )
                .env(
                    "ASTER__OBSERVABILITY__PROMETHEUS__BASIC_AUTH__USERNAME",
                    "invalid:username",
                )
                .env(
                    "ASTER__OBSERVABILITY__PROMETHEUS__BASIC_AUTH__PASSWORD_HASH",
                    "invalid-secret-marker",
                );
        }
        if !cfg!(feature = "otlp-metrics") {
            command
                .env("ASTER__OBSERVABILITY__OTLP_METRICS__ENABLED", "true")
                .env(
                    "ASTER__OBSERVABILITY__OTLP_METRICS__ENDPOINT",
                    "invalid://invalid-secret-marker",
                )
                .env("ASTER__OBSERVABILITY__OTLP_METRICS__INTERVAL_SECONDS", "0");
        }
        let mut process = TestProcess::spawn(&format!("ignored-exporter-{mode}"), &mut command);
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap();
        tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                process.assert_running();
                if client
                    .get(format!("http://127.0.0.1:{port}/health"))
                    .send()
                    .await
                    .is_ok_and(|response| response.status().is_success())
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .unwrap_or_else(|error| panic!("server did not start: {error}\n{}", process.diagnostics()));
        let response = client
            .get(format!("http://127.0.0.1:{port}/health/metrics"))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::NOT_FOUND);
        assert!(
            process.terminate_gracefully(Duration::from_secs(20)),
            "{}",
            process.diagnostics()
        );
        assert!(!process.diagnostics().contains("invalid-secret-marker"));
    }
}

#[test]
fn version_output_is_structured_and_does_not_load_instance_configuration() {
    let root = aster_forge_utils::raii::TempDirGuard::new(
        std::env::temp_dir().join(format!("aster-cli-version-{}", uuid::Uuid::new_v4())),
        "CLI version fixture",
    );
    std::fs::create_dir_all(root.path().join("data")).unwrap();
    let path = root.path().join("data/config.toml");
    std::fs::write(&path, "invalid TOML credential-marker").unwrap();
    let json_output = run_aster_drive_with_env_in_dir(&["version", "--json"], &[], root.path());
    assert!(
        json_output.status.success(),
        "{}",
        String::from_utf8_lossy(&json_output.stderr)
    );
    assert!(json_output.stderr.is_empty());
    let identity: Value = serde_json::from_slice(&json_output.stdout).unwrap();
    assert_eq!(identity["product"], "AsterDrive");
    assert_eq!(identity["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(identity["revision"], env!("ASTER_BUILD_REVISION"));
    assert_eq!(identity["target"], env!("ASTER_BUILD_TARGET"));
    assert_eq!(identity["profile"], env!("ASTER_BUILD_PROFILE"));
    assert_eq!(identity["build_time"], env!("ASTER_BUILD_TIME"));
    assert!(identity["dirty"].is_boolean() || identity["dirty"].is_null());
    for (feature, enabled) in [
        ("metrics", cfg!(feature = "metrics")),
        ("otlp-metrics", cfg!(feature = "otlp-metrics")),
        ("operations-cli", cfg!(feature = "operations-cli")),
        ("server", cfg!(feature = "server")),
        ("openapi", cfg!(feature = "openapi")),
    ] {
        assert_eq!(
            identity["features"]
                .as_array()
                .unwrap()
                .iter()
                .any(|value| value == feature),
            enabled
        );
    }
    let human = run_aster_drive_with_env_in_dir(&["version"], &[], root.path());
    assert!(human.status.success());
    let human = String::from_utf8(human.stdout).unwrap();
    for label in ["Revision:", "Target:", "Profile:", "Features:", "Built:"] {
        assert!(human.contains(label));
    }
    for args in [vec!["-V"], vec!["--version"]] {
        let output = run_aster_drive_with_env_in_dir(&args, &[], root.path());
        assert!(output.status.success());
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            concat!("AsterDrive ", env!("CARGO_PKG_VERSION"), "\n")
        );
        assert!(output.stderr.is_empty());
    }
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "invalid TOML credential-marker"
    );
    assert_eq!(
        std::fs::read_dir(root.path().join("data")).unwrap().count(),
        1
    );
}

#[test]
fn root_help_is_compact_hides_environment_values_and_orders_common_commands_first() {
    for args in [vec!["-h"], vec!["--help"]] {
        let output = run_aster_drive_with_env(
            &args,
            &[
                ("ASTER_CLI_CONFIG_FILE", "/secret-path-marker"),
                ("ASTER_CLI_CONFIG_VALUE", "credential-marker"),
            ],
        );
        assert!(output.status.success());
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(text.starts_with(concat!(
            "AsterDrive ",
            env!("CARGO_PKG_VERSION"),
            "\n",
            env!("CARGO_PKG_DESCRIPTION"),
            "\n"
        )));
        assert!(
            !text.contains("[env:")
                && !text.contains("secret-path-marker")
                && !text.contains("credential-marker")
        );
        assert!(!text.contains("\n\n\n"));
        assert!(
            text.lines().count() <= 32,
            "root help is not compact: {text}"
        );
        let commands = [
            "  serve ",
            "  config ",
            "  doctor ",
            "  node ",
            "  database-migrate ",
            "  hash-password ",
            "  version ",
        ];
        let positions: Vec<usize> = commands
            .iter()
            .map(|command| text.find(command).unwrap())
            .collect();
        assert!(positions.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(text.contains("--config-file <PATH>") && text.contains("--data-dir <PATH>"));
        assert!(
            text.contains("Examples:")
                && text.contains("Running without a command starts all server roles.")
        );
    }
}
