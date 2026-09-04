//! One-shot conversion from the 0.5.0 flattened remote-target schema.
//!
//! TODO(remote-storage-target-0.7.0): delete this conversion and the legacy
//! columns after every supported deployment has crossed the 0.6.0 upgrade.

use crate::errors::{AsterError, Result};
use aster_drive_model::entities::remote_storage_target;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set,
    TransactionTrait,
};

const LOCAL_CONNECTOR_ID: &str = "asterdrive.storage.local";
const S3_CONNECTOR_ID: &str = "asterdrive.storage.s3";
const UNRESOLVED_CONNECTOR_ID: &str = "asterdrive.storage.unresolved";

fn legacy_connector_mapping(
    driver_type: &str,
    access_key: &str,
    secret_key: &str,
) -> Result<&'static str> {
    match driver_type {
        "local" => Ok(LOCAL_CONNECTOR_ID),
        "s3" if !access_key.trim().is_empty() && !secret_key.trim().is_empty() => {
            Ok(S3_CONNECTOR_ID)
        }
        "s3" => Err(AsterError::database_operation(
            "legacy S3 credential payload is incomplete",
        )),
        other => Err(AsterError::database_operation(format!(
            "unknown legacy remote target driver '{other}'"
        ))),
    }
}

/// Convert legacy rows atomically before the server starts listening.
pub async fn convert_legacy_rows(db: &DatabaseConnection, encryption_key: &str) -> Result<()> {
    let txn = db.begin().await.map_err(AsterError::from)?;
    let rows = remote_storage_target::Entity::find()
        .filter(remote_storage_target::Column::ConnectorId.is_null())
        .all(&txn)
        .await
        .map_err(AsterError::from)?;

    for target in rows {
        let id = target.id;
        let driver_type = target.driver_type.as_str().to_string();
        let endpoint = target.endpoint.clone();
        let bucket = target.bucket.clone();
        let access_key = target.access_key.clone();
        let secret_key = target.secret_key.clone();
        let base_path = target.base_path.clone();
        let mapping = legacy_connector_mapping(&driver_type, &access_key, &secret_key);
        let values = match driver_type.as_str() {
            "local" => serde_json::json!({ "base_path": base_path }),
            "s3" => {
                serde_json::json!({
                    "endpoint": endpoint,
                    "bucket": bucket,
                    "base_path": base_path,
                })
            }
            _ => serde_json::json!({
                "legacy_driver_type": driver_type,
                "endpoint": endpoint,
                "bucket": bucket,
                "base_path": base_path,
            }),
        };
        let (connector_id, migration_error) = match mapping {
            Ok(connector_id) => (connector_id, None),
            Err(error) => (UNRESOLVED_CONNECTOR_ID, Some(error.to_string())),
        };
        let config = serde_json::to_string(&serde_json::json!({
            "format_version": 1,
            "connector_id": connector_id,
            "schema_version": 1,
            "values": values,
        }))
        .map_err(|error| AsterError::database_operation(error.to_string()))?;
        let mut active: remote_storage_target::ActiveModel = target.into();
        active.connector_id = Set(Some(connector_id.to_string()));
        active.connector_config = Set(Some(config));

        if connector_id == S3_CONNECTOR_ID {
            let plaintext = serde_json::to_string(&serde_json::json!({
                "s3_access_key_id": access_key,
                "s3_secret_access_key": secret_key,
            }))
            .map_err(|error| AsterError::database_operation(error.to_string()))?;
            let ciphertext =
                crate::services::storage_policy::credential::crypto::encrypt_connector_credential(
                    encryption_key,
                    id,
                    connector_id,
                    1,
                    &plaintext,
                )?;
            crate::db::repository::remote_storage_target_credential_repo::upsert(
                &txn,
                id,
                connector_id.to_string(),
                1,
                ciphertext,
            )
            .await?;
        }
        if let Some(error) = migration_error {
            active.last_error = Set(format!(
                "legacy remote storage target migration failed: {error}"
            ));
        }
        active.access_key = Set(String::new());
        active.secret_key = Set(String::new());
        active.update(&txn).await.map_err(AsterError::from)?;
    }
    txn.commit().await.map_err(AsterError::from)
}

#[cfg(test)]
mod tests {
    use super::{
        LOCAL_CONNECTOR_ID, S3_CONNECTOR_ID, UNRESOLVED_CONNECTOR_ID, convert_legacy_rows,
        legacy_connector_mapping,
    };

    #[test]
    fn legacy_mapping_rejects_unknown_and_incomplete_rows() {
        assert_eq!(
            legacy_connector_mapping("local", "", "").unwrap(),
            LOCAL_CONNECTOR_ID
        );
        assert_eq!(
            legacy_connector_mapping("s3", "ACCESS", "SECRET").unwrap(),
            S3_CONNECTOR_ID
        );
        assert!(legacy_connector_mapping("s3", "", "SECRET").is_err());
        assert!(legacy_connector_mapping("gcs", "ACCESS", "SECRET").is_err());
    }

    #[tokio::test]
    async fn sqlite_conversion_writes_generic_config_and_encrypted_credentials() {
        use aster_drive_model::entities::{master_binding, remote_storage_target};
        use sea_orm::{ActiveModelTrait, Database, EntityTrait, Set};

        let db = Database::connect("sqlite::memory:").await.unwrap();
        aster_drive_migration::Migrator::up(&db, None)
            .await
            .unwrap();
        let now = chrono::Utc::now();
        master_binding::ActiveModel {
            id: Set(1),
            name: Set("migration-binding".into()),
            master_url: Set("https://primary.example.test".into()),
            access_key: Set("ak".into()),
            secret_key: Set("sk".into()),
            storage_namespace: Set("ns".into()),
            is_enabled: Set(true),
            resolved_transport: Set(
                aster_drive_model::types::ResolvedRemoteTransport::ReverseTunnel,
            ),
            desired_revision: Set(1),
            applied_revision: Set(1),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(&db)
        .await
        .unwrap();
        remote_storage_target::ActiveModel {
            id: Set(1),
            master_binding_id: Set(1),
            target_key: Set("legacy-s3".into()),
            name: Set("Legacy S3".into()),
            driver_type: Set("s3".to_string()),
            endpoint: Set("https://s3.example.test".into()),
            bucket: Set("bucket".into()),
            access_key: Set("ACCESS".into()),
            secret_key: Set("SECRET".into()),
            base_path: Set("prefix".into()),
            is_default: Set(true),
            desired_revision: Set(1),
            applied_revision: Set(1),
            last_error: Set(String::new()),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();

        remote_storage_target::ActiveModel {
            id: Set(2),
            master_binding_id: Set(1),
            target_key: Set("legacy-local".into()),
            name: Set("Legacy Local".into()),
            driver_type: Set("local".into()),
            endpoint: Set(String::new()),
            bucket: Set(String::new()),
            access_key: Set(String::new()),
            secret_key: Set(String::new()),
            base_path: Set("local-data".into()),
            is_default: Set(false),
            desired_revision: Set(1),
            applied_revision: Set(1),
            last_error: Set(String::new()),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();
        remote_storage_target::ActiveModel {
            id: Set(3),
            master_binding_id: Set(1),
            target_key: Set("legacy-unknown".into()),
            name: Set("Legacy Unknown".into()),
            driver_type: Set("gcs".into()),
            endpoint: Set("https://gcs.example.test".into()),
            bucket: Set("bucket".into()),
            access_key: Set("ACCESS".into()),
            secret_key: Set("SECRET".into()),
            base_path: Set("unknown".into()),
            is_default: Set(false),
            desired_revision: Set(1),
            applied_revision: Set(1),
            last_error: Set(String::new()),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();

        convert_legacy_rows(&db, "test-encryption-key-0123456789012345")
            .await
            .unwrap();
        let target = remote_storage_target::Entity::find_by_id(1)
            .one(&db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(target.connector_id.as_deref(), Some(S3_CONNECTOR_ID));
        let legacy = remote_storage_target::Entity::find()
            .one(&db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(legacy.access_key, "");
        assert_eq!(legacy.secret_key, "");
        let config: serde_json::Value =
            serde_json::from_str(target.connector_config.as_deref().unwrap()).unwrap();
        assert_eq!(config["connector_id"], S3_CONNECTOR_ID);
        assert_eq!(config["values"]["bucket"], "bucket");
        let credential =
            crate::db::repository::remote_storage_target_credential_repo::find_by_target(
                &db, target.id,
            )
            .await
            .unwrap()
            .unwrap();
        assert_ne!(credential.ciphertext, "ACCESS");
        assert_ne!(credential.ciphertext, "SECRET");

        // Startup conversion is idempotent: a second pass leaves the row untouched.
        convert_legacy_rows(&db, "test-encryption-key-0123456789012345")
            .await
            .unwrap();
        let second = remote_storage_target::Entity::find()
            .one(&db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(second.connector_id, target.connector_id);
        assert_eq!(second.connector_config, target.connector_config);

        let local = remote_storage_target::Entity::find_by_id(2)
            .one(&db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(local.connector_id.as_deref(), Some(LOCAL_CONNECTOR_ID));
        assert_eq!(local.last_error, "");

        let unresolved = remote_storage_target::Entity::find_by_id(3)
            .one(&db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            unresolved.connector_id.as_deref(),
            Some(UNRESOLVED_CONNECTOR_ID)
        );
        assert!(
            unresolved
                .last_error
                .contains("unknown legacy remote target driver")
        );
        let _: crate::storage::remote_protocol::RemoteStorageTargetInfo =
            unresolved.try_into().unwrap();
    }
}
