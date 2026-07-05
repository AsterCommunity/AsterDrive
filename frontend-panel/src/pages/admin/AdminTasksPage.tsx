import type { TFunction } from "i18next";
import type { FormEvent, ReactNode, SetStateAction } from "react";
import { useReducer } from "react";
import { useTranslation } from "react-i18next";
import { useSearchParams } from "react-router-dom";
import { toast } from "sonner";
import { AdminOffsetPagination } from "@/components/admin/AdminOffsetPagination";
import { AdminTaskCleanupDialog } from "@/components/admin/admin-tasks-page/AdminTaskCleanupDialog";
import { AdminTaskFiltersToolbar } from "@/components/admin/admin-tasks-page/AdminTaskFiltersToolbar";
import {
	AdminTaskDetailDialog,
	AdminTaskTableHeader,
	AdminTaskTableRow,
} from "@/components/admin/admin-tasks-page/AdminTaskTable";
import { AdminTableList } from "@/components/common/AdminTableList";
import { UserIdentity } from "@/components/common/UserIdentity";
import { AdminLayout } from "@/components/layout/AdminLayout";
import { AdminPageHeader } from "@/components/layout/AdminPageHeader";
import { AdminPageShell } from "@/components/layout/AdminPageShell";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { handleApiError } from "@/hooks/useApiError";
import { useManagedAdminList } from "@/hooks/useManagedAdminList";
import {
	type ManagedListQuerySchema,
	managedOffsetQueryField,
	managedPageSizeQueryField,
	managedSortByQueryField,
	managedSortOrderQueryField,
	useManagedListQueryState,
} from "@/hooks/useManagedListQueryState";
import { usePageTitle } from "@/hooks/usePageTitle";
import { ADMIN_CONTROL_HEIGHT_CLASS } from "@/lib/constants";
import { toDateTimeLocalValue, toIsoDateTime } from "@/lib/dateTimeLocal";
import { formatDateTime } from "@/lib/format";
import { parsePageSizeOption, type SortOrder } from "@/lib/pagination";
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
type KnownTaskKind = BackgroundTaskKind | "image_preview_generate";
const TASK_KIND_FILTER_VALUES = [
	"archive_extract",
	"archive_compress",
	"archive_preview_generate",
	"thumbnail_generate",
	"image_preview_generate",
	"media_metadata_extract",
	"trash_purge_all",
	"storage_policy_temp_cleanup",
	"storage_policy_migration",
	"blob_maintenance",
	"system_runtime",
] as const satisfies readonly KnownTaskKind[];
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

type TaskKindFilter = "__all__" | KnownTaskKind;
type TaskStatusFilter = "__all__" | BackgroundTaskStatus;
type TaskTerminalStatusFilter =
	| "__all__"
	| (typeof TASK_TERMINAL_STATUS_FILTER_VALUES)[number];
type TaskCleanupRequest = {
	finished_before: string;
	kind?: KnownTaskKind;
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

function parseTaskKindSearchParam(value: string | null): TaskKindFilter {
	return TASK_KIND_FILTER_VALUES.includes(
		value as (typeof TASK_KIND_FILTER_VALUES)[number],
	)
		? (value as KnownTaskKind)
		: "__all__";
}

function parseTaskStatusSearchParam(value: string | null): TaskStatusFilter {
	return TASK_STATUS_FILTER_VALUES.includes(
		value as (typeof TASK_STATUS_FILTER_VALUES)[number],
	)
		? (value as BackgroundTaskStatus)
		: "__all__";
}

const MANAGED_TASK_QUERY_DEFAULTS = {
	kind: "__all__",
	offset: 0,
	pageSize: DEFAULT_TASK_PAGE_SIZE,
	sortBy: DEFAULT_TASK_SORT_BY,
	sortOrder: DEFAULT_TASK_SORT_ORDER,
	status: "__all__",
} satisfies ManagedTaskQuery;

const MANAGED_TASK_QUERY_SCHEMA = {
	kind: {
		keys: ["kind"],
		parse: (searchParams) => parseTaskKindSearchParam(searchParams.get("kind")),
		serialize: (value) => (value !== "__all__" ? value : undefined),
	},
	offset: managedOffsetQueryField(),
	pageSize: managedPageSizeQueryField(
		TASK_PAGE_SIZE_OPTIONS,
		DEFAULT_TASK_PAGE_SIZE,
	),
	sortBy: managedSortByQueryField(TASK_SORT_BY_OPTIONS, DEFAULT_TASK_SORT_BY),
	sortOrder: managedSortOrderQueryField(DEFAULT_TASK_SORT_ORDER),
	status: {
		keys: ["status"],
		parse: (searchParams) =>
			parseTaskStatusSearchParam(searchParams.get("status")),
		serialize: (value) => (value !== "__all__" ? value : undefined),
	},
} satisfies ManagedListQuerySchema<ManagedTaskQuery>;

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

function formatKnownTaskKind(t: TFunction, kind: KnownTaskKind) {
	return formatSharedTaskKind(t, kind as BackgroundTaskKind);
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
			label: formatKnownTaskKind(t, value),
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
				? formatKnownTaskKind(t, request.kind)
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
	const { query: taskQuery, setQuery: setTaskQuery } = useManagedListQueryState(
		{
			defaults: MANAGED_TASK_QUERY_DEFAULTS,
			schema: MANAGED_TASK_QUERY_SCHEMA,
			searchParams,
			setSearchParams,
		},
	);
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
	const setOffset = (value: SetStateAction<number>) => {
		setTaskQuery((current) => ({
			offset: Math.max(
				0,
				typeof value === "function" ? value(current.offset) : value,
			),
		}));
	};

	const {
		currentPage,
		items,
		loading,
		nextPageDisabled,
		prevPageDisabled,
		reload,
		total,
		totalPages,
	} = useManagedAdminList<TaskInfo, ManagedTaskQuery>({
		deps: [kindFilter, offset, pageSize, sortBy, sortOrder, statusFilter],
		loadPage: (query) =>
			adminTaskService.list({
				limit: query.pageSize,
				offset: query.offset,
				...(query.kind !== "__all__"
					? { kind: query.kind as BackgroundTaskKind }
					: {}),
				...(query.status !== "__all__" ? { status: query.status } : {}),
				sort_by: query.sortBy,
				sort_order: query.sortOrder,
			}),
		query: taskQuery,
		setOffset,
	});

	const activeFilterCount =
		(kindFilter !== "__all__" ? 1 : 0) + (statusFilter !== "__all__" ? 1 : 0);
	const hasServerFilters = activeFilterCount > 0;
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
			const result = await adminTaskService.cleanupCompleted({
				...cleanupRequest,
				kind:
					cleanupRequest.kind != null
						? (cleanupRequest.kind as BackgroundTaskKind)
						: undefined,
			});
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
	const detailTask =
		items.find((task) => task.id === visibleDetailTaskId) ?? null;
	const taskEmptyIcon = <Icon name="Clock" className="size-10" />;
	const taskFilteredEmptyAction = (
		<Button variant="outline" onClick={resetFilters}>
			{t("admin:clear_filters")}
		</Button>
	);
	const taskTableHeader = (
		<AdminTaskTableHeader
			sortBy={sortBy}
			sortOrder={sortOrder}
			onSortChange={handleSortChange}
		/>
	);
	const taskPagination = (
		<AdminOffsetPagination
			total={total}
			currentPage={currentPage}
			totalPages={totalPages}
			pageSize={String(pageSize)}
			pageSizeOptions={pageSizeOptions}
			onPageSizeChange={handlePageSizeChange}
			prevDisabled={prevPageDisabled}
			nextDisabled={nextPageDisabled}
			onPrevious={() => setOffset((current) => Math.max(0, current - pageSize))}
			onNext={() => setOffset((current) => current + pageSize)}
		/>
	);

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

				<AdminTableList
					loading={loading}
					items={items}
					columns={8}
					rows={6}
					emptyIcon={taskEmptyIcon}
					emptyTitle={t("admin:no_tasks")}
					emptyDescription={t("admin:no_tasks_desc")}
					filtered={hasServerFilters}
					filteredEmptyTitle={t("admin:no_filtered_tasks")}
					filteredEmptyDescription={t("admin:no_filtered_tasks_desc")}
					filteredEmptyAction={taskFilteredEmptyAction}
					headerRow={taskTableHeader}
					pagination={taskPagination}
					renderRow={(task) => (
						<AdminTaskTableRow
							key={task.id}
							detailTaskId={visibleDetailTaskId}
							formatTaskKind={(kind) => formatAdminTaskKind(t, kind)}
							formatTaskSource={(item) => formatAdminTaskSource(t, item)}
							formatTaskStatus={(status) => formatTaskStatusLabel(t, status)}
							onOpenDetail={(taskId) =>
								dispatchUi({
									taskId,
									type: "set_detail_dialog_task",
								})
							}
							task={task}
						/>
					)}
				/>
			</AdminPageShell>

			<AdminTaskDetailDialog
				detailTask={detailTask}
				formatTaskKind={(kind) => formatAdminTaskKind(t, kind)}
				formatTaskStatus={(status) => formatTaskStatusLabel(t, status)}
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
			/>

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
