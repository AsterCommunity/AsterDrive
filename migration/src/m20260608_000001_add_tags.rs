//! Add first-class tag definitions and reverse lookup index for tag bindings.

use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        create_tags(manager).await?;
        create_tag_indexes(manager).await?;
        create_entity_property_tag_lookup_index(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("idx_entity_properties_namespace_name_entity")
                    .table(EntityProperties::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(Table::drop().table(Tags::Table).if_exists().to_owned())
            .await
    }
}

async fn create_tags(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            Table::create()
                .table(Tags::Table)
                .if_not_exists()
                .col(
                    ColumnDef::new(Tags::Id)
                        .big_integer()
                        .not_null()
                        .auto_increment()
                        .primary_key(),
                )
                .col(ColumnDef::new(Tags::ScopeType).string_len(16).not_null())
                .col(ColumnDef::new(Tags::OwnerUserId).big_integer().null())
                .col(ColumnDef::new(Tags::TeamId).big_integer().null())
                .col(ColumnDef::new(Tags::Name).string_len(64).not_null())
                .col(
                    ColumnDef::new(Tags::NormalizedName)
                        .string_len(64)
                        .not_null(),
                )
                .col(ColumnDef::new(Tags::Color).string_len(16).not_null())
                .col(
                    ColumnDef::new(Tags::SortOrder)
                        .integer()
                        .not_null()
                        .default(0),
                )
                .col(crate::time::utc_date_time_column(manager, Tags::CreatedAt).not_null())
                .col(crate::time::utc_date_time_column(manager, Tags::UpdatedAt).not_null())
                .check((
                    Alias::new("ck_tags_scope_owner"),
                    Expr::cust(
                        "(scope_type = 'personal' AND owner_user_id IS NOT NULL AND team_id IS NULL) OR \
                         (scope_type = 'team' AND team_id IS NOT NULL AND owner_user_id IS NULL)",
                    ),
                ))
                .foreign_key(
                    ForeignKey::create()
                        .name("fk_tags_owner_user_id")
                        .from(Tags::Table, Tags::OwnerUserId)
                        .to(Users::Table, Users::Id)
                        .on_delete(ForeignKeyAction::Cascade),
                )
                .foreign_key(
                    ForeignKey::create()
                        .name("fk_tags_team_id")
                        .from(Tags::Table, Tags::TeamId)
                        .to(Teams::Table, Teams::Id)
                        .on_delete(ForeignKeyAction::Cascade),
                )
                .to_owned(),
        )
        .await
}

async fn create_tag_indexes(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_index(
            Index::create()
                .name("idx_tags_personal_unique")
                .table(Tags::Table)
                .col(Tags::ScopeType)
                .col(Tags::OwnerUserId)
                .col(Tags::NormalizedName)
                .unique()
                .to_owned(),
        )
        .await?;
    manager
        .create_index(
            Index::create()
                .name("idx_tags_team_unique")
                .table(Tags::Table)
                .col(Tags::ScopeType)
                .col(Tags::TeamId)
                .col(Tags::NormalizedName)
                .unique()
                .to_owned(),
        )
        .await?;
    manager
        .create_index(
            Index::create()
                .name("idx_tags_scope_sort")
                .table(Tags::Table)
                .col(Tags::ScopeType)
                .col(Tags::OwnerUserId)
                .col(Tags::TeamId)
                .col(Tags::SortOrder)
                .col(Tags::Name)
                .to_owned(),
        )
        .await
}

async fn create_entity_property_tag_lookup_index(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_index(
            Index::create()
                .name("idx_entity_properties_namespace_name_entity")
                .table(EntityProperties::Table)
                .col(EntityProperties::Namespace)
                .col(EntityProperties::Name)
                .col(EntityProperties::EntityType)
                .col(EntityProperties::EntityId)
                .to_owned(),
        )
        .await
}

#[derive(DeriveIden)]
enum Tags {
    Table,
    Id,
    ScopeType,
    OwnerUserId,
    TeamId,
    Name,
    NormalizedName,
    Color,
    SortOrder,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum EntityProperties {
    Table,
    Namespace,
    Name,
    EntityType,
    EntityId,
}

#[derive(DeriveIden)]
enum Users {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum Teams {
    Table,
    Id,
}
