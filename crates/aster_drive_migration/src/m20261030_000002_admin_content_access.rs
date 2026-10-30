//! Persist administrator access contexts independently of cache availability.
use sea_orm_migration::prelude::*;
#[derive(DeriveMigrationName)]
pub struct Migration;
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(AdminContentAccesses::Table)
                    .col(
                        ColumnDef::new(AdminContentAccesses::Id)
                            .string_len(64)
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(AdminContentAccesses::ActorUserId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(AdminContentAccesses::SessionId)
                            .string_len(128)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(AdminContentAccesses::Payload)
                            .text()
                            .not_null(),
                    )
                    .col(
                        aster_forge_db_migration::utc_date_time_column(
                            manager,
                            AdminContentAccesses::ExpiresAt,
                        )
                        .not_null(),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_admin_content_access_expiry")
                    .table(AdminContentAccesses::Table)
                    .col(AdminContentAccesses::ExpiresAt)
                    .to_owned(),
            )
            .await
    }
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(AdminContentAccesses::Table).to_owned())
            .await
    }
}
#[derive(DeriveIden)]
enum AdminContentAccesses {
    Table,
    Id,
    ActorUserId,
    SessionId,
    Payload,
    ExpiresAt,
}

#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm_migration::sea_orm::{ConnectionTrait, Database};

    #[tokio::test]
    async fn access_migration_roundtrip_preserves_existing_product_data() {
        let db = Database::connect("sqlite::memory:").await.unwrap();
        db.execute_unprepared("CREATE TABLE retained_audit (id INTEGER PRIMARY KEY, evidence TEXT NOT NULL); INSERT INTO retained_audit VALUES (1, 'retained')").await.unwrap();
        let manager = SchemaManager::new(&db);
        Migration.up(&manager).await.unwrap();
        assert!(manager.has_table("admin_content_accesses").await.unwrap());
        db.execute_unprepared("INSERT INTO admin_content_accesses (id, actor_user_id, session_id, payload, expires_at) VALUES ('temporary', 1, 'session', '{}', '2026-10-30 00:00:00')").await.unwrap();
        Migration.down(&manager).await.unwrap();
        assert!(!manager.has_table("admin_content_accesses").await.unwrap());
        let evidence = db
            .query_one_raw(sea_orm_migration::sea_orm::Statement::from_string(
                sea_orm_migration::sea_orm::DbBackend::Sqlite,
                "SELECT evidence FROM retained_audit WHERE id = 1",
            ))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(evidence.try_get_by_index::<String>(0).unwrap(), "retained");
        Migration.up(&manager).await.unwrap();
        assert!(manager.has_table("admin_content_accesses").await.unwrap());
    }
}
