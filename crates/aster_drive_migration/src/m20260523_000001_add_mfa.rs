//! 数据库迁移：新增 MFA 因子、恢复码、登录 flow 与 TOTP setup flow。

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        create_mfa_factors(manager).await?;
        create_mfa_recovery_codes(manager).await?;
        create_mfa_login_flows(manager).await?;
        create_mfa_totp_setup_flows(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for table in [
            MfaTotpSetupFlows::Table.into_iden(),
            MfaLoginFlows::Table.into_iden(),
            MfaRecoveryCodes::Table.into_iden(),
            MfaFactors::Table.into_iden(),
        ] {
            manager
                .drop_table(Table::drop().table(table).if_exists().to_owned())
                .await?;
        }
        Ok(())
    }
}

async fn create_mfa_factors(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            Table::create()
                .table(MfaFactors::Table)
                .if_not_exists()
                .col(aster_forge_db_migration::big_integer_primary_key(
                    MfaFactors::Id,
                ))
                .col(ColumnDef::new(MfaFactors::UserId).big_integer().not_null())
                .col(ColumnDef::new(MfaFactors::Method).string_len(16).not_null())
                .col(ColumnDef::new(MfaFactors::Name).string_len(128).not_null())
                .col(
                    ColumnDef::new(MfaFactors::SecretCiphertext)
                        .text()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(MfaFactors::SecretVersion)
                        .integer()
                        .not_null()
                        .default(1),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(manager, MfaFactors::EnabledAt)
                        .not_null(),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(manager, MfaFactors::LastUsedAt)
                        .null(),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(manager, MfaFactors::CreatedAt)
                        .not_null(),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(manager, MfaFactors::UpdatedAt)
                        .not_null(),
                )
                .foreign_key(
                    ForeignKey::create()
                        .from(MfaFactors::Table, MfaFactors::UserId)
                        .to(Users::Table, Users::Id)
                        .on_delete(ForeignKeyAction::Cascade),
                )
                .to_owned(),
        )
        .await?;

    for index in [
        Index::create()
            .name("idx_mfa_factors_user_id")
            .table(MfaFactors::Table)
            .col(MfaFactors::UserId)
            .to_owned(),
        Index::create()
            .name("idx_mfa_factors_user_method")
            .table(MfaFactors::Table)
            .col(MfaFactors::UserId)
            .col(MfaFactors::Method)
            .unique()
            .to_owned(),
    ] {
        manager.create_index(index).await?;
    }

    Ok(())
}

async fn create_mfa_recovery_codes(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            Table::create()
                .table(MfaRecoveryCodes::Table)
                .if_not_exists()
                .col(aster_forge_db_migration::big_integer_primary_key(
                    MfaRecoveryCodes::Id,
                ))
                .col(
                    ColumnDef::new(MfaRecoveryCodes::UserId)
                        .big_integer()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(MfaRecoveryCodes::CodeHash)
                        .string_len(255)
                        .not_null(),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(
                        manager,
                        MfaRecoveryCodes::UsedAt,
                    )
                    .null(),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(
                        manager,
                        MfaRecoveryCodes::CreatedAt,
                    )
                    .not_null(),
                )
                .foreign_key(
                    ForeignKey::create()
                        .from(MfaRecoveryCodes::Table, MfaRecoveryCodes::UserId)
                        .to(Users::Table, Users::Id)
                        .on_delete(ForeignKeyAction::Cascade),
                )
                .to_owned(),
        )
        .await?;

    for index in [
        Index::create()
            .name("idx_mfa_recovery_codes_user_id")
            .table(MfaRecoveryCodes::Table)
            .col(MfaRecoveryCodes::UserId)
            .to_owned(),
        Index::create()
            .name("idx_mfa_recovery_codes_unused")
            .table(MfaRecoveryCodes::Table)
            .col(MfaRecoveryCodes::UserId)
            .col(MfaRecoveryCodes::UsedAt)
            .to_owned(),
    ] {
        manager.create_index(index).await?;
    }

    Ok(())
}

async fn create_mfa_login_flows(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            Table::create()
                .table(MfaLoginFlows::Table)
                .if_not_exists()
                .col(aster_forge_db_migration::big_integer_primary_key(
                    MfaLoginFlows::Id,
                ))
                .col(
                    ColumnDef::new(MfaLoginFlows::FlowTokenHash)
                        .string_len(64)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(MfaLoginFlows::UserId)
                        .big_integer()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(MfaLoginFlows::UserSessionVersion)
                        .big_integer()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(MfaLoginFlows::FirstFactor)
                        .string_len(32)
                        .not_null(),
                )
                .col(ColumnDef::new(MfaLoginFlows::ReturnPath).text().null())
                .col(
                    ColumnDef::new(MfaLoginFlows::IpAddress)
                        .string_len(45)
                        .null(),
                )
                .col(
                    ColumnDef::new(MfaLoginFlows::UserAgent)
                        .string_len(512)
                        .null(),
                )
                .col(
                    ColumnDef::new(MfaLoginFlows::AttemptCount)
                        .integer()
                        .not_null()
                        .default(0),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(
                        manager,
                        MfaLoginFlows::ExpiresAt,
                    )
                    .not_null(),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(
                        manager,
                        MfaLoginFlows::ConsumedAt,
                    )
                    .null(),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(
                        manager,
                        MfaLoginFlows::CreatedAt,
                    )
                    .not_null(),
                )
                .foreign_key(
                    ForeignKey::create()
                        .from(MfaLoginFlows::Table, MfaLoginFlows::UserId)
                        .to(Users::Table, Users::Id)
                        .on_delete(ForeignKeyAction::Cascade),
                )
                .to_owned(),
        )
        .await?;

    for index in [
        Index::create()
            .name("idx_mfa_login_flows_token_hash")
            .table(MfaLoginFlows::Table)
            .col(MfaLoginFlows::FlowTokenHash)
            .unique()
            .to_owned(),
        Index::create()
            .name("idx_mfa_login_flows_user_id")
            .table(MfaLoginFlows::Table)
            .col(MfaLoginFlows::UserId)
            .to_owned(),
        Index::create()
            .name("idx_mfa_login_flows_expires_at")
            .table(MfaLoginFlows::Table)
            .col(MfaLoginFlows::ExpiresAt)
            .to_owned(),
    ] {
        manager.create_index(index).await?;
    }

    Ok(())
}

async fn create_mfa_totp_setup_flows(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            Table::create()
                .table(MfaTotpSetupFlows::Table)
                .if_not_exists()
                .col(aster_forge_db_migration::big_integer_primary_key(
                    MfaTotpSetupFlows::Id,
                ))
                .col(
                    ColumnDef::new(MfaTotpSetupFlows::FlowTokenHash)
                        .string_len(64)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(MfaTotpSetupFlows::UserId)
                        .big_integer()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(MfaTotpSetupFlows::SecretCiphertext)
                        .text()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(MfaTotpSetupFlows::SecretVersion)
                        .integer()
                        .not_null()
                        .default(1),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(
                        manager,
                        MfaTotpSetupFlows::ExpiresAt,
                    )
                    .not_null(),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(
                        manager,
                        MfaTotpSetupFlows::ConsumedAt,
                    )
                    .null(),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(
                        manager,
                        MfaTotpSetupFlows::CreatedAt,
                    )
                    .not_null(),
                )
                .foreign_key(
                    ForeignKey::create()
                        .from(MfaTotpSetupFlows::Table, MfaTotpSetupFlows::UserId)
                        .to(Users::Table, Users::Id)
                        .on_delete(ForeignKeyAction::Cascade),
                )
                .to_owned(),
        )
        .await?;

    for index in [
        Index::create()
            .name("idx_mfa_totp_setup_flows_token_hash")
            .table(MfaTotpSetupFlows::Table)
            .col(MfaTotpSetupFlows::FlowTokenHash)
            .unique()
            .to_owned(),
        Index::create()
            .name("idx_mfa_totp_setup_flows_user_id")
            .table(MfaTotpSetupFlows::Table)
            .col(MfaTotpSetupFlows::UserId)
            .to_owned(),
        Index::create()
            .name("idx_mfa_totp_setup_flows_expires_at")
            .table(MfaTotpSetupFlows::Table)
            .col(MfaTotpSetupFlows::ExpiresAt)
            .to_owned(),
    ] {
        manager.create_index(index).await?;
    }

    Ok(())
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum MfaFactors {
    Table,
    Id,
    UserId,
    Method,
    Name,
    SecretCiphertext,
    SecretVersion,
    EnabledAt,
    LastUsedAt,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum MfaRecoveryCodes {
    Table,
    Id,
    UserId,
    CodeHash,
    UsedAt,
    CreatedAt,
}

#[derive(DeriveIden)]
enum MfaLoginFlows {
    Table,
    Id,
    FlowTokenHash,
    UserId,
    UserSessionVersion,
    FirstFactor,
    ReturnPath,
    IpAddress,
    UserAgent,
    AttemptCount,
    ExpiresAt,
    ConsumedAt,
    CreatedAt,
}

#[derive(DeriveIden)]
enum MfaTotpSetupFlows {
    Table,
    Id,
    FlowTokenHash,
    UserId,
    SecretCiphertext,
    SecretVersion,
    ExpiresAt,
    ConsumedAt,
    CreatedAt,
}
