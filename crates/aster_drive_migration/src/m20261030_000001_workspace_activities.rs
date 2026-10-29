//! Business attribution and durable workspace activity, independent of optional audit logging.
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for table in ["files", "folders"] {
            for mut column in [
                ColumnDef::new(Alias::new("modified_by_user_id"))
                    .big_integer()
                    .null()
                    .to_owned(),
                ColumnDef::new(Alias::new("modified_by_username"))
                    .string_len(255)
                    .null()
                    .to_owned(),
                aster_forge_db_migration::utc_date_time_column(manager, Alias::new("modified_at"))
                    .null()
                    .to_owned(),
            ] {
                manager
                    .alter_table(
                        Table::alter()
                            .table(Alias::new(table))
                            .add_column(&mut column)
                            .to_owned(),
                    )
                    .await?;
            }
        }
        // No cascading references: actor/resource deletion must preserve the historical fact.
        manager
            .create_table(
                Table::create()
                    .table(WorkspaceActivities::Table)
                    .col(
                        ColumnDef::new(WorkspaceActivities::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(WorkspaceActivities::EventId)
                            .string_len(36)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(WorkspaceActivities::OperationId)
                            .string_len(36)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(WorkspaceActivities::TeamId)
                            .big_integer()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(WorkspaceActivities::OwnerUserId)
                            .big_integer()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(WorkspaceActivities::EntityType)
                            .string_len(16)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(WorkspaceActivities::EntityId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(WorkspaceActivities::EntityName)
                            .string_len(255)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(WorkspaceActivities::ActorUserId)
                            .big_integer()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(WorkspaceActivities::ActorName)
                            .string_len(255)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(WorkspaceActivities::Action)
                            .string_len(64)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(WorkspaceActivities::Source)
                            .string_len(32)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(WorkspaceActivities::Details)
                            .text()
                            .not_null(),
                    )
                    .col(
                        aster_forge_db_migration::utc_date_time_column(
                            manager,
                            WorkspaceActivities::CreatedAt,
                        )
                        .not_null(),
                    )
                    .to_owned(),
            )
            .await?;
        for index in [
            Index::create()
                .name("idx_workspace_activity_event")
                .table(WorkspaceActivities::Table)
                .col(WorkspaceActivities::EventId)
                .unique()
                .to_owned(),
            Index::create()
                .name("idx_workspace_activity_team")
                .table(WorkspaceActivities::Table)
                .col(WorkspaceActivities::TeamId)
                .col(WorkspaceActivities::Id)
                .to_owned(),
            Index::create()
                .name("idx_workspace_activity_owner")
                .table(WorkspaceActivities::Table)
                .col(WorkspaceActivities::OwnerUserId)
                .col(WorkspaceActivities::Id)
                .to_owned(),
            Index::create()
                .name("idx_workspace_activity_resource")
                .table(WorkspaceActivities::Table)
                .col(WorkspaceActivities::EntityType)
                .col(WorkspaceActivities::EntityId)
                .col(WorkspaceActivities::Id)
                .to_owned(),
            Index::create()
                .name("idx_workspace_activity_operation")
                .table(WorkspaceActivities::Table)
                .col(WorkspaceActivities::OperationId)
                .to_owned(),
            Index::create()
                .name("idx_workspace_activity_retention")
                .table(WorkspaceActivities::Table)
                .col(WorkspaceActivities::CreatedAt)
                .to_owned(),
        ] {
            manager.create_index(index).await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(WorkspaceActivities::Table).to_owned())
            .await?;
        for table in ["files", "folders"] {
            for column in ["modified_at", "modified_by_username", "modified_by_user_id"] {
                manager
                    .alter_table(
                        Table::alter()
                            .table(Alias::new(table))
                            .drop_column(Alias::new(column))
                            .to_owned(),
                    )
                    .await?;
            }
        }
        Ok(())
    }
}

#[derive(DeriveIden)]
enum WorkspaceActivities {
    Table,
    Id,
    EventId,
    OperationId,
    TeamId,
    OwnerUserId,
    EntityType,
    EntityId,
    EntityName,
    ActorUserId,
    ActorName,
    Action,
    Source,
    Details,
    CreatedAt,
}
