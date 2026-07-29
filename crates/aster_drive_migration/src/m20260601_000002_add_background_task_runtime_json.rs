//! 数据库迁移：为后台任务增加执行期 runtime_json。

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(BackgroundTasks::Table)
                    .add_column(ColumnDef::new(BackgroundTasks::RuntimeJson).text().null())
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(BackgroundTasks::Table)
                    .drop_column(BackgroundTasks::RuntimeJson)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum BackgroundTasks {
    Table,
    RuntimeJson,
}
