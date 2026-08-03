//! AsterDrive 0.5.0-only startup migration from deprecated credential stores.
//!
//! This is intentionally application-level rather than a historical schema
//! migration: connector code owns payload conversion, while the already-loaded
//! runtime config supplies the encryption key. The whole import and legacy
//! cleanup run in one transaction before the server begins listening.
//!
//! This module and both deprecated source tables are scheduled for complete
//! removal in AsterDrive 0.6.0.

#![allow(deprecated)]

use std::collections::BTreeMap;

use sea_orm::{
    ColumnTrait, ConnectionTrait, DbBackend, EntityTrait, QueryFilter, QueryOrder, QuerySelect,
    TransactionTrait,
    sea_query::{Alias, Expr, Query},
};

use crate::config::Config;
use crate::errors::{AsterError, Result};
use crate::storage::connectors::{
    LegacyStorageConnectorCredentialInput, LegacyStoragePolicyStaticCredential,
    StorageConnectorRegistry,
};
use aster_drive_model::deprecated::{
    storage_connector_application_config, storage_policy_credential,
};
use aster_drive_model::entities::{storage_policy, storage_policy_connector_credential};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct LegacyStorageCredentialMigrationReport {
    pub(crate) migrated: usize,
    pub(crate) already_current: usize,
    pub(crate) legacy_rows_deleted: usize,
}

#[derive(Debug)]
struct LegacyStaticCredentialRow {
    policy_id: i64,
    access_key: String,
    secret_key: String,
}

/// AsterDrive 0.5.0-only import after config and schema loading.
///
/// Policy rows are locked on shared SQL backends so concurrent primary startup
/// cannot race the import. A conversion, decryption, conflict, or cleanup error
/// aborts the transaction and therefore aborts startup without partial writes.
/// This entrypoint is scheduled for removal in AsterDrive 0.6.0.
pub(crate) async fn migrate_legacy_storage_credentials(
    db: &sea_orm::DatabaseConnection,
    config: &Config,
    connectors: &StorageConnectorRegistry,
) -> Result<LegacyStorageCredentialMigrationReport> {
    let transaction = db.begin().await.map_err(AsterError::from)?;
    let mut policy_query = storage_policy::Entity::find().order_by_asc(storage_policy::Column::Id);
    if transaction.get_database_backend() != DbBackend::Sqlite {
        policy_query = policy_query.lock_exclusive();
    }
    let policies = policy_query
        .all(&transaction)
        .await
        .map_err(AsterError::from)?;
    let static_rows = load_legacy_static_credentials(&transaction).await?;
    let applications = storage_connector_application_config::Entity::find()
        .order_by_asc(storage_connector_application_config::Column::Id)
        .all(&transaction)
        .await
        .map_err(AsterError::from)?;
    let authorizations = storage_policy_credential::Entity::find()
        .order_by_asc(storage_policy_credential::Column::Id)
        .all(&transaction)
        .await
        .map_err(AsterError::from)?;

    let has_legacy_data = static_rows
        .iter()
        .any(|row| !row.access_key.is_empty() || !row.secret_key.is_empty())
        || !applications.is_empty()
        || !authorizations.is_empty();
    if !has_legacy_data {
        transaction.commit().await.map_err(AsterError::from)?;
        return Ok(LegacyStorageCredentialMigrationReport::default());
    }

    let mut static_by_policy = static_rows
        .into_iter()
        .map(|row| (row.policy_id, row))
        .collect::<BTreeMap<_, _>>();
    let mut applications_by_policy = group_application_rows(applications);
    let mut authorizations_by_policy = group_authorization_rows(authorizations);
    let mut report = LegacyStorageCredentialMigrationReport::default();

    for policy in &policies {
        let static_credential = static_by_policy.remove(&policy.id).and_then(|row| {
            let access_key = row.access_key.trim().to_string();
            let secret_key = row.secret_key.trim().to_string();
            (!access_key.is_empty() || !secret_key.is_empty()).then_some(
                LegacyStoragePolicyStaticCredential {
                    access_key,
                    secret_key,
                },
            )
        });
        let application_config = take_single_legacy_row(
            &mut applications_by_policy,
            policy.id,
            "application credential",
        )?;
        let authorization = take_single_legacy_row(
            &mut authorizations_by_policy,
            policy.id,
            "authorization credential",
        )?;
        let input = LegacyStorageConnectorCredentialInput {
            static_credential,
            application_config,
            authorization,
        };
        if input.is_empty() {
            continue;
        }

        let connector = connectors.require_policy(policy)?;
        let descriptor = connector.descriptor();
        let Some(imported) = connector.import_legacy_credential(
            &config.auth.storage_credential_secret_key,
            policy,
            input,
        )?
        else {
            continue;
        };
        let existing = storage_policy_connector_credential::Entity::find()
            .filter(storage_policy_connector_credential::Column::PolicyId.eq(policy.id))
            .one(&transaction)
            .await
            .map_err(AsterError::from)?;
        if let Some(existing) = existing {
            let existing_payload = crate::storage::connectors::decode_connector_credential(
                &config.auth.storage_credential_secret_key,
                &existing,
                &descriptor.connector_id,
                descriptor.config_schema_version,
            )?;
            if existing_payload != imported {
                return Err(AsterError::database_operation(format!(
                    "storage policy {} has conflicting legacy and connector-owned credentials",
                    policy.id
                )));
            }
            report.already_current += 1;
            continue;
        }

        crate::storage::connectors::persist_connector_credential_payload(
            &transaction,
            &config.auth.storage_credential_secret_key,
            policy.id,
            &descriptor.connector_id,
            descriptor.config_schema_version,
            &imported,
        )
        .await?;
        report.migrated += 1;
    }

    ensure_no_orphaned_legacy_rows(
        &static_by_policy,
        &applications_by_policy,
        &authorizations_by_policy,
    )?;
    clear_legacy_static_credentials(&transaction).await?;
    let deleted_applications = storage_connector_application_config::Entity::delete_many()
        .exec(&transaction)
        .await
        .map_err(AsterError::from)?
        .rows_affected;
    let deleted_authorizations = storage_policy_credential::Entity::delete_many()
        .exec(&transaction)
        .await
        .map_err(AsterError::from)?
        .rows_affected;
    report.legacy_rows_deleted = usize::try_from(
        deleted_applications
            .checked_add(deleted_authorizations)
            .ok_or_else(|| {
                AsterError::database_operation("legacy credential delete count overflow")
            })?,
    )
    .map_err(|_| AsterError::database_operation("legacy credential delete count exceeds usize"))?;

    transaction.commit().await.map_err(AsterError::from)?;
    tracing::info!(
        migrated = report.migrated,
        already_current = report.already_current,
        legacy_rows_deleted = report.legacy_rows_deleted,
        "legacy storage credentials migrated to connector-owned payloads"
    );
    Ok(report)
}

fn group_application_rows(
    rows: Vec<storage_connector_application_config::Model>,
) -> BTreeMap<i64, Vec<storage_connector_application_config::Model>> {
    let mut grouped = BTreeMap::new();
    for row in rows {
        grouped
            .entry(row.policy_id)
            .or_insert_with(Vec::new)
            .push(row);
    }
    grouped
}

fn group_authorization_rows(
    rows: Vec<storage_policy_credential::Model>,
) -> BTreeMap<i64, Vec<storage_policy_credential::Model>> {
    let mut grouped = BTreeMap::new();
    for row in rows {
        grouped
            .entry(row.policy_id)
            .or_insert_with(Vec::new)
            .push(row);
    }
    grouped
}

fn take_single_legacy_row<T>(
    rows: &mut BTreeMap<i64, Vec<T>>,
    policy_id: i64,
    kind: &str,
) -> Result<Option<T>> {
    let Some(mut rows) = rows.remove(&policy_id) else {
        return Ok(None);
    };
    if rows.len() != 1 {
        return Err(AsterError::database_operation(format!(
            "storage policy {policy_id} has multiple legacy {kind} rows"
        )));
    }
    Ok(rows.pop())
}

fn ensure_no_orphaned_legacy_rows<A, C>(
    static_rows: &BTreeMap<i64, LegacyStaticCredentialRow>,
    application_rows: &BTreeMap<i64, Vec<A>>,
    authorization_rows: &BTreeMap<i64, Vec<C>>,
) -> Result<()> {
    let orphaned_policy_id = static_rows
        .keys()
        .chain(application_rows.keys())
        .chain(authorization_rows.keys())
        .next()
        .copied();
    if let Some(policy_id) = orphaned_policy_id {
        return Err(AsterError::database_operation(format!(
            "legacy storage credentials reference missing storage policy {policy_id}"
        )));
    }
    Ok(())
}

async fn load_legacy_static_credentials(
    db: &sea_orm::DatabaseTransaction,
) -> Result<Vec<LegacyStaticCredentialRow>> {
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
        .map_err(AsterError::from)?
        .into_iter()
        .map(|row| {
            Ok(LegacyStaticCredentialRow {
                policy_id: row.try_get_by_index(0).map_err(AsterError::from)?,
                access_key: row.try_get_by_index(1).map_err(AsterError::from)?,
                secret_key: row.try_get_by_index(2).map_err(AsterError::from)?,
            })
        })
        .collect()
}

async fn clear_legacy_static_credentials(db: &sea_orm::DatabaseTransaction) -> Result<()> {
    let statement = Query::update()
        .table(Alias::new("storage_policies"))
        .values([
            (Alias::new("access_key"), Expr::value("")),
            (Alias::new("secret_key"), Expr::value("")),
        ])
        .to_owned();
    db.execute(&statement)
        .await
        .map(|_| ())
        .map_err(AsterError::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use sea_orm::{ActiveModelTrait, DatabaseConnection, Set};
    use serde::Serialize;

    use aster_drive_model::types::{
        MicrosoftGraphCloud, OneDriveAccountMode, ProviderDownloadFilenameMode,
        ProviderDownloadStrategy, ProviderResumableUploadStrategy, StorageCredentialKind,
        StorageCredentialProvider, StorageCredentialStatus,
    };
    use aster_drive_storage::{
        ConnectorConfigEnvelope, ConnectorId, StoragePolicyBehaviorConfig,
        encode_storage_policy_config,
    };

    use crate::storage::connectors::{
        OneDriveConnectorConfigV1, OneDriveCredentialV1, builtin_storage_connector_registry,
    };

    const KEY: &str = "legacy-storage-credential-test-key-32bytes";
    const OTHER_KEY: &str = "different-storage-credential-key-32bytes";

    #[derive(Serialize)]
    struct EmptyTestConnectorConfig {}

    #[derive(Serialize)]
    struct TestOneDriveMetadata<'a> {
        cloud: MicrosoftGraphCloud,
        drive_id: &'a str,
        root_item_id: &'a str,
        root_item_name: &'a str,
        id_token: &'a str,
    }

    async fn database() -> DatabaseConnection {
        let db = crate::db::connect_with_metrics(
            &crate::config::DatabaseConfig {
                url: "sqlite::memory:".into(),
                pool_size: 1,
                retry_count: 0,
            },
            aster_drive_metrics::NoopMetrics::arc(),
        )
        .await
        .expect("credential migration test database should connect");
        aster_drive_migration::Migrator::up(&db, None)
            .await
            .expect("credential migration test schema should migrate");
        db
    }

    fn config(encryption_key: &str) -> Config {
        let mut config = Config::default();
        config.auth.storage_credential_secret_key = encryption_key.to_string();
        config
    }

    fn onedrive_config() -> OneDriveConnectorConfigV1 {
        OneDriveConnectorConfigV1 {
            base_path: String::new(),
            provider_resumable_upload_strategy: ProviderResumableUploadStrategy::ServerRelay,
            provider_download_strategy: ProviderDownloadStrategy::ServerRelay,
            provider_download_filename_mode: ProviderDownloadFilenameMode::ProviderNative,
            cloud: MicrosoftGraphCloud::Global,
            account_mode: OneDriveAccountMode::Personal,
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
        let connector_config =
            serde_json::to_value(connector_config).expect("test connector config should serialize");
        let storage_config = encode_storage_policy_config(
            ConnectorConfigEnvelope::new(ConnectorId::declared(connector_id), 1, connector_config),
            StoragePolicyBehaviorConfig::default(),
        )
        .expect("test storage policy config should encode");
        // Keep the 0.5.0 migration fixture compatible with databases created
        // before the legacy policy columns are removed in 0.6.0. The current
        // production entity deliberately has no fields for these columns.
        let driver_type = connector_id
            .rsplit('.')
            .next()
            .expect("connector id should contain a driver suffix");
        let now = Utc::now();
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
                Expr::value(id).into(),
                Expr::value(format!("policy-{id}")).into(),
                Expr::value(driver_type).into(),
                Expr::value("").into(),
                Expr::value("").into(),
                Expr::value("").into(),
                Expr::value("").into(),
                Expr::value("").into(),
                Expr::value(Option::<i64>::None).into(),
                Expr::value(Option::<String>::None).into(),
                Expr::value(0_i64).into(),
                Expr::value("[]").into(),
                Expr::value("{}").into(),
                Expr::value(false).into(),
                Expr::value(0_i64).into(),
                Expr::value(now).into(),
                Expr::value(now).into(),
                Expr::value(connector_id).into(),
                Expr::value(storage_config).into(),
            ])
            .expect("test storage policy insert values should be valid")
            .to_owned();
        db.execute(&statement)
            .await
            .expect("test storage policy should insert");
    }

    async fn set_legacy_static_credential(
        db: &DatabaseConnection,
        policy_id: i64,
        access_key: &str,
        secret_key: &str,
    ) {
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
            .expect("legacy static credential should update");
    }

    async fn insert_legacy_application(
        db: &DatabaseConnection,
        policy_id: i64,
        encryption_key: &str,
        provider: StorageCredentialProvider,
        ciphertext: Option<String>,
    ) {
        let now = Utc::now();
        let ciphertext = ciphertext.or_else(|| {
            Some(
                crate::services::storage_policy::credential::encrypt_application_client_secret(
                    encryption_key,
                    policy_id,
                    "client-secret",
                )
                .expect("legacy application secret should encrypt"),
            )
        });
        storage_connector_application_config::ActiveModel {
            policy_id: Set(policy_id),
            provider: Set(provider),
            tenant_id: Set(Some(" common ".to_string())),
            scopes: Set(
                serde_json::to_string(&vec!["offline_access", "Files.ReadWrite"])
                    .expect("legacy application scopes should serialize"),
            ),
            client_id: Set(Some(" client-id ".to_string())),
            client_secret_ciphertext: Set(ciphertext),
            metadata: Set(serde_json::to_string(
                &serde_json::Map::<String, serde_json::Value>::new(),
            )
            .expect("legacy application metadata should serialize")),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("legacy application credential should insert");
    }

    async fn insert_legacy_authorization(
        db: &DatabaseConnection,
        policy_id: i64,
        encryption_key: &str,
        provider: StorageCredentialProvider,
        credential_kind: StorageCredentialKind,
        access_ciphertext: Option<String>,
    ) {
        let now = Utc::now();
        let access_ciphertext = access_ciphertext.or_else(|| {
            Some(
                crate::services::storage_policy::credential::crypto::encrypt_token(
                    encryption_key,
                    crate::services::storage_policy::credential::crypto::token_aad(
                        policy_id,
                        StorageCredentialProvider::MicrosoftGraph.as_str(),
                        "access",
                    )
                    .as_bytes(),
                    "access-token",
                )
                .expect("legacy access token should encrypt"),
            )
        });
        let refresh_ciphertext =
            crate::services::storage_policy::credential::crypto::encrypt_token(
                encryption_key,
                crate::services::storage_policy::credential::crypto::token_aad(
                    policy_id,
                    StorageCredentialProvider::MicrosoftGraph.as_str(),
                    "refresh",
                )
                .as_bytes(),
                "refresh-token",
            )
            .expect("legacy refresh token should encrypt");
        let metadata = serde_json::to_string(&TestOneDriveMetadata {
            cloud: MicrosoftGraphCloud::Global,
            drive_id: "drive-id",
            root_item_id: "root-item-id",
            root_item_name: "Documents",
            id_token: "***REDACTED***",
        })
        .expect("legacy authorization metadata should serialize");
        storage_policy_credential::ActiveModel {
            policy_id: Set(policy_id),
            provider: Set(provider),
            credential_kind: Set(credential_kind),
            account_label: Set(Some(" Documents ".to_string())),
            subject: Set(Some(" subject-id ".to_string())),
            tenant_id: Set(Some(" common ".to_string())),
            scopes: Set(
                serde_json::to_string(&vec!["offline_access", "Files.ReadWrite"])
                    .expect("legacy authorization scopes should serialize"),
            ),
            access_token_ciphertext: Set(access_ciphertext),
            refresh_token_ciphertext: Set(Some(refresh_ciphertext)),
            metadata: Set(metadata),
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
        .expect("legacy authorization credential should insert");
    }

    async fn stored_payload(
        db: &DatabaseConnection,
        encryption_key: &str,
        policy_id: i64,
        connector_id: &str,
    ) -> serde_json::Value {
        let record =
            crate::db::repository::storage_policy_connector_credential_repo::find_by_policy(
                db, policy_id,
            )
            .await
            .expect("connector credential lookup should succeed")
            .expect("connector credential should exist");
        crate::storage::connectors::decode_connector_credential(
            encryption_key,
            &record,
            &ConnectorId::declared(connector_id),
            1,
        )
        .expect("connector credential should decrypt")
    }

    async fn assert_legacy_static_cleared(db: &DatabaseConnection) {
        let rows = load_legacy_static_credentials(
            &db.begin()
                .await
                .expect("legacy static verification transaction should begin"),
        )
        .await
        .expect("legacy static rows should load");
        assert!(
            rows.iter()
                .all(|row| row.access_key.is_empty() && row.secret_key.is_empty())
        );
    }

    #[tokio::test]
    async fn migrates_all_static_connector_credentials_and_clears_legacy_columns() {
        let db = database().await;
        let connectors = builtin_storage_connector_registry().unwrap();
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
            insert_policy(&db, policy_id, connector_id, EmptyTestConnectorConfig {}).await;
            set_legacy_static_credential(&db, policy_id, " legacy-id ", " legacy-secret ").await;
        }

        let report = migrate_legacy_storage_credentials(&db, &config(KEY), &connectors)
            .await
            .unwrap();
        assert_eq!(report.migrated, cases.len());
        assert_eq!(report.already_current, 0);
        for (policy_id, connector_id, id_field, secret_field) in cases {
            let payload = stored_payload(&db, KEY, policy_id, connector_id).await;
            assert_eq!(payload[id_field], "legacy-id");
            assert_eq!(payload[secret_field], "legacy-secret");
            assert!(payload.get("access_key").is_none());
            assert!(payload.get("secret_key").is_none());
        }
        assert_legacy_static_cleared(&db).await;
    }

    #[tokio::test]
    async fn migrates_onedrive_application_without_authorization() {
        let db = database().await;
        insert_policy(&db, 1, "asterdrive.storage.onedrive", onedrive_config()).await;
        insert_legacy_application(&db, 1, KEY, StorageCredentialProvider::MicrosoftGraph, None)
            .await;

        let report = migrate_legacy_storage_credentials(
            &db,
            &config(KEY),
            &builtin_storage_connector_registry().unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(report.migrated, 1);
        assert_eq!(report.legacy_rows_deleted, 1);
        let payload: OneDriveCredentialV1 = serde_json::from_value(
            stored_payload(&db, KEY, 1, "asterdrive.storage.onedrive").await,
        )
        .unwrap();
        assert_eq!(payload.application.client_id, "client-id");
        assert_eq!(payload.application.client_secret, "client-secret");
        assert!(payload.authorization.is_none());
        assert!(
            storage_connector_application_config::Entity::find()
                .all(&db)
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn merges_onedrive_application_and_oauth_credentials() {
        let db = database().await;
        insert_policy(&db, 1, "asterdrive.storage.onedrive", onedrive_config()).await;
        insert_legacy_application(&db, 1, KEY, StorageCredentialProvider::MicrosoftGraph, None)
            .await;
        insert_legacy_authorization(
            &db,
            1,
            KEY,
            StorageCredentialProvider::MicrosoftGraph,
            StorageCredentialKind::OauthDelegated,
            None,
        )
        .await;

        let report = migrate_legacy_storage_credentials(
            &db,
            &config(KEY),
            &builtin_storage_connector_registry().unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(report.migrated, 1);
        assert_eq!(report.legacy_rows_deleted, 2);
        let payload: OneDriveCredentialV1 = serde_json::from_value(
            stored_payload(&db, KEY, 1, "asterdrive.storage.onedrive").await,
        )
        .unwrap();
        let authorization = payload.authorization.unwrap();
        assert_eq!(authorization.access_token, "access-token");
        assert_eq!(
            authorization.refresh_token.as_deref(),
            Some("refresh-token")
        );
        assert_eq!(authorization.metadata.drive_id, "drive-id");
        assert_eq!(authorization.metadata.root_item_id, "root-item-id");
        assert!(authorization.metadata.id_token_present);
        assert_eq!(authorization.account_label.as_deref(), Some("Documents"));
        assert_eq!(authorization.subject.as_deref(), Some("subject-id"));
    }

    #[tokio::test]
    async fn rejects_onedrive_oauth_without_application_and_preserves_legacy_row() {
        let db = database().await;
        insert_policy(&db, 1, "asterdrive.storage.onedrive", onedrive_config()).await;
        insert_legacy_authorization(
            &db,
            1,
            KEY,
            StorageCredentialProvider::MicrosoftGraph,
            StorageCredentialKind::OauthDelegated,
            None,
        )
        .await;

        let error = migrate_legacy_storage_credentials(
            &db,
            &config(KEY),
            &builtin_storage_connector_registry().unwrap(),
        )
        .await
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("without application credentials")
        );
        assert_eq!(
            storage_policy_credential::Entity::find()
                .all(&db)
                .await
                .unwrap()
                .len(),
            1
        );
        assert!(
            crate::db::repository::storage_policy_connector_credential_repo::find_by_policy(&db, 1)
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn rejects_onedrive_provider_and_kind_mismatches() {
        for (provider, credential_kind, expected) in [
            (
                StorageCredentialProvider::GoogleDrive,
                StorageCredentialKind::OauthDelegated,
                "application provider",
            ),
            (
                StorageCredentialProvider::MicrosoftGraph,
                StorageCredentialKind::ServiceAccount,
                "authorization provider",
            ),
        ] {
            let db = database().await;
            insert_policy(&db, 1, "asterdrive.storage.onedrive", onedrive_config()).await;
            insert_legacy_application(&db, 1, KEY, provider, None).await;
            if provider == StorageCredentialProvider::MicrosoftGraph {
                insert_legacy_authorization(&db, 1, KEY, provider, credential_kind, None).await;
            }

            let error = migrate_legacy_storage_credentials(
                &db,
                &config(KEY),
                &builtin_storage_connector_registry().unwrap(),
            )
            .await
            .unwrap_err();
            assert!(error.to_string().contains(expected), "{error}");
        }
    }

    #[tokio::test]
    async fn rejects_incomplete_static_credentials_but_accepts_empty_columns() {
        let db = database().await;
        insert_policy(&db, 1, "asterdrive.storage.s3", EmptyTestConnectorConfig {}).await;
        set_legacy_static_credential(&db, 1, "only-id", "").await;
        let error = migrate_legacy_storage_credentials(
            &db,
            &config(KEY),
            &builtin_storage_connector_registry().unwrap(),
        )
        .await
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("incomplete legacy static credentials")
        );
        assert_eq!(
            load_legacy_static_credentials(&db.begin().await.unwrap())
                .await
                .unwrap()[0]
                .access_key,
            "only-id"
        );

        set_legacy_static_credential(&db, 1, "", "").await;
        let report = migrate_legacy_storage_credentials(
            &db,
            &config(KEY),
            &builtin_storage_connector_registry().unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(report, LegacyStorageCredentialMigrationReport::default());
        assert!(
            crate::db::repository::storage_policy_connector_credential_repo::find_by_policy(&db, 1)
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn rejects_corrupt_or_wrong_key_onedrive_ciphertext_without_cleanup() {
        for (stored_key, startup_key, ciphertext) in [
            (KEY, KEY, Some("not-a-ciphertext".to_string())),
            (KEY, OTHER_KEY, None),
        ] {
            let db = database().await;
            insert_policy(&db, 1, "asterdrive.storage.onedrive", onedrive_config()).await;
            insert_legacy_application(
                &db,
                1,
                stored_key,
                StorageCredentialProvider::MicrosoftGraph,
                ciphertext,
            )
            .await;

            let error = migrate_legacy_storage_credentials(
                &db,
                &config(startup_key),
                &builtin_storage_connector_registry().unwrap(),
            )
            .await
            .unwrap_err();
            assert!(
                error.to_string().contains("decrypt") || error.to_string().contains("ciphertext")
            );
            assert_eq!(
                storage_connector_application_config::Entity::find()
                    .all(&db)
                    .await
                    .unwrap()
                    .len(),
                1
            );
        }
    }

    #[tokio::test]
    async fn matching_target_is_idempotent_but_conflicting_target_aborts() {
        let db = database().await;
        insert_policy(&db, 1, "asterdrive.storage.s3", EmptyTestConnectorConfig {}).await;
        set_legacy_static_credential(&db, 1, "id-one", "secret-one").await;
        let connectors = builtin_storage_connector_registry().unwrap();
        migrate_legacy_storage_credentials(&db, &config(KEY), &connectors)
            .await
            .unwrap();

        set_legacy_static_credential(&db, 1, "id-one", "secret-one").await;
        let report = migrate_legacy_storage_credentials(&db, &config(KEY), &connectors)
            .await
            .unwrap();
        assert_eq!(report.migrated, 0);
        assert_eq!(report.already_current, 1);
        assert_legacy_static_cleared(&db).await;

        set_legacy_static_credential(&db, 1, "id-two", "secret-two").await;
        let error = migrate_legacy_storage_credentials(&db, &config(KEY), &connectors)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("conflicting legacy"));
        let payload = stored_payload(&db, KEY, 1, "asterdrive.storage.s3").await;
        assert_eq!(payload["s3_access_key_id"], "id-one");
        assert_eq!(
            load_legacy_static_credentials(&db.begin().await.unwrap())
                .await
                .unwrap()[0]
                .access_key,
            "id-two"
        );
    }

    #[tokio::test]
    async fn failure_rolls_back_prior_policy_import_and_all_cleanup() {
        let db = database().await;
        insert_policy(&db, 1, "asterdrive.storage.s3", EmptyTestConnectorConfig {}).await;
        insert_policy(
            &db,
            2,
            "asterdrive.storage.sftp",
            EmptyTestConnectorConfig {},
        )
        .await;
        set_legacy_static_credential(&db, 1, "good-id", "good-secret").await;
        set_legacy_static_credential(&db, 2, "broken-user", "").await;

        let error = migrate_legacy_storage_credentials(
            &db,
            &config(KEY),
            &builtin_storage_connector_registry().unwrap(),
        )
        .await
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("incomplete legacy static credentials")
        );
        assert!(
            crate::db::repository::storage_policy_connector_credential_repo::find_all(&db)
                .await
                .unwrap()
                .is_empty()
        );
        let rows = load_legacy_static_credentials(&db.begin().await.unwrap())
            .await
            .unwrap();
        assert_eq!(rows[0].access_key, "good-id");
        assert_eq!(rows[0].secret_key, "good-secret");
        assert_eq!(rows[1].access_key, "broken-user");
    }
}
