import {
	CHUNK_PROCESSING_PROGRESS,
	getProcessingProgress,
	S3_PROCESSING_PROGRESS,
	type UploadMode,
} from "@/components/files/uploadResume";
import { getApiErrorMessage } from "@/hooks/useApiError";
import { appendCompletedPart, removeSession } from "@/lib/uploadPersistence";
import {
	type CompletedPart,
	type InitUploadResponse,
	uploadService,
} from "@/services/uploadService";
import {
	completeWithRetry,
	shouldRemovePersistedSession,
	type UploadTask,
} from "./uploadAreaManagerShared";
import { createResumableUploadShared } from "./uploadAreaResumableUploadShared";
import type {
	UploadModeRunnerContext,
	UploadModeRunners,
} from "./uploadAreaUploadRunnerShared";
import {
	abortUploadRequests,
	withTrackedUploadRequest,
} from "./uploadAreaUploadRunnerShared";

const PRESIGNED_MULTIPART_URL_BATCH_SIZE = 16;

export function createResumableUploadRunners({
	abortFlagsRef,
	flushProgress,
	markFolderForRefresh,
	markTaskFailed,
	multipartInFlightRef,
	patchTask,
	patchTaskThrottled,
	uploadRequestRef,
}: UploadModeRunnerContext): Pick<
	UploadModeRunners,
	| "cancelMultipartSession"
	| "resumeCompletionTask"
	| "runChunkedUpload"
	| "runMultipartUpload"
> {
	const {
		runResumableTransfer,
		runRetryableUploadOperation,
		waitForMultipartDrain,
		withTrackedMultipartRequest,
	} = createResumableUploadShared({
		abortFlagsRef,
		flushProgress,
		markFolderForRefresh,
		markTaskFailed,
		multipartInFlightRef,
		patchTask,
		patchTaskThrottled,
	});

	const cancelMultipartSession = async (task: UploadTask) => {
		abortFlagsRef.current.set(task.id, true);
		abortUploadRequests(uploadRequestRef, task.id);
		if (!task.uploadId) return;

		await waitForMultipartDrain(task.id);
		try {
			await uploadService.cancelUpload(task.uploadId);
		} catch {}
		removeSession(task.uploadId);
	};

	const resumeCompletionTask = async (
		task: UploadTask,
		parts?: CompletedPart[],
	) => {
		const uploadId = task.uploadId;
		if (!uploadId) return;

		abortFlagsRef.current.set(task.id, false);
		patchTask(task.id, {
			status: "processing",
			progress: getProcessingProgress(task.mode),
			speedBps: undefined,
		});

		try {
			await completeWithRetry(uploadId, parts);
			if (abortFlagsRef.current.get(task.id)) {
				patchTask(task.id, {
					status: "cancelled",
					error: null,
					speedBps: undefined,
				});
				return;
			}
			removeSession(uploadId);
			patchTask(task.id, {
				status: "completed",
				progress: 100,
				uploadedBytes: task.totalBytes,
				speedBps: undefined,
				error: null,
			});
			markFolderForRefresh(task);
		} catch (error) {
			if (abortFlagsRef.current.get(task.id)) {
				patchTask(task.id, {
					status: "cancelled",
					error: null,
					speedBps: undefined,
				});
				return;
			}
			const message = getApiErrorMessage(error);
			if (!task.file) {
				if (shouldRemovePersistedSession(error)) {
					removeSession(uploadId);
					patchTask(task.id, {
						status: "pending_file",
						error: message,
						progress: 0,
						uploadedBytes: 0,
						speedBps: undefined,
						uploadId: null,
						completedChunks: 0,
						totalChunks: 0,
						mode: null,
					});
					return;
				}
				markTaskFailed(task.id, message);
				return;
			}
			markTaskFailed(task.id, message);
		} finally {
			abortFlagsRef.current.delete(task.id);
		}
	};

	const runChunkedUpload = async (
		task: UploadTask,
		init: InitUploadResponse,
		alreadyReceived: number[] = [],
	) => {
		if (!task.file) return;

		const file = task.file;
		const uploadId = init.upload_id as string;
		const chunkSize = init.chunk_size as number;
		const totalChunks = init.total_chunks as number;
		const pendingChunkNumbers = Array.from(
			{ length: totalChunks },
			(_, index) => index,
		).filter((index) => !alreadyReceived.includes(index));
		const getChunkSize = (chunkNumber: number) => {
			const start = chunkNumber * chunkSize;
			return Math.max(0, Math.min(chunkSize, file.size - start));
		};

		await runResumableTransfer({
			completeUpload: () => completeWithRetry(uploadId),
			initialCompleted: alreadyReceived.length,
			initialCompletedBytes: alreadyReceived.reduce(
				(total, chunkNumber) => total + getChunkSize(chunkNumber),
				0,
			),
			items: pendingChunkNumbers,
			getItemSize: getChunkSize,
			processingProgress: CHUNK_PROCESSING_PROGRESS,
			progressScale: 95,
			task,
			totalItems: totalChunks,
			totalBytes: file.size,
			uploadId,
			uploadItem: async (chunkNumber, reportProgress) => {
				const start = chunkNumber * chunkSize;
				const end = Math.min(start + chunkSize, file.size);
				const blob = file.slice(start, end);

				await runRetryableUploadOperation({
					run: async () => {
						return withTrackedMultipartRequest(task.id, () =>
							withTrackedUploadRequest(
								uploadRequestRef,
								task.id,
								(onCreateXhr) =>
									uploadService.uploadChunk(
										uploadId,
										chunkNumber,
										blob,
										reportProgress,
										onCreateXhr,
									),
							),
						);
					},
				});
			},
			uploadingPatch: {
				mode: "chunked",
				status: "uploading",
				uploadId,
				totalChunks,
				completedChunks: alreadyReceived.length,
			},
		});
	};

	const runMultipartUpload = async (
		task: UploadTask,
		init: InitUploadResponse,
		alreadyCompleted: CompletedPart[] = [],
	) => {
		if (!task.file) return;

		const file = task.file;
		const uploadId = init.upload_id as string;
		const chunkSize = init.chunk_size as number;
		const totalChunks = init.total_chunks as number;
		abortFlagsRef.current.set(task.id, false);
		const getPartSize = (partNumber: number) => {
			const start = (partNumber - 1) * chunkSize;
			return Math.max(0, Math.min(chunkSize, file.size - start));
		};

		const collectedParts: CompletedPart[] = [...alreadyCompleted];
		const completedSet = new Set(
			alreadyCompleted.map((part) => part.part_number),
		);

		patchTask(task.id, {
			mode: "presigned_multipart" as UploadMode,
			status: "uploading",
			uploadId,
			totalChunks,
			completedChunks: completedSet.size,
			progress: Math.round(
				(completedSet.size / totalChunks) * S3_PROCESSING_PROGRESS,
			),
			uploadedBytes: [...completedSet].reduce(
				(total, partNumber) => total + getPartSize(partNumber),
				0,
			),
			speedBps: undefined,
		});

		const queue = Array.from(
			{ length: totalChunks },
			(_, index) => index + 1,
		).filter((partNumber) => !completedSet.has(partNumber));
		const pendingPartNumbers = [...queue];

		let urlCache: Record<number, string> = {};
		const inFlightPresignBatches = new Map<number, Promise<void>>();

		const getPartUrl = async (partNumber: number): Promise<string> => {
			if (urlCache[partNumber]) return urlCache[partNumber];

			const inFlightBatch = inFlightPresignBatches.get(partNumber);
			if (inFlightBatch) {
				await inFlightBatch;
				if (urlCache[partNumber]) return urlCache[partNumber];
			}

			const currentIndex = pendingPartNumbers.indexOf(partNumber);
			const candidates =
				currentIndex >= 0
					? pendingPartNumbers.slice(currentIndex)
					: [partNumber];
			let batch = candidates
				.filter((candidate) => !urlCache[candidate])
				.slice(0, PRESIGNED_MULTIPART_URL_BATCH_SIZE);
			if (!batch.includes(partNumber)) {
				batch = [partNumber, ...batch].slice(
					0,
					PRESIGNED_MULTIPART_URL_BATCH_SIZE,
				);
			}

			const presignBatch = uploadService
				.presignParts(uploadId, batch)
				.then((urls) => {
					urlCache = { ...urlCache, ...urls };
				});
			for (const batchPartNumber of batch) {
				inFlightPresignBatches.set(batchPartNumber, presignBatch);
			}

			try {
				await presignBatch;
			} finally {
				for (const batchPartNumber of batch) {
					inFlightPresignBatches.delete(batchPartNumber);
				}
			}

			const url = urlCache[partNumber];
			if (!url) {
				throw new Error(`Missing presigned URL for part ${partNumber}`);
			}
			return url;
		};

		await runResumableTransfer({
			completeUpload: async () => {
				collectedParts.sort(
					(left, right) => left.part_number - right.part_number,
				);
				await completeWithRetry(uploadId, collectedParts);
			},
			initialCompleted: completedSet.size,
			initialCompletedBytes: [...completedSet].reduce(
				(total, partNumber) => total + getPartSize(partNumber),
				0,
			),
			items: queue,
			getItemSize: getPartSize,
			processingProgress: getProcessingProgress(task.mode),
			progressScale: S3_PROCESSING_PROGRESS,
			task,
			totalItems: totalChunks,
			totalBytes: file.size,
			uploadId,
			uploadItem: async (partNumber, reportProgress) => {
				const start = (partNumber - 1) * chunkSize;
				const end = Math.min(start + chunkSize, file.size);
				const blob = file.slice(start, end);

				const etag = await runRetryableUploadOperation({
					onRetryableError: () => {
						delete urlCache[partNumber];
					},
					run: async () => {
						const url = await getPartUrl(partNumber);
						return withTrackedMultipartRequest(task.id, () =>
							withTrackedUploadRequest(
								uploadRequestRef,
								task.id,
								(onCreateXhr) =>
									uploadService.presignedUpload(
										url,
										blob,
										reportProgress,
										onCreateXhr,
									),
							),
						);
					},
				});

				const part: CompletedPart = {
					part_number: partNumber,
					etag: etag.replace(/"/g, ""),
				};
				collectedParts.push(part);
				appendCompletedPart(uploadId, part);
			},
			uploadingPatch: {
				mode: "presigned_multipart" as UploadMode,
				status: "uploading",
				uploadId,
				totalChunks,
				completedChunks: completedSet.size,
			},
		});
	};

	return {
		cancelMultipartSession,
		resumeCompletionTask,
		runChunkedUpload,
		runMultipartUpload,
	};
}
