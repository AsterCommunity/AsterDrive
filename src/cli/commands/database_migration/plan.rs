//! Product table selection, copy ordering and migration preflight plans.
use super::{
    helpers::count_rows,
    model::{TablePlan, TableReport},
    schema::{load_column_type_rows, load_table_plan, source_table_names},
};
use crate::cli::common::database::{quote_ident, scalar_i64};
use crate::db;
use crate::errors::{AsterError, Result};
use sea_orm::{DatabaseConnection, DbBackend};
use std::collections::BTreeSet;
pub(super) const COPY_TABLE_ORDER: &[&str] = &[
    "managed_followers",
    "remote_tunnel_owners",
    "storage_policies",
    "storage_policy_connector_credentials",
    "storage_policy_groups",
    "storage_policy_group_rules",
    "storage_policy_group_rule_targets",
    "storage_policy_group_items",
    "follower_enrollment_sessions",
    "users",
    "storage_policy_authorization_flows",
    "user_profiles",
    "user_invitations",
    "auth_sessions",
    "admin_content_accesses",
    "passkeys",
    "mfa_factors",
    "mfa_totp_used_steps",
    "mfa_recovery_codes",
    "mfa_login_flows",
    "mfa_email_codes",
    "mfa_totp_setup_flows",
    "teams",
    "team_members",
    "tags",
    "folders",
    "webdav_accounts",
    "file_blobs",
    "blob_media_metadata",
    "files",
    "file_create_idempotencies",
    "file_revision_histories",
    "file_revisions",
    "file_revision_properties",
    "shares",
    "upload_sessions",
    "upload_session_parts",
    "contact_verification_tokens",
    "external_auth_providers",
    "external_auth_identities",
    "external_auth_login_flows",
    "external_auth_email_verification_flows",
    "master_bindings",
    "remote_storage_targets",
    "remote_storage_target_credentials",
    "system_config",
    "audit_logs",
    "workspace_activities",
    "mail_outbox",
    "background_tasks",
    "task_resource_publications",
    "folder_tree_operation_members",
    aster_forge_db::RUNTIME_LEASES_TABLE,
    aster_forge_db::SCHEDULED_TASKS_TABLE,
    "storage_migration_checkpoints",
    "entity_properties",
    "resource_lock_namespaces",
    "resource_locks",
    "wopi_sessions",
];

pub(super) const MIGRATION_TABLE: &str = "seaql_migrations";
pub(super) const CHECKPOINT_TABLE: &str = "aster_cli_database_migrations";
pub(super) const DEFAULT_COPY_BATCH_SIZE: i64 = 200;
pub(super) const PROGRESS_ENV: &str = "ASTER_CLI_PROGRESS";
pub(super) const COPY_BATCH_SIZE_ENV: &str = "ASTER_CLI_COPY_BATCH_SIZE";
pub(super) const FAIL_AFTER_BATCHES_ENV: &str = "ASTER_CLI_FAIL_AFTER_BATCHES";

/// Loads the ordered source table plans that drive copy and verification stages.
pub(super) async fn load_source_plans(source: &DatabaseConnection) -> Result<Vec<TablePlan>> {
    let backend = source.get_database_backend();
    let existing_tables = source_table_names(source, backend).await?;
    validate_source_tables(backend, &existing_tables)?;
    validate_legacy_storage_source(source, backend, &existing_tables).await?;

    let existing_lookup: BTreeSet<&str> = existing_tables.iter().map(String::as_str).collect();
    let mut plans = Vec::with_capacity(COPY_TABLE_ORDER.len());
    for table in COPY_TABLE_ORDER {
        if !existing_lookup.contains(*table) {
            return Err(AsterError::validation_error(format!(
                "source database is missing expected table '{}'",
                table
            )));
        }
        let mut plan = load_table_plan(source, backend, table).await?;
        if *table == "storage_policies" {
            plan.columns.retain(|column| {
                !matches!(
                    column.name.as_str(),
                    "driver_type"
                        | "endpoint"
                        | "bucket"
                        | "access_key"
                        | "secret_key"
                        | "base_path"
                        | "remote_node_id"
                        | "remote_storage_target_key"
                        | "options"
                )
            });
        }
        plans.push(plan);
    }
    Ok(plans)
}

pub(super) fn plans_to_reports(plans: &[TablePlan]) -> Vec<TableReport> {
    plans
        .iter()
        .map(|plan| TableReport {
            name: plan.name.clone(),
            primary_key: plan.primary_key.clone(),
            source_rows: plan.source_rows,
            target_rows: 0,
            copied_rows: 0,
            sequence_reset: plan.sequence_reset,
        })
        .collect()
}

pub(super) fn total_source_rows(plans: &[TablePlan]) -> i64 {
    plans.iter().map(|plan| plan.source_rows).sum()
}

fn validate_source_tables(backend: DbBackend, existing_tables: &[String]) -> Result<()> {
    let known: BTreeSet<&str> = COPY_TABLE_ORDER
        .iter()
        .copied()
        .chain([MIGRATION_TABLE, CHECKPOINT_TABLE])
        .collect();
    let unexpected: Vec<String> = existing_tables
        .iter()
        .filter(|table| {
            !(known.contains(table.as_str())
                || backend == DbBackend::Sqlite && db::sqlite_search::is_sqlite_search_table(table))
        })
        .cloned()
        .collect();

    if !unexpected.is_empty() {
        return Err(AsterError::validation_error(format!(
            "source database contains unsupported tables that would not be migrated: {}",
            unexpected.join(", ")
        )));
    }

    Ok(())
}

async fn validate_legacy_storage_source(
    source: &DatabaseConnection,
    backend: DbBackend,
    existing_tables: &[String],
) -> Result<()> {
    for table in [
        "storage_policy_credentials",
        "storage_connector_application_configs",
    ] {
        if existing_tables.iter().any(|name| name == table) {
            let rows = count_rows(source, backend, table).await?;
            if rows > 0 {
                return Err(AsterError::validation_error(format!(
                    "source database contains {rows} unmigrated rows in removed legacy table '{table}'; start it successfully on AsterDrive 0.5.x before copying"
                )));
            }
        }
    }

    if !existing_tables
        .iter()
        .any(|name| name == "storage_policies")
    {
        return Ok(());
    }
    let columns = load_column_type_rows(source, backend, "storage_policies").await?;
    let static_columns = columns
        .iter()
        .map(|(name, _)| name.as_str())
        .filter(|name| matches!(*name, "access_key" | "secret_key"))
        .collect::<Vec<_>>();
    if static_columns.is_empty() {
        return Ok(());
    }
    let predicates = static_columns
        .iter()
        .map(|column| format!("TRIM(COALESCE({}, '')) <> ''", quote_ident(backend, column)))
        .collect::<Vec<_>>()
        .join(" OR ");
    let count = scalar_i64(
        source,
        backend,
        &format!(
            "SELECT COUNT(*) FROM {} WHERE {predicates}",
            quote_ident(backend, "storage_policies")
        ),
    )
    .await?;
    if count > 0 {
        return Err(AsterError::validation_error(format!(
            "source database contains {count} unmigrated static storage credential row(s); start it successfully on AsterDrive 0.5.x before copying"
        )));
    }
    Ok(())
}
