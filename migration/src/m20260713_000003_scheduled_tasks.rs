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
        manager
            .drop_index(aster_forge_db::drop_scheduled_tasks_next_run_index())
            .await?;
        manager
            .drop_index(aster_forge_db::drop_scheduled_tasks_namespace_name_unique_index())
            .await?;
        manager
            .drop_table(aster_forge_db::drop_scheduled_tasks_table())
            .await
    }
}
