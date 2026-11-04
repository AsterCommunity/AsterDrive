//! One-time online V1 -> V2 conversion, before listeners or credential reload.
//! Network discovery finishes before one atomic database compare-and-swap commit.
use crate::storage::drivers::qiniu::{
    QiniuStaticCredentials,
    upgrade::{LegacyConfig, NativeLocation, NativeUpgradeResolver, OfficialResolver},
};
use crate::{
    config::Config,
    errors::{AsterError, Result},
};
use sea_orm::{
    ConnectionTrait, DatabaseConnection, ExprTrait, TransactionTrait,
    sea_query::{Alias, Expr, Order, Query},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
const CONNECTOR: &str = "asterdrive.storage.qiniu";

struct Change {
    table: &'static str,
    column: &'static str,
    id: i64,
    original: String,
    updated: String,
    credential: Credential,
}
struct Credential {
    table: &'static str,
    id_column: &'static str,
    id: i64,
    ciphertext: String,
}
#[derive(Deserialize)]
struct Credentials {
    qiniu_access_key: String,
    qiniu_secret_key: String,
}

pub(crate) async fn convert(db: &DatabaseConnection, config: &Config) -> Result<()> {
    convert_with(
        db,
        &config.auth.storage_credential_secret_key,
        &OfficialResolver,
    )
    .await
}

async fn convert_with(
    db: &DatabaseConnection,
    key: &str,
    resolver: &dyn NativeUpgradeResolver,
) -> Result<()> {
    if !aster_drive_migration::qiniu_online_upgrade_schema_ready(db).await? {
        return Ok(());
    }
    let mut changes = Vec::new();
    let mut locations =
        BTreeMap::<(String, String, String, String, String, String), NativeLocation>::new();
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
            let rows = db
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
                let original: String = row.try_get_by_index(1)?;
                let mut value: Value = serde_json::from_str(&original)
                    .map_err(|_| failure(table, id, "invalid configuration envelope"))?;
                let (mut envelope, policy_id, snapshot) = match table {
                    "storage_policies" => (
                        value
                            .get("connector")
                            .cloned()
                            .ok_or_else(|| failure(table, id, "missing connector envelope"))?,
                        id,
                        None,
                    ),
                    "remote_storage_targets" => (value.clone(), id, None),
                    "background_tasks" => {
                        if value
                            .pointer("/policy/connector_id")
                            .and_then(Value::as_str)
                            != Some(CONNECTOR)
                        {
                            continue;
                        }
                        let status = read_string(db, table, "status", "id", id)
                            .await?
                            .ok_or_else(|| failure(table, id, "task disappeared"))?;
                        if matches!(status.as_str(), "succeeded" | "canceled") {
                            continue;
                        }
                        let raw = value
                            .pointer("/policy/storage_config")
                            .and_then(Value::as_str)
                            .ok_or_else(|| failure(table, id, "missing cleanup configuration"))?;
                        let policy: Value = serde_json::from_str(raw)
                            .map_err(|_| failure(table, id, "invalid cleanup configuration"))?;
                        (
                            policy
                                .get("connector")
                                .cloned()
                                .ok_or_else(|| failure(table, id, "missing cleanup connector"))?,
                            value
                                .pointer("/policy/id")
                                .and_then(Value::as_i64)
                                .ok_or_else(|| failure(table, id, "missing cleanup policy id"))?,
                            value
                                .pointer("/driver_snapshot/payload/ciphertext")
                                .and_then(Value::as_str)
                                .map(str::to_string),
                        )
                    }
                    _ => return Err(failure(table, id, "unknown upgrade owner")),
                };
                if envelope.get("connector_id").and_then(Value::as_str) != Some(CONNECTOR)
                    || envelope.get("format_version").and_then(Value::as_u64) != Some(1)
                {
                    return Err(failure(table, id, "invalid connector identity"));
                }
                match envelope.get("schema_version").and_then(Value::as_u64) {
                    Some(2) => continue,
                    Some(1) => {}
                    _ => return Err(failure(table, id, "unknown connector version")),
                }
                if table == "background_tasks"
                    && value
                        .get("multipart_uploads")
                        .and_then(Value::as_array)
                        .is_some_and(|v| !v.is_empty())
                {
                    return Err(failure(
                        table,
                        id,
                        "finish or cancel legacy S3 multipart cleanup before upgrading",
                    ));
                }
                let old: LegacyConfig = serde_json::from_value(
                    envelope
                        .get("values")
                        .cloned()
                        .ok_or_else(|| failure(table, id, "missing legacy values"))?,
                )
                .map_err(|_| failure(table, id, "invalid legacy fields"))?;
                if table == "storage_policies" {
                    let active = db
                        .query_one(
                            &Query::select()
                                .column(Alias::new("id"))
                                .from(Alias::new("upload_sessions"))
                                .and_where(Expr::col(Alias::new("policy_id")).eq(id))
                                .and_where(Expr::col(Alias::new("status")).ne("completed"))
                                .limit(1)
                                .to_owned(),
                        )
                        .await?;
                    if active.is_some() {
                        return Err(failure(
                            table,
                            id,
                            "finish or cancel old Kodo uploads before upgrading",
                        ));
                    }
                }
                let credential = if let Some(ciphertext) = snapshot {
                    Credential {
                        table: "background_tasks",
                        id_column: "id",
                        id,
                        ciphertext,
                    }
                } else {
                    let (credential_table, id_column) = if table == "storage_policies" {
                        ("storage_policy_connector_credentials", "policy_id")
                    } else {
                        ("remote_storage_target_credentials", "target_id")
                    };
                    let row = db
                        .query_one(
                            &Query::select()
                                .columns([
                                    Alias::new("ciphertext"),
                                    Alias::new("connector_id"),
                                    Alias::new("schema_version"),
                                ])
                                .from(Alias::new(credential_table))
                                .and_where(Expr::col(Alias::new(id_column)).eq(id))
                                .to_owned(),
                        )
                        .await?
                        .ok_or_else(|| failure(table, id, "missing encrypted credentials"))?;
                    if row.try_get_by_index::<String>(1)? != CONNECTOR
                        || row.try_get_by_index::<i32>(2)? != 1
                    {
                        return Err(failure(table, id, "credential identity mismatch"));
                    }
                    Credential {
                        table: credential_table,
                        id_column,
                        id,
                        ciphertext: row.try_get_by_index(0)?,
                    }
                };
                let plaintext=crate::services::storage_policy::credential::crypto::decrypt_connector_credential(key,policy_id,CONNECTOR,1,&credential.ciphertext).map_err(|_|failure(table,id,"cannot decrypt stored credentials"))?;
                let credentials: Credentials = serde_json::from_str(&plaintext)
                    .map_err(|_| failure(table, id, "invalid stored credentials"))?;
                let cache_key = (
                    credentials.qiniu_access_key.clone(),
                    old.bucket.clone(),
                    old.s3_region.clone(),
                    old.base_path.clone(),
                    aster_forge_crypto::sha256_hex(credentials.qiniu_secret_key.as_bytes()),
                    old.endpoint.clone(),
                );
                let location = match locations.get(&cache_key) {
                    Some(value) => value.clone(),
                    None => {
                        let value = tokio::time::timeout(
                            std::time::Duration::from_secs(300),
                            resolver.resolve(
                                &old,
                                QiniuStaticCredentials {
                                    access_key: credentials.qiniu_access_key,
                                    secret_key: credentials.qiniu_secret_key,
                                },
                            ),
                        )
                        .await
                        .map_err(|_| failure(table, id, "online identity verification timed out"))?
                        .map_err(|error| {
                            failure(
                                table,
                                id,
                                &format!(
                                    "online identity verification failed ({})",
                                    error.kind().as_str()
                                ),
                            )
                        })?;
                        locations.insert(cache_key, value.clone());
                        value
                    }
                };
                envelope["schema_version"] = json!(2);
                envelope["values"] = json!({"bucket":location.bucket,"base_path":old.base_path,"download_origin":location.download_origin,"download_delivery":"origin","download_base_url":null,"download_auth_validity_seconds":180,
                    "provider_resumable_upload_strategy":match old.object_storage_upload_strategy {aster_drive_model::types::ObjectStorageUploadStrategy::Presigned=>"frontend_direct",aster_drive_model::types::ObjectStorageUploadStrategy::RelayStream=>"server_relay"},
                    "object_storage_download_strategy":old.object_storage_download_strategy});
                match table {
                    "storage_policies" => value["connector"] = envelope,
                    "remote_storage_targets" => value = envelope,
                    "background_tasks" => {
                        let mut policy: Value = serde_json::from_str(
                            value
                                .pointer("/policy/storage_config")
                                .and_then(Value::as_str)
                                .ok_or_else(|| failure(table, id, "invalid cleanup policy"))?,
                        )
                        .map_err(|_| failure(table, id, "invalid cleanup policy"))?;
                        policy["connector"] = envelope;
                        value["policy"]["storage_config"] = json!(policy.to_string());
                    }
                    _ => return Err(failure(table, id, "unknown upgrade owner")),
                }
                changes.push(Change {
                    table,
                    column,
                    id,
                    original,
                    updated: value.to_string(),
                    credential,
                });
            }
        }
    }
    if changes.is_empty() {
        return Ok(());
    }
    let tx = db.begin().await?;
    for change in changes {
        if change.credential.table != "background_tasks" {
            let current = read_string(
                &tx,
                change.credential.table,
                "ciphertext",
                change.credential.id_column,
                change.credential.id,
            )
            .await?;
            if current.as_deref() != Some(change.credential.ciphertext.as_str()) {
                return Err(failure(
                    change.table,
                    change.id,
                    "credentials changed during online verification; retry upgrade",
                ));
            }
            let identity = tx
                .query_one(
                    &Query::select()
                        .columns([Alias::new("connector_id"), Alias::new("schema_version")])
                        .from(Alias::new(change.credential.table))
                        .and_where(
                            Expr::col(Alias::new(change.credential.id_column))
                                .eq(change.credential.id),
                        )
                        .to_owned(),
                )
                .await?
                .ok_or_else(|| {
                    failure(
                        change.table,
                        change.id,
                        "credentials disappeared during verification",
                    )
                })?;
            if identity.try_get_by_index::<String>(0)? != CONNECTOR
                || identity.try_get_by_index::<i32>(1)? != 1
            {
                return Err(failure(
                    change.table,
                    change.id,
                    "credential identity changed during verification",
                ));
            }
        }
        let updated = tx
            .execute(
                &Query::update()
                    .table(Alias::new(change.table))
                    .value(Alias::new(change.column), change.updated)
                    .and_where(Expr::col(Alias::new("id")).eq(change.id))
                    .and_where(Expr::col(Alias::new(change.column)).eq(change.original))
                    .and_where(
                        Expr::col(Alias::new(if change.table == "background_tasks" {
                            "kind"
                        } else {
                            "connector_id"
                        }))
                        .eq(if change.table == "background_tasks" {
                            "storage_policy_temp_cleanup"
                        } else {
                            CONNECTOR
                        }),
                    )
                    .to_owned(),
            )
            .await?;
        if updated.rows_affected() != 1 {
            return Err(failure(
                change.table,
                change.id,
                "configuration changed during online verification; retry upgrade",
            ));
        }
    }
    tx.commit().await?;
    Ok(())
}
async fn read_string<C: ConnectionTrait>(
    db: &C,
    table: &str,
    column: &str,
    id_column: &str,
    id: i64,
) -> Result<Option<String>> {
    db.query_one(
        &Query::select()
            .column(Alias::new(column))
            .from(Alias::new(table))
            .and_where(Expr::col(Alias::new(id_column)).eq(id))
            .to_owned(),
    )
    .await?
    .map(|row| row.try_get_by_index(0).map_err(Into::into))
    .transpose()
}
fn failure(table: &str, id: i64, reason: &str) -> AsterError {
    AsterError::database_operation(format!(
        "native Kodo upgrade: {table} row {id}: {reason}; original configuration retained, no values logged"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    const KEY: &str = "qiniu-online-upgrade-master-key-32bytes";
    struct Resolver {
        calls: AtomicUsize,
        fail: Option<usize>,
    }
    #[async_trait::async_trait]
    impl NativeUpgradeResolver for Resolver {
        async fn resolve(
            &self,
            old: &LegacyConfig,
            credentials: QiniuStaticCredentials,
        ) -> aster_drive_storage::Result<NativeLocation> {
            assert_eq!(credentials.access_key, "AK");
            assert_eq!(credentials.secret_key, "secret-SK");
            assert_eq!(old.base_path, "existing/prefix");
            let call = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
            if self.fail == Some(call) {
                return Err(aster_drive_storage::storage_driver_error(
                    aster_drive_storage::StorageErrorKind::Transient,
                    "injected provider outage",
                ));
            }
            Ok(NativeLocation {
                bucket: format!("native-{}", old.bucket),
                download_origin: "https://provider-source.example.test".into(),
            })
        }
    }
    fn resolver(fail: Option<usize>) -> Resolver {
        Resolver {
            calls: AtomicUsize::new(0),
            fail,
        }
    }
    fn old(bucket: &str) -> Value {
        json!({"format_version":1,"connector_id":CONNECTOR,"schema_version":1,"values":{"endpoint":"https://s3.cn-east-1.qiniucs.com","bucket":bucket,"s3_region":"cn-east-1","base_path":"existing/prefix","object_storage_upload_strategy":"presigned","object_storage_download_strategy":"relay_stream"}})
    }
    async fn db() -> DatabaseConnection {
        let db = sea_orm::Database::connect("sqlite::memory:").await.unwrap();
        for sql in [
            "CREATE TABLE storage_policies(id INTEGER PRIMARY KEY,connector_id TEXT,storage_config TEXT)",
            "CREATE TABLE remote_storage_targets(id INTEGER PRIMARY KEY,connector_id TEXT,connector_config TEXT)",
            "CREATE TABLE storage_policy_connector_credentials(policy_id INTEGER PRIMARY KEY,connector_id TEXT,schema_version INTEGER,ciphertext TEXT)",
            "CREATE TABLE remote_storage_target_credentials(target_id INTEGER PRIMARY KEY,connector_id TEXT,schema_version INTEGER,ciphertext TEXT)",
            "CREATE TABLE background_tasks(id INTEGER PRIMARY KEY,kind TEXT,status TEXT,payload_json TEXT)",
            "CREATE TABLE upload_sessions(id TEXT PRIMARY KEY,policy_id INTEGER,status TEXT)",
        ] {
            db.execute_unprepared(sql).await.unwrap();
        }
        db
    }
    fn encrypted(id: i64) -> String {
        crate::services::storage_policy::credential::crypto::encrypt_connector_credential(
            KEY,
            id,
            CONNECTOR,
            1,
            &json!({"qiniu_access_key":"AK","qiniu_secret_key":"secret-SK"}).to_string(),
        )
        .unwrap()
    }
    async fn insert(db: &DatabaseConnection, table: &str, id: i64, bucket: &str) -> String {
        let (column, credentials, id_column, value) = if table == "storage_policies" {
            (
                "storage_config",
                "storage_policy_connector_credentials",
                "policy_id",
                json!({"format_version":1,"connector":old(bucket),"behavior":{"format_version":1,"schema_version":1,"values":{"content_dedup":false}}}),
            )
        } else {
            (
                "connector_config",
                "remote_storage_target_credentials",
                "target_id",
                old(bucket),
            )
        };
        let raw = value.to_string();
        db.execute(
            &Query::insert()
                .into_table(Alias::new(table))
                .columns([
                    Alias::new("id"),
                    Alias::new("connector_id"),
                    Alias::new(column),
                ])
                .values_panic([id.into(), CONNECTOR.into(), raw.clone().into()])
                .to_owned(),
        )
        .await
        .unwrap();
        db.execute(
            &Query::insert()
                .into_table(Alias::new(credentials))
                .columns([
                    Alias::new(id_column),
                    Alias::new("connector_id"),
                    Alias::new("schema_version"),
                    Alias::new("ciphertext"),
                ])
                .values_panic([id.into(), CONNECTOR.into(), 1.into(), encrypted(id).into()])
                .to_owned(),
        )
        .await
        .unwrap();
        raw
    }
    #[tokio::test]
    async fn converts_policy_target_and_cleanup_atomically_with_existing_credentials() {
        let db = db().await;
        insert(&db, "storage_policies", 1, "s3-alias").await;
        insert(&db, "remote_storage_targets", 2, "remote-alias").await;
        let cipher = read_string(
            &db,
            "storage_policy_connector_credentials",
            "ciphertext",
            "policy_id",
            1,
        )
        .await
        .unwrap();
        let snapshot = json!({"policy":{"id":3,"connector_id":CONNECTOR,"storage_config":json!({"connector":old("deleted-alias")}).to_string()},"driver_snapshot":{"connector_id":CONNECTOR,"schema_version":1,"payload":{"ciphertext":encrypted(3)}},"temp_keys":["owned-object"],"multipart_uploads":[]});
        db.execute(
            &Query::insert()
                .into_table(Alias::new("background_tasks"))
                .columns([
                    Alias::new("id"),
                    Alias::new("kind"),
                    Alias::new("status"),
                    Alias::new("payload_json"),
                ])
                .values_panic([
                    3.into(),
                    "storage_policy_temp_cleanup".into(),
                    "pending".into(),
                    snapshot.to_string().into(),
                ])
                .to_owned(),
        )
        .await
        .unwrap();
        let resolver = resolver(None);
        convert_with(&db, KEY, &resolver).await.unwrap();
        assert_eq!(resolver.calls.load(Ordering::SeqCst), 3);
        let policy: Value = serde_json::from_str(
            &read_string(&db, "storage_policies", "storage_config", "id", 1)
                .await
                .unwrap()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(policy["connector"]["schema_version"], 2);
        assert_eq!(policy["connector"]["values"]["bucket"], "native-s3-alias");
        assert_eq!(
            policy["connector"]["values"]["download_origin"],
            "https://provider-source.example.test"
        );
        assert_eq!(
            policy["connector"]["values"]["base_path"],
            "existing/prefix"
        );
        assert_eq!(
            policy["connector"]["values"]["provider_resumable_upload_strategy"],
            "frontend_direct"
        );
        assert!(policy["connector"]["values"].get("endpoint").is_none());
        assert_eq!(
            cipher,
            read_string(
                &db,
                "storage_policy_connector_credentials",
                "ciphertext",
                "policy_id",
                1
            )
            .await
            .unwrap()
        );
        let target: Value = serde_json::from_str(
            &read_string(&db, "remote_storage_targets", "connector_config", "id", 2)
                .await
                .unwrap()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(target["values"]["bucket"], "native-remote-alias");
        let task: Value = serde_json::from_str(
            &read_string(&db, "background_tasks", "payload_json", "id", 3)
                .await
                .unwrap()
                .unwrap(),
        )
        .unwrap();
        let config: Value =
            serde_json::from_str(task["policy"]["storage_config"].as_str().unwrap()).unwrap();
        assert_eq!(
            config["connector"]["values"]["bucket"],
            "native-deleted-alias"
        );
        assert_eq!(task["temp_keys"], snapshot["temp_keys"]);
        convert_with(&db, KEY, &resolver).await.unwrap();
        assert_eq!(
            resolver.calls.load(Ordering::SeqCst),
            3,
            "native rows must not access the network on later restarts"
        );
    }
    #[tokio::test]
    async fn provider_failure_does_not_discard_any_original_configuration() {
        let db = db().await;
        let original = insert(&db, "storage_policies", 1, "first").await;
        let target = insert(&db, "remote_storage_targets", 2, "second").await;
        let error = convert_with(&db, KEY, &resolver(Some(2)))
            .await
            .unwrap_err();
        assert!(!error.to_string().contains("secret-SK"));
        assert_eq!(
            Some(original),
            read_string(&db, "storage_policies", "storage_config", "id", 1)
                .await
                .unwrap()
        );
        assert_eq!(
            Some(target),
            read_string(&db, "remote_storage_targets", "connector_config", "id", 2)
                .await
                .unwrap()
        );
    }
    #[tokio::test]
    async fn active_upload_and_invalid_encryption_key_stop_before_network_access() {
        let db = db().await;
        let original = insert(&db, "storage_policies", 1, "first").await;
        db.execute_unprepared("INSERT INTO upload_sessions VALUES('active',1,'presigned')")
            .await
            .unwrap();
        let resolver = resolver(None);
        assert!(convert_with(&db, KEY, &resolver).await.is_err());
        assert_eq!(resolver.calls.load(Ordering::SeqCst), 0);
        db.execute_unprepared("DELETE FROM upload_sessions")
            .await
            .unwrap();
        assert!(
            convert_with(&db, "wrong encryption key but long enough", &resolver)
                .await
                .is_err()
        );
        assert_eq!(resolver.calls.load(Ordering::SeqCst), 0);
        assert_eq!(
            Some(original),
            read_string(&db, "storage_policies", "storage_config", "id", 1)
                .await
                .unwrap()
        );
    }
    struct ConcurrentChange {
        db: DatabaseConnection,
    }
    #[async_trait::async_trait]
    impl NativeUpgradeResolver for ConcurrentChange {
        async fn resolve(
            &self,
            _old: &LegacyConfig,
            _credentials: QiniuStaticCredentials,
        ) -> aster_drive_storage::Result<NativeLocation> {
            self.db
                .execute_unprepared(
                    "UPDATE storage_policies SET storage_config='changed-by-administrator'",
                )
                .await
                .unwrap();
            Ok(NativeLocation {
                bucket: "native".into(),
                download_origin: "https://source.example.test".into(),
            })
        }
    }
    #[tokio::test]
    async fn concurrent_configuration_edits_are_not_overwritten() {
        let db = db().await;
        insert(&db, "storage_policies", 1, "first").await;
        assert!(
            convert_with(&db, KEY, &ConcurrentChange { db: db.clone() })
                .await
                .is_err()
        );
        assert_eq!(
            read_string(&db, "storage_policies", "storage_config", "id", 1)
                .await
                .unwrap()
                .as_deref(),
            Some("changed-by-administrator")
        );
    }

    #[tokio::test]
    async fn native_multipart_cleanup_does_not_trigger_legacy_upgrade_on_restart() {
        let db = db().await;
        let mut native = old("already-native");
        native["schema_version"] = json!(2);
        let payload = json!({"policy":{"id":8,"connector_id":CONNECTOR,"storage_config":json!({"connector":native}).to_string()},"multipart_uploads":[{"temp_key":"owned","multipart_id":"native-session"}],"temp_keys":[]});
        let original = payload.to_string();
        db.execute(
            &Query::insert()
                .into_table(Alias::new("background_tasks"))
                .columns([
                    Alias::new("id"),
                    Alias::new("kind"),
                    Alias::new("status"),
                    Alias::new("payload_json"),
                ])
                .values_panic([
                    8.into(),
                    "storage_policy_temp_cleanup".into(),
                    "pending".into(),
                    original.clone().into(),
                ])
                .to_owned(),
        )
        .await
        .unwrap();
        let resolver = resolver(None);
        convert_with(&db, KEY, &resolver).await.unwrap();
        assert_eq!(resolver.calls.load(Ordering::SeqCst), 0);
        assert_eq!(
            read_string(&db, "background_tasks", "payload_json", "id", 8)
                .await
                .unwrap()
                .as_deref(),
            Some(original.as_str())
        );
    }

    #[tokio::test]
    async fn legacy_s3_multipart_cleanup_blocks_conversion_before_network_access() {
        let db = db().await;
        let payload = json!({"policy":{"id":8,"connector_id":CONNECTOR,"storage_config":json!({"connector":old("s3-alias")}).to_string()},"multipart_uploads":[{"temp_key":"owned","multipart_id":"legacy-s3-session"}],"temp_keys":[]});
        let original = payload.to_string();
        db.execute(
            &Query::insert()
                .into_table(Alias::new("background_tasks"))
                .columns([
                    Alias::new("id"),
                    Alias::new("kind"),
                    Alias::new("status"),
                    Alias::new("payload_json"),
                ])
                .values_panic([
                    8.into(),
                    "storage_policy_temp_cleanup".into(),
                    "pending".into(),
                    original.clone().into(),
                ])
                .to_owned(),
        )
        .await
        .unwrap();
        let resolver = resolver(None);
        let error = convert_with(&db, KEY, &resolver).await.unwrap_err();
        assert!(error.to_string().contains("legacy S3 multipart cleanup"));
        assert_eq!(resolver.calls.load(Ordering::SeqCst), 0);
        assert_eq!(
            read_string(&db, "background_tasks", "payload_json", "id", 8)
                .await
                .unwrap()
                .as_deref(),
            Some(original.as_str())
        );
    }
}
