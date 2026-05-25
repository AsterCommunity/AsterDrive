import { getApiErrorMessage } from "@/hooks/useApiError";
import { removeSession } from "@/lib/uploadPersistence";
import { isRetryableUploadError } from "@/services/uploadService";
import { useAuthStore } from "@/stores/authStore";
import {
	CHUNK_CONCURRENT,
	CHUNK_MAX_RETRIES,
	MULTIPART_DRAIN_POLL_MS,
	MULTIPART_DRAIN_TIMEOUT_MS,
	type UploadTask,
} from "./uploadAreaManagerShared";
import type { UploadModeRunnerContext } from "./uploadAreaUploadRunnerShared";
import { createUploadSpeedTracker } from "./uploadSpeed";

type ResumableUploadSharedContext = Pick<
	UploadModeRunnerContext,
	| "abortFlagsRef"
	| "flushProgress"
	| "markFolderForRefresh"
	| "markTaskFailed"
	| "multipartInFlightRef"
	| "patchTask"
	| "patchTaskThrottled"
>;

interface RunResumableTransferOptions<TItem> {
	completeUpload: () => Promise<unknown>;
	initialCompleted: number;
	initialCompletedBytes?: number;
	items: TItem[];
	getItemSize?: (item: TItem) => number;
	processingProgress: number;
	progressScale: number;
	task: UploadTask;
	totalItems: number;
	totalBytes?: number;
	uploadItem: (
		item: TItem,
		reportProgress: (loaded: number) => void,
	) => Promise<void>;
	uploadId: string;
	uploadingPatch: Partial<UploadTask>;
}

function calculateProgress(
	completed: number,
	totalItems: number,
	progressScale: number,
) {
	if (totalItems <= 0) return 0;
	return Math.round((completed / totalItems) * progressScale);
}

function calculateByteProgress(
	completedBytes: number,
	inFlightBytes: Iterable<number>,
	totalBytes: number,
	progressScale: number,
) {
	if (totalBytes <= 0) return 0;
	let uploadedBytes = completedBytes;
	for (const bytes of inFlightBytes) {
		uploadedBytes += bytes;
	}
	const clampedBytes = Math.min(Math.max(uploadedBytes, 0), totalBytes);
	return Math.min(
		progressScale,
		Math.round((clampedBytes / totalBytes) * progressScale),
	);
}

function clampLoadedBytes(loaded: number, total: number) {
	return Math.min(Math.max(loaded, 0), Math.max(total, 0));
}

function isAuthFailureUploadError(error: Error) {
	return (
		"authFailure" in error &&
		(error as { authFailure?: boolean }).authFailure === true
	);
}

export function createResumableUploadShared({
	abortFlagsRef,
	flushProgress,
	markFolderForRefresh,
	markTaskFailed,
	multipartInFlightRef,
	patchTask,
	patchTaskThrottled,
}: ResumableUploadSharedContext) {
	const adjustMultipartInFlight = (taskId: string, delta: number) => {
		const current = multipartInFlightRef.current.get(taskId) ?? 0;
		const next = current + delta;
		if (next <= 0) {
			multipartInFlightRef.current.delete(taskId);
			return;
		}
		multipartInFlightRef.current.set(taskId, next);
	};

	const withTrackedMultipartRequest = async <T>(
		taskId: string,
		run: () => Promise<T>,
	): Promise<T> => {
		adjustMultipartInFlight(taskId, 1);
		try {
			return await run();
		} finally {
			adjustMultipartInFlight(taskId, -1);
		}
	};

	const waitForMultipartDrain = async (taskId: string) => {
		const startedAt = Date.now();
		while ((multipartInFlightRef.current.get(taskId) ?? 0) > 0) {
			if (Date.now() - startedAt >= MULTIPART_DRAIN_TIMEOUT_MS) {
				return;
			}
			await new Promise((resolve) =>
				setTimeout(resolve, MULTIPART_DRAIN_POLL_MS),
			);
		}
	};

	const runRetryableUploadOperation = async <T>({
		onRetryableError,
		run,
	}: {
		onRetryableError?: (error: Error, attempt: number) => void;
		run: () => Promise<T>;
	}): Promise<T> => {
		let lastError: Error | null = null;

		for (let attempt = 0; attempt < CHUNK_MAX_RETRIES; attempt++) {
			try {
				return await run();
			} catch (error) {
				lastError = error instanceof Error ? error : new Error(String(error));
				onRetryableError?.(lastError, attempt);
				if (!isRetryableUploadError(lastError)) {
					break;
				}
				let shouldDelayRetry = true;
				if (isAuthFailureUploadError(lastError)) {
					await useAuthStore.getState().refreshToken();
					shouldDelayRetry = false;
				}
				if (shouldDelayRetry && attempt < CHUNK_MAX_RETRIES - 1) {
					await new Promise((resolve) =>
						setTimeout(resolve, 1000 * 2 ** attempt),
					);
				}
			}
		}

		throw lastError ?? new Error("upload failed");
	};

	const runResumableTransfer = async <TItem>({
		completeUpload,
		initialCompleted,
		initialCompletedBytes,
		items,
		getItemSize,
		processingProgress,
		progressScale,
		task,
		totalItems,
		totalBytes,
		uploadId,
		uploadItem,
		uploadingPatch,
	}: RunResumableTransferOptions<TItem>) => {
		abortFlagsRef.current.set(task.id, false);
		const useByteProgress = Boolean(
			totalBytes && totalBytes > 0 && getItemSize,
		);
		let completed = initialCompleted;
		let completedBytes = initialCompletedBytes ?? 0;
		const inFlightBytes = new Map<TItem, number>();
		const speedTracker = createUploadSpeedTracker(completedBytes);
		const getCurrentProgress = () =>
			useByteProgress && totalBytes
				? calculateByteProgress(
						completedBytes,
						inFlightBytes.values(),
						totalBytes,
						progressScale,
					)
				: calculateProgress(completed, totalItems, progressScale);
		const getCurrentUploadedBytes = () => {
			let uploadedBytes = completedBytes;
			for (const bytes of inFlightBytes.values()) {
				uploadedBytes += bytes;
			}
			return totalBytes
				? Math.min(Math.max(uploadedBytes, 0), totalBytes)
				: Math.max(uploadedBytes, 0);
		};

		patchTask(task.id, {
			...uploadingPatch,
			progress: getCurrentProgress(),
			...(useByteProgress
				? {
						uploadedBytes: getCurrentUploadedBytes(),
						speedBps: undefined,
					}
				: {}),
		});

		const queue = [...items];

		const uploadOneItem = async () => {
			while (queue.length > 0) {
				if (abortFlagsRef.current.get(task.id)) return;
				const item = queue.shift();
				if (item === undefined) return;

				const reportProgress = (loaded: number) => {
					if (!useByteProgress || !getItemSize) return;
					inFlightBytes.set(item, clampLoadedBytes(loaded, getItemSize(item)));
					patchTaskThrottled(task.id, {
						progress: getCurrentProgress(),
						...speedTracker.sample(getCurrentUploadedBytes()),
					});
				};

				await uploadItem(item, reportProgress);
				completed += 1;
				if (useByteProgress && getItemSize) {
					inFlightBytes.delete(item);
					completedBytes += getItemSize(item);
				}
				patchTaskThrottled(task.id, {
					completedChunks: completed,
					progress: getCurrentProgress(),
					...(useByteProgress
						? speedTracker.sample(getCurrentUploadedBytes())
						: {}),
				});
			}
		};

		try {
			const workers = Array.from(
				{ length: Math.min(CHUNK_CONCURRENT, queue.length || 1) },
				() => uploadOneItem(),
			);
			await Promise.all(workers);

			if (abortFlagsRef.current.get(task.id)) {
				patchTask(task.id, {
					status: "cancelled",
					error: null,
					speedBps: undefined,
				});
				return;
			}

			flushProgress();
			patchTask(task.id, {
				status: "processing",
				progress: processingProgress,
				speedBps: undefined,
			});
			await completeUpload();
			removeSession(uploadId);
			patchTask(task.id, {
				status: "completed",
				progress: 100,
				uploadedBytes: totalBytes,
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
			markTaskFailed(task.id, message);
		} finally {
			abortFlagsRef.current.delete(task.id);
		}
	};

	return {
		runResumableTransfer,
		runRetryableUploadOperation,
		waitForMultipartDrain,
		withTrackedMultipartRequest,
	};
}
