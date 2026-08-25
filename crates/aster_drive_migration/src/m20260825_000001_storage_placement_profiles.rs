//! Storage placement profile topology.
//!
//! The legacy storage_policy_group_items table remains as a compatibility
//! projection until 0.6.0. This migration materializes each legacy item as a
//! single rule with one target and preserves first-match size routing.
//!
//! TODO(0.6.0): remove the legacy item projection, compatibility readers and
//! the old policy-group-only migration path after all supported databases have
//! applied this migration and the placement API is authoritative.

use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::{ConnectionTrait, Statement, TransactionTrait};
use serde_json::json;

#[derive(DeriveMigrationName)]
pub struct Migration;

const DEFAULT_ADMISSION: &str = r#"{"format_version":1,"schema_version":1,"values":{"allowed_extensions":[],"denied_extensions":[],"accept_extensionless":true,"allowed_categories":[],"denied_categories":[],"max_file_size":0}}"#;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
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
        // TODO(0.6.0): remove this down migration once the legacy group
        // columns become part of the permanent placement schema.
        Ok(())
    }
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
                    ColumnDef::new(StoragePolicyGroupRuleTargets::IsEnabled)
                        .boolean()
                        .not_null()
                        .default(true),
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
        let created_at: String = row.try_get_by_index(6)?;
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
                created_at.clone().into(),
                created_at.clone().into(),
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
                StoragePolicyGroupRuleTargets::IsEnabled,
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
                true.into(),
                1_i32.into(),
                created_at.clone().into(),
                created_at.into(),
            ]);
        transaction.execute(&target).await?;
    }
    transaction.commit().await
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
    IsEnabled,
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
