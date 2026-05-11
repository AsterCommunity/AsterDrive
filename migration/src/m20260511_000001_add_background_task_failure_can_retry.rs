//! Track whether a failed background task can be manually retried.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

const BACKGROUND_TASKS_TABLE: &str = "background_tasks";
const FAILURE_CAN_RETRY_COLUMN: &str = "failure_can_retry";

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if manager
            .has_column(BACKGROUND_TASKS_TABLE, FAILURE_CAN_RETRY_COLUMN)
            .await?
        {
            return Ok(());
        }

        manager
            .alter_table(
                Table::alter()
                    .table(BackgroundTasks::Table)
                    .add_column(
                        ColumnDef::new(BackgroundTasks::FailureCanRetry)
                            .boolean()
                            .null(),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if !manager
            .has_column(BACKGROUND_TASKS_TABLE, FAILURE_CAN_RETRY_COLUMN)
            .await?
        {
            return Ok(());
        }

        manager
            .alter_table(
                Table::alter()
                    .table(BackgroundTasks::Table)
                    .drop_column(BackgroundTasks::FailureCanRetry)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum BackgroundTasks {
    Table,
    FailureCanRetry,
}
