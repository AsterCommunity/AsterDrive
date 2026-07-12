//! Multi-instance singleton runtime lease table.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(aster_forge_db::create_runtime_leases_table(
                manager.get_database_backend(),
            ))
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(aster_forge_db::drop_runtime_leases_table())
            .await
    }
}
