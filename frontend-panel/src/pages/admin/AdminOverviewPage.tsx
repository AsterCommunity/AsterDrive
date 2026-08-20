import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { OverviewBackgroundTasksSection } from "@/components/admin/admin-overview-page/OverviewBackgroundTasksSection";
import { OverviewDailyReportsSection } from "@/components/admin/admin-overview-page/OverviewDailyReportsSection";
import { OverviewRecentEventsSection } from "@/components/admin/admin-overview-page/OverviewRecentEventsSection";
import { OverviewStatsSection } from "@/components/admin/admin-overview-page/OverviewStatsSection";
import {
	OverviewTrendChart,
	type OverviewTrendSeries,
} from "@/components/admin/admin-overview-page/OverviewTrendChart";
import { SystemHealthBanner } from "@/components/admin/admin-overview-page/SystemHealthBanner";
import { EmptyState } from "@/components/common/EmptyState";
import { AdminLayout } from "@/components/layout/AdminLayout";
import { AdminPageHeader } from "@/components/layout/AdminPageHeader";
import { AdminPageShell } from "@/components/layout/AdminPageShell";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { Skeleton } from "@/components/ui/skeleton";
import { handleApiError } from "@/hooks/useApiError";
import { usePageTitle } from "@/hooks/usePageTitle";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import { formatDateAbsolute, formatDateAbsoluteWithOffset } from "@/lib/format";
import { cn } from "@/lib/utils";
import { adminOverviewService } from "@/services/adminService";
import {
	resolveActiveDisplayTimeZone,
	useDisplayTimeZoneStore,
} from "@/stores/displayTimeZoneStore";
import type { AdminOverview } from "@/types/api";

const OVERVIEW_TREND_DAYS = 7;
const DEFAULT_EVENT_LIMIT = 10;

export default function AdminOverviewPage() {
	const { t } = useTranslation(["admin", "tasks"]);
	usePageTitle(t("overview"));
	const timezone = useDisplayTimeZoneStore((s) =>
		resolveActiveDisplayTimeZone(s.preference),
	);
	const trendSeries: OverviewTrendSeries[] = [
		{
			badgeClass:
				"border-emerald-500/30 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300",
			key: "uploads",
			label: t("overview_report_uploads"),
			stroke: "#10b981",
			strokeWidth: 2.5,
		},
		{
			badgeClass:
				"border-sky-500/30 bg-sky-500/10 text-sky-700 dark:text-sky-300",
			key: "shareCreations",
			label: t("overview_report_shares"),
			stroke: "#0ea5e9",
			strokeWidth: 2.5,
		},
		{
			badgeClass:
				"border-amber-500/30 bg-amber-500/10 text-amber-700 dark:text-amber-300",
			key: "newUsers",
			label: t("overview_report_new_users"),
			stroke: "#f59e0b",
			strokeWidth: 2.5,
		},
	];
	const [overview, setOverview] = useState<AdminOverview | null>(null);
	const [loading, setLoading] = useState(true);
	const [refreshing, setRefreshing] = useState(false);

	const load = useCallback(
		async (mode: "initial" | "refresh" = "initial") => {
			try {
				if (mode === "initial") {
					setLoading(true);
				} else {
					setRefreshing(true);
				}

				const nextOverview = await adminOverviewService.get({
					days: OVERVIEW_TREND_DAYS,
					timezone,
					event_limit: DEFAULT_EVENT_LIMIT,
				});
				setOverview(nextOverview);
			} catch (error) {
				handleApiError(error);
			} finally {
				setLoading(false);
				setRefreshing(false);
			}
		},
		[timezone],
	);

	useEffect(() => {
		void load();
	}, [load]);

	const secondaryBadges = overview?.stats
		? [
				{
					key: "active-users",
					label: t("overview_active_users_badge", {
						count: overview.stats.active_users,
					}),
				},
				{
					key: "disabled-users",
					label: t("overview_disabled_users_badge", {
						count: overview.stats.disabled_users,
					}),
				},
				{
					key: "today-events",
					label: t("overview_today_events_badge", {
						count: overview.stats.audit_events_today,
					}),
				},
				{
					key: "today-new-users",
					label: t("overview_today_new_users_badge", {
						count: overview.stats.new_users_today,
					}),
				},
				{
					key: "today-uploads",
					label: t("overview_today_uploads_badge", {
						count: overview.stats.uploads_today,
					}),
				},
				{
					key: "today-shares",
					label: t("overview_today_shares_badge", {
						count: overview.stats.shares_today,
					}),
				},
			]
		: [];
	return (
		<AdminLayout>
			<AdminPageShell className="gap-8 md:gap-10">
				{/* 页头 px-0：shell 已提供页缘 padding，去掉 header 自带 px 避免与裸 section 错位 */}
				<AdminPageHeader
					title={t("overview")}
					description={t("overview_intro")}
					className="px-0 md:px-0"
					actions={
						<Button
							variant="outline"
							size="sm"
							className={ADMIN_CONTROL_HEIGHT_CLASS}
							onClick={() => void load("refresh")}
							disabled={loading || refreshing}
						>
							<Icon
								name={refreshing ? "Spinner" : "ArrowClockwise"}
								className={cn("size-4", refreshing && "animate-spin")}
							/>
							{t("core:refresh")}
						</Button>
					}
				/>

				{loading && !overview ? (
					<div className="space-y-4">
						<Skeleton className="h-7 w-64" />
						<Skeleton className="h-[320px] w-full rounded-2xl" />
					</div>
				) : overview ? (
					<section className="min-w-0 space-y-5">
						<SystemHealthBanner health={overview.system_health} />
						<OverviewTrendChart
							reports={overview.daily_reports}
							emptyTitle={t("overview_daily_trend_empty")}
							emptyDescription={t("overview_daily_trend_empty_desc")}
							averageLabel={t("overview_daily_trend_average")}
							latestLabel={t("overview_daily_trend_latest")}
							peakLabel={t("overview_daily_trend_peak")}
							series={trendSeries}
						/>
						<div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
							<div className="flex min-w-0 flex-1 flex-wrap items-center gap-2">
								{secondaryBadges.map((badge) => (
									<Badge key={badge.key} variant="secondary">
										{badge.label}
									</Badge>
								))}
							</div>
							<span
								className="ml-auto whitespace-nowrap text-right"
								title={formatDateAbsoluteWithOffset(overview.generated_at)}
							>
								{t("overview_generated_at", {
									date: formatDateAbsolute(overview.generated_at),
								})}
							</span>
						</div>
					</section>
				) : (
					<EmptyState
						icon={<Icon name="Presentation" className="size-10" />}
						title={t("overview_empty_title")}
						description={t("overview_empty_desc")}
						action={
							<Button
								variant="outline"
								size="sm"
								onClick={() => void load("refresh")}
							>
								<Icon name="ArrowClockwise" className="size-4" />
								{t("core:refresh")}
							</Button>
						}
					/>
				)}

				{/* 统计独占一行指标带；两个 feed 表（活动/任务）同排 */}
				<OverviewStatsSection loading={loading} overview={overview} />

				<div className="grid gap-8 md:gap-10 xl:grid-cols-[minmax(0,1.05fr)_minmax(0,0.95fr)]">
					<OverviewRecentEventsSection loading={loading} overview={overview} />
					<OverviewBackgroundTasksSection
						loading={loading}
						overview={overview}
					/>
				</div>

				<OverviewDailyReportsSection
					defaultDays={OVERVIEW_TREND_DAYS}
					loading={loading}
					overview={overview}
				/>
			</AdminPageShell>
		</AdminLayout>
	);
}
