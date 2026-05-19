//! `doctor` 命令的执行编排。
//!
//! 这里把数据库连接、基础检查、运行时配置检查和深度审计串成一条执行链，
//! 最终汇总成统一的 `DoctorReport`。

use sea_orm::{DatabaseConnection, DbBackend};

use crate::cli::db_shared::{backend_name, pending_migrations, redact_database_url};

use super::{
    DoctorArgs, DoctorCheck, DoctorDeepScope, DoctorReport, DoctorStatus,
    doctor_blob_ref_count_check, doctor_check, doctor_folder_tree_check, doctor_mail_check,
    doctor_preview_apps_check, doctor_public_site_url_check, doctor_scope_enabled,
    doctor_sqlite_search_check, doctor_storage_policy_check, doctor_storage_scan_checks,
    doctor_storage_usage_check, effective_deep_scopes,
};

struct DoctorMigrationInspection {
    pending: Vec<String>,
}

/// Executes the full doctor flow and assembles the final report payload.
pub(super) async fn execute_doctor_command_impl(args: &DoctorArgs) -> DoctorReport {
    let redacted_database_url = redact_database_url(&args.database_url);
    // `--fix`、`--scope`、`--policy-id` 都意味着用户已经在请求深度审计；
    // 不要求再额外显式带 `--deep`，避免 CLI 使用上出现重复开关。
    let deep = args.deep || args.fix || !args.scopes.is_empty() || args.policy_id.is_some();
    let scopes = if deep {
        effective_deep_scopes(args)
    } else {
        Vec::new()
    };
    let mut backend = None;
    let mut checks = Vec::new();

    let Some((db, db_backend)) =
        connect_doctor_database(args, &redacted_database_url, &mut backend, &mut checks).await
    else {
        return DoctorReport::new(args, redacted_database_url, backend, deep, scopes, checks);
    };

    let migration_inspection = inspect_doctor_migrations(&db, &mut checks).await;

    if db_backend == DbBackend::Sqlite {
        checks.push(
            sqlite_search_check(
                &db,
                migration_inspection
                    .as_ref()
                    .map(|inspection| inspection.pending.as_slice()),
            )
            .await,
        );
    }

    if let Some(runtime_config) = load_runtime_config_checks(&db, &mut checks).await {
        checks.push(doctor_public_site_url_check(&runtime_config));
        checks.push(doctor_mail_check(&runtime_config));
        checks.push(doctor_preview_apps_check(&runtime_config));
    }

    checks.push(doctor_storage_policy_check(&db).await);

    let (effective_policy_id, policy_filter_valid) =
        resolve_policy_filter(&db, args, deep, &mut checks).await;

    push_deep_checks(
        &db,
        args,
        &scopes,
        effective_policy_id,
        policy_filter_valid,
        &mut checks,
    )
    .await;

    DoctorReport::new(args, redacted_database_url, backend, deep, scopes, checks)
}

async fn connect_doctor_database(
    args: &DoctorArgs,
    redacted_database_url: &str,
    backend: &mut Option<String>,
    checks: &mut Vec<DoctorCheck>,
) -> Option<(DatabaseConnection, DbBackend)> {
    match super::connect_database(&args.database_url).await {
        Ok(db) => {
            let db_backend = db.get_database_backend();
            let db_backend_name = backend_name(db_backend).to_string();
            *backend = Some(db_backend_name.clone());
            checks.push(doctor_check(
                "database_connection",
                "Database connection",
                DoctorStatus::Ok,
                format!("connected to {db_backend_name}"),
                vec![format!("database_url={redacted_database_url}")],
                None,
            ));
            Some((db, db_backend))
        }
        Err(err) => {
            checks.push(doctor_check(
                "database_connection",
                "Database connection",
                DoctorStatus::Fail,
                "database connection failed",
                vec![err.message().to_string()],
                Some(
                    "Check --database-url, database availability, and access permissions."
                        .to_string(),
                ),
            ));
            None
        }
    }
}

async fn inspect_doctor_migrations(
    db: &DatabaseConnection,
    checks: &mut Vec<DoctorCheck>,
) -> Option<DoctorMigrationInspection> {
    let history = match migration::inspect_migration_history(db).await {
        Ok(history) => history,
        Err(err) => {
            checks.push(doctor_check(
                "database_migrations",
                "Database migrations",
                DoctorStatus::Fail,
                "failed to inspect migration history",
                vec![err.to_string()],
                Some(
                    "Check the seaql_migrations table and database permissions to ensure migration metadata is readable."
                        .to_string(),
                ),
            ));
            return None;
        }
    };

    if history.track == migration::MigrationTrack::Unknown {
        let details = if !history.unknown_applied.is_empty() {
            history.unknown_applied.clone()
        } else if history.applied.is_empty() {
            vec!["empty migration history with existing schema objects".to_string()]
        } else {
            vec!["non-prefix migration history".to_string()]
        };
        checks.push(doctor_check(
            "database_migrations",
            "Database migrations",
            DoctorStatus::Fail,
            "database contains unknown migration versions",
            details,
            Some(
                "Compare the database with the current migration baseline before running maintenance-oriented CLI commands."
                    .to_string(),
            ),
        ));
        return None;
    }

    match pending_migrations(db).await {
        Ok(pending) => {
            checks.push(if pending.is_empty() {
                doctor_check(
                    "database_migrations",
                    "Database migrations",
                    DoctorStatus::Ok,
                    "no pending migrations",
                    vec![format!("history_mode={}", history.track.label())],
                    None,
                )
            } else {
                let mut details = vec![format!("history_mode={}", history.track.label())];
                details.extend(pending.clone());
                doctor_check(
                    "database_migrations",
                    "Database migrations",
                    DoctorStatus::Warn,
                    format!("{} pending migration(s)", pending.len()),
                    details,
                    Some(
                        "Apply pending migrations before running maintenance-oriented CLI commands."
                            .to_string(),
                    ),
                )
            });
            Some(DoctorMigrationInspection { pending })
        }
        Err(err) => {
            checks.push(doctor_check(
                "database_migrations",
                "Database migrations",
                DoctorStatus::Fail,
                "failed to inspect migration history",
                vec![err.message().to_string()],
                Some(
                    "Check the seaql_migrations table and database permissions to ensure migration metadata is readable."
                        .to_string(),
                ),
            ));
            None
        }
    }
}

async fn sqlite_search_check(
    db: &DatabaseConnection,
    pending_migrations: Option<&[String]>,
) -> DoctorCheck {
    match pending_migrations {
        Some(pending) => doctor_sqlite_search_check(db, pending).await,
        None => doctor_check(
            "sqlite_search_acceleration",
            "SQLite search acceleration",
            DoctorStatus::Fail,
            "failed to verify SQLite search acceleration",
            vec!["migration status is unavailable".to_string()],
            Some(
                "Fix migration metadata access first, then rerun doctor to validate SQLite FTS5 trigram support."
                    .to_string(),
            ),
        ),
    }
}

async fn load_runtime_config_checks(
    db: &DatabaseConnection,
    checks: &mut Vec<DoctorCheck>,
) -> Option<crate::config::RuntimeConfig> {
    let runtime_config = crate::config::RuntimeConfig::new();
    match runtime_config.reload(db).await {
        Ok(()) => {
            checks.push(doctor_check(
                "runtime_config",
                "Runtime configuration",
                DoctorStatus::Ok,
                "runtime config snapshot loaded",
                Vec::new(),
                None,
            ));
            Some(runtime_config)
        }
        Err(err) => {
            checks.push(doctor_check(
                "runtime_config",
                "Runtime configuration",
                DoctorStatus::Fail,
                "failed to load runtime config snapshot",
                vec![err.message().to_string()],
                Some(
                    "Check whether the system_config schema and stored values are complete."
                        .to_string(),
                ),
            ));
            None
        }
    }
}

async fn resolve_policy_filter(
    db: &DatabaseConnection,
    args: &DoctorArgs,
    deep: bool,
    checks: &mut Vec<DoctorCheck>,
) -> (Option<i64>, bool) {
    let mut effective_policy_id = args.policy_id;
    let mut policy_filter_valid = true;

    if deep && let Some(policy_id) = args.policy_id {
        match crate::db::repository::policy_repo::find_by_id(db, policy_id).await {
            Ok(policy) => checks.push(doctor_check(
                "policy_filter",
                "Policy filter",
                DoctorStatus::Ok,
                format!("scoped to storage policy #{} ({})", policy.id, policy.name),
                Vec::new(),
                None,
            )),
            Err(err) => {
                checks.push(doctor_check(
                    "policy_filter",
                    "Policy filter",
                    DoctorStatus::Fail,
                    format!("storage policy #{} does not exist", policy_id),
                    vec![err.message().to_string()],
                    Some("Use a valid --policy-id or remove the filter.".to_string()),
                ));
                effective_policy_id = None;
                policy_filter_valid = false;
            }
        }
    }

    (effective_policy_id, policy_filter_valid)
}

async fn push_deep_checks(
    db: &DatabaseConnection,
    args: &DoctorArgs,
    scopes: &[DoctorDeepScope],
    effective_policy_id: Option<i64>,
    policy_filter_valid: bool,
    checks: &mut Vec<DoctorCheck>,
) {
    if !args.deep && !args.fix && args.scopes.is_empty() && args.policy_id.is_none() {
        return;
    }

    if doctor_scope_enabled(scopes, DoctorDeepScope::StorageUsage) {
        checks.push(match doctor_storage_usage_check(db, args.fix).await {
            Ok(check) => check,
            Err(err) => doctor_check(
                "storage_usage_consistency",
                "Storage usage counters",
                DoctorStatus::Fail,
                "failed to audit storage usage counters",
                vec![err.message().to_string()],
                Some(
                    "Check whether the users, teams, files, and file_versions tables are complete and readable."
                        .to_string(),
                ),
            ),
        });
    }

    if doctor_scope_enabled(scopes, DoctorDeepScope::BlobRefCounts) && policy_filter_valid {
        checks.push(match doctor_blob_ref_count_check(db, args.fix, effective_policy_id).await {
            Ok(check) => check,
            Err(err) => doctor_check(
                "blob_ref_counts",
                "Blob reference counters",
                DoctorStatus::Fail,
                "failed to audit blob reference counters",
                vec![err.message().to_string()],
                Some(
                    "Check whether the file_blobs, files, and file_versions tables are complete and readable."
                        .to_string(),
                ),
            ),
        });
    }

    if doctor_scope_enabled(scopes, DoctorDeepScope::StorageObjects) && policy_filter_valid {
        // storage object 扫描会真实触达底层驱动，和前面的纯数据库检查分开汇总，
        // 这样运维侧能一眼区分“库里坏了”还是“对象存储不可达/有漂移”。
        match doctor_storage_scan_checks(db, effective_policy_id).await {
            Ok(storage_checks) => checks.extend(storage_checks),
            Err(err) => checks.extend([
                doctor_check(
                    "tracked_blob_objects",
                    "Tracked blob objects",
                    DoctorStatus::Fail,
                    "failed to scan storage objects",
                    vec![err.message().to_string()],
                    Some(
                        "Check storage policy configuration, driver permissions, and object storage connectivity."
                            .to_string(),
                    ),
                ),
                doctor_check(
                    "untracked_storage_objects",
                    "Untracked storage objects",
                    DoctorStatus::Fail,
                    "failed to scan storage objects",
                    vec![err.message().to_string()],
                    Some(
                        "Check storage policy configuration, driver permissions, and object storage connectivity."
                            .to_string(),
                    ),
                ),
                doctor_check(
                    "thumbnail_objects",
                    "Thumbnail objects",
                    DoctorStatus::Fail,
                    "failed to scan storage objects",
                    vec![err.message().to_string()],
                    Some(
                        "Check storage policy configuration, driver permissions, and object storage connectivity."
                            .to_string(),
                    ),
                ),
            ]),
        }
    }

    if doctor_scope_enabled(scopes, DoctorDeepScope::FolderTree) {
        checks.push(match doctor_folder_tree_check(db).await {
            Ok(check) => check,
            Err(err) => doctor_check(
                "folder_tree_integrity",
                "Folder tree integrity",
                DoctorStatus::Fail,
                "failed to audit folder tree",
                vec![err.message().to_string()],
                Some(
                    "Check whether the folders table is complete and whether parent_id relationships are readable."
                        .to_string(),
                ),
            ),
        });
    }
}
