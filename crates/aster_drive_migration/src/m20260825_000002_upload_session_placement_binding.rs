//! Persist the placement decision selected during upload initialization.
//!
//! Profile/revision nullability is tightened by the later expired-compatibility
//! cleanup migration. Rule IDs remain nullable for folder-policy overrides.

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
        // Historical additive migration; the cleanup migration prevents downgrade.
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
