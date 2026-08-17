use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::ToSchema;

use crate::api::api_error_code::ApiErrorCode;
use crate::config::Config;
use crate::db::repository::remote_storage_target_credential_repo;
use crate::errors::{AsterError, MapAsterErr, Result, validation_error_with_code};
use crate::runtime::FollowerRuntimeState;
use crate::storage::drivers::s3_config::normalize_s3_endpoint_and_bucket;
use crate::storage::drivers::{
    local::LocalDriver,
    s3::{S3Driver, S3DriverConfig, S3DriverOptions, S3StaticCredentials},
};
use crate::storage::remote_protocol::RemoteStorageTargetCredentialInput;
use aster_drive_model::entities::remote_storage_target;
use aster_drive_storage::connector_descriptor::{
    StorageConnectorFieldDescriptor, StorageConnectorFieldKind, StorageConnectorFieldScope,
    storage_connector_field,
};
use aster_drive_storage::field_contract::{
    normalize_object_storage_prefix, normalize_required_storage_field,
};
use aster_drive_storage::{ConnectorConfigEnvelope, ConnectorId, StorageDriver};

use super::credential;
use super::paths::{normalize_relative_local_path, resolve_remote_storage_target_local_path};

pub const LOCAL_CONNECTOR_ID: &str = "asterdrive.remote-target.local";
pub const S3_CONNECTOR_ID: &str = "asterdrive.remote-target.s3";
pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct RemoteStorageTargetConnectorDescriptor {
    pub connector_id: ConnectorId,
    pub label_key: String,
    pub description_key: String,
    pub config_schema_version: u32,
    pub credential_schema_version: Option<u32>,
    pub fields: Vec<StorageConnectorFieldDescriptor>,
}

pub(in crate::services::remote::storage_target) struct NormalizedConnectorInput {
    pub config: ConnectorConfigEnvelope,
    pub credential_json: Option<String>,
    pub credential_schema_version: Option<u32>,
}

pub(in crate::services::remote::storage_target) struct ImportedLegacyRemoteStorageTarget {
    pub config: ConnectorConfigEnvelope,
    pub credential_json: Option<String>,
    pub credential_schema_version: Option<u32>,
}

struct RemoteStorageTargetConnectorContext<'a> {
    writer_db: &'a DatabaseConnection,
    config: &'a Config,
}

#[async_trait]
trait RemoteStorageTargetConnector: Send + Sync {
    fn descriptor(&self) -> RemoteStorageTargetConnectorDescriptor;

    fn legacy_driver_type(&self) -> Option<&'static str> {
        None
    }

    fn normalize(
        &self,
        config: ConnectorConfigEnvelope,
        credential: Option<RemoteStorageTargetCredentialInput>,
        saved_credential: Option<String>,
    ) -> Result<NormalizedConnectorInput>;

    fn import_legacy_v050(
        &self,
        target: &remote_storage_target::Model,
    ) -> Result<ImportedLegacyRemoteStorageTarget>;

    fn validate_persisted(
        &self,
        config: &ConnectorConfigEnvelope,
        credential: Option<&BTreeMap<String, Value>>,
    ) -> Result<()>;

    async fn build_driver(
        &self,
        context: &RemoteStorageTargetConnectorContext<'_>,
        target: &remote_storage_target::Model,
    ) -> Result<Arc<dyn StorageDriver>>;
}

#[derive(Clone)]
struct RemoteStorageTargetConnectorRegistry {
    ordered: Vec<Arc<dyn RemoteStorageTargetConnector>>,
    by_connector_id: HashMap<ConnectorId, Arc<dyn RemoteStorageTargetConnector>>,
    by_legacy_driver_type: HashMap<&'static str, Arc<dyn RemoteStorageTargetConnector>>,
}

impl RemoteStorageTargetConnectorRegistry {
    fn new(connectors: Vec<Arc<dyn RemoteStorageTargetConnector>>) -> Result<Self> {
        let mut by_connector_id = HashMap::with_capacity(connectors.len());
        let mut by_legacy_driver_type = HashMap::new();
        for connector in &connectors {
            let descriptor = connector.descriptor();
            validate_descriptor(&descriptor)?;
            if by_connector_id
                .insert(descriptor.connector_id.clone(), connector.clone())
                .is_some()
            {
                return Err(AsterError::internal_error(format!(
                    "remote storage target connector '{}' is registered more than once",
                    descriptor.connector_id
                )));
            }
            if let Some(legacy_driver_type) = connector.legacy_driver_type()
                && by_legacy_driver_type
                    .insert(legacy_driver_type, connector.clone())
                    .is_some()
            {
                return Err(AsterError::internal_error(format!(
                    "legacy remote storage target driver '{legacy_driver_type}' is registered more than once"
                )));
            }
        }
        Ok(Self {
            ordered: connectors,
            by_connector_id,
            by_legacy_driver_type,
        })
    }

    fn require_input_connector(
        &self,
        connector_id: &ConnectorId,
    ) -> Result<Arc<dyn RemoteStorageTargetConnector>> {
        connector_id
            .validate()
            .map_err(|error| AsterError::validation_error(error.to_string()))?;
        self.by_connector_id
            .get(connector_id)
            .cloned()
            .ok_or_else(|| unavailable_connector_error(connector_id.as_str()))
    }

    fn require_target(
        &self,
        target: &remote_storage_target::Model,
    ) -> Result<Arc<dyn RemoteStorageTargetConnector>> {
        let connector_id = ConnectorId::declared(target.connector_id.clone());
        connector_id.validate().map_err(|error| {
            AsterError::database_operation(format!(
                "remote storage target #{} has invalid connector id '{}': {error}",
                target.id, target.connector_id
            ))
        })?;
        self.by_connector_id
            .get(&connector_id)
            .cloned()
            .ok_or_else(|| {
                AsterError::database_operation(format!(
                    "remote storage target #{} references unavailable connector '{}'",
                    target.id, target.connector_id
                ))
            })
    }

    fn connector(&self, connector_id: &str) -> Option<Arc<dyn RemoteStorageTargetConnector>> {
        self.by_connector_id
            .get(&ConnectorId::declared(connector_id))
            .cloned()
    }

    fn require_legacy_connector(
        &self,
        target: &remote_storage_target::Model,
    ) -> Result<Arc<dyn RemoteStorageTargetConnector>> {
        self.by_legacy_driver_type
            .get(target.driver_type.as_str())
            .cloned()
            .ok_or_else(|| {
                AsterError::database_operation(format!(
                    "legacy remote storage target #{} uses unknown driver '{}'",
                    target.id, target.driver_type
                ))
            })
    }

    #[cfg(test)]
    fn descriptors(&self) -> Vec<RemoteStorageTargetConnectorDescriptor> {
        self.ordered
            .iter()
            .map(|connector| connector.descriptor())
            .collect()
    }
}

fn builtin_remote_storage_target_connector_registry() -> Result<RemoteStorageTargetConnectorRegistry>
{
    RemoteStorageTargetConnectorRegistry::new(vec![
        Arc::new(LocalRemoteStorageTargetConnector),
        Arc::new(S3RemoteStorageTargetConnector),
    ])
}

fn validate_descriptor(descriptor: &RemoteStorageTargetConnectorDescriptor) -> Result<()> {
    descriptor.connector_id.validate().map_err(|error| {
        AsterError::internal_error(format!(
            "remote storage target connector '{}' declares an invalid id: {error}",
            descriptor.connector_id
        ))
    })?;
    if descriptor.config_schema_version == 0 {
        return Err(AsterError::internal_error(format!(
            "remote storage target connector '{}' declares config schema version zero",
            descriptor.connector_id
        )));
    }
    let mut names = HashSet::with_capacity(descriptor.fields.len());
    let mut has_credentials = false;
    for field in &descriptor.fields {
        field.validate().map_err(|error| {
            AsterError::internal_error(format!(
                "remote storage target connector '{}' declares invalid field '{}': {error}",
                descriptor.connector_id, field.name
            ))
        })?;
        if !matches!(
            field.scope,
            StorageConnectorFieldScope::ConnectorConfig
                | StorageConnectorFieldScope::StaticCredential
        ) {
            return Err(AsterError::internal_error(format!(
                "remote storage target connector '{}' field '{}' uses unsupported scope {:?}",
                descriptor.connector_id, field.name, field.scope
            )));
        }
        if field.scope == StorageConnectorFieldScope::StaticCredential {
            has_credentials = true;
        }
        if !names.insert(field.name.as_str()) {
            return Err(AsterError::internal_error(format!(
                "remote storage target connector '{}' declares field '{}' more than once",
                descriptor.connector_id, field.name
            )));
        }
    }
    if has_credentials != descriptor.credential_schema_version.is_some() {
        return Err(AsterError::internal_error(format!(
            "remote storage target connector '{}' credential schema does not match its fields",
            descriptor.connector_id
        )));
    }
    Ok(())
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct LocalConfigV1 {
    base_path: String,
}

struct LocalRemoteStorageTargetConnector;

#[async_trait]
impl RemoteStorageTargetConnector for LocalRemoteStorageTargetConnector {
    fn descriptor(&self) -> RemoteStorageTargetConnectorDescriptor {
        RemoteStorageTargetConnectorDescriptor {
            connector_id: ConnectorId::declared(LOCAL_CONNECTOR_ID),
            label_key: "remote_node_storage_target_connector_local".to_string(),
            description_key: "remote_node_ingress_profile_local_scope_hint".to_string(),
            config_schema_version: SCHEMA_VERSION,
            credential_schema_version: None,
            fields: vec![storage_connector_field(
                "base_path",
                StorageConnectorFieldScope::ConnectorConfig,
                StorageConnectorFieldKind::Text,
                true,
                false,
            )],
        }
    }

    fn legacy_driver_type(&self) -> Option<&'static str> {
        Some("local")
    }

    fn normalize(
        &self,
        config: ConnectorConfigEnvelope,
        credential: Option<RemoteStorageTargetCredentialInput>,
        saved_credential: Option<String>,
    ) -> Result<NormalizedConnectorInput> {
        ensure_envelope(&config, LOCAL_CONNECTOR_ID)?;
        if credential.is_some() || saved_credential.is_some() {
            return Err(AsterError::validation_error(
                "local remote target does not accept credentials",
            ));
        }
        let values: LocalConfigV1 = values(config.values)?;
        Ok(NormalizedConnectorInput {
            config: envelope(
                LOCAL_CONNECTOR_ID,
                LocalConfigV1 {
                    base_path: normalize_relative_local_path(&values.base_path)?,
                },
            )?,
            credential_json: None,
            credential_schema_version: None,
        })
    }

    fn import_legacy_v050(
        &self,
        target: &remote_storage_target::Model,
    ) -> Result<ImportedLegacyRemoteStorageTarget> {
        if !target.access_key.is_empty() || !target.secret_key.is_empty() {
            return Err(AsterError::database_operation(format!(
                "legacy local remote storage target #{} contains credentials",
                target.id
            )));
        }
        Ok(ImportedLegacyRemoteStorageTarget {
            config: envelope(
                LOCAL_CONNECTOR_ID,
                LocalConfigV1 {
                    base_path: normalize_relative_local_path(&target.base_path)?,
                },
            )?,
            credential_json: None,
            credential_schema_version: None,
        })
    }

    fn validate_persisted(
        &self,
        config: &ConnectorConfigEnvelope,
        credential: Option<&BTreeMap<String, Value>>,
    ) -> Result<()> {
        ensure_envelope(config, LOCAL_CONNECTOR_ID)?;
        if credential.is_some() {
            return Err(AsterError::database_operation(
                "local remote storage target unexpectedly has credentials",
            ));
        }
        let values: LocalConfigV1 = values(config.values.clone())?;
        normalize_relative_local_path(&values.base_path)?;
        Ok(())
    }

    async fn build_driver(
        &self,
        context: &RemoteStorageTargetConnectorContext<'_>,
        target: &remote_storage_target::Model,
    ) -> Result<Arc<dyn StorageDriver>> {
        let config: LocalConfigV1 = values(decode_config(target, LOCAL_CONNECTOR_ID)?.values)?;
        let path = resolve_remote_storage_target_local_path(
            &context
                .config
                .server
                .follower
                .remote_storage_target_local_root,
            &config.base_path,
        )?;
        std::fs::create_dir_all(&path).map_aster_err_ctx(
            &format!(
                "create remote storage target local path '{}'",
                path.display()
            ),
            AsterError::config_error,
        )?;
        let path = path.to_str().ok_or_else(|| {
            AsterError::config_error("remote storage target local path is not UTF-8")
        })?;
        Ok(Arc::new(LocalDriver::new(path)?))
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct S3ConfigV1 {
    endpoint: String,
    bucket: String,
    base_path: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct S3CredentialV1 {
    s3_access_key_id: String,
    s3_secret_access_key: String,
}

struct S3RemoteStorageTargetConnector;

#[async_trait]
impl RemoteStorageTargetConnector for S3RemoteStorageTargetConnector {
    fn descriptor(&self) -> RemoteStorageTargetConnectorDescriptor {
        RemoteStorageTargetConnectorDescriptor {
            connector_id: ConnectorId::declared(S3_CONNECTOR_ID),
            label_key: "remote_node_storage_target_connector_s3".to_string(),
            description_key: "remote_node_ingress_profile_s3_path_hint".to_string(),
            config_schema_version: SCHEMA_VERSION,
            credential_schema_version: Some(SCHEMA_VERSION),
            fields: vec![
                storage_connector_field(
                    "endpoint",
                    StorageConnectorFieldScope::ConnectorConfig,
                    StorageConnectorFieldKind::Text,
                    true,
                    false,
                ),
                storage_connector_field(
                    "bucket",
                    StorageConnectorFieldScope::ConnectorConfig,
                    StorageConnectorFieldKind::Text,
                    true,
                    false,
                ),
                storage_connector_field(
                    "base_path",
                    StorageConnectorFieldScope::ConnectorConfig,
                    StorageConnectorFieldKind::Text,
                    false,
                    false,
                ),
                storage_connector_field(
                    "s3_access_key_id",
                    StorageConnectorFieldScope::StaticCredential,
                    StorageConnectorFieldKind::Text,
                    true,
                    false,
                ),
                storage_connector_field(
                    "s3_secret_access_key",
                    StorageConnectorFieldScope::StaticCredential,
                    StorageConnectorFieldKind::Secret,
                    true,
                    true,
                ),
            ],
        }
    }

    fn legacy_driver_type(&self) -> Option<&'static str> {
        Some("s3")
    }

    fn normalize(
        &self,
        config: ConnectorConfigEnvelope,
        credential: Option<RemoteStorageTargetCredentialInput>,
        saved_credential: Option<String>,
    ) -> Result<NormalizedConnectorInput> {
        ensure_envelope(&config, S3_CONNECTOR_ID)?;
        let config_values: S3ConfigV1 = values(config.values)?;
        let endpoint =
            normalize_s3_endpoint_and_bucket(&config_values.endpoint, &config_values.bucket)
                .map_err(|error| error.into_aster_error())?;
        let credential_json = match credential {
            Some(input) => {
                if input.mode != "static" {
                    return Err(AsterError::validation_error(
                        "S3 remote target credential mode must be static",
                    ));
                }
                let value: S3CredentialV1 = values(input.values)?;
                Some(serialize_credential(S3CredentialV1 {
                    s3_access_key_id: normalize_required_storage_field(
                        "s3_access_key_id",
                        &value.s3_access_key_id,
                    )?,
                    s3_secret_access_key: normalize_required_storage_field(
                        "s3_secret_access_key",
                        &value.s3_secret_access_key,
                    )?,
                })?)
            }
            None => saved_credential,
        };
        if credential_json.is_none() {
            return Err(AsterError::validation_error(
                "S3 remote target requires static credentials",
            ));
        }
        Ok(NormalizedConnectorInput {
            config: envelope(
                S3_CONNECTOR_ID,
                S3ConfigV1 {
                    endpoint: endpoint.endpoint,
                    bucket: endpoint.bucket,
                    base_path: normalize_object_storage_prefix(&config_values.base_path),
                },
            )?,
            credential_json,
            credential_schema_version: Some(SCHEMA_VERSION),
        })
    }

    fn import_legacy_v050(
        &self,
        target: &remote_storage_target::Model,
    ) -> Result<ImportedLegacyRemoteStorageTarget> {
        if target.access_key.trim().is_empty() || target.secret_key.trim().is_empty() {
            return Err(AsterError::database_operation(format!(
                "legacy S3 remote storage target #{} has incomplete credentials",
                target.id
            )));
        }
        let endpoint = normalize_s3_endpoint_and_bucket(&target.endpoint, &target.bucket)
            .map_err(|error| error.into_aster_error())?;
        Ok(ImportedLegacyRemoteStorageTarget {
            config: envelope(
                S3_CONNECTOR_ID,
                S3ConfigV1 {
                    endpoint: endpoint.endpoint,
                    bucket: endpoint.bucket,
                    base_path: normalize_object_storage_prefix(&target.base_path),
                },
            )?,
            credential_json: Some(serialize_credential(S3CredentialV1 {
                s3_access_key_id: normalize_required_storage_field(
                    "s3_access_key_id",
                    &target.access_key,
                )?,
                s3_secret_access_key: normalize_required_storage_field(
                    "s3_secret_access_key",
                    &target.secret_key,
                )?,
            })?),
            credential_schema_version: Some(SCHEMA_VERSION),
        })
    }

    fn validate_persisted(
        &self,
        config: &ConnectorConfigEnvelope,
        credential: Option<&BTreeMap<String, Value>>,
    ) -> Result<()> {
        ensure_envelope(config, S3_CONNECTOR_ID)?;
        let config: S3ConfigV1 = values(config.values.clone())?;
        normalize_s3_endpoint_and_bucket(&config.endpoint, &config.bucket)
            .map_err(|error| error.into_aster_error())?;
        let credential = credential.ok_or_else(|| {
            AsterError::database_operation("S3 remote storage target is missing credentials")
        })?;
        let credential: S3CredentialV1 = values(credential.clone())?;
        normalize_required_storage_field("s3_access_key_id", &credential.s3_access_key_id)?;
        normalize_required_storage_field("s3_secret_access_key", &credential.s3_secret_access_key)?;
        Ok(())
    }

    async fn build_driver(
        &self,
        context: &RemoteStorageTargetConnectorContext<'_>,
        target: &remote_storage_target::Model,
    ) -> Result<Arc<dyn StorageDriver>> {
        let config: S3ConfigV1 = values(decode_config(target, S3_CONNECTOR_ID)?.values)?;
        let credential =
            load_credential_from_context(context, target, S3_CONNECTOR_ID, SCHEMA_VERSION)
                .await?
                .ok_or_else(|| {
                    AsterError::validation_error("S3 remote target credential is missing")
                })?;
        let credential: S3CredentialV1 = serde_json::from_str(&credential).map_aster_err_ctx(
            "invalid S3 remote target credential",
            AsterError::database_operation,
        )?;
        Ok(Arc::new(S3Driver::new(
            S3DriverConfig {
                endpoint: config.endpoint,
                bucket: config.bucket,
                base_path: config.base_path,
                region: "auto".to_string(),
                path_style: true,
                connect_timeout: Duration::from_secs(5),
                read_timeout: Duration::from_secs(30),
                operation_timeout: Duration::from_secs(3_600),
            },
            S3StaticCredentials {
                access_key: credential.s3_access_key_id,
                secret_key: credential.s3_secret_access_key,
            },
            S3DriverOptions::default(),
            std::convert::identity,
        )?))
    }
}

pub(crate) fn registered_remote_storage_target_connector_ids() -> Vec<String> {
    builtin_remote_storage_target_connector_registry()
        .map(|registry| {
            registry
                .ordered
                .iter()
                .map(|connector| connector.descriptor().connector_id.as_str().to_string())
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
pub(crate) fn list_registered_remote_storage_target_connector_descriptors()
-> Vec<RemoteStorageTargetConnectorDescriptor> {
    builtin_remote_storage_target_connector_registry()
        .map(|registry| registry.descriptors())
        .unwrap_or_default()
}

pub(crate) fn remote_storage_target_connector_descriptor(
    connector_id: &str,
) -> Result<RemoteStorageTargetConnectorDescriptor> {
    let connector_id = ConnectorId::declared(connector_id);
    Ok(builtin_remote_storage_target_connector_registry()?
        .require_input_connector(&connector_id)?
        .descriptor())
}

pub(in crate::services::remote::storage_target) fn normalize_connector_input(
    config: ConnectorConfigEnvelope,
    credential: Option<RemoteStorageTargetCredentialInput>,
    saved_credential: Option<String>,
) -> Result<NormalizedConnectorInput> {
    builtin_remote_storage_target_connector_registry()?
        .require_input_connector(&config.connector_id)?
        .normalize(config, credential, saved_credential)
}

pub(in crate::services::remote::storage_target) fn import_legacy_remote_storage_target(
    target: &remote_storage_target::Model,
) -> Result<ImportedLegacyRemoteStorageTarget> {
    builtin_remote_storage_target_connector_registry()?
        .require_legacy_connector(target)?
        .import_legacy_v050(target)
}

pub(in crate::services::remote::storage_target) fn validate_registered_persisted_connector(
    target: &remote_storage_target::Model,
    config: &ConnectorConfigEnvelope,
    credential: Option<(u32, &BTreeMap<String, Value>)>,
) -> Result<()> {
    let registry = builtin_remote_storage_target_connector_registry()?;
    let Some(connector) = registry.connector(&target.connector_id) else {
        return Ok(());
    };
    let descriptor = connector.descriptor();
    if config.schema_version != descriptor.config_schema_version {
        return Err(AsterError::database_operation(format!(
            "remote storage target #{} uses unsupported connector schema version {}",
            target.id, config.schema_version
        )));
    }
    match (descriptor.credential_schema_version, credential) {
        (None, Some(_)) => {
            return Err(AsterError::database_operation(format!(
                "remote storage target #{} unexpectedly has credentials",
                target.id
            )));
        }
        (Some(_), None) => {
            return Err(AsterError::database_operation(format!(
                "remote storage target #{} is missing credentials",
                target.id
            )));
        }
        (Some(expected), Some((actual, _))) if expected != actual => {
            return Err(AsterError::database_operation(format!(
                "remote storage target #{} uses unsupported credential schema version {}",
                target.id, actual
            )));
        }
        _ => {}
    }
    connector.validate_persisted(config, credential.map(|(_, values)| values))
}

pub(in crate::services::remote::storage_target) async fn validate_connector_from_target<
    S: FollowerRuntimeState,
>(
    state: &S,
    target: &remote_storage_target::Model,
) -> Result<()> {
    build_driver_from_target(state, target).await.map(|_| ())
}

pub(in crate::services::remote::storage_target) async fn build_driver_from_target<
    S: FollowerRuntimeState,
>(
    state: &S,
    target: &remote_storage_target::Model,
) -> Result<Arc<dyn StorageDriver>> {
    let connector = builtin_remote_storage_target_connector_registry()?.require_target(target)?;
    connector
        .build_driver(
            &RemoteStorageTargetConnectorContext {
                writer_db: state.writer_db(),
                config: state.config().as_ref(),
            },
            target,
        )
        .await
}

pub(crate) fn connector_available(connector_id: &str) -> bool {
    builtin_remote_storage_target_connector_registry()
        .is_ok_and(|registry| registry.connector(connector_id).is_some())
}

pub(in crate::services::remote::storage_target) async fn load_credential<
    S: FollowerRuntimeState,
>(
    state: &S,
    target: &remote_storage_target::Model,
    connector_id: &str,
) -> Result<Option<String>> {
    let connector = builtin_remote_storage_target_connector_registry()?
        .require_input_connector(&ConnectorId::declared(connector_id))?;
    let Some(schema_version) = connector.descriptor().credential_schema_version else {
        if remote_storage_target_credential_repo::find_by_target(state.writer_db(), target.id)
            .await?
            .is_some()
        {
            return Err(AsterError::database_operation(
                "remote target unexpectedly has a credential record",
            ));
        }
        return Ok(None);
    };
    load_credential_from_context(
        &RemoteStorageTargetConnectorContext {
            writer_db: state.writer_db(),
            config: state.config().as_ref(),
        },
        target,
        connector_id,
        schema_version,
    )
    .await
}

async fn load_credential_from_context(
    context: &RemoteStorageTargetConnectorContext<'_>,
    target: &remote_storage_target::Model,
    connector_id: &str,
    schema_version: u32,
) -> Result<Option<String>> {
    let Some(record) =
        remote_storage_target_credential_repo::find_by_target(context.writer_db, target.id).await?
    else {
        return Ok(None);
    };
    if record.connector_id != connector_id || record.schema_version != schema_version as i32 {
        return Err(AsterError::database_operation(
            "remote target credential record does not match connector schema",
        ));
    }
    credential::decrypt(
        &context.config.auth.storage_credential_secret_key,
        target.id,
        connector_id,
        schema_version,
        &record.ciphertext,
    )
    .map(Some)
}

fn decode_config(
    target: &remote_storage_target::Model,
    connector_id: &str,
) -> Result<ConnectorConfigEnvelope> {
    let config: ConnectorConfigEnvelope = serde_json::from_str(&target.connector_config)
        .map_aster_err_ctx(
            "invalid remote target connector config",
            AsterError::database_operation,
        )?;
    ensure_envelope(&config, connector_id)?;
    Ok(config)
}

fn ensure_envelope(config: &ConnectorConfigEnvelope, connector_id: &str) -> Result<()> {
    config
        .connector_id
        .validate()
        .map_err(|error| AsterError::validation_error(error.to_string()))?;
    if config.format_version != aster_drive_storage::CONNECTOR_CONFIG_FORMAT_VERSION
        || config.schema_version != SCHEMA_VERSION
        || config.connector_id.as_str() != connector_id
    {
        return Err(AsterError::validation_error(format!(
            "remote target connector config does not match '{connector_id}' schema v{SCHEMA_VERSION}"
        )));
    }
    Ok(())
}

fn values<T: for<'de> Deserialize<'de>>(values: BTreeMap<String, Value>) -> Result<T> {
    serde_json::from_value(Value::Object(values.into_iter().collect())).map_aster_err_ctx(
        "invalid remote target connector values",
        AsterError::validation_error,
    )
}

fn envelope<T: Serialize>(connector_id: &str, values: T) -> Result<ConnectorConfigEnvelope> {
    let values = serde_json::to_value(values).map_aster_err_ctx(
        "serialize remote target connector values",
        AsterError::internal_error,
    )?;
    let Value::Object(values) = values else {
        return Err(AsterError::internal_error(
            "remote target connector values must be an object",
        ));
    };
    Ok(ConnectorConfigEnvelope::new(
        ConnectorId::declared(connector_id),
        SCHEMA_VERSION,
        values.into_iter().collect(),
    ))
}

fn serialize_credential<T: Serialize>(credential: T) -> Result<String> {
    serde_json::to_string(&credential)
        .map_err(|error| AsterError::internal_error(error.to_string()))
}

fn unavailable_connector_error(connector_id: &str) -> AsterError {
    validation_error_with_code(
        ApiErrorCode::ManagedIngressDriverUnsupported,
        format!("remote storage target connector '{connector_id}' is unavailable"),
    )
}

#[cfg(test)]
mod registry_tests {
    use super::*;

    #[test]
    fn registry_rejects_duplicate_and_unknown_connector_ids() {
        let duplicate = RemoteStorageTargetConnectorRegistry::new(vec![
            Arc::new(LocalRemoteStorageTargetConnector),
            Arc::new(LocalRemoteStorageTargetConnector),
        ])
        .err()
        .expect("duplicate connector registration must fail");
        assert!(duplicate.message().contains("registered more than once"));

        let registry = builtin_remote_storage_target_connector_registry().unwrap();
        let unknown = registry
            .require_input_connector(&ConnectorId::declared("plugin.example.missing"))
            .err()
            .expect("unknown request connector must fail");
        assert_eq!(
            unknown.api_error_code_override(),
            Some(ApiErrorCode::ManagedIngressDriverUnsupported)
        );
    }
}
