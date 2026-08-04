//! Integration coverage for the AsterDrive 0.5.0-only startup credential migration.
//!
//! Remove this test with the deprecated source stores in AsterDrive 0.6.0.

#![expect(
    deprecated,
    reason = "AsterDrive 0.5.0 integration coverage reads deprecated credential stores until 0.6.0"
)]

use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, Generate, KeyInit},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use hkdf::Hkdf;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, Database, DatabaseConnection, EntityTrait,
    QueryFilter, Set,
    sea_query::ExprTrait,
    sea_query::{Alias, Expr, Query},
};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use aster_drive::config::{Config, node_mode::NodeRuntimeMode};
use aster_drive::runtime::startup::initialize_database_state;
use aster_drive_migration::Migrator;
use aster_drive_model::deprecated::{
    storage_connector_application_config, storage_policy_credential,
};
use aster_drive_model::entities::storage_policy_connector_credential;
use aster_drive_model::types::{
    MicrosoftGraphCloud, StorageCredentialKind, StorageCredentialProvider, StorageCredentialStatus,
};
use aster_drive_storage::{
    ConnectorConfigEnvelope, ConnectorId, StoragePolicyBehaviorConfig, encode_storage_policy_config,
};

const KEY: &str = "legacy-storage-credential-test-key-32bytes";
const OTHER_KEY: &str = "different-storage-credential-key-32bytes";
const STORAGE_CREDENTIAL_INFO: &[u8] = b"asterdrive:storage-credential-token:v1";

#[derive(Serialize)]
struct EmptyConnectorConfig {}

#[derive(Serialize)]
struct OneDriveConfig {
    base_path: String,
    provider_resumable_upload_strategy: &'static str,
    provider_download_strategy: &'static str,
    provider_download_filename_mode: &'static str,
    cloud: MicrosoftGraphCloud,
    account_mode: &'static str,
    tenant: Option<String>,
    drive_id: Option<String>,
    root_item_id: Option<String>,
    site_id: Option<String>,
    group_id: Option<String>,
}

#[derive(Serialize)]
struct OneDriveMetadata<'a> {
    cloud: MicrosoftGraphCloud,
    drive_id: &'a str,
    root_item_id: &'a str,
    root_item_name: &'a str,
    id_token: &'a str,
}

#[derive(Deserialize, Serialize)]
struct ConnectorCiphertextEnvelope {
    format_version: u32,
    connector_id: String,
    schema_version: u32,
    ciphertext: String,
}

fn test_config(key: &str) -> Config {
    let mut config = Config::default();
    config.auth.storage_credential_secret_key = key.to_string();
    config
}

async fn database() -> DatabaseConnection {
    let db = Database::connect("sqlite::memory:")
        .await
        .expect("credential migration integration database should connect");
    Migrator::up(&db, None)
        .await
        .expect("credential migration integration schema should migrate");
    db
}

fn onedrive_config() -> OneDriveConfig {
    OneDriveConfig {
        base_path: String::new(),
        provider_resumable_upload_strategy: "server_relay",
        provider_download_strategy: "server_relay",
        provider_download_filename_mode: "provider_native",
        cloud: MicrosoftGraphCloud::Global,
        account_mode: "personal",
        tenant: Some("common".to_string()),
        drive_id: Some("drive-id".to_string()),
        root_item_id: Some("root-item-id".to_string()),
        site_id: None,
        group_id: None,
    }
}

async fn insert_policy<T: Serialize>(
    db: &DatabaseConnection,
    id: i64,
    connector_id: &str,
    connector_config: T,
) {
    let connector_config = serde_json::to_value(connector_config)
        .expect("integration connector config should serialize");
    let storage_config = encode_storage_policy_config(
        ConnectorConfigEnvelope::new(ConnectorId::declared(connector_id), 1, connector_config),
        StoragePolicyBehaviorConfig::default(),
    )
    .expect("integration storage policy config should encode");
    // The 0.5.0 fixture intentionally writes the historical columns that are
    // still present in an existing database. Production models no longer
    // expose these columns; they are removed with the deprecated stores in
    // 0.6.0.
    let driver_type = connector_id
        .rsplit('.')
        .next()
        .expect("connector id should contain a driver suffix");
    let now = chrono::Utc::now();
    let statement = Query::insert()
        .into_table(Alias::new("storage_policies"))
        .columns([
            Alias::new("id"),
            Alias::new("name"),
            Alias::new("driver_type"),
            Alias::new("endpoint"),
            Alias::new("bucket"),
            Alias::new("access_key"),
            Alias::new("secret_key"),
            Alias::new("base_path"),
            Alias::new("remote_node_id"),
            Alias::new("remote_storage_target_key"),
            Alias::new("max_file_size"),
            Alias::new("allowed_types"),
            Alias::new("options"),
            Alias::new("is_default"),
            Alias::new("chunk_size"),
            Alias::new("created_at"),
            Alias::new("updated_at"),
            Alias::new("connector_id"),
            Alias::new("storage_config"),
        ])
        .values([
            Expr::value(id),
            Expr::value(format!("policy-{id}")),
            Expr::value(driver_type),
            Expr::value(""),
            Expr::value(""),
            Expr::value(""),
            Expr::value(""),
            Expr::value(""),
            Expr::value(Option::<i64>::None),
            Expr::value(Option::<String>::None),
            Expr::value(0_i64),
            Expr::value("[]"),
            Expr::value("{}"),
            Expr::value(false),
            Expr::value(0_i64),
            Expr::value(now),
            Expr::value(now),
            Expr::value(connector_id),
            Expr::value(storage_config),
        ])
        .expect("integration storage policy insert values should be valid")
        .to_owned();
    db.execute(&statement)
        .await
        .expect("integration storage policy should insert");
}

async fn set_static(db: &DatabaseConnection, policy_id: i64, access_key: &str, secret_key: &str) {
    let statement = Query::update()
        .table(Alias::new("storage_policies"))
        .values([
            (Alias::new("access_key"), Expr::value(access_key)),
            (Alias::new("secret_key"), Expr::value(secret_key)),
        ])
        .and_where(Expr::col(Alias::new("id")).eq(policy_id))
        .to_owned();
    db.execute(&statement)
        .await
        .expect("integration legacy static credential should update");
}

fn cipher(master_key: &str) -> Aes256Gcm {
    let hk = Hkdf::<Sha256>::new(None, master_key.trim().as_bytes());
    let mut key = [0_u8; 32];
    hk.expand(STORAGE_CREDENTIAL_INFO, &mut key)
        .expect("integration storage credential key should derive");
    Aes256Gcm::new_from_slice(&key).expect("integration AES key should be valid")
}

fn encrypt_token(master_key: &str, aad: &[u8], plaintext: &str) -> String {
    let nonce = Nonce::generate();
    let ciphertext = cipher(master_key)
        .encrypt(
            &nonce,
            aes_gcm::aead::Payload {
                msg: plaintext.as_bytes(),
                aad,
            },
        )
        .expect("integration legacy token should encrypt");
    format!(
        "v1:{}:{}",
        URL_SAFE_NO_PAD.encode(nonce),
        URL_SAFE_NO_PAD.encode(ciphertext)
    )
}

fn decrypt_token(master_key: &str, aad: &[u8], ciphertext: &str) -> String {
    let mut parts = ciphertext.split(':');
    assert_eq!(parts.next(), Some("v1"));
    let nonce = URL_SAFE_NO_PAD
        .decode(parts.next().expect("ciphertext should contain nonce"))
        .expect("ciphertext nonce should decode");
    let encrypted = URL_SAFE_NO_PAD
        .decode(parts.next().expect("ciphertext should contain payload"))
        .expect("ciphertext payload should decode");
    assert!(parts.next().is_none());
    let nonce = Nonce::try_from(nonce.as_slice()).expect("ciphertext nonce should be 12 bytes");
    String::from_utf8(
        cipher(master_key)
            .decrypt(
                &nonce,
                aes_gcm::aead::Payload {
                    msg: &encrypted,
                    aad,
                },
            )
            .expect("connector credential should decrypt"),
    )
    .expect("connector credential should be UTF-8")
}

fn token_aad(policy_id: i64, token_name: &str) -> String {
    format!("storage_policy_credential:{policy_id}:microsoft_graph:{token_name}")
}

fn encrypt_connector_payload(
    key: &str,
    policy_id: i64,
    connector_id: &str,
    payload: &serde_json::Value,
) -> String {
    let aad = format!("storage_policy_connector_credential:{policy_id}:{connector_id}:1");
    let ciphertext = encrypt_token(
        key,
        aad.as_bytes(),
        &serde_json::to_string(payload).expect("current connector payload should serialize"),
    );
    serde_json::to_string(&ConnectorCiphertextEnvelope {
        format_version: 1,
        connector_id: connector_id.to_string(),
        schema_version: 1,
        ciphertext,
    })
    .expect("current connector ciphertext envelope should serialize")
}

async fn insert_current_connector_credential(
    db: &DatabaseConnection,
    key: &str,
    policy_id: i64,
    connector_id: &str,
    payload: serde_json::Value,
) {
    let ciphertext = encrypt_connector_payload(key, policy_id, connector_id, &payload);
    aster_drive::db::repository::storage_policy_connector_credential_repo::upsert(
        db,
        policy_id,
        connector_id.to_string(),
        1,
        ciphertext,
    )
    .await
    .expect("current connector credential should insert");
}

fn application_secret_aad(policy_id: i64) -> String {
    format!("storage_connector_application_config:{policy_id}:microsoft_graph:client_secret")
}

async fn insert_onedrive_application(
    db: &DatabaseConnection,
    policy_id: i64,
    key: &str,
    ciphertext: Option<String>,
) {
    let now = chrono::Utc::now();
    storage_connector_application_config::ActiveModel {
        policy_id: Set(policy_id),
        provider: Set(StorageCredentialProvider::MicrosoftGraph),
        tenant_id: Set(Some(" common ".to_string())),
        scopes: Set(serde_json::to_string(&vec!["offline_access", "Files.ReadWrite"]).unwrap()),
        client_id: Set(Some(" client-id ".to_string())),
        client_secret_ciphertext: Set(Some(ciphertext.unwrap_or_else(|| {
            encrypt_token(
                key,
                application_secret_aad(policy_id).as_bytes(),
                "client-secret",
            )
        }))),
        metadata: Set(
            serde_json::to_string(&serde_json::Map::<String, serde_json::Value>::new()).unwrap(),
        ),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("integration legacy application should insert");
}

async fn insert_onedrive_authorization(db: &DatabaseConnection, policy_id: i64, key: &str) {
    let now = chrono::Utc::now();
    storage_policy_credential::ActiveModel {
        policy_id: Set(policy_id),
        provider: Set(StorageCredentialProvider::MicrosoftGraph),
        credential_kind: Set(StorageCredentialKind::OauthDelegated),
        account_label: Set(Some(" Documents ".to_string())),
        subject: Set(Some(" subject-id ".to_string())),
        tenant_id: Set(Some(" common ".to_string())),
        scopes: Set(serde_json::to_string(&vec!["offline_access", "Files.ReadWrite"]).unwrap()),
        access_token_ciphertext: Set(Some(encrypt_token(
            key,
            token_aad(policy_id, "access").as_bytes(),
            "access-token",
        ))),
        refresh_token_ciphertext: Set(Some(encrypt_token(
            key,
            token_aad(policy_id, "refresh").as_bytes(),
            "refresh-token",
        ))),
        metadata: Set(serde_json::to_string(&OneDriveMetadata {
            cloud: MicrosoftGraphCloud::Global,
            drive_id: "drive-id",
            root_item_id: "root-item-id",
            root_item_name: "Documents",
            id_token: "***REDACTED***",
        })
        .unwrap()),
        status: Set(StorageCredentialStatus::Authorized),
        status_reason: Set(None),
        expires_at: Set(Some(now + chrono::Duration::hours(1))),
        authorized_at: Set(Some(now)),
        last_refreshed_at: Set(None),
        last_validated_at: Set(Some(now)),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("integration legacy authorization should insert");
}

async fn connector_payload(
    db: &DatabaseConnection,
    key: &str,
    policy_id: i64,
    connector_id: &str,
) -> serde_json::Value {
    let record = storage_policy_connector_credential::Entity::find()
        .filter(storage_policy_connector_credential::Column::PolicyId.eq(policy_id))
        .one(db)
        .await
        .expect("connector credential query should succeed")
        .expect("connector credential should exist");
    let envelope: ConnectorCiphertextEnvelope = serde_json::from_str(&record.ciphertext)
        .expect("connector ciphertext envelope should parse");
    assert_eq!(envelope.format_version, 1);
    assert_eq!(envelope.connector_id, connector_id);
    assert_eq!(envelope.schema_version, 1);
    let aad = format!("storage_policy_connector_credential:{policy_id}:{connector_id}:1");
    serde_json::from_str(&decrypt_token(key, aad.as_bytes(), &envelope.ciphertext))
        .expect("connector payload should be JSON")
}

async fn legacy_static_rows(db: &DatabaseConnection) -> Vec<(i64, String, String)> {
    let statement = Query::select()
        .columns([
            Alias::new("id"),
            Alias::new("access_key"),
            Alias::new("secret_key"),
        ])
        .from(Alias::new("storage_policies"))
        .order_by(Alias::new("id"), sea_orm::sea_query::Order::Asc)
        .to_owned();
    db.query_all(&statement)
        .await
        .expect("legacy static rows should query")
        .into_iter()
        .map(|row| {
            (
                row.try_get_by_index(0).unwrap(),
                row.try_get_by_index(1).unwrap(),
                row.try_get_by_index(2).unwrap(),
            )
        })
        .collect()
}

async fn assert_legacy_static_columns_removed(db: &DatabaseConnection) {
    let manager = aster_drive_migration::SchemaManager::new(db);
    assert!(
        !manager
            .has_column("storage_policies", "access_key")
            .await
            .unwrap()
    );
    assert!(
        !manager
            .has_column("storage_policies", "secret_key")
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn startup_migrates_all_static_connectors_with_typed_field_names() {
    let config = test_config(KEY);
    let db = database().await;
    let cases = [
        (
            1,
            "asterdrive.storage.s3",
            "s3_access_key_id",
            "s3_secret_access_key",
        ),
        (
            2,
            "asterdrive.storage.sftp",
            "sftp_username",
            "sftp_password",
        ),
        (
            3,
            "asterdrive.storage.azure_blob",
            "azure_blob_account_name",
            "azure_blob_account_key",
        ),
        (
            4,
            "asterdrive.storage.tencent_cos",
            "tencent_cos_secret_id",
            "tencent_cos_secret_key",
        ),
    ];
    for (policy_id, connector_id, _, _) in cases {
        insert_policy(&db, policy_id, connector_id, EmptyConnectorConfig {}).await;
        set_static(&db, policy_id, " legacy-id ", " legacy-secret ").await;
    }

    initialize_database_state(&db, &config, NodeRuntimeMode::Primary)
        .await
        .unwrap();

    for (policy_id, connector_id, id_field, secret_field) in cases {
        let payload = connector_payload(&db, KEY, policy_id, connector_id).await;
        assert_eq!(payload[id_field], "legacy-id");
        assert_eq!(payload[secret_field], "legacy-secret");
        assert!(payload.get("access_key").is_none());
        assert!(payload.get("secret_key").is_none());
    }
    assert_legacy_static_columns_removed(&db).await;
}

#[tokio::test]
async fn startup_merges_onedrive_application_and_oauth_then_cleans_old_tables() {
    let config = test_config(KEY);
    let db = database().await;
    insert_policy(&db, 1, "asterdrive.storage.onedrive", onedrive_config()).await;
    insert_onedrive_application(&db, 1, KEY, None).await;
    insert_onedrive_authorization(&db, 1, KEY).await;

    initialize_database_state(&db, &config, NodeRuntimeMode::Primary)
        .await
        .unwrap();

    let payload = connector_payload(&db, KEY, 1, "asterdrive.storage.onedrive").await;
    assert_eq!(payload["application"]["client_id"], "client-id");
    assert_eq!(payload["application"]["client_secret"], "client-secret");
    assert_eq!(payload["authorization"]["access_token"], "access-token");
    assert_eq!(payload["authorization"]["refresh_token"], "refresh-token");
    assert_eq!(payload["authorization"]["metadata"]["drive_id"], "drive-id");
    assert_eq!(
        payload["authorization"]["metadata"]["root_item_id"],
        "root-item-id"
    );
    assert_eq!(
        payload["authorization"]["metadata"]["id_token_present"],
        true
    );
    assert!(
        storage_connector_application_config::Entity::find()
            .all(&db)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        storage_policy_credential::Entity::find()
            .all(&db)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn startup_rejects_wrong_key_and_rolls_back_prior_static_import() {
    let config = test_config(OTHER_KEY);
    let db = database().await;
    insert_policy(&db, 1, "asterdrive.storage.s3", EmptyConnectorConfig {}).await;
    set_static(&db, 1, "good-id", "good-secret").await;
    insert_policy(&db, 2, "asterdrive.storage.onedrive", onedrive_config()).await;
    insert_onedrive_application(&db, 2, KEY, None).await;

    let error = initialize_database_state(&db, &config, NodeRuntimeMode::Primary)
        .await
        .err()
        .expect("wrong encryption key should abort startup migration");
    assert!(
        error.to_string().contains("decrypt") || error.to_string().contains("ciphertext"),
        "{error}"
    );
    assert!(
        storage_policy_connector_credential::Entity::find()
            .all(&db)
            .await
            .unwrap()
            .is_empty()
    );
    let rows = legacy_static_rows(&db).await;
    assert_eq!(rows[0].1, "good-id");
    assert_eq!(rows[0].2, "good-secret");
    assert_eq!(
        storage_connector_application_config::Entity::find()
            .all(&db)
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn startup_is_idempotent_for_matching_target_and_rejects_conflicts() {
    let config = test_config(KEY);
    let db = database().await;
    insert_policy(&db, 1, "asterdrive.storage.s3", EmptyConnectorConfig {}).await;
    set_static(&db, 1, "id-one", "secret-one").await;
    insert_current_connector_credential(
        &db,
        KEY,
        1,
        "asterdrive.storage.s3",
        serde_json::json!({
            "s3_access_key_id": "id-one",
            "s3_secret_access_key": "secret-one",
        }),
    )
    .await;
    initialize_database_state(&db, &config, NodeRuntimeMode::Primary)
        .await
        .unwrap();

    initialize_database_state(&db, &config, NodeRuntimeMode::Primary)
        .await
        .unwrap();
    let record = storage_policy_connector_credential::Entity::find()
        .filter(storage_policy_connector_credential::Column::PolicyId.eq(1))
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(record.revision, 1);
    assert_legacy_static_columns_removed(&db).await;

    let conflict_db = database().await;
    insert_policy(
        &conflict_db,
        1,
        "asterdrive.storage.s3",
        EmptyConnectorConfig {},
    )
    .await;
    set_static(&conflict_db, 1, "id-two", "secret-two").await;
    insert_current_connector_credential(
        &conflict_db,
        KEY,
        1,
        "asterdrive.storage.s3",
        serde_json::json!({
            "s3_access_key_id": "id-one",
            "s3_secret_access_key": "secret-one",
        }),
    )
    .await;
    let error = initialize_database_state(&conflict_db, &config, NodeRuntimeMode::Primary)
        .await
        .err()
        .expect("conflicting credential should abort startup migration");
    assert!(error.to_string().contains("conflicting legacy"));
    let payload = connector_payload(&conflict_db, KEY, 1, "asterdrive.storage.s3").await;
    assert_eq!(payload["s3_access_key_id"], "id-one");
    let legacy_rows = legacy_static_rows(&conflict_db).await;
    assert_eq!(legacy_rows[0].1, "id-two");
    assert_eq!(legacy_rows[0].2, "secret-two");
}
