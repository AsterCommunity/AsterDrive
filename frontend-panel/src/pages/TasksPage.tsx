import { useCallback, useEffect, useMemo, useReducer } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router-dom";
import { toast } from "sonner";
import { EmptyState } from "@/components/common/EmptyState";
import { AppLayout } from "@/components/layout/AppLayout";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Icon } from "@/components/ui/icon";
import { handleApiError } from "@/hooks/useApiError";
import { usePageTitle } from "@/hooks/usePageTitle";
import { PAGE_SECTION_PADDING_CLASS } from "@/lib/constants";
import { workspaceFolderPath } from "@/lib/workspace";
import { taskService } from "@/services/taskService";
import { useWorkspaceStore } from "@/stores/workspaceStore";
import type { TaskInfo } from "@/types/api";
import { TaskCard } from "./tasks/TaskCard";
import { taskHasExpandableDetails } from "./tasks/taskDetails";
import { ACTIVE_TASK_STATUSES } from "./tasks/taskPresentation";

const PAGE_SIZE = 20;
const TASK_POLL_INTERVAL_MS = 3000;

type TasksPageState = {
	expandedTaskIds: Set<number>;
	loading: boolean;
	page: number;
	retryingTaskId: number | null;
	tasks: TaskInfo[];
	total: number;
};

type TasksPageAction =
	| { type: "load_finish"; tasks: TaskInfo[]; total: number }
	| { type: "loading"; loading: boolean }
	| { type: "page"; page: number }
	| { type: "retrying"; taskId: number | null }
	| { type: "toggle_details"; taskId: number };

const initialTasksPageState: TasksPageState = {
	expandedTaskIds: new Set(),
	loading: true,
	page: 0,
	retryingTaskId: null,
	tasks: [],
	total: 0,
};

function tasksPageReducer(
	state: TasksPageState,
	action: TasksPageAction,
): TasksPageState {
	switch (action.type) {
		case "load_finish":
			return {
				...state,
				tasks: action.tasks,
				total: action.total,
			};
		case "loading":
			return { ...state, loading: action.loading };
		case "page":
			return { ...state, page: action.page };
		case "retrying":
			return { ...state, retryingTaskId: action.taskId };
		case "toggle_details": {
			const expandedTaskIds = new Set(state.expandedTaskIds);
			if (expandedTaskIds.has(action.taskId)) {
				expandedTaskIds.delete(action.taskId);
			} else {
				expandedTaskIds.add(action.taskId);
			}
			return { ...state, expandedTaskIds };
		}
	}
}

export default function TasksPage() {
	const { t } = useTranslation(["core", "tasks"]);
	const navigate = useNavigate();
	const workspace = useWorkspaceStore((s) => s.workspace);
	usePageTitle(t("tasks:title"));
	const [state, dispatch] = useReducer(tasksPageReducer, initialTasksPageState);
	const { expandedTaskIds, loading, page, retryingTaskId, tasks, total } =
		state;

	const loadPage = useCallback(
		async (targetPage: number, options?: { silent?: boolean }) => {
			const silent = options?.silent ?? false;
			try {
				if (!silent) {
					dispatch({ loading: true, type: "loading" });
				}
				const data = await taskService.listInWorkspace({
					limit: PAGE_SIZE,
					offset: targetPage * PAGE_SIZE,
				});
				dispatch({
					tasks: data.items,
					total: data.total,
					type: "load_finish",
				});
				return data;
			} catch (error) {
				if (!silent) {
					handleApiError(error);
				}
				return null;
			} finally {
				if (!silent) {
					dispatch({ loading: false, type: "loading" });
				}
			}
		},
		[],
	);

	useEffect(() => {
		void loadPage(page);
	}, [loadPage, page]);

	const hasActiveTasks = useMemo(
		() => tasks.some((task) => ACTIVE_TASK_STATUSES.has(task.status)),
		[tasks],
	);

	useEffect(() => {
		if (!hasActiveTasks) {
			return;
		}

		const timer = window.setInterval(() => {
			void loadPage(page, { silent: true });
		}, TASK_POLL_INTERVAL_MS);

		return () => window.clearInterval(timer);
	}, [hasActiveTasks, loadPage, page]);

	const totalPages = Math.max(1, Math.ceil(total / PAGE_SIZE));

	const handleRetry = useCallback(
		async (taskId: number) => {
			try {
				dispatch({ taskId, type: "retrying" });
				await taskService.retryTask(taskId);
				toast.success(t("tasks:retry_success"));
				await loadPage(page, { silent: true });
			} catch (error) {
				handleApiError(error);
			} finally {
				dispatch({ taskId: null, type: "retrying" });
			}
		},
		[loadPage, page, t],
	);

	const toggleTaskDetails = useCallback((taskId: number) => {
		dispatch({ taskId, type: "toggle_details" });
	}, []);

	const openTaskTargetFolder = useCallback(
		(targetFolderId: number | null) => {
			navigate(workspaceFolderPath(workspace, targetFolderId), {
				viewTransition: false,
			});
		},
		[navigate, workspace],
	);

	return (
		<AppLayout>
			<div className="flex min-h-0 flex-1 flex-col overflow-auto">
				<div
					className={`mx-auto flex w-full max-w-6xl flex-col gap-5 py-4 md:py-6 ${PAGE_SECTION_PADDING_CLASS}`}
				>
					<div className="flex flex-wrap items-center gap-3">
						<h1 className="text-2xl font-semibold tracking-tight">
							{t("tasks:title")}
						</h1>
						<Button
							variant="ghost"
							size="icon-sm"
							onClick={() => void loadPage(page)}
							disabled={loading}
							aria-label={t("core:refresh")}
							title={t("core:refresh")}
						>
							<Icon
								name={loading ? "Spinner" : "ArrowsClockwise"}
								className={`size-4 ${loading ? "animate-spin" : ""}`}
							/>
						</Button>
						{hasActiveTasks ? (
							<span className="text-sm text-muted-foreground">
								{t("tasks:active_polling_hint")}
							</span>
						) : null}
					</div>

					{loading ? (
						<div className="space-y-3">
							{["task-s1", "task-s2", "task-s3"].map((key) => (
								<Card key={key} className="h-48 animate-pulse bg-muted/20" />
							))}
						</div>
					) : tasks.length === 0 ? (
						<Card className="bg-muted/15">
							<div className="py-12">
								<EmptyState
									icon={<Icon name="Clock" className="size-10" />}
									title={t("tasks:empty_title")}
									description={t("tasks:empty_desc")}
								/>
							</div>
						</Card>
					) : (
						<div className="space-y-3">
							{tasks.map((task) => (
								<TaskCard
									key={task.id}
									task={task}
									detailsExpanded={
										expandedTaskIds.has(task.id) &&
										taskHasExpandableDetails(task)
									}
									retrying={retryingTaskId === task.id}
									onOpenTargetFolder={openTaskTargetFolder}
									onRetry={(taskId) => void handleRetry(taskId)}
									onToggleDetails={toggleTaskDetails}
								/>
							))}
						</div>
					)}

					{tasks.length > 0 ? (
						<div className="flex flex-wrap items-center justify-between gap-3 text-sm text-muted-foreground">
							<span>
								{t("tasks:pagination_desc", {
									current: page + 1,
									total: totalPages,
									count: total,
								})}
							</span>
							<div className="flex items-center gap-2">
								<Button
									variant="outline"
									size="sm"
									onClick={() =>
										dispatch({
											page: Math.max(0, page - 1),
											type: "page",
										})
									}
									disabled={page === 0}
								>
									{t("tasks:prev_page")}
								</Button>
								<Button
									variant="outline"
									size="sm"
									onClick={() =>
										dispatch({
											page: Math.min(totalPages - 1, page + 1),
											type: "page",
										})
									}
									disabled={page >= totalPages - 1}
								>
									{t("tasks:next_page")}
								</Button>
							</div>
						</div>
					) : null}
				</div>
			</div>
		</AppLayout>
	);
}
