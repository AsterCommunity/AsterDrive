import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import {
	ADMIN_INTERACTIVE_TABLE_ROW_CLASS,
	ADMIN_TABLE_BADGE_CELL_CLASS,
	ADMIN_TABLE_MONO_TEXT_CLASS,
	ADMIN_TABLE_TEXT_CELL_CLASS,
	AdminSortableTableHead,
	AdminTableShell,
	AdminTable as Table,
	AdminTableBody as TableBody,
	AdminTableCell as TableCell,
	AdminTableHead as TableHead,
	AdminTableHeader as TableHeader,
	AdminTableRow as TableRow,
} from "@/components/common/AdminTable";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { Icon } from "@/components/ui/icon";
import { formatDateAbsolute, formatDateAbsoluteWithOffset } from "@/lib/format";
import type { SortOrder } from "@/lib/pagination";
import { cn } from "@/lib/utils";
import {
	TaskDetailsContent,
	TaskStepsPreview,
} from "@/pages/tasks/TaskDetailsPanel";
import { taskHasExpandableDetails } from "@/pages/tasks/taskDetails";
import { formatTaskDisplayName } from "@/pages/tasks/taskPresentation";
import type { AdminTaskSortBy } from "@/types/adminSort";
import type {
	BackgroundTaskKind,
	BackgroundTaskStatus,
	TaskInfo,
} from "@/types/api";

interface AdminTaskTableProps {
	formatTaskKind: (kind: BackgroundTaskKind) => string;
	formatTaskSource: (task: TaskInfo) => ReactNode;
	formatTaskStatus: (status: BackgroundTaskStatus) => string;
	items: TaskInfo[];
	detailTaskId: number | null;
	sortBy: AdminTaskSortBy;
	sortOrder: SortOrder;
	onOpenDetail: (taskId: number) => void;
	onOpenDetailChange: (open: boolean) => void;
	onResumeStorageMigration?: (taskId: number) => void;
	resumingTaskId?: number | null;
	onSortChange: (sortBy: AdminTaskSortBy, sortOrder: SortOrder) => void;
}

function getTaskStatusBadgeClass(status: BackgroundTaskStatus) {
	switch (status) {
		case "succeeded":
			return "border-emerald-200 bg-emerald-50 text-emerald-700 dark:border-emerald-900 dark:bg-emerald-950/60 dark:text-emerald-300";
		case "failed":
			return "border-red-200 bg-red-50 text-red-700 dark:border-red-900 dark:bg-red-950/60 dark:text-red-300";
		case "processing":
		case "retry":
			return "border-amber-200 bg-amber-50 text-amber-700 dark:border-amber-900 dark:bg-amber-950/60 dark:text-amber-300";
		case "pending":
			return "border-sky-200 bg-sky-50 text-sky-700 dark:border-sky-900 dark:bg-sky-950/60 dark:text-sky-300";
		case "canceled":
			return "border-border bg-muted/30 text-muted-foreground";
	}
}

function taskExecutionAt(task: TaskInfo) {
	return task.started_at ?? task.created_at;
}

function taskDetail(task: TaskInfo) {
	return task.last_error ?? task.status_text ?? "-";
}

function canResumeStorageMigration(task: TaskInfo) {
	return (
		task.kind === "storage_policy_migration" &&
		task.status === "failed" &&
		task.can_retry
	);
}

export function AdminTaskTable({
	detailTaskId,
	formatTaskKind,
	formatTaskSource,
	formatTaskStatus,
	items,
	onOpenDetail,
	onOpenDetailChange,
	onResumeStorageMigration,
	onSortChange,
	resumingTaskId,
	sortBy,
	sortOrder,
}: AdminTaskTableProps) {
	const { t } = useTranslation(["admin", "core", "tasks"]);
	const detailTask = items.find((task) => task.id === detailTaskId) ?? null;

	return (
		<>
			<AdminTableShell>
				<Table>
					<TableHeader>
						<TableRow>
							<AdminSortableTableHead
								className="w-16"
								sortKey="id"
								sortBy={sortBy}
								sortOrder={sortOrder}
								onSortChange={onSortChange}
							>
								{t("admin:id")}
							</AdminSortableTableHead>
							<AdminSortableTableHead
								className="min-w-[240px]"
								sortKey="display_name"
								sortBy={sortBy}
								sortOrder={sortOrder}
								onSortChange={onSortChange}
							>
								{t("admin:task_name")}
							</AdminSortableTableHead>
							<AdminSortableTableHead
								className="w-[180px]"
								sortKey="kind"
								sortBy={sortBy}
								sortOrder={sortOrder}
								onSortChange={onSortChange}
							>
								{t("core:type")}
							</AdminSortableTableHead>
							<AdminSortableTableHead
								className="w-[160px]"
								sortKey="status"
								sortBy={sortBy}
								sortOrder={sortOrder}
								onSortChange={onSortChange}
							>
								{t("core:status")}
							</AdminSortableTableHead>
							<TableHead className="w-[160px]">
								{t("admin:task_source")}
							</TableHead>
							<AdminSortableTableHead
								className="w-[160px]"
								sortKey="progress"
								sortBy={sortBy}
								sortOrder={sortOrder}
								onSortChange={onSortChange}
							>
								{t("admin:task_progress")}
							</AdminSortableTableHead>
							<AdminSortableTableHead
								className="w-[180px]"
								sortKey="started_at"
								sortBy={sortBy}
								sortOrder={sortOrder}
								onSortChange={onSortChange}
							>
								{t("admin:task_execution_time")}
							</AdminSortableTableHead>
							<TableHead className="min-w-[240px]">
								{t("admin:task_detail")}
							</TableHead>
						</TableRow>
					</TableHeader>
					<TableBody>
						{items.map((task) => {
							const expandable = taskHasExpandableDetails(task);
							const detailContentId = `admin-task-detail-${task.id}`;
							const detailOpen = detailTaskId === task.id;

							return (
								<TableRow
									key={task.id}
									className={cn(
										expandable ? ADMIN_INTERACTIVE_TABLE_ROW_CLASS : undefined,
									)}
									role={expandable ? "button" : undefined}
									aria-expanded={expandable ? detailOpen : undefined}
									aria-controls={expandable ? detailContentId : undefined}
									onClick={() => {
										if (expandable) {
											onOpenDetail(task.id);
										}
									}}
									onKeyDown={(event) => {
										if (!expandable) {
											return;
										}
										if (event.key === "Enter" || event.key === " ") {
											event.preventDefault();
											onOpenDetail(task.id);
										}
									}}
									tabIndex={expandable ? 0 : undefined}
								>
									<TableCell>
										<div className={ADMIN_TABLE_TEXT_CELL_CLASS}>
											<span className={ADMIN_TABLE_MONO_TEXT_CLASS}>
												{task.id}
											</span>
										</div>
									</TableCell>
									<TableCell>
										<div className={ADMIN_TABLE_TEXT_CELL_CLASS}>
											<span className="truncate text-sm font-medium text-foreground">
												{formatTaskDisplayName(t, task)}
											</span>
										</div>
									</TableCell>
									<TableCell>
										<div className={ADMIN_TABLE_BADGE_CELL_CLASS}>
											<Badge variant="outline">
												{formatTaskKind(task.kind)}
											</Badge>
										</div>
									</TableCell>
									<TableCell>
										<div className={ADMIN_TABLE_BADGE_CELL_CLASS}>
											<span
												className={`inline-flex items-center rounded-full border px-2 py-0.5 text-xs font-medium ${getTaskStatusBadgeClass(task.status)}`}
											>
												{formatTaskStatus(task.status)}
											</span>
										</div>
									</TableCell>
									<TableCell>
										<div className={ADMIN_TABLE_TEXT_CELL_CLASS}>
											{formatTaskSource(task)}
										</div>
									</TableCell>
									<TableCell>
										<div className={ADMIN_TABLE_TEXT_CELL_CLASS}>
											<span className="text-sm font-medium text-foreground">
												{task.progress_percent}%
											</span>
										</div>
									</TableCell>
									<TableCell>
										<div className={ADMIN_TABLE_TEXT_CELL_CLASS}>
											<span
												className="whitespace-nowrap text-xs text-muted-foreground"
												title={formatDateAbsoluteWithOffset(
													taskExecutionAt(task),
												)}
											>
												{formatDateAbsolute(taskExecutionAt(task))}
											</span>
										</div>
									</TableCell>
									<TableCell>
										<div className={ADMIN_TABLE_TEXT_CELL_CLASS}>
											<span
												className="truncate text-xs text-muted-foreground"
												title={taskDetail(task)}
											>
												{taskDetail(task)}
											</span>
										</div>
									</TableCell>
								</TableRow>
							);
						})}
					</TableBody>
				</Table>
			</AdminTableShell>

			<Dialog open={detailTask !== null} onOpenChange={onOpenDetailChange}>
				{detailTask ? (
					<DialogContent
						keepMounted
						id={`admin-task-detail-${detailTask.id}`}
						className="flex max-h-[min(860px,calc(100vh-2rem))] flex-col gap-0 overflow-hidden p-0 sm:max-w-[min(1040px,calc(100vw-2rem))]"
					>
						<DialogHeader className="shrink-0 border-b px-6 pt-5 pb-4 pr-14 max-lg:px-4 max-lg:pt-4">
							<DialogTitle className="truncate text-lg">
								{formatTaskDisplayName(t, detailTask)}
							</DialogTitle>
							<div className="flex flex-wrap items-center gap-2 pt-1 text-xs text-muted-foreground">
								<span className={ADMIN_TABLE_MONO_TEXT_CLASS}>
									#{detailTask.id}
								</span>
								<span>{formatTaskKind(detailTask.kind)}</span>
								<span>{formatTaskStatus(detailTask.status)}</span>
							</div>
						</DialogHeader>
						<div className="min-h-0 flex-1 overflow-y-auto px-6 py-4 max-lg:px-4">
							<div className="space-y-3">
								<TaskStepsPreview task={detailTask} />
								<TaskDetailsContent task={detailTask} />
							</div>
						</div>
						{canResumeStorageMigration(detailTask) &&
						onResumeStorageMigration ? (
							<DialogFooter className="shrink-0 border-t px-6 py-3 max-lg:px-4">
								<Button
									type="button"
									onClick={() => onResumeStorageMigration(detailTask.id)}
									disabled={resumingTaskId === detailTask.id}
								>
									<Icon
										name={
											resumingTaskId === detailTask.id
												? "Spinner"
												: "ArrowsClockwise"
										}
										className={`mr-1 size-4 ${
											resumingTaskId === detailTask.id ? "animate-spin" : ""
										}`}
									/>
									{resumingTaskId === detailTask.id
										? t("admin:storage_migration_resuming")
										: t("admin:storage_migration_resume")}
								</Button>
							</DialogFooter>
						) : null}
					</DialogContent>
				) : null}
			</Dialog>
		</>
	);
}
