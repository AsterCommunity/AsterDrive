use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::DbBackend;

pub(crate) fn json_text_column_with_default_for_supported_backends<T: IntoIden>(
    manager: &SchemaManager<'_>,
    name: T,
) -> ColumnDef {
    let mut column = ColumnDef::new(name);
    column.text();

    // MySQL rejects defaults on TEXT/JSON-like columns; other backends can keep
    // the stricter NOT NULL DEFAULT '{}' shape.
    if manager.get_database_backend() == DbBackend::MySql {
        column.null();
    } else {
        column.not_null().default("{}");
    }

    column
}
