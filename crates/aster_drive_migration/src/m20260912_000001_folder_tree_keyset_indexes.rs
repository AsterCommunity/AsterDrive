//! Indexes for the keyset pages used by folder-tree delete/restore traversal.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for index in [
            Index::create()
                .name("idx_folders_owner_team_parent_id")
                .table(Folders::Table)
                .col(Folders::OwnerUserId)
                .col(Folders::TeamId)
                .col(Folders::ParentId)
                .col(Folders::Id)
                .to_owned(),
            Index::create()
                .name("idx_folders_owner_team_parent_deleted_id")
                .table(Folders::Table)
                .col(Folders::OwnerUserId)
                .col(Folders::TeamId)
                .col(Folders::ParentId)
                .col(Folders::DeletedAt)
                .col(Folders::Id)
                .to_owned(),
            Index::create()
                .name("idx_folders_team_parent_id")
                .table(Folders::Table)
                .col(Folders::TeamId)
                .col(Folders::ParentId)
                .col(Folders::Id)
                .to_owned(),
            Index::create()
                .name("idx_folders_team_parent_deleted_id")
                .table(Folders::Table)
                .col(Folders::TeamId)
                .col(Folders::ParentId)
                .col(Folders::DeletedAt)
                .col(Folders::Id)
                .to_owned(),
            Index::create()
                .name("idx_files_owner_team_folder_id")
                .table(Files::Table)
                .col(Files::OwnerUserId)
                .col(Files::TeamId)
                .col(Files::FolderId)
                .col(Files::Id)
                .to_owned(),
            Index::create()
                .name("idx_files_owner_team_folder_deleted_id")
                .table(Files::Table)
                .col(Files::OwnerUserId)
                .col(Files::TeamId)
                .col(Files::FolderId)
                .col(Files::DeletedAt)
                .col(Files::Id)
                .to_owned(),
            Index::create()
                .name("idx_files_team_folder_id")
                .table(Files::Table)
                .col(Files::TeamId)
                .col(Files::FolderId)
                .col(Files::Id)
                .to_owned(),
            Index::create()
                .name("idx_files_team_folder_deleted_id")
                .table(Files::Table)
                .col(Files::TeamId)
                .col(Files::FolderId)
                .col(Files::DeletedAt)
                .col(Files::Id)
                .to_owned(),
        ] {
            manager.create_index(index).await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for index in [
            "idx_folders_owner_team_parent_id",
            "idx_folders_owner_team_parent_deleted_id",
            "idx_folders_team_parent_id",
            "idx_folders_team_parent_deleted_id",
        ] {
            manager
                .drop_index(Index::drop().name(index).table(Folders::Table).to_owned())
                .await?;
        }
        for index in [
            "idx_files_owner_team_folder_id",
            "idx_files_owner_team_folder_deleted_id",
            "idx_files_team_folder_id",
            "idx_files_team_folder_deleted_id",
        ] {
            manager
                .drop_index(Index::drop().name(index).table(Files::Table).to_owned())
                .await?;
        }
        Ok(())
    }
}

#[derive(DeriveIden)]
enum Folders {
    Table,
    OwnerUserId,
    TeamId,
    ParentId,
    DeletedAt,
    Id,
}

#[derive(DeriveIden)]
enum Files {
    Table,
    OwnerUserId,
    TeamId,
    FolderId,
    DeletedAt,
    Id,
}
