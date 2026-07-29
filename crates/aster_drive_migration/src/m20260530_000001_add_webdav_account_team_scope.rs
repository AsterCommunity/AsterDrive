//! Add team workspace scope to WebDAV accounts.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(WebdavAccounts::Table)
                    .add_column(ColumnDef::new(WebdavAccounts::TeamId).big_integer().null())
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_webdav_accounts_team_id")
                    .table(WebdavAccounts::Table)
                    .col(WebdavAccounts::TeamId)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("idx_webdav_accounts_team_id")
                    .table(WebdavAccounts::Table)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(WebdavAccounts::Table)
                    .drop_column(WebdavAccounts::TeamId)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum WebdavAccounts {
    Table,
    TeamId,
}
