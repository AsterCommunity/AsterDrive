//! 数据库迁移：新增 Google Drive 存储策略 OAuth flow 表。

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        create_google_drive_oauth_flows(manager).await?;
        create_indexes(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(GoogleDriveOauthFlows::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await
    }
}

async fn create_google_drive_oauth_flows(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            Table::create()
                .table(GoogleDriveOauthFlows::Table)
                .if_not_exists()
                .col(big_integer_pk(GoogleDriveOauthFlows::Id))
                .col(
                    ColumnDef::new(GoogleDriveOauthFlows::PolicyId)
                        .big_integer()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(GoogleDriveOauthFlows::StateHash)
                        .string_len(64)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(GoogleDriveOauthFlows::PkceVerifier)
                        .string_len(256)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(GoogleDriveOauthFlows::RedirectUri)
                        .text()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(GoogleDriveOauthFlows::ReturnPath)
                        .text()
                        .null(),
                )
                .col(
                    crate::time::utc_date_time_column(manager, GoogleDriveOauthFlows::CreatedAt)
                        .not_null(),
                )
                .col(
                    crate::time::utc_date_time_column(manager, GoogleDriveOauthFlows::ExpiresAt)
                        .not_null(),
                )
                .col(
                    crate::time::utc_date_time_column(manager, GoogleDriveOauthFlows::ConsumedAt)
                        .null(),
                )
                .foreign_key(
                    ForeignKey::create()
                        .from(
                            GoogleDriveOauthFlows::Table,
                            GoogleDriveOauthFlows::PolicyId,
                        )
                        .to(StoragePolicies::Table, StoragePolicies::Id)
                        .on_delete(ForeignKeyAction::Cascade),
                )
                .to_owned(),
        )
        .await
}

async fn create_indexes(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    for index in [
        Index::create()
            .name("idx_google_drive_oauth_flows_policy")
            .table(GoogleDriveOauthFlows::Table)
            .col(GoogleDriveOauthFlows::PolicyId)
            .to_owned(),
        Index::create()
            .name("idx_google_drive_oauth_flows_state_hash")
            .table(GoogleDriveOauthFlows::Table)
            .col(GoogleDriveOauthFlows::StateHash)
            .unique()
            .to_owned(),
        Index::create()
            .name("idx_google_drive_oauth_flows_expires_at")
            .table(GoogleDriveOauthFlows::Table)
            .col(GoogleDriveOauthFlows::ExpiresAt)
            .to_owned(),
    ] {
        manager.create_index(index).await?;
    }
    Ok(())
}

fn big_integer_pk<T>(column: T) -> ColumnDef
where
    T: IntoIden,
{
    let mut column = ColumnDef::new(column);
    column
        .big_integer()
        .not_null()
        .auto_increment()
        .primary_key();
    column
}

#[derive(DeriveIden)]
enum StoragePolicies {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum GoogleDriveOauthFlows {
    Table,
    Id,
    PolicyId,
    StateHash,
    PkceVerifier,
    RedirectUri,
    ReturnPath,
    CreatedAt,
    ExpiresAt,
    ConsumedAt,
}
