//! AsterDrive 0.5.0-only conversion from flattened target rows.

use std::collections::{BTreeMap, HashMap};

use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseTransaction, EntityTrait, QueryFilter, Set};
use serde_json::Value;

use crate::db::repository::remote_storage_target_credential_repo;
use crate::errors::{AsterError, Result};
use aster_drive_model::entities::{remote_storage_target, remote_storage_target_credential};
use aster_drive_storage::ConnectorConfigEnvelope;

use super::credential;
use super::driver::{import_legacy_remote_storage_target, validate_registered_persisted_connector};

pub(crate) async fn migrate_legacy_remote_storage_targets(
    transaction: &DatabaseTransaction,
    encryption_key: &str,
) -> Result<usize> {
    let targets = remote_storage_target::Entity::find()
        .all(transaction)
        .await
        .map_err(AsterError::from)?;
    let credentials = remote_storage_target_credential::Entity::find()
        .all(transaction)
        .await
        .map_err(AsterError::from)?;
    let target_ids = targets
        .iter()
        .map(|target| target.id)
        .collect::<std::collections::HashSet<_>>();
    let mut credentials_by_target = HashMap::new();
    for row in credentials {
        if !target_ids.contains(&row.target_id) {
            return Err(AsterError::database_operation(format!(
                "remote storage target credential #{} is orphaned from target #{}",
                row.id, row.target_id
            )));
        }
        if credentials_by_target.insert(row.target_id, row).is_some() {
            return Err(AsterError::database_operation(
                "remote storage target has duplicate credential records",
            ));
        }
    }
    let mut converted = Vec::new();
    for target in targets {
        if !target.connector_id.trim().is_empty() || !target.connector_config.trim().is_empty() {
            validate_current_row(
                &target,
                credentials_by_target.get(&target.id),
                encryption_key,
            )?;
            continue;
        }
        if credentials_by_target.contains_key(&target.id) {
            return Err(AsterError::database_operation(format!(
                "remote storage target #{} has a conflicting credential record",
                target.id
            )));
        }
        converted.push((
            target.clone(),
            import_legacy_remote_storage_target(&target)?,
        ));
    }

    let migrated = converted.len();
    for (target, imported) in converted {
        if remote_storage_target_credential::Entity::find()
            .filter(remote_storage_target_credential::Column::TargetId.eq(target.id))
            .one(transaction)
            .await
            .map_err(AsterError::from)?
            .is_some()
        {
            return Err(AsterError::database_operation(format!(
                "remote storage target #{} has a conflicting credential record",
                target.id
            )));
        }
        let connector_id = imported.config.connector_id.as_str();
        let connector_config = serde_json::to_string(&imported.config)
            .map_err(|error| AsterError::internal_error(error.to_string()))?;
        if let Some(plaintext) = imported.credential_json {
            let schema_version = imported.credential_schema_version.ok_or_else(|| {
                AsterError::internal_error(
                    "imported remote storage target credential is missing its schema version",
                )
            })?;
            let ciphertext = credential::encrypt(
                encryption_key,
                target.id,
                connector_id,
                schema_version,
                &plaintext,
            )?;
            remote_storage_target_credential_repo::upsert(
                transaction,
                target.id,
                connector_id.to_string(),
                schema_version as i32,
                ciphertext,
            )
            .await?;
        }
        let mut active: remote_storage_target::ActiveModel = target.into();
        active.connector_id = Set(connector_id.to_string());
        active.connector_config = Set(connector_config);
        active.driver_type = Set(String::new());
        active.endpoint = Set(String::new());
        active.bucket = Set(String::new());
        active.access_key = Set(String::new());
        active.secret_key = Set(String::new());
        active.base_path = Set(String::new());
        active.update(transaction).await.map_err(AsterError::from)?;
    }
    Ok(migrated)
}

fn validate_current_row(
    target: &remote_storage_target::Model,
    credential_row: Option<&remote_storage_target_credential::Model>,
    encryption_key: &str,
) -> Result<()> {
    if target.connector_id.trim().is_empty() || target.connector_config.trim().is_empty() {
        return Err(AsterError::database_operation(format!(
            "remote storage target #{} has a partial connector payload",
            target.id
        )));
    }
    if !target.driver_type.is_empty()
        || !target.endpoint.is_empty()
        || !target.bucket.is_empty()
        || !target.access_key.is_empty()
        || !target.secret_key.is_empty()
        || !target.base_path.is_empty()
    {
        return Err(AsterError::database_operation(format!(
            "remote storage target #{} has conflicting connector and legacy payloads",
            target.id
        )));
    }
    let envelope: ConnectorConfigEnvelope = serde_json::from_str(&target.connector_config)
        .map_err(|error| AsterError::database_operation(error.to_string()))?;
    if envelope.format_version != aster_drive_storage::CONNECTOR_CONFIG_FORMAT_VERSION
        || envelope.connector_id.as_str() != target.connector_id
    {
        return Err(AsterError::database_operation(format!(
            "remote storage target #{} does not match its config envelope",
            target.id
        )));
    }
    let mut credential_values = None;
    if let Some(row) = credential_row {
        if row.connector_id != target.connector_id || row.schema_version <= 0 {
            return Err(AsterError::database_operation(format!(
                "remote storage target #{} credential metadata does not match its config envelope",
                target.id
            )));
        }
        let schema_version = aster_forge_utils::numbers::i32_to_usize(
            row.schema_version,
            "remote storage target credential schema version",
        )
        .and_then(|value| {
            aster_forge_utils::numbers::usize_to_u32(
                value,
                "remote storage target credential schema version",
            )
        })
        .map_err(|error| AsterError::database_operation(error.to_string()))?;
        let plaintext = credential::decrypt(
            encryption_key,
            target.id,
            &row.connector_id,
            schema_version,
            &row.ciphertext,
        )?;
        let values: BTreeMap<String, Value> = serde_json::from_str(&plaintext).map_err(|_| {
            AsterError::database_operation(format!(
                "remote storage target #{} credential payload is not a valid value map",
                target.id
            ))
        })?;
        credential_values = Some((schema_version, values));
    }
    validate_registered_persisted_connector(
        target,
        &envelope,
        credential_values
            .as_ref()
            .map(|(schema_version, values)| (*schema_version, values)),
    )
}
