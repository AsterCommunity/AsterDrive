import type { TFunction } from "i18next";
import type { FormEvent, ReactNode, SetStateAction } from "react";
import { useReducer } from "react";
import { useTranslation } from "react-i18next";
import { useSearchParams } from "react-router-dom";
import { toast } from "sonner";
import { AdminOffsetPagination } from "@/components/admin/AdminOffsetPagination";
import { AdminTaskCleanupDialog } from "@/components/admin/admin-tasks-page/AdminTaskCleanupDialog";
import { AdminTaskFiltersToolbar } from "@/components/admin/admin-tasks-page/AdminTaskFiltersToolbar";
import { AdminTaskTable } from "@/components/admin/admin-tasks-page/AdminTaskTable";
import { EmptyState } from "@/components/common/EmptyState";
import { SkeletonTable } from "@/components/common/SkeletonTable";
import { UserIdentity } from "@/components/common/UserIdentity";
import { AdminLayout } from "@/components/layout/AdminLayout";
import { AdminPageHeader } from "@/components/layout/AdminPageHeader";
import { AdminPageShell } from "@/components/layout/AdminPageShell";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { handleApiError } from "@/hooks/useApiError";
import { useApiList } from "@/hooks/useApiList";
import { usePageTitle } from "@/hooks/usePageTitle";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import { toDateTimeLocalValue, toIsoDateTime } from "@/lib/dateTimeLocal";
import { formatDateTime } from "@/lib/format";
import {
	buildOffsetPaginationSearchParams,
	parseOffsetSearchParam,
	parsePageSizeOption,
	parsePageSizeSearchParam,
	parseSortOrderSearchParam,
	parseSortSearchParam,
	type SortOrder,
} from "@/lib/pagination";
import { formatTaskKind as formatSharedTaskKind } from "@/pages/tasks/taskPresentation";
import { adminTaskService } from "@/services/adminService";
import type { AdminTaskSortBy } from "@/types/adminSort";
import type {
	BackgroundTaskKind,
	BackgroundTaskStatus,
	TaskInfo,
} from "@/types/api";

const TASK_PAGE_SIZE_OPTIONS = [20, 50, 100] as const;
const DEFAULT_TASK_PAGE_SIZE = 20 as const;
const TASK_MANAGED_QUERY_KEYS = [
	"kind",
	"offset",
	"pageSize",
	"sortBy",
	"sortOrder",
	"status",
] as const;
const TASK_SORT_BY_OPTIONS = [
	"id",
	"display_name",
	"kind",
	"status",
	"progress",
	"created_at",
	"updated_at",
	"started_at",
	"finished_at",
] as const satisfies readonly AdminTaskSortBy[];
const DEFAULT_TASK_SORT_BY = "updated_at" as const satisfies AdminTaskSortBy;
const DEFAULT_TASK_SORT_ORDER = "desc" as const satisfies SortOrder;
const TASK_KIND_FILTER_VALUES = [
	"archive_extract",
	"archive_compress",
	"archive_preview_generate",
	"thumbnail_generate",
	"trash_purge_all",
	"storage_policy_migration",
	"blob_maintenance",
	"system_runtime",
] as const;
const TASK_STATUS_FILTER_VALUES = [
	"pending",
	"processing",
	"retry",
	"succeeded",
	"failed",
	"canceled",
] as const;
const TASK_TERMINAL_STATUS_FILTER_VALUES = [
	"succeeded",
	"failed",
	"canceled",
] as const;
const DEFAULT_TASK_CLEANUP_LOOKBACK_HOURS = 24;

type TaskKindFilter = "__all__" | BackgroundTaskKind;
type TaskStatusFilter = "__all__" | BackgroundTaskStatus;
type TaskTerminalStatusFilter =
	| "__all__"
	| (typeof TASK_TERMINAL_STATUS_FILTER_VALUES)[number];
type TaskCleanupRequest = {
	finished_before: string;
	kind?: BackgroundTaskKind;
	status?: (typeof TASK_TERMINAL_STATUS_FILTER_VALUES)[number];
};
type ManagedTaskQuery = {
	offset: number;
	pageSize: (typeof TASK_PAGE_SIZE_OPTIONS)[number];
	kind: TaskKindFilter;
	status: TaskStatusFilter;
	sortBy: AdminTaskSortBy;
	sortOrder: SortOrder;
};
type AdminTasksUiState = {
	cleanupDialogOpen: boolean;
	cleanupFinishedBefore: string;
	cleanupKindFilter: TaskKindFilter;
	cleanupStatusFilter: TaskTerminalStatusFilter;
	cleanupSubmitting: boolean;
	detailDialogTaskId: number | null;
	resumingStorageMigrationTaskId: number | null;
};
type AdminTasksUiAction =
	| { open: boolean; type: "set_cleanup_dialog_open" }
	| { taskId: number | null; type: "set_detail_dialog_task" }
	| { taskId: number | null; type: "set_resuming_storage_migration_task" }
	| { value: string; type: "set_cleanup_finished_before" }
	| { value: TaskKindFilter; type: "set_cleanup_kind_filter" }
	| { value: TaskTerminalStatusFilter; type: "set_cleanup_status_filter" }
	| { submitting: boolean; type: "set_cleanup_submitting" }
	| { type: "reset_cleanup_conditions" };

function normalizeOffset(offset: number) {
	return Math.max(0, Math.floor(offset));
}

function parseTaskKindSearchParam(value: string | null): TaskKindFilter {
	return TASK_KIND_FILTER_VALUES.includes(
		value as (typeof TASK_KIND_FILTER_VALUES)[number],
	)
		? (value as BackgroundTaskKind)
		: "__all__";
}

function parseTaskStatusSearchParam(value: string | null): TaskStatusFilter {
	return TASK_STATUS_FILTER_VALUES.includes(
		value as (typeof TASK_STATUS_FILTER_VALUES)[number],
	)
		? (value as BackgroundTaskStatus)
		: "__all__";
}

function buildManagedTaskSearchParams({
	offset,
	pageSize,
	kind,
	status,
	sortBy,
	sortOrder,
}: ManagedTaskQuery) {
	return buildOffsetPaginationSearchParams({
		offset,
		pageSize,
		defaultPageSize: DEFAULT_TASK_PAGE_SIZE,
		extraParams: {
			kind: kind !== "__all__" ? kind : undefined,
			sortBy: sortBy !== DEFAULT_TASK_SORT_BY ? sortBy : undefined,
			sortOrder: sortOrder !== DEFAULT_TASK_SORT_ORDER ? sortOrder : undefined,
			status: status !== "__all__" ? status : undefined,
		},
	});
}

function readManagedTaskQuery(searchParams: URLSearchParams): ManagedTaskQuery {
	return {
		offset: normalizeOffset(parseOffsetSearchParam(searchParams.get("offset"))),
		pageSize: parsePageSizeSearchParam(
			searchParams.get("pageSize"),
			TASK_PAGE_SIZE_OPTIONS,
			DEFAULT_TASK_PAGE_SIZE,
		),
		kind: parseTaskKindSearchParam(searchParams.get("kind")),
		status: parseTaskStatusSearchParam(searchParams.get("status")),
		sortBy: parseSortSearchParam(
			searchParams.get("sortBy"),
			TASK_SORT_BY_OPTIONS,
			DEFAULT_TASK_SORT_BY,
		),
		sortOrder: parseSortOrderSearchParam(
			searchParams.get("sortOrder"),
			DEFAULT_TASK_SORT_ORDER,
		),
	};
}

function mergeManagedTaskSearchParams(
	searchParams: URLSearchParams,
	managedSearchParams: URLSearchParams,
) {
	const merged = new URLSearchParams(searchParams);
	for (const key of TASK_MANAGED_QUERY_KEYS) {
		merged.delete(key);
	}
	for (const [key, value] of managedSearchParams.entries()) {
		merged.set(key, value);
	}
	return merged;
}

function defaultCleanupFinishedBeforeValue() {
	return toDateTimeLocalValue(
		new Date(
			Date.now() - DEFAULT_TASK_CLEANUP_LOOKBACK_HOURS * 60 * 60 * 1000,
		).toISOString(),
	);
}

function createInitialAdminTasksUiState(): AdminTasksUiState {
	return {
		cleanupDialogOpen: false,
		cleanupFinishedBefore: defaultCleanupFinishedBeforeValue(),
		cleanupKindFilter: "__all__",
		cleanupStatusFilter: "__all__",
		cleanupSubmitting: false,
		detailDialogTaskId: null,
		resumingStorageMigrationTaskId: null,
	};
}

function adminTasksUiReducer(
	state: AdminTasksUiState,
	action: AdminTasksUiAction,
): AdminTasksUiState {
	switch (action.type) {
		case "set_cleanup_dialog_open":
			return { ...state, cleanupDialogOpen: action.open };
		case "set_detail_dialog_task":
			return { ...state, detailDialogTaskId: action.taskId };
		case "set_resuming_storage_migration_task":
			return { ...state, resumingStorageMigrationTaskId: action.taskId };
		case "set_cleanup_finished_before":
			return { ...state, cleanupFinishedBefore: action.value };
		case "set_cleanup_kind_filter":
			return { ...state, cleanupKindFilter: action.value };
		case "set_cleanup_status_filter":
			return { ...state, cleanupStatusFilter: action.value };
		case "set_cleanup_submitting":
			return { ...state, cleanupSubmitting: action.submitting };
		case "reset_cleanup_conditions":
			return {
				...state,
				cleanupFinishedBefore: defaultCleanupFinishedBeforeValue(),
				cleanupKindFilter: "__all__",
				cleanupStatusFilter: "__all__",
			};
	}
}

function formatTaskStatusLabel(t: TFunction, status: BackgroundTaskStatus) {
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
}

function formatAdminTaskKind(t: TFunction, kind: BackgroundTaskKind) {
	return formatSharedTaskKind(t, kind);
}

function formatAdminTaskSource(t: TFunction, task: TaskInfo): ReactNode {
	if (task.team_id != null) {
		return t("admin:overview_background_tasks_source_team", {
			id: task.team_id,
		});
	}
	if (task.creator) {
		return <UserIdentity user={task.creator} />;
	}
	return t("admin:overview_background_tasks_source_system");
}

function buildTaskKindFilterOptions(t: TFunction) {
	return [
		{ label: t("admin:all_task_types"), value: "__all__" },
		...TASK_KIND_FILTER_VALUES.map((value) => ({
			label: formatAdminTaskKind(t, value),
			value,
		})),
	] satisfies ReadonlyArray<{ label: string; value: string }>;
}

function buildTaskStatusFilterOptions(t: TFunction) {
	return [
		{ label: t("admin:all_task_statuses"), value: "__all__" },
		...TASK_STATUS_FILTER_VALUES.map((value) => ({
			label: formatTaskStatusLabel(t, value),
			value,
		})),
	] satisfies ReadonlyArray<{ label: string; value: string }>;
}

function buildCleanupStatusFilterOptions(t: TFunction) {
	return [
		{ label: t("admin:all_completed_task_statuses"), value: "__all__" },
		...TASK_TERMINAL_STATUS_FILTER_VALUES.map((value) => ({
			label: formatTaskStatusLabel(t, value),
			value,
		})),
	] satisfies ReadonlyArray<{ label: string; value: string }>;
}

function buildCleanupRequest(
	cleanupFinishedBefore: string,
	cleanupKindFilter: TaskKindFilter,
	cleanupStatusFilter: TaskTerminalStatusFilter,
): TaskCleanupRequest | null {
	const finishedBefore = toIsoDateTime(cleanupFinishedBefore);
	if (finishedBefore == null) {
		return null;
	}
	return {
		finished_before: finishedBefore,
		...(cleanupKindFilter !== "__all__" ? { kind: cleanupKindFilter } : {}),
		...(cleanupStatusFilter !== "__all__"
			? { status: cleanupStatusFilter }
			: {}),
	};
}

function describeCleanupConditions(
	t: TFunction,
	request: TaskCleanupRequest | null,
) {
	if (request == null) {
		return t("admin:task_cleanup_confirm_desc_invalid");
	}
	return t("admin:task_cleanup_confirm_desc", {
		finishedBefore: formatDateTime(request.finished_before),
		kind:
			request.kind != null
				? formatAdminTaskKind(t, request.kind)
				: t("admin:all_task_types"),
		status:
			request.status != null
				? formatTaskStatusLabel(t, request.status)
				: t("admin:all_completed_task_statuses"),
	});
}

function useAdminTasksPageContent() {
	const { t } = useTranslation(["admin", "tasks", "core"]);
	usePageTitle(t("admin:tasks"));
	const [searchParams, setSearchParams] = useSearchParams();
	const taskQuery = readManagedTaskQuery(searchParams);
	const {
		kind: kindFilter,
		offset,
		pageSize,
		sortBy,
		sortOrder,
		status: statusFilter,
	} = taskQuery;
	const [uiState, dispatchUi] = useReducer(
		adminTasksUiReducer,
		undefined,
		createInitialAdminTasksUiState,
	);
	const {
		cleanupDialogOpen,
		cleanupFinishedBefore,
		cleanupKindFilter,
		cleanupStatusFilter,
		cleanupSubmitting,
		detailDialogTaskId,
		resumingStorageMigrationTaskId,
	} = uiState;
	const setTaskQuery = (updates: Partial<ManagedTaskQuery>) => {
		const nextManagedSearchParams = buildManagedTaskSearchParams({
			...taskQuery,
			...updates,
		});
		setSearchParams(
			mergeManagedTaskSearchParams(searchParams, nextManagedSearchParams),
			{ replace: true },
		);
	};
	const setOffset = (value: SetStateAction<number>) => {
		setTaskQuery({
			offset: normalizeOffset(
				typeof value === "function" ? value(offset) : value,
			),
		});
	};

	const { items, loading, reload, total } = useApiList(
		() =>
			adminTaskService.list({
				limit: pageSize,
				offset,
				...(kindFilter !== "__all__" ? { kind: kindFilter } : {}),
				...(statusFilter !== "__all__" ? { status: statusFilter } : {}),
				sort_by: sortBy,
				sort_order: sortOrder,
			}),
		[kindFilter, offset, pageSize, sortBy, sortOrder, statusFilter],
	);

	const activeFilterCount =
		(kindFilter !== "__all__" ? 1 : 0) + (statusFilter !== "__all__" ? 1 : 0);
	const hasServerFilters = activeFilterCount > 0;
	const totalPages = Math.max(1, Math.ceil(total / pageSize));
	const currentPage = Math.floor(offset / pageSize) + 1;
	const prevPageDisabled = offset === 0;
	const nextPageDisabled = offset + pageSize >= total;
	const visibleDetailTaskId =
		detailDialogTaskId != null &&
		items.some((task) => task.id === detailDialogTaskId)
			? detailDialogTaskId
			: null;
	const pageSizeOptions = TASK_PAGE_SIZE_OPTIONS.map((size) => ({
		label: t("admin:page_size_option", { count: size }),
		value: String(size),
	}));

	const taskKindFilterOptions = buildTaskKindFilterOptions(t);
	const taskStatusFilterOptions = buildTaskStatusFilterOptions(t);
	const cleanupStatusFilterOptions = buildCleanupStatusFilterOptions(t);

	const resetFilters = () => {
		setTaskQuery({ kind: "__all__", offset: 0, status: "__all__" });
	};

	const resetCleanupConditions = () => {
		dispatchUi({ type: "reset_cleanup_conditions" });
	};

	const handlePageSizeChange = (value: string | null) => {
		const next = parsePageSizeOption(value, TASK_PAGE_SIZE_OPTIONS);
		if (next == null) return;
		setTaskQuery({ offset: 0, pageSize: next });
	};

	const handleKindFilterChange = (value: string | null) => {
		setTaskQuery({
			kind: value === "__all__" ? "__all__" : parseTaskKindSearchParam(value),
			offset: 0,
		});
	};

	const handleStatusFilterChange = (value: string | null) => {
		setTaskQuery({
			offset: 0,
			status:
				value === "__all__" ? "__all__" : parseTaskStatusSearchParam(value),
		});
	};

	const handleSortChange = (
		nextSortBy: AdminTaskSortBy,
		nextOrder: SortOrder,
	) => {
		setTaskQuery({ offset: 0, sortBy: nextSortBy, sortOrder: nextOrder });
	};

	const cleanupRequest = buildCleanupRequest(
		cleanupFinishedBefore,
		cleanupKindFilter,
		cleanupStatusFilter,
	);

	const handleCleanupSubmit = async (event: FormEvent<HTMLFormElement>) => {
		event.preventDefault();
		if (cleanupRequest == null) {
			return;
		}

		dispatchUi({ submitting: true, type: "set_cleanup_submitting" });
		try {
			const result = await adminTaskService.cleanupCompleted(cleanupRequest);
			toast.success(t("admin:tasks_cleaned", { count: result.removed }));
			dispatchUi({ open: false, type: "set_cleanup_dialog_open" });
			if (offset !== 0) {
				setOffset(0);
			} else {
				await reload();
			}
		} catch (error) {
			handleApiError(error);
		} finally {
			dispatchUi({ submitting: false, type: "set_cleanup_submitting" });
		}
	};

	const handleResumeStorageMigration = async (taskId: number) => {
		if (resumingStorageMigrationTaskId !== null) {
			return;
		}

		dispatchUi({
			taskId,
			type: "set_resuming_storage_migration_task",
		});
		try {
			await adminTaskService.resumeStoragePolicyMigration(taskId);
			toast.success(t("admin:storage_migration_resume_queued"));
			await reload();
		} catch (error) {
			handleApiError(error);
		} finally {
			dispatchUi({
				taskId: null,
				type: "set_resuming_storage_migration_task",
			});
		}
	};

	return (
		<AdminLayout>
			<AdminPageShell>
				<AdminPageHeader
					title={t("admin:tasks")}
					description={t("admin:tasks_intro")}
					actions={
						<>
							<Button
								variant="outline"
								size="sm"
								className={ADMIN_CONTROL_HEIGHT_CLASS}
								onClick={() =>
									dispatchUi({ open: true, type: "set_cleanup_dialog_open" })
								}
								disabled={cleanupSubmitting}
							>
								<Icon name="Trash" className="mr-1 size-3.5" />
								{t("admin:task_cleanup_action")}
							</Button>
							<Button
								variant="outline"
								size="sm"
								className={ADMIN_CONTROL_HEIGHT_CLASS}
								onClick={() => void reload()}
								disabled={loading || cleanupSubmitting}
							>
								<Icon
									name={loading ? "Spinner" : "ArrowsClockwise"}
									className={`mr-1 size-3.5 ${loading ? "animate-spin" : ""}`}
								/>
								{t("core:refresh")}
							</Button>
						</>
					}
					toolbar={
						<AdminTaskFiltersToolbar
							activeFilterCount={activeFilterCount}
							hasServerFilters={hasServerFilters}
							kindFilter={kindFilter}
							kindOptions={taskKindFilterOptions}
							onKindChange={handleKindFilterChange}
							onResetFilters={resetFilters}
							onStatusChange={handleStatusFilterChange}
							statusFilter={statusFilter}
							statusOptions={taskStatusFilterOptions}
						/>
					}
				/>

				{loading ? (
					<SkeletonTable columns={8} rows={6} />
				) : items.length === 0 ? (
					hasServerFilters ? (
						<EmptyState
							icon={<Icon name="Clock" className="size-10" />}
							title={t("admin:no_filtered_tasks")}
							description={t("admin:no_filtered_tasks_desc")}
							action={
								<Button variant="outline" onClick={resetFilters}>
									{t("admin:clear_filters")}
								</Button>
							}
						/>
					) : (
						<EmptyState
							icon={<Icon name="Clock" className="size-10" />}
							title={t("admin:no_tasks")}
							description={t("admin:no_tasks_desc")}
						/>
					)
				) : (
					<AdminTaskTable
						items={items}
						detailTaskId={visibleDetailTaskId}
						formatTaskKind={(kind) => formatAdminTaskKind(t, kind)}
						formatTaskSource={(task) => formatAdminTaskSource(t, task)}
						formatTaskStatus={(status) => formatTaskStatusLabel(t, status)}
						onOpenDetail={(taskId) =>
							dispatchUi({
								taskId,
								type: "set_detail_dialog_task",
							})
						}
						onOpenDetailChange={(open) => {
							if (!open) {
								dispatchUi({
									taskId: null,
									type: "set_detail_dialog_task",
								});
							}
						}}
						onResumeStorageMigration={(taskId) =>
							void handleResumeStorageMigration(taskId)
						}
						resumingTaskId={resumingStorageMigrationTaskId}
						sortBy={sortBy}
						sortOrder={sortOrder}
						onSortChange={handleSortChange}
					/>
				)}

				<AdminOffsetPagination
					total={total}
					currentPage={currentPage}
					totalPages={totalPages}
					pageSize={String(pageSize)}
					pageSizeOptions={pageSizeOptions}
					onPageSizeChange={handlePageSizeChange}
					prevDisabled={prevPageDisabled}
					nextDisabled={nextPageDisabled}
					onPrevious={() =>
						setOffset((current) => Math.max(0, current - pageSize))
					}
					onNext={() => setOffset((current) => current + pageSize)}
				/>
			</AdminPageShell>

			<AdminTaskCleanupDialog
				description={describeCleanupConditions(t, cleanupRequest)}
				finishedBefore={cleanupFinishedBefore}
				kindFilter={cleanupKindFilter}
				kindOptions={taskKindFilterOptions}
				onFinishedBeforeChange={(value) =>
					dispatchUi({
						type: "set_cleanup_finished_before",
						value,
					})
				}
				onKindFilterChange={(value) =>
					dispatchUi({
						type: "set_cleanup_kind_filter",
						value:
							value == null || value === "__all__"
								? "__all__"
								: parseTaskKindSearchParam(value),
					})
				}
				onOpenChange={(open) =>
					dispatchUi({ open, type: "set_cleanup_dialog_open" })
				}
				onResetConditions={resetCleanupConditions}
				onStatusFilterChange={(value) =>
					dispatchUi({
						type: "set_cleanup_status_filter",
						value:
							value == null || value === "__all__"
								? "__all__"
								: (value as TaskTerminalStatusFilter),
					})
				}
				onSubmit={handleCleanupSubmit}
				open={cleanupDialogOpen}
				statusFilter={cleanupStatusFilter}
				statusOptions={cleanupStatusFilterOptions}
				submitDisabled={cleanupRequest == null || cleanupSubmitting}
				submitting={cleanupSubmitting}
			/>
		</AdminLayout>
	);
}

export default function AdminTasksPage() {
	return useAdminTasksPageContent();
}
