//! Add the `access_mode` column to `webdav_accounts` so each account can be
//! restricted to read-only access independently of its owner's product
//! permissions. Existing accounts default to `read_write`, preserving current
//! behavior.
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[derive(DeriveIden)]
enum WebdavAccounts {
    Table,
    AccessMode,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(WebdavAccounts::Table)
                    .add_column(
                        ColumnDef::new(WebdavAccounts::AccessMode)
                            .string_len(16)
                            .not_null()
                            .default("read_write")
                            .to_owned(),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(WebdavAccounts::Table)
                    .drop_column(WebdavAccounts::AccessMode)
                    .to_owned(),
            )
            .await
    }
}
