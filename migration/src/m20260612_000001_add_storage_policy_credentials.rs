//! Add generic OAuth-managed storage policy credentials and authorization flows.

use crate::column::json_text_column_for_final_schema;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        create_storage_policy_credentials(manager).await?;
        create_storage_policy_authorization_flows(manager).await?;
        create_indexes(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for table in [
            StoragePolicyAuthorizationFlows::Table.into_iden(),
            StoragePolicyCredentials::Table.into_iden(),
        ] {
            manager
                .drop_table(Table::drop().table(table).if_exists().to_owned())
                .await?;
        }
        Ok(())
    }
}

async fn create_storage_policy_credentials(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            Table::create()
                .table(StoragePolicyCredentials::Table)
                .if_not_exists()
                .col(big_integer_pk(StoragePolicyCredentials::Id))
                .col(
                    ColumnDef::new(StoragePolicyCredentials::PolicyId)
                        .big_integer()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(StoragePolicyCredentials::Provider)
                        .string_len(32)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(StoragePolicyCredentials::CredentialKind)
                        .string_len(32)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(StoragePolicyCredentials::AccountLabel)
                        .string_len(255)
                        .null(),
                )
                .col(
                    ColumnDef::new(StoragePolicyCredentials::Subject)
                        .string_len(255)
                        .null(),
                )
                .col(
                    ColumnDef::new(StoragePolicyCredentials::TenantId)
                        .string_len(255)
                        .null(),
                )
                .col(
                    ColumnDef::new(StoragePolicyCredentials::Scopes)
                        .text()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(StoragePolicyCredentials::AccessTokenCiphertext)
                        .text()
                        .null(),
                )
                .col(
                    ColumnDef::new(StoragePolicyCredentials::RefreshTokenCiphertext)
                        .text()
                        .null(),
                )
                .col(json_text_column_for_final_schema(
                    manager,
                    StoragePolicyCredentials::Metadata,
                ))
                .col(
                    ColumnDef::new(StoragePolicyCredentials::Status)
                        .string_len(32)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(StoragePolicyCredentials::StatusReason)
                        .text()
                        .null(),
                )
                .col(
                    crate::time::utc_date_time_column(manager, StoragePolicyCredentials::ExpiresAt)
                        .null(),
                )
                .col(
                    crate::time::utc_date_time_column(
                        manager,
                        StoragePolicyCredentials::AuthorizedAt,
                    )
                    .null(),
                )
                .col(
                    crate::time::utc_date_time_column(
                        manager,
                        StoragePolicyCredentials::LastRefreshedAt,
                    )
                    .null(),
                )
                .col(
                    crate::time::utc_date_time_column(
                        manager,
                        StoragePolicyCredentials::LastValidatedAt,
                    )
                    .null(),
                )
                .col(
                    crate::time::utc_date_time_column(manager, StoragePolicyCredentials::CreatedAt)
                        .not_null(),
                )
                .col(
                    crate::time::utc_date_time_column(manager, StoragePolicyCredentials::UpdatedAt)
                        .not_null(),
                )
                .foreign_key(
                    ForeignKey::create()
                        .from(
                            StoragePolicyCredentials::Table,
                            StoragePolicyCredentials::PolicyId,
                        )
                        .to(StoragePolicies::Table, StoragePolicies::Id)
                        .on_delete(ForeignKeyAction::Cascade),
                )
                .to_owned(),
        )
        .await
}

async fn create_storage_policy_authorization_flows(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    manager
        .create_table(
            Table::create()
                .table(StoragePolicyAuthorizationFlows::Table)
                .if_not_exists()
                .col(big_integer_pk(StoragePolicyAuthorizationFlows::Id))
                .col(
                    ColumnDef::new(StoragePolicyAuthorizationFlows::Provider)
                        .string_len(32)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(StoragePolicyAuthorizationFlows::PolicyId)
                        .big_integer()
                        .null(),
                )
                .col(
                    ColumnDef::new(StoragePolicyAuthorizationFlows::CreatedByUserId)
                        .big_integer()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(StoragePolicyAuthorizationFlows::StateHash)
                        .string_len(128)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(StoragePolicyAuthorizationFlows::PkceVerifier)
                        .text()
                        .null(),
                )
                .col(
                    ColumnDef::new(StoragePolicyAuthorizationFlows::RedirectUri)
                        .text()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(StoragePolicyAuthorizationFlows::Scopes)
                        .text()
                        .not_null(),
                )
                .col(json_text_column_for_final_schema(
                    manager,
                    StoragePolicyAuthorizationFlows::Context,
                ))
                .col(
                    ColumnDef::new(StoragePolicyAuthorizationFlows::Status)
                        .string_len(32)
                        .not_null(),
                )
                .col(
                    crate::time::utc_date_time_column(
                        manager,
                        StoragePolicyAuthorizationFlows::CreatedAt,
                    )
                    .not_null(),
                )
                .col(
                    crate::time::utc_date_time_column(
                        manager,
                        StoragePolicyAuthorizationFlows::ExpiresAt,
                    )
                    .not_null(),
                )
                .col(
                    crate::time::utc_date_time_column(
                        manager,
                        StoragePolicyAuthorizationFlows::ConsumedAt,
                    )
                    .null(),
                )
                .foreign_key(
                    ForeignKey::create()
                        .from(
                            StoragePolicyAuthorizationFlows::Table,
                            StoragePolicyAuthorizationFlows::PolicyId,
                        )
                        .to(StoragePolicies::Table, StoragePolicies::Id)
                        .on_delete(ForeignKeyAction::Cascade),
                )
                .foreign_key(
                    ForeignKey::create()
                        .from(
                            StoragePolicyAuthorizationFlows::Table,
                            StoragePolicyAuthorizationFlows::CreatedByUserId,
                        )
                        .to(Users::Table, Users::Id)
                        .on_delete(ForeignKeyAction::Cascade),
                )
                .to_owned(),
        )
        .await
}

async fn create_indexes(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_index(
            Index::create()
                .name("idx_storage_policy_credentials_policy_provider_kind")
                .table(StoragePolicyCredentials::Table)
                .col(StoragePolicyCredentials::PolicyId)
                .col(StoragePolicyCredentials::Provider)
                .col(StoragePolicyCredentials::CredentialKind)
                .unique()
                .to_owned(),
        )
        .await?;

    manager
        .create_index(
            Index::create()
                .name("idx_storage_policy_credentials_status")
                .table(StoragePolicyCredentials::Table)
                .col(StoragePolicyCredentials::Status)
                .to_owned(),
        )
        .await?;

    manager
        .create_index(
            Index::create()
                .name("idx_storage_policy_authorization_flows_state")
                .table(StoragePolicyAuthorizationFlows::Table)
                .col(StoragePolicyAuthorizationFlows::StateHash)
                .unique()
                .to_owned(),
        )
        .await?;

    manager
        .create_index(
            Index::create()
                .name("idx_storage_policy_authorization_flows_policy")
                .table(StoragePolicyAuthorizationFlows::Table)
                .col(StoragePolicyAuthorizationFlows::PolicyId)
                .to_owned(),
        )
        .await?;

    manager
        .create_index(
            Index::create()
                .name("idx_storage_policy_authorization_flows_expires_at")
                .table(StoragePolicyAuthorizationFlows::Table)
                .col(StoragePolicyAuthorizationFlows::ExpiresAt)
                .to_owned(),
        )
        .await
}

fn big_integer_pk<T: IntoIden>(name: T) -> ColumnDef {
    let mut column = ColumnDef::new(name);
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
enum Users {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum StoragePolicyCredentials {
    Table,
    Id,
    PolicyId,
    Provider,
    CredentialKind,
    AccountLabel,
    Subject,
    TenantId,
    Scopes,
    AccessTokenCiphertext,
    RefreshTokenCiphertext,
    Metadata,
    Status,
    StatusReason,
    ExpiresAt,
    AuthorizedAt,
    LastRefreshedAt,
    LastValidatedAt,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum StoragePolicyAuthorizationFlows {
    Table,
    Id,
    Provider,
    PolicyId,
    CreatedByUserId,
    StateHash,
    PkceVerifier,
    RedirectUri,
    Scopes,
    Context,
    Status,
    CreatedAt,
    ExpiresAt,
    ConsumedAt,
}
