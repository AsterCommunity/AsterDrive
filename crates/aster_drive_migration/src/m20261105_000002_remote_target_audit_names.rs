//! Normalize the retired ingress audit terminology, including configured action selection.
//! Only identifiers change; historical details and actor/target snapshots remain untouched.
use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::{ConnectionTrait, TransactionTrait};

#[derive(DeriveMigrationName)]
pub struct Migration;

const ACTION_NAMES: &[(&str, &str)] = &[
    (
        "admin_create_remote_ingress_profile",
        "admin_create_remote_storage_target",
    ),
    (
        "admin_update_remote_ingress_profile",
        "admin_update_remote_storage_target",
    ),
    (
        "admin_delete_remote_ingress_profile",
        "admin_delete_remote_storage_target",
    ),
    (
        "follower_ingress_profile_create",
        "follower_storage_target_create",
    ),
    (
        "follower_ingress_profile_update",
        "follower_storage_target_update",
    ),
    (
        "follower_ingress_profile_delete",
        "follower_storage_target_delete",
    ),
];
const ENTITY_NAMES: (&str, &str) = ("remote_ingress_profile", "remote_storage_target");
const ACTIONS_CONFIG_KEY: &str = "audit_log_recorded_actions";

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    /// Migrate historical audit identifiers and action selection in one transaction.
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        rewrite_names(manager, true).await
    }

    /// Reverse the exact audit identifier mapping while preserving historical payloads.
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        rewrite_names(manager, false).await
    }
}

/// Choose the exact historical-to-current identifier mapping for upgrade or rollback.
fn directed_names(
    names: (&'static str, &'static str),
    upgrade: bool,
) -> (&'static str, &'static str) {
    if upgrade { names } else { (names.1, names.0) }
}

/// Atomically normalize audit identifiers and the configured action selection.
/// Leave historical payloads and actor/target snapshots untouched; reject invalid selection JSON.
async fn rewrite_names(manager: &SchemaManager<'_>, upgrade: bool) -> Result<(), DbErr> {
    // A nested transaction is a savepoint under the product migration lock;
    // MySQL also gets an explicit DML transaction, so config/log changes agree.
    let transaction = manager.get_connection().begin().await?;
    let config_rows = transaction
        .query_all(
            &Query::select()
                .columns([Alias::new("id"), Alias::new("value")])
                .from(Alias::new("system_config"))
                .and_where(Expr::col(Alias::new("key")).eq(ACTIONS_CONFIG_KEY))
                .to_owned(),
        )
        .await?;
    for row in config_rows {
        let id: i64 = row.try_get_by_index(0)?;
        let raw: String = row.try_get_by_index(1)?;
        let mut actions: Vec<String> = serde_json::from_str(&raw).map_err(|_| DbErr::Migration(
            "cannot migrate audit_log_recorded_actions: expected a JSON array of strings; repair it on the previous release before upgrading".into()))?;
        let mut changed = false;
        for action in &mut actions {
            for names in ACTION_NAMES {
                let (from, to) = directed_names(*names, upgrade);
                if action == from {
                    *action = to.into();
                    changed = true;
                    break;
                }
            }
        }
        if changed {
            // If a partially updated configuration already contains both names,
            // retain exactly one selection for the same logical action.
            let mut seen = std::collections::HashSet::new();
            actions.retain(|action| seen.insert(action.clone()));
            // String arrays have an infallible JSON representation; build that
            // representation directly instead of inventing an unreachable failure.
            let value = serde_json::Value::Array(
                actions.into_iter().map(serde_json::Value::String).collect(),
            )
            .to_string();
            transaction
                .execute(
                    &Query::update()
                        .table(Alias::new("system_config"))
                        .value(Alias::new("value"), value)
                        .and_where(Expr::col(Alias::new("id")).eq(id))
                        .to_owned(),
                )
                .await?;
        }
    }
    for names in ACTION_NAMES {
        let (from, to) = directed_names(*names, upgrade);
        transaction
            .execute(
                &Query::update()
                    .table(Alias::new("audit_logs"))
                    .value(Alias::new("action"), to)
                    .and_where(Expr::col(Alias::new("action")).eq(from))
                    .to_owned(),
            )
            .await?;
    }
    let (from, to) = directed_names(ENTITY_NAMES, upgrade);
    transaction
        .execute(
            &Query::update()
                .table(Alias::new("audit_logs"))
                .value(Alias::new("entity_type"), to)
                .and_where(Expr::col(Alias::new("entity_type")).eq(from))
                .to_owned(),
        )
        .await?;
    transaction.commit().await
}
