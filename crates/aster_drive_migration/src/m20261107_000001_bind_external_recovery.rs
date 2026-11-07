//! Browser-bound external recovery and pending identity association through MFA. Existing
//! unbound recovery rows remain NULL and are rejected rather than silently gaining authority.
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

const BINDING_COLUMNS: [(&str, &str); 3] = [
    (
        "external_auth_email_verification_flows",
        "browser_binding_hash",
    ),
    ("mfa_login_flows", "external_recovery_flow_id"),
    ("mfa_login_flows", "browser_binding_hash"),
];

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // MySQL can commit each ALTER independently. Inspect each column so retry after a
        // partial DDL failure can finish without rewriting rows or fabricating browser proofs.
        // Separate statements also work with SQLite's one-column ALTER restriction.
        for (table, column) in BINDING_COLUMNS {
            if manager.has_column(table, column).await? {
                continue;
            }
            let mut definition = ColumnDef::new(Alias::new(column));
            match column {
                "external_recovery_flow_id" => {
                    definition.big_integer();
                }
                _ => {
                    definition.string_len(64);
                }
            }
            manager
                .alter_table(
                    Table::alter()
                        .table(Alias::new(table))
                        .add_column(definition.null())
                        .to_owned(),
                )
                .await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for (table, column) in BINDING_COLUMNS.into_iter().rev() {
            if manager.has_column(table, column).await? {
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
