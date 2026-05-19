import { useTranslation } from "react-i18next";
import {
	AdminTable as Table,
	AdminTableBody as TableBody,
	AdminTableCell as TableCell,
	AdminTableHead as TableHead,
	AdminTableHeader as TableHeader,
	AdminTableRow as TableRow,
} from "@/components/common/AdminTable";
import { EmptyState } from "@/components/common/EmptyState";
import { SkeletonTable } from "@/components/common/SkeletonTable";
import { UserIdentity } from "@/components/common/UserIdentity";
import { AdminSurface } from "@/components/layout/AdminSurface";
import { Badge } from "@/components/ui/badge";
import { Icon } from "@/components/ui/icon";
import { PAGE_SECTION_PADDING_CLASS } from "@/lib/constants";
import { formatDateAbsolute, formatDateAbsoluteWithOffset } from "@/lib/format";
import { cn } from "@/lib/utils";
import type {
	AdminOverview,
	BackgroundTaskKind,
	BackgroundTaskStatus,
} from "@/types/api";
import {
	type BackgroundTaskEvent,
	backgroundTaskEventTime,
	formatOverviewRuntimeDuration,
	getOverviewBackgroundTaskStatusBadgeClass,
} from "./overviewPresentation";

interface OverviewBackgroundTasksSectionProps {
	loading: boolean;
	overview: AdminOverview | null;
}

export function OverviewBackgroundTasksSection({
	loading,
	overview,
}: OverviewBackgroundTasksSectionProps) {
	const { t } = useTranslation(["admin", "tasks"]);

	const formatBackgroundTaskStatus = (status: BackgroundTaskStatus) => {
		switch (status) {
			case "pending":
				return t("tasks:status_pending");
			case "processing":
				return t("tasks:status_processing");
			case "retry":
				return t("tasks:status_retry");
			case "succeeded":
				return t("tasks:status_succeeded");
			case "failed":
				return t("tasks:status_failed");
			case "canceled":
				return t("tasks:status_canceled");
		}
	};

	const formatBackgroundTaskKind = (kind: BackgroundTaskKind) => {
		switch (kind) {
			case "archive_extract":
				return t("tasks:kind_archive_extract");
			case "archive_compress":
				return t("tasks:kind_archive_compress");
			case "archive_preview_generate":
				return t("tasks:kind_archive_preview_generate");
			case "thumbnail_generate":
				return t("tasks:kind_thumbnail_generate");
			case "trash_purge_all":
				return t("tasks:kind_trash_purge_all");
			case "system_runtime":
				return t("tasks:kind_system_runtime");
			default:
				return String(kind).replaceAll("_", " ");
		}
	};

	const formatBackgroundTaskSource = (task: BackgroundTaskEvent) => {
		if (task.team_id != null) {
			return t("overview_background_tasks_source_team", { id: task.team_id });
		}
		if (task.creator) {
			return <UserIdentity user={task.creator} />;
		}
		return t("overview_background_tasks_source_system");
	};

	return (
		<AdminSurface padded={false} className="min-h-0 overflow-hidden">
			<div className={cn("border-b py-4", PAGE_SECTION_PADDING_CLASS)}>
				<h3 className="text-base font-semibold">
					{t("overview_background_tasks")}
				</h3>
				<p className="mt-1 text-sm text-muted-foreground">
					{t("overview_background_tasks_desc")}
				</p>
			</div>

			{loading && !overview ? (
				<div className="py-4 md:py-6">
					<SkeletonTable columns={5} rows={6} />
				</div>
			) : overview?.recent_background_tasks.length ? (
				<Table>
					<TableHeader>
						<TableRow>
							<TableHead>{t("overview_background_tasks_time")}</TableHead>
							<TableHead>{t("overview_background_tasks_task")}</TableHead>
							<TableHead>{t("overview_background_tasks_status")}</TableHead>
							<TableHead>{t("overview_background_tasks_source")}</TableHead>
							<TableHead>{t("overview_background_tasks_detail")}</TableHead>
						</TableRow>
					</TableHeader>
					<TableBody>
						{overview.recent_background_tasks.map((task) => {
							const duration = formatOverviewRuntimeDuration(task.duration_ms);
							const detail = task.last_error ?? task.status_text ?? "---";

							return (
								<TableRow key={task.id}>
									<TableCell
										className="text-xs text-muted-foreground whitespace-nowrap"
										title={formatDateAbsoluteWithOffset(
											backgroundTaskEventTime(task),
										)}
									>
										{formatDateAbsolute(backgroundTaskEventTime(task))}
									</TableCell>
									<TableCell>
										<div className="flex flex-col gap-1">
											<span className="text-sm font-medium">
												{task.display_name}
											</span>
											<span className="text-xs text-muted-foreground">
												{formatBackgroundTaskKind(task.kind)}
												{duration
													? ` · ${t("overview_background_tasks_duration", {
															duration,
														})}`
													: ""}
											</span>
										</div>
									</TableCell>
									<TableCell>
										<Badge
											variant="outline"
											className={getOverviewBackgroundTaskStatusBadgeClass(
												task.status,
											)}
										>
											{formatBackgroundTaskStatus(task.status)}
										</Badge>
									</TableCell>
									<TableCell>{formatBackgroundTaskSource(task)}</TableCell>
									<TableCell className="text-sm text-muted-foreground">
										{detail}
									</TableCell>
								</TableRow>
							);
						})}
					</TableBody>
				</Table>
			) : (
				<EmptyState
					icon={<Icon name="Clock" className="h-10 w-10" />}
					title={t("overview_background_tasks_empty")}
					description={t("overview_background_tasks_empty_desc")}
				/>
			)}
		</AdminSurface>
	);
}
