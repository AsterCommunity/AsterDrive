use std::collections::BTreeMap;

use chrono::Utc;
use sea_orm::{DatabaseConnection, IntoActiveModel, NotSet};
use serde::Serialize;

use aster_drive_model::entities::storage_policy;
use aster_drive_model::types::{
    MicrosoftGraphCloud, ObjectStorageDownloadStrategy, ObjectStorageUploadStrategy,
    ProviderDownloadFilenameMode, ProviderDownloadStrategy, ProviderResumableUploadStrategy,
    RemoteDownloadStrategy, RemoteUploadStrategy, StoredStoragePolicyAllowedTypes,
    StoredStoragePolicyConfig,
};
use aster_drive_storage::{ConnectorConfigEnvelope, ConnectorId, StoragePolicyBehaviorConfig};

use super::local::{LocalConnector, LocalConnectorConfigV1};
use super::onedrive::{OneDriveAccountMode, OneDriveConnector, OneDriveConnectorConfigV1};
use super::remote::{RemoteConnector, RemoteConnectorConfigV1};
use super::s3::{S3Connector, S3ConnectorConfigV1};
use super::{StorageConnector, StorageConnectorConnectionInput, StorageConnectorCredentialInput};

/// Build the AsterDrive 0.5.x database shape used by connector tests.
///
/// Historical migrations intentionally retain legacy policy columns for the
/// startup credential importer. Issue #463 removes that compatibility schema in
/// 0.6.0; current entities safely ignore the extra columns.
pub(crate) async fn migrate_current_storage_test_schema(database: &DatabaseConnection) {
    aster_drive_migration::Migrator::up(database, None)
        .await
        .expect("test database migrations should succeed");
}

pub(crate) fn persisted_connector_config<T: Serialize>(
    connector_id: &'static str,
    schema_version: u32,
    values: T,
) -> ConnectorConfigEnvelope<serde_json::Value> {
    ConnectorConfigEnvelope::new(
        ConnectorId::declared(connector_id),
        schema_version,
        serde_json::to_value(values).expect("serialize typed test connector config"),
    )
}

pub(crate) fn connection_config<T: Serialize>(
    connector_id: &'static str,
    schema_version: u32,
    values: T,
) -> ConnectorConfigEnvelope<BTreeMap<String, serde_json::Value>> {
    let values = serde_json::to_value(values)
        .and_then(serde_json::from_value)
        .expect("serialize typed test connector connection config");
    ConnectorConfigEnvelope::new(ConnectorId::declared(connector_id), schema_version, values)
}

pub(crate) fn policy_config<T: Serialize>(
    connector_id: &'static str,
    schema_version: u32,
    connector_values: T,
    behavior: StoragePolicyBehaviorConfig,
) -> StoredStoragePolicyConfig {
    aster_drive_storage::encode_storage_policy_config(
        persisted_connector_config(connector_id, schema_version, connector_values),
        behavior,
    )
    .map(StoredStoragePolicyConfig)
    .expect("typed test storage policy config")
}

pub(crate) fn policy<T: Serialize>(
    connector_id: &'static str,
    schema_version: u32,
    connector_values: T,
    behavior: StoragePolicyBehaviorConfig,
) -> storage_policy::Model {
    let now = Utc::now();
    storage_policy::Model {
        id: 1,
        name: "test-policy".to_string(),
        connector_id: connector_id.to_string(),
        storage_config: policy_config(connector_id, schema_version, connector_values, behavior),
        max_file_size: 0,
        allowed_types: StoredStoragePolicyAllowedTypes::empty(),
        is_default: false,
        chunk_size: 0,
        created_at: now,
        updated_at: now,
    }
}

pub(crate) fn insertable_policy(policy: storage_policy::Model) -> storage_policy::ActiveModel {
    let mut active = policy.into_active_model();
    active.id = NotSet;
    active
}

pub(crate) fn local_policy(base_path: impl Into<String>) -> storage_policy::Model {
    local_policy_with_behavior(base_path, StoragePolicyBehaviorConfig::default())
}

pub(crate) fn local_policy_with_behavior(
    base_path: impl Into<String>,
    behavior: StoragePolicyBehaviorConfig,
) -> storage_policy::Model {
    policy(
        LocalConnector::ID,
        1,
        LocalConnectorConfigV1 {
            base_path: base_path.into(),
            content_dedup: false,
        },
        behavior,
    )
}

pub(crate) fn local_base_path(policy: &storage_policy::Model) -> String {
    LocalConnector
        .local_filesystem_projection(policy)
        .expect("decode typed local policy config")
        .expect("local connector must expose a filesystem projection")
        .base_path
}

pub(crate) fn with_local_content_dedup(
    policy: &storage_policy::Model,
    content_dedup: bool,
) -> storage_policy::Model {
    let (_, behavior) =
        aster_drive_storage::decode_storage_policy_config::<LocalConnectorConfigV1>(
            policy.storage_config.as_ref(),
            &ConnectorId::declared(LocalConnector::ID),
            1,
        )
        .expect("decode typed test local policy");
    let mut updated = policy.clone();
    updated.storage_config = policy_config(
        LocalConnector::ID,
        1,
        LocalConnectorConfigV1 {
            base_path: local_base_path(policy),
            content_dedup,
        },
        behavior,
    );
    updated
}

pub(crate) fn s3_policy(
    endpoint: impl Into<String>,
    bucket: impl Into<String>,
    base_path: impl Into<String>,
    upload_strategy: ObjectStorageUploadStrategy,
    download_strategy: ObjectStorageDownloadStrategy,
) -> storage_policy::Model {
    policy(
        S3Connector::ID,
        1,
        S3ConnectorConfigV1 {
            endpoint: endpoint.into(),
            bucket: bucket.into(),
            base_path: base_path.into(),
            object_storage_upload_strategy: upload_strategy,
            object_storage_download_strategy: download_strategy,
            s3_path_style: true,
            s3_region: "auto".to_string(),
            s3_connect_timeout_secs: 5,
            s3_read_timeout_secs: 30,
            s3_operation_timeout_secs: 3_600,
        },
        StoragePolicyBehaviorConfig::default(),
    )
}

pub(crate) fn onedrive_policy(
    account_mode: OneDriveAccountMode,
    drive_id: Option<String>,
    site_id: Option<String>,
    group_id: Option<String>,
    behavior: StoragePolicyBehaviorConfig,
) -> storage_policy::Model {
    onedrive_policy_with_download(
        account_mode,
        drive_id,
        site_id,
        group_id,
        ProviderDownloadStrategy::ServerRelay,
        ProviderDownloadFilenameMode::ProviderNative,
        behavior,
    )
}

pub(crate) fn onedrive_policy_with_download(
    account_mode: OneDriveAccountMode,
    drive_id: Option<String>,
    site_id: Option<String>,
    group_id: Option<String>,
    download_strategy: ProviderDownloadStrategy,
    download_filename_mode: ProviderDownloadFilenameMode,
    behavior: StoragePolicyBehaviorConfig,
) -> storage_policy::Model {
    policy(
        OneDriveConnector::ID,
        1,
        OneDriveConnectorConfigV1 {
            base_path: String::new(),
            provider_resumable_upload_strategy: ProviderResumableUploadStrategy::ServerRelay,
            provider_download_strategy: download_strategy,
            provider_download_filename_mode: download_filename_mode,
            cloud: MicrosoftGraphCloud::Global,
            account_mode,
            tenant: None,
            drive_id,
            root_item_id: None,
            site_id,
            group_id,
        },
        behavior,
    )
}

pub(crate) fn remote_policy(
    base_path: impl Into<String>,
    remote_node_id: Option<i64>,
    download_strategy: RemoteDownloadStrategy,
    upload_strategy: RemoteUploadStrategy,
) -> storage_policy::Model {
    policy(
        RemoteConnector::ID,
        1,
        RemoteConnectorConfigV1 {
            base_path: base_path.into(),
            remote_node_id,
            remote_storage_target_key: None,
            remote_download_strategy: download_strategy,
            remote_upload_strategy: upload_strategy,
        },
        StoragePolicyBehaviorConfig::default(),
    )
}

pub(crate) fn local_connection(base_path: impl Into<String>) -> StorageConnectorConnectionInput {
    StorageConnectorConnectionInput {
        connector_config: connection_config(
            LocalConnector::ID,
            1,
            LocalConnectorConfigV1 {
                base_path: base_path.into(),
                content_dedup: false,
            },
        ),
        behavior: StoragePolicyBehaviorConfig::default(),
        credential: StorageConnectorCredentialInput::None,
    }
}

pub(crate) fn remote_connection(
    base_path: impl Into<String>,
    remote_node_id: Option<i64>,
    remote_storage_target_key: Option<String>,
) -> StorageConnectorConnectionInput {
    StorageConnectorConnectionInput {
        connector_config: connection_config(
            RemoteConnector::ID,
            1,
            RemoteConnectorConfigV1 {
                base_path: base_path.into(),
                remote_node_id,
                remote_storage_target_key,
                remote_download_strategy: RemoteDownloadStrategy::RelayStream,
                remote_upload_strategy: RemoteUploadStrategy::RelayStream,
            },
        ),
        behavior: StoragePolicyBehaviorConfig::default(),
        credential: StorageConnectorCredentialInput::None,
    }
}
