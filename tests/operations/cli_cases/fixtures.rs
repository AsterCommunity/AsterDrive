//! Product migration fixture construction and assertions.
use super::*;

pub(super) async fn seed_migration_fixture(database_url: &str) -> i64 {
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

pub(super) async fn seed_contact_verification_history(database_url: &str) {
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

pub(super) async fn assert_migrated_fixture(
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
