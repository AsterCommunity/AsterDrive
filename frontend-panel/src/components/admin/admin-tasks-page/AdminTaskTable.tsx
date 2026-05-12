import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import {
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
import { formatDateAbsolute, formatDateAbsoluteWithOffset } from "@/lib/format";
import type { SortOrder } from "@/lib/pagination";
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
	sortBy: AdminTaskSortBy;
	sortOrder: SortOrder;
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

export function AdminTaskTable({
	formatTaskKind,
	formatTaskSource,
	formatTaskStatus,
	items,
	onSortChange,
	sortBy,
	sortOrder,
}: AdminTaskTableProps) {
	const { t } = useTranslation(["admin", "core"]);

	return (
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
					{items.map((task) => (
						<TableRow key={task.id}>
							<TableCell>
								<div className={ADMIN_TABLE_TEXT_CELL_CLASS}>
									<span className={ADMIN_TABLE_MONO_TEXT_CLASS}>{task.id}</span>
								</div>
							</TableCell>
							<TableCell>
								<div className={ADMIN_TABLE_TEXT_CELL_CLASS}>
									<span className="truncate text-sm font-medium text-foreground">
										{task.display_name}
									</span>
								</div>
							</TableCell>
							<TableCell>
								<div className={ADMIN_TABLE_BADGE_CELL_CLASS}>
									<Badge variant="outline">{formatTaskKind(task.kind)}</Badge>
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
										className="text-xs text-muted-foreground whitespace-nowrap"
										title={formatDateAbsoluteWithOffset(taskExecutionAt(task))}
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
					))}
				</TableBody>
			</Table>
		</AdminTableShell>
	);
}
