//! Persist the placement decision selected during upload initialization.
//!
//! TODO(0.6.0): make placement_profile_id, placement_rule_id and
//! placement_revision non-null after all pre-placement sessions have expired
//! or have been explicitly finalized/cancelled.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        add_column_if_missing(
            manager,
            UploadSessions::PlacementProfileId,
            ColumnDef::new(UploadSessions::PlacementProfileId)
                .big_integer()
                .null()
                .to_owned(),
        )
        .await?;
        add_column_if_missing(
            manager,
            UploadSessions::PlacementRuleId,
            ColumnDef::new(UploadSessions::PlacementRuleId)
                .big_integer()
                .null()
                .to_owned(),
        )
        .await?;
        add_column_if_missing(
            manager,
            UploadSessions::PlacementRevision,
            ColumnDef::new(UploadSessions::PlacementRevision)
                .big_integer()
                .null()
                .to_owned(),
        )
        .await?;
        add_column_if_missing(
            manager,
            UploadSessions::PlacementExecutionPreference,
            ColumnDef::new(UploadSessions::PlacementExecutionPreference)
                .string_len(32)
                .not_null()
                .default("automatic")
                .to_owned(),
        )
        .await
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        // TODO(0.6.0): remove the compatibility down path with the old
        // policy-only upload session contract.
        Ok(())
    }
}

async fn add_column_if_missing(
    manager: &SchemaManager<'_>,
    column: UploadSessions,
    definition: ColumnDef,
) -> Result<(), DbErr> {
    if !manager
        .has_column("upload_sessions", column.to_string())
        .await?
    {
        manager
            .alter_table(
                Table::alter()
                    .table(UploadSessions::Table)
                    .add_column(definition)
                    .to_owned(),
            )
            .await?;
    }
    Ok(())
}

#[derive(DeriveIden)]
enum UploadSessions {
    Table,
    PlacementProfileId,
    PlacementRuleId,
    PlacementRevision,
    PlacementExecutionPreference,
}
