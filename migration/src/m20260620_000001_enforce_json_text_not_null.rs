//! Enforce final NOT NULL constraints for JSON text columns that were nullable on MySQL.

use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::{ConnectionTrait, DbBackend};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for target in json_text_columns() {
            backfill_empty_json(manager, target).await?;
        }

        if manager.get_database_backend() != DbBackend::MySql {
            return Ok(());
        }

        for target in json_text_columns() {
            modify_json_text_nullability(manager, target, false).await?;
        }

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if manager.get_database_backend() != DbBackend::MySql {
            return Ok(());
        }

        for target in json_text_columns() {
            modify_json_text_nullability(manager, target, true).await?;
        }

        Ok(())
    }
}

#[derive(Clone, Copy)]
struct JsonTextColumn {
    table: JsonTextTable,
    column: JsonTextColumnName,
}

const fn json_text_columns() -> [JsonTextColumn; 3] {
    [
        JsonTextColumn {
            table: JsonTextTable::StoragePolicyCredentials,
            column: JsonTextColumnName::Metadata,
        },
        JsonTextColumn {
            table: JsonTextTable::StoragePolicyAuthorizationFlows,
            column: JsonTextColumnName::Context,
        },
        JsonTextColumn {
            table: JsonTextTable::StorageConnectorApplicationConfigs,
            column: JsonTextColumnName::Metadata,
        },
    ]
}

async fn backfill_empty_json(
    manager: &SchemaManager<'_>,
    target: JsonTextColumn,
) -> Result<(), DbErr> {
    manager
        .get_connection()
        .execute(
            &Query::update()
                .table(target.table)
                .value(target.column, "{}")
                .and_where(Expr::col(target.column).is_null())
                .to_owned(),
        )
        .await
        .map(|_| ())
}

async fn modify_json_text_nullability(
    manager: &SchemaManager<'_>,
    target: JsonTextColumn,
    nullable: bool,
) -> Result<(), DbErr> {
    let mut column = ColumnDef::new(target.column);
    column.text();
    if nullable {
        column.null();
    } else {
        column.not_null();
    }

    manager
        .alter_table(
            Table::alter()
                .table(target.table)
                .modify_column(column)
                .to_owned(),
        )
        .await
}

#[derive(DeriveIden, Clone, Copy)]
#[allow(clippy::enum_variant_names)]
enum JsonTextTable {
    StoragePolicyCredentials,
    StoragePolicyAuthorizationFlows,
    StorageConnectorApplicationConfigs,
}

#[derive(DeriveIden, Clone, Copy)]
enum JsonTextColumnName {
    Metadata,
    Context,
}
