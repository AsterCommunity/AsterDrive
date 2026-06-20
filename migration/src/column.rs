use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::DbBackend;

pub(crate) fn json_text_column_for_final_schema<T: IntoIden>(
    manager: &SchemaManager<'_>,
    name: T,
) -> ColumnDef {
    let mut column = ColumnDef::new(name);
    column.text();

    if manager.get_database_backend() == DbBackend::MySql {
        column.not_null();
    } else {
        column.not_null().default("{}");
    }

    column
}

pub(crate) fn nullable_json_text_column_for_backfill<T: IntoIden>(name: T) -> ColumnDef {
    let mut column = ColumnDef::new(name);
    column.text().null();
    column
}
