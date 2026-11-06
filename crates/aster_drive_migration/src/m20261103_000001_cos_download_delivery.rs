//! Preserve released COS custom download domains while making delivery explicit.
//! Rewrite persisted policies, remote targets and deferred cleanup snapshots.

use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::{ConnectionTrait, TransactionTrait};
use serde_json::Value;

#[derive(DeriveMigrationName)]
pub struct Migration;

const CONNECTOR: &str = "asterdrive.storage.tencent_cos";
const BATCH_SIZE: u64 = 200;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        rewrite_configs(manager, true).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        rewrite_configs(manager, false).await
    }
}

async fn rewrite_configs(manager: &SchemaManager<'_>, upgrade: bool) -> Result<(), DbErr> {
    let transaction = manager.get_connection().begin().await?;
    for (table, column, filter_column, filter_value) in [
        (
            "storage_policies",
            "storage_config",
            "connector_id",
            CONNECTOR,
        ),
        (
            "remote_storage_targets",
            "connector_config",
            "connector_id",
            CONNECTOR,
        ),
        (
            "background_tasks",
            "payload_json",
            "kind",
            "storage_policy_temp_cleanup",
        ),
    ] {
        let mut cursor = i64::MIN;
        loop {
            let select = Query::select()
                .columns([Alias::new("id"), Alias::new(column)])
                .from(Alias::new(table))
                .and_where(Expr::col(Alias::new(filter_column)).eq(filter_value))
                .and_where(Expr::col(Alias::new("id")).gt(cursor))
                .order_by(Alias::new("id"), Order::Asc)
                .limit(BATCH_SIZE)
                .to_owned();
            let rows = transaction.query_all(&select).await?;
            if rows.is_empty() {
                break;
            }
            for row in rows {
                let id: i64 = row.try_get_by_index(0)?;
                cursor = id;
                let Some(raw): Option<String> = row.try_get_by_index(1)? else {
                    continue;
                };
                let mut value: Value =
                    serde_json::from_str(&raw).map_err(|_| invalid_row(table, id))?;
                let changed = if table == "background_tasks" {
                    rewrite_cleanup(&mut value, upgrade)
                } else if table == "storage_policies" {
                    rewrite_connector(
                        value
                            .get_mut("connector")
                            .ok_or_else(|| invalid_row(table, id))?,
                        upgrade,
                    )
                } else {
                    rewrite_connector(&mut value, upgrade)
                }
                .map_err(|_| invalid_row(table, id))?;
                if changed {
                    transaction
                        .execute(
                            &Query::update()
                                .table(Alias::new(table))
                                .value(Alias::new(column), value.to_string())
                                .and_where(Expr::col(Alias::new("id")).eq(id))
                                .to_owned(),
                        )
                        .await?;
                }
            }
        }
    }
    transaction.commit().await
}

fn invalid_row(table: &str, id: i64) -> DbErr {
    DbErr::Migration(format!(
        "invalid COS download delivery configuration in {table} row {id}; no configuration values were logged"
    ))
}

fn rewrite_cleanup(payload: &mut Value, upgrade: bool) -> Result<bool, ()> {
    let Some(policy) = payload.get_mut("policy") else {
        return Ok(false);
    };
    if policy.get("connector_id").and_then(Value::as_str) != Some(CONNECTOR) {
        return Ok(false);
    }
    let raw = policy
        .get("storage_config")
        .and_then(Value::as_str)
        .ok_or(())?;
    let mut config: Value = serde_json::from_str(raw).map_err(|_| ())?;
    let changed = rewrite_connector(config.get_mut("connector").ok_or(())?, upgrade)?;
    if changed {
        policy["storage_config"] = Value::String(config.to_string());
    }
    Ok(changed)
}

fn rewrite_connector(config: &mut Value, upgrade: bool) -> Result<bool, ()> {
    match (
        config.get("connector_id").and_then(Value::as_str),
        config.get("format_version").and_then(Value::as_u64),
        config.get("schema_version").and_then(Value::as_u64),
    ) {
        (Some(CONNECTOR), Some(1), Some(1)) => {}
        _ => return Err(()),
    }
    let values = config
        .get_mut("values")
        .and_then(Value::as_object_mut)
        .ok_or(())?;
    match (upgrade, values.get("download_delivery")) {
        (true, None) => match values.get("download_base_url") {
            None | Some(Value::Null) => Ok(false),
            Some(Value::String(url)) if url.trim().is_empty() => Ok(false),
            Some(Value::String(_)) => {
                values.insert(
                    "download_delivery".into(),
                    Value::String("custom_origin".into()),
                );
                Ok(true)
            }
            Some(_) => Err(()),
        },
        (false, Some(Value::String(mode))) if mode == "custom_origin" => {
            // Earlier versions use the optional address when the mode is absent.
            values.remove("download_delivery");
            Ok(true)
        }
        (true, Some(_)) | (false, _) => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm_migration::sea_orm::{Database, DatabaseConnection};
    use serde_json::json;

    fn connector(address: Value, mode: Option<&str>) -> Value {
        let mut value = json!({"format_version":1,"connector_id":CONNECTOR,"schema_version":1,"values":{"endpoint":"https://cos.ap-guangzhou.myqcloud.com","bucket":"archive-1250000000","download_base_url":address,"base_path":"tenant"}});
        if let Some(mode) = mode {
            value["values"]["download_delivery"] = json!(mode);
        }
        value
    }

    fn policy(config: Value) -> Value {
        json!({"format_version":1,"connector":config,"behavior":{"format_version":1,"schema_version":1,"values":{}}})
    }

    #[test]
    fn missing_modes_only_are_upgraded_and_conversion_is_idempotent() {
        for mode in [
            None,
            Some("cos_origin"),
            Some("custom_origin"),
            Some("tencent_cdn_type_d_sha256"),
        ] {
            let mut value = connector(json!("https://download.example.test/assets"), mode);
            let original = value.clone();
            assert_eq!(rewrite_connector(&mut value, true).unwrap(), mode.is_none());
            assert_eq!(
                value["values"]["download_delivery"],
                mode.unwrap_or("custom_origin")
            );
            assert_eq!(
                value["values"]["download_base_url"],
                original["values"]["download_base_url"]
            );
            assert!(!rewrite_connector(&mut value, true).unwrap());
            match mode {
                None => {
                    assert!(rewrite_connector(&mut value, false).unwrap());
                    assert_eq!(value, original);
                }
                Some("custom_origin") => {
                    assert!(rewrite_connector(&mut value, false).unwrap());
                    assert!(value["values"].get("download_delivery").is_none());
                }
                Some(_) => {
                    assert!(!rewrite_connector(&mut value, false).unwrap());
                    assert_eq!(value, original);
                }
            }
            assert!(!rewrite_connector(&mut value, false).unwrap());
        }
        for address in [Value::Null, json!(""), json!("  ")] {
            let mut value = connector(address, None);
            let original = value.clone();
            assert!(!rewrite_connector(&mut value, true).unwrap());
            assert_eq!(value, original);
        }
        let mut value = connector(json!("ignored"), None);
        value["values"]
            .as_object_mut()
            .unwrap()
            .remove("download_base_url");
        assert!(!rewrite_connector(&mut value, true).unwrap());
    }

    async fn fixture() -> DatabaseConnection {
        // Optional URL must name a fresh disposable test database.
        let test_url = std::env::var("ASTER_COS_DELIVERY_TEST_DATABASE_URL")
            .unwrap_or_else(|_| "sqlite::memory:".into());
        let db = Database::connect(test_url).await.unwrap();
        for (table, column, filter) in [
            ("storage_policies", "storage_config", "connector_id"),
            ("remote_storage_targets", "connector_config", "connector_id"),
            ("background_tasks", "payload_json", "kind"),
        ] {
            db.execute(
                &Table::create()
                    .table(Alias::new(table))
                    .col(ColumnDef::new(Alias::new("id")).big_integer().primary_key())
                    .col(ColumnDef::new(Alias::new(filter)).string())
                    .col(ColumnDef::new(Alias::new(column)).text())
                    .to_owned(),
            )
            .await
            .unwrap();
        }
        db
    }

    async fn insert(
        db: &DatabaseConnection,
        table: &str,
        column: &str,
        filter: &str,
        filter_value: &str,
        id: i64,
        value: &Value,
    ) {
        db.execute(
            &Query::insert()
                .into_table(Alias::new(table))
                .columns([Alias::new("id"), Alias::new(filter), Alias::new(column)])
                .values_panic([id.into(), filter_value.into(), value.to_string().into()])
                .to_owned(),
        )
        .await
        .unwrap();
    }

    async fn read(db: &DatabaseConnection, table: &str, column: &str, id: i64) -> Value {
        let row = db
            .query_one(
                &Query::select()
                    .column(Alias::new(column))
                    .from(Alias::new(table))
                    .and_where(Expr::col(Alias::new("id")).eq(id))
                    .to_owned(),
            )
            .await
            .unwrap()
            .unwrap();
        serde_json::from_str(&row.try_get_by_index::<String>(0).unwrap()).unwrap()
    }

    #[tokio::test]
    async fn migration_round_trips_policies_targets_and_cleanup_across_batches() {
        let db = fixture().await;
        let old = connector(json!("https://download.example.test/assets"), None);
        let stored = policy(old.clone());
        for id in 1..=205 {
            insert(
                &db,
                "storage_policies",
                "storage_config",
                "connector_id",
                CONNECTOR,
                id,
                &stored,
            )
            .await;
        }
        insert(
            &db,
            "remote_storage_targets",
            "connector_config",
            "connector_id",
            CONNECTOR,
            1,
            &old,
        )
        .await;
        let cleanup = json!({"policy":{"connector_id":CONNECTOR,"storage_config":stored.to_string()},"driver_snapshot":{"ciphertext":"unchanged"},"temp_keys":["uploads/file"]});
        insert(
            &db,
            "background_tasks",
            "payload_json",
            "kind",
            "storage_policy_temp_cleanup",
            1,
            &cleanup,
        )
        .await;
        let unrelated = json!({"connector":{"values":{"download_base_url":"untouched"}}});
        insert(
            &db,
            "storage_policies",
            "storage_config",
            "connector_id",
            "plugin.other",
            206,
            &unrelated,
        )
        .await;
        let manager = SchemaManager::new(&db);
        Migration.up(&manager).await.unwrap();
        Migration.up(&manager).await.unwrap();
        for id in [1, 200, 205] {
            assert_eq!(
                read(&db, "storage_policies", "storage_config", id).await["connector"]["values"]["download_delivery"],
                "custom_origin"
            );
        }
        assert_eq!(
            read(&db, "remote_storage_targets", "connector_config", 1).await["values"]["download_delivery"],
            "custom_origin"
        );
        let migrated = read(&db, "background_tasks", "payload_json", 1).await;
        let nested: Value =
            serde_json::from_str(migrated["policy"]["storage_config"].as_str().unwrap()).unwrap();
        assert_eq!(
            nested["connector"]["values"]["download_delivery"],
            "custom_origin"
        );
        assert_eq!(migrated["driver_snapshot"], cleanup["driver_snapshot"]);
        assert_eq!(
            read(&db, "storage_policies", "storage_config", 206).await,
            unrelated
        );
        Migration.down(&manager).await.unwrap();
        Migration.down(&manager).await.unwrap();
        assert_eq!(
            read(&db, "storage_policies", "storage_config", 205).await,
            stored
        );
        assert_eq!(
            read(&db, "remote_storage_targets", "connector_config", 1).await,
            old
        );
        assert_eq!(
            read(&db, "background_tasks", "payload_json", 1).await,
            cleanup
        );
    }

    #[tokio::test]
    async fn malformed_cleanup_rolls_back_updates_without_exposing_values() {
        let db = fixture().await;
        let stored = policy(connector(json!("https://download.example.test"), None));
        insert(
            &db,
            "storage_policies",
            "storage_config",
            "connector_id",
            CONNECTOR,
            1,
            &stored,
        )
        .await;
        let invalid =
            json!({"policy":{"connector_id":CONNECTOR,"storage_config":"PRIVATE-CONFIG"}});
        insert(
            &db,
            "background_tasks",
            "payload_json",
            "kind",
            "storage_policy_temp_cleanup",
            1,
            &invalid,
        )
        .await;
        let error = Migration.up(&SchemaManager::new(&db)).await.unwrap_err();
        assert!(!error.to_string().contains("PRIVATE-CONFIG"));
        assert_eq!(
            read(&db, "storage_policies", "storage_config", 1).await,
            stored
        );
    }
}
