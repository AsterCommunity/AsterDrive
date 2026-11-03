//! Keep migration execution within the default Windows main-thread stack.

use aster_drive_migration::{CurrentMigrator, Migrator, MigratorTrait};
use sea_orm_migration::{SchemaManager, sea_orm::Database};

#[test]
fn sqlite_migrations_fit_one_mib_stack() {
    std::thread::Builder::new()
        .name("migration-one-mib-stack".into())
        .stack_size(1024 * 1024)
        .spawn(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async {
                    let db = Database::connect("sqlite::memory:").await.unwrap();
                    // Exercise both baseline directions before the complete
                    // startup path, including later migrations and retries.
                    CurrentMigrator::up(&db, Some(1)).await.unwrap();
                    let manager = SchemaManager::new(&db);
                    assert!(manager.has_table("users").await.unwrap());
                    CurrentMigrator::down(&db, Some(1)).await.unwrap();
                    assert!(!manager.has_table("users").await.unwrap());
                    db.close().await.unwrap();

                    let db = Database::connect("sqlite::memory:").await.unwrap();
                    Migrator::up(&db, None).await.unwrap();
                    Migrator::up(&db, None).await.unwrap();
                    let history = aster_drive_migration::inspect_migration_history(&db)
                        .await
                        .unwrap();
                    assert_eq!(
                        history.applied,
                        aster_drive_migration::current_migration_names()
                    );
                    assert!(history.pending_current.is_empty());
                    db.close().await.unwrap();
                });
        })
        .unwrap()
        .join()
        .unwrap();
}
