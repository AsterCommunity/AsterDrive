//! Add a plugin-safe storage configuration envelope to storage policies.
//!
//! This migration intentionally freezes the legacy-to-envelope mapping instead
//! of calling runtime connector code. Historical migrations must keep producing
//! the same bytes when connector defaults or schemas evolve later.

use std::collections::HashSet;

use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::{ConnectionTrait, DbBackend, TransactionTrait};
use serde::Serialize;
#[cfg(test)]
use serde_json::json;
use serde_json::{Map, Value as JsonValue};

const STORAGE_CONFIG_FORMAT_VERSION: u32 = 1;
const CONNECTOR_CONFIG_FORMAT_VERSION: u32 = 1;
const CONNECTOR_CONFIG_SCHEMA_VERSION: u32 = 1;
const BEHAVIOR_CONFIG_FORMAT_VERSION: u32 = 1;
const BEHAVIOR_CONFIG_SCHEMA_VERSION: u32 = 1;
const UNCONFIGURED_CONNECTOR_ID: &str = "asterdrive.storage.unconfigured";

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        add_columns(manager).await?;
        backfill_config_envelopes(manager).await?;
        enforce_mysql_not_null(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for column in [StoragePolicies::StorageConfig, StoragePolicies::ConnectorId] {
            if manager
                .has_column(StoragePolicies::Table.to_string(), column.to_string())
                .await?
            {
                manager
                    .alter_table(
                        Table::alter()
                            .table(StoragePolicies::Table)
                            .drop_column(column)
                            .to_owned(),
                    )
                    .await?;
            }
        }
        Ok(())
    }
}

async fn add_columns(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let backend = manager.get_database_backend();
    let empty_storage_config = serialize_storage_config(
        0,
        UNCONFIGURED_CONNECTOR_ID,
        FrozenUnconfiguredConfigV1 {},
        FrozenBehaviorConfigV1::default(),
    )?;

    if !manager
        .has_column(
            StoragePolicies::Table.to_string(),
            StoragePolicies::ConnectorId.to_string(),
        )
        .await?
    {
        manager
            .alter_table(
                Table::alter()
                    .table(StoragePolicies::Table)
                    .add_column(
                        ColumnDef::new(StoragePolicies::ConnectorId)
                            .string_len(128)
                            .not_null()
                            .default(UNCONFIGURED_CONNECTOR_ID),
                    )
                    .to_owned(),
            )
            .await?;
    }

    for (column, default_json) in [(
        StoragePolicies::StorageConfig,
        empty_storage_config.as_str(),
    )] {
        if manager
            .has_column(StoragePolicies::Table.to_string(), column.to_string())
            .await?
        {
            continue;
        }

        let mut definition = ColumnDef::new(column);
        definition.text();
        if backend == DbBackend::MySql {
            definition.null();
        } else {
            definition.not_null().default(default_json);
        }
        manager
            .alter_table(
                Table::alter()
                    .table(StoragePolicies::Table)
                    .add_column(&mut definition)
                    .to_owned(),
            )
            .await?;
    }

    Ok(())
}

async fn backfill_config_envelopes(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let connection = manager.get_connection();
    let mut select = Query::select();
    select
        .columns([
            StoragePolicies::Id,
            StoragePolicies::DriverType,
            StoragePolicies::Endpoint,
            StoragePolicies::Bucket,
            StoragePolicies::BasePath,
            StoragePolicies::RemoteNodeId,
            StoragePolicies::RemoteStorageTargetKey,
            StoragePolicies::Options,
        ])
        .from(StoragePolicies::Table)
        .order_by(StoragePolicies::Id, Order::Asc);
    let rows = connection.query_all(&select).await?;

    // Convert every row before opening the write transaction. A malformed row
    // therefore blocks the whole backfill without partially updating policies.
    let mut updates = Vec::with_capacity(rows.len());
    for row in rows {
        let legacy = LegacyStoragePolicy {
            id: row.try_get_by_index(0)?,
            driver_type: row.try_get_by_index(1)?,
            endpoint: row.try_get_by_index(2)?,
            bucket: row.try_get_by_index(3)?,
            base_path: row.try_get_by_index(4)?,
            remote_node_id: row.try_get_by_index(5)?,
            remote_storage_target_key: row.try_get_by_index(6)?,
            options: row.try_get_by_index(7)?,
        };
        updates.push(convert_legacy_policy(legacy)?);
    }

    let transaction = connection.begin().await?;
    for update in updates {
        let mut statement = Query::update();
        statement
            .table(StoragePolicies::Table)
            .values([
                (
                    StoragePolicies::ConnectorId,
                    update.connector_id.clone().into(),
                ),
                (StoragePolicies::StorageConfig, update.storage_config.into()),
            ])
            .and_where(Expr::col(StoragePolicies::Id).eq(update.id));
        transaction.execute(&statement).await?;
    }
    transaction.commit().await
}

async fn enforce_mysql_not_null(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    if manager.get_database_backend() != DbBackend::MySql {
        return Ok(());
    }

    for column in [StoragePolicies::StorageConfig] {
        manager
            .alter_table(
                Table::alter()
                    .table(StoragePolicies::Table)
                    .modify_column(ColumnDef::new(column).text().not_null())
                    .to_owned(),
            )
            .await?;
    }
    Ok(())
}

#[derive(Debug)]
struct LegacyStoragePolicy {
    id: i64,
    driver_type: String,
    endpoint: String,
    bucket: String,
    base_path: String,
    remote_node_id: Option<i64>,
    remote_storage_target_key: Option<String>,
    options: String,
}

#[derive(Debug)]
struct ConfigBackfill {
    id: i64,
    connector_id: String,
    storage_config: String,
}

#[derive(Serialize)]
struct FrozenStorageConfigEnvelopeV1<T> {
    format_version: u32,
    connector: FrozenConnectorConfigEnvelopeV1<T>,
    behavior: FrozenBehaviorConfigEnvelopeV1,
}

#[derive(Serialize)]
struct FrozenConnectorConfigEnvelopeV1<T> {
    format_version: u32,
    connector_id: &'static str,
    schema_version: u32,
    values: T,
}

#[derive(Serialize)]
struct FrozenBehaviorConfigEnvelopeV1 {
    format_version: u32,
    schema_version: u32,
    values: FrozenBehaviorConfigV1,
}

#[derive(Debug, Default, Serialize)]
struct FrozenBehaviorConfigV1 {
    storage_native_thumbnail_enabled: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    storage_native_thumbnail_extensions: Vec<String>,
    storage_native_media_metadata_enabled: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    storage_native_media_metadata_extensions: Vec<String>,
}

#[derive(Serialize)]
struct FrozenUnconfiguredConfigV1 {}

#[derive(Serialize)]
struct FrozenLocalConfigV1 {
    base_path: String,
    content_dedup: bool,
}

#[derive(Serialize)]
struct FrozenObjectStorageConfigV1 {
    endpoint: String,
    bucket: String,
    base_path: String,
    object_storage_upload_strategy: String,
    object_storage_download_strategy: String,
}

#[derive(Serialize)]
struct FrozenS3ConfigV1 {
    endpoint: String,
    bucket: String,
    base_path: String,
    object_storage_upload_strategy: String,
    object_storage_download_strategy: String,
    s3_path_style: bool,
    s3_region: String,
    s3_connect_timeout_secs: u64,
    s3_read_timeout_secs: u64,
    s3_operation_timeout_secs: u64,
}

#[derive(Serialize)]
struct FrozenSftpConfigV1 {
    endpoint: String,
    base_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    sftp_host_key_fingerprint: Option<String>,
}

#[derive(Serialize)]
struct FrozenRemoteConfigV1 {
    base_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    remote_node_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    remote_storage_target_key: Option<String>,
    remote_download_strategy: String,
    remote_upload_strategy: String,
}

#[derive(Serialize)]
struct FrozenOneDriveConfigV1 {
    base_path: String,
    provider_resumable_upload_strategy: String,
    provider_download_strategy: String,
    provider_download_filename_mode: String,
    cloud: String,
    account_mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tenant: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    drive_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    root_item_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    site_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    group_id: Option<String>,
}

#[derive(Serialize)]
#[serde(untagged)]
enum FrozenConnectorConfigV1 {
    Local(FrozenLocalConfigV1),
    S3(FrozenS3ConfigV1),
    Sftp(FrozenSftpConfigV1),
    AzureBlob(FrozenObjectStorageConfigV1),
    TencentCos(FrozenObjectStorageConfigV1),
    Remote(FrozenRemoteConfigV1),
    OneDrive(FrozenOneDriveConfigV1),
}

impl FrozenConnectorConfigV1 {
    fn connector_id(&self) -> &'static str {
        match self {
            Self::Local(_) => "asterdrive.storage.local",
            Self::S3(_) => "asterdrive.storage.s3",
            Self::Sftp(_) => "asterdrive.storage.sftp",
            Self::AzureBlob(_) => "asterdrive.storage.azure_blob",
            Self::TencentCos(_) => "asterdrive.storage.tencent_cos",
            Self::Remote(_) => "asterdrive.storage.remote",
            Self::OneDrive(_) => "asterdrive.storage.onedrive",
        }
    }
}

fn serialize_storage_config<T: Serialize>(
    policy_id: i64,
    connector_id: &'static str,
    connector: T,
    behavior: FrozenBehaviorConfigV1,
) -> Result<String, DbErr> {
    serde_json::to_string(&FrozenStorageConfigEnvelopeV1 {
        format_version: STORAGE_CONFIG_FORMAT_VERSION,
        connector: FrozenConnectorConfigEnvelopeV1 {
            format_version: CONNECTOR_CONFIG_FORMAT_VERSION,
            connector_id,
            schema_version: CONNECTOR_CONFIG_SCHEMA_VERSION,
            values: connector,
        },
        behavior: FrozenBehaviorConfigEnvelopeV1 {
            format_version: BEHAVIOR_CONFIG_FORMAT_VERSION,
            schema_version: BEHAVIOR_CONFIG_SCHEMA_VERSION,
            values: behavior,
        },
    })
    .map_err(|error| migration_error(policy_id, format!("serialize storage config: {error}")))
}

fn convert_legacy_policy(policy: LegacyStoragePolicy) -> Result<ConfigBackfill, DbErr> {
    let mut options = parse_legacy_options(policy.id, &policy.options)?;
    let behavior = take_behavior_values(policy.id, &mut options)?;
    let connector = connector_values(policy.id, &policy, &mut options)?;

    if let Some(field) = options.keys().next() {
        return Err(migration_error(
            policy.id,
            format!(
                "legacy option '{field}' is not owned by driver '{}'",
                policy.driver_type
            ),
        ));
    }

    let connector_id = connector.connector_id();
    let storage_config = serialize_storage_config(policy.id, connector_id, connector, behavior)?;

    Ok(ConfigBackfill {
        id: policy.id,
        connector_id: connector_id.to_string(),
        storage_config,
    })
}

fn parse_legacy_options(policy_id: i64, raw: &str) -> Result<Map<String, JsonValue>, DbErr> {
    if raw.trim().is_empty() {
        return Ok(Map::new());
    }
    let value: JsonValue = serde_json::from_str(raw).map_err(|error| {
        migration_error(
            policy_id,
            format!("legacy options contain invalid JSON: {error}"),
        )
    })?;
    let JsonValue::Object(mut options) = value else {
        return Err(migration_error(
            policy_id,
            "legacy options must be a JSON object",
        ));
    };

    for (alias, canonical) in [
        ("s3_upload_strategy", "object_storage_upload_strategy"),
        ("s3_download_strategy", "object_storage_download_strategy"),
    ] {
        if let Some(value) = options.remove(alias) {
            if options.contains_key(canonical) {
                return Err(migration_error(
                    policy_id,
                    format!("legacy options contain both '{canonical}' and alias '{alias}'"),
                ));
            }
            options.insert(canonical.to_string(), value);
        }
    }

    let known = known_legacy_option_names();
    if let Some(field) = options.keys().find(|field| !known.contains(field.as_str())) {
        return Err(migration_error(
            policy_id,
            format!("legacy options contain unknown field '{field}'"),
        ));
    }
    options.retain(|_, value| !matches!(value, JsonValue::Null));
    Ok(options)
}

fn known_legacy_option_names() -> HashSet<&'static str> {
    [
        "object_storage_upload_strategy",
        "object_storage_download_strategy",
        "s3_path_style",
        "s3_region",
        "remote_download_strategy",
        "remote_upload_strategy",
        "provider_resumable_upload_strategy",
        "provider_download_strategy",
        "provider_download_filename_mode",
        "thumbnail_processor",
        "thumbnail_extensions",
        "content_dedup",
        "storage_native_processing_enabled",
        "storage_native_media_metadata_enabled",
        "media_metadata_extensions",
        "s3_connect_timeout_secs",
        "s3_read_timeout_secs",
        "s3_operation_timeout_secs",
        "onedrive_cloud",
        "onedrive_account_mode",
        "onedrive_tenant",
        "onedrive_drive_id",
        "onedrive_root_item_id",
        "onedrive_site_id",
        "onedrive_group_id",
        "sftp_host_key_fingerprint",
    ]
    .into_iter()
    .collect()
}

fn take_behavior_values(
    policy_id: i64,
    options: &mut Map<String, JsonValue>,
) -> Result<FrozenBehaviorConfigV1, DbErr> {
    let thumbnail_processor = take_string_enum(
        policy_id,
        options,
        "thumbnail_processor",
        &[
            "images",
            "lofty",
            "vips_cli",
            "ffmpeg_cli",
            "ffprobe_cli",
            "storage_native",
        ],
    )?;
    let storage_native_thumbnail_extensions =
        take_extension_list(policy_id, options, "thumbnail_extensions")?.unwrap_or_default();
    let storage_native_media_metadata_extensions =
        take_extension_list(policy_id, options, "media_metadata_extensions")?.unwrap_or_default();
    Ok(FrozenBehaviorConfigV1 {
        storage_native_thumbnail_enabled: thumbnail_processor.as_deref() == Some("storage_native"),
        storage_native_thumbnail_extensions,
        storage_native_media_metadata_enabled: !storage_native_media_metadata_extensions.is_empty(),
        storage_native_media_metadata_extensions,
    })
}

fn connector_values(
    policy_id: i64,
    policy: &LegacyStoragePolicy,
    options: &mut Map<String, JsonValue>,
) -> Result<FrozenConnectorConfigV1, DbErr> {
    match policy.driver_type.as_str() {
        "local" => local_values(policy_id, policy, options),
        "s3" => s3_values(policy_id, policy, options),
        "sftp" => sftp_values(policy_id, policy, options),
        "azure_blob" => object_storage_values(policy_id, policy, options, false),
        "tencent_cos" => object_storage_values(policy_id, policy, options, true),
        "remote" => remote_values(policy_id, policy, options),
        "onedrive" => onedrive_values(policy_id, policy, options),
        driver => Err(migration_error(
            policy_id,
            format!("unknown legacy storage driver '{driver}'"),
        )),
    }
}

fn local_values(
    policy_id: i64,
    policy: &LegacyStoragePolicy,
    options: &mut Map<String, JsonValue>,
) -> Result<FrozenConnectorConfigV1, DbErr> {
    Ok(FrozenConnectorConfigV1::Local(FrozenLocalConfigV1 {
        base_path: policy.base_path.clone(),
        content_dedup: take_bool(policy_id, options, "content_dedup")?.unwrap_or(false),
    }))
}

fn s3_values(
    policy_id: i64,
    policy: &LegacyStoragePolicy,
    options: &mut Map<String, JsonValue>,
) -> Result<FrozenConnectorConfigV1, DbErr> {
    let transfer = object_storage_transfer_values(policy_id, policy, options)?;
    Ok(FrozenConnectorConfigV1::S3(FrozenS3ConfigV1 {
        endpoint: transfer.endpoint,
        bucket: transfer.bucket,
        base_path: transfer.base_path,
        object_storage_upload_strategy: transfer.object_storage_upload_strategy,
        object_storage_download_strategy: transfer.object_storage_download_strategy,
        s3_path_style: take_bool(policy_id, options, "s3_path_style")?.unwrap_or(true),
        s3_region: take_trimmed_string(policy_id, options, "s3_region")?
            .unwrap_or_else(|| "auto".to_string()),
        s3_connect_timeout_secs: effective_timeout(
            policy_id,
            options,
            "s3_connect_timeout_secs",
            5,
        )?,
        s3_read_timeout_secs: effective_timeout(policy_id, options, "s3_read_timeout_secs", 30)?,
        s3_operation_timeout_secs: effective_timeout(
            policy_id,
            options,
            "s3_operation_timeout_secs",
            3_600,
        )?,
    }))
}

fn sftp_values(
    policy_id: i64,
    policy: &LegacyStoragePolicy,
    options: &mut Map<String, JsonValue>,
) -> Result<FrozenConnectorConfigV1, DbErr> {
    Ok(FrozenConnectorConfigV1::Sftp(FrozenSftpConfigV1 {
        endpoint: policy.endpoint.clone(),
        base_path: policy.base_path.clone(),
        sftp_host_key_fingerprint: take_trimmed_string(
            policy_id,
            options,
            "sftp_host_key_fingerprint",
        )?,
    }))
}

fn object_storage_values(
    policy_id: i64,
    policy: &LegacyStoragePolicy,
    options: &mut Map<String, JsonValue>,
    supports_native_processing: bool,
) -> Result<FrozenConnectorConfigV1, DbErr> {
    let transfer = object_storage_transfer_values(policy_id, policy, options)?;
    if supports_native_processing {
        // These duplicate connector switches never controlled the old runtime
        // behavior. Validate and consume them, but derive the sole final state
        // from the legacy core behavior fields in `take_behavior_values`.
        take_bool(policy_id, options, "storage_native_processing_enabled")?;
        take_bool(policy_id, options, "storage_native_media_metadata_enabled")?;
        return Ok(FrozenConnectorConfigV1::TencentCos(transfer));
    }
    Ok(FrozenConnectorConfigV1::AzureBlob(transfer))
}

fn object_storage_transfer_values(
    policy_id: i64,
    policy: &LegacyStoragePolicy,
    options: &mut Map<String, JsonValue>,
) -> Result<FrozenObjectStorageConfigV1, DbErr> {
    Ok(FrozenObjectStorageConfigV1 {
        endpoint: policy.endpoint.clone(),
        bucket: policy.bucket.clone(),
        base_path: policy.base_path.clone(),
        object_storage_upload_strategy: take_string_enum(
            policy_id,
            options,
            "object_storage_upload_strategy",
            &["relay_stream", "presigned"],
        )?
        .unwrap_or_else(|| "relay_stream".to_string()),
        object_storage_download_strategy: take_string_enum(
            policy_id,
            options,
            "object_storage_download_strategy",
            &["relay_stream", "presigned"],
        )?
        .unwrap_or_else(|| "relay_stream".to_string()),
    })
}

fn remote_values(
    policy_id: i64,
    policy: &LegacyStoragePolicy,
    options: &mut Map<String, JsonValue>,
) -> Result<FrozenConnectorConfigV1, DbErr> {
    let remote_storage_target_key = policy
        .remote_storage_target_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    Ok(FrozenConnectorConfigV1::Remote(FrozenRemoteConfigV1 {
        base_path: policy.base_path.clone(),
        remote_node_id: policy.remote_node_id,
        remote_storage_target_key,
        remote_download_strategy: take_string_enum(
            policy_id,
            options,
            "remote_download_strategy",
            &["relay_stream", "presigned"],
        )?
        .unwrap_or_else(|| "relay_stream".to_string()),
        remote_upload_strategy: take_string_enum(
            policy_id,
            options,
            "remote_upload_strategy",
            &["relay_stream", "presigned"],
        )?
        .unwrap_or_else(|| "relay_stream".to_string()),
    }))
}

fn onedrive_values(
    policy_id: i64,
    policy: &LegacyStoragePolicy,
    options: &mut Map<String, JsonValue>,
) -> Result<FrozenConnectorConfigV1, DbErr> {
    Ok(FrozenConnectorConfigV1::OneDrive(FrozenOneDriveConfigV1 {
        base_path: policy.base_path.clone(),
        provider_resumable_upload_strategy: take_string_enum(
            policy_id,
            options,
            "provider_resumable_upload_strategy",
            &["server_relay", "frontend_direct"],
        )?
        .unwrap_or_else(|| "server_relay".to_string()),
        provider_download_strategy: take_string_enum(
            policy_id,
            options,
            "provider_download_strategy",
            &["server_relay", "frontend_direct"],
        )?
        .unwrap_or_else(|| "server_relay".to_string()),
        provider_download_filename_mode: take_string_enum(
            policy_id,
            options,
            "provider_download_filename_mode",
            &["provider_native", "strict_current"],
        )?
        .unwrap_or_else(|| "provider_native".to_string()),
        cloud: take_string_enum(policy_id, options, "onedrive_cloud", &["global", "china"])?
            .unwrap_or_else(|| "global".to_string()),
        account_mode: take_string_enum(
            policy_id,
            options,
            "onedrive_account_mode",
            &[
                "personal",
                "work_or_school",
                "sharepoint_site",
                "group_drive",
            ],
        )?
        .unwrap_or_else(|| "personal".to_string()),
        tenant: take_trimmed_string(policy_id, options, "onedrive_tenant")?,
        drive_id: take_trimmed_string(policy_id, options, "onedrive_drive_id")?,
        root_item_id: take_trimmed_string(policy_id, options, "onedrive_root_item_id")?,
        site_id: take_trimmed_string(policy_id, options, "onedrive_site_id")?,
        group_id: take_trimmed_string(policy_id, options, "onedrive_group_id")?,
    }))
}

fn effective_timeout(
    policy_id: i64,
    options: &mut Map<String, JsonValue>,
    field: &str,
    default: u64,
) -> Result<u64, DbErr> {
    Ok(take_u64(policy_id, options, field)?
        .filter(|value| *value > 0)
        .unwrap_or(default))
}

fn take_bool(
    policy_id: i64,
    options: &mut Map<String, JsonValue>,
    field: &str,
) -> Result<Option<bool>, DbErr> {
    let Some(value) = options.remove(field) else {
        return Ok(None);
    };
    value.as_bool().map(Some).ok_or_else(|| {
        migration_error(
            policy_id,
            format!("legacy option '{field}' must be boolean"),
        )
    })
}

fn take_u64(
    policy_id: i64,
    options: &mut Map<String, JsonValue>,
    field: &str,
) -> Result<Option<u64>, DbErr> {
    let Some(value) = options.remove(field) else {
        return Ok(None);
    };
    value.as_u64().map(Some).ok_or_else(|| {
        migration_error(
            policy_id,
            format!("legacy option '{field}' must be a non-negative integer"),
        )
    })
}

fn take_trimmed_string(
    policy_id: i64,
    options: &mut Map<String, JsonValue>,
    field: &str,
) -> Result<Option<String>, DbErr> {
    let Some(value) = options.remove(field) else {
        return Ok(None);
    };
    let Some(value) = value.as_str() else {
        return Err(migration_error(
            policy_id,
            format!("legacy option '{field}' must be a string"),
        ));
    };
    let value = value.trim();
    Ok((!value.is_empty()).then(|| value.to_string()))
}

fn take_string_enum(
    policy_id: i64,
    options: &mut Map<String, JsonValue>,
    field: &str,
    allowed: &[&str],
) -> Result<Option<String>, DbErr> {
    let Some(value) = take_trimmed_string(policy_id, options, field)? else {
        return Ok(None);
    };
    if allowed.contains(&value.as_str()) {
        Ok(Some(value))
    } else {
        Err(migration_error(
            policy_id,
            format!("legacy option '{field}' has unsupported value '{value}'"),
        ))
    }
}

fn take_extension_list(
    policy_id: i64,
    options: &mut Map<String, JsonValue>,
    field: &str,
) -> Result<Option<Vec<String>>, DbErr> {
    let Some(value) = options.remove(field) else {
        return Ok(None);
    };
    let JsonValue::Array(values) = value else {
        return Err(migration_error(
            policy_id,
            format!("legacy option '{field}' must be an array"),
        ));
    };
    let mut normalized = Vec::new();
    for value in values {
        let Some(value) = value.as_str() else {
            return Err(migration_error(
                policy_id,
                format!("legacy option '{field}' must contain only strings"),
            ));
        };
        let value = value.trim().trim_start_matches('.').to_ascii_lowercase();
        if !value.is_empty() && !normalized.contains(&value) {
            normalized.push(value);
        }
    }
    Ok(Some(normalized))
}

fn migration_error(policy_id: i64, message: impl Into<String>) -> DbErr {
    DbErr::Migration(format!(
        "storage policy {policy_id} connector config backfill failed: {}",
        message.into()
    ))
}

#[derive(DeriveIden, Clone, Copy)]
enum StoragePolicies {
    Table,
    Id,
    DriverType,
    Endpoint,
    Bucket,
    BasePath,
    RemoteNodeId,
    RemoteStorageTargetKey,
    Options,
    ConnectorId,
    StorageConfig,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(driver_type: &str, options: JsonValue) -> LegacyStoragePolicy {
        LegacyStoragePolicy {
            id: 7,
            driver_type: driver_type.to_string(),
            endpoint: "https://storage.example.test".to_string(),
            bucket: "bucket".to_string(),
            base_path: "tenant/root".to_string(),
            remote_node_id: Some(42),
            remote_storage_target_key: Some(" rst_hot ".to_string()),
            options: serde_json::to_string(&options).unwrap(),
        }
    }

    #[test]
    fn maps_every_builtin_driver_to_stable_connector_id() {
        for (driver, connector) in [
            ("local", "asterdrive.storage.local"),
            ("s3", "asterdrive.storage.s3"),
            ("sftp", "asterdrive.storage.sftp"),
            ("azure_blob", "asterdrive.storage.azure_blob"),
            ("tencent_cos", "asterdrive.storage.tencent_cos"),
            ("remote", "asterdrive.storage.remote"),
            ("onedrive", "asterdrive.storage.onedrive"),
        ] {
            let converted = convert_legacy_policy(policy(driver, json!({}))).unwrap();
            assert_eq!(converted.connector_id, connector);
            let envelope: JsonValue = serde_json::from_str(&converted.storage_config).unwrap();
            assert_eq!(envelope["format_version"], 1);
            assert_eq!(envelope["connector"]["schema_version"], 1);
            assert_eq!(envelope["connector"]["connector_id"], connector);
            assert_eq!(envelope["behavior"]["schema_version"], 1);
            assert_eq!(
                envelope["behavior"]["values"],
                json!({
                    "storage_native_thumbnail_enabled": false,
                    "storage_native_media_metadata_enabled": false
                })
            );
        }
    }

    #[test]
    fn s3_aliases_defaults_false_and_zero_preserve_effective_behavior() {
        let converted = convert_legacy_policy(policy(
            "s3",
            json!({
                "s3_upload_strategy": "presigned",
                "s3_download_strategy": "relay_stream",
                "s3_path_style": false,
                "s3_connect_timeout_secs": 0,
                "thumbnail_processor": "storage_native",
                "thumbnail_extensions": [" .JPG ", "jpg", "WEBP"],
                "media_metadata_extensions": []
            }),
        ))
        .unwrap();
        let storage: JsonValue = serde_json::from_str(&converted.storage_config).unwrap();
        let connector = &storage["connector"];
        let behavior = &storage["behavior"];

        assert_eq!(
            connector["values"]["object_storage_upload_strategy"],
            "presigned"
        );
        assert_eq!(connector["values"]["s3_path_style"], false);
        assert_eq!(connector["values"]["s3_connect_timeout_secs"], 5);
        assert_eq!(behavior["schema_version"], 1);
        assert_eq!(behavior["values"]["storage_native_thumbnail_enabled"], true);
        assert_eq!(
            behavior["values"]["storage_native_thumbnail_extensions"],
            json!(["jpg", "webp"])
        );
        assert_eq!(
            behavior["values"]["storage_native_media_metadata_enabled"],
            false
        );
    }

    #[test]
    fn tencent_cos_uses_final_v1_behavior_without_duplicate_connector_switches() {
        let converted = convert_legacy_policy(policy(
            "tencent_cos",
            json!({
                "thumbnail_processor": "images",
                "thumbnail_extensions": ["jpg"],
                "media_metadata_extensions": ["mp4"],
                "storage_native_processing_enabled": true,
                "storage_native_media_metadata_enabled": false
            }),
        ))
        .unwrap();
        let storage: JsonValue = serde_json::from_str(&converted.storage_config).unwrap();

        assert_eq!(storage["connector"]["schema_version"], 1);
        assert!(
            storage["connector"]["values"]
                .get("storage_native_processing_enabled")
                .is_none()
        );
        assert!(
            storage["connector"]["values"]
                .get("storage_native_media_metadata_enabled")
                .is_none()
        );
        assert_eq!(
            storage["behavior"]["values"],
            json!({
                "storage_native_thumbnail_enabled": false,
                "storage_native_thumbnail_extensions": ["jpg"],
                "storage_native_media_metadata_enabled": true,
                "storage_native_media_metadata_extensions": ["mp4"]
            })
        );
    }

    #[test]
    fn secrets_never_enter_connector_config() {
        let mut legacy = policy(
            "sftp",
            json!({"sftp_host_key_fingerprint": " SHA256:test "}),
        );
        legacy.endpoint = "sftp://host".to_string();
        let converted = convert_legacy_policy(legacy).unwrap();

        assert!(!converted.storage_config.contains("access_key"));
        assert!(!converted.storage_config.contains("secret_key"));
        let storage: JsonValue = serde_json::from_str(&converted.storage_config).unwrap();
        let connector = &storage["connector"];
        assert_eq!(
            connector["values"]["sftp_host_key_fingerprint"],
            "SHA256:test"
        );
    }

    #[test]
    fn rejects_damaged_non_object_unknown_and_ambiguous_legacy_options() {
        for raw in [
            "{",
            "[]",
            r#"{"future_field":true}"#,
            r#"{"s3_upload_strategy":"presigned","object_storage_upload_strategy":"relay_stream"}"#,
        ] {
            let mut legacy = policy("s3", json!({}));
            legacy.options = raw.to_string();
            assert!(convert_legacy_policy(legacy).is_err(), "{raw}");
        }
    }

    #[test]
    fn rejects_options_owned_by_another_connector_and_unknown_drivers() {
        assert!(
            convert_legacy_policy(policy(
                "local",
                json!({"remote_upload_strategy": "presigned"}),
            ))
            .is_err()
        );
        assert!(convert_legacy_policy(policy("custom_plugin", json!({}))).is_err());
    }
}
