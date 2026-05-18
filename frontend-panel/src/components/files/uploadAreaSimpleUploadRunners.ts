import {
	getProcessingProgress,
	S3_PROCESSING_PROGRESS,
} from "@/components/files/uploadResume";
import { getApiErrorMessage } from "@/hooks/useApiError";
import { api } from "@/services/http";
import type { InitUploadResponse } from "@/services/uploadService";
import {
	buildUploadPath,
	UploadRequestError,
	uploadService,
} from "@/services/uploadService";
import type { UploadTask } from "./uploadAreaManagerShared";
import { completeWithRetry } from "./uploadAreaManagerShared";
import type {
	UploadModeRunnerContext,
	UploadModeRunners,
} from "./uploadAreaUploadRunnerShared";
import { withTrackedUploadRequest } from "./uploadAreaUploadRunnerShared";
import { createUploadSpeedTracker } from "./uploadSpeed";

function buildDirectUploadPath(
	task: UploadTask,
	workspace: UploadModeRunnerContext["workspace"],
) {
	const params = new URLSearchParams();
	if (task.baseFolderId !== null) {
		params.set("folder_id", String(task.baseFolderId));
	}
	if (task.relativePath) {
		params.set("relative_path", task.relativePath);
	}
	if (task.file) {
		params.set("declared_size", String(task.file.size));
	}

	const basePath = buildUploadPath(workspace, "/files/upload");
	const query = params.toString();
	return query ? `${basePath}?${query}` : basePath;
}

export function createSimpleUploadRunners({
	directAbortRef,
	flushProgress,
	markFolderForRefresh,
	markTaskFailed,
	patchTask,
	patchTaskThrottled,
	uploadRequestRef,
	workspace,
}: UploadModeRunnerContext): Pick<
	UploadModeRunners,
	"runDirectUpload" | "runPresignedUpload"
> {
	const runDirectUpload = async (task: UploadTask) => {
		if (!task.file) return;

		const file = task.file;
		patchTask(task.id, {
			mode: "direct",
			status: "uploading",
			progress: 0,
			uploadedBytes: 0,
			speedBps: undefined,
		});
		const speedTracker = createUploadSpeedTracker();
		const controller = new AbortController();
		directAbortRef.current.set(task.id, controller);

		try {
			const formData = new FormData();
			formData.append("file", file);
			await api.client.post(buildDirectUploadPath(task, workspace), formData, {
				headers: { "Content-Type": "multipart/form-data" },
				signal: controller.signal,
				timeout: 0,
				onUploadProgress: (event) => {
					if (!event.total) return;
					patchTaskThrottled(task.id, {
						progress: Math.round((event.loaded / event.total) * 100),
						...speedTracker.sample(event.loaded),
					});
				},
			});

			patchTask(task.id, {
				status: "completed",
				progress: 100,
				...speedTracker.stop(file.size),
				error: null,
			});
			markFolderForRefresh(task);
		} catch (error) {
			if (controller.signal.aborted) {
				patchTask(task.id, { status: "cancelled", error: null });
				return;
			}
			const message = getApiErrorMessage(error);
			markTaskFailed(task.id, message);
		} finally {
			directAbortRef.current.delete(task.id);
		}
	};

	const runPresignedUpload = async (
		task: UploadTask,
		init: InitUploadResponse,
	) => {
		if (!task.file) return;

		const file = task.file;
		const uploadId = init.upload_id as string;
		const presignedUrl = init.presigned_url as string;
		patchTask(task.id, {
			mode: "presigned",
			status: "uploading",
			uploadId,
			progress: 0,
			uploadedBytes: 0,
			speedBps: undefined,
		});
		const speedTracker = createUploadSpeedTracker();

		try {
			await withTrackedUploadRequest(uploadRequestRef, task.id, (onCreateXhr) =>
				uploadService.presignedUpload(
					presignedUrl,
					file,
					(loaded, total) => {
						patchTaskThrottled(task.id, {
							progress: Math.round((loaded / total) * S3_PROCESSING_PROGRESS),
							...speedTracker.sample(loaded),
						});
					},
					onCreateXhr,
				),
			);

			flushProgress();
			patchTask(task.id, {
				status: "processing",
				progress: getProcessingProgress(task.mode),
				...speedTracker.stop(file.size),
			});
			await completeWithRetry(uploadId);
			patchTask(task.id, {
				status: "completed",
				progress: 100,
				uploadedBytes: file.size,
				speedBps: undefined,
				error: null,
			});
			markFolderForRefresh(task);
		} catch (error) {
			if (error instanceof UploadRequestError && error.isAborted) {
				patchTask(task.id, { status: "cancelled", error: null });
				return;
			}
			const message = getApiErrorMessage(error);
			markTaskFailed(task.id, message);
		}
	};

	return {
		runDirectUpload,
		runPresignedUpload,
	};
}
