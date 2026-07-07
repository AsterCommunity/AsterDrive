//! 管理概览缓存。

use crate::cache::CacheExt;
use crate::runtime::SharedRuntimeState;

use super::AdminOverviewCore;

const ADMIN_OVERVIEW_CORE_CACHE_TTL: u64 = 15;

fn overview_core_cache_key(days: u32, timezone_name: &str) -> String {
    format!("admin_overview_core:{days}:{timezone_name}")
}

pub(super) async fn load_overview_core(
    state: &impl SharedRuntimeState,
    days: u32,
    timezone_name: &str,
) -> Option<AdminOverviewCore> {
    state
        .cache()
        .get::<AdminOverviewCore>(&overview_core_cache_key(days, timezone_name))
        .await
}

pub(super) async fn store_overview_core(
    state: &impl SharedRuntimeState,
    days: u32,
    timezone_name: &str,
    core: &AdminOverviewCore,
) {
    state
        .cache()
        .set(
            &overview_core_cache_key(days, timezone_name),
            core,
            Some(ADMIN_OVERVIEW_CORE_CACHE_TTL),
        )
        .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::test_support::CacheOnlyState;

    fn core(days: u32, timezone: &str, total_users: u64) -> AdminOverviewCore {
        AdminOverviewCore {
            generated_at: chrono::Utc::now(),
            timezone: timezone.to_string(),
            days,
            stats: super::super::AdminOverviewStats {
                total_users,
                active_users: total_users,
                disabled_users: 0,
                total_files: 0,
                total_file_bytes: 0,
                total_blobs: 0,
                total_blob_bytes: 0,
                total_shares: 0,
                audit_events_today: 0,
                new_users_today: 0,
                uploads_today: 0,
                shares_today: 0,
            },
            system_health: super::super::AdminSystemHealthSummary {
                status: super::super::AdminSystemHealthStatus::Unknown,
                summary: None,
                details: None,
                components: Vec::new(),
                checked_at: None,
                task_id: None,
            },
            daily_reports: Vec::new(),
        }
    }

    #[tokio::test]
    async fn overview_core_cache_is_scoped_by_days_and_timezone() {
        let state = CacheOnlyState::new().await;

        store_overview_core(&state, 7, "UTC", &core(7, "UTC", 7)).await;
        store_overview_core(&state, 30, "UTC", &core(30, "UTC", 30)).await;
        store_overview_core(&state, 7, "Asia/Shanghai", &core(7, "Asia/Shanghai", 70)).await;

        assert_eq!(
            load_overview_core(&state, 7, "UTC")
                .await
                .map(|cached| cached.stats.total_users),
            Some(7)
        );
        assert_eq!(
            load_overview_core(&state, 30, "UTC")
                .await
                .map(|cached| cached.stats.total_users),
            Some(30)
        );
        assert_eq!(
            load_overview_core(&state, 7, "Asia/Shanghai")
                .await
                .map(|cached| cached.stats.total_users),
            Some(70)
        );
    }
}
