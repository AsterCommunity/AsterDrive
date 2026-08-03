//! Add plugin-safe connector and core behavior envelopes to storage policies.
//!
//! This migration intentionally freezes the legacy-to-envelope mapping instead
//! of calling runtime connector code. Historical migrations must keep producing
//! the same bytes when connector defaults or schemas evolve later.

use std::collections::{BTreeMap, HashSet};

use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::{ConnectionTrait, DbBackend, TransactionTrait};
use serde_json::{Map, Value as JsonValue, json};

const CONNECTOR_CONFIG_FORMAT_VERSION: u32 = 1;
const CONNECTOR_CONFIG_SCHEMA_VERSION: u32 = 1;
const BEHAVIOR_CONFIG_FORMAT_VERSION: u32 = 1;
const BEHAVIOR_CONFIG_SCHEMA_VERSION: u32 = 1;
const UNCONFIGURED_CONNECTOR_ID: &str = "asterdrive.storage.unconfigured";
const EMPTY_CONNECTOR_CONFIG: &str = r#"{"format_version":1,"connector_id":"asterdrive.storage.unconfigured","schema_version":1,"values":{}}"#;
const EMPTY_BEHAVIOR_CONFIG: &str = r#"{"format_version":1,"schema_version":1,"values":{}}"#;

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
        for column in [
            StoragePolicies::BehaviorConfig,
            StoragePolicies::ConnectorConfig,
            StoragePolicies::ConnectorId,
        ] {
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

    for (column, default_json) in [
        (StoragePolicies::ConnectorConfig, EMPTY_CONNECTOR_CONFIG),
        (StoragePolicies::BehaviorConfig, EMPTY_BEHAVIOR_CONFIG),
    ] {
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
                (
                    StoragePolicies::ConnectorConfig,
                    update.connector_config.into(),
                ),
                (
                    StoragePolicies::BehaviorConfig,
                    update.behavior_config.into(),
                ),
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

    for column in [
        StoragePolicies::ConnectorConfig,
        StoragePolicies::BehaviorConfig,
    ] {
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
    connector_config: String,
    behavior_config: String,
}

fn convert_legacy_policy(policy: LegacyStoragePolicy) -> Result<ConfigBackfill, DbErr> {
    let mut options = parse_legacy_options(policy.id, &policy.options)?;
    let legacy_native_thumbnail = options
        .get("thumbnail_processor")
        .and_then(JsonValue::as_str)
        == Some("storage_native");
    let behavior_values = take_behavior_values(policy.id, &mut options)?;
    let (connector_id, connector_values) =
        connector_values(policy.id, &policy, &mut options, legacy_native_thumbnail)?;

    if let Some(field) = options.keys().next() {
        return Err(migration_error(
            policy.id,
            format!(
                "legacy option '{field}' is not owned by driver '{}'",
                policy.driver_type
            ),
        ));
    }

    let connector_config = serde_json::to_string(&json!({
        "format_version": CONNECTOR_CONFIG_FORMAT_VERSION,
        "connector_id": connector_id,
        "schema_version": CONNECTOR_CONFIG_SCHEMA_VERSION,
        "values": connector_values,
    }))
    .map_err(|error| migration_error(policy.id, format!("serialize connector config: {error}")))?;
    let behavior_config = serde_json::to_string(&json!({
        "format_version": BEHAVIOR_CONFIG_FORMAT_VERSION,
        "schema_version": BEHAVIOR_CONFIG_SCHEMA_VERSION,
        "values": behavior_values,
    }))
    .map_err(|error| migration_error(policy.id, format!("serialize behavior config: {error}")))?;

    Ok(ConfigBackfill {
        id: policy.id,
        connector_id: connector_id.to_string(),
        connector_config,
        behavior_config,
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
) -> Result<BTreeMap<String, JsonValue>, DbErr> {
    let mut values = BTreeMap::new();
    if let Some(value) = take_string_enum(
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
    )? {
        values.insert("thumbnail_processor".to_string(), JsonValue::String(value));
    }
    for field in ["thumbnail_extensions", "media_metadata_extensions"] {
        if let Some(extensions) = take_extension_list(policy_id, options, field)?
            && !extensions.is_empty()
        {
            values.insert(field.to_string(), json!(extensions));
        }
    }
    Ok(values)
}

fn connector_values(
    policy_id: i64,
    policy: &LegacyStoragePolicy,
    options: &mut Map<String, JsonValue>,
    legacy_native_thumbnail: bool,
) -> Result<(&'static str, BTreeMap<String, JsonValue>), DbErr> {
    match policy.driver_type.as_str() {
        "local" => local_values(policy_id, policy, options),
        "s3" => s3_values(policy_id, policy, options),
        "sftp" => sftp_values(policy_id, policy, options),
        "azure_blob" => object_storage_values(
            policy_id,
            policy,
            options,
            "asterdrive.storage.azure_blob",
            false,
            legacy_native_thumbnail,
        ),
        "tencent_cos" => object_storage_values(
            policy_id,
            policy,
            options,
            "asterdrive.storage.tencent_cos",
            true,
            legacy_native_thumbnail,
        ),
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
) -> Result<(&'static str, BTreeMap<String, JsonValue>), DbErr> {
    let mut values = connection_values(policy, false, false);
    values.insert(
        "content_dedup".to_string(),
        JsonValue::Bool(take_bool(policy_id, options, "content_dedup")?.unwrap_or(false)),
    );
    Ok(("asterdrive.storage.local", values))
}

fn s3_values(
    policy_id: i64,
    policy: &LegacyStoragePolicy,
    options: &mut Map<String, JsonValue>,
) -> Result<(&'static str, BTreeMap<String, JsonValue>), DbErr> {
    let mut values = object_storage_transfer_values(policy_id, policy, options)?;
    values.insert(
        "s3_path_style".to_string(),
        JsonValue::Bool(take_bool(policy_id, options, "s3_path_style")?.unwrap_or(true)),
    );
    values.insert(
        "s3_region".to_string(),
        JsonValue::String(
            take_trimmed_string(policy_id, options, "s3_region")?
                .unwrap_or_else(|| "auto".to_string()),
        ),
    );
    for (field, default) in [
        ("s3_connect_timeout_secs", 5_u64),
        ("s3_read_timeout_secs", 30_u64),
        ("s3_operation_timeout_secs", 3_600_u64),
    ] {
        // Legacy runtime treated an explicit zero exactly like an omitted
        // timeout, so materialize the effective default in the new schema.
        let timeout = take_u64(policy_id, options, field)?
            .filter(|value| *value > 0)
            .unwrap_or(default);
        values.insert(field.to_string(), json!(timeout));
    }
    Ok(("asterdrive.storage.s3", values))
}

fn sftp_values(
    policy_id: i64,
    policy: &LegacyStoragePolicy,
    options: &mut Map<String, JsonValue>,
) -> Result<(&'static str, BTreeMap<String, JsonValue>), DbErr> {
    let mut values = connection_values(policy, true, false);
    if let Some(value) = take_trimmed_string(policy_id, options, "sftp_host_key_fingerprint")? {
        values.insert(
            "sftp_host_key_fingerprint".to_string(),
            JsonValue::String(value),
        );
    }
    Ok(("asterdrive.storage.sftp", values))
}

fn object_storage_values(
    policy_id: i64,
    policy: &LegacyStoragePolicy,
    options: &mut Map<String, JsonValue>,
    connector_id: &'static str,
    supports_native_processing: bool,
    legacy_native_thumbnail: bool,
) -> Result<(&'static str, BTreeMap<String, JsonValue>), DbErr> {
    let mut values = object_storage_transfer_values(policy_id, policy, options)?;
    if supports_native_processing {
        let processing = take_bool(policy_id, options, "storage_native_processing_enabled")?
            .unwrap_or(legacy_native_thumbnail);
        let metadata = take_bool(policy_id, options, "storage_native_media_metadata_enabled")?
            .unwrap_or(false);
        values.insert(
            "storage_native_processing_enabled".to_string(),
            JsonValue::Bool(processing),
        );
        values.insert(
            "storage_native_media_metadata_enabled".to_string(),
            JsonValue::Bool(metadata),
        );
    }
    Ok((connector_id, values))
}

fn object_storage_transfer_values(
    policy_id: i64,
    policy: &LegacyStoragePolicy,
    options: &mut Map<String, JsonValue>,
) -> Result<BTreeMap<String, JsonValue>, DbErr> {
    let mut values = connection_values(policy, true, true);
    for field in [
        "object_storage_upload_strategy",
        "object_storage_download_strategy",
    ] {
        let value = take_string_enum(policy_id, options, field, &["relay_stream", "presigned"])?
            .unwrap_or_else(|| "relay_stream".to_string());
        values.insert(field.to_string(), JsonValue::String(value));
    }
    Ok(values)
}

fn remote_values(
    policy_id: i64,
    policy: &LegacyStoragePolicy,
    options: &mut Map<String, JsonValue>,
) -> Result<(&'static str, BTreeMap<String, JsonValue>), DbErr> {
    let mut values = connection_values(policy, false, false);
    if let Some(remote_node_id) = policy.remote_node_id {
        values.insert("remote_node_id".to_string(), json!(remote_node_id));
    }
    if let Some(target_key) = policy
        .remote_storage_target_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        values.insert(
            "remote_storage_target_key".to_string(),
            JsonValue::String(target_key.to_string()),
        );
    }
    for field in ["remote_download_strategy", "remote_upload_strategy"] {
        let value = take_string_enum(policy_id, options, field, &["relay_stream", "presigned"])?
            .unwrap_or_else(|| "relay_stream".to_string());
        values.insert(field.to_string(), JsonValue::String(value));
    }
    Ok(("asterdrive.storage.remote", values))
}

fn onedrive_values(
    policy_id: i64,
    policy: &LegacyStoragePolicy,
    options: &mut Map<String, JsonValue>,
) -> Result<(&'static str, BTreeMap<String, JsonValue>), DbErr> {
    let mut values = connection_values(policy, false, false);
    for (legacy, current, allowed, default) in [
        (
            "provider_resumable_upload_strategy",
            "provider_resumable_upload_strategy",
            &["server_relay", "frontend_direct"][..],
            "server_relay",
        ),
        (
            "provider_download_strategy",
            "provider_download_strategy",
            &["server_relay", "frontend_direct"][..],
            "server_relay",
        ),
        (
            "provider_download_filename_mode",
            "provider_download_filename_mode",
            &["provider_native", "strict_current"][..],
            "provider_native",
        ),
        (
            "onedrive_cloud",
            "cloud",
            &["global", "china"][..],
            "global",
        ),
        (
            "onedrive_account_mode",
            "account_mode",
            &[
                "personal",
                "work_or_school",
                "sharepoint_site",
                "group_drive",
            ][..],
            "personal",
        ),
    ] {
        let value = take_string_enum(policy_id, options, legacy, allowed)?
            .unwrap_or_else(|| default.to_string());
        values.insert(current.to_string(), JsonValue::String(value));
    }
    for (legacy, current) in [
        ("onedrive_tenant", "tenant"),
        ("onedrive_drive_id", "drive_id"),
        ("onedrive_root_item_id", "root_item_id"),
        ("onedrive_site_id", "site_id"),
        ("onedrive_group_id", "group_id"),
    ] {
        if let Some(value) = take_trimmed_string(policy_id, options, legacy)? {
            values.insert(current.to_string(), JsonValue::String(value));
        }
    }
    Ok(("asterdrive.storage.onedrive", values))
}

fn connection_values(
    policy: &LegacyStoragePolicy,
    include_endpoint: bool,
    include_bucket: bool,
) -> BTreeMap<String, JsonValue> {
    let mut values = BTreeMap::from([(
        "base_path".to_string(),
        JsonValue::String(policy.base_path.clone()),
    )]);
    if include_endpoint {
        values.insert(
            "endpoint".to_string(),
            JsonValue::String(policy.endpoint.clone()),
        );
    }
    if include_bucket {
        values.insert(
            "bucket".to_string(),
            JsonValue::String(policy.bucket.clone()),
        );
    }
    values
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
    ConnectorConfig,
    BehaviorConfig,
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
            let envelope: JsonValue = serde_json::from_str(&converted.connector_config).unwrap();
            assert_eq!(envelope["format_version"], 1);
            assert_eq!(envelope["schema_version"], 1);
            assert_eq!(envelope["connector_id"], connector);
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
        let connector: JsonValue = serde_json::from_str(&converted.connector_config).unwrap();
        let behavior: JsonValue = serde_json::from_str(&converted.behavior_config).unwrap();

        assert_eq!(
            connector["values"]["object_storage_upload_strategy"],
            "presigned"
        );
        assert_eq!(connector["values"]["s3_path_style"], false);
        assert_eq!(connector["values"]["s3_connect_timeout_secs"], 5);
        assert_eq!(
            behavior["values"]["thumbnail_extensions"],
            json!(["jpg", "webp"])
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

        assert!(!converted.connector_config.contains("access_key"));
        assert!(!converted.connector_config.contains("secret_key"));
        let connector: JsonValue = serde_json::from_str(&converted.connector_config).unwrap();
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
