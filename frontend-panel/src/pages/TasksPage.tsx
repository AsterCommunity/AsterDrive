import { useCallback, useEffect, useMemo, useState } from "react";
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
import { taskHasExpandableDetails } from "./tasks/TaskDetailsPanel";
import { ACTIVE_TASK_STATUSES } from "./tasks/taskPresentation";

const PAGE_SIZE = 20;
const TASK_POLL_INTERVAL_MS = 3000;

export default function TasksPage() {
	const { t } = useTranslation(["core", "tasks"]);
	const navigate = useNavigate();
	const workspace = useWorkspaceStore((s) => s.workspace);
	usePageTitle(t("tasks:title"));
	const [page, setPage] = useState(0);
	const [loading, setLoading] = useState(true);
	const [tasks, setTasks] = useState<TaskInfo[]>([]);
	const [total, setTotal] = useState(0);
	const [expandedTaskIds, setExpandedTaskIds] = useState<Set<number>>(
		() => new Set(),
	);
	const [retryingTaskId, setRetryingTaskId] = useState<number | null>(null);

	const loadPage = useCallback(
		async (targetPage: number, options?: { silent?: boolean }) => {
			const silent = options?.silent ?? false;
			try {
				if (!silent) {
					setLoading(true);
				}
				const data = await taskService.listInWorkspace({
					limit: PAGE_SIZE,
					offset: targetPage * PAGE_SIZE,
				});
				setTasks(data.items);
				setTotal(data.total);
				return data;
			} catch (error) {
				if (!silent) {
					handleApiError(error);
				}
				return null;
			} finally {
				if (!silent) {
					setLoading(false);
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

	useEffect(() => {
		setExpandedTaskIds((prev) => {
			if (prev.size === 0) {
				return prev;
			}

			const expandableTaskIds = new Set(
				tasks
					.filter((task) => taskHasExpandableDetails(task))
					.map((task) => task.id),
			);
			const next = new Set<number>();
			let changed = false;

			for (const taskId of prev) {
				if (expandableTaskIds.has(taskId)) {
					next.add(taskId);
				} else {
					changed = true;
				}
			}

			return changed ? next : prev;
		});
	}, [tasks]);

	const totalPages = Math.max(1, Math.ceil(total / PAGE_SIZE));

	const handleRetry = useCallback(
		async (taskId: number) => {
			try {
				setRetryingTaskId(taskId);
				await taskService.retryTask(taskId);
				toast.success(t("tasks:retry_success"));
				await loadPage(page, { silent: true });
			} catch (error) {
				handleApiError(error);
			} finally {
				setRetryingTaskId(null);
			}
		},
		[loadPage, page, t],
	);

	const toggleTaskDetails = useCallback((taskId: number) => {
		setExpandedTaskIds((prev) => {
			const next = new Set(prev);
			if (next.has(taskId)) {
				next.delete(taskId);
			} else {
				next.add(taskId);
			}
			return next;
		});
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
									detailsExpanded={expandedTaskIds.has(task.id)}
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
									onClick={() => setPage((current) => Math.max(0, current - 1))}
									disabled={page === 0}
								>
									{t("tasks:prev_page")}
								</Button>
								<Button
									variant="outline"
									size="sm"
									onClick={() =>
										setPage((current) => Math.min(totalPages - 1, current + 1))
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
