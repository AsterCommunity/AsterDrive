import type { UploadTaskView } from "@/components/files/UploadPanel";
import { formatBytesPerSecond } from "@/lib/format";
import {
	ACTIVE_QUEUE_STATUS_SET,
	ACTIVE_QUEUE_STATUSES,
	type UploadAreaManagerTranslationFn,
	type UploadTask,
} from "./uploadAreaManagerShared";

interface UploadTaskSummary {
	activeCount: number;
	failedCount: number;
	overallProgress: number;
	successCount: number;
	totalCount: number;
}

interface BuildUploadTaskViewsOptions {
	cancelTask: (taskId: string) => Promise<void>;
	requestResumeFilePicker: (taskId: string) => void;
	retryTask: (taskId: string) => Promise<void>;
	t: UploadAreaManagerTranslationFn;
	tasks: UploadTask[];
}

function getModeLabel(task: UploadTask) {
	if (task.mode === "chunked") return "Chunked";
	if (task.mode === "presigned") return "Presigned";
	if (task.mode === "presigned_multipart") return "Presigned Multipart";
	if (task.mode === "direct") return "Direct";
	return "Pending";
}

function getStatusLabel(task: UploadTask, t: UploadAreaManagerTranslationFn) {
	if (task.status === "pending_file") return t("files:upload_pending_file");
	if (task.status === "queued") return t("files:processing");
	if (task.status === "initializing") return t("files:processing");
	if (task.status === "uploading") return t("files:uploading_to_storage");
	if (task.status === "processing") return t("files:upload_processing");
	if (task.status === "completed") return t("files:upload_success");
	if (task.status === "cancelled") return t("files:upload_dismiss");
	return t("files:upload_failed");
}

function getSpeedLabel(task: UploadTask) {
	if (task.status !== "uploading" || task.speedBps == null) return undefined;
	return formatBytesPerSecond(task.speedBps);
}

export function summarizeUploadTasks(tasks: UploadTask[]): UploadTaskSummary {
	let successCount = 0;
	let failedCount = 0;
	let activeCount = 0;
	let progressSum = 0;
	let weightedProgressSum = 0;
	let totalBytes = 0;

	for (const task of tasks) {
		progressSum += task.progress;
		if (task.totalBytes > 0) {
			totalBytes += task.totalBytes;
			weightedProgressSum += task.progress * task.totalBytes;
		}
		if (task.status === "completed") {
			successCount += 1;
			continue;
		}
		if (task.status === "failed") {
			failedCount += 1;
			continue;
		}
		if (ACTIVE_QUEUE_STATUS_SET.has(task.status)) {
			activeCount += 1;
		}
	}

	const totalCount = tasks.length;
	const overallProgress =
		totalBytes > 0
			? Math.round(weightedProgressSum / totalBytes)
			: totalCount === 0
				? 0
				: Math.round(progressSum / totalCount);

	return {
		activeCount,
		failedCount,
		overallProgress,
		successCount,
		totalCount,
	};
}

export function buildUploadTaskViews({
	cancelTask,
	requestResumeFilePicker,
	retryTask,
	t,
	tasks,
}: BuildUploadTaskViewsOptions): UploadTaskView[] {
	return tasks.map((task) => {
		const isPendingFile = task.status === "pending_file";
		const statusLabel = getStatusLabel(task, t);
		const detail = isPendingFile
			? (task.error ??
				t("files:upload_chunk_status", {
					current: task.completedChunks ?? 0,
					total: task.totalChunks ?? 0,
				}))
			: task.status === "failed"
				? (task.error ?? t("files:upload_failed"))
				: (task.mode === "chunked" || task.mode === "presigned_multipart") &&
						task.status === "uploading"
					? t("files:upload_chunk_status", {
							current: task.completedChunks ?? 0,
							total: task.totalChunks ?? 0,
						})
					: task.relativePath || statusLabel;

		const actions = isPendingFile
			? [
					{
						label: t("files:upload_resume_select"),
						icon: "Upload" as const,
						onClick: () => requestResumeFilePicker(task.id),
						variant: "outline" as const,
					},
					{
						label: t("files:upload_dismiss"),
						icon: "X" as const,
						onClick: () => {
							void cancelTask(task.id);
						},
					},
				]
			: task.status === "failed"
				? [
						{
							label: t("files:upload_retry"),
							icon: "ArrowsClockwise" as const,
							onClick: () => {
								void retryTask(task.id);
							},
							variant: "outline" as const,
						},
					]
				: ACTIVE_QUEUE_STATUSES.includes(task.status)
					? [
							{
								label: t("files:upload_dismiss"),
								icon: "X" as const,
								onClick: () => {
									void cancelTask(task.id);
								},
							},
						]
					: [];

		const group = task.relativePath
			? task.relativePath.split("/").slice(0, -1).join("/")
			: "";

		return {
			id: task.id,
			title: task.filename,
			status: statusLabel,
			mode: getModeLabel(task),
			progress: task.progress,
			group,
			targetLabel: task.baseFolderName,
			detail,
			speed: getSpeedLabel(task),
			completed: task.status === "completed",
			actions,
		};
	});
}
