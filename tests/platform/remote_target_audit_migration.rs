//! Remote-target audit names migrate logs and action selection as one DML transaction.
use crate::common;
use aster_drive_migration::{CurrentMigrator, Migrator, MigratorTrait};
use sea_orm::sea_query::{Alias, Expr, ExprTrait, Order, Query};
use sea_orm::{ConnectionTrait, Database, DatabaseConnection};

const MIGRATION: &str = "m20261105_000002_remote_target_audit_names";
const ACTIONS: &[(&str, &str)] = &[
    (
        "admin_create_remote_ingress_profile",
        "admin_create_remote_storage_target",
    ),
    (
        "admin_update_remote_ingress_profile",
        "admin_update_remote_storage_target",
    ),
    (
        "admin_delete_remote_ingress_profile",
        "admin_delete_remote_storage_target",
    ),
    (
        "follower_ingress_profile_create",
        "follower_storage_target_create",
    ),
    (
        "follower_ingress_profile_update",
        "follower_storage_target_update",
    ),
    (
        "follower_ingress_profile_delete",
        "follower_storage_target_delete",
    ),
];
const DETAILS: &str =
    r#"{ "target_key": "old-target", "driver_type": "s3", "note": "remote_ingress_profile" }"#;

async fn fixture(db: &DatabaseConnection, config: &str) {
    let position = CurrentMigrator::migrations()
        .iter()
        .position(|m| m.name() == MIGRATION)
        .unwrap();
    CurrentMigrator::up(db, Some(u32::try_from(position).unwrap()))
        .await
        .unwrap();
    for (index, (old, new)) in ACTIONS.iter().enumerate() {
        for (offset, action, entity) in [
            (0_i64, *old, "remote_ingress_profile"),
            (100_i64, *new, "remote_storage_target"),
        ] {
            aster_forge_db::create_audit_log_row(
                db,
                aster_forge_db::AuditLogCreate {
                    user_id: 0,
                    action: action.into(),
                    entity_type: entity.into(),
                    entity_id: Some(offset + i64::try_from(index).unwrap()),
                    entity_name: Some("Historical target snapshot".into()),
                    details: Some(DETAILS.into()),
                    ip_address: Some("127.0.0.1".into()),
                    user_agent: Some("historic-agent".into()),
                    created_at: chrono::Utc::now(),
                },
            )
            .await
            .unwrap();
        }
    }
    aster_forge_db::create_audit_log_row(
        db,
        aster_forge_db::AuditLogCreate {
            user_id: 0,
            action: "server_start".into(),
            entity_type: "system_config".into(),
            entity_id: None,
            entity_name: None,
            details: None,
            ip_address: None,
            user_agent: None,
            created_at: chrono::Utc::now(),
        },
    )
    .await
    .unwrap();
    db.execute(
        &Query::insert()
            .into_table(Alias::new("system_config"))
            .columns(["key", "value", "category", "description", "updated_at"].map(Alias::new))
            .values_panic([
                "audit_log_recorded_actions".into(),
                config.into(),
                "audit".into(),
                "Stored action selection".into(),
                chrono::Utc::now().into(),
            ])
            .to_owned(),
    )
    .await
    .unwrap();
}

async fn read_config(db: &DatabaseConnection) -> String {
    db.query_one(
        &Query::select()
            .column(Alias::new("value"))
            .from(Alias::new("system_config"))
            .and_where(Expr::col(Alias::new("key")).eq("audit_log_recorded_actions"))
            .to_owned(),
    )
    .await
    .unwrap()
    .unwrap()
    .try_get_by_index(0)
    .unwrap()
}

async fn logs(db: &DatabaseConnection) -> Vec<serde_json::Value> {
    db.query_all(
        &Query::select()
            .columns(
                [
                    "id",
                    "user_id",
                    "action",
                    "entity_type",
                    "entity_id",
                    "entity_name",
                    "details",
                    "ip_address",
                    "user_agent",
                    "created_at",
                ]
                .map(Alias::new),
            )
            .from(Alias::new("audit_logs"))
            .order_by(Alias::new("id"), Order::Asc)
            .to_owned(),
    )
    .await
    .unwrap()
    .iter()
    .map(|row| {
        serde_json::json!({
            "id":row.try_get::<i64>("", "id").unwrap(),
            "user_id":row.try_get::<i64>("", "user_id").unwrap(),
            "action":row.try_get::<String>("", "action").unwrap(),
            "entity_type":row.try_get::<String>("", "entity_type").unwrap(),
            "entity_id":row.try_get::<Option<i64>>("", "entity_id").unwrap(),
            "entity_name":row.try_get::<Option<String>>("", "entity_name").unwrap(),
            "details":row.try_get::<Option<String>>("", "details").unwrap(),
            "ip_address":row.try_get::<Option<String>>("", "ip_address").unwrap(),
            "user_agent":row.try_get::<Option<String>>("", "user_agent").unwrap(),
            "created_at":row.try_get::<chrono::DateTime<chrono::Utc>>("", "created_at").unwrap(),
        })
    })
    .collect()
}

async fn assert_migration(db: &DatabaseConnection) {
    let selection = serde_json::to_string(
        &ACTIONS
            .iter()
            .map(|(old, _)| *old)
            .chain(["server_start"])
            .collect::<Vec<_>>(),
    )
    .unwrap();
    fixture(db, &selection).await;
    let before = logs(db).await;
    Migrator::up(db, None).await.unwrap();
    let after = logs(db).await;
    assert_eq!(after.len(), before.len());
    for (original, migrated) in before.iter().zip(&after) {
        let mut expected = original.clone();
        for (old, new) in ACTIONS {
            if expected["action"] == *old {
                expected["action"] = (*new).into();
            }
        }
        if expected["entity_type"] == "remote_ingress_profile" {
            expected["entity_type"] = "remote_storage_target".into();
        }
        assert_eq!(
            *migrated, expected,
            "only exact action/entity identifiers may change"
        );
    }
    let migrated_selection = read_config(db).await;
    let actions =
        aster_drive::config::audit::parse_recorded_actions_config_value(&migrated_selection)
            .unwrap();
    assert_eq!(actions.len(), 7);
    for (action, (_, new)) in actions.iter().zip(ACTIONS) {
        assert_eq!(action.as_str(), *new);
    }
    assert_eq!(actions.last().unwrap().as_str(), "server_start");
    Migrator::up(db, None).await.unwrap();
    assert_eq!(logs(db).await, after);
    assert_eq!(read_config(db).await, migrated_selection);
    CurrentMigrator::down(db, Some(1)).await.unwrap();
    assert_eq!(read_config(db).await, selection);
    let down = logs(db).await;
    for entry in &down[..12] {
        assert_eq!(entry["entity_type"], "remote_ingress_profile");
        assert_eq!(entry["details"], DETAILS);
    }
    Migrator::up(db, None).await.unwrap();
    assert_eq!(logs(db).await, after);
}

async fn assert_invalid_config(db: &DatabaseConnection) {
    fixture(db, "invalid-array-do-not-log").await;
    let before = logs(db).await;
    let error = Migrator::up(db, None).await.unwrap_err().to_string();
    assert!(error.contains("audit_log_recorded_actions"));
    assert!(!error.contains("invalid-array-do-not-log"));
    assert_eq!(logs(db).await, before);
    assert_eq!(read_config(db).await, "invalid-array-do-not-log");
}

#[tokio::test]
async fn audit_names_migrate_sqlite() {
    assert_migration(&Database::connect("sqlite::memory:").await.unwrap()).await;
}
#[tokio::test]
async fn audit_names_migrate_postgres() {
    assert_migration(
        &Database::connect(common::postgres_empty_test_database_url().await)
            .await
            .unwrap(),
    )
    .await;
}
#[tokio::test]
async fn audit_names_migrate_mysql() {
    assert_migration(
        &Database::connect(common::mysql_empty_test_database_url().await)
            .await
            .unwrap(),
    )
    .await;
}
#[tokio::test]
async fn audit_names_invalid_config_sqlite() {
    assert_invalid_config(&Database::connect("sqlite::memory:").await.unwrap()).await;
}
#[tokio::test]
async fn audit_names_invalid_config_postgres() {
    assert_invalid_config(
        &Database::connect(common::postgres_empty_test_database_url().await)
            .await
            .unwrap(),
    )
    .await;
}
#[tokio::test]
async fn audit_names_invalid_config_mysql() {
    assert_invalid_config(
        &Database::connect(common::mysql_empty_test_database_url().await)
            .await
            .unwrap(),
    )
    .await;
}

#[tokio::test]
async fn audit_names_preserve_empty_selection_and_merge_duplicate_names() {
    for (input, expected) in [
        ("[]", "[]"),
        (
            r#"["admin_create_remote_ingress_profile","admin_create_remote_storage_target","server_start"]"#,
            r#"["admin_create_remote_storage_target","server_start"]"#,
        ),
    ] {
        let db = Database::connect("sqlite::memory:").await.unwrap();
        fixture(&db, input).await;
        Migrator::up(&db, None).await.unwrap();
        assert_eq!(read_config(&db).await, expected);
    }
}

#[tokio::test]
async fn audit_names_update_failure_rolls_back_logs_and_action_selection() {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    let config = r#"["admin_create_remote_ingress_profile"]"#;
    fixture(&db, config).await;
    let before = logs(&db).await;
    db.execute_unprepared("CREATE TRIGGER reject_audit_update BEFORE UPDATE ON audit_logs BEGIN SELECT RAISE(ABORT, 'injected audit failure'); END").await.unwrap();
    assert!(Migrator::up(&db, None).await.is_err());
    assert_eq!(logs(&db).await, before);
    assert_eq!(read_config(&db).await, config);
    db.execute_unprepared("DROP TRIGGER reject_audit_update")
        .await
        .unwrap();
    Migrator::up(&db, None).await.unwrap();
    assert_eq!(
        read_config(&db).await,
        r#"["admin_create_remote_storage_target"]"#
    );
}
