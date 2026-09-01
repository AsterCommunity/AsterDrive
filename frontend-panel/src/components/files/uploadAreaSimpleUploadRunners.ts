import {
	getProcessingProgress,
	SERVER_FINALIZE_PROGRESS,
} from "@/components/files/uploadResume";
import { removeSession } from "@/lib/uploadPersistence";
import type { InitUploadResponse } from "@/services/uploadService";
import { UploadRequestError, uploadService } from "@/services/uploadService";
import type { UploadTask } from "./uploadAreaManagerShared";
import { completeWithRetry } from "./uploadAreaManagerShared";
import type {
	UploadTransportRunnerContext,
	UploadTransportRunners,
} from "./uploadAreaUploadRunnerShared";
import { withTrackedUploadRequest } from "./uploadAreaUploadRunnerShared";
import { createUploadSpeedTracker } from "./uploadSpeed";

export function createSimpleUploadRunners({
	flushProgress,
	markFolderForRefresh,
	markTaskFailed,
	patchTask,
	patchTaskThrottled,
	uploadRequestRef,
}: UploadTransportRunnerContext): Pick<
	UploadTransportRunners,
	"runStreamUpload" | "runPresignedUpload"
> {
	const runStreamUpload = async (
		task: UploadTask,
		_init?: InitUploadResponse,
	) => {
		if (!task.file) return;

		const file = task.file;
		patchTask(task.id, {
			mode: "stream",
			status: "uploading",
			progress: 0,
			uploadedBytes: 0,
			speedBps: undefined,
		});
		const speedTracker = createUploadSpeedTracker();
		const uploadId = _init?.upload_id ?? task.uploadId;
		if (!uploadId) {
			markTaskFailed(task.id, new Error("Missing stream upload session"));
			return;
		}

		try {
			await withTrackedUploadRequest(uploadRequestRef, task.id, (onCreateXhr) =>
				uploadService.streamUploadBody(
					uploadId,
					file,
					(loaded, total) => {
						patchTaskThrottled(task.id, {
							progress: Math.round((loaded / total) * 100),
							...speedTracker.sample(loaded),
						});
					},
					onCreateXhr,
				),
			);

			removeSession(uploadId);
			patchTask(task.id, {
				status: "completed",
				progress: 100,
				...speedTracker.stop(file.size),
				error: null,
			});
			markFolderForRefresh(task);
		} catch (error) {
			if (error instanceof UploadRequestError && error.isAborted) {
				patchTask(task.id, { status: "cancelled", error: null });
				return;
			}
			markTaskFailed(task.id, error);
		}
	};

	const runPresignedUpload = async (
		task: UploadTask,
		init: InitUploadResponse,
	) => {
		if (!task.file) return;

		const file = task.file;
		const uploadId = init.upload_id as string;
		const presignedRequest = init.presigned_request;
		if (!presignedRequest) {
			markTaskFailed(task.id, new Error("Missing presigned upload request"));
			return;
		}
		patchTask(task.id, {
			mode: "presigned",
			status: "uploading",
			uploadId,
			progress: 0,
			uploadedBytes: 0,
			speedBps: undefined,
		});
		const speedTracker = createUploadSpeedTracker();
		const requireEtag = init.presigned_require_etag ?? true;

		try {
			await withTrackedUploadRequest(
				uploadRequestRef,
				task.id,
				(onCreateXhr) => {
					const onProgress = (loaded: number, total: number) => {
						patchTaskThrottled(task.id, {
							progress: Math.round((loaded / total) * SERVER_FINALIZE_PROGRESS),
							...speedTracker.sample(loaded),
						});
					};
					return uploadService.presignedUpload(
						presignedRequest.url,
						file,
						onProgress,
						{
							headers: presignedRequest.headers,
							onCreateXhr,
							requireEtag,
						},
					);
				},
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
			markTaskFailed(task.id, error);
		}
	};

	return {
		runStreamUpload,
		runPresignedUpload,
	};
}
