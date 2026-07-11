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
        for index in [
            aster_forge_db::drop_audit_logs_entity_type_created_id_index(),
            aster_forge_db::drop_audit_logs_action_created_id_index(),
            aster_forge_db::drop_audit_logs_user_created_id_index(),
            aster_forge_db::drop_audit_logs_created_id_index(),
            aster_forge_db::drop_audit_logs_action_created_user_index(),
        ] {
            manager.drop_index(index).await?;
        }
        Ok(())
    }
}
