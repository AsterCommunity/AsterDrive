//! Repository for encrypted connector-owned storage-policy credentials.

use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, Set, sea_query::Expr,
};

use crate::errors::{AsterError, Result};
use aster_drive_model::entities::storage_policy_connector_credential::{
    self, Entity as StoragePolicyConnectorCredential,
};

pub async fn find_all<C: ConnectionTrait>(
    db: &C,
) -> Result<Vec<storage_policy_connector_credential::Model>> {
    StoragePolicyConnectorCredential::find()
        .all(db)
        .await
        .map_err(AsterError::from)
}

pub async fn find_by_policy<C: ConnectionTrait>(
    db: &C,
    policy_id: i64,
) -> Result<Option<storage_policy_connector_credential::Model>> {
    StoragePolicyConnectorCredential::find()
        .filter(storage_policy_connector_credential::Column::PolicyId.eq(policy_id))
        .one(db)
        .await
        .map_err(AsterError::from)
}

pub async fn upsert<C: ConnectionTrait>(
    db: &C,
    policy_id: i64,
    connector_id: String,
    schema_version: i32,
    ciphertext: String,
) -> Result<storage_policy_connector_credential::Model> {
    let now = Utc::now();
    if let Some(existing) = find_by_policy(db, policy_id).await? {
        let next_revision = existing.revision.checked_add(1).ok_or_else(|| {
            AsterError::database_operation("storage connector credential revision overflow")
        })?;
        let mut active: storage_policy_connector_credential::ActiveModel = existing.into();
        active.connector_id = Set(connector_id);
        active.schema_version = Set(schema_version);
        active.revision = Set(next_revision);
        active.ciphertext = Set(ciphertext);
        active.updated_at = Set(now);
        return active.update(db).await.map_err(AsterError::from);
    }

    storage_policy_connector_credential::ActiveModel {
        id: Default::default(),
        policy_id: Set(policy_id),
        connector_id: Set(connector_id),
        schema_version: Set(schema_version),
        revision: Set(1),
        ciphertext: Set(ciphertext),
        created_at: Set(now),
        updated_at: Set(now),
    }
    .insert(db)
    .await
    .map_err(AsterError::from)
}

/// Replace an encrypted connector payload only when the caller still owns the
/// revision it decoded. Refreshable connectors use this to prevent token
/// rotation from being overwritten by a concurrent primary instance.
pub async fn update_if_revision<C: ConnectionTrait>(
    db: &C,
    policy_id: i64,
    connector_id: &str,
    schema_version: i32,
    expected_revision: i64,
    ciphertext: String,
) -> Result<bool> {
    let next_revision = expected_revision.checked_add(1).ok_or_else(|| {
        AsterError::database_operation("storage connector credential revision overflow")
    })?;
    let result = StoragePolicyConnectorCredential::update_many()
        .col_expr(
            storage_policy_connector_credential::Column::Ciphertext,
            Expr::value(ciphertext),
        )
        .col_expr(
            storage_policy_connector_credential::Column::Revision,
            Expr::value(next_revision),
        )
        .col_expr(
            storage_policy_connector_credential::Column::UpdatedAt,
            Expr::value(Utc::now()),
        )
        .filter(storage_policy_connector_credential::Column::PolicyId.eq(policy_id))
        .filter(storage_policy_connector_credential::Column::ConnectorId.eq(connector_id))
        .filter(storage_policy_connector_credential::Column::SchemaVersion.eq(schema_version))
        .filter(storage_policy_connector_credential::Column::Revision.eq(expected_revision))
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}

pub struct ConnectorCredentialPromotion<'a> {
    pub policy_id: i64,
    pub source_connector_id: &'a str,
    pub source_schema_version: i32,
    pub expected_revision: i64,
    pub target_connector_id: String,
    pub target_schema_version: i32,
    pub ciphertext: String,
}

pub async fn promote_if_revision<C: ConnectionTrait>(
    db: &C,
    promotion: ConnectorCredentialPromotion<'_>,
) -> Result<bool> {
    let next_revision = promotion.expected_revision.checked_add(1).ok_or_else(|| {
        AsterError::database_operation("storage connector credential revision overflow")
    })?;
    let result = StoragePolicyConnectorCredential::update_many()
        .col_expr(
            storage_policy_connector_credential::Column::ConnectorId,
            Expr::value(promotion.target_connector_id),
        )
        .col_expr(
            storage_policy_connector_credential::Column::SchemaVersion,
            Expr::value(promotion.target_schema_version),
        )
        .col_expr(
            storage_policy_connector_credential::Column::Ciphertext,
            Expr::value(promotion.ciphertext),
        )
        .col_expr(
            storage_policy_connector_credential::Column::Revision,
            Expr::value(next_revision),
        )
        .col_expr(
            storage_policy_connector_credential::Column::UpdatedAt,
            Expr::value(Utc::now()),
        )
        .filter(storage_policy_connector_credential::Column::PolicyId.eq(promotion.policy_id))
        .filter(
            storage_policy_connector_credential::Column::ConnectorId
                .eq(promotion.source_connector_id),
        )
        .filter(
            storage_policy_connector_credential::Column::SchemaVersion
                .eq(promotion.source_schema_version),
        )
        .filter(
            storage_policy_connector_credential::Column::Revision.eq(promotion.expected_revision),
        )
        .exec(db)
        .await
        .map_err(AsterError::from)?;
    Ok(result.rows_affected == 1)
}

pub async fn delete_by_policy<C: ConnectionTrait>(db: &C, policy_id: i64) -> Result<()> {
    StoragePolicyConnectorCredential::delete_many()
        .filter(storage_policy_connector_credential::Column::PolicyId.eq(policy_id))
        .exec(db)
        .await
        .map(|_| ())
        .map_err(AsterError::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm::{ConnectOptions, Database, DbBackend, Schema};

    async fn build_test_db() -> sea_orm::DatabaseConnection {
        let mut options = ConnectOptions::new("sqlite::memory:");
        options.max_connections(1);
        let db = Database::connect(options)
            .await
            .expect("connector credential repository test database should connect");
        db.execute_unprepared("PRAGMA foreign_keys = OFF")
            .await
            .expect("unrelated storage policy foreign keys should be disabled");
        let schema = Schema::new(DbBackend::Sqlite);
        db.execute(&schema.create_table_from_entity(storage_policy_connector_credential::Entity))
            .await
            .expect("connector credential table should be created");
        db
    }

    async fn insert_source_credential(
        db: &sea_orm::DatabaseConnection,
        policy_id: i64,
    ) -> storage_policy_connector_credential::Model {
        let now = Utc::now();
        storage_policy_connector_credential::ActiveModel {
            id: Default::default(),
            policy_id: Set(policy_id),
            connector_id: Set("asterdrive.storage.s3".to_string()),
            schema_version: Set(1),
            revision: Set(7),
            ciphertext: Set(format!("source-ciphertext-{policy_id}")),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(db)
        .await
        .expect("source connector credential should be inserted")
    }

    #[tokio::test]
    async fn promotion_revision_overflow_fails_before_database_update() {
        let db = sea_orm::Database::connect("sqlite::memory:")
            .await
            .expect("test database should connect");
        let error = promote_if_revision(
            &db,
            ConnectorCredentialPromotion {
                policy_id: 1,
                source_connector_id: "asterdrive.storage.s3",
                source_schema_version: 1,
                expected_revision: i64::MAX,
                target_connector_id: "asterdrive.storage.tencent_cos".to_string(),
                target_schema_version: 1,
                ciphertext: "ciphertext".to_string(),
            },
        )
        .await
        .expect_err("revision overflow must fail");
        assert!(error.message().contains("revision overflow"));
    }

    #[tokio::test]
    async fn promotion_compare_and_swap_updates_only_the_matching_source_revision() {
        let db = build_test_db().await;
        for policy_id in 1..=4 {
            insert_source_credential(&db, policy_id).await;
        }

        assert!(
            promote_if_revision(
                &db,
                ConnectorCredentialPromotion {
                    policy_id: 1,
                    source_connector_id: "asterdrive.storage.s3",
                    source_schema_version: 1,
                    expected_revision: 7,
                    target_connector_id: "asterdrive.storage.tencent_cos".to_string(),
                    target_schema_version: 2,
                    ciphertext: "target-ciphertext".to_string(),
                },
            )
            .await
            .expect("matching promotion compare-and-swap should execute")
        );
        let promoted = find_by_policy(&db, 1)
            .await
            .expect("promoted credential lookup should execute")
            .expect("promoted credential should exist");
        assert_eq!(promoted.connector_id, "asterdrive.storage.tencent_cos");
        assert_eq!(promoted.schema_version, 2);
        assert_eq!(promoted.revision, 8);
        assert_eq!(promoted.ciphertext, "target-ciphertext");

        let revision_mismatch_before = find_by_policy(&db, 2).await.unwrap().unwrap();
        assert!(
            !promote_if_revision(
                &db,
                ConnectorCredentialPromotion {
                    policy_id: 2,
                    source_connector_id: "asterdrive.storage.s3",
                    source_schema_version: 1,
                    expected_revision: 8,
                    target_connector_id: "asterdrive.storage.tencent_cos".to_string(),
                    target_schema_version: 2,
                    ciphertext: "revision-mismatch".to_string(),
                },
            )
            .await
            .expect("revision mismatch should execute without updating")
        );
        assert_eq!(
            find_by_policy(&db, 2).await.unwrap().unwrap(),
            revision_mismatch_before
        );

        let connector_mismatch_before = find_by_policy(&db, 3).await.unwrap().unwrap();
        assert!(
            !promote_if_revision(
                &db,
                ConnectorCredentialPromotion {
                    policy_id: 3,
                    source_connector_id: "asterdrive.storage.other",
                    source_schema_version: 1,
                    expected_revision: 7,
                    target_connector_id: "asterdrive.storage.tencent_cos".to_string(),
                    target_schema_version: 2,
                    ciphertext: "connector-mismatch".to_string(),
                },
            )
            .await
            .expect("source connector mismatch should execute without updating")
        );
        assert_eq!(
            find_by_policy(&db, 3).await.unwrap().unwrap(),
            connector_mismatch_before
        );

        let schema_mismatch_before = find_by_policy(&db, 4).await.unwrap().unwrap();
        assert!(
            !promote_if_revision(
                &db,
                ConnectorCredentialPromotion {
                    policy_id: 4,
                    source_connector_id: "asterdrive.storage.s3",
                    source_schema_version: 2,
                    expected_revision: 7,
                    target_connector_id: "asterdrive.storage.tencent_cos".to_string(),
                    target_schema_version: 2,
                    ciphertext: "schema-mismatch".to_string(),
                },
            )
            .await
            .expect("source schema mismatch should execute without updating")
        );
        assert_eq!(
            find_by_policy(&db, 4).await.unwrap().unwrap(),
            schema_mismatch_before
        );
    }
}
