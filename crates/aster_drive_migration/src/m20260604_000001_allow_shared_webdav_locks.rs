//! Allow multiple shared WebDAV locks on the same resource.

use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::{DbBackend, Statement};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_resource_locks_entity_index(manager).await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_resource_locks_entity")
                    .table(ResourceLocks::Table)
                    .col(ResourceLocks::EntityType)
                    .col(ResourceLocks::EntityId)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        abort_if_duplicate_entity_locks_exist(manager).await?;

        drop_resource_locks_entity_index(manager).await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_resource_locks_entity")
                    .table(ResourceLocks::Table)
                    .col(ResourceLocks::EntityType)
                    .col(ResourceLocks::EntityId)
                    .unique()
                    .to_owned(),
            )
            .await
    }
}

async fn drop_resource_locks_entity_index(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    const INDEX_NAME: &str = "idx_resource_locks_entity";

    match manager.get_database_backend() {
        DbBackend::MySql => {
            let exists = manager
                .get_connection()
                .query_one_raw(Statement::from_string(
                    DbBackend::MySql,
                    "SELECT 1 \
                     FROM INFORMATION_SCHEMA.STATISTICS \
                     WHERE TABLE_SCHEMA = DATABASE() \
                       AND TABLE_NAME = 'resource_locks' \
                       AND INDEX_NAME = 'idx_resource_locks_entity' \
                     LIMIT 1",
                ))
                .await?
                .is_some();
            if exists {
                manager
                    .get_connection()
                    .execute_unprepared("DROP INDEX idx_resource_locks_entity ON resource_locks")
                    .await?;
            }
            Ok(())
        }
        DbBackend::Sqlite | DbBackend::Postgres => {
            manager
                .drop_index(
                    Index::drop()
                        .name(INDEX_NAME)
                        .table(ResourceLocks::Table)
                        .if_exists()
                        .to_owned(),
                )
                .await
        }
        backend => Err(DbErr::Migration(format!(
            "unsupported database backend for resource lock index migration: {backend:?}"
        ))),
    }
}

async fn abort_if_duplicate_entity_locks_exist(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let select = Query::select()
        .column(ResourceLocks::EntityType)
        .column(ResourceLocks::EntityId)
        .expr_as(
            Expr::col(ResourceLocks::Id).count(),
            Alias::new("lock_count"),
        )
        .from(ResourceLocks::Table)
        .group_by_col(ResourceLocks::EntityType)
        .group_by_col(ResourceLocks::EntityId)
        .and_having(Expr::col(ResourceLocks::Id).count().gt(1))
        .order_by(ResourceLocks::EntityType, Order::Asc)
        .order_by(ResourceLocks::EntityId, Order::Asc)
        .to_owned();

    let duplicates = manager
        .get_connection()
        .query_all(&select)
        .await?
        .into_iter()
        .map(|row| {
            let entity_type = row.try_get_by_index::<String>(0)?;
            let entity_id = row.try_get_by_index::<i64>(1)?;
            let lock_count = row.try_get_by_index::<i64>(2)?;
            Ok(format!("{entity_type}:{entity_id} ({lock_count} locks)"))
        })
        .collect::<Result<Vec<_>, DbErr>>()?;

    if duplicates.is_empty() {
        return Ok(());
    }

    Err(DbErr::Migration(format!(
        "cannot recreate unique index idx_resource_locks_entity; duplicate resource_locks rows exist for: {}. Remove or merge duplicate locks before rolling this migration back.",
        duplicates.join(", ")
    )))
}

#[derive(DeriveIden)]
enum ResourceLocks {
    Table,
    Id,
    EntityType,
    EntityId,
}
