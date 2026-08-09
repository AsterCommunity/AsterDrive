//! Staged membership for bounded folder-tree background mutations.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(FolderTreeOperationMembers::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(FolderTreeOperationMembers::TaskId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(FolderTreeOperationMembers::ResourceKind)
                            .string_len(16)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(FolderTreeOperationMembers::ResourceId)
                            .big_integer()
                            .not_null(),
                    )
                    .primary_key(
                        Index::create()
                            .col(FolderTreeOperationMembers::TaskId)
                            .col(FolderTreeOperationMembers::ResourceKind)
                            .col(FolderTreeOperationMembers::ResourceId),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_folder_tree_operation_members_task")
                            .from(
                                FolderTreeOperationMembers::Table,
                                FolderTreeOperationMembers::TaskId,
                            )
                            .to(BackgroundTasks::Table, BackgroundTasks::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(FolderTreeOperationMembers::Table)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum FolderTreeOperationMembers {
    Table,
    TaskId,
    ResourceKind,
    ResourceId,
}

#[derive(DeriveIden)]
enum BackgroundTasks {
    Table,
    Id,
}
