//! Add user invitation registration flow storage.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        create_user_invitations(manager).await?;
        create_user_invitation_indexes(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(UserInvitations::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await
    }
}

async fn create_user_invitations(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            Table::create()
                .table(UserInvitations::Table)
                .if_not_exists()
                .col(
                    ColumnDef::new(UserInvitations::Id)
                        .big_integer()
                        .not_null()
                        .auto_increment()
                        .primary_key(),
                )
                .col(
                    ColumnDef::new(UserInvitations::Email)
                        .string_len(255)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(UserInvitations::TokenHash)
                        .string_len(64)
                        .not_null()
                        .unique_key(),
                )
                .col(
                    ColumnDef::new(UserInvitations::Status)
                        .string_len(16)
                        .not_null()
                        .default("pending"),
                )
                .col(
                    ColumnDef::new(UserInvitations::InvitedBy)
                        .big_integer()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(UserInvitations::AcceptedUserId)
                        .big_integer()
                        .null(),
                )
                .col(
                    crate::time::utc_date_time_column(manager, UserInvitations::ExpiresAt)
                        .not_null(),
                )
                .col(
                    crate::time::utc_date_time_column(manager, UserInvitations::CreatedAt)
                        .not_null(),
                )
                .col(
                    crate::time::utc_date_time_column(manager, UserInvitations::UpdatedAt)
                        .not_null(),
                )
                .col(crate::time::utc_date_time_column(manager, UserInvitations::AcceptedAt).null())
                .col(crate::time::utc_date_time_column(manager, UserInvitations::RevokedAt).null())
                .foreign_key(
                    ForeignKey::create()
                        .name("fk_user_invitations_invited_by")
                        .from(UserInvitations::Table, UserInvitations::InvitedBy)
                        .to(Users::Table, Users::Id)
                        .on_delete(ForeignKeyAction::Cascade),
                )
                .foreign_key(
                    ForeignKey::create()
                        .name("fk_user_invitations_accepted_user_id")
                        .from(UserInvitations::Table, UserInvitations::AcceptedUserId)
                        .to(Users::Table, Users::Id)
                        .on_delete(ForeignKeyAction::SetNull),
                )
                .to_owned(),
        )
        .await
}

async fn create_user_invitation_indexes(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_index(
            Index::create()
                .name("idx_user_invitations_email")
                .table(UserInvitations::Table)
                .col(UserInvitations::Email)
                .to_owned(),
        )
        .await?;
    manager
        .create_index(
            Index::create()
                .name("idx_user_invitations_status_expires_at")
                .table(UserInvitations::Table)
                .col(UserInvitations::Status)
                .col(UserInvitations::ExpiresAt)
                .to_owned(),
        )
        .await?;
    manager
        .create_index(
            Index::create()
                .name("idx_user_invitations_invited_by")
                .table(UserInvitations::Table)
                .col(UserInvitations::InvitedBy)
                .to_owned(),
        )
        .await?;
    manager
        .create_index(
            Index::create()
                .name("idx_user_invitations_accepted_user_id")
                .table(UserInvitations::Table)
                .col(UserInvitations::AcceptedUserId)
                .to_owned(),
        )
        .await
}

#[derive(DeriveIden)]
enum UserInvitations {
    Table,
    Id,
    Email,
    TokenHash,
    Status,
    InvitedBy,
    AcceptedUserId,
    ExpiresAt,
    CreatedAt,
    UpdatedAt,
    AcceptedAt,
    RevokedAt,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}
