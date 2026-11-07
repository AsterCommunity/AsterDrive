//! Persistent account MFA budgets survive flow/session replacement and instance changes.
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum MfaAttemptBudgets {
    Table,
    UserId,
    Purpose,
    FailureCount,
    WindowStartedAt,
    BlockedUntil,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(MfaAttemptBudgets::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(MfaAttemptBudgets::UserId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(MfaAttemptBudgets::Purpose)
                            .string_len(24)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(MfaAttemptBudgets::FailureCount)
                            .integer()
                            .not_null(),
                    )
                    .col(
                        aster_forge_db_migration::utc_date_time_column(
                            manager,
                            MfaAttemptBudgets::WindowStartedAt,
                        )
                        .not_null(),
                    )
                    .col(
                        aster_forge_db_migration::utc_date_time_column(
                            manager,
                            MfaAttemptBudgets::BlockedUntil,
                        )
                        .null(),
                    )
                    .col(
                        aster_forge_db_migration::utc_date_time_column(
                            manager,
                            MfaAttemptBudgets::UpdatedAt,
                        )
                        .not_null(),
                    )
                    .primary_key(
                        Index::create()
                            .col(MfaAttemptBudgets::UserId)
                            .col(MfaAttemptBudgets::Purpose),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(MfaAttemptBudgets::Table, MfaAttemptBudgets::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(MfaAttemptBudgets::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await
    }
}
