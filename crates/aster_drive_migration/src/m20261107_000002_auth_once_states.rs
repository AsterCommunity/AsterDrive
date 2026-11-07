//! Authentication challenges and signed-request replay reservations share writer authority.
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let mut payload = ColumnDef::new(AuthOnceStates::Payload);
        match manager.get_database_backend() {
            sea_orm_migration::sea_orm::DbBackend::MySql => {
                payload.custom(Alias::new("MEDIUMTEXT"))
            }
            _ => payload.text(),
        };
        payload.not_null();
        manager
            .create_table(
                Table::create()
                    .table(AuthOnceStates::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(AuthOnceStates::KeyHash)
                            .string_len(64)
                            .not_null()
                            .primary_key(),
                    )
                    .col(&mut payload)
                    .col(
                        aster_forge_db_migration::utc_date_time_column(
                            manager,
                            AuthOnceStates::ExpiresAt,
                        )
                        .not_null(),
                    )
                    .to_owned(),
            )
            .await?;
        if !manager
            .has_index("auth_once_states", "idx_auth_once_states_expires_at")
            .await?
        {
            manager
                .create_index(
                    Index::create()
                        .name("idx_auth_once_states_expires_at")
                        .table(AuthOnceStates::Table)
                        .col(AuthOnceStates::ExpiresAt)
                        .to_owned(),
                )
                .await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(AuthOnceStates::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum AuthOnceStates {
    Table,
    KeyHash,
    Payload,
    ExpiresAt,
}
