//! Persist the structured folder icon discriminator and value.

use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::DbBackend;

const ICON_CHECK_NAME: &str = "ck_folders_icon_kind_value";
const SQLITE_ICON_INSERT_TRIGGER: &str = "ck_folders_icon_kind_value_insert";
const SQLITE_ICON_UPDATE_TRIGGER: &str = "ck_folders_icon_kind_value_update";
const ICON_CHECK_PREDICATE: &str = "((icon_kind = 'default' AND icon_value IS NULL) OR (icon_kind IN ('builtin', 'emoji') AND icon_value IS NOT NULL))";

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Folders::Table)
                    .add_column(
                        ColumnDef::new(Folders::IconKind)
                            .string_len(16)
                            .not_null()
                            .default("default"),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Folders::Table)
                    .add_column(ColumnDef::new(Folders::IconValue).string_len(64).null())
                    .to_owned(),
            )
            .await?;
        create_icon_check(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_icon_check(manager).await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Folders::Table)
                    .drop_column(Folders::IconValue)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(Folders::Table)
                    .drop_column(Folders::IconKind)
                    .to_owned(),
            )
            .await
    }
}

async fn create_icon_check(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let db = manager.get_connection();
    match db.get_database_backend() {
        DbBackend::Sqlite => {
            let predicate = ICON_CHECK_PREDICATE
                .replace("icon_kind", "NEW.icon_kind")
                .replace("icon_value", "NEW.icon_value");
            db.execute_unprepared(&format!(
                "CREATE TRIGGER {SQLITE_ICON_INSERT_TRIGGER} \
                 BEFORE INSERT ON folders FOR EACH ROW WHEN NOT {predicate} \
                 BEGIN SELECT RAISE(ABORT, 'invalid folder icon kind/value'); END; \
                 CREATE TRIGGER {SQLITE_ICON_UPDATE_TRIGGER} \
                 BEFORE UPDATE OF icon_kind, icon_value ON folders FOR EACH ROW WHEN NOT {predicate} \
                 BEGIN SELECT RAISE(ABORT, 'invalid folder icon kind/value'); END;"
            ))
            .await?;
        }
        DbBackend::Postgres | DbBackend::MySql => {
            db.execute_unprepared(&format!(
                "ALTER TABLE folders ADD CONSTRAINT {ICON_CHECK_NAME} CHECK ({ICON_CHECK_PREDICATE})"
            ))
            .await?;
        }
        backend => {
            return Err(DbErr::Custom(format!(
                "unsupported database backend for folder icon constraint: {backend:?}"
            )));
        }
    }
    Ok(())
}

async fn drop_icon_check(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let db = manager.get_connection();
    match db.get_database_backend() {
        DbBackend::Sqlite => {
            db.execute_unprepared(&format!(
                "DROP TRIGGER {SQLITE_ICON_UPDATE_TRIGGER}; \
                 DROP TRIGGER {SQLITE_ICON_INSERT_TRIGGER};"
            ))
            .await?;
        }
        DbBackend::Postgres => {
            db.execute_unprepared(&format!(
                "ALTER TABLE folders DROP CONSTRAINT {ICON_CHECK_NAME}"
            ))
            .await?;
        }
        DbBackend::MySql => {
            db.execute_unprepared(&format!("ALTER TABLE folders DROP CHECK {ICON_CHECK_NAME}"))
                .await?;
        }
        backend => {
            return Err(DbErr::Custom(format!(
                "unsupported database backend for folder icon constraint removal: {backend:?}"
            )));
        }
    }
    Ok(())
}

#[derive(DeriveIden)]
enum Folders {
    Table,
    IconKind,
    IconValue,
}

#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm_migration::sea_orm::{ConnectionTrait, Database, DbBackend, Statement};

    #[tokio::test]
    async fn migration_backfills_existing_folders_and_is_reversible() {
        let db = Database::connect("sqlite::memory:").await.unwrap();
        db.execute_unprepared(
            "CREATE TABLE folders (id INTEGER PRIMARY KEY, name TEXT NOT NULL); \
             INSERT INTO folders (id, name) VALUES (1, 'Existing');",
        )
        .await
        .unwrap();
        let manager = SchemaManager::new(&db);

        Migration.up(&manager).await.unwrap();
        let row = db
            .query_one_raw(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT icon_kind, icon_value FROM folders WHERE id = 1",
            ))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(row.try_get_by_index::<String>(0).unwrap(), "default");
        assert_eq!(row.try_get_by_index::<Option<String>>(1).unwrap(), None);
        assert!(
            db.execute_unprepared("UPDATE folders SET icon_kind = 'builtin', icon_value = NULL")
                .await
                .is_err()
        );
        db.execute_unprepared("UPDATE folders SET icon_kind = 'builtin', icon_value = 'documents'")
            .await
            .unwrap();
        Migration.down(&manager).await.unwrap();
        assert!(!manager.has_column("folders", "icon_kind").await.unwrap());
        assert!(!manager.has_column("folders", "icon_value").await.unwrap());
    }
}
