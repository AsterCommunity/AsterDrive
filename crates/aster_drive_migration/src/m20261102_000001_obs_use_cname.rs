//! Replace the two OBS addressing choices with the shared CNAME boolean contract.
//! Includes remote targets and deleted-policy cleanup snapshots, so deferred
//! cleanup continues to address the same bucket after upgrade or downgrade.

use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::{ConnectionTrait, TransactionTrait};
use serde_json::Value;

#[derive(DeriveMigrationName)]
pub struct Migration;

const CONNECTOR: &str = "asterdrive.storage.huawei_obs";
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
        "invalid OBS CNAME configuration in {table} row {id}; no configuration values were logged"
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
    if config.get("connector_id").and_then(Value::as_str) != Some(CONNECTOR) {
        return Err(());
    }
    if config.get("format_version").and_then(Value::as_u64) != Some(1)
        || config.get("schema_version").and_then(Value::as_u64) != Some(1)
    {
        return Err(());
    }
    let values = config
        .get_mut("values")
        .and_then(Value::as_object_mut)
        .ok_or(())?;
    let old = values
        .get("obs_addressing_mode")
        .map(|mode| match mode.as_str() {
            Some("virtual_hosted") => Ok(false),
            Some("custom_domain") => Ok(true),
            _ => Err(()),
        })
        .transpose()?;
    let new = values
        .get("obs_use_cname")
        .map(|value| value.as_bool().ok_or(()))
        .transpose()?;
    if old.zip(new).is_some_and(|(old, new)| old != new) {
        return Err(());
    }
    let enabled = new.or(old).unwrap_or(false);
    if upgrade {
        let changed = values.remove("obs_addressing_mode").is_some() || new.is_none();
        values.insert("obs_use_cname".to_string(), Value::Bool(enabled));
        Ok(changed)
    } else {
        let changed = values.remove("obs_use_cname").is_some() || old.is_none();
        values.insert(
            "obs_addressing_mode".to_string(),
            Value::String(
                if enabled {
                    "custom_domain"
                } else {
                    "virtual_hosted"
                }
                .to_string(),
            ),
        );
        Ok(changed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm_migration::sea_orm::{Database, DatabaseConnection};
    use serde_json::json;

    fn connector(mode: Option<&str>) -> Value {
        let mut config = json!({
            "format_version": 1, "connector_id": CONNECTOR, "schema_version": 1,
            "values": {"endpoint":"https://files.example.test", "bucket":"archive", "base_path":"tenant/a"}
        });
        if let Some(mode) = mode {
            config["values"]["obs_addressing_mode"] = json!(mode);
        }
        config
    }

    fn policy(config: Value) -> Value {
        json!({"format_version":1,"connector":config,"behavior":{"format_version":1,"schema_version":1,"values":{}}})
    }

    #[test]
    fn conversion_is_idempotent_and_preserves_both_modes_and_unrelated_values() {
        for (mode, expected) in [
            (Some("custom_domain"), true),
            (Some("virtual_hosted"), false),
            (None, false),
        ] {
            let mut config = connector(mode);
            assert!(rewrite_connector(&mut config, true).unwrap());
            assert_eq!(config["values"]["obs_use_cname"], expected);
            assert!(config["values"].get("obs_addressing_mode").is_none());
            assert_eq!(config["values"]["base_path"], "tenant/a");
            assert!(!rewrite_connector(&mut config, true).unwrap());
            assert!(rewrite_connector(&mut config, false).unwrap());
            assert_eq!(
                config["values"]["obs_addressing_mode"],
                mode.unwrap_or("virtual_hosted")
            );
            assert!(config["values"].get("obs_use_cname").is_none());
            assert!(!rewrite_connector(&mut config, false).unwrap());
        }
    }

    #[test]
    fn invalid_or_conflicting_modes_are_rejected_without_logging_values() {
        for bad in [json!("unknown"), json!(true), Value::Null] {
            let mut config = connector(None);
            config["values"]["obs_addressing_mode"] = bad;
            assert!(rewrite_connector(&mut config, true).is_err());
        }
        let mut config = connector(Some("custom_domain"));
        config["values"]["obs_use_cname"] = json!(false);
        assert!(rewrite_connector(&mut config, true).is_err());
        config["values"]["obs_use_cname"] = json!(true);
        assert!(rewrite_connector(&mut config, true).unwrap());
        let mut config = connector(None);
        config["values"]["obs_use_cname"] = json!("true");
        assert!(rewrite_connector(&mut config, true).is_err());
        assert!(
            !invalid_row("storage_policies", 1)
                .to_string()
                .contains("files.example")
        );
    }

    async fn fixture() -> DatabaseConnection {
        // Optional URL must point at a fresh, disposable test database.
        let test_url = std::env::var("ASTER_OBS_CNAME_TEST_DATABASE_URL")
            .unwrap_or_else(|_| "sqlite::memory:".to_string());
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
        config: &Value,
    ) {
        db.execute(
            &Query::insert()
                .into_table(Alias::new(table))
                .columns([Alias::new("id"), Alias::new(filter), Alias::new(column)])
                .values_panic([id.into(), filter_value.into(), config.to_string().into()])
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
    async fn migration_round_trips_policies_targets_and_deleted_policy_cleanup_across_batches() {
        let db = fixture().await;
        let original = policy(connector(Some("custom_domain")));
        for id in 1..=205 {
            insert(
                &db,
                "storage_policies",
                "storage_config",
                "connector_id",
                CONNECTOR,
                id,
                &original,
            )
            .await;
        }
        let remote = connector(Some("virtual_hosted"));
        insert(
            &db,
            "remote_storage_targets",
            "connector_config",
            "connector_id",
            CONNECTOR,
            1,
            &remote,
        )
        .await;
        let cleanup = json!({"policy":{"connector_id":CONNECTOR,"storage_config":original.to_string()},"driver_snapshot":{"ciphertext":"unchanged"},"temp_keys":["uploads/a"],"multipart_uploads":[]});
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
        let unrelated = json!({"connector":{"values":{"obs_addressing_mode":"unrelated"}}});
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
        for id in [1, 200, 205] {
            let current = read(&db, "storage_policies", "storage_config", id).await;
            assert_eq!(current["connector"]["values"]["obs_use_cname"], true);
            assert!(
                current["connector"]["values"]
                    .get("obs_addressing_mode")
                    .is_none()
            );
        }
        assert_eq!(
            read(&db, "remote_storage_targets", "connector_config", 1).await["values"]["obs_use_cname"],
            false
        );
        let current = read(&db, "background_tasks", "payload_json", 1).await;
        let config: Value =
            serde_json::from_str(current["policy"]["storage_config"].as_str().unwrap()).unwrap();
        assert_eq!(config["connector"]["values"]["obs_use_cname"], true);
        assert_eq!(current["driver_snapshot"], cleanup["driver_snapshot"]);
        assert_eq!(
            read(&db, "storage_policies", "storage_config", 206).await,
            unrelated
        );
        Migration.down(&manager).await.unwrap();
        assert_eq!(
            read(&db, "storage_policies", "storage_config", 205).await,
            original
        );
        assert_eq!(
            read(&db, "remote_storage_targets", "connector_config", 1).await,
            remote
        );
        assert_eq!(
            read(&db, "background_tasks", "payload_json", 1).await,
            cleanup
        );
    }

    #[tokio::test]
    async fn invalid_cleanup_snapshot_rolls_back_earlier_policy_and_target_updates() {
        let db = fixture().await;
        let original = policy(connector(Some("custom_domain")));
        insert(
            &db,
            "storage_policies",
            "storage_config",
            "connector_id",
            CONNECTOR,
            1,
            &original,
        )
        .await;
        let remote = connector(Some("virtual_hosted"));
        insert(
            &db,
            "remote_storage_targets",
            "connector_config",
            "connector_id",
            CONNECTOR,
            1,
            &remote,
        )
        .await;
        let invalid = policy(connector(Some("unrecognized")));
        let cleanup =
            json!({"policy":{"connector_id":CONNECTOR,"storage_config":invalid.to_string()}});
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
        assert!(Migration.up(&SchemaManager::new(&db)).await.is_err());
        assert_eq!(
            read(&db, "storage_policies", "storage_config", 1).await,
            original
        );
        assert_eq!(
            read(&db, "remote_storage_targets", "connector_config", 1).await,
            remote
        );
        assert_eq!(
            read(&db, "background_tasks", "payload_json", 1).await,
            cleanup
        );
    }
}
