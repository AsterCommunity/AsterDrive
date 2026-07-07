//! 服务模块：`ops::admin`。

mod cache;

use chrono::{DateTime, Duration, LocalResult, NaiveDate, TimeZone, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
#[cfg(all(debug_assertions, feature = "openapi"))]
use utoipa::{IntoParams, ToSchema};

use crate::db::repository::{
    audit_log_repo, background_task_repo, file_repo, share_repo, user_repo,
};
use crate::errors::{AsterError, MapAsterErr, Result};
use crate::runtime::SharedRuntimeState;
use crate::services::{
    ops::audit,
    task_service::{SystemRuntimeTaskKind, types::RuntimeSystemHealthStatus},
    user::account,
    user::profile,
};
use crate::types::{BackgroundTaskKind, BackgroundTaskStatus, UserStatus};
use crate::utils::numbers::u32_to_usize;

type DateTimeUtc = DateTime<Utc>;

const DEFAULT_DAYS: u32 = 7;
const MAX_DAYS: u32 = 90;
const DEFAULT_EVENT_LIMIT: u64 = 8;
const MAX_EVENT_LIMIT: u64 = 50;
const DEFAULT_TIMEZONE: &str = "UTC";
const ADMIN_OVERVIEW_AUDIT_ACTION_BATCH_SIZE: u64 = 1_000;

#[derive(Clone, Debug, Deserialize)]
#[cfg_attr(
    all(debug_assertions, feature = "openapi"),
    derive(IntoParams, ToSchema)
)]
pub struct AdminOverviewQuery {
    pub days: Option<u32>,
    pub timezone: Option<String>,
    pub event_limit: Option<u64>,
}

impl AdminOverviewQuery {
    pub fn days_or_default(&self) -> u32 {
        self.days
            .map(|days| days.clamp(1, MAX_DAYS))
            .unwrap_or(DEFAULT_DAYS)
    }

    pub fn event_limit_or_default(&self) -> u64 {
        self.event_limit
            .map(|limit| limit.clamp(1, MAX_EVENT_LIMIT))
            .unwrap_or(DEFAULT_EVENT_LIMIT)
    }

    pub fn timezone_name(&self) -> &str {
        self.timezone
            .as_deref()
            .filter(|timezone| !timezone.trim().is_empty())
            .unwrap_or(DEFAULT_TIMEZONE)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct AdminOverviewStats {
    pub total_users: u64,
    pub active_users: u64,
    pub disabled_users: u64,
    pub total_files: u64,
    pub total_file_bytes: i64,
    pub total_blobs: u64,
    pub total_blob_bytes: i64,
    pub total_shares: u64,
    pub audit_events_today: u64,
    pub new_users_today: u64,
    pub uploads_today: u64,
    pub shares_today: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct AdminOverviewDailyReport {
    pub date: String,
    pub sign_ins: u64,
    pub new_users: u64,
    pub uploads: u64,
    pub share_creations: u64,
    pub deletions: u64,
    pub total_events: u64,
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct AdminBackgroundTaskEvent {
    pub id: i64,
    pub kind: BackgroundTaskKind,
    pub status: BackgroundTaskStatus,
    pub display_name: String,
    pub creator: Option<account::UserSummary>,
    pub team_id: Option<i64>,
    pub status_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presentation: Option<crate::services::task_service::types::TaskPresentation>,
    pub last_error: Option<String>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub created_at: DateTimeUtc,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub started_at: Option<DateTimeUtc>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub finished_at: Option<DateTimeUtc>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub updated_at: DateTimeUtc,
    pub duration_ms: Option<i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum AdminSystemHealthStatus {
    Unknown,
    Healthy,
    Degraded,
    Unhealthy,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct AdminSystemHealthComponent {
    pub name: String,
    pub status: AdminSystemHealthStatus,
    pub message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct AdminSystemHealthSummary {
    pub status: AdminSystemHealthStatus,
    pub summary: Option<String>,
    pub details: Option<String>,
    pub components: Vec<AdminSystemHealthComponent>,
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = Option<String>))]
    pub checked_at: Option<DateTimeUtc>,
    pub task_id: Option<i64>,
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(all(debug_assertions, feature = "openapi"), derive(ToSchema))]
pub struct AdminOverview {
    #[cfg_attr(all(debug_assertions, feature = "openapi"), schema(value_type = String))]
    pub generated_at: DateTimeUtc,
    pub timezone: String,
    pub days: u32,
    pub stats: AdminOverviewStats,
    pub system_health: AdminSystemHealthSummary,
    pub daily_reports: Vec<AdminOverviewDailyReport>,
    pub recent_events: Vec<audit::AuditLogEntry>,
    pub recent_background_tasks: Vec<AdminBackgroundTaskEvent>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct AdminOverviewCore {
    generated_at: DateTimeUtc,
    timezone: String,
    days: u32,
    stats: AdminOverviewStats,
    system_health: AdminSystemHealthSummary,
    daily_reports: Vec<AdminOverviewDailyReport>,
}

impl AdminOverviewCore {
    fn into_overview(
        self,
        recent_events: Vec<audit::AuditLogEntry>,
        recent_background_tasks: Vec<AdminBackgroundTaskEvent>,
    ) -> AdminOverview {
        AdminOverview {
            generated_at: self.generated_at,
            timezone: self.timezone,
            days: self.days,
            stats: self.stats,
            system_health: self.system_health,
            daily_reports: self.daily_reports,
            recent_events,
            recent_background_tasks,
        }
    }
}

pub async fn get_overview(
    state: &impl SharedRuntimeState,
    days: u32,
    timezone_name: &str,
    event_limit: u64,
) -> Result<AdminOverview> {
    let generated_at = Utc::now();
    let timezone = parse_timezone(timezone_name)?;
    let (recent_events, recent_background_tasks) =
        load_recent_overview_events(state, event_limit).await?;
    if let Some(core) = cache::load_overview_core(state, days, timezone.name()).await {
        tracing::debug!(
            days,
            timezone = timezone.name(),
            event_limit,
            "admin overview cache hit"
        );
        return Ok(core.into_overview(recent_events, recent_background_tasks));
    }

    let today = generated_at.with_timezone(&timezone).date_naive();

    let (
        total_users,
        active_users,
        disabled_users,
        total_files,
        total_file_bytes,
        total_blobs,
        total_blob_bytes,
        total_shares,
        daily_reports,
        latest_health_task,
    ) = tokio::try_join!(
        user_repo::count_all(state.reader_db()),
        user_repo::count_by_status(state.reader_db(), UserStatus::Active),
        user_repo::count_by_status(state.reader_db(), UserStatus::Disabled),
        file_repo::count_live_files(state.reader_db()),
        file_repo::sum_live_file_bytes(state.reader_db()),
        file_repo::count_all_blobs(state.reader_db()),
        file_repo::sum_blob_bytes(state.reader_db()),
        share_repo::count_all(state.reader_db()),
        build_daily_reports(state, today, days, timezone),
        crate::services::task_service::runtime::find_latest_system_runtime_by_task_name(
            state,
            SystemRuntimeTaskKind::SystemHealthCheck,
        ),
    )?;
    let today_report = daily_reports
        .first()
        .cloned()
        .unwrap_or(AdminOverviewDailyReport {
            date: today.to_string(),
            sign_ins: 0,
            new_users: 0,
            uploads: 0,
            share_creations: 0,
            deletions: 0,
            total_events: 0,
        });
    let system_health = build_system_health_summary(latest_health_task);

    let core = AdminOverviewCore {
        generated_at,
        timezone: timezone.name().to_string(),
        days,
        stats: AdminOverviewStats {
            total_users,
            active_users,
            disabled_users,
            total_files,
            total_file_bytes,
            total_blobs,
            total_blob_bytes,
            total_shares,
            audit_events_today: today_report.total_events,
            new_users_today: today_report.new_users,
            uploads_today: today_report.uploads,
            shares_today: today_report.share_creations,
        },
        system_health,
        daily_reports,
    };
    cache::store_overview_core(state, days, timezone.name(), &core).await;
    tracing::debug!(
        days,
        timezone = timezone.name(),
        event_limit,
        "admin overview cache miss"
    );
    Ok(core.into_overview(recent_events, recent_background_tasks))
}

async fn load_recent_overview_events(
    state: &impl SharedRuntimeState,
    event_limit: u64,
) -> Result<(Vec<audit::AuditLogEntry>, Vec<AdminBackgroundTaskEvent>)> {
    let (recent_events, recent_background_tasks) = tokio::try_join!(
        audit::query(
            state,
            audit::AuditLogFilters {
                user_id: None,
                action: None,
                entity_type: None,
                entity_id: None,
                after: None,
                before: None,
            },
            event_limit,
            0,
            crate::api::pagination::AdminAuditLogSortBy::CreatedAt,
            crate::api::pagination::SortOrder::Desc,
        ),
        background_task_repo::list_recent(state.reader_db(), event_limit),
    )?;
    Ok((
        recent_events.items,
        build_background_task_events(state, recent_background_tasks).await?,
    ))
}

async fn build_background_task_events(
    state: &impl SharedRuntimeState,
    tasks: Vec<crate::entities::background_task::Model>,
) -> Result<Vec<AdminBackgroundTaskEvent>> {
    let creator_ids: Vec<i64> = tasks
        .iter()
        .filter_map(|task| task.creator_user_id)
        .collect();
    let creators =
        account::user_summaries_by_ids(state, &creator_ids, profile::AvatarAudience::AdminUser)
            .await?;

    tasks
        .into_iter()
        .map(|task| build_background_task_event(task, &creators))
        .collect()
}

fn build_background_task_event(
    task: crate::entities::background_task::Model,
    creators: &HashMap<i64, account::UserSummary>,
) -> Result<AdminBackgroundTaskEvent> {
    let duration_ms = match (task.started_at, task.finished_at) {
        (Some(started_at), Some(finished_at)) => Some(std::cmp::max(
            (finished_at - started_at).num_milliseconds(),
            0,
        )),
        _ => None,
    };
    let presentation = match crate::services::task_service::build_task_presentation_for_model(&task)
    {
        Ok(presentation) => presentation,
        Err(error) => {
            tracing::warn!(
                task_id = task.id,
                error = %error,
                "failed to build background task presentation for admin overview"
            );
            None
        }
    };

    Ok(AdminBackgroundTaskEvent {
        id: task.id,
        kind: task.kind,
        status: task.status,
        display_name: task.display_name,
        creator: task
            .creator_user_id
            .and_then(|user_id| creators.get(&user_id).cloned()),
        team_id: task.team_id,
        status_text: task.status_text,
        presentation,
        last_error: task.last_error,
        created_at: task.created_at,
        started_at: task.started_at,
        finished_at: task.finished_at,
        updated_at: task.updated_at,
        duration_ms,
    })
}

fn build_system_health_summary(
    task: Option<crate::entities::background_task::Model>,
) -> AdminSystemHealthSummary {
    let Some(task) = task else {
        return AdminSystemHealthSummary {
            status: AdminSystemHealthStatus::Unknown,
            summary: None,
            details: None,
            components: Vec::new(),
            checked_at: None,
            task_id: None,
        };
    };

    let summary = task.status_text.clone();
    let details = task.last_error.clone();
    let parsed_result = parse_runtime_task_result(&task);
    let (status, components) = parsed_result
        .and_then(|result| result.system_health)
        .map(|health| {
            (
                admin_health_status_from_runtime(health.status),
                health
                    .components
                    .into_iter()
                    .map(|component| AdminSystemHealthComponent {
                        name: component.name,
                        status: admin_health_status_from_runtime(component.status),
                        message: component.message,
                    })
                    .collect(),
            )
        })
        .unwrap_or_else(|| {
            let status = match task.status {
                BackgroundTaskStatus::Succeeded => AdminSystemHealthStatus::Healthy,
                BackgroundTaskStatus::Failed => AdminSystemHealthStatus::Unhealthy,
                _ => AdminSystemHealthStatus::Unknown,
            };
            (status, Vec::new())
        });

    AdminSystemHealthSummary {
        status,
        summary,
        details,
        components,
        checked_at: Some(task.finished_at.unwrap_or(task.updated_at)),
        task_id: Some(task.id),
    }
}

fn parse_runtime_task_result(
    task: &crate::entities::background_task::Model,
) -> Option<crate::services::task_service::types::RuntimeTaskResult> {
    let raw = task.result_json.as_ref()?;
    match serde_json::from_str(raw.as_ref()) {
        Ok(result) => Some(result),
        Err(error) => {
            tracing::warn!(
                task_id = task.id,
                "failed to parse system health runtime result: {error}"
            );
            None
        }
    }
}

fn admin_health_status_from_runtime(status: RuntimeSystemHealthStatus) -> AdminSystemHealthStatus {
    match status {
        RuntimeSystemHealthStatus::Healthy => AdminSystemHealthStatus::Healthy,
        RuntimeSystemHealthStatus::Degraded => AdminSystemHealthStatus::Degraded,
        RuntimeSystemHealthStatus::Unhealthy => AdminSystemHealthStatus::Unhealthy,
    }
}

async fn build_daily_reports(
    state: &impl SharedRuntimeState,
    today: NaiveDate,
    days: u32,
    timezone: Tz,
) -> Result<Vec<AdminOverviewDailyReport>> {
    let capacity = u32_to_usize(days, "admin overview days")?;
    let mut reports = Vec::with_capacity(capacity);
    let mut report_indexes = HashMap::with_capacity(capacity);

    for offset in 0..days {
        let date = today - Duration::days(i64::from(offset));
        report_indexes.insert(date, reports.len());

        reports.push(AdminOverviewDailyReport {
            date: date.to_string(),
            sign_ins: 0,
            new_users: 0,
            uploads: 0,
            share_creations: 0,
            deletions: 0,
            total_events: 0,
        });
    }

    let oldest_date = today - Duration::days(i64::from(days.saturating_sub(1)));
    let start = start_of_local_day(oldest_date, timezone)?;
    let end = start_of_local_day(today + Duration::days(1), timezone)?;

    audit::flush_global_audit_log_manager().await;
    let mut cursor = None;
    loop {
        let events = audit_log_repo::find_action_page_in_range(
            state.reader_db(),
            start,
            end,
            cursor,
            ADMIN_OVERVIEW_AUDIT_ACTION_BATCH_SIZE,
        )
        .await?;
        if events.is_empty() {
            break;
        }

        for (_id, action, created_at) in &events {
            let date = created_at.with_timezone(&timezone).date_naive();
            let Some(report_index) = report_indexes.get(&date).copied() else {
                continue;
            };
            let report = &mut reports[report_index];
            let action = audit::AuditAction::from_str_name(action);
            record_audit_action(report, action);
        }
        cursor = events.last().map(|(id, _, created_at)| (*created_at, *id));
    }

    Ok(reports)
}

fn record_audit_action(report: &mut AdminOverviewDailyReport, action: Option<audit::AuditAction>) {
    report.total_events += 1;

    match action {
        Some(audit::AuditAction::UserLogin)
        | Some(audit::AuditAction::UserPasskeyLogin)
        | Some(audit::AuditAction::UserExternalAuthLogin) => report.sign_ins += 1,
        Some(audit::AuditAction::UserRegister) | Some(audit::AuditAction::AdminCreateUser) => {
            report.new_users += 1
        }
        Some(audit::AuditAction::FileUpload) => report.uploads += 1,
        Some(audit::AuditAction::ShareCreate) => report.share_creations += 1,
        Some(audit::AuditAction::BatchDelete)
        | Some(audit::AuditAction::FileDelete)
        | Some(audit::AuditAction::FolderDelete) => report.deletions += 1,
        _ => {}
    }
}

fn parse_timezone(timezone_name: &str) -> Result<Tz> {
    timezone_name.parse::<Tz>().map_aster_err_with(|| {
        AsterError::validation_error(format!("invalid timezone '{timezone_name}'"))
    })
}

fn start_of_local_day(date: NaiveDate, timezone: Tz) -> Result<DateTimeUtc> {
    let naive = date
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| AsterError::validation_error("invalid local day start"))?;
    match timezone.from_local_datetime(&naive) {
        LocalResult::Single(dt) => Ok(dt.with_timezone(&Utc)),
        LocalResult::Ambiguous(earliest, _) => Ok(earliest.with_timezone(&Utc)),
        LocalResult::None => Err(AsterError::validation_error(format!(
            "timezone '{}' cannot represent local midnight for {}",
            timezone.name(),
            date
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::{AdminOverviewDailyReport, record_audit_action};
    use crate::services::ops::audit;

    fn empty_report() -> AdminOverviewDailyReport {
        AdminOverviewDailyReport {
            date: "2026-03-30".to_string(),
            sign_ins: 0,
            new_users: 0,
            uploads: 0,
            share_creations: 0,
            deletions: 0,
            total_events: 0,
        }
    }

    #[test]
    fn record_audit_action_counts_categories() {
        let mut report = empty_report();

        for action in [
            Some(audit::AuditAction::UserLogin),
            Some(audit::AuditAction::UserRegister),
            Some(audit::AuditAction::AdminCreateUser),
            Some(audit::AuditAction::FileUpload),
            Some(audit::AuditAction::ShareCreate),
            Some(audit::AuditAction::BatchDelete),
            Some(audit::AuditAction::FileDelete),
            Some(audit::AuditAction::FolderDelete),
            Some(audit::AuditAction::UserPasskeyLogin),
            Some(audit::AuditAction::UserExternalAuthLogin),
            Some(audit::AuditAction::ConfigUpdate),
            None,
        ] {
            record_audit_action(&mut report, action);
        }

        assert_eq!(report.sign_ins, 3);
        assert_eq!(report.new_users, 2);
        assert_eq!(report.uploads, 1);
        assert_eq!(report.share_creations, 1);
        assert_eq!(report.deletions, 3);
        assert_eq!(report.total_events, 12);
    }
}
