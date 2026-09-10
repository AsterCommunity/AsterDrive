//! Storage placement profile topology.
//!
//! The legacy storage_policy_group_items table remains as a compatibility
//! projection until 0.6.0. This migration materializes each legacy item as a
//! single rule with one target and preserves first-match size routing.
//! The same unreleased topology migration also removes binding-wide remote
//! target defaults now that remote policies persist an explicit target key.
//!
//! TODO(0.6.0): remove the legacy item projection, compatibility readers and
//! the old policy-group-only migration path after all supported databases have
//! applied this migration and the placement API is authoritative.

use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::{
    ConnectionTrait, DbBackend, Statement, TransactionTrait, prelude::DateTimeUtc,
};
use serde_json::json;

#[derive(DeriveMigrationName)]
pub struct Migration;

const DEFAULT_ADMISSION: &str = r#"{"format_version":1,"schema_version":1,"values":{"allowed_extensions":[],"denied_extensions":[],"accept_extensionless":true,"allowed_categories":[],"max_file_size":0}}"#;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        remove_remote_storage_target_default(manager).await?;
        report_legacy_allowed_types(manager).await?;
        add_profile_columns(manager).await?;
        create_rules(manager).await?;
        create_targets(manager).await?;
        materialize_legacy_items(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(StoragePolicyGroupRuleTargets::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(
                Table::drop()
                    .table(StoragePolicyGroupRules::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        restore_remote_storage_target_default(manager).await?;
        // TODO(0.6.0): remove this down migration once the legacy group
        // columns become part of the permanent placement schema.
        Ok(())
    }
}

async fn remove_remote_storage_target_default(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    if !manager.has_table("remote_storage_targets").await? {
        return Ok(());
    }
    aster_forge_db_migration::drop_index_if_exists(
        manager.get_connection(),
        "remote_storage_targets",
        "idx_remote_storage_targets_binding_default",
    )
    .await?;
    if manager
        .has_column("remote_storage_targets", "is_default")
        .await?
    {
        manager
            .alter_table(
                Table::alter()
                    .table(RemoteStorageTargets::Table)
                    .drop_column(RemoteStorageTargets::IsDefault)
                    .to_owned(),
            )
            .await?;
    }
    Ok(())
}

async fn restore_remote_storage_target_default(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    if !manager.has_table("remote_storage_targets").await? {
        return Ok(());
    }
    if !manager
        .has_column("remote_storage_targets", "is_default")
        .await?
    {
        manager
            .alter_table(
                Table::alter()
                    .table(RemoteStorageTargets::Table)
                    .add_column(
                        ColumnDef::new(RemoteStorageTargets::IsDefault)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .to_owned(),
            )
            .await?;
    }

    let restore_default_sql = match manager.get_database_backend() {
        DbBackend::MySql => {
            "UPDATE remote_storage_targets AS target \
             JOIN (SELECT master_binding_id, MIN(id) AS id \
                   FROM remote_storage_targets GROUP BY master_binding_id) AS first_target \
               ON first_target.id = target.id \
             SET target.is_default = TRUE"
        }
        _ => {
            "UPDATE remote_storage_targets SET is_default = TRUE \
             WHERE id IN (SELECT MIN(id) FROM remote_storage_targets \
                          GROUP BY master_binding_id)"
        }
    };
    manager
        .get_connection()
        .execute_unprepared(restore_default_sql)
        .await?;
    manager
        .create_index(
            Index::create()
                .name("idx_remote_storage_targets_binding_default")
                .table(RemoteStorageTargets::Table)
                .col(RemoteStorageTargets::MasterBindingId)
                .col(RemoteStorageTargets::IsDefault)
                .if_not_exists()
                .to_owned(),
        )
        .await
}

async fn report_legacy_allowed_types(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    if !manager.has_table("storage_policies").await? {
        return Ok(());
    }
    let backend = manager.get_database_backend();
    let statement = Statement::from_string(
        backend,
        "SELECT COUNT(*) FROM storage_policies WHERE TRIM(COALESCE(allowed_types, '')) NOT IN ('', '[]')",
    );
    let row = manager
        .get_connection()
        .query_one_raw(statement)
        .await?
        .ok_or_else(|| DbErr::Migration("allowed_types audit returned no row".to_string()))?;
    let count: i64 = row.try_get_by_index(0)?;
    if count > 0 {
        tracing::warn!(
            non_empty_policy_count = count,
            "storage policy allowed_types values require explicit placement-admission audit; they remain non-authoritative until 0.6.0"
        );
    }
    Ok(())
}

async fn add_profile_columns(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    for (column, definition) in [
        (
            StoragePolicyGroups::AdmissionConfig,
            ColumnDef::new(StoragePolicyGroups::AdmissionConfig)
                .string_len(4_000)
                .not_null()
                .default(DEFAULT_ADMISSION),
        ),
        (
            StoragePolicyGroups::UploadExecutionPreference,
            ColumnDef::new(StoragePolicyGroups::UploadExecutionPreference)
                .string_len(32)
                .not_null()
                .default("automatic"),
        ),
        (
            StoragePolicyGroups::RoutingRevision,
            ColumnDef::new(StoragePolicyGroups::RoutingRevision)
                .big_integer()
                .not_null()
                .default(1),
        ),
    ] {
        if !manager
            .has_column("storage_policy_groups", column.to_string())
            .await?
        {
            manager
                .alter_table(
                    Table::alter()
                        .table(StoragePolicyGroups::Table)
                        .add_column(definition)
                        .to_owned(),
                )
                .await?;
        }
    }
    Ok(())
}

async fn create_rules(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            Table::create()
                .table(StoragePolicyGroupRules::Table)
                .if_not_exists()
                .col(aster_forge_db_migration::big_integer_primary_key(
                    StoragePolicyGroupRules::Id,
                ))
                .col(
                    ColumnDef::new(StoragePolicyGroupRules::GroupId)
                        .big_integer()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(StoragePolicyGroupRules::Name)
                        .string_len(128)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(StoragePolicyGroupRules::Description)
                        .string_len(512)
                        .not_null()
                        .default(""),
                )
                .col(
                    ColumnDef::new(StoragePolicyGroupRules::Priority)
                        .integer()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(StoragePolicyGroupRules::IsEnabled)
                        .boolean()
                        .not_null()
                        .default(true),
                )
                .col(
                    ColumnDef::new(StoragePolicyGroupRules::Matcher)
                        .string_len(4_000)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(StoragePolicyGroupRules::SelectionMode)
                        .string_len(32)
                        .not_null()
                        .default("first_available"),
                )
                .col(
                    ColumnDef::new(StoragePolicyGroupRules::UnavailableBehavior)
                        .string_len(32)
                        .not_null()
                        .default("next_rule"),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(
                        manager,
                        StoragePolicyGroupRules::CreatedAt,
                    )
                    .not_null(),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(
                        manager,
                        StoragePolicyGroupRules::UpdatedAt,
                    )
                    .not_null(),
                )
                .foreign_key(
                    ForeignKey::create()
                        .from(
                            StoragePolicyGroupRules::Table,
                            StoragePolicyGroupRules::GroupId,
                        )
                        .to(StoragePolicyGroups::Table, StoragePolicyGroups::Id)
                        .on_delete(ForeignKeyAction::Cascade),
                )
                .to_owned(),
        )
        .await?;
    manager
        .create_index(
            Index::create()
                .name("idx_spgr_group_priority")
                .table(StoragePolicyGroupRules::Table)
                .col(StoragePolicyGroupRules::GroupId)
                .col(StoragePolicyGroupRules::Priority)
                .unique()
                .to_owned(),
        )
        .await
}

async fn create_targets(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            Table::create()
                .table(StoragePolicyGroupRuleTargets::Table)
                .if_not_exists()
                .col(aster_forge_db_migration::big_integer_primary_key(
                    StoragePolicyGroupRuleTargets::Id,
                ))
                .col(
                    ColumnDef::new(StoragePolicyGroupRuleTargets::RuleId)
                        .big_integer()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(StoragePolicyGroupRuleTargets::PolicyId)
                        .big_integer()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(StoragePolicyGroupRuleTargets::Weight)
                        .integer()
                        .not_null()
                        .default(100),
                )
                .col(
                    ColumnDef::new(StoragePolicyGroupRuleTargets::AcceptingNewWrites)
                        .boolean()
                        .not_null()
                        .default(true),
                )
                .col(
                    ColumnDef::new(StoragePolicyGroupRuleTargets::StableOrder)
                        .integer()
                        .not_null()
                        .default(1),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(
                        manager,
                        StoragePolicyGroupRuleTargets::CreatedAt,
                    )
                    .not_null(),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(
                        manager,
                        StoragePolicyGroupRuleTargets::UpdatedAt,
                    )
                    .not_null(),
                )
                .foreign_key(
                    ForeignKey::create()
                        .from(
                            StoragePolicyGroupRuleTargets::Table,
                            StoragePolicyGroupRuleTargets::RuleId,
                        )
                        .to(StoragePolicyGroupRules::Table, StoragePolicyGroupRules::Id)
                        .on_delete(ForeignKeyAction::Cascade),
                )
                .foreign_key(
                    ForeignKey::create()
                        .from(
                            StoragePolicyGroupRuleTargets::Table,
                            StoragePolicyGroupRuleTargets::PolicyId,
                        )
                        .to(StoragePolicies::Table, StoragePolicies::Id)
                        .on_delete(ForeignKeyAction::Cascade),
                )
                .to_owned(),
        )
        .await?;
    manager
        .create_index(
            Index::create()
                .name("idx_spgrt_rule_policy")
                .table(StoragePolicyGroupRuleTargets::Table)
                .col(StoragePolicyGroupRuleTargets::RuleId)
                .col(StoragePolicyGroupRuleTargets::PolicyId)
                .unique()
                .to_owned(),
        )
        .await
}

async fn materialize_legacy_items(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let connection = manager.get_connection();
    let select = Query::select()
        .columns([
            StoragePolicyGroupItems::Id,
            StoragePolicyGroupItems::GroupId,
            StoragePolicyGroupItems::PolicyId,
            StoragePolicyGroupItems::Priority,
            StoragePolicyGroupItems::MinFileSize,
            StoragePolicyGroupItems::MaxFileSize,
            StoragePolicyGroupItems::CreatedAt,
        ])
        .from(StoragePolicyGroupItems::Table)
        .order_by(StoragePolicyGroupItems::Id, Order::Asc)
        .to_owned();
    let rows = connection.query_all(&select).await?;
    let transaction = connection.begin().await?;
    for row in rows {
        let id: i64 = row.try_get_by_index(0)?;
        let group_id: i64 = row.try_get_by_index(1)?;
        let policy_id: i64 = row.try_get_by_index(2)?;
        let priority: i32 = row.try_get_by_index(3)?;
        let min_file_size: i64 = row.try_get_by_index(4)?;
        let max_file_size: i64 = row.try_get_by_index(5)?;
        let created_at: DateTimeUtc = row.try_get_by_index(6)?;
        let matcher = json!({
            "format_version": 1,
            "schema_version": 1,
            "values": {
                "min_file_size": min_file_size,
                "max_file_size": max_file_size,
                "extensions": [],
                "compound_extensions": [],
                "extensionless": null,
                "categories": []
            }
        })
        .to_string();
        let mut rule = Query::insert();
        rule.into_table(StoragePolicyGroupRules::Table)
            .columns([
                StoragePolicyGroupRules::Id,
                StoragePolicyGroupRules::GroupId,
                StoragePolicyGroupRules::Name,
                StoragePolicyGroupRules::Description,
                StoragePolicyGroupRules::Priority,
                StoragePolicyGroupRules::IsEnabled,
                StoragePolicyGroupRules::Matcher,
                StoragePolicyGroupRules::SelectionMode,
                StoragePolicyGroupRules::UnavailableBehavior,
                StoragePolicyGroupRules::CreatedAt,
                StoragePolicyGroupRules::UpdatedAt,
            ])
            .values_panic([
                id.into(),
                group_id.into(),
                "Legacy placement rule".into(),
                "Materialized from storage_policy_group_items; TODO remove in 0.6.0".into(),
                priority.into(),
                true.into(),
                matcher.into(),
                "first_available".into(),
                "next_rule".into(),
                created_at.into(),
                created_at.into(),
            ]);
        transaction.execute(&rule).await?;

        let mut target = Query::insert();
        target
            .into_table(StoragePolicyGroupRuleTargets::Table)
            .columns([
                StoragePolicyGroupRuleTargets::Id,
                StoragePolicyGroupRuleTargets::RuleId,
                StoragePolicyGroupRuleTargets::PolicyId,
                StoragePolicyGroupRuleTargets::Weight,
                StoragePolicyGroupRuleTargets::AcceptingNewWrites,
                StoragePolicyGroupRuleTargets::StableOrder,
                StoragePolicyGroupRuleTargets::CreatedAt,
                StoragePolicyGroupRuleTargets::UpdatedAt,
            ])
            .values_panic([
                id.into(),
                id.into(),
                policy_id.into(),
                100_i32.into(),
                true.into(),
                1_i32.into(),
                created_at.into(),
                created_at.into(),
            ]);
        transaction.execute(&target).await?;
    }
    transaction.commit().await?;
    if manager.get_database_backend() == DbBackend::Postgres {
        for table in [
            "storage_policy_group_rules",
            "storage_policy_group_rule_targets",
        ] {
            manager
                .get_connection()
                .execute_unprepared(&format!(
                    "SELECT setval(pg_get_serial_sequence('{table}', 'id'), COALESCE((SELECT MAX(id) FROM {table}), 0) + 1, false)"
                ))
                .await?;
        }
    }
    Ok(())
}

#[derive(DeriveIden)]
enum StoragePolicyGroups {
    Table,
    Id,
    AdmissionConfig,
    UploadExecutionPreference,
    RoutingRevision,
}

#[derive(DeriveIden)]
enum StoragePolicyGroupItems {
    Table,
    Id,
    GroupId,
    PolicyId,
    Priority,
    MinFileSize,
    MaxFileSize,
    CreatedAt,
}

#[derive(DeriveIden)]
enum StoragePolicyGroupRules {
    Table,
    Id,
    GroupId,
    Name,
    Description,
    Priority,
    IsEnabled,
    Matcher,
    SelectionMode,
    UnavailableBehavior,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum StoragePolicyGroupRuleTargets {
    Table,
    Id,
    RuleId,
    PolicyId,
    Weight,
    AcceptingNewWrites,
    StableOrder,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum StoragePolicies {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum RemoteStorageTargets {
    Table,
    MasterBindingId,
    IsDefault,
}

#[cfg(test)]
mod tests {
    use super::{remove_remote_storage_target_default, restore_remote_storage_target_default};
    use sea_orm_migration::SchemaManager;
    use sea_orm_migration::sea_orm::{ConnectionTrait, Database, DbBackend, Statement};

    #[tokio::test]
    async fn sqlite_removes_and_restores_default_target_schema() {
        let db = Database::connect("sqlite::memory:").await.unwrap();
        db.execute_unprepared(
            "CREATE TABLE remote_storage_targets (\
                id INTEGER PRIMARY KEY, \
                master_binding_id INTEGER NOT NULL, \
                is_default BOOLEAN NOT NULL DEFAULT 0\
            ); \
            CREATE INDEX idx_remote_storage_targets_binding_default \
                ON remote_storage_targets(master_binding_id, is_default); \
            INSERT INTO remote_storage_targets (id, master_binding_id, is_default) \
                VALUES (1, 7, 0), (2, 7, 1);",
        )
        .await
        .unwrap();
        let manager = SchemaManager::new(&db);

        remove_remote_storage_target_default(&manager)
            .await
            .unwrap();
        assert!(
            !manager
                .has_column("remote_storage_targets", "is_default")
                .await
                .unwrap()
        );
        assert!(
            !manager
                .has_index(
                    "remote_storage_targets",
                    "idx_remote_storage_targets_binding_default"
                )
                .await
                .unwrap()
        );

        restore_remote_storage_target_default(&manager)
            .await
            .unwrap();
        assert!(
            manager
                .has_column("remote_storage_targets", "is_default")
                .await
                .unwrap()
        );
        let row = db
            .query_one_raw(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT id FROM remote_storage_targets \
                 WHERE master_binding_id = 7 AND is_default = 1"
                    .to_string(),
            ))
            .await
            .unwrap()
            .expect("rollback should restore one default target per binding");
        assert_eq!(row.try_get::<i64>("", "id").unwrap(), 1);
    }
}
