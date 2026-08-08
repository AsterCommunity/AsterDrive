//! Make core policy behavior the sole owner of storage-native enablement.
//!
//! # Downgrade limitation
//!
//! V1 used a non-empty `media_metadata_extensions` list to mean both enabled
//! and configured, so it cannot represent V2's disabled state with retained
//! media-metadata extensions. Downgrading such a policy returns an error
//! instead of either discarding the retained configuration or silently
//! re-enabling provider-native requests. Dormant thumbnail extensions remain
//! reversible because V1 stored `thumbnail_processor` separately from
//! `thumbnail_extensions`.

use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::{ConnectionTrait, TransactionTrait};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value as JsonValue};

#[derive(DeriveMigrationName)]
pub struct Migration;

const TENCENT_COS_CONNECTOR_ID: &str = "asterdrive.storage.tencent_cos";

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        rewrite_storage_policy_configs(manager, Direction::Up).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        rewrite_storage_policy_configs(manager, Direction::Down).await
    }
}

#[derive(Clone, Copy)]
enum Direction {
    Up,
    Down,
}

async fn rewrite_storage_policy_configs(
    manager: &SchemaManager<'_>,
    direction: Direction,
) -> Result<(), DbErr> {
    let db = manager.get_connection();
    let transaction = db.begin().await?;
    let mut select = Query::select();
    select
        .columns([
            StoragePolicies::Id,
            StoragePolicies::ConnectorId,
            StoragePolicies::StorageConfig,
        ])
        .from(StoragePolicies::Table)
        .order_by(StoragePolicies::Id, Order::Asc);

    let rows = transaction.query_all(&select).await?;
    for row in rows {
        let policy_id = row.try_get_by_index::<i64>(0)?;
        let connector_id = row.try_get_by_index::<String>(1)?;
        let raw = row.try_get_by_index::<String>(2)?;
        let rewritten = match direction {
            Direction::Up => upgrade_policy_config(policy_id, &connector_id, &raw),
            Direction::Down => downgrade_policy_config(policy_id, &connector_id, &raw),
        }?;

        let mut update = Query::update();
        update
            .table(StoragePolicies::Table)
            .value(StoragePolicies::StorageConfig, rewritten)
            .and_where(Expr::col(StoragePolicies::Id).eq(policy_id));
        transaction.execute(&update).await?;
    }
    transaction.commit().await
}

fn upgrade_policy_config(policy_id: i64, connector_id: &str, raw: &str) -> Result<String, DbErr> {
    let mut envelope: FrozenStoragePolicyConfigV1 = decode_policy_config(policy_id, raw)?;
    validate_envelope(policy_id, connector_id, &envelope, 1)?;

    let media_metadata_enabled = !envelope
        .behavior
        .values
        .media_metadata_extensions
        .is_empty();
    let thumbnail_enabled =
        envelope.behavior.values.thumbnail_processor.as_deref() == Some("storage_native");
    if connector_id == TENCENT_COS_CONNECTOR_ID {
        validate_tencent_cos_v1_values(policy_id, &envelope.connector.values)?;
        envelope
            .connector
            .values
            .remove("storage_native_processing_enabled");
        envelope
            .connector
            .values
            .remove("storage_native_media_metadata_enabled");
        envelope.connector.schema_version = 2;
    }

    let upgraded = FrozenStoragePolicyConfigV2 {
        format_version: envelope.format_version,
        connector: envelope.connector,
        behavior: FrozenBehaviorEnvelopeV2 {
            format_version: envelope.behavior.format_version,
            schema_version: 2,
            values: FrozenBehaviorV2 {
                storage_native_thumbnail_enabled: thumbnail_enabled,
                storage_native_thumbnail_extensions: envelope.behavior.values.thumbnail_extensions,
                storage_native_media_metadata_enabled: media_metadata_enabled,
                storage_native_media_metadata_extensions: envelope
                    .behavior
                    .values
                    .media_metadata_extensions,
            },
        },
    };
    encode_policy_config(policy_id, &upgraded)
}

fn downgrade_policy_config(policy_id: i64, connector_id: &str, raw: &str) -> Result<String, DbErr> {
    let mut envelope: FrozenStoragePolicyConfigV2 = decode_policy_config(policy_id, raw)?;
    validate_envelope(policy_id, connector_id, &envelope, 2)?;

    // V1 treats a non-empty metadata extension list as enabled and therefore
    // cannot encode dormant configuration. Thumbnail fields need no matching
    // guard because V1 stored their processor and extensions independently.
    if !envelope
        .behavior
        .values
        .storage_native_media_metadata_enabled
        && !envelope
            .behavior
            .values
            .storage_native_media_metadata_extensions
            .is_empty()
    {
        return Err(policy_error(
            policy_id,
            "cannot downgrade disabled storage-native media metadata with retained extensions to behavior schema V1",
        ));
    }

    if connector_id == TENCENT_COS_CONNECTOR_ID {
        if envelope.connector.schema_version != 2 {
            return Err(policy_error(
                policy_id,
                format!(
                    "Tencent COS connector schema must be 2, got {}",
                    envelope.connector.schema_version
                ),
            ));
        }
        let media_enabled = envelope
            .behavior
            .values
            .storage_native_media_metadata_enabled;
        let processing_enabled =
            media_enabled || envelope.behavior.values.storage_native_thumbnail_enabled;
        envelope.connector.values.insert(
            "storage_native_processing_enabled".to_string(),
            JsonValue::Bool(processing_enabled),
        );
        envelope.connector.values.insert(
            "storage_native_media_metadata_enabled".to_string(),
            JsonValue::Bool(media_enabled),
        );
        envelope.connector.schema_version = 1;
    }

    let downgraded = FrozenStoragePolicyConfigV1 {
        format_version: envelope.format_version,
        connector: envelope.connector,
        behavior: FrozenBehaviorEnvelopeV1 {
            format_version: envelope.behavior.format_version,
            schema_version: 1,
            values: FrozenBehaviorV1 {
                thumbnail_processor: envelope
                    .behavior
                    .values
                    .storage_native_thumbnail_enabled
                    .then(|| "storage_native".to_string()),
                thumbnail_extensions: envelope.behavior.values.storage_native_thumbnail_extensions,
                media_metadata_extensions: envelope
                    .behavior
                    .values
                    .storage_native_media_metadata_extensions,
            },
        },
    };
    encode_policy_config(policy_id, &downgraded)
}

fn validate_envelope<T>(
    policy_id: i64,
    connector_id: &str,
    envelope: &FrozenStoragePolicyConfig<T>,
    behavior_schema_version: u32,
) -> Result<(), DbErr> {
    if envelope.format_version != 1
        || envelope.connector.format_version != 1
        || envelope.behavior.format_version != 1
    {
        return Err(policy_error(policy_id, "unsupported config format version"));
    }
    if envelope.connector.connector_id != connector_id {
        return Err(policy_error(
            policy_id,
            format!(
                "connector id mismatch: row has '{connector_id}', config has '{}'",
                envelope.connector.connector_id
            ),
        ));
    }
    if envelope.behavior.schema_version != behavior_schema_version {
        return Err(policy_error(
            policy_id,
            format!(
                "behavior schema must be {behavior_schema_version}, got {}",
                envelope.behavior.schema_version
            ),
        ));
    }
    if connector_id == TENCENT_COS_CONNECTOR_ID {
        let expected = if behavior_schema_version == 1 { 1 } else { 2 };
        if envelope.connector.schema_version != expected {
            return Err(policy_error(
                policy_id,
                format!(
                    "Tencent COS connector schema must be {expected}, got {}",
                    envelope.connector.schema_version
                ),
            ));
        }
    }
    Ok(())
}

fn validate_tencent_cos_v1_values(
    policy_id: i64,
    values: &Map<String, JsonValue>,
) -> Result<(), DbErr> {
    for field in [
        "storage_native_processing_enabled",
        "storage_native_media_metadata_enabled",
    ] {
        if values.get(field).is_some_and(|value| !value.is_boolean()) {
            return Err(policy_error(
                policy_id,
                format!("Tencent COS field '{field}' must be boolean"),
            ));
        }
    }
    Ok(())
}

fn decode_policy_config<T>(policy_id: i64, raw: &str) -> Result<T, DbErr>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_str(raw)
        .map_err(|error| policy_error(policy_id, format!("invalid storage_config JSON: {error}")))
}

fn encode_policy_config<T: Serialize>(policy_id: i64, value: &T) -> Result<String, DbErr> {
    serde_json::to_string(value)
        .map_err(|error| policy_error(policy_id, format!("serialize storage_config: {error}")))
}

fn policy_error(policy_id: i64, message: impl Into<String>) -> DbErr {
    DbErr::Migration(format!("storage policy {policy_id}: {}", message.into()))
}

type FrozenStoragePolicyConfigV1 = FrozenStoragePolicyConfig<FrozenBehaviorV1>;
type FrozenStoragePolicyConfigV2 = FrozenStoragePolicyConfig<FrozenBehaviorV2>;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FrozenStoragePolicyConfig<T> {
    format_version: u32,
    connector: FrozenConnectorEnvelope,
    behavior: FrozenBehaviorEnvelope<T>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FrozenConnectorEnvelope {
    format_version: u32,
    connector_id: String,
    schema_version: u32,
    values: Map<String, JsonValue>,
}

type FrozenBehaviorEnvelopeV1 = FrozenBehaviorEnvelope<FrozenBehaviorV1>;
type FrozenBehaviorEnvelopeV2 = FrozenBehaviorEnvelope<FrozenBehaviorV2>;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FrozenBehaviorEnvelope<T> {
    format_version: u32,
    schema_version: u32,
    values: T,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FrozenBehaviorV1 {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    thumbnail_processor: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    thumbnail_extensions: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    media_metadata_extensions: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FrozenBehaviorV2 {
    #[serde(default)]
    storage_native_thumbnail_enabled: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    storage_native_thumbnail_extensions: Vec<String>,
    #[serde(default)]
    storage_native_media_metadata_enabled: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    storage_native_media_metadata_extensions: Vec<String>,
}

#[derive(DeriveIden)]
enum StoragePolicies {
    Table,
    Id,
    ConnectorId,
    StorageConfig,
}

#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm_migration::sea_orm::{Database, DbBackend, Statement};

    fn config(
        connector_id: &str,
        connector_values: JsonValue,
        behavior_values: JsonValue,
    ) -> String {
        serde_json::json!({
            "format_version": 1,
            "connector": {
                "format_version": 1,
                "connector_id": connector_id,
                "schema_version": 1,
                "values": connector_values,
            },
            "behavior": {
                "format_version": 1,
                "schema_version": 1,
                "values": behavior_values,
            },
        })
        .to_string()
    }

    fn upgraded(raw: &str) -> JsonValue {
        serde_json::from_str(
            &upgrade_policy_config(7, TENCENT_COS_CONNECTOR_ID, raw)
                .expect("fixture should upgrade"),
        )
        .expect("upgraded config should be JSON")
    }

    #[test]
    fn upgrade_uses_executed_behavior_instead_of_legacy_connector_switches() {
        let cases = [
            (
                false,
                false,
                serde_json::json!({}),
                false,
                serde_json::json!({
                    "storage_native_thumbnail_enabled": false,
                    "storage_native_media_metadata_enabled": false
                }),
            ),
            (
                true,
                true,
                serde_json::json!({}),
                false,
                serde_json::json!({
                    "storage_native_thumbnail_enabled": false,
                    "storage_native_media_metadata_enabled": false
                }),
            ),
            (
                false,
                false,
                serde_json::json!({
                    "thumbnail_processor": "storage_native",
                    "thumbnail_extensions": ["jpg"]
                }),
                false,
                serde_json::json!({
                    "storage_native_thumbnail_enabled": true,
                    "storage_native_thumbnail_extensions": ["jpg"],
                    "storage_native_media_metadata_enabled": false
                }),
            ),
            (
                false,
                false,
                serde_json::json!({"media_metadata_extensions": ["mp4"]}),
                true,
                serde_json::json!({
                    "storage_native_thumbnail_enabled": false,
                    "storage_native_media_metadata_enabled": true,
                    "storage_native_media_metadata_extensions": ["mp4"]
                }),
            ),
        ];

        for (processing, metadata, behavior, expected_metadata, expected_behavior) in cases {
            let raw = config(
                TENCENT_COS_CONNECTOR_ID,
                serde_json::json!({
                    "endpoint": "https://bucket.cos.example",
                    "bucket": "bucket",
                    "base_path": "",
                    "storage_native_processing_enabled": processing,
                    "storage_native_media_metadata_enabled": metadata,
                }),
                behavior,
            );
            let value = upgraded(&raw);
            assert_eq!(value["connector"]["schema_version"], 2);
            assert_eq!(
                value["connector"]["values"].get("storage_native_processing_enabled"),
                None
            );
            assert_eq!(
                value["connector"]["values"].get("storage_native_media_metadata_enabled"),
                None
            );
            assert_eq!(
                value["behavior"]["values"], expected_behavior,
                "legacy connector switches must not override executed behavior"
            );
            assert_eq!(
                value["behavior"]["values"]
                    .get("storage_native_media_metadata_enabled")
                    .and_then(JsonValue::as_bool)
                    .unwrap_or(false),
                expected_metadata
            );
        }
    }

    #[test]
    fn upgrade_preserves_dormant_thumbnail_extensions_and_other_connectors() {
        let inactive = config(
            TENCENT_COS_CONNECTOR_ID,
            serde_json::json!({
                "storage_native_processing_enabled": false,
                "storage_native_media_metadata_enabled": false,
            }),
            serde_json::json!({
                "thumbnail_processor": "images",
                "thumbnail_extensions": ["jpg"]
            }),
        );
        assert_eq!(
            upgraded(&inactive)["behavior"]["values"],
            serde_json::json!({
                "storage_native_thumbnail_enabled": false,
                "storage_native_thumbnail_extensions": ["jpg"],
                "storage_native_media_metadata_enabled": false
            })
        );

        let other = config(
            "com.example.storage",
            serde_json::json!({"opaque": {"nested": true}}),
            serde_json::json!({}),
        );
        let value: JsonValue = serde_json::from_str(
            &upgrade_policy_config(8, "com.example.storage", &other)
                .expect("plugin config should upgrade behavior only"),
        )
        .unwrap();
        assert_eq!(value["connector"]["schema_version"], 1);
        assert_eq!(value["connector"]["values"]["opaque"]["nested"], true);
        assert_eq!(value["behavior"]["schema_version"], 2);
    }

    #[test]
    fn upgrade_rejects_malformed_versions_namespaces_and_legacy_switch_types() {
        let valid = config(
            TENCENT_COS_CONNECTOR_ID,
            serde_json::json!({
                "storage_native_processing_enabled": false,
                "storage_native_media_metadata_enabled": false,
            }),
            serde_json::json!({}),
        );
        let malformed = [
            ("other.connector", valid.clone()),
            (
                TENCENT_COS_CONNECTOR_ID,
                valid.replace("\"schema_version\":1", "\"schema_version\":9"),
            ),
            (
                TENCENT_COS_CONNECTOR_ID,
                valid.replace(
                    "\"storage_native_processing_enabled\":false",
                    "\"storage_native_processing_enabled\":\"false\"",
                ),
            ),
        ];
        for (connector_id, raw) in malformed {
            assert!(upgrade_policy_config(9, connector_id, &raw).is_err());
        }
    }

    #[test]
    fn downgrade_reconstructs_legacy_switches_and_rejects_unrepresentable_dormant_metadata() {
        let up = upgrade_policy_config(
            10,
            TENCENT_COS_CONNECTOR_ID,
            &config(
                TENCENT_COS_CONNECTOR_ID,
                serde_json::json!({
                    "storage_native_processing_enabled": false,
                    "storage_native_media_metadata_enabled": false,
                }),
                serde_json::json!({"media_metadata_extensions": ["mp4"]}),
            ),
        )
        .unwrap();
        let down: JsonValue = serde_json::from_str(
            &downgrade_policy_config(10, TENCENT_COS_CONNECTOR_ID, &up).unwrap(),
        )
        .unwrap();
        assert_eq!(down["connector"]["schema_version"], 1);
        assert_eq!(
            down["connector"]["values"]["storage_native_processing_enabled"],
            true
        );
        assert_eq!(
            down["connector"]["values"]["storage_native_media_metadata_enabled"],
            true
        );

        let dormant_thumbnail = upgrade_policy_config(
            11,
            TENCENT_COS_CONNECTOR_ID,
            &config(
                TENCENT_COS_CONNECTOR_ID,
                serde_json::json!({
                    "storage_native_processing_enabled": false,
                    "storage_native_media_metadata_enabled": false,
                }),
                serde_json::json!({"thumbnail_extensions": ["jpg"]}),
            ),
        )
        .unwrap();
        let dormant_thumbnail_down: JsonValue = serde_json::from_str(
            &downgrade_policy_config(11, TENCENT_COS_CONNECTOR_ID, &dormant_thumbnail).unwrap(),
        )
        .unwrap();
        assert_eq!(
            dormant_thumbnail_down["behavior"]["values"]["thumbnail_extensions"],
            serde_json::json!(["jpg"])
        );

        let dormant_metadata = up.replace(
            "\"storage_native_media_metadata_enabled\":true",
            "\"storage_native_media_metadata_enabled\":false",
        );
        let error = downgrade_policy_config(10, TENCENT_COS_CONNECTOR_ID, &dormant_metadata)
            .expect_err("V1 cannot represent disabled metadata with retained extensions");
        assert!(
            error
                .to_string()
                .contains("cannot downgrade disabled storage-native media metadata")
        );
    }

    #[tokio::test]
    async fn migration_rewrites_every_row_atomically_and_round_trips_down() {
        let db = Database::connect("sqlite::memory:").await.unwrap();
        db.execute_raw(Statement::from_string(
            DbBackend::Sqlite,
            "CREATE TABLE storage_policies (id INTEGER PRIMARY KEY, connector_id TEXT NOT NULL, storage_config TEXT NOT NULL)".to_string(),
        ))
        .await
        .unwrap();
        let cos = config(
            TENCENT_COS_CONNECTOR_ID,
            serde_json::json!({
                "storage_native_processing_enabled": false,
                "storage_native_media_metadata_enabled": false,
            }),
            serde_json::json!({"media_metadata_extensions": ["mp4"]}),
        );
        let plugin = config(
            "com.example.storage",
            serde_json::json!({"opaque": "kept"}),
            serde_json::json!({}),
        );
        for (id, connector_id, raw) in [
            (1_i64, TENCENT_COS_CONNECTOR_ID, cos),
            (2_i64, "com.example.storage", plugin),
        ] {
            let mut insert = Query::insert();
            insert
                .into_table(StoragePolicies::Table)
                .columns([
                    StoragePolicies::Id,
                    StoragePolicies::ConnectorId,
                    StoragePolicies::StorageConfig,
                ])
                .values_panic([id.into(), connector_id.into(), raw.into()]);
            db.execute(&insert).await.unwrap();
        }

        let manager = SchemaManager::new(&db);
        Migration.up(&manager).await.unwrap();
        let rows = db
            .query_all_raw(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT storage_config FROM storage_policies ORDER BY id".to_string(),
            ))
            .await
            .unwrap();
        let cos_up: JsonValue =
            serde_json::from_str(&rows[0].try_get_by_index::<String>(0).unwrap()).unwrap();
        let plugin_up: JsonValue =
            serde_json::from_str(&rows[1].try_get_by_index::<String>(0).unwrap()).unwrap();
        assert_eq!(cos_up["connector"]["schema_version"], 2);
        assert_eq!(plugin_up["connector"]["values"]["opaque"], "kept");
        assert_eq!(plugin_up["behavior"]["schema_version"], 2);

        Migration.down(&manager).await.unwrap();
        let rows = db
            .query_all_raw(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT storage_config FROM storage_policies ORDER BY id".to_string(),
            ))
            .await
            .unwrap();
        let cos_down: JsonValue =
            serde_json::from_str(&rows[0].try_get_by_index::<String>(0).unwrap()).unwrap();
        let plugin_down: JsonValue =
            serde_json::from_str(&rows[1].try_get_by_index::<String>(0).unwrap()).unwrap();
        assert_eq!(cos_down["connector"]["schema_version"], 1);
        assert_eq!(cos_down["behavior"]["schema_version"], 1);
        assert_eq!(plugin_down["behavior"]["schema_version"], 1);
    }
}
