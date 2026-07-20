import { useEffect, useMemo } from "react";
import { useTranslation } from "react-i18next";
import {
	TransferActivitySection,
	TransferTaskItem,
} from "@/components/files/TransferActivitySection";
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
import { ScrollArea } from "@/components/ui/scroll-area";
import { handleApiError } from "@/hooks/useApiError";
import { formatBytes, formatBytesPerSecond } from "@/lib/format";
import { createBatchService } from "@/services/batchService";
import {
	cancelDownloadTask,
	retryDownloadTask,
	startAuthenticatedFileDownload,
	startDirectoryDownload,
	startProxyArchiveDownload,
	startProxyFileDownload,
	supportsDirectoryDownload,
} from "@/services/downloadCoordinator";
import {
	DOWNLOAD_TASK_STATUS,
	type DownloadTask,
	useDownloadStore,
} from "@/stores/downloadStore";
import { useFrontendConfigStore } from "@/stores/frontendConfigStore";
import {
	TRANSFER_ACTIVITY,
	useTransferActivityStore,
} from "@/stores/transferActivityStore";

const DOWNLOAD_PANEL_EXPANDED_BODY_CLASS = "h-[min(26rem,calc(100dvh-15rem))]";

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

function DownloadTaskItem({ task }: { task: DownloadTask }) {
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
	const tone =
		task.status === DOWNLOAD_TASK_STATUS.completed
			? ("success" as const)
			: task.status === DOWNLOAD_TASK_STATUS.failed ||
					task.status === DOWNLOAD_TASK_STATUS.canceled
				? ("error" as const)
				: isActive
					? ("active" as const)
					: ("default" as const);
	const detailParts = [t(`download_status_${task.status}`)];
	if (task.totalItems > 1) {
		detailParts.push(
			t("download_items_progress", {
				completed: task.completedItems,
				failed: task.failedItems,
				total: task.totalItems,
			}),
		);
	}
	if (task.bytesReceived > 0) {
		detailParts.push(
			task.totalBytes !== null
				? `${formatBytes(task.bytesReceived)} / ${formatBytes(task.totalBytes)}`
				: formatBytes(task.bytesReceived),
		);
	}
	if (task.speedBps) detailParts.push(formatBytesPerSecond(task.speedBps));
	if (remainingSeconds !== null) {
		detailParts.push(
			t("download_estimated_remaining", { seconds: remainingSeconds }),
		);
	}

	return (
		<TransferTaskItem
			title={
				task.name === "download_to_folder" ? t("download_to_folder") : task.name
			}
			detail={detailParts.join(" · ")}
			icon={taskStatusIcon(task)}
			tone={tone}
			progress={progress}
			progressLabel={progress !== null ? `${Math.round(progress)}%` : undefined}
			warning={task.warning ? t(task.warning) : undefined}
			error={
				task.error
					? task.error.startsWith("download_")
						? t(task.error)
						: task.error
					: undefined
			}
			actions={
				isActive ? (
					<Button
						type="button"
						variant="ghost"
						size="icon-xs"
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
						size="icon-xs"
						onClick={() => retryDownloadTask(task.id)}
						aria-label={t("download_retry_failed")}
						title={t("download_retry_failed")}
					>
						<Icon name="ArrowClockwise" className="size-4" />
					</Button>
				) : null
			}
		/>
	);
}

export function DownloadCenter() {
	const { t } = useTranslation("files");
	const archiveDownloadEnabled = useFrontendConfigStore(
		(state) => state.isLoaded && state.archiveDownloadUserEnabled,
	);
	const pendingSelection = useDownloadStore((state) => state.pendingSelection);
	const dismissSelection = useDownloadStore((state) => state.dismissSelection);
	const tasks = useDownloadStore((state) => state.tasks);
	const removeCompleted = useDownloadStore((state) => state.removeCompleted);
	const downloadPanelOpen = useTransferActivityStore(
		(state) => state.expandedActivity === TRANSFER_ACTIVITY.download,
	);
	const setActivityOpen = useTransferActivityStore(
		(state) => state.setActivityOpen,
	);
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
	const directoryDownloadSupported = supportsDirectoryDownload();
	const canRemoveCompleted = tasks.some(
		(task) =>
			task.status === DOWNLOAD_TASK_STATUS.completed ||
			task.status === DOWNLOAD_TASK_STATUS.canceled,
	);
	const sectionTone =
		summary.activeCount > 0
			? ("active" as const)
			: summary.failedCount > 0
				? ("error" as const)
				: allCompleted
					? ("success" as const)
					: ("default" as const);

	useEffect(() => {
		if (tasks.length === 0 && downloadPanelOpen) {
			setActivityOpen(TRANSFER_ACTIVITY.download, false);
		}
	}, [downloadPanelOpen, setActivityOpen, tasks.length]);

	return (
		<>
			<BottomRightActivityPortal>
				{tasks.length > 0 ? (
					<TransferActivitySection
						open={downloadPanelOpen}
						onToggle={() =>
							setActivityOpen(TRANSFER_ACTIVITY.download, (open) => !open)
						}
						title={
							allCompleted
								? t("download_center_completed")
								: t("download_center")
						}
						summary={compactSummary}
						icon={allCompleted ? "Check" : "Download"}
						tone={sectionTone}
						progress={summary.activeCount > 0 ? summary.overallProgress : null}
						toggleLabel={t("download_center")}
						expandedBodyClassName={DOWNLOAD_PANEL_EXPANDED_BODY_CLASS}
					>
						<div className="flex h-full min-h-0 flex-col">
							<ScrollArea className="min-h-0 flex-1 bg-background/70 dark:bg-background/20">
								<div>
									{tasks.map((task) => (
										<DownloadTaskItem key={task.id} task={task} />
									))}
								</div>
							</ScrollArea>
							{canRemoveCompleted ? (
								<div className="flex shrink-0 justify-end border-t border-border/60 bg-card/80 px-4 py-3 dark:bg-card/65">
									<Button
										type="button"
										variant="outline"
										size="sm"
										onClick={removeCompleted}
									>
										<Icon name="X" className="size-3.5" />
										{t("download_clear_completed")}
									</Button>
								</div>
							) : null}
						</div>
					</TransferActivitySection>
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
						{pendingSelection && archiveDownloadEnabled ? (
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
						{pendingSelection &&
						(directoryDownloadSupported || archiveDownloadEnabled) ? (
							<Button
								type="button"
								variant="outline"
								className="h-auto justify-start gap-3 p-3 text-left"
								onClick={() => {
									dismissSelection();
									if (directoryDownloadSupported) {
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
										{directoryDownloadSupported
											? t("download_to_folder_desc")
											: t("download_directory_fallback")}
									</span>
								</span>
							</Button>
						) : null}
						{pendingSelection && !singleFile && archiveDownloadEnabled ? (
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
									void startAuthenticatedFileDownload(
										pendingSelection.workspace,
										singleFile.id,
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
		</>
	);
}
