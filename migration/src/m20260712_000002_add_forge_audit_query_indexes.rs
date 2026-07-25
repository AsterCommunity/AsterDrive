//! Install Forge's shared query indexes on the product-owned audit table.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for index in aster_forge_db::create_audit_logs_query_indexes() {
            manager.create_index(index).await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for index_name in [
            aster_forge_db::AUDIT_LOG_ENTITY_TYPE_CREATED_ID_INDEX,
            aster_forge_db::AUDIT_LOG_ACTION_CREATED_ID_INDEX,
            aster_forge_db::AUDIT_LOG_USER_CREATED_ID_INDEX,
            aster_forge_db::AUDIT_LOG_CREATED_ID_INDEX,
            aster_forge_db::AUDIT_LOG_ACTION_CREATED_USER_INDEX,
        ] {
            aster_forge_db_migration::drop_index_if_exists(
                manager.get_connection(),
                aster_forge_db::AUDIT_LOGS_TABLE,
                index_name,
            )
            .await?;
        }
        Ok(())
    }
}
