//! `aster_drive doctor` 的聚合入口。
//!
//! 这里负责把环境检查、运行时配置检查和深度一致性审计组合成一份结构化报告；
//! 真正的全局数据核对逻辑则下沉在 `ops::integrity`。

mod execute;
mod storage_scan;

use crate::errors::Result;
use crate::services::ops::integrity;
use clap::{Args, ValueEnum};
use serde::Serialize;

use super::shared::{
    CliTerminalPalette, OutputFormat, ResolvedOutputFormat, connect_database, human_key,
    render_success_envelope,
};

#[derive(Debug, Clone, Args)]
pub struct DoctorArgs {
    #[arg(long, env = "ASTER_CLI_DATABASE_URL")]
    pub database_url: String,
    #[arg(long, env = "ASTER_CLI_DOCTOR_STRICT", default_value_t = false)]
    pub strict: bool,
    #[arg(long, env = "ASTER_CLI_DOCTOR_DEEP", default_value_t = false)]
    pub deep: bool,
    #[arg(long, env = "ASTER_CLI_DOCTOR_FIX", default_value_t = false)]
    pub fix: bool,
    #[arg(
        long = "scope",
        env = "ASTER_CLI_DOCTOR_SCOPE",
        value_enum,
        value_delimiter = ','
    )]
    pub scopes: Vec<DoctorDeepScope>,
    #[arg(long, env = "ASTER_CLI_DOCTOR_POLICY_ID")]
    pub policy_id: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum DoctorDeepScope {
    StorageUsage,
    BlobRefCounts,
    StorageObjects,
    FolderTree,
}

impl DoctorDeepScope {
    fn label(self) -> &'static str {
        match self {
            Self::StorageUsage => "storage_usage",
            Self::BlobRefCounts => "blob_ref_counts",
            Self::StorageObjects => "storage_objects",
            Self::FolderTree => "folder_tree",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DoctorStatus {
    Ok,
    Warn,
    Fail,
}

impl DoctorStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Warn => "warn",
            Self::Fail => "fail",
        }
    }
}

#[derive(Debug, Serialize)]
pub struct DoctorSummary {
    total: usize,
    ok: usize,
    warn: usize,
    fail: usize,
}

#[derive(Debug, Serialize)]
pub struct DoctorCheck {
    name: &'static str,
    label: &'static str,
    status: DoctorStatus,
    summary: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    details: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    suggestion: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DoctorReport {
    strict: bool,
    deep: bool,
    fix: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    scopes: Vec<DoctorDeepScope>,
    #[serde(skip_serializing_if = "Option::is_none")]
    policy_id: Option<i64>,
    status: DoctorStatus,
    database_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    backend: Option<String>,
    summary: DoctorSummary,
    checks: Vec<DoctorCheck>,
}

impl DoctorReport {
    fn new(
        args: &DoctorArgs,
        database_url: String,
        backend: Option<String>,
        deep: bool,
        scopes: Vec<DoctorDeepScope>,
        checks: Vec<DoctorCheck>,
    ) -> Self {
        let mut ok = 0;
        let mut warn = 0;
        let mut fail = 0;
        for check in &checks {
            match check.status {
                DoctorStatus::Ok => ok += 1,
                DoctorStatus::Warn => warn += 1,
                DoctorStatus::Fail => fail += 1,
            }
        }

        let status = if fail > 0 || (args.strict && warn > 0) {
            DoctorStatus::Fail
        } else if warn > 0 {
            DoctorStatus::Warn
        } else {
            DoctorStatus::Ok
        };

        Self {
            strict: args.strict,
            deep,
            fix: args.fix,
            scopes,
            policy_id: args.policy_id,
            status,
            database_url,
            backend,
            summary: DoctorSummary {
                total: checks.len(),
                ok,
                warn,
                fail,
            },
            checks,
        }
    }

    pub fn should_exit_nonzero(&self) -> bool {
        self.status == DoctorStatus::Fail
    }
}

fn effective_deep_scopes(args: &DoctorArgs) -> Vec<DoctorDeepScope> {
    if args.scopes.is_empty() {
        return vec![
            DoctorDeepScope::StorageUsage,
            DoctorDeepScope::BlobRefCounts,
            DoctorDeepScope::StorageObjects,
            DoctorDeepScope::FolderTree,
        ];
    }

    let mut deduped = Vec::new();
    for scope in &args.scopes {
        if !deduped.contains(scope) {
            deduped.push(*scope);
        }
    }
    deduped
}

fn doctor_scope_enabled(scopes: &[DoctorDeepScope], target: DoctorDeepScope) -> bool {
    scopes.contains(&target)
}

/// Executes the `doctor` command and returns a structured report for rendering.
pub async fn execute_doctor_command(args: &DoctorArgs) -> DoctorReport {
    execute::execute_doctor_command_impl(args).await
}

fn doctor_check(
    name: &'static str,
    label: &'static str,
    status: DoctorStatus,
    summary: impl Into<String>,
    details: Vec<String>,
    suggestion: Option<String>,
) -> DoctorCheck {
    DoctorCheck {
        name,
        label,
        status,
        summary: summary.into(),
        details,
        suggestion,
    }
}

async fn doctor_sqlite_search_check(
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

/// Renders a successful doctor report in the requested output format.
pub fn render_doctor_success(format: OutputFormat, report: &DoctorReport) -> String {
    match format.resolve() {
        ResolvedOutputFormat::Json => render_success_envelope(report, false),
        ResolvedOutputFormat::PrettyJson => render_success_envelope(report, true),
        ResolvedOutputFormat::Human => render_doctor_human(report),
    }
}

fn render_doctor_human(report: &DoctorReport) -> String {
    let palette = CliTerminalPalette::stdout();
    let mut lines = vec![
        palette.title("System doctor"),
        palette.dim("--------------------------------------------------"),
        format!(
            "{} {}",
            human_key("Database", &palette),
            report.database_url
        ),
        format!(
            "{} {}",
            human_key("Backend", &palette),
            report.backend.as_deref().unwrap_or("unknown")
        ),
        format!(
            "{} {}",
            human_key("Mode", &palette),
            doctor_mode_label(report)
        ),
        format!(
            "{} {}",
            human_key("Scope", &palette),
            if report.deep {
                doctor_scope_label(report)
            } else {
                "default".to_string()
            }
        ),
        format!(
            "{} {} {}",
            human_key("Status", &palette),
            palette.status_badge(report.status.as_str()),
            doctor_status_label(report.status)
        ),
        format!(
            "{} {} total, {} ok, {} warn, {} fail",
            human_key("Checks", &palette),
            report.summary.total,
            report.summary.ok,
            report.summary.warn,
            report.summary.fail
        ),
    ];

    if report.checks.is_empty() {
        lines.push(String::new());
        lines.push(palette.dim("No checks were executed."));
        return lines.join("\n");
    }

    lines.push(String::new());
    lines.push(palette.label("Checks:"));
    for check in &report.checks {
        lines.push(format!(
            "  {} {}",
            palette.status_badge(check.status.as_str()),
            check.label
        ));
        lines.push(format!("    {}", check.summary));
        for detail in &check.details {
            lines.push(format!("    {}", palette.dim(detail)));
        }
        if let Some(suggestion) = &check.suggestion {
            lines.push(format!(
                "    {} {}",
                palette.label("hint:"),
                palette.accent(suggestion)
            ));
        }
    }

    lines.join("\n")
}

fn doctor_mode_label(report: &DoctorReport) -> String {
    let mut parts = Vec::new();
    parts.push(if report.strict { "strict" } else { "standard" });
    if report.deep {
        parts.push("deep");
    }
    if report.fix {
        parts.push("fix");
    }
    parts.join(" + ")
}

fn doctor_scope_label(report: &DoctorReport) -> String {
    let mut label = if report.scopes.is_empty() {
        "default".to_string()
    } else {
        report
            .scopes
            .iter()
            .map(|scope| scope.label())
            .collect::<Vec<_>>()
            .join(", ")
    };
    if let Some(policy_id) = report.policy_id {
        label.push_str(&format!(" | policy_id={policy_id}"));
    }
    label
}

async fn doctor_storage_usage_check(
    db: &sea_orm::DatabaseConnection,
    fix: bool,
) -> Result<DoctorCheck> {
    let mut drifts = integrity::audit_storage_usage(db).await?;
    let detected = drifts.len();
    let mut fixed = 0usize;

    if fix && !drifts.is_empty() {
        integrity::fix_storage_usage_drifts(db, &drifts).await?;
        fixed = drifts.len();
        drifts = integrity::audit_storage_usage(db).await?;
    }

    if drifts.is_empty() {
        let summary = if fixed > 0 {
            format!("fixed {fixed} storage usage mismatch(es)")
        } else {
            "storage usage counters match logical file sizes".to_string()
        };
        let mut details = Vec::new();
        if detected > 0 {
            details.push(format!("detected_before_fix={detected}"));
        }
        return Ok(DoctorCheck {
            name: "storage_usage_consistency",
            label: "Storage usage counters",
            status: DoctorStatus::Ok,
            summary,
            details,
            suggestion: None,
        });
    }

    let details = drifts
        .into_iter()
        .map(|drift| {
            format!(
                "{}#{} recorded={} actual={} delta={}",
                match drift.owner_kind {
                    integrity::StorageOwnerKind::User => "user",
                    integrity::StorageOwnerKind::Team => "team",
                },
                drift.owner_id,
                drift.recorded_bytes,
                drift.actual_bytes,
                drift.delta_bytes
            )
        })
        .collect();

    Ok(DoctorCheck {
        name: "storage_usage_consistency",
        label: "Storage usage counters",
        status: DoctorStatus::Warn,
        summary: format!("{} storage usage mismatch(es)", detected.max(fixed)),
        details,
        suggestion: Some(
            "Run doctor --deep --fix to write back users.storage_used and teams.storage_used."
                .to_string(),
        ),
    })
}

async fn doctor_blob_ref_count_check(
    db: &sea_orm::DatabaseConnection,
    fix: bool,
    policy_id: Option<i64>,
) -> Result<DoctorCheck> {
    let mut drifts = integrity::audit_blob_ref_counts(db, policy_id).await?;
    let detected = drifts.len();
    let mut fixed = 0usize;

    if fix && !drifts.is_empty() {
        integrity::fix_blob_ref_count_drifts(db, &drifts).await?;
        fixed = drifts.len();
        drifts = integrity::audit_blob_ref_counts(db, policy_id).await?;
    }

    if drifts.is_empty() {
        let summary = if fixed > 0 {
            format!("fixed {fixed} blob ref_count mismatch(es)")
        } else {
            if let Some(policy_id) = policy_id {
                format!("blob ref_count values match file references for policy #{policy_id}")
            } else {
                "blob ref_count values match file references".to_string()
            }
        };
        let mut details = Vec::new();
        if detected > 0 {
            details.push(format!("detected_before_fix={detected}"));
        }
        return Ok(DoctorCheck {
            name: "blob_ref_counts",
            label: "Blob reference counters",
            status: DoctorStatus::Ok,
            summary,
            details,
            suggestion: None,
        });
    }

    let details = drifts
        .into_iter()
        .map(|drift| {
            format!(
                "blob#{} recorded={} actual={} policy_id={} path={}",
                drift.blob_id,
                drift.recorded_ref_count,
                drift.actual_ref_count,
                drift.policy_id,
                drift.storage_path
            )
        })
        .collect();

    Ok(DoctorCheck {
        name: "blob_ref_counts",
        label: "Blob reference counters",
        status: DoctorStatus::Warn,
        summary: match policy_id {
            Some(policy_id) => format!(
                "{} blob ref_count mismatch(es) for policy #{}",
                detected.max(fixed),
                policy_id
            ),
            None => format!("{} blob ref_count mismatch(es)", detected.max(fixed)),
        },
        details,
        suggestion: Some("Run doctor --deep --fix to write back file_blobs.ref_count.".to_string()),
    })
}

async fn doctor_storage_scan_checks(
    db: &sea_orm::DatabaseConnection,
    runtime_config: &crate::config::RuntimeConfig,
    policy_id: Option<i64>,
) -> Result<Vec<DoctorCheck>> {
    storage_scan::doctor_storage_scan_checks(db, runtime_config, policy_id).await
}

async fn doctor_folder_tree_check(db: &sea_orm::DatabaseConnection) -> Result<DoctorCheck> {
    let issues = integrity::audit_folder_tree(db).await?;
    if issues.is_empty() {
        return Ok(DoctorCheck {
            name: "folder_tree_integrity",
            label: "Folder tree integrity",
            status: DoctorStatus::Ok,
            summary: "folder parent chains are internally consistent".to_string(),
            details: Vec::new(),
            suggestion: None,
        });
    }

    let has_cycle = issues
        .iter()
        .any(|issue| issue.kind == integrity::FolderTreeIssueKind::Cycle);
    let details = issues
        .into_iter()
        .map(|issue| {
            format!(
                "{} folder#{} {}",
                match issue.kind {
                    integrity::FolderTreeIssueKind::MissingParent => "missing_parent",
                    integrity::FolderTreeIssueKind::CrossScopeParent => "cross_scope_parent",
                    integrity::FolderTreeIssueKind::Cycle => "cycle",
                },
                issue.folder_id,
                issue.detail
            )
        })
        .collect();

    Ok(DoctorCheck {
        name: "folder_tree_integrity",
        label: "Folder tree integrity",
        status: DoctorStatus::Fail,
        summary: if has_cycle {
            "folder tree contains cycles or invalid parent references".to_string()
        } else {
            "folder tree contains invalid parent references".to_string()
        },
        details,
        suggestion: Some(
            "Fix dangling parent_id values or folder cycles before continuing with bulk move or delete operations."
                .to_string(),
        ),
    })
}

fn doctor_public_site_url_check(runtime_config: &crate::config::RuntimeConfig) -> DoctorCheck {
    let Some(raw_value) = runtime_config.get(crate::config::site_url::PUBLIC_SITE_URL_KEY) else {
        return DoctorCheck {
            name: "public_site_url",
            label: "Public site URL",
            status: DoctorStatus::Warn,
            summary: "public_site_url is not configured".to_string(),
            details: vec![
                "share, preview, and callback URLs will not have a stable public origin"
                    .to_string(),
            ],
            suggestion: Some(
                "Set config public_site_url to an externally reachable HTTP(S) origin.".to_string(),
            ),
        };
    };

    if raw_value.trim().is_empty() {
        return DoctorCheck {
            name: "public_site_url",
            label: "Public site URL",
            status: DoctorStatus::Warn,
            summary: "public_site_url is empty".to_string(),
            details: vec![
                "share, preview, and callback URLs will not have a stable public origin"
                    .to_string(),
            ],
            suggestion: Some(
                "Set config public_site_url to an externally reachable HTTP(S) origin.".to_string(),
            ),
        };
    }

    match crate::config::site_url::parse_public_site_url_value(&raw_value) {
        Ok(origins) => {
            let configured = serde_json::to_string(&origins)
                .unwrap_or_else(|_| "<invalid public_site_url origins>".to_string());
            if origins.is_empty() {
                return DoctorCheck {
                    name: "public_site_url",
                    label: "Public site URL",
                    status: DoctorStatus::Warn,
                    summary: "public_site_url is empty".to_string(),
                    details: vec![
                        "share, preview, and callback URLs will not have a stable public origin"
                            .to_string(),
                    ],
                    suggestion: Some(
                        "Set config public_site_url to at least one externally reachable HTTP(S) origin."
                            .to_string(),
                    ),
                };
            }
            if origins.iter().any(|origin| origin.starts_with("http://")) {
                return DoctorCheck {
                    name: "public_site_url",
                    label: "Public site URL",
                    status: DoctorStatus::Warn,
                    summary: "public_site_url uses insecure HTTP".to_string(),
                    details: vec![
                        format!("configured={configured}"),
                        "production deployments should terminate TLS at a reverse proxy"
                            .to_string(),
                    ],
                    suggestion: Some(
                        "Put the site behind an HTTPS reverse proxy and change public_site_url to an https:// origin."
                            .to_string(),
                    ),
                };
            }

            DoctorCheck {
                name: "public_site_url",
                label: "Public site URL",
                status: DoctorStatus::Ok,
                summary: format!("configured as {configured}"),
                details: Vec::new(),
                suggestion: None,
            }
        }
        Err(err) => DoctorCheck {
            name: "public_site_url",
            label: "Public site URL",
            status: DoctorStatus::Fail,
            summary: "public_site_url is invalid".to_string(),
            details: vec![err.message().to_string()],
            suggestion: Some(
                "Use a plain origin such as https://drive.example.com, without a path or non-HTTP(S) scheme."
                    .to_string(),
            ),
        },
    }
}

fn doctor_mail_check(runtime_config: &crate::config::RuntimeConfig) -> DoctorCheck {
    let settings = crate::config::mail::runtime_mail_settings(runtime_config);
    let mut details = vec![
        format!(
            "smtp_host={}",
            non_empty_or_placeholder(&settings.smtp_host)
        ),
        format!("smtp_port={}", settings.smtp_port),
        format!(
            "from_address={}",
            non_empty_or_placeholder(&settings.from_address)
        ),
        format!(
            "auth={}",
            if settings.smtp_username.trim().is_empty() {
                "disabled"
            } else {
                "enabled"
            }
        ),
        format!(
            "transport_security={}",
            if settings.encryption_enabled {
                "enabled"
            } else {
                "disabled"
            }
        ),
    ];

    if settings.smtp_username.trim().is_empty() ^ settings.smtp_password.trim().is_empty() {
        details.push(
            "mail_smtp_username and mail_smtp_password must both be set or both be empty"
                .to_string(),
        );
        return DoctorCheck {
            name: "mail_configuration",
            label: "Mail configuration",
            status: DoctorStatus::Fail,
            summary: "SMTP authentication is only partially configured".to_string(),
            details,
            suggestion: Some(
                "Set both mail_smtp_username and mail_smtp_password together, or leave both empty."
                    .to_string(),
            ),
        };
    }

    if !settings.is_configured() {
        let mut missing = Vec::new();
        if settings.smtp_host.trim().is_empty() {
            missing.push("mail_smtp_host");
        }
        if settings.from_address.trim().is_empty() {
            missing.push("mail_from_address");
        }
        details.push(format!("missing={}", missing.join(", ")));
        return DoctorCheck {
            name: "mail_configuration",
            label: "Mail configuration",
            status: DoctorStatus::Warn,
            summary: "mail delivery is not fully configured".to_string(),
            details,
            suggestion: Some(
                "At minimum, set mail_smtp_host and mail_from_address to make mail delivery usable."
                    .to_string(),
            ),
        };
    }

    DoctorCheck {
        name: "mail_configuration",
        label: "Mail configuration",
        status: DoctorStatus::Ok,
        summary: "mail delivery settings are configured".to_string(),
        details,
        suggestion: None,
    }
}

fn doctor_preview_apps_check(runtime_config: &crate::config::RuntimeConfig) -> DoctorCheck {
    let raw = runtime_config
        .get(crate::services::preview::apps::PREVIEW_APPS_CONFIG_KEY)
        .unwrap_or_else(crate::services::preview::apps::default_public_preview_apps_json);

    let normalized =
        match crate::services::preview::apps::normalize_public_preview_apps_config_value(&raw) {
            Ok(normalized) => normalized,
            Err(err) => {
                return DoctorCheck {
                    name: "preview_apps",
                    label: "Preview app registry",
                    status: DoctorStatus::Fail,
                    summary: "preview app registry is invalid".to_string(),
                    details: vec![err.message().to_string()],
                    suggestion: Some(
                        "Fix frontend_preview_apps_json or restore the default preview app configuration."
                            .to_string(),
                    ),
                };
            }
        };

    let parsed: crate::services::preview::apps::PublicPreviewAppsConfig = match serde_json::from_str(
        &normalized,
    ) {
        Ok(parsed) => parsed,
        Err(err) => {
            return DoctorCheck {
                    name: "preview_apps",
                    label: "Preview app registry",
                    status: DoctorStatus::Fail,
                    summary: "preview app registry could not be parsed".to_string(),
                    details: vec![err.to_string()],
                    suggestion: Some(
                        "Check whether frontend_preview_apps_json was edited into an invalid state; restore the default value if needed."
                            .to_string(),
                    ),
                };
        }
    };

    let total_apps = parsed.apps.len();
    let enabled_apps = parsed.apps.iter().filter(|app| app.enabled).count();
    let wopi_apps = parsed
        .apps
        .iter()
        .filter(|app| {
            app.enabled && app.provider == crate::services::preview::apps::PreviewAppProvider::Wopi
        })
        .count();
    let details = vec![
        format!("apps={total_apps}"),
        format!("enabled={enabled_apps}"),
        format!("wopi_enabled={wopi_apps}"),
    ];

    if wopi_apps > 0 && crate::config::site_url::public_site_urls(runtime_config).is_empty() {
        return DoctorCheck {
            name: "preview_apps",
            label: "Preview app registry",
            status: DoctorStatus::Warn,
            summary: "WOPI apps are configured but public_site_url is empty".to_string(),
            details,
            suggestion: Some(
                "Set public_site_url or disable WOPI apps to avoid generating unusable launch entry points."
                    .to_string(),
            ),
        };
    }

    DoctorCheck {
        name: "preview_apps",
        label: "Preview app registry",
        status: DoctorStatus::Ok,
        summary: "preview app registry is valid".to_string(),
        details,
        suggestion: None,
    }
}

async fn doctor_storage_policy_check(db: &sea_orm::DatabaseConnection) -> DoctorCheck {
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
    if let Err(err) = snapshot.reload(db).await {
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

fn doctor_status_label(status: DoctorStatus) -> &'static str {
    match status {
        DoctorStatus::Ok => "ready",
        DoctorStatus::Warn => "attention",
        DoctorStatus::Fail => "failed",
    }
}

fn non_empty_or_placeholder(value: &str) -> &str {
    if value.trim().is_empty() {
        "<empty>"
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::{DoctorStatus, doctor_public_site_url_check};
    use crate::config::RuntimeConfig;
    use crate::config::definitions::CONFIG_CATEGORY_SITE;
    use crate::config::site_url::PUBLIC_SITE_URL_KEY;
    use aster_forge_config::{ConfigSource, ConfigValueType};
    use aster_forge_db::system_config;
    use chrono::Utc;

    fn config_model(key: &str, value: &str) -> system_config::Model {
        system_config::Model {
            id: 1,
            key: key.to_string(),
            value: value.to_string(),
            value_type: ConfigValueType::String,
            requires_restart: false,
            is_sensitive: false,
            source: ConfigSource::System,
            visibility: aster_forge_config::ConfigVisibility::Private,
            namespace: String::new(),
            category: CONFIG_CATEGORY_SITE.to_string(),
            description: "test".to_string(),
            updated_at: Utc::now(),
            updated_by: None,
        }
    }

    #[test]
    fn doctor_public_site_url_warns_for_http_origins() {
        let runtime_config = RuntimeConfig::new();
        runtime_config.apply(config_model(
            PUBLIC_SITE_URL_KEY,
            r#"["http://drive.example.com"]"#,
        ));

        let check = doctor_public_site_url_check(&runtime_config);

        assert_eq!(check.status, DoctorStatus::Warn);
        assert_eq!(check.summary, "public_site_url uses insecure HTTP");
        assert!(
            check
                .details
                .iter()
                .any(|detail| { detail == r#"configured=["http://drive.example.com"]"# })
        );
        assert!(
            check
                .suggestion
                .as_deref()
                .is_some_and(|hint| hint.contains("https://"))
        );
    }

    #[test]
    fn doctor_public_site_url_accepts_https_origins() {
        let runtime_config = RuntimeConfig::new();
        runtime_config.apply(config_model(
            PUBLIC_SITE_URL_KEY,
            r#"["https://drive.example.com"]"#,
        ));

        let check = doctor_public_site_url_check(&runtime_config);

        assert_eq!(check.status, DoctorStatus::Ok);
        assert_eq!(
            check.summary,
            r#"configured as ["https://drive.example.com"]"#
        );
    }
}
