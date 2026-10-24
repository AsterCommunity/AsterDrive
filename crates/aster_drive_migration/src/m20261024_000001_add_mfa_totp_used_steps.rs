//! Add `mfa_totp_used_steps` so a TOTP code can be consumed once per time step
//! across login challenges and sensitive-action confirmations.
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum MfaTotpUsedSteps {
    Table,
    Id,
    FactorId,
    TimeStep,
    UsedAt,
}

#[derive(DeriveIden)]
enum MfaFactors {
    Table,
    Id,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(MfaTotpUsedSteps::Table)
                    .if_not_exists()
                    .col(aster_forge_db_migration::big_integer_primary_key(
                        MfaTotpUsedSteps::Id,
                    ))
                    .col(
                        ColumnDef::new(MfaTotpUsedSteps::FactorId)
                            .big_integer()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(MfaTotpUsedSteps::Table, MfaTotpUsedSteps::FactorId)
                            .to(MfaFactors::Table, MfaFactors::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .col(
                        ColumnDef::new(MfaTotpUsedSteps::TimeStep)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        aster_forge_db_migration::utc_date_time_column(
                            manager,
                            MfaTotpUsedSteps::UsedAt,
                        )
                        .not_null(),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_mfa_totp_used_steps_factor_step")
                    .table(MfaTotpUsedSteps::Table)
                    .col(MfaTotpUsedSteps::FactorId)
                    .col(MfaTotpUsedSteps::TimeStep)
                    .unique()
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(MfaTotpUsedSteps::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await
    }
}
