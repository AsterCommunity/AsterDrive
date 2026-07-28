//! Persistent scheduled task catalog for multi-instance runtime jobs.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(aster_forge_db::create_scheduled_tasks_table(
                manager.get_database_backend(),
            ))
            .await?;
        manager
            .create_index(aster_forge_db::create_scheduled_tasks_namespace_name_unique_index())
            .await?;
        manager
            .create_index(aster_forge_db::create_scheduled_tasks_next_run_index())
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        aster_forge_db_migration::drop_index_if_exists(
            manager.get_connection(),
            aster_forge_db::SCHEDULED_TASKS_TABLE,
            aster_forge_db::SCHEDULED_TASK_NEXT_RUN_INDEX,
        )
        .await?;
        aster_forge_db_migration::drop_index_if_exists(
            manager.get_connection(),
            aster_forge_db::SCHEDULED_TASKS_TABLE,
            aster_forge_db::SCHEDULED_TASK_NAMESPACE_NAME_UNIQUE_INDEX,
        )
        .await?;
        manager
            .drop_table(aster_forge_db::drop_scheduled_tasks_table())
            .await
    }
}
