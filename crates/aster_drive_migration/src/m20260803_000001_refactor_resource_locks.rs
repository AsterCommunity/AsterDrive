//! Replace path/boolean-oriented resource locks with workspace-owned lock roots.

use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::{ConnectionTrait, DbBackend, Statement};

#[derive(DeriveMigrationName)]
pub struct Migration;

const REBUILT_TABLE: &str = "resource_locks__workspace_rebuild";
const LEGACY_REBUILT_TABLE: &str = "resource_locks__legacy_rebuild";

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        validate_legacy_locks(manager).await?;
        create_namespaces(manager).await?;
        backfill_namespaces(manager).await?;
        replace_resource_locks(manager).await?;
        drop_derived_lock_columns(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        validate_downgrade_locks(manager).await?;
        replace_resource_locks_with_legacy(manager).await?;
        restore_derived_lock_columns(manager).await
    }
}

async fn validate_downgrade_locks(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let backend = manager.get_database_backend();
    let sql = "SELECT COUNT(*) \
               FROM resource_locks rl \
               LEFT JOIN resource_lock_namespaces ns ON ns.id = rl.namespace_id \
               LEFT JOIN files f ON rl.root_kind = 'file' AND f.id = rl.root_file_id \
               LEFT JOIN folders d ON rl.root_kind = 'folder' AND d.id = rl.root_folder_id \
               WHERE ns.id IS NULL \
                  OR ns.workspace_type NOT IN ('personal', 'team') \
                  OR rl.lockroot_path IS NULL \
                  OR rl.depth NOT IN ('resource', 'infinity') \
                  OR rl.mode NOT IN ('exclusive', 'shared') \
                  OR (rl.root_kind = 'workspace_root' AND (rl.root_folder_id IS NOT NULL OR rl.root_file_id IS NOT NULL)) \
                  OR (rl.root_kind = 'folder' AND (rl.root_folder_id IS NULL OR rl.root_file_id IS NOT NULL OR d.id IS NULL)) \
                  OR (rl.root_kind = 'file' AND (rl.root_folder_id IS NOT NULL OR rl.root_file_id IS NULL OR f.id IS NULL)) \
                  OR rl.root_kind NOT IN ('workspace_root', 'folder', 'file') \
                  OR (rl.root_kind = 'folder' AND NOT ( \
                       (ns.workspace_type = 'team' AND d.team_id = ns.workspace_id) \
                    OR (ns.workspace_type = 'personal' AND d.team_id IS NULL AND d.owner_user_id = ns.workspace_id))) \
                  OR (rl.root_kind = 'file' AND NOT ( \
                       (ns.workspace_type = 'team' AND f.team_id = ns.workspace_id) \
                    OR (ns.workspace_type = 'personal' AND f.team_id IS NULL AND f.owner_user_id = ns.workspace_id)))";
    let invalid_count = manager
        .get_connection()
        .query_one_raw(Statement::from_string(backend, sql))
        .await?
        .ok_or_else(|| {
            DbErr::Migration("resource lock downgrade validation returned no row".to_string())
        })?
        .try_get_by_index::<i64>(0)
        .map_err(|error| {
            DbErr::Migration(format!(
                "failed to decode invalid resource lock downgrade count: {error}"
            ))
        })?;

    if invalid_count != 0 {
        return Err(DbErr::Migration(format!(
            "cannot downgrade resource locks: {invalid_count} lock row(s) cannot be represented by the legacy lock schema"
        )));
    }
    Ok(())
}

async fn replace_resource_locks_with_legacy(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let connection = manager.get_connection();
    if manager.get_database_backend() == DbBackend::Sqlite {
        connection
            .execute_unprepared("PRAGMA foreign_keys = OFF")
            .await?;
    }

    let result = async {
        manager
            .drop_table(
                Table::drop()
                    .table(Alias::new(LEGACY_REBUILT_TABLE))
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        manager
            .create_table(legacy_resource_locks_table(
                Alias::new(LEGACY_REBUILT_TABLE),
                manager,
            ))
            .await?;

        let backend = manager.get_database_backend();
        let copy_sql = format!(
            "INSERT INTO {LEGACY_REBUILT_TABLE} \
             (id, token, entity_type, entity_id, path, owner_id, owner_info, timeout_at, shared, deep, created_at) \
             SELECT rl.id, rl.token, \
                    CASE \
                      WHEN rl.root_kind = 'workspace_root' AND ns.workspace_type = 'personal' THEN 'personal_root' \
                      WHEN rl.root_kind = 'workspace_root' THEN 'team_root' \
                      WHEN rl.root_kind = 'folder' THEN 'folder' ELSE 'file' END, \
                    CASE \
                      WHEN rl.root_kind = 'workspace_root' THEN ns.workspace_id \
                      WHEN rl.root_kind = 'folder' THEN rl.root_folder_id ELSE rl.root_file_id END, \
                    rl.lockroot_path, rl.holder_user_id, rl.owner_info, rl.timeout_at, \
                    CASE WHEN rl.mode = 'shared' THEN TRUE ELSE FALSE END, \
                    CASE WHEN rl.depth = 'infinity' THEN TRUE ELSE FALSE END, \
                    rl.created_at \
             FROM resource_locks rl \
             JOIN resource_lock_namespaces ns ON ns.id = rl.namespace_id"
        );
        connection
            .execute_raw(Statement::from_string(backend, copy_sql))
            .await?;

        manager
            .drop_table(Table::drop().table(ResourceLocks::Table).to_owned())
            .await?;
        manager
            .rename_table(
                Table::rename()
                    .table(Alias::new(LEGACY_REBUILT_TABLE), ResourceLocks::Table)
                    .to_owned(),
            )
            .await?;
        create_legacy_resource_lock_indexes(manager).await?;
        manager
            .drop_table(
                Table::drop()
                    .table(ResourceLockNamespaces::Table)
                    .to_owned(),
            )
            .await?;
        reset_resource_lock_sequence(manager).await
    }
    .await;

    if manager.get_database_backend() == DbBackend::Sqlite {
        let restore_result = connection
            .execute_unprepared("PRAGMA foreign_keys = ON")
            .await;
        if let Err(error) = result {
            restore_result.map_err(|restore_error| {
                DbErr::Migration(format!(
                    "resource lock downgrade failed: {error}; also failed to restore SQLite foreign keys: {restore_error}"
                ))
            })?;
            return Err(error);
        }
        restore_result?;
        let violations = connection
            .query_all_raw(Statement::from_string(
                DbBackend::Sqlite,
                "PRAGMA foreign_key_check",
            ))
            .await?;
        if !violations.is_empty() {
            return Err(DbErr::Migration(format!(
                "resource lock downgrade introduced {} foreign key violation(s)",
                violations.len()
            )));
        }
        return Ok(());
    }
    result
}

async fn restore_derived_lock_columns(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .alter_table(
            Table::alter()
                .table(Files::Table)
                .add_column(
                    ColumnDef::new(Files::IsLocked)
                        .boolean()
                        .not_null()
                        .default(false),
                )
                .to_owned(),
        )
        .await?;
    manager
        .alter_table(
            Table::alter()
                .table(Folders::Table)
                .add_column(
                    ColumnDef::new(Folders::IsLocked)
                        .boolean()
                        .not_null()
                        .default(false),
                )
                .to_owned(),
        )
        .await?;

    let backend = manager.get_database_backend();
    for sql in [
        "UPDATE files SET is_locked = TRUE WHERE EXISTS (SELECT 1 FROM resource_locks rl WHERE rl.entity_type = 'file' AND rl.entity_id = files.id)",
        "UPDATE folders SET is_locked = TRUE WHERE EXISTS (SELECT 1 FROM resource_locks rl WHERE rl.entity_type = 'folder' AND rl.entity_id = folders.id)",
    ] {
        manager
            .get_connection()
            .execute_raw(Statement::from_string(backend, sql))
            .await?;
    }
    Ok(())
}

async fn drop_derived_lock_columns(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .alter_table(
            Table::alter()
                .table(Files::Table)
                .drop_column(Files::IsLocked)
                .to_owned(),
        )
        .await?;
    manager
        .alter_table(
            Table::alter()
                .table(Folders::Table)
                .drop_column(Folders::IsLocked)
                .to_owned(),
        )
        .await
}

async fn validate_legacy_locks(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let backend = manager.get_database_backend();
    let sql = "SELECT COUNT(*) \
               FROM resource_locks rl \
               LEFT JOIN files f ON rl.entity_type = 'file' AND f.id = rl.entity_id \
               LEFT JOIN folders d ON rl.entity_type = 'folder' AND d.id = rl.entity_id \
               LEFT JOIN users u ON rl.entity_type = 'personal_root' AND u.id = rl.entity_id \
               LEFT JOIN teams t ON rl.entity_type = 'team_root' AND t.id = rl.entity_id \
               WHERE (rl.entity_type = 'file' AND (f.id IS NULL OR (f.team_id IS NULL AND f.owner_user_id IS NULL))) \
                  OR (rl.entity_type = 'folder' AND (d.id IS NULL OR (d.team_id IS NULL AND d.owner_user_id IS NULL))) \
                  OR (rl.entity_type = 'personal_root' AND u.id IS NULL) \
                  OR (rl.entity_type = 'team_root' AND t.id IS NULL) \
                  OR rl.entity_type NOT IN ('file', 'folder', 'personal_root', 'team_root')";
    let invalid_count = manager
        .get_connection()
        .query_one_raw(Statement::from_string(backend, sql))
        .await?
        .ok_or_else(|| DbErr::Migration("resource lock validation returned no row".to_string()))?
        .try_get_by_index::<i64>(0)
        .map_err(|error| {
            DbErr::Migration(format!(
                "failed to decode invalid resource lock count: {error}"
            ))
        })?;

    if invalid_count != 0 {
        return Err(DbErr::Migration(format!(
            "cannot migrate resource locks: {invalid_count} lock row(s) have an unresolved or invalid workspace/root identity"
        )));
    }
    Ok(())
}

async fn create_namespaces(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            Table::create()
                .table(ResourceLockNamespaces::Table)
                .col(aster_forge_db_migration::big_integer_primary_key(
                    ResourceLockNamespaces::Id,
                ))
                .col(
                    ColumnDef::new(ResourceLockNamespaces::WorkspaceType)
                        .string_len(16)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(ResourceLockNamespaces::WorkspaceId)
                        .big_integer()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(ResourceLockNamespaces::Generation)
                        .big_integer()
                        .not_null()
                        .default(0),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(
                        manager,
                        ResourceLockNamespaces::CreatedAt,
                    )
                    .not_null(),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(
                        manager,
                        ResourceLockNamespaces::UpdatedAt,
                    )
                    .not_null(),
                )
                .to_owned(),
        )
        .await?;
    manager
        .create_index(
            Index::create()
                .name("uq_resource_lock_namespaces_workspace")
                .table(ResourceLockNamespaces::Table)
                .col(ResourceLockNamespaces::WorkspaceType)
                .col(ResourceLockNamespaces::WorkspaceId)
                .unique()
                .to_owned(),
        )
        .await
}

async fn backfill_namespaces(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let backend = manager.get_database_backend();
    for sql in [
        "INSERT INTO resource_lock_namespaces \
         (workspace_type, workspace_id, generation, created_at, updated_at) \
         SELECT 'personal', id, 0, created_at, updated_at FROM users",
        "INSERT INTO resource_lock_namespaces \
         (workspace_type, workspace_id, generation, created_at, updated_at) \
         SELECT 'team', id, 0, created_at, updated_at FROM teams",
    ] {
        manager
            .get_connection()
            .execute_raw(Statement::from_string(backend, sql))
            .await?;
    }
    Ok(())
}

async fn replace_resource_locks(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let connection = manager.get_connection();
    if manager.get_database_backend() == DbBackend::Sqlite {
        connection
            .execute_unprepared("PRAGMA foreign_keys = OFF")
            .await?;
    }

    let result = async {
        manager
            .drop_table(
                Table::drop()
                    .table(Alias::new(REBUILT_TABLE))
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        manager
            .create_table(resource_locks_table(Alias::new(REBUILT_TABLE), manager))
            .await?;

        let backend = manager.get_database_backend();
        let copy_sql = format!(
            "INSERT INTO {REBUILT_TABLE} \
             (id, token, namespace_id, root_kind, root_folder_id, root_file_id, depth, mode, origin, \
              holder_user_id, owner_info, timeout_at, lockroot_path, created_at) \
             SELECT rl.id, rl.token, ns.id, \
                    CASE \
                      WHEN rl.entity_type IN ('personal_root', 'team_root') THEN 'workspace_root' \
                      WHEN rl.entity_type = 'folder' THEN 'folder' ELSE 'file' END, \
                    CASE WHEN rl.entity_type = 'folder' THEN rl.entity_id ELSE NULL END, \
                    CASE WHEN rl.entity_type = 'file' THEN rl.entity_id ELSE NULL END, \
                    CASE WHEN rl.deep THEN 'infinity' ELSE 'resource' END, \
                    CASE WHEN rl.shared THEN 'shared' ELSE 'exclusive' END, \
                    CASE \
                      WHEN rl.owner_info LIKE '%\"kind\":\"webdav\"%' THEN 'webdav' \
                      WHEN rl.owner_info LIKE '%\"kind\":\"wopi\"%' THEN 'wopi' \
                      ELSE 'product' END, \
                    rl.owner_id, rl.owner_info, rl.timeout_at, rl.path, rl.created_at \
             FROM resource_locks rl \
             LEFT JOIN files f ON rl.entity_type = 'file' AND f.id = rl.entity_id \
             LEFT JOIN folders d ON rl.entity_type = 'folder' AND d.id = rl.entity_id \
             JOIN resource_lock_namespaces ns \
               ON ns.workspace_type = CASE \
                    WHEN rl.entity_type = 'team_root' OR f.team_id IS NOT NULL OR d.team_id IS NOT NULL \
                      THEN 'team' ELSE 'personal' END \
              AND ns.workspace_id = CASE \
                    WHEN rl.entity_type IN ('personal_root', 'team_root') THEN rl.entity_id \
                    WHEN rl.entity_type = 'file' THEN COALESCE(f.team_id, f.owner_user_id) \
                    ELSE COALESCE(d.team_id, d.owner_user_id) END"
        );
        connection
            .execute_raw(Statement::from_string(backend, copy_sql))
            .await?;

        manager
            .drop_table(Table::drop().table(ResourceLocks::Table).to_owned())
            .await?;
        manager
            .rename_table(
                Table::rename()
                    .table(Alias::new(REBUILT_TABLE), ResourceLocks::Table)
                    .to_owned(),
            )
            .await?;
        create_resource_lock_indexes(manager).await?;
        reset_resource_lock_sequence(manager).await
    }
    .await;

    if manager.get_database_backend() == DbBackend::Sqlite {
        let restore = connection
            .execute_unprepared("PRAGMA foreign_keys = ON")
            .await;
        result?;
        restore?;
        let violations = connection
            .query_all_raw(Statement::from_string(
                DbBackend::Sqlite,
                "PRAGMA foreign_key_check",
            ))
            .await?;
        if !violations.is_empty() {
            return Err(DbErr::Migration(format!(
                "resource lock migration introduced {} foreign key violation(s)",
                violations.len()
            )));
        }
        return Ok(());
    }
    result
}

fn resource_locks_table<T>(table: T, manager: &SchemaManager<'_>) -> TableCreateStatement
where
    T: IntoIden,
{
    let table = table.into_iden();
    Table::create()
        .table(table.clone())
        .col(aster_forge_db_migration::big_integer_primary_key(
            ResourceLocks::Id,
        ))
        .col(
            ColumnDef::new(ResourceLocks::Token)
                .string()
                .not_null()
                .unique_key(),
        )
        .col(
            ColumnDef::new(ResourceLocks::NamespaceId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(ResourceLocks::RootKind)
                .string_len(16)
                .not_null(),
        )
        .col(
            ColumnDef::new(ResourceLocks::RootFolderId)
                .big_integer()
                .null(),
        )
        .col(
            ColumnDef::new(ResourceLocks::RootFileId)
                .big_integer()
                .null(),
        )
        .col(
            ColumnDef::new(ResourceLocks::Depth)
                .string_len(16)
                .not_null(),
        )
        .col(
            ColumnDef::new(ResourceLocks::Mode)
                .string_len(16)
                .not_null(),
        )
        .col(
            ColumnDef::new(ResourceLocks::Origin)
                .string_len(16)
                .not_null(),
        )
        .col(
            ColumnDef::new(ResourceLocks::HolderUserId)
                .big_integer()
                .null(),
        )
        .col(ColumnDef::new(ResourceLocks::OwnerInfo).text().null())
        .col(
            aster_forge_db_migration::utc_date_time_column(manager, ResourceLocks::TimeoutAt)
                .null(),
        )
        .col(ColumnDef::new(ResourceLocks::LockrootPath).string().null())
        .col(
            aster_forge_db_migration::utc_date_time_column(manager, ResourceLocks::CreatedAt)
                .not_null(),
        )
        .foreign_key(
            ForeignKey::create()
                .from(table.clone(), ResourceLocks::NamespaceId)
                .to(ResourceLockNamespaces::Table, ResourceLockNamespaces::Id)
                .on_delete(ForeignKeyAction::Cascade),
        )
        .foreign_key(
            ForeignKey::create()
                .from(table.clone(), ResourceLocks::RootFolderId)
                .to(Folders::Table, Folders::Id)
                .on_delete(ForeignKeyAction::Cascade),
        )
        .foreign_key(
            ForeignKey::create()
                .from(table, ResourceLocks::RootFileId)
                .to(Files::Table, Files::Id)
                .on_delete(ForeignKeyAction::Cascade),
        )
        .to_owned()
}

fn legacy_resource_locks_table<T>(table: T, manager: &SchemaManager<'_>) -> TableCreateStatement
where
    T: IntoIden,
{
    Table::create()
        .table(table)
        .col(aster_forge_db_migration::big_integer_primary_key(
            LegacyResourceLocks::Id,
        ))
        .col(
            ColumnDef::new(LegacyResourceLocks::Token)
                .string()
                .not_null()
                .unique_key(),
        )
        .col(
            ColumnDef::new(LegacyResourceLocks::EntityType)
                .string_len(16)
                .not_null(),
        )
        .col(
            ColumnDef::new(LegacyResourceLocks::EntityId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(LegacyResourceLocks::Path)
                .string()
                .not_null(),
        )
        .col(
            ColumnDef::new(LegacyResourceLocks::OwnerId)
                .big_integer()
                .null(),
        )
        .col(ColumnDef::new(LegacyResourceLocks::OwnerInfo).text().null())
        .col(
            aster_forge_db_migration::utc_date_time_column(manager, LegacyResourceLocks::TimeoutAt)
                .null(),
        )
        .col(
            ColumnDef::new(LegacyResourceLocks::Shared)
                .boolean()
                .not_null()
                .default(false),
        )
        .col(
            ColumnDef::new(LegacyResourceLocks::Deep)
                .boolean()
                .not_null()
                .default(false),
        )
        .col(
            aster_forge_db_migration::utc_date_time_column(manager, LegacyResourceLocks::CreatedAt)
                .not_null(),
        )
        .to_owned()
}

async fn create_resource_lock_indexes(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    for index in [
        Index::create()
            .name("idx_resource_locks_namespace_timeout")
            .table(ResourceLocks::Table)
            .col(ResourceLocks::NamespaceId)
            .col(ResourceLocks::TimeoutAt)
            .to_owned(),
        Index::create()
            .name("idx_resource_locks_namespace_folder")
            .table(ResourceLocks::Table)
            .col(ResourceLocks::NamespaceId)
            .col(ResourceLocks::RootKind)
            .col(ResourceLocks::RootFolderId)
            .to_owned(),
        Index::create()
            .name("idx_resource_locks_namespace_file")
            .table(ResourceLocks::Table)
            .col(ResourceLocks::NamespaceId)
            .col(ResourceLocks::RootKind)
            .col(ResourceLocks::RootFileId)
            .to_owned(),
        Index::create()
            .name("idx_resource_locks_holder_timeout")
            .table(ResourceLocks::Table)
            .col(ResourceLocks::HolderUserId)
            .col(ResourceLocks::TimeoutAt)
            .to_owned(),
        Index::create()
            .name("idx_resource_locks_namespace_lockroot")
            .table(ResourceLocks::Table)
            .col(ResourceLocks::NamespaceId)
            .col(ResourceLocks::LockrootPath)
            .to_owned(),
    ] {
        manager.create_index(index).await?;
    }
    Ok(())
}

async fn create_legacy_resource_lock_indexes(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    for index in [
        Index::create()
            .name("idx_resource_locks_entity")
            .table(ResourceLocks::Table)
            .col(LegacyResourceLocks::EntityType)
            .col(LegacyResourceLocks::EntityId)
            .to_owned(),
        Index::create()
            .name("idx_resource_locks_path")
            .table(ResourceLocks::Table)
            .col(LegacyResourceLocks::Path)
            .to_owned(),
    ] {
        manager.create_index(index).await?;
    }
    Ok(())
}

async fn reset_resource_lock_sequence(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    if manager.get_database_backend() == DbBackend::Postgres {
        manager
            .get_connection()
            .execute_unprepared(
                "SELECT setval(pg_get_serial_sequence('resource_locks', 'id'), \
                 COALESCE((SELECT MAX(id) FROM resource_locks), 0) + 1, false)",
            )
            .await?;
    }
    Ok(())
}

#[derive(DeriveIden)]
enum ResourceLockNamespaces {
    Table,
    Id,
    WorkspaceType,
    WorkspaceId,
    Generation,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum ResourceLocks {
    Table,
    Id,
    Token,
    NamespaceId,
    RootKind,
    RootFolderId,
    RootFileId,
    Depth,
    Mode,
    Origin,
    HolderUserId,
    OwnerInfo,
    TimeoutAt,
    LockrootPath,
    CreatedAt,
}

#[derive(DeriveIden)]
enum LegacyResourceLocks {
    Id,
    Token,
    EntityType,
    EntityId,
    Path,
    OwnerId,
    OwnerInfo,
    TimeoutAt,
    Shared,
    Deep,
    CreatedAt,
}

#[derive(DeriveIden)]
enum Files {
    Table,
    Id,
    IsLocked,
}

#[derive(DeriveIden)]
enum Folders {
    Table,
    Id,
    IsLocked,
}
