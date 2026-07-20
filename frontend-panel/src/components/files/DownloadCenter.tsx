import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import { BottomRightActivityPortal } from "@/components/layout/BottomRightActivityShell";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { Icon } from "@/components/ui/icon";
import { Progress } from "@/components/ui/progress";
import { ScrollArea } from "@/components/ui/scroll-area";
import { handleApiError } from "@/hooks/useApiError";
import { startAuthenticatedDownload } from "@/lib/authenticatedDownload";
import { formatBytes, formatBytesPerSecond } from "@/lib/format";
import { cn } from "@/lib/utils";
import { createBatchService } from "@/services/batchService";
import {
	cancelDownloadTask,
	retryDownloadTask,
	startDirectoryDownload,
	startProxyArchiveDownload,
	startProxyFileDownload,
	supportsDirectoryDownload,
} from "@/services/downloadCoordinator";
import { createFileService } from "@/services/fileService";
import {
	DOWNLOAD_TASK_STATUS,
	type DownloadTask,
	useDownloadStore,
} from "@/stores/downloadStore";

function taskProgress(task: DownloadTask) {
	if (task.totalBytes && task.totalBytes > 0) {
		return Math.min(100, (task.bytesReceived / task.totalBytes) * 100);
	}
	if (task.totalItems > 0) {
		return Math.min(
			100,
			((task.completedItems + task.failedItems) / task.totalItems) * 100,
		);
	}
	return null;
}

function estimatedRemainingSeconds(task: DownloadTask) {
	if (
		task.totalBytes === null ||
		task.totalBytes <= task.bytesReceived ||
		!task.speedBps ||
		task.speedBps <= 0
	) {
		return null;
	}
	return Math.ceil((task.totalBytes - task.bytesReceived) / task.speedBps);
}

function summarizeDownloadTasks(tasks: DownloadTask[]) {
	let activeCount = 0;
	let completedCount = 0;
	let failedCount = 0;
	let completedWeight = 0;
	let totalWeight = 0;

	for (const task of tasks) {
		const active =
			task.status === DOWNLOAD_TASK_STATUS.queued ||
			task.status === DOWNLOAD_TASK_STATUS.preparing ||
			task.status === DOWNLOAD_TASK_STATUS.downloading;
		if (active) activeCount += 1;
		const taskCompletedItems =
			task.totalItems > 1
				? task.completedItems
				: task.status === DOWNLOAD_TASK_STATUS.completed
					? 1
					: 0;
		const taskFailedItems =
			task.totalItems > 1
				? task.failedItems
				: task.status === DOWNLOAD_TASK_STATUS.failed
					? 1
					: 0;
		completedCount += taskCompletedItems;
		failedCount += taskFailedItems;

		if (task.totalBytes && task.totalBytes > 0) {
			totalWeight += task.totalBytes;
			completedWeight += Math.min(task.bytesReceived, task.totalBytes);
		} else {
			totalWeight += 1;
			completedWeight += task.status === DOWNLOAD_TASK_STATUS.completed ? 1 : 0;
		}
	}

	return {
		activeCount,
		completedCount,
		failedCount,
		overallProgress:
			totalWeight > 0 ? Math.round((completedWeight / totalWeight) * 100) : 0,
		totalCount: tasks.reduce(
			(sum, task) => sum + Math.max(task.totalItems, 1),
			0,
		),
	};
}

function taskStatusIcon(task: DownloadTask) {
	switch (task.status) {
		case DOWNLOAD_TASK_STATUS.completed:
			return "Check" as const;
		case DOWNLOAD_TASK_STATUS.failed:
			return "CircleAlert" as const;
		case DOWNLOAD_TASK_STATUS.canceled:
			return "X" as const;
		case DOWNLOAD_TASK_STATUS.queued:
		case DOWNLOAD_TASK_STATUS.preparing:
			return "Clock" as const;
		case DOWNLOAD_TASK_STATUS.downloading:
			return "Download" as const;
	}
}

function DownloadTaskCard({ task }: { task: DownloadTask }) {
	const { t } = useTranslation("files");
	const progress = taskProgress(task);
	const isActive =
		task.status === DOWNLOAD_TASK_STATUS.queued ||
		task.status === DOWNLOAD_TASK_STATUS.preparing ||
		task.status === DOWNLOAD_TASK_STATUS.downloading;
	const canRetry =
		!isActive &&
		(task.status === DOWNLOAD_TASK_STATUS.failed || task.failedItems > 0);
	const remainingSeconds = estimatedRemainingSeconds(task);

	return (
		<div className="rounded-lg border border-border/65 bg-card p-3 shadow-xs">
			<div className="flex items-start gap-3">
				<span className="mt-0.5 flex size-8 shrink-0 items-center justify-center rounded-lg bg-muted/60 text-muted-foreground">
					<Icon name={taskStatusIcon(task)} className="size-4" />
				</span>
				<div className="min-w-0 flex-1">
					<div className="flex items-start gap-2">
						<div className="min-w-0 flex-1">
							<p className="truncate text-sm font-medium text-foreground">
								{task.name === "download_to_folder"
									? t("download_to_folder")
									: task.name}
							</p>
							<p className="mt-0.5 text-xs text-muted-foreground">
								{t(`download_status_${task.status}`)}
								{task.totalItems > 1
									? ` · ${t("download_items_progress", {
											completed: task.completedItems,
											failed: task.failedItems,
											total: task.totalItems,
										})}`
									: ""}
							</p>
						</div>
						{isActive ? (
							<Button
								type="button"
								variant="ghost"
								size="icon-sm"
								onClick={() => cancelDownloadTask(task.id)}
								aria-label={t("download_cancel")}
								title={t("download_cancel")}
							>
								<Icon name="X" className="size-4" />
							</Button>
						) : canRetry ? (
							<Button
								type="button"
								variant="ghost"
								size="icon-sm"
								onClick={() => retryDownloadTask(task.id)}
								aria-label={t("download_retry_failed")}
								title={t("download_retry_failed")}
							>
								<Icon name="ArrowClockwise" className="size-4" />
							</Button>
						) : null}
					</div>

					{progress !== null ? (
						<Progress value={progress} className="mt-3 gap-0" />
					) : null}
					<div className="mt-2 flex flex-wrap gap-x-3 gap-y-1 text-xs text-muted-foreground">
						{task.bytesReceived > 0 ? (
							<span>
								{formatBytes(task.bytesReceived)}
								{task.totalBytes !== null
									? ` / ${formatBytes(task.totalBytes)}`
									: ""}
							</span>
						) : null}
						{task.speedBps ? (
							<span>{formatBytesPerSecond(task.speedBps)}</span>
						) : null}
						{remainingSeconds !== null ? (
							<span>
								{t("download_estimated_remaining", {
									seconds: remainingSeconds,
								})}
							</span>
						) : null}
					</div>
					{task.warning ? (
						<p className="mt-2 text-xs text-amber-700 dark:text-amber-300">
							{t(task.warning)}
						</p>
					) : null}
					{task.error ? (
						<p className="mt-2 break-words text-xs text-destructive">
							{task.error.startsWith("download_") ? t(task.error) : task.error}
						</p>
					) : null}
				</div>
			</div>
		</div>
	);
}

export function DownloadCenter() {
	const { t } = useTranslation("files");
	const pendingSelection = useDownloadStore((state) => state.pendingSelection);
	const dismissSelection = useDownloadStore((state) => state.dismissSelection);
	const isPanelOpen = useDownloadStore((state) => state.isPanelOpen);
	const setPanelOpen = useDownloadStore((state) => state.setPanelOpen);
	const tasks = useDownloadStore((state) => state.tasks);
	const removeCompleted = useDownloadStore((state) => state.removeCompleted);
	const summary = useMemo(() => summarizeDownloadTasks(tasks), [tasks]);
	const allCompleted =
		summary.totalCount > 0 &&
		summary.completedCount === summary.totalCount &&
		summary.failedCount === 0;
	const compactSummary =
		summary.activeCount > 0
			? t("download_center_progress_summary", {
					completed: summary.completedCount,
					total: summary.totalCount,
				})
			: summary.failedCount > 0
				? t("download_center_failed_summary", {
						completed: summary.completedCount,
						failed: summary.failedCount,
					})
				: t("download_center_completed_summary", {
						count: summary.completedCount,
					});
	const singleFile =
		pendingSelection?.files.length === 1 &&
		pendingSelection.folders.length === 0
			? pendingSelection.files[0]
			: null;

	return (
		<>
			<BottomRightActivityPortal>
				{tasks.length > 0 ? (
					<button
						type="button"
						className={cn(
							"pointer-events-auto w-[22rem] max-w-full rounded-lg border bg-card/95 px-3 py-2.5 text-left shadow-lg shadow-black/10 backdrop-blur transition-[border-color,background-color,box-shadow] duration-200 hover:bg-card dark:shadow-none",
							summary.activeCount > 0 &&
								"border-blue-500/70 ring-1 ring-blue-500/20 shadow-blue-950/10 dark:border-blue-400/65 dark:ring-blue-400/20",
							allCompleted &&
								"border-emerald-500/55 ring-1 ring-emerald-500/15",
							summary.failedCount > 0 &&
								summary.activeCount === 0 &&
								"border-destructive/55 ring-1 ring-destructive/15",
						)}
						onClick={() => setPanelOpen(true)}
						aria-label={t("download_center")}
					>
						<span className="flex items-center gap-2.5">
							<span
								className={cn(
									"flex size-8 shrink-0 items-center justify-center rounded-lg bg-muted/55 text-muted-foreground",
									summary.activeCount > 0 &&
										"bg-blue-500/10 text-blue-600 dark:text-blue-300",
									allCompleted &&
										"bg-emerald-500/10 text-emerald-600 dark:text-emerald-300",
								)}
							>
								<Icon
									name={allCompleted ? "Check" : "Download"}
									className="size-4"
								/>
							</span>
							<span className="min-w-0 flex-1">
								<span className="flex items-center justify-between gap-3">
									<span className="truncate text-sm font-semibold text-foreground">
										{allCompleted
											? t("download_center_completed")
											: t("download_center")}
									</span>
									<span className="shrink-0 text-xs font-medium tabular-nums text-muted-foreground">
										{summary.overallProgress}%
									</span>
								</span>
								<span className="mt-0.5 block truncate text-xs text-muted-foreground">
									{compactSummary}
								</span>
							</span>
						</span>
						<Progress
							value={summary.overallProgress}
							className={cn(
								"mt-2.5 gap-0",
								allCompleted &&
									"[&_[data-slot=progress-indicator]]:bg-emerald-500",
							)}
						/>
					</button>
				) : null}
			</BottomRightActivityPortal>

			<Dialog
				open={pendingSelection !== null}
				onOpenChange={(open) => !open && dismissSelection()}
			>
				<DialogContent className="sm:max-w-lg">
					<DialogHeader>
						<DialogTitle>{t("download_method_title")}</DialogTitle>
						<DialogDescription>
							{t("download_method_description", {
								count:
									(pendingSelection?.files.length ?? 0) +
									(pendingSelection?.folders.length ?? 0),
							})}
						</DialogDescription>
					</DialogHeader>
					<div className="grid gap-2">
						{singleFile && pendingSelection ? (
							<Button
								type="button"
								variant="outline"
								className="h-auto justify-start gap-3 p-3 text-left"
								onClick={() => {
									dismissSelection();
									void startProxyFileDownload(
										pendingSelection.workspace,
										singleFile,
									);
								}}
							>
								<Icon name="Download" className="size-5 shrink-0" />
								<span>
									<span className="block font-medium">
										{t("download_proxy_file")}
									</span>
									<span className="mt-0.5 block text-xs text-muted-foreground">
										{t("download_proxy_file_desc")}
									</span>
								</span>
							</Button>
						) : null}
						{pendingSelection ? (
							<Button
								type="button"
								variant="outline"
								className="h-auto justify-start gap-3 p-3 text-left"
								onClick={() => {
									dismissSelection();
									void startProxyArchiveDownload(pendingSelection);
								}}
							>
								<Icon name="FileZip" className="size-5 shrink-0" />
								<span>
									<span className="block font-medium">
										{t("download_proxy_archive")}
									</span>
									<span className="mt-0.5 block text-xs text-muted-foreground">
										{t("download_proxy_archive_desc")}
									</span>
								</span>
							</Button>
						) : null}
						{pendingSelection ? (
							<Button
								type="button"
								variant="outline"
								className="h-auto justify-start gap-3 p-3 text-left"
								onClick={() => {
									dismissSelection();
									if (supportsDirectoryDownload()) {
										void startDirectoryDownload(pendingSelection);
									} else {
										void startProxyArchiveDownload(pendingSelection);
									}
								}}
							>
								<Icon name="FolderOpen" className="size-5 shrink-0" />
								<span>
									<span className="block font-medium">
										{t("download_to_folder")}
									</span>
									<span className="mt-0.5 block text-xs text-muted-foreground">
										{supportsDirectoryDownload()
											? t("download_to_folder_desc")
											: t("download_directory_fallback")}
									</span>
								</span>
							</Button>
						) : null}
						{pendingSelection && !singleFile ? (
							<Button
								type="button"
								variant="ghost"
								className="h-auto justify-start gap-3 p-3 text-left"
								onClick={() => {
									dismissSelection();
									void createBatchService(pendingSelection.workspace)
										.streamArchiveDownload(
											pendingSelection.files.map((file) => file.id),
											pendingSelection.folders.map((folder) => folder.id),
										)
										.catch(handleApiError);
								}}
							>
								<Icon name="ArrowSquareOut" className="size-5 shrink-0" />
								<span>
									<span className="block font-medium">
										{t("download_browser_archive")}
									</span>
									<span className="mt-0.5 block text-xs text-muted-foreground">
										{t("download_browser_archive_desc")}
									</span>
								</span>
							</Button>
						) : null}
						{singleFile && pendingSelection ? (
							<Button
								type="button"
								variant="ghost"
								className="h-auto justify-start gap-3 p-3 text-left"
								onClick={() => {
									dismissSelection();
									void startAuthenticatedDownload(
										createFileService(pendingSelection.workspace).downloadPath(
											singleFile.id,
										),
									);
								}}
							>
								<Icon name="ArrowSquareOut" className="size-5 shrink-0" />
								<span>
									<span className="block font-medium">
										{t("download_browser_default")}
									</span>
									<span className="mt-0.5 block text-xs text-muted-foreground">
										{t("download_browser_default_desc")}
									</span>
								</span>
							</Button>
						) : null}
					</div>
				</DialogContent>
			</Dialog>

			<Dialog open={isPanelOpen} onOpenChange={setPanelOpen}>
				<DialogContent className="flex max-h-[min(44rem,calc(100dvh-2rem))] flex-col sm:max-w-xl">
					<DialogHeader>
						<div className="flex items-center justify-between gap-3 pr-8">
							<div>
								<DialogTitle>{t("download_center")}</DialogTitle>
								<DialogDescription>
									{t("download_center_desc")}
								</DialogDescription>
							</div>
							{tasks.length > 0 ? (
								<Button
									type="button"
									variant="ghost"
									size="sm"
									onClick={removeCompleted}
								>
									{t("download_clear_completed")}
								</Button>
							) : null}
						</div>
					</DialogHeader>
					<ScrollArea className="min-h-0 flex-1 pr-3">
						<div className="space-y-2">
							{tasks.length === 0 ? (
								<div className="py-10 text-center text-sm text-muted-foreground">
									{t("download_center_empty")}
								</div>
							) : (
								tasks.map((task) => (
									<DownloadTaskCard key={task.id} task={task} />
								))
							)}
						</div>
					</ScrollArea>
				</DialogContent>
			</Dialog>
		</>
	);
}
