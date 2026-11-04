//! One-time Kodo S3 -> native configuration gate. Never guesses a bucket mapping.
//! TODO: remove this upgrade bridge only when the released V1 upgrade window closes.
use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::{ConnectionTrait, TransactionTrait};
use serde_json::Value;

#[derive(DeriveMigrationName)]
pub struct Migration;
const CONNECTOR: &str = "asterdrive.storage.qiniu";

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Product startup decrypts credentials and verifies V1 namespaces online first.
        // This schema gate never discards unverified data, even from a schema-only CLI.
        let tx = manager.get_connection().begin().await?;
        for (table, column, filter_column, filter) in [
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
                let rows = tx
                    .query_all(
                        &Query::select()
                            .columns([Alias::new("id"), Alias::new(column)])
                            .from(Alias::new(table))
                            .and_where(Expr::col(Alias::new(filter_column)).eq(filter))
                            .and_where(Expr::col(Alias::new("id")).gt(cursor))
                            .order_by(Alias::new("id"), Order::Asc)
                            .limit(200)
                            .to_owned(),
                    )
                    .await?;
                if rows.is_empty() {
                    break;
                }
                for row in rows {
                    let id: i64 = row.try_get_by_index(0)?;
                    cursor = id;
                    let raw: String = row.try_get_by_index(1)?;
                    let mut value: Value =
                        serde_json::from_str(&raw).map_err(|_| invalid(table, id))?;
                    match table {
                        "storage_policies" => {
                            upgrade(
                                value
                                    .get_mut("connector")
                                    .ok_or_else(|| invalid(table, id))?,
                            )
                            .map_err(|_| invalid(table, id))?;
                        }
                        "remote_storage_targets" => {
                            upgrade(&mut value).map_err(|_| invalid(table, id))?;
                        }
                        "background_tasks" => {
                            if value
                                .pointer("/policy/connector_id")
                                .and_then(Value::as_str)
                                != Some(CONNECTOR)
                            {
                                continue;
                            }
                            let task = tx
                                .query_one(
                                    &Query::select()
                                        .column(Alias::new("status"))
                                        .from(Alias::new(table))
                                        .and_where(Expr::col(Alias::new("id")).eq(id))
                                        .to_owned(),
                                )
                                .await?
                                .ok_or_else(|| invalid(table, id))?;
                            if matches!(
                                task.try_get_by_index::<String>(0)?.as_str(),
                                "succeeded" | "canceled"
                            ) {
                                continue;
                            }
                            let raw = value
                                .pointer("/policy/storage_config")
                                .and_then(Value::as_str)
                                .ok_or_else(|| invalid(table, id))?;
                            let mut policy: Value =
                                serde_json::from_str(raw).map_err(|_| invalid(table, id))?;
                            upgrade(
                                policy
                                    .get_mut("connector")
                                    .ok_or_else(|| invalid(table, id))?,
                            )
                            .map_err(|_| invalid(table, id))?;
                        }
                        _ => return Err(invalid(table, id)),
                    }
                }
            }
        }
        tx.commit().await
    }
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for table in ["storage_policies", "remote_storage_targets"] {
            let row = manager
                .get_connection()
                .query_one(
                    &Query::select()
                        .column(Alias::new("id"))
                        .from(Alias::new(table))
                        .and_where(Expr::col(Alias::new("connector_id")).eq(CONNECTOR))
                        .limit(1)
                        .to_owned(),
                )
                .await?;
            if row.is_some() {
                return Err(DbErr::Migration("native Kodo configuration migration is irreversible; V1 S3 configuration was intentionally discarded".into()));
            }
        }
        // With no Kodo configuration there is nothing to reverse and no schema change.
        Ok(())
    }
}

fn invalid(table: &str, id: i64) -> DbErr {
    DbErr::Migration(format!(
        "Kodo configuration in {table} row {id} requires verified online upgrade; start the application with its credential encryption key before schema-only migration; no original values were changed or logged"
    ))
}

fn upgrade(config: &mut Value) -> Result<bool, ()> {
    if config.get("connector_id").and_then(Value::as_str) != Some(CONNECTOR)
        || config.get("format_version").and_then(Value::as_u64) != Some(1)
    {
        return Err(());
    }
    match config.get("schema_version").and_then(Value::as_u64) {
        Some(2) => return Ok(false),
        Some(1) => {}
        _ => return Err(()),
    }
    Err(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn schema_only_upgrade_refuses_unverified_v1_and_accepts_verified_native_configuration() {
        let mut old = json!({"format_version":1,"connector_id":CONNECTOR,"schema_version":1,"values":{"bucket":"legacy-s3","base_path":"retained"}});
        let original = old.clone();
        assert!(upgrade(&mut old).is_err());
        assert_eq!(old, original);
        let mut native = json!({"format_version":1,"connector_id":CONNECTOR,"schema_version":2,"values":{"bucket":"confirmed-native","download_origin":"https://native.example.test"}});
        assert!(!upgrade(&mut native).unwrap());
    }
    #[tokio::test]
    async fn schema_gate_rejects_corruption_and_validates_all_configuration_owners() {
        use sea_orm_migration::sea_orm::Database;
        for (table, column, raw) in [
            ("storage_policies", "storage_config", "not-json"),
            ("storage_policies", "storage_config", "{}"),
            ("remote_storage_targets", "connector_config", "{}"),
            (
                "background_tasks",
                "payload_json",
                r#"{"policy":{"connector_id":"asterdrive.storage.qiniu"}}"#,
            ),
            (
                "background_tasks",
                "payload_json",
                r#"{"policy":{"connector_id":"asterdrive.storage.qiniu","storage_config":"not-json"}}"#,
            ),
            (
                "background_tasks",
                "payload_json",
                r#"{"policy":{"connector_id":"asterdrive.storage.qiniu","storage_config":"{}"}}"#,
            ),
        ] {
            let db = Database::connect("sqlite::memory:").await.unwrap();
            for sql in [
                "CREATE TABLE storage_policies(id INTEGER,connector_id TEXT,storage_config TEXT)",
                "CREATE TABLE remote_storage_targets(id INTEGER,connector_id TEXT,connector_config TEXT)",
                "CREATE TABLE background_tasks(id INTEGER,kind TEXT,status TEXT,payload_json TEXT)",
            ] {
                db.execute_unprepared(sql).await.unwrap();
            }
            let mut insert = Query::insert();
            insert.into_table(Alias::new(table));
            match table {
                "background_tasks" => {
                    insert
                        .columns([
                            Alias::new("id"),
                            Alias::new("kind"),
                            Alias::new("status"),
                            Alias::new(column),
                        ])
                        .values_panic([
                            1.into(),
                            "storage_policy_temp_cleanup".into(),
                            "pending".into(),
                            raw.into(),
                        ]);
                }
                _ => {
                    insert
                        .columns([
                            Alias::new("id"),
                            Alias::new("connector_id"),
                            Alias::new(column),
                        ])
                        .values_panic([1.into(), CONNECTOR.into(), raw.into()]);
                }
            }
            db.execute(&insert.to_owned()).await.unwrap();
            let manager = SchemaManager::new(&db);
            assert!(Migration.up(&manager).await.is_err(), "{table}: {raw}");
            if table != "background_tasks" {
                assert!(Migration.down(&manager).await.is_err());
            }
        }
        for mut value in [
            json!({"format_version":1,"connector_id":"other","schema_version":2}),
            json!({"format_version":2,"connector_id":CONNECTOR,"schema_version":2}),
            json!({"format_version":1,"connector_id":CONNECTOR,"schema_version":3}),
        ] {
            assert!(upgrade(&mut value).is_err());
        }
    }
}
