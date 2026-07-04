//! Rename managed ingress profile storage tables to remote storage target terms.

use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::DbBackend;

use crate::index_helpers::{drop_index_if_exists, rename_mysql_index_if_exists};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if manager.get_database_backend() != DbBackend::MySql {
            drop_index_if_exists(
                manager,
                "managed_ingress_profiles",
                "idx_managed_ingress_profiles_binding_profile_key",
            )
            .await?;
            drop_index_if_exists(
                manager,
                "managed_ingress_profiles",
                "idx_managed_ingress_profiles_binding_default",
            )
            .await?;
        }

        manager
            .rename_table(
                Table::rename()
                    .table(ManagedIngressProfiles::Table, RemoteStorageTargets::Table)
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(RemoteStorageTargets::Table)
                    .rename_column(
                        ManagedIngressProfiles::ProfileKey,
                        RemoteStorageTargets::TargetKey,
                    )
                    .to_owned(),
            )
            .await?;

        if manager.get_database_backend() == DbBackend::MySql {
            rename_mysql_index_if_exists(
                manager,
                "remote_storage_targets",
                "idx_managed_ingress_profiles_binding_profile_key",
                "idx_remote_storage_targets_binding_target_key",
            )
            .await?;
            rename_mysql_index_if_exists(
                manager,
                "remote_storage_targets",
                "idx_managed_ingress_profiles_binding_default",
                "idx_remote_storage_targets_binding_default",
            )
            .await
        } else {
            create_target_indexes(manager).await
        }
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if manager.get_database_backend() != DbBackend::MySql {
            drop_index_if_exists(
                manager,
                "remote_storage_targets",
                "idx_remote_storage_targets_binding_target_key",
            )
            .await?;
            drop_index_if_exists(
                manager,
                "remote_storage_targets",
                "idx_remote_storage_targets_binding_default",
            )
            .await?;
        }

        manager
            .alter_table(
                Table::alter()
                    .table(RemoteStorageTargets::Table)
                    .rename_column(
                        RemoteStorageTargets::TargetKey,
                        ManagedIngressProfiles::ProfileKey,
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .rename_table(
                Table::rename()
                    .table(RemoteStorageTargets::Table, ManagedIngressProfiles::Table)
                    .to_owned(),
            )
            .await?;

        if manager.get_database_backend() == DbBackend::MySql {
            rename_mysql_index_if_exists(
                manager,
                "managed_ingress_profiles",
                "idx_remote_storage_targets_binding_target_key",
                "idx_managed_ingress_profiles_binding_profile_key",
            )
            .await?;
            rename_mysql_index_if_exists(
                manager,
                "managed_ingress_profiles",
                "idx_remote_storage_targets_binding_default",
                "idx_managed_ingress_profiles_binding_default",
            )
            .await
        } else {
            create_profile_indexes(manager).await
        }
    }
}

async fn create_target_indexes(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_index(
            Index::create()
                .name("idx_remote_storage_targets_binding_target_key")
                .table(RemoteStorageTargets::Table)
                .col(RemoteStorageTargets::MasterBindingId)
                .col(RemoteStorageTargets::TargetKey)
                .unique()
                .to_owned(),
        )
        .await?;

    manager
        .create_index(
            Index::create()
                .name("idx_remote_storage_targets_binding_default")
                .table(RemoteStorageTargets::Table)
                .col(RemoteStorageTargets::MasterBindingId)
                .col(RemoteStorageTargets::IsDefault)
                .to_owned(),
        )
        .await
}

async fn create_profile_indexes(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_index(
            Index::create()
                .name("idx_managed_ingress_profiles_binding_profile_key")
                .table(ManagedIngressProfiles::Table)
                .col(ManagedIngressProfiles::MasterBindingId)
                .col(ManagedIngressProfiles::ProfileKey)
                .unique()
                .to_owned(),
        )
        .await?;

    manager
        .create_index(
            Index::create()
                .name("idx_managed_ingress_profiles_binding_default")
                .table(ManagedIngressProfiles::Table)
                .col(ManagedIngressProfiles::MasterBindingId)
                .col(ManagedIngressProfiles::IsDefault)
                .to_owned(),
        )
        .await
}

#[derive(DeriveIden)]
enum ManagedIngressProfiles {
    Table,
    MasterBindingId,
    ProfileKey,
    IsDefault,
}

#[derive(DeriveIden)]
enum RemoteStorageTargets {
    Table,
    MasterBindingId,
    TargetKey,
    IsDefault,
}
