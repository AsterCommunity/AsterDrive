//! Migration support and storage policy availability checks.
use super::super::report::{DoctorCheck, DoctorStatus};
pub(in crate::cli::commands::doctor) async fn doctor_sqlite_search_check(
    db: &sea_orm::DatabaseConnection,
    pending_migrations: &[String],
) -> DoctorCheck {
    match crate::db::sqlite_search::inspect_sqlite_search_status(db).await {
        Ok(Some(status)) if status.is_ready() => DoctorCheck {
            name: "sqlite_search_acceleration",
            label: "SQLite search acceleration",
            status: DoctorStatus::Ok,
            summary: "FTS5 trigram search acceleration ready".to_string(),
            details: status.detail_lines(),
            suggestion: None,
        },
        Ok(Some(status))
            if pending_migrations
                .iter()
                .any(|name| {
                    crate::db::sqlite_search::SQLITE_SEARCH_MIGRATION_NAMES
                        .contains(&name.as_str())
                }) =>
        {
            DoctorCheck {
                name: "sqlite_search_acceleration",
                label: "SQLite search acceleration",
                status: DoctorStatus::Warn,
                summary: "SQLite search acceleration migration pending".to_string(),
                details: status.detail_lines(),
                suggestion: Some(
                    "Apply pending migrations on a SQLite build that includes FTS5 with the trigram tokenizer."
                        .to_string(),
                ),
            }
        }
        Ok(Some(status)) if !status.probe_supported() => DoctorCheck {
            name: "sqlite_search_acceleration",
            label: "SQLite search acceleration",
            status: DoctorStatus::Fail,
            summary: "SQLite build lacks FTS5 trigram search support".to_string(),
            details: status.detail_lines(),
            suggestion: Some(
                "Use a SQLite build with FTS5 + trigram tokenizer support, or switch the deployment to PostgreSQL / MySQL."
                    .to_string(),
            ),
        },
        Ok(Some(status)) => DoctorCheck {
            name: "sqlite_search_acceleration",
            label: "SQLite search acceleration",
            status: DoctorStatus::Fail,
            summary: "SQLite search acceleration objects are missing".to_string(),
            details: status.detail_lines(),
            suggestion: Some(
                "Apply the latest migrations and restore the files_name_fts / folders_name_fts / users_search_fts / teams_search_fts objects if they were removed manually."
                    .to_string(),
            ),
        },
        Ok(None) => DoctorCheck {
            name: "sqlite_search_acceleration",
            label: "SQLite search acceleration",
            status: DoctorStatus::Ok,
            summary: "not applicable".to_string(),
            details: Vec::new(),
            suggestion: None,
        },
        Err(err) => DoctorCheck {
            name: "sqlite_search_acceleration",
            label: "SQLite search acceleration",
            status: DoctorStatus::Fail,
            summary: "failed to verify SQLite search acceleration".to_string(),
            details: vec![err.message().to_string()],
            suggestion: Some(
                "Check SQLite metadata access, then rerun doctor to validate FTS5 trigram support and search objects."
                    .to_string(),
            ),
        },
    }
}

pub(in crate::cli::commands::doctor) async fn doctor_storage_policy_check(
    db: &sea_orm::DatabaseConnection,
) -> DoctorCheck {
    let policies = match crate::db::repository::policy_repo::find_all(db).await {
        Ok(policies) => policies,
        Err(err) => {
            return DoctorCheck {
                name: "storage_policies",
                label: "Storage policies",
                status: DoctorStatus::Fail,
                summary: "failed to load storage policies".to_string(),
                details: vec![err.message().to_string()],
                suggestion: Some(
                    "Ensure database migrations are complete and the storage_policies table is accessible."
                        .to_string(),
                ),
            };
        }
    };
    let groups = match crate::db::repository::policy_group_repo::find_all_groups(db).await {
        Ok(groups) => groups,
        Err(err) => {
            return DoctorCheck {
                name: "storage_policies",
                label: "Storage policies",
                status: DoctorStatus::Fail,
                summary: "failed to load storage policy groups".to_string(),
                details: vec![err.message().to_string()],
                suggestion: Some(
                    "Ensure database migrations are complete and the storage_policy_groups table is accessible."
                        .to_string(),
                ),
            };
        }
    };

    let snapshot = crate::storage::PolicySnapshot::new();
    let driver_registry = match crate::storage::DriverRegistry::noop() {
        Ok(registry) => registry,
        Err(err) => {
            return DoctorCheck {
                name: "storage_policies",
                label: "Storage policies",
                status: DoctorStatus::Fail,
                summary: "failed to initialize storage connector registry".to_string(),
                details: vec![err.message().to_string()],
                suggestion: Some(
                    "Check the built-in storage connector descriptor registry for duplicate or invalid contracts."
                        .to_string(),
                ),
            };
        }
    };
    if let Err(err) = driver_registry.reload_policy_snapshot(&snapshot, db).await {
        return DoctorCheck {
            name: "storage_policies",
            label: "Storage policies",
            status: DoctorStatus::Fail,
            summary: "failed to build storage policy snapshot".to_string(),
            details: vec![err.message().to_string()],
            suggestion: Some(
                "Check whether policies, policy groups, and user policy group assignments are consistent."
                    .to_string(),
            ),
        };
    }

    let default_policy = policies.iter().find(|policy| policy.is_default);
    let default_group = groups.iter().find(|group| group.is_default);
    let mut details = vec![
        format!("policies={}", policies.len()),
        format!("groups={}", groups.len()),
    ];
    let mut problems = Vec::new();

    if policies.is_empty() {
        problems.push("no storage policies found".to_string());
    }
    if let Some(policy) = default_policy {
        details.push(format!("default_policy={}", policy.name));
    } else {
        problems.push("no default storage policy found".to_string());
    }
    if let Some(group) = default_group {
        details.push(format!("default_group={}", group.name));
    } else {
        problems.push("no default storage policy group found".to_string());
    }
    if snapshot.system_default_policy().is_none() {
        problems.push("policy snapshot has no system default policy".to_string());
    }
    if snapshot.system_default_policy_group().is_none() {
        problems.push("policy snapshot has no system default group".to_string());
    }

    if problems.is_empty() {
        DoctorCheck {
            name: "storage_policies",
            label: "Storage policies",
            status: DoctorStatus::Ok,
            summary: "storage policy defaults are ready".to_string(),
            details,
            suggestion: None,
        }
    } else {
        details.extend(problems);
        DoctorCheck {
            name: "storage_policies",
            label: "Storage policies",
            status: DoctorStatus::Fail,
            summary: "storage policy setup is incomplete".to_string(),
            details,
            suggestion: Some(
                "Start the server once or seed the default storage policy and policy group data manually."
                    .to_string(),
            ),
        }
    }
}
