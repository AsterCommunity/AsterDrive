//! 数据库迁移：合并后的当前基线 schema。

use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::{ConnectionTrait, DbBackend};

use aster_forge_db_migration::{
    SqliteFtsConfig, ensure_postgres_extension, execute_sqlite_statements,
    mysql_fulltext_index_sql, postgres_drop_index, postgres_trigram_index,
    sqlite_fts_down_statements, sqlite_fts_up_statements,
};

const SHARE_TARGET_CHECK_NAME: &str = "chk_shares_exactly_one_target";
const SHARE_TOKEN_LENGTH_CHECK_NAME: &str = "chk_shares_token_length_32";

const FILES_NAME_FTS_TABLE: &str = "files_name_fts";
const FILES_NAME_FTS_INSERT_TRIGGER: &str = "trg_files_name_fts_ai";
const FILES_NAME_FTS_DELETE_TRIGGER: &str = "trg_files_name_fts_ad";
const FILES_NAME_FTS_UPDATE_TRIGGER: &str = "trg_files_name_fts_au";

const FOLDERS_NAME_FTS_TABLE: &str = "folders_name_fts";
const FOLDERS_NAME_FTS_INSERT_TRIGGER: &str = "trg_folders_name_fts_ai";
const FOLDERS_NAME_FTS_DELETE_TRIGGER: &str = "trg_folders_name_fts_ad";
const FOLDERS_NAME_FTS_UPDATE_TRIGGER: &str = "trg_folders_name_fts_au";

const USERS_SEARCH_FTS_TABLE: &str = "users_search_fts";
const USERS_SEARCH_FTS_INSERT_TRIGGER: &str = "trg_users_search_fts_ai";
const USERS_SEARCH_FTS_DELETE_TRIGGER: &str = "trg_users_search_fts_ad";
const USERS_SEARCH_FTS_UPDATE_TRIGGER: &str = "trg_users_search_fts_au";
const POSTGRES_USERS_USERNAME_TRGM_INDEX: &str = "idx_users_username_trgm";
const POSTGRES_USERS_EMAIL_TRGM_INDEX: &str = "idx_users_email_trgm";
const MYSQL_USERS_SEARCH_FULLTEXT_INDEX: &str = "idx_users_search_fulltext";

const TEAMS_SEARCH_FTS_TABLE: &str = "teams_search_fts";
const TEAMS_SEARCH_FTS_INSERT_TRIGGER: &str = "trg_teams_search_fts_ai";
const TEAMS_SEARCH_FTS_DELETE_TRIGGER: &str = "trg_teams_search_fts_ad";
const TEAMS_SEARCH_FTS_UPDATE_TRIGGER: &str = "trg_teams_search_fts_au";
const POSTGRES_TEAMS_NAME_TRGM_INDEX: &str = "idx_teams_name_trgm";
const POSTGRES_TEAMS_DESCRIPTION_TRGM_INDEX: &str = "idx_teams_description_trgm";
const MYSQL_TEAMS_SEARCH_FULLTEXT_INDEX: &str = "idx_teams_search_fulltext";

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        create_storage_policy_groups(manager).await?;
        create_managed_followers(manager).await?;
        create_users(manager).await?;
        create_storage_policies(manager).await?;
        create_storage_policy_group_items(manager).await?;
        create_teams(manager).await?;
        create_folders(manager).await?;
        create_file_blobs(manager).await?;
        create_files(manager).await?;
        create_system_config(manager).await?;
        create_upload_sessions(manager).await?;
        create_webdav_accounts(manager).await?;
        create_entity_properties(manager).await?;
        create_resource_locks(manager).await?;
        create_file_versions(manager).await?;
        create_audit_logs(manager).await?;
        create_upload_session_parts(manager).await?;
        create_team_members(manager).await?;
        create_shares(manager).await?;
        create_contact_verification_tokens(manager).await?;
        create_mail_outbox(manager).await?;
        create_background_tasks(manager).await?;
        create_wopi_sessions(manager).await?;
        create_user_profiles(manager).await?;
        create_auth_sessions(manager).await?;
        create_follower_enrollment_sessions(manager).await?;
        create_master_bindings(manager).await?;
        create_managed_ingress_profiles(manager).await?;
        create_secondary_indexes(manager).await?;
        create_search_acceleration(manager).await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_search_acceleration(manager).await?;

        for table in [
            AuthSessions::Table.into_iden(),
            ManagedIngressProfiles::Table.into_iden(),
            MasterBindings::Table.into_iden(),
            FollowerEnrollmentSessions::Table.into_iden(),
            WopiSessions::Table.into_iden(),
            BackgroundTasks::Table.into_iden(),
            MailOutbox::Table.into_iden(),
            ContactVerificationTokens::Table.into_iden(),
            TeamMembers::Table.into_iden(),
            Shares::Table.into_iden(),
            UploadSessionParts::Table.into_iden(),
            UploadSessions::Table.into_iden(),
            FileVersions::Table.into_iden(),
            Files::Table.into_iden(),
            FileBlobs::Table.into_iden(),
            WebdavAccounts::Table.into_iden(),
            Folders::Table.into_iden(),
            AuditLogs::Table.into_iden(),
            ResourceLocks::Table.into_iden(),
            EntityProperties::Table.into_iden(),
            Teams::Table.into_iden(),
            UserProfiles::Table.into_iden(),
            StoragePolicyGroupItems::Table.into_iden(),
            StoragePolicies::Table.into_iden(),
            Users::Table.into_iden(),
            StoragePolicyGroups::Table.into_iden(),
            ManagedFollowers::Table.into_iden(),
            SystemConfig::Table.into_iden(),
        ] {
            manager
                .drop_table(Table::drop().table(table).if_exists().to_owned())
                .await?;
        }

        Ok(())
    }
}

fn text_not_null_for_backend<T>(
    backend: DbBackend,
    column: T,
    default_for_non_mysql: Option<&'static str>,
) -> ColumnDef
where
    T: IntoIden,
{
    let mut column = ColumnDef::new(column);
    column.text().not_null();
    if backend != DbBackend::MySql
        && let Some(default_value) = default_for_non_mysql
    {
        column.default(default_value);
    }
    column
}

async fn create_storage_policy_groups(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            Table::create()
                .table(StoragePolicyGroups::Table)
                .if_not_exists()
                .col(aster_forge_db_migration::big_integer_primary_key(
                    StoragePolicyGroups::Id,
                ))
                .col(
                    ColumnDef::new(StoragePolicyGroups::Name)
                        .string_len(128)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(StoragePolicyGroups::Description)
                        .string_len(512)
                        .not_null()
                        .default(""),
                )
                .col(
                    ColumnDef::new(StoragePolicyGroups::IsEnabled)
                        .boolean()
                        .not_null()
                        .default(true),
                )
                .col(
                    ColumnDef::new(StoragePolicyGroups::IsDefault)
                        .boolean()
                        .not_null()
                        .default(false),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(
                        manager,
                        StoragePolicyGroups::CreatedAt,
                    )
                    .not_null(),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(
                        manager,
                        StoragePolicyGroups::UpdatedAt,
                    )
                    .not_null(),
                )
                .to_owned(),
        )
        .await
}

async fn create_managed_followers(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let backend = manager.get_database_backend();

    manager
        .create_table(
            Table::create()
                .table(ManagedFollowers::Table)
                .if_not_exists()
                .col(aster_forge_db_migration::big_integer_primary_key(
                    ManagedFollowers::Id,
                ))
                .col(
                    ColumnDef::new(ManagedFollowers::Name)
                        .string_len(128)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(ManagedFollowers::BaseUrl)
                        .string_len(512)
                        .not_null()
                        .default(""),
                )
                .col(
                    ColumnDef::new(ManagedFollowers::AccessKey)
                        .string_len(128)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(ManagedFollowers::SecretKey)
                        .string_len(255)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(ManagedFollowers::IsEnabled)
                        .boolean()
                        .not_null()
                        .default(true),
                )
                .col(text_not_null_for_backend(
                    backend,
                    ManagedFollowers::LastCapabilities,
                    Some("{}"),
                ))
                .col(text_not_null_for_backend(
                    backend,
                    ManagedFollowers::LastError,
                    Some(""),
                ))
                .col(
                    aster_forge_db_migration::utc_date_time_column(
                        manager,
                        ManagedFollowers::LastCheckedAt,
                    )
                    .null(),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(
                        manager,
                        ManagedFollowers::CreatedAt,
                    )
                    .not_null(),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(
                        manager,
                        ManagedFollowers::UpdatedAt,
                    )
                    .not_null(),
                )
                .to_owned(),
        )
        .await
}

async fn create_users(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let backend = manager.get_database_backend();
    let mut table = Table::create();
    table
        .table(Users::Table)
        .if_not_exists()
        .col(aster_forge_db_migration::big_integer_primary_key(Users::Id))
        .col(
            ColumnDef::new(Users::Username)
                .string_len(64)
                .not_null()
                .unique_key(),
        )
        .col(
            ColumnDef::new(Users::Email)
                .string_len(255)
                .not_null()
                .unique_key(),
        )
        .col(
            ColumnDef::new(Users::PasswordHash)
                .string_len(255)
                .not_null(),
        )
        .col(
            ColumnDef::new(Users::Role)
                .string_len(16)
                .not_null()
                .default("user"),
        )
        .col(
            ColumnDef::new(Users::Status)
                .string_len(16)
                .not_null()
                .default("active"),
        )
        .col(
            ColumnDef::new(Users::SessionVersion)
                .big_integer()
                .not_null()
                .default(1),
        )
        .col(aster_forge_db_migration::utc_date_time_column(manager, Users::EmailVerifiedAt).null())
        .col(ColumnDef::new(Users::PendingEmail).string_len(255).null())
        .col(
            ColumnDef::new(Users::StorageUsed)
                .big_integer()
                .not_null()
                .default(0),
        )
        .col(
            ColumnDef::new(Users::StorageQuota)
                .big_integer()
                .not_null()
                .default(0),
        )
        .col(ColumnDef::new(Users::PolicyGroupId).big_integer().null())
        .col(aster_forge_db_migration::utc_date_time_column(manager, Users::CreatedAt).not_null())
        .col(aster_forge_db_migration::utc_date_time_column(manager, Users::UpdatedAt).not_null())
        .col(ColumnDef::new(Users::Config).text().null());

    if backend != DbBackend::Sqlite {
        table.foreign_key(
            ForeignKey::create()
                .name("fk_users_policy_group_id")
                .from(Users::Table, Users::PolicyGroupId)
                .to(StoragePolicyGroups::Table, StoragePolicyGroups::Id)
                .on_delete(ForeignKeyAction::SetNull),
        );
    }

    manager.create_table(table.to_owned()).await
}

async fn create_storage_policies(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let backend = manager.get_database_backend();
    let mut table = Table::create();
    table
        .table(StoragePolicies::Table)
        .if_not_exists()
        .col(aster_forge_db_migration::big_integer_primary_key(
            StoragePolicies::Id,
        ))
        .col(
            ColumnDef::new(StoragePolicies::Name)
                .string_len(128)
                .not_null(),
        )
        .col(
            ColumnDef::new(StoragePolicies::DriverType)
                .string_len(32)
                .not_null(),
        )
        .col(
            ColumnDef::new(StoragePolicies::Endpoint)
                .string_len(512)
                .not_null()
                .default(""),
        )
        .col(
            ColumnDef::new(StoragePolicies::Bucket)
                .string_len(255)
                .not_null()
                .default(""),
        )
        .col(
            ColumnDef::new(StoragePolicies::AccessKey)
                .string_len(512)
                .not_null()
                .default(""),
        )
        .col(
            ColumnDef::new(StoragePolicies::SecretKey)
                .string_len(512)
                .not_null()
                .default(""),
        )
        .col(
            ColumnDef::new(StoragePolicies::BasePath)
                .string_len(512)
                .not_null()
                .default(""),
        )
        .col(
            ColumnDef::new(StoragePolicies::RemoteNodeId)
                .big_integer()
                .null(),
        )
        .col(
            ColumnDef::new(StoragePolicies::MaxFileSize)
                .big_integer()
                .not_null()
                .default(0),
        )
        .col(text_not_null_for_backend(
            backend,
            StoragePolicies::AllowedTypes,
            Some("[]"),
        ))
        .col(text_not_null_for_backend(
            backend,
            StoragePolicies::Options,
            Some("{}"),
        ))
        .col(
            ColumnDef::new(StoragePolicies::IsDefault)
                .boolean()
                .not_null()
                .default(false),
        )
        .col(
            ColumnDef::new(StoragePolicies::ChunkSize)
                .big_integer()
                .not_null()
                .default(5_242_880i64),
        )
        .col(
            aster_forge_db_migration::utc_date_time_column(manager, StoragePolicies::CreatedAt)
                .not_null(),
        )
        .col(
            aster_forge_db_migration::utc_date_time_column(manager, StoragePolicies::UpdatedAt)
                .not_null(),
        );

    if backend != DbBackend::Sqlite {
        table.foreign_key(
            ForeignKey::create()
                .name("fk_storage_policies_remote_node_id")
                .from(StoragePolicies::Table, StoragePolicies::RemoteNodeId)
                .to(ManagedFollowers::Table, ManagedFollowers::Id)
                .on_delete(ForeignKeyAction::SetNull),
        );
    }

    manager.create_table(table.to_owned()).await
}

async fn create_storage_policy_group_items(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            Table::create()
                .table(StoragePolicyGroupItems::Table)
                .if_not_exists()
                .col(aster_forge_db_migration::big_integer_primary_key(
                    StoragePolicyGroupItems::Id,
                ))
                .col(
                    ColumnDef::new(StoragePolicyGroupItems::GroupId)
                        .big_integer()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(StoragePolicyGroupItems::PolicyId)
                        .big_integer()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(StoragePolicyGroupItems::Priority)
                        .integer()
                        .not_null()
                        .default(1),
                )
                .col(
                    ColumnDef::new(StoragePolicyGroupItems::MinFileSize)
                        .big_integer()
                        .not_null()
                        .default(0),
                )
                .col(
                    ColumnDef::new(StoragePolicyGroupItems::MaxFileSize)
                        .big_integer()
                        .not_null()
                        .default(0),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(
                        manager,
                        StoragePolicyGroupItems::CreatedAt,
                    )
                    .not_null(),
                )
                .foreign_key(
                    ForeignKey::create()
                        .from(
                            StoragePolicyGroupItems::Table,
                            StoragePolicyGroupItems::GroupId,
                        )
                        .to(StoragePolicyGroups::Table, StoragePolicyGroups::Id)
                        .on_delete(ForeignKeyAction::Cascade),
                )
                .foreign_key(
                    ForeignKey::create()
                        .from(
                            StoragePolicyGroupItems::Table,
                            StoragePolicyGroupItems::PolicyId,
                        )
                        .to(StoragePolicies::Table, StoragePolicies::Id)
                        .on_delete(ForeignKeyAction::Cascade),
                )
                .to_owned(),
        )
        .await
}

async fn create_teams(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            Table::create()
                .table(Teams::Table)
                .if_not_exists()
                .col(aster_forge_db_migration::big_integer_primary_key(Teams::Id))
                .col(ColumnDef::new(Teams::Name).string_len(128).not_null())
                .col(
                    ColumnDef::new(Teams::Description)
                        .string_len(512)
                        .not_null()
                        .default(""),
                )
                .col(ColumnDef::new(Teams::CreatedBy).big_integer().not_null())
                .col(
                    ColumnDef::new(Teams::StorageUsed)
                        .big_integer()
                        .not_null()
                        .default(0),
                )
                .col(
                    ColumnDef::new(Teams::StorageQuota)
                        .big_integer()
                        .not_null()
                        .default(0),
                )
                .col(ColumnDef::new(Teams::PolicyGroupId).big_integer().null())
                .col(
                    aster_forge_db_migration::utc_date_time_column(manager, Teams::CreatedAt)
                        .not_null(),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(manager, Teams::UpdatedAt)
                        .not_null(),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(manager, Teams::ArchivedAt)
                        .null(),
                )
                .foreign_key(
                    ForeignKey::create()
                        .from(Teams::Table, Teams::CreatedBy)
                        .to(Users::Table, Users::Id)
                        .on_delete(ForeignKeyAction::Restrict),
                )
                .foreign_key(
                    ForeignKey::create()
                        .from(Teams::Table, Teams::PolicyGroupId)
                        .to(StoragePolicyGroups::Table, StoragePolicyGroups::Id)
                        .on_delete(ForeignKeyAction::SetNull),
                )
                .to_owned(),
        )
        .await
}

async fn create_folders(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let backend = manager.get_database_backend();
    let mut table = Table::create();
    table
        .table(Folders::Table)
        .if_not_exists()
        .col(aster_forge_db_migration::big_integer_primary_key(
            Folders::Id,
        ))
        .col(ColumnDef::new(Folders::Name).string_len(255).not_null())
        .col(ColumnDef::new(Folders::ParentId).big_integer().null())
        .col(ColumnDef::new(Folders::TeamId).big_integer().null())
        .col(ColumnDef::new(Folders::OwnerUserId).big_integer().null())
        .col(
            ColumnDef::new(Folders::CreatedByUserId)
                .big_integer()
                .null(),
        )
        .col(
            ColumnDef::new(Folders::CreatedByUsername)
                .string_len(255)
                .not_null()
                .default(""),
        )
        .col(ColumnDef::new(Folders::PolicyId).big_integer().null())
        .col(aster_forge_db_migration::utc_date_time_column(manager, Folders::CreatedAt).not_null())
        .col(aster_forge_db_migration::utc_date_time_column(manager, Folders::UpdatedAt).not_null())
        .col(aster_forge_db_migration::utc_date_time_column(manager, Folders::DeletedAt).null())
        .col(
            ColumnDef::new(Folders::IsLocked)
                .boolean()
                .not_null()
                .default(false),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_folders_owner_user_id")
                .from(Folders::Table, Folders::OwnerUserId)
                .to(Users::Table, Users::Id)
                .on_delete(ForeignKeyAction::SetNull),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_folders_created_by_user_id")
                .from(Folders::Table, Folders::CreatedByUserId)
                .to(Users::Table, Users::Id)
                .on_delete(ForeignKeyAction::SetNull),
        )
        .foreign_key(
            ForeignKey::create()
                .from(Folders::Table, Folders::PolicyId)
                .to(StoragePolicies::Table, StoragePolicies::Id)
                .on_delete(ForeignKeyAction::SetNull),
        )
        .foreign_key(
            ForeignKey::create()
                .from(Folders::Table, Folders::ParentId)
                .to(Folders::Table, Folders::Id)
                .on_delete(ForeignKeyAction::SetNull),
        );

    if backend != DbBackend::Sqlite {
        table.foreign_key(
            ForeignKey::create()
                .name("fk_folders_team_id")
                .from(Folders::Table, Folders::TeamId)
                .to(Teams::Table, Teams::Id)
                .on_delete(ForeignKeyAction::SetNull),
        );
    }

    manager.create_table(table.to_owned()).await
}

async fn create_file_blobs(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            Table::create()
                .table(FileBlobs::Table)
                .if_not_exists()
                .col(aster_forge_db_migration::big_integer_primary_key(
                    FileBlobs::Id,
                ))
                .col(ColumnDef::new(FileBlobs::Hash).string_len(64).not_null())
                .col(ColumnDef::new(FileBlobs::Size).big_integer().not_null())
                .col(ColumnDef::new(FileBlobs::PolicyId).big_integer().not_null())
                .col(
                    ColumnDef::new(FileBlobs::StoragePath)
                        .string_len(1024)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(FileBlobs::ThumbnailPath)
                        .string_len(1024)
                        .null(),
                )
                .col(
                    ColumnDef::new(FileBlobs::ThumbnailProcessor)
                        .string_len(32)
                        .null(),
                )
                .col(
                    ColumnDef::new(FileBlobs::ThumbnailVersion)
                        .string_len(32)
                        .null(),
                )
                .col(
                    ColumnDef::new(FileBlobs::RefCount)
                        .integer()
                        .not_null()
                        .default(1),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(manager, FileBlobs::CreatedAt)
                        .not_null(),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(manager, FileBlobs::UpdatedAt)
                        .not_null(),
                )
                .foreign_key(
                    ForeignKey::create()
                        .from(FileBlobs::Table, FileBlobs::PolicyId)
                        .to(StoragePolicies::Table, StoragePolicies::Id),
                )
                .to_owned(),
        )
        .await
}

async fn create_files(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let backend = manager.get_database_backend();
    let mut table = Table::create();
    table
        .table(Files::Table)
        .if_not_exists()
        .col(aster_forge_db_migration::big_integer_primary_key(Files::Id))
        .col(ColumnDef::new(Files::Name).string_len(255).not_null())
        .col(ColumnDef::new(Files::FolderId).big_integer().null())
        .col(ColumnDef::new(Files::TeamId).big_integer().null())
        .col(ColumnDef::new(Files::BlobId).big_integer().not_null())
        .col(
            ColumnDef::new(Files::Size)
                .big_integer()
                .not_null()
                .default(0),
        )
        .col(ColumnDef::new(Files::OwnerUserId).big_integer().null())
        .col(ColumnDef::new(Files::CreatedByUserId).big_integer().null())
        .col(
            ColumnDef::new(Files::CreatedByUsername)
                .string_len(255)
                .not_null()
                .default(""),
        )
        .col(ColumnDef::new(Files::MimeType).string_len(128).not_null())
        .col(aster_forge_db_migration::utc_date_time_column(manager, Files::CreatedAt).not_null())
        .col(aster_forge_db_migration::utc_date_time_column(manager, Files::UpdatedAt).not_null())
        .col(aster_forge_db_migration::utc_date_time_column(manager, Files::DeletedAt).null())
        .col(
            ColumnDef::new(Files::IsLocked)
                .boolean()
                .not_null()
                .default(false),
        )
        .foreign_key(
            ForeignKey::create()
                .from(Files::Table, Files::FolderId)
                .to(Folders::Table, Folders::Id)
                .on_delete(ForeignKeyAction::SetNull),
        )
        .foreign_key(
            ForeignKey::create()
                .from(Files::Table, Files::BlobId)
                .to(FileBlobs::Table, FileBlobs::Id)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_files_owner_user_id")
                .from(Files::Table, Files::OwnerUserId)
                .to(Users::Table, Users::Id)
                .on_delete(ForeignKeyAction::SetNull),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_files_created_by_user_id")
                .from(Files::Table, Files::CreatedByUserId)
                .to(Users::Table, Users::Id)
                .on_delete(ForeignKeyAction::SetNull),
        );

    if backend != DbBackend::Sqlite {
        table.foreign_key(
            ForeignKey::create()
                .name("fk_files_team_id")
                .from(Files::Table, Files::TeamId)
                .to(Teams::Table, Teams::Id)
                .on_delete(ForeignKeyAction::SetNull),
        );
    }

    manager.create_table(table.to_owned()).await
}

async fn create_system_config(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let backend = manager.get_database_backend();

    manager
        .create_table(
            Table::create()
                .table(SystemConfig::Table)
                .if_not_exists()
                .col(aster_forge_db_migration::big_integer_primary_key(
                    SystemConfig::Id,
                ))
                .col(
                    ColumnDef::new(SystemConfig::Key)
                        .string_len(128)
                        .not_null()
                        .unique_key(),
                )
                .col(ColumnDef::new(SystemConfig::Value).text().not_null())
                .col(
                    ColumnDef::new(SystemConfig::ValueType)
                        .string_len(32)
                        .not_null()
                        .default("string"),
                )
                .col(
                    ColumnDef::new(SystemConfig::RequiresRestart)
                        .boolean()
                        .not_null()
                        .default(false),
                )
                .col(
                    ColumnDef::new(SystemConfig::IsSensitive)
                        .boolean()
                        .not_null()
                        .default(false),
                )
                .col(
                    ColumnDef::new(SystemConfig::Source)
                        .string_len(16)
                        .not_null()
                        .default("system"),
                )
                .col(
                    ColumnDef::new(SystemConfig::Namespace)
                        .string_len(128)
                        .not_null()
                        .default(""),
                )
                .col(
                    ColumnDef::new(SystemConfig::Category)
                        .string_len(64)
                        .not_null()
                        .default(""),
                )
                .col(text_not_null_for_backend(
                    backend,
                    SystemConfig::Description,
                    Some(""),
                ))
                .col(
                    aster_forge_db_migration::utc_date_time_column(
                        manager,
                        SystemConfig::UpdatedAt,
                    )
                    .not_null(),
                )
                .col(ColumnDef::new(SystemConfig::UpdatedBy).big_integer().null())
                .to_owned(),
        )
        .await
}

async fn create_upload_sessions(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let backend = manager.get_database_backend();
    let mut table = Table::create();
    table
        .table(UploadSessions::Table)
        .if_not_exists()
        .col(
            ColumnDef::new(UploadSessions::Id)
                .string_len(36)
                .not_null()
                .primary_key(),
        )
        .col(
            ColumnDef::new(UploadSessions::UserId)
                .big_integer()
                .not_null(),
        )
        .col(ColumnDef::new(UploadSessions::TeamId).big_integer().null())
        .col(
            ColumnDef::new(UploadSessions::Filename)
                .string_len(255)
                .not_null(),
        )
        .col(
            ColumnDef::new(UploadSessions::TotalSize)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(UploadSessions::ChunkSize)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(UploadSessions::TotalChunks)
                .integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(UploadSessions::ReceivedCount)
                .integer()
                .not_null()
                .default(0),
        )
        .col(
            ColumnDef::new(UploadSessions::FolderId)
                .big_integer()
                .null(),
        )
        .col(
            ColumnDef::new(UploadSessions::PolicyId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(UploadSessions::Status)
                .string_len(16)
                .not_null()
                .default("uploading"),
        )
        .col(ColumnDef::new(UploadSessions::S3TempKey).text().null())
        .col(ColumnDef::new(UploadSessions::S3MultipartId).text().null())
        .col(ColumnDef::new(UploadSessions::FileId).big_integer().null())
        .col(
            aster_forge_db_migration::utc_date_time_column(manager, UploadSessions::CreatedAt)
                .not_null(),
        )
        .col(
            aster_forge_db_migration::utc_date_time_column(manager, UploadSessions::ExpiresAt)
                .not_null(),
        )
        .col(
            aster_forge_db_migration::utc_date_time_column(manager, UploadSessions::UpdatedAt)
                .not_null(),
        )
        .foreign_key(
            ForeignKey::create()
                .from(UploadSessions::Table, UploadSessions::UserId)
                .to(Users::Table, Users::Id)
                .on_delete(ForeignKeyAction::Cascade),
        );

    if backend != DbBackend::Sqlite {
        table.foreign_key(
            ForeignKey::create()
                .name("fk_upload_sessions_team_id")
                .from(UploadSessions::Table, UploadSessions::TeamId)
                .to(Teams::Table, Teams::Id)
                .on_delete(ForeignKeyAction::SetNull),
        );
    }

    manager.create_table(table.to_owned()).await
}

async fn create_webdav_accounts(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            Table::create()
                .table(WebdavAccounts::Table)
                .if_not_exists()
                .col(aster_forge_db_migration::big_integer_primary_key(
                    WebdavAccounts::Id,
                ))
                .col(
                    ColumnDef::new(WebdavAccounts::UserId)
                        .big_integer()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(WebdavAccounts::Username)
                        .string_len(64)
                        .not_null()
                        .unique_key(),
                )
                .col(
                    ColumnDef::new(WebdavAccounts::PasswordHash)
                        .string_len(255)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(WebdavAccounts::RootFolderId)
                        .big_integer()
                        .null(),
                )
                .col(
                    ColumnDef::new(WebdavAccounts::IsActive)
                        .boolean()
                        .not_null()
                        .default(true),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(
                        manager,
                        WebdavAccounts::CreatedAt,
                    )
                    .not_null(),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(
                        manager,
                        WebdavAccounts::UpdatedAt,
                    )
                    .not_null(),
                )
                .foreign_key(
                    ForeignKey::create()
                        .from(WebdavAccounts::Table, WebdavAccounts::UserId)
                        .to(Users::Table, Users::Id)
                        .on_delete(ForeignKeyAction::Cascade),
                )
                .to_owned(),
        )
        .await
}

async fn create_entity_properties(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            Table::create()
                .table(EntityProperties::Table)
                .if_not_exists()
                .col(aster_forge_db_migration::big_integer_primary_key(
                    EntityProperties::Id,
                ))
                .col(
                    ColumnDef::new(EntityProperties::EntityType)
                        .string_len(16)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(EntityProperties::EntityId)
                        .big_integer()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(EntityProperties::Namespace)
                        .string_len(256)
                        .not_null()
                        .default(""),
                )
                .col(
                    ColumnDef::new(EntityProperties::Name)
                        .string_len(255)
                        .not_null(),
                )
                .col(ColumnDef::new(EntityProperties::Value).text().null())
                .to_owned(),
        )
        .await
}

async fn create_resource_locks(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            Table::create()
                .table(ResourceLocks::Table)
                .if_not_exists()
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
                    ColumnDef::new(ResourceLocks::EntityType)
                        .string_len(16)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(ResourceLocks::EntityId)
                        .big_integer()
                        .not_null(),
                )
                .col(ColumnDef::new(ResourceLocks::Path).string().not_null())
                .col(ColumnDef::new(ResourceLocks::OwnerId).big_integer().null())
                .col(ColumnDef::new(ResourceLocks::OwnerInfo).text().null())
                .col(
                    aster_forge_db_migration::utc_date_time_column(
                        manager,
                        ResourceLocks::TimeoutAt,
                    )
                    .null(),
                )
                .col(
                    ColumnDef::new(ResourceLocks::Shared)
                        .boolean()
                        .not_null()
                        .default(false),
                )
                .col(
                    ColumnDef::new(ResourceLocks::Deep)
                        .boolean()
                        .not_null()
                        .default(false),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(
                        manager,
                        ResourceLocks::CreatedAt,
                    )
                    .not_null(),
                )
                .to_owned(),
        )
        .await
}

async fn create_file_versions(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            Table::create()
                .table(FileVersions::Table)
                .if_not_exists()
                .col(aster_forge_db_migration::big_integer_primary_key(
                    FileVersions::Id,
                ))
                .col(
                    ColumnDef::new(FileVersions::FileId)
                        .big_integer()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(FileVersions::BlobId)
                        .big_integer()
                        .not_null(),
                )
                .col(ColumnDef::new(FileVersions::Version).integer().not_null())
                .col(ColumnDef::new(FileVersions::Size).big_integer().not_null())
                .col(
                    aster_forge_db_migration::utc_date_time_column(
                        manager,
                        FileVersions::CreatedAt,
                    )
                    .not_null(),
                )
                .foreign_key(
                    ForeignKey::create()
                        .from(FileVersions::Table, FileVersions::BlobId)
                        .to(FileBlobs::Table, FileBlobs::Id)
                        .on_delete(ForeignKeyAction::Restrict),
                )
                .to_owned(),
        )
        .await
}

async fn create_audit_logs(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            Table::create()
                .table(AuditLogs::Table)
                .if_not_exists()
                .col(aster_forge_db_migration::big_integer_primary_key(
                    AuditLogs::Id,
                ))
                .col(ColumnDef::new(AuditLogs::UserId).big_integer().not_null())
                .col(ColumnDef::new(AuditLogs::Action).string_len(64).not_null())
                .col(ColumnDef::new(AuditLogs::EntityType).string_len(16).null())
                .col(ColumnDef::new(AuditLogs::EntityId).big_integer().null())
                .col(ColumnDef::new(AuditLogs::EntityName).string_len(255).null())
                .col(ColumnDef::new(AuditLogs::Details).text().null())
                .col(ColumnDef::new(AuditLogs::IpAddress).string_len(45).null())
                .col(ColumnDef::new(AuditLogs::UserAgent).string_len(512).null())
                .col(
                    aster_forge_db_migration::utc_date_time_column(manager, AuditLogs::CreatedAt)
                        .not_null(),
                )
                .to_owned(),
        )
        .await
}

async fn create_upload_session_parts(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            Table::create()
                .table(UploadSessionParts::Table)
                .if_not_exists()
                .col(aster_forge_db_migration::big_integer_primary_key(
                    UploadSessionParts::Id,
                ))
                .col(
                    ColumnDef::new(UploadSessionParts::UploadId)
                        .string_len(36)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(UploadSessionParts::PartNumber)
                        .integer()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(UploadSessionParts::Etag)
                        .string_len(512)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(UploadSessionParts::Size)
                        .big_integer()
                        .not_null()
                        .default(0),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(
                        manager,
                        UploadSessionParts::CreatedAt,
                    )
                    .not_null(),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(
                        manager,
                        UploadSessionParts::UpdatedAt,
                    )
                    .not_null(),
                )
                .foreign_key(
                    ForeignKey::create()
                        .from(UploadSessionParts::Table, UploadSessionParts::UploadId)
                        .to(UploadSessions::Table, UploadSessions::Id)
                        .on_delete(ForeignKeyAction::Cascade),
                )
                .to_owned(),
        )
        .await
}

async fn create_team_members(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            Table::create()
                .table(TeamMembers::Table)
                .if_not_exists()
                .col(aster_forge_db_migration::big_integer_primary_key(
                    TeamMembers::Id,
                ))
                .col(ColumnDef::new(TeamMembers::TeamId).big_integer().not_null())
                .col(ColumnDef::new(TeamMembers::UserId).big_integer().not_null())
                .col(
                    ColumnDef::new(TeamMembers::Role)
                        .string_len(16)
                        .not_null()
                        .default("member"),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(manager, TeamMembers::CreatedAt)
                        .not_null(),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(manager, TeamMembers::UpdatedAt)
                        .not_null(),
                )
                .check(Expr::col(TeamMembers::Role).is_in(["owner", "admin", "member"]))
                .foreign_key(
                    ForeignKey::create()
                        .from(TeamMembers::Table, TeamMembers::TeamId)
                        .to(Teams::Table, Teams::Id)
                        .on_delete(ForeignKeyAction::Cascade),
                )
                .foreign_key(
                    ForeignKey::create()
                        .from(TeamMembers::Table, TeamMembers::UserId)
                        .to(Users::Table, Users::Id)
                        .on_delete(ForeignKeyAction::Cascade),
                )
                .to_owned(),
        )
        .await
}

async fn create_shares(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let backend = manager.get_database_backend();
    let mut table = Table::create();
    table
        .table(Shares::Table)
        .if_not_exists()
        .col(aster_forge_db_migration::big_integer_primary_key(
            Shares::Id,
        ))
        .col(
            ColumnDef::new(Shares::Token)
                .string_len(32)
                .not_null()
                .unique_key(),
        )
        .col(ColumnDef::new(Shares::UserId).big_integer().not_null())
        .col(ColumnDef::new(Shares::TeamId).big_integer().null())
        .col(ColumnDef::new(Shares::FileId).big_integer().null())
        .col(ColumnDef::new(Shares::FolderId).big_integer().null())
        .col(ColumnDef::new(Shares::Password).string_len(255).null())
        .col(aster_forge_db_migration::utc_date_time_column(manager, Shares::ExpiresAt).null())
        .col(
            ColumnDef::new(Shares::MaxDownloads)
                .big_integer()
                .not_null()
                .default(0),
        )
        .col(
            ColumnDef::new(Shares::DownloadCount)
                .big_integer()
                .not_null()
                .default(0),
        )
        .col(
            ColumnDef::new(Shares::ViewCount)
                .big_integer()
                .not_null()
                .default(0),
        )
        .col(aster_forge_db_migration::utc_date_time_column(manager, Shares::CreatedAt).not_null())
        .col(aster_forge_db_migration::utc_date_time_column(manager, Shares::UpdatedAt).not_null())
        .check((
            Alias::new(SHARE_TARGET_CHECK_NAME),
            Expr::cust("(file_id IS NULL) <> (folder_id IS NULL)"),
        ))
        .check((
            Alias::new(SHARE_TOKEN_LENGTH_CHECK_NAME),
            Expr::cust("length(token) <= 32"),
        ))
        .foreign_key(
            ForeignKey::create()
                .from(Shares::Table, Shares::UserId)
                .to(Users::Table, Users::Id)
                .on_delete(ForeignKeyAction::Cascade),
        );

    if backend != DbBackend::Sqlite {
        table.foreign_key(
            ForeignKey::create()
                .name("fk_shares_team_id")
                .from(Shares::Table, Shares::TeamId)
                .to(Teams::Table, Teams::Id)
                .on_delete(ForeignKeyAction::Cascade),
        );
    }

    manager.create_table(table.to_owned()).await
}

async fn create_contact_verification_tokens(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            Table::create()
                .table(ContactVerificationTokens::Table)
                .if_not_exists()
                .col(aster_forge_db_migration::big_integer_primary_key(
                    ContactVerificationTokens::Id,
                ))
                .col(
                    ColumnDef::new(ContactVerificationTokens::UserId)
                        .big_integer()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(ContactVerificationTokens::Channel)
                        .string_len(16)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(ContactVerificationTokens::Purpose)
                        .string_len(32)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(ContactVerificationTokens::Target)
                        .string_len(255)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(ContactVerificationTokens::TokenHash)
                        .string_len(64)
                        .not_null()
                        .unique_key(),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(
                        manager,
                        ContactVerificationTokens::ExpiresAt,
                    )
                    .not_null(),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(
                        manager,
                        ContactVerificationTokens::ConsumedAt,
                    )
                    .null(),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(
                        manager,
                        ContactVerificationTokens::CreatedAt,
                    )
                    .not_null(),
                )
                .foreign_key(
                    ForeignKey::create()
                        .from(
                            ContactVerificationTokens::Table,
                            ContactVerificationTokens::UserId,
                        )
                        .to(Users::Table, Users::Id)
                        .on_delete(ForeignKeyAction::Cascade),
                )
                .to_owned(),
        )
        .await
}

async fn create_mail_outbox(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            Table::create()
                .table(MailOutbox::Table)
                .if_not_exists()
                .col(aster_forge_db_migration::big_integer_primary_key(
                    MailOutbox::Id,
                ))
                .col(
                    ColumnDef::new(MailOutbox::TemplateCode)
                        .string_len(32)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(MailOutbox::ToAddress)
                        .string_len(255)
                        .not_null(),
                )
                .col(ColumnDef::new(MailOutbox::ToName).string_len(255).null())
                .col(ColumnDef::new(MailOutbox::PayloadJson).text().not_null())
                .col(ColumnDef::new(MailOutbox::Status).string_len(16).not_null())
                .col(
                    ColumnDef::new(MailOutbox::AttemptCount)
                        .integer()
                        .not_null()
                        .default(0),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(
                        manager,
                        MailOutbox::NextAttemptAt,
                    )
                    .not_null(),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(
                        manager,
                        MailOutbox::ProcessingStartedAt,
                    )
                    .null(),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(manager, MailOutbox::SentAt)
                        .null(),
                )
                .col(ColumnDef::new(MailOutbox::LastError).text().null())
                .col(
                    aster_forge_db_migration::utc_date_time_column(manager, MailOutbox::CreatedAt)
                        .not_null(),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(manager, MailOutbox::UpdatedAt)
                        .not_null(),
                )
                .to_owned(),
        )
        .await
}

async fn create_background_tasks(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            Table::create()
                .table(BackgroundTasks::Table)
                .if_not_exists()
                .col(aster_forge_db_migration::big_integer_primary_key(
                    BackgroundTasks::Id,
                ))
                .col(
                    ColumnDef::new(BackgroundTasks::Kind)
                        .string_len(32)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(BackgroundTasks::Status)
                        .string_len(16)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(BackgroundTasks::CreatorUserId)
                        .big_integer()
                        .null(),
                )
                .col(ColumnDef::new(BackgroundTasks::TeamId).big_integer().null())
                .col(
                    ColumnDef::new(BackgroundTasks::ShareId)
                        .big_integer()
                        .null(),
                )
                .col(
                    ColumnDef::new(BackgroundTasks::DisplayName)
                        .string_len(255)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(BackgroundTasks::PayloadJson)
                        .text()
                        .not_null(),
                )
                .col(ColumnDef::new(BackgroundTasks::ResultJson).text().null())
                .col(ColumnDef::new(BackgroundTasks::StepsJson).text().null())
                .col(
                    ColumnDef::new(BackgroundTasks::ProgressCurrent)
                        .big_integer()
                        .not_null()
                        .default(0),
                )
                .col(
                    ColumnDef::new(BackgroundTasks::ProgressTotal)
                        .big_integer()
                        .not_null()
                        .default(0),
                )
                .col(
                    ColumnDef::new(BackgroundTasks::StatusText)
                        .string_len(255)
                        .null(),
                )
                .col(
                    ColumnDef::new(BackgroundTasks::AttemptCount)
                        .integer()
                        .not_null()
                        .default(0),
                )
                .col(
                    ColumnDef::new(BackgroundTasks::MaxAttempts)
                        .integer()
                        .not_null()
                        .default(3),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(
                        manager,
                        BackgroundTasks::NextRunAt,
                    )
                    .not_null(),
                )
                .col(
                    ColumnDef::new(BackgroundTasks::ProcessingToken)
                        .big_integer()
                        .not_null()
                        .default(0),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(
                        manager,
                        BackgroundTasks::ProcessingStartedAt,
                    )
                    .null(),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(
                        manager,
                        BackgroundTasks::LastHeartbeatAt,
                    )
                    .null(),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(
                        manager,
                        BackgroundTasks::LeaseExpiresAt,
                    )
                    .null(),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(
                        manager,
                        BackgroundTasks::StartedAt,
                    )
                    .null(),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(
                        manager,
                        BackgroundTasks::FinishedAt,
                    )
                    .null(),
                )
                .col(ColumnDef::new(BackgroundTasks::LastError).text().null())
                .col(
                    ColumnDef::new(BackgroundTasks::FailureCanRetry)
                        .boolean()
                        .null(),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(
                        manager,
                        BackgroundTasks::ExpiresAt,
                    )
                    .not_null(),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(
                        manager,
                        BackgroundTasks::CreatedAt,
                    )
                    .not_null(),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(
                        manager,
                        BackgroundTasks::UpdatedAt,
                    )
                    .not_null(),
                )
                .to_owned(),
        )
        .await
}

async fn create_wopi_sessions(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            Table::create()
                .table(WopiSessions::Table)
                .if_not_exists()
                .col(aster_forge_db_migration::big_integer_primary_key(
                    WopiSessions::Id,
                ))
                .col(
                    ColumnDef::new(WopiSessions::TokenHash)
                        .string_len(64)
                        .not_null()
                        .unique_key(),
                )
                .col(
                    ColumnDef::new(WopiSessions::ActorUserId)
                        .big_integer()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(WopiSessions::SessionVersion)
                        .big_integer()
                        .not_null(),
                )
                .col(ColumnDef::new(WopiSessions::TeamId).big_integer().null())
                .col(
                    ColumnDef::new(WopiSessions::FileId)
                        .big_integer()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(WopiSessions::AppKey)
                        .string_len(255)
                        .not_null(),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(
                        manager,
                        WopiSessions::ExpiresAt,
                    )
                    .not_null(),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(
                        manager,
                        WopiSessions::CreatedAt,
                    )
                    .not_null(),
                )
                .foreign_key(
                    ForeignKey::create()
                        .from(WopiSessions::Table, WopiSessions::ActorUserId)
                        .to(Users::Table, Users::Id)
                        .on_delete(ForeignKeyAction::Cascade),
                )
                .foreign_key(
                    ForeignKey::create()
                        .from(WopiSessions::Table, WopiSessions::FileId)
                        .to(Files::Table, Files::Id)
                        .on_delete(ForeignKeyAction::Cascade),
                )
                .to_owned(),
        )
        .await
}

async fn create_user_profiles(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            Table::create()
                .table(UserProfiles::Table)
                .if_not_exists()
                .col(
                    ColumnDef::new(UserProfiles::UserId)
                        .big_integer()
                        .not_null()
                        .primary_key(),
                )
                .col(
                    ColumnDef::new(UserProfiles::DisplayName)
                        .string_len(64)
                        .null(),
                )
                .col(
                    ColumnDef::new(UserProfiles::WopiUserInfo)
                        .string_len(1024)
                        .null(),
                )
                .col(
                    ColumnDef::new(UserProfiles::AvatarSource)
                        .string_len(16)
                        .not_null()
                        .default("none"),
                )
                .col(
                    ColumnDef::new(UserProfiles::AvatarKey)
                        .string_len(512)
                        .null(),
                )
                .col(
                    ColumnDef::new(UserProfiles::AvatarVersion)
                        .integer()
                        .not_null()
                        .default(0),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(
                        manager,
                        UserProfiles::CreatedAt,
                    )
                    .not_null(),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(
                        manager,
                        UserProfiles::UpdatedAt,
                    )
                    .not_null(),
                )
                .foreign_key(
                    ForeignKey::create()
                        .from(UserProfiles::Table, UserProfiles::UserId)
                        .to(Users::Table, Users::Id)
                        .on_delete(ForeignKeyAction::Cascade),
                )
                .to_owned(),
        )
        .await
}

async fn create_auth_sessions(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            Table::create()
                .table(AuthSessions::Table)
                .if_not_exists()
                .col(
                    ColumnDef::new(AuthSessions::Id)
                        .string_len(36)
                        .not_null()
                        .primary_key(),
                )
                .col(
                    ColumnDef::new(AuthSessions::UserId)
                        .big_integer()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(AuthSessions::CurrentRefreshJti)
                        .string_len(36)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(AuthSessions::PreviousRefreshJti)
                        .string_len(36)
                        .null(),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(
                        manager,
                        AuthSessions::RefreshExpiresAt,
                    )
                    .not_null(),
                )
                .col(ColumnDef::new(AuthSessions::IpAddress).text().null())
                .col(ColumnDef::new(AuthSessions::UserAgent).text().null())
                .col(
                    aster_forge_db_migration::utc_date_time_column(
                        manager,
                        AuthSessions::CreatedAt,
                    )
                    .not_null(),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(
                        manager,
                        AuthSessions::LastSeenAt,
                    )
                    .not_null(),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(
                        manager,
                        AuthSessions::RevokedAt,
                    )
                    .null(),
                )
                .foreign_key(
                    ForeignKey::create()
                        .from(AuthSessions::Table, AuthSessions::UserId)
                        .to(Users::Table, Users::Id)
                        .on_delete(ForeignKeyAction::Cascade),
                )
                .to_owned(),
        )
        .await
}

async fn create_follower_enrollment_sessions(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            Table::create()
                .table(FollowerEnrollmentSessions::Table)
                .if_not_exists()
                .col(aster_forge_db_migration::big_integer_primary_key(
                    FollowerEnrollmentSessions::Id,
                ))
                .col(
                    ColumnDef::new(FollowerEnrollmentSessions::ManagedFollowerId)
                        .big_integer()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(FollowerEnrollmentSessions::TokenHash)
                        .string_len(128)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(FollowerEnrollmentSessions::AckTokenHash)
                        .string_len(128)
                        .not_null(),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(
                        manager,
                        FollowerEnrollmentSessions::ExpiresAt,
                    )
                    .not_null(),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(
                        manager,
                        FollowerEnrollmentSessions::RedeemedAt,
                    )
                    .null(),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(
                        manager,
                        FollowerEnrollmentSessions::AckedAt,
                    )
                    .null(),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(
                        manager,
                        FollowerEnrollmentSessions::InvalidatedAt,
                    )
                    .null(),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(
                        manager,
                        FollowerEnrollmentSessions::CreatedAt,
                    )
                    .not_null(),
                )
                .foreign_key(
                    ForeignKey::create()
                        .from(
                            FollowerEnrollmentSessions::Table,
                            FollowerEnrollmentSessions::ManagedFollowerId,
                        )
                        .to(ManagedFollowers::Table, ManagedFollowers::Id)
                        .on_delete(ForeignKeyAction::Cascade),
                )
                .to_owned(),
        )
        .await
}

async fn create_master_bindings(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            Table::create()
                .table(MasterBindings::Table)
                .if_not_exists()
                .col(aster_forge_db_migration::big_integer_primary_key(
                    MasterBindings::Id,
                ))
                .col(
                    ColumnDef::new(MasterBindings::Name)
                        .string_len(128)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(MasterBindings::MasterUrl)
                        .string_len(512)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(MasterBindings::AccessKey)
                        .string_len(128)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(MasterBindings::SecretKey)
                        .string_len(255)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(MasterBindings::StorageNamespace)
                        .string_len(128)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(MasterBindings::IsEnabled)
                        .boolean()
                        .not_null()
                        .default(true),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(
                        manager,
                        MasterBindings::CreatedAt,
                    )
                    .not_null(),
                )
                .col(
                    aster_forge_db_migration::utc_date_time_column(
                        manager,
                        MasterBindings::UpdatedAt,
                    )
                    .not_null(),
                )
                .to_owned(),
        )
        .await
}

async fn create_managed_ingress_profiles(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let backend = manager.get_database_backend();
    let mut table = Table::create();
    table
        .table(ManagedIngressProfiles::Table)
        .if_not_exists()
        .col(aster_forge_db_migration::big_integer_primary_key(
            ManagedIngressProfiles::Id,
        ))
        .col(
            ColumnDef::new(ManagedIngressProfiles::MasterBindingId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(ManagedIngressProfiles::ProfileKey)
                .string_len(64)
                .not_null(),
        )
        .col(
            ColumnDef::new(ManagedIngressProfiles::Name)
                .string_len(128)
                .not_null(),
        )
        .col(
            ColumnDef::new(ManagedIngressProfiles::DriverType)
                .string_len(32)
                .not_null(),
        )
        .col(
            ColumnDef::new(ManagedIngressProfiles::Endpoint)
                .string_len(512)
                .not_null()
                .default(""),
        )
        .col(
            ColumnDef::new(ManagedIngressProfiles::Bucket)
                .string_len(255)
                .not_null()
                .default(""),
        )
        .col(
            ColumnDef::new(ManagedIngressProfiles::AccessKey)
                .string_len(512)
                .not_null()
                .default(""),
        )
        .col(
            ColumnDef::new(ManagedIngressProfiles::SecretKey)
                .string_len(512)
                .not_null()
                .default(""),
        )
        .col(
            ColumnDef::new(ManagedIngressProfiles::BasePath)
                .string_len(1024)
                .not_null()
                .default(""),
        )
        .col(
            ColumnDef::new(ManagedIngressProfiles::MaxFileSize)
                .big_integer()
                .not_null()
                .default(0),
        )
        .col(
            ColumnDef::new(ManagedIngressProfiles::IsDefault)
                .boolean()
                .not_null()
                .default(false),
        )
        .col(
            ColumnDef::new(ManagedIngressProfiles::DesiredRevision)
                .big_integer()
                .not_null()
                .default(1),
        )
        .col(
            ColumnDef::new(ManagedIngressProfiles::AppliedRevision)
                .big_integer()
                .not_null()
                .default(0),
        )
        .col(text_not_null_for_backend(
            backend,
            ManagedIngressProfiles::LastError,
            Some(""),
        ))
        .col(
            aster_forge_db_migration::utc_date_time_column(
                manager,
                ManagedIngressProfiles::CreatedAt,
            )
            .not_null(),
        )
        .col(
            aster_forge_db_migration::utc_date_time_column(
                manager,
                ManagedIngressProfiles::UpdatedAt,
            )
            .not_null(),
        );

    if backend != DbBackend::Sqlite {
        table.foreign_key(
            ForeignKey::create()
                .name("fk_managed_ingress_profiles_master_binding_id")
                .from(
                    ManagedIngressProfiles::Table,
                    ManagedIngressProfiles::MasterBindingId,
                )
                .to(MasterBindings::Table, MasterBindings::Id)
                .on_delete(ForeignKeyAction::Cascade),
        );
    }

    manager.create_table(table.to_owned()).await
}

async fn create_secondary_indexes(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    create_simple_indexes(manager).await?;
    create_mysql_prefix_indexes(manager).await?;
    create_live_name_unique_indexes(manager).await?;
    create_contact_verification_single_active_index(manager).await
}

async fn create_simple_indexes(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    for index in [
        Index::create()
            .name("idx_file_blobs_hash_policy")
            .table(FileBlobs::Table)
            .col(FileBlobs::Hash)
            .col(FileBlobs::PolicyId)
            .unique()
            .to_owned(),
        Index::create()
            .name("idx_spgi_group_priority")
            .table(StoragePolicyGroupItems::Table)
            .col(StoragePolicyGroupItems::GroupId)
            .col(StoragePolicyGroupItems::Priority)
            .unique()
            .to_owned(),
        Index::create()
            .name("idx_spgi_group_policy")
            .table(StoragePolicyGroupItems::Table)
            .col(StoragePolicyGroupItems::GroupId)
            .col(StoragePolicyGroupItems::PolicyId)
            .unique()
            .to_owned(),
        Index::create()
            .name("idx_users_policy_group_id")
            .table(Users::Table)
            .col(Users::PolicyGroupId)
            .to_owned(),
        Index::create()
            .name("idx_users_pending_email")
            .table(Users::Table)
            .col(Users::PendingEmail)
            .unique()
            .to_owned(),
        Index::create()
            .name("idx_storage_policies_remote_node_id")
            .table(StoragePolicies::Table)
            .col(StoragePolicies::RemoteNodeId)
            .to_owned(),
        Index::create()
            .name("idx_teams_created_by")
            .table(Teams::Table)
            .col(Teams::CreatedBy)
            .to_owned(),
        Index::create()
            .name("idx_teams_policy_group_id")
            .table(Teams::Table)
            .col(Teams::PolicyGroupId)
            .to_owned(),
        Index::create()
            .name("idx_teams_archived_at")
            .table(Teams::Table)
            .col(Teams::ArchivedAt)
            .to_owned(),
        Index::create()
            .name("idx_team_members_team_user")
            .table(TeamMembers::Table)
            .col(TeamMembers::TeamId)
            .col(TeamMembers::UserId)
            .unique()
            .to_owned(),
        Index::create()
            .name("idx_team_members_user_team")
            .table(TeamMembers::Table)
            .col(TeamMembers::UserId)
            .col(TeamMembers::TeamId)
            .to_owned(),
        Index::create()
            .name("idx_team_members_team_role")
            .table(TeamMembers::Table)
            .col(TeamMembers::TeamId)
            .col(TeamMembers::Role)
            .to_owned(),
        Index::create()
            .name("idx_folders_owner_deleted_parent_name")
            .table(Folders::Table)
            .col(Folders::OwnerUserId)
            .col(Folders::DeletedAt)
            .col(Folders::ParentId)
            .col(Folders::Name)
            .to_owned(),
        Index::create()
            .name("idx_files_owner_deleted_folder_name")
            .table(Files::Table)
            .col(Files::OwnerUserId)
            .col(Files::DeletedAt)
            .col(Files::FolderId)
            .col(Files::Name)
            .to_owned(),
        Index::create()
            .name("idx_folders_owner_deleted_at_id")
            .table(Folders::Table)
            .col(Folders::OwnerUserId)
            .col((Folders::DeletedAt, IndexOrder::Desc))
            .col((Folders::Id, IndexOrder::Asc))
            .to_owned(),
        Index::create()
            .name("idx_files_owner_deleted_at_id")
            .table(Files::Table)
            .col(Files::OwnerUserId)
            .col((Files::DeletedAt, IndexOrder::Desc))
            .col((Files::Id, IndexOrder::Asc))
            .to_owned(),
        Index::create()
            .name("idx_folders_created_by_user_id")
            .table(Folders::Table)
            .col(Folders::CreatedByUserId)
            .to_owned(),
        Index::create()
            .name("idx_files_created_by_user_id")
            .table(Files::Table)
            .col(Files::CreatedByUserId)
            .to_owned(),
        Index::create()
            .name("idx_files_team_id")
            .table(Files::Table)
            .col(Files::TeamId)
            .to_owned(),
        Index::create()
            .name("idx_files_team_deleted_folder_name")
            .table(Files::Table)
            .col(Files::TeamId)
            .col(Files::DeletedAt)
            .col(Files::FolderId)
            .col(Files::Name)
            .to_owned(),
        Index::create()
            .name("idx_folders_team_id")
            .table(Folders::Table)
            .col(Folders::TeamId)
            .to_owned(),
        Index::create()
            .name("idx_folders_team_deleted_parent_name")
            .table(Folders::Table)
            .col(Folders::TeamId)
            .col(Folders::DeletedAt)
            .col(Folders::ParentId)
            .col(Folders::Name)
            .to_owned(),
        Index::create()
            .name("idx_upload_sessions_team_id")
            .table(UploadSessions::Table)
            .col(UploadSessions::TeamId)
            .to_owned(),
        Index::create()
            .name("idx_shares_token")
            .table(Shares::Table)
            .col(Shares::Token)
            .unique()
            .to_owned(),
        Index::create()
            .name("idx_shares_user_file")
            .table(Shares::Table)
            .col(Shares::UserId)
            .col(Shares::FileId)
            .to_owned(),
        Index::create()
            .name("idx_shares_user_folder")
            .table(Shares::Table)
            .col(Shares::UserId)
            .col(Shares::FolderId)
            .to_owned(),
        Index::create()
            .name("idx_shares_team_id")
            .table(Shares::Table)
            .col(Shares::TeamId)
            .to_owned(),
        Index::create()
            .name("idx_shares_team_file")
            .table(Shares::Table)
            .col(Shares::TeamId)
            .col(Shares::FileId)
            .to_owned(),
        Index::create()
            .name("idx_shares_team_folder")
            .table(Shares::Table)
            .col(Shares::TeamId)
            .col(Shares::FolderId)
            .to_owned(),
        Index::create()
            .name("idx_upload_sessions_status_expires_at")
            .table(UploadSessions::Table)
            .col(UploadSessions::Status)
            .col(UploadSessions::ExpiresAt)
            .to_owned(),
        Index::create()
            .name("idx_files_blob_id")
            .table(Files::Table)
            .col(Files::BlobId)
            .to_owned(),
        Index::create()
            .name("idx_file_versions_file_id")
            .table(FileVersions::Table)
            .col(FileVersions::FileId)
            .to_owned(),
        Index::create()
            .name("idx_file_versions_blob_id")
            .table(FileVersions::Table)
            .col(FileVersions::BlobId)
            .to_owned(),
        Index::create()
            .name("uq_upload_session_parts_upload_id_part_number")
            .table(UploadSessionParts::Table)
            .col(UploadSessionParts::UploadId)
            .col(UploadSessionParts::PartNumber)
            .unique()
            .to_owned(),
        Index::create()
            .name("idx_entity_properties_unique")
            .table(EntityProperties::Table)
            .col(EntityProperties::EntityType)
            .col(EntityProperties::EntityId)
            .col(EntityProperties::Namespace)
            .col(EntityProperties::Name)
            .unique()
            .to_owned(),
        Index::create()
            .name("idx_entity_properties_entity")
            .table(EntityProperties::Table)
            .col(EntityProperties::EntityType)
            .col(EntityProperties::EntityId)
            .to_owned(),
        Index::create()
            .name("idx_resource_locks_entity")
            .table(ResourceLocks::Table)
            .col(ResourceLocks::EntityType)
            .col(ResourceLocks::EntityId)
            .unique()
            .to_owned(),
        Index::create()
            .name("idx_resource_locks_path")
            .table(ResourceLocks::Table)
            .col(ResourceLocks::Path)
            .to_owned(),
        Index::create()
            .name("idx_audit_logs_user_id")
            .table(AuditLogs::Table)
            .col(AuditLogs::UserId)
            .to_owned(),
        Index::create()
            .name("idx_audit_logs_action")
            .table(AuditLogs::Table)
            .col(AuditLogs::Action)
            .to_owned(),
        Index::create()
            .name("idx_audit_logs_created_at")
            .table(AuditLogs::Table)
            .col(AuditLogs::CreatedAt)
            .to_owned(),
        Index::create()
            .name("idx_audit_logs_entity")
            .table(AuditLogs::Table)
            .col(AuditLogs::EntityType)
            .col(AuditLogs::EntityId)
            .to_owned(),
        Index::create()
            .name("idx_contact_verification_tokens_user_purpose")
            .table(ContactVerificationTokens::Table)
            .col(ContactVerificationTokens::UserId)
            .col(ContactVerificationTokens::Channel)
            .col(ContactVerificationTokens::Purpose)
            .to_owned(),
        Index::create()
            .name("idx_contact_verification_tokens_expires_at")
            .table(ContactVerificationTokens::Table)
            .col(ContactVerificationTokens::ExpiresAt)
            .to_owned(),
        Index::create()
            .name("idx_mail_outbox_due")
            .table(MailOutbox::Table)
            .col(MailOutbox::Status)
            .col(MailOutbox::NextAttemptAt)
            .col(MailOutbox::CreatedAt)
            .to_owned(),
        Index::create()
            .name("idx_mail_outbox_processing")
            .table(MailOutbox::Table)
            .col(MailOutbox::Status)
            .col(MailOutbox::ProcessingStartedAt)
            .col(MailOutbox::CreatedAt)
            .to_owned(),
        Index::create()
            .name("idx_mail_outbox_sent_at")
            .table(MailOutbox::Table)
            .col(MailOutbox::SentAt)
            .to_owned(),
        Index::create()
            .name("idx_background_tasks_due")
            .table(BackgroundTasks::Table)
            .col(BackgroundTasks::Status)
            .col(BackgroundTasks::NextRunAt)
            .col(BackgroundTasks::CreatedAt)
            .to_owned(),
        Index::create()
            .name("idx_background_tasks_processing")
            .table(BackgroundTasks::Table)
            .col(BackgroundTasks::Status)
            .col(BackgroundTasks::ProcessingStartedAt)
            .col(BackgroundTasks::CreatedAt)
            .to_owned(),
        Index::create()
            .name("idx_background_tasks_personal")
            .table(BackgroundTasks::Table)
            .col(BackgroundTasks::CreatorUserId)
            .col(BackgroundTasks::TeamId)
            .col(BackgroundTasks::CreatedAt)
            .to_owned(),
        Index::create()
            .name("idx_background_tasks_team")
            .table(BackgroundTasks::Table)
            .col(BackgroundTasks::TeamId)
            .col(BackgroundTasks::CreatedAt)
            .to_owned(),
        Index::create()
            .name("idx_background_tasks_expires_at")
            .table(BackgroundTasks::Table)
            .col(BackgroundTasks::ExpiresAt)
            .to_owned(),
        Index::create()
            .name("idx_background_tasks_processing_heartbeat")
            .table(BackgroundTasks::Table)
            .col(BackgroundTasks::Status)
            .col(BackgroundTasks::LastHeartbeatAt)
            .col(BackgroundTasks::CreatedAt)
            .to_owned(),
        Index::create()
            .name("idx_background_tasks_processing_lease")
            .table(BackgroundTasks::Table)
            .col(BackgroundTasks::Status)
            .col(BackgroundTasks::LeaseExpiresAt)
            .col(BackgroundTasks::CreatedAt)
            .to_owned(),
        Index::create()
            .name("idx_wopi_sessions_expires_at")
            .table(WopiSessions::Table)
            .col(WopiSessions::ExpiresAt)
            .to_owned(),
        Index::create()
            .name("idx_auth_sessions_user_id")
            .table(AuthSessions::Table)
            .col(AuthSessions::UserId)
            .to_owned(),
        Index::create()
            .name("idx_auth_sessions_current_refresh_jti")
            .table(AuthSessions::Table)
            .col(AuthSessions::CurrentRefreshJti)
            .unique()
            .to_owned(),
        Index::create()
            .name("idx_auth_sessions_refresh_expires_at")
            .table(AuthSessions::Table)
            .col(AuthSessions::RefreshExpiresAt)
            .to_owned(),
        Index::create()
            .name("idx_auth_sessions_previous_refresh_jti")
            .table(AuthSessions::Table)
            .col(AuthSessions::PreviousRefreshJti)
            .to_owned(),
        Index::create()
            .name("idx_managed_followers_access_key")
            .table(ManagedFollowers::Table)
            .col(ManagedFollowers::AccessKey)
            .unique()
            .to_owned(),
        Index::create()
            .name("idx_follower_enrollment_sessions_managed_follower_id")
            .table(FollowerEnrollmentSessions::Table)
            .col(FollowerEnrollmentSessions::ManagedFollowerId)
            .to_owned(),
        Index::create()
            .name("idx_follower_enrollment_sessions_token_hash")
            .table(FollowerEnrollmentSessions::Table)
            .col(FollowerEnrollmentSessions::TokenHash)
            .unique()
            .to_owned(),
        Index::create()
            .name("idx_follower_enrollment_sessions_ack_token_hash")
            .table(FollowerEnrollmentSessions::Table)
            .col(FollowerEnrollmentSessions::AckTokenHash)
            .unique()
            .to_owned(),
        Index::create()
            .name("idx_master_bindings_access_key")
            .table(MasterBindings::Table)
            .col(MasterBindings::AccessKey)
            .unique()
            .to_owned(),
        Index::create()
            .name("idx_master_bindings_storage_namespace")
            .table(MasterBindings::Table)
            .col(MasterBindings::StorageNamespace)
            .unique()
            .to_owned(),
        Index::create()
            .name("idx_managed_ingress_profiles_binding_profile_key")
            .table(ManagedIngressProfiles::Table)
            .col(ManagedIngressProfiles::MasterBindingId)
            .col(ManagedIngressProfiles::ProfileKey)
            .unique()
            .to_owned(),
        Index::create()
            .name("idx_managed_ingress_profiles_binding_default")
            .table(ManagedIngressProfiles::Table)
            .col(ManagedIngressProfiles::MasterBindingId)
            .col(ManagedIngressProfiles::IsDefault)
            .to_owned(),
    ] {
        manager.create_index(index).await?;
    }

    Ok(())
}

async fn create_mysql_prefix_indexes(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let mut file_blob_storage_path = Index::create();
    file_blob_storage_path
        .name("idx_file_blobs_storage_path")
        .table(FileBlobs::Table);

    if manager.get_database_backend() == DbBackend::MySql {
        file_blob_storage_path.col((FileBlobs::StoragePath, 255));
    } else {
        file_blob_storage_path.col(FileBlobs::StoragePath);
    }

    manager
        .create_index(file_blob_storage_path.to_owned())
        .await
}

async fn create_live_name_unique_indexes(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let db = manager.get_connection();
    let statements = match manager.get_database_backend() {
        DbBackend::Sqlite | DbBackend::Postgres => [
            "CREATE UNIQUE INDEX idx_files_unique_live_name \
             ON files ( \
                (CASE WHEN team_id IS NULL THEN 0 ELSE 1 END), \
                (CASE WHEN team_id IS NULL THEN owner_user_id ELSE team_id END), \
                (COALESCE(folder_id, 0)), \
                name, \
                (CASE WHEN deleted_at IS NULL THEN 1 ELSE NULL END) \
             );",
            "CREATE UNIQUE INDEX idx_folders_unique_live_name \
             ON folders ( \
                (CASE WHEN team_id IS NULL THEN 0 ELSE 1 END), \
                (CASE WHEN team_id IS NULL THEN owner_user_id ELSE team_id END), \
                (COALESCE(parent_id, 0)), \
                name, \
                (CASE WHEN deleted_at IS NULL THEN 1 ELSE NULL END) \
             );",
        ],
        DbBackend::MySql => [
            "CREATE UNIQUE INDEX idx_files_unique_live_name \
             ON files ( \
                ((CASE WHEN team_id IS NULL THEN 0 ELSE 1 END)), \
                ((CASE WHEN team_id IS NULL THEN owner_user_id ELSE team_id END)), \
                ((COALESCE(folder_id, 0))), \
                name, \
                ((CASE WHEN deleted_at IS NULL THEN 1 ELSE NULL END)) \
             );",
            "CREATE UNIQUE INDEX idx_folders_unique_live_name \
             ON folders ( \
                ((CASE WHEN team_id IS NULL THEN 0 ELSE 1 END)), \
                ((CASE WHEN team_id IS NULL THEN owner_user_id ELSE team_id END)), \
                ((COALESCE(parent_id, 0))), \
                name, \
                ((CASE WHEN deleted_at IS NULL THEN 1 ELSE NULL END)) \
             );",
        ],
        backend => {
            return Err(DbErr::Migration(format!(
                "unsupported database backend for live-name unique indexes: {backend:?}"
            )));
        }
    };

    for statement in statements {
        db.execute_unprepared(statement).await?;
    }

    Ok(())
}

async fn create_contact_verification_single_active_index(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    let statement = match manager.get_database_backend() {
        DbBackend::Sqlite | DbBackend::Postgres => {
            "CREATE UNIQUE INDEX idx_contact_verification_tokens_single_active \
             ON contact_verification_tokens ( \
                user_id, \
                channel, \
                purpose, \
                (CASE WHEN consumed_at IS NULL THEN 1 ELSE NULL END) \
             );"
        }
        DbBackend::MySql => {
            "CREATE UNIQUE INDEX idx_contact_verification_tokens_single_active \
             ON contact_verification_tokens ( \
                user_id, \
                channel, \
                purpose, \
                ((CASE WHEN consumed_at IS NULL THEN 1 ELSE NULL END)) \
             );"
        }
        backend => {
            return Err(DbErr::Migration(format!(
                "unsupported database backend for contact verification active index: {backend:?}"
            )));
        }
    };

    manager
        .get_connection()
        .execute_unprepared(statement)
        .await?;
    Ok(())
}

async fn create_search_acceleration(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    match manager.get_database_backend() {
        DbBackend::Sqlite => {
            let statements = sqlite_fts_up_statements(&files_name_fts_config())?
                .into_iter()
                .chain(sqlite_fts_up_statements(&folders_name_fts_config())?)
                .chain(sqlite_fts_up_statements(&users_search_fts_config())?)
                .chain(sqlite_fts_up_statements(&teams_search_fts_config())?);
            execute_sqlite_statements(
                manager,
                statements,
                "SQLite search acceleration baseline requires FTS5 with trigram tokenizer support",
            )
            .await
        }
        DbBackend::Postgres => {
            ensure_postgres_extension(manager, "pg_trgm").await?;
            manager
                .get_connection()
                .execute_unprepared(
                    "CREATE INDEX IF NOT EXISTS idx_files_live_name_trgm \
                     ON files USING gin (name gin_trgm_ops) \
                     WHERE deleted_at IS NULL",
                )
                .await?;
            manager
                .get_connection()
                .execute_unprepared(
                    "CREATE INDEX IF NOT EXISTS idx_folders_live_name_trgm \
                     ON folders USING gin (name gin_trgm_ops) \
                     WHERE deleted_at IS NULL",
                )
                .await?;
            manager
                .create_index(postgres_trigram_index(
                    POSTGRES_USERS_USERNAME_TRGM_INDEX,
                    "users",
                    "username",
                ))
                .await?;
            manager
                .create_index(postgres_trigram_index(
                    POSTGRES_USERS_EMAIL_TRGM_INDEX,
                    "users",
                    "email",
                ))
                .await?;
            manager
                .create_index(postgres_trigram_index(
                    POSTGRES_TEAMS_NAME_TRGM_INDEX,
                    "teams",
                    "name",
                ))
                .await?;
            manager
                .create_index(postgres_trigram_index(
                    POSTGRES_TEAMS_DESCRIPTION_TRGM_INDEX,
                    "teams",
                    "description",
                ))
                .await
        }
        DbBackend::MySql => {
            for statement in [
                mysql_fulltext_index_sql("idx_files_name_fulltext", "files", &["name"])?,
                mysql_fulltext_index_sql("idx_folders_name_fulltext", "folders", &["name"])?,
                mysql_fulltext_index_sql(
                    MYSQL_USERS_SEARCH_FULLTEXT_INDEX,
                    "users",
                    &["username", "email"],
                )?,
                mysql_fulltext_index_sql(
                    MYSQL_TEAMS_SEARCH_FULLTEXT_INDEX,
                    "teams",
                    &["name", "description"],
                )?,
            ] {
                manager
                    .get_connection()
                    .execute_unprepared(&statement)
                    .await?;
            }
            Ok(())
        }
        backend => Err(DbErr::Migration(format!(
            "unsupported database backend for search acceleration baseline: {backend:?}"
        ))),
    }
}

async fn drop_search_acceleration(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    match manager.get_database_backend() {
        DbBackend::Sqlite => {
            let statements = sqlite_fts_down_statements(&teams_search_fts_config())?
                .into_iter()
                .chain(sqlite_fts_down_statements(&users_search_fts_config())?)
                .chain(sqlite_fts_down_statements(&folders_name_fts_config())?)
                .chain(sqlite_fts_down_statements(&files_name_fts_config())?);
            execute_sqlite_statements(
                manager,
                statements,
                "drop SQLite search acceleration baseline objects",
            )
            .await
        }
        DbBackend::Postgres => {
            for index in [
                POSTGRES_TEAMS_DESCRIPTION_TRGM_INDEX,
                POSTGRES_TEAMS_NAME_TRGM_INDEX,
                POSTGRES_USERS_EMAIL_TRGM_INDEX,
                POSTGRES_USERS_USERNAME_TRGM_INDEX,
            ] {
                manager.drop_index(postgres_drop_index(index)).await?;
            }
            manager
                .get_connection()
                .execute_unprepared("DROP INDEX IF EXISTS idx_folders_live_name_trgm")
                .await?;
            manager
                .get_connection()
                .execute_unprepared("DROP INDEX IF EXISTS idx_files_live_name_trgm")
                .await?;
            Ok(())
        }
        DbBackend::MySql => Ok(()),
        backend => Err(DbErr::Migration(format!(
            "unsupported database backend for dropping search acceleration baseline: {backend:?}"
        ))),
    }
}

fn files_name_fts_config() -> SqliteFtsConfig<'static> {
    SqliteFtsConfig {
        virtual_table: FILES_NAME_FTS_TABLE,
        source_table: "files",
        columns: &["name"],
        insert_trigger: FILES_NAME_FTS_INSERT_TRIGGER,
        delete_trigger: FILES_NAME_FTS_DELETE_TRIGGER,
        update_trigger: FILES_NAME_FTS_UPDATE_TRIGGER,
    }
}

fn folders_name_fts_config() -> SqliteFtsConfig<'static> {
    SqliteFtsConfig {
        virtual_table: FOLDERS_NAME_FTS_TABLE,
        source_table: "folders",
        columns: &["name"],
        insert_trigger: FOLDERS_NAME_FTS_INSERT_TRIGGER,
        delete_trigger: FOLDERS_NAME_FTS_DELETE_TRIGGER,
        update_trigger: FOLDERS_NAME_FTS_UPDATE_TRIGGER,
    }
}

fn users_search_fts_config() -> SqliteFtsConfig<'static> {
    SqliteFtsConfig {
        virtual_table: USERS_SEARCH_FTS_TABLE,
        source_table: "users",
        columns: &["username", "email"],
        insert_trigger: USERS_SEARCH_FTS_INSERT_TRIGGER,
        delete_trigger: USERS_SEARCH_FTS_DELETE_TRIGGER,
        update_trigger: USERS_SEARCH_FTS_UPDATE_TRIGGER,
    }
}

fn teams_search_fts_config() -> SqliteFtsConfig<'static> {
    SqliteFtsConfig {
        virtual_table: TEAMS_SEARCH_FTS_TABLE,
        source_table: "teams",
        columns: &["name", "description"],
        insert_trigger: TEAMS_SEARCH_FTS_INSERT_TRIGGER,
        delete_trigger: TEAMS_SEARCH_FTS_DELETE_TRIGGER,
        update_trigger: TEAMS_SEARCH_FTS_UPDATE_TRIGGER,
    }
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
    Username,
    Email,
    PasswordHash,
    Role,
    Status,
    SessionVersion,
    EmailVerifiedAt,
    PendingEmail,
    StorageUsed,
    StorageQuota,
    PolicyGroupId,
    CreatedAt,
    UpdatedAt,
    Config,
}

#[derive(DeriveIden)]
enum StoragePolicies {
    Table,
    Id,
    Name,
    DriverType,
    Endpoint,
    Bucket,
    AccessKey,
    SecretKey,
    BasePath,
    RemoteNodeId,
    MaxFileSize,
    AllowedTypes,
    Options,
    IsDefault,
    ChunkSize,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum Folders {
    Table,
    Id,
    Name,
    ParentId,
    TeamId,
    OwnerUserId,
    CreatedByUserId,
    CreatedByUsername,
    PolicyId,
    CreatedAt,
    UpdatedAt,
    DeletedAt,
    IsLocked,
}

#[derive(DeriveIden)]
enum FileBlobs {
    Table,
    Id,
    Hash,
    Size,
    PolicyId,
    StoragePath,
    ThumbnailPath,
    ThumbnailProcessor,
    ThumbnailVersion,
    RefCount,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum Files {
    Table,
    Id,
    Name,
    FolderId,
    TeamId,
    BlobId,
    Size,
    OwnerUserId,
    CreatedByUserId,
    CreatedByUsername,
    MimeType,
    CreatedAt,
    UpdatedAt,
    DeletedAt,
    IsLocked,
}

#[derive(DeriveIden)]
enum SystemConfig {
    Table,
    Id,
    Key,
    Value,
    ValueType,
    RequiresRestart,
    IsSensitive,
    Source,
    Namespace,
    Category,
    Description,
    UpdatedAt,
    UpdatedBy,
}

#[derive(DeriveIden)]
enum UploadSessions {
    Table,
    Id,
    UserId,
    TeamId,
    Filename,
    TotalSize,
    ChunkSize,
    TotalChunks,
    ReceivedCount,
    FolderId,
    PolicyId,
    Status,
    S3TempKey,
    S3MultipartId,
    FileId,
    CreatedAt,
    ExpiresAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum WebdavAccounts {
    Table,
    Id,
    UserId,
    Username,
    PasswordHash,
    RootFolderId,
    IsActive,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum EntityProperties {
    Table,
    Id,
    EntityType,
    EntityId,
    Namespace,
    Name,
    Value,
}

#[derive(DeriveIden)]
enum ResourceLocks {
    Table,
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
enum FileVersions {
    Table,
    Id,
    FileId,
    BlobId,
    Version,
    Size,
    CreatedAt,
}

#[derive(DeriveIden)]
enum AuditLogs {
    Table,
    Id,
    UserId,
    Action,
    EntityType,
    EntityId,
    EntityName,
    Details,
    IpAddress,
    UserAgent,
    CreatedAt,
}

#[derive(DeriveIden)]
enum UploadSessionParts {
    Table,
    Id,
    UploadId,
    PartNumber,
    Etag,
    Size,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum StoragePolicyGroups {
    Table,
    Id,
    Name,
    Description,
    IsEnabled,
    IsDefault,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum StoragePolicyGroupItems {
    Table,
    Id,
    GroupId,
    PolicyId,
    Priority,
    MinFileSize,
    MaxFileSize,
    CreatedAt,
}

#[derive(DeriveIden)]
enum Teams {
    Table,
    Id,
    Name,
    Description,
    CreatedBy,
    StorageUsed,
    StorageQuota,
    PolicyGroupId,
    CreatedAt,
    UpdatedAt,
    ArchivedAt,
}

#[derive(DeriveIden)]
enum TeamMembers {
    Table,
    Id,
    TeamId,
    UserId,
    Role,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum Shares {
    Table,
    Id,
    Token,
    UserId,
    TeamId,
    FileId,
    FolderId,
    Password,
    ExpiresAt,
    MaxDownloads,
    DownloadCount,
    ViewCount,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum ContactVerificationTokens {
    Table,
    Id,
    UserId,
    Channel,
    Purpose,
    Target,
    TokenHash,
    ExpiresAt,
    ConsumedAt,
    CreatedAt,
}

#[derive(DeriveIden)]
enum MailOutbox {
    Table,
    Id,
    TemplateCode,
    ToAddress,
    ToName,
    PayloadJson,
    Status,
    AttemptCount,
    NextAttemptAt,
    ProcessingStartedAt,
    SentAt,
    LastError,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum BackgroundTasks {
    Table,
    Id,
    Kind,
    Status,
    CreatorUserId,
    TeamId,
    ShareId,
    DisplayName,
    PayloadJson,
    ResultJson,
    StepsJson,
    ProgressCurrent,
    ProgressTotal,
    StatusText,
    AttemptCount,
    MaxAttempts,
    NextRunAt,
    ProcessingToken,
    ProcessingStartedAt,
    LastHeartbeatAt,
    LeaseExpiresAt,
    StartedAt,
    FinishedAt,
    LastError,
    FailureCanRetry,
    ExpiresAt,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum WopiSessions {
    Table,
    Id,
    TokenHash,
    ActorUserId,
    SessionVersion,
    TeamId,
    FileId,
    AppKey,
    ExpiresAt,
    CreatedAt,
}

#[derive(DeriveIden)]
enum UserProfiles {
    Table,
    UserId,
    DisplayName,
    WopiUserInfo,
    AvatarSource,
    AvatarKey,
    AvatarVersion,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum AuthSessions {
    Table,
    Id,
    UserId,
    CurrentRefreshJti,
    PreviousRefreshJti,
    RefreshExpiresAt,
    IpAddress,
    UserAgent,
    CreatedAt,
    LastSeenAt,
    RevokedAt,
}

#[derive(DeriveIden)]
enum ManagedFollowers {
    Table,
    Id,
    Name,
    BaseUrl,
    AccessKey,
    SecretKey,
    IsEnabled,
    LastCapabilities,
    LastError,
    LastCheckedAt,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum FollowerEnrollmentSessions {
    Table,
    Id,
    ManagedFollowerId,
    TokenHash,
    AckTokenHash,
    ExpiresAt,
    RedeemedAt,
    AckedAt,
    InvalidatedAt,
    CreatedAt,
}

#[derive(DeriveIden)]
enum MasterBindings {
    Table,
    Id,
    Name,
    MasterUrl,
    AccessKey,
    SecretKey,
    StorageNamespace,
    IsEnabled,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum ManagedIngressProfiles {
    Table,
    Id,
    MasterBindingId,
    ProfileKey,
    Name,
    DriverType,
    Endpoint,
    Bucket,
    AccessKey,
    SecretKey,
    BasePath,
    MaxFileSize,
    IsDefault,
    DesiredRevision,
    AppliedRevision,
    LastError,
    CreatedAt,
    UpdatedAt,
}
