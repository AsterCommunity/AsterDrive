//! Shared index helpers for database migrations.

use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::{ConnectionTrait, DbBackend, Statement};

pub(crate) async fn drop_index_if_exists(
    manager: &SchemaManager<'_>,
    table_name: &str,
    index_name: &str,
) -> Result<(), DbErr> {
    if manager.get_database_backend() == DbBackend::MySql
        && !mysql_index_exists(manager, table_name, index_name).await?
    {
        return Ok(());
    }

    let mut drop = Index::drop();
    drop.name(index_name).table(Alias::new(table_name));
    if manager.get_database_backend() != DbBackend::MySql {
        drop.if_exists();
    }

    manager.drop_index(drop.to_owned()).await
}

pub(crate) async fn mysql_index_exists(
    manager: &SchemaManager<'_>,
    table_name: &str,
    index_name: &str,
) -> Result<bool, DbErr> {
    let db = manager.get_connection();
    let row = db
        .query_one_raw(Statement::from_sql_and_values(
            DbBackend::MySql,
            "SELECT 1 FROM information_schema.statistics \
             WHERE table_schema = DATABASE() AND table_name = ? AND index_name = ? LIMIT 1",
            [table_name.into(), index_name.into()],
        ))
        .await?;

    Ok(row.is_some())
}

pub(crate) async fn rename_mysql_index_if_exists(
    manager: &SchemaManager<'_>,
    table_name: &str,
    old_index_name: &str,
    new_index_name: &str,
) -> Result<(), DbErr> {
    if !mysql_index_exists(manager, table_name, old_index_name).await?
        || mysql_index_exists(manager, table_name, new_index_name).await?
    {
        return Ok(());
    }

    manager
        .get_connection()
        .execute_unprepared(&format!(
            "ALTER TABLE `{table_name}` RENAME INDEX `{old_index_name}` TO `{new_index_name}`"
        ))
        .await?;

    Ok(())
}
