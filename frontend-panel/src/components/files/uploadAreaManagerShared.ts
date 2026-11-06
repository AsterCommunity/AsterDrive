import {
	assertCurrentAuthOperation,
	type UploadOperationBinding,
} from "@/lib/authOperationOwner";
import { ApiError } from "@/services/http";
import {
	type CompletedPart,
	createUploadService,
} from "@/services/uploadService";
import type { FileInfo } from "@/types/api";
import { ApiErrorCode } from "@/types/api-helpers";
import type { UploadTransport } from "./uploadResume";

export type UploadStatus =
	| "pending_file"
	| "queued"
	| "initializing"
	| "uploading"
	| "processing"
	| "completed"
	| "failed"
	| "cancelled";

export interface UploadTask {
	/** 目标和账号在入队时冻结，页面导航不会重定向后续分片或 complete。 */
	readonly binding: UploadOperationBinding;
	id: string;
	file: File | null;
	filename: string;
	relativePath: string | null;
	baseFolderId: number | null;
	baseFolderName: string;
	totalBytes: number;
	mode: UploadTransport | null;
	status: UploadStatus;
	progress: number;
	uploadedBytes?: number;
	speedBps?: number;
	error: string | null;
	retryable?: boolean;
	uploadId: string | null;
	/** Reused by zero-byte metadata-only creation across retries. */
	emptyFileIdempotencyKey?: string;
	completedChunks?: number;
	totalChunks?: number;
}

export type UploadAreaManagerTranslationFn = (
	key: string,
	options?: Record<string, unknown>,
) => string;

export const ACTIVE_QUEUE_STATUSES: UploadStatus[] = [
	"queued",
	"initializing",
	"uploading",
	"processing",
];
export const ACTIVE_QUEUE_STATUS_SET = new Set<UploadStatus>(
	ACTIVE_QUEUE_STATUSES,
);

export const TERMINAL_UPLOAD_STATUS_SET = new Set<UploadStatus>([
	"completed",
	"failed",
	"cancelled",
]);

export const CONCURRENCY_ACTIVE_STATUSES: UploadStatus[] = [
	"initializing",
	"uploading",
	"processing",
];

export const CHUNK_CONCURRENT = 3;
export const CHUNK_MAX_RETRIES = 3;
export const PROGRESS_FLUSH_INTERVAL = 500;
export const RESTORE_PROGRESS_CONCURRENCY = 4;
export const MULTIPART_DRAIN_TIMEOUT_MS = 3000;
export const MULTIPART_DRAIN_POLL_MS = 50;

export function shouldRemovePersistedSession(error: unknown): boolean {
	return (
		error instanceof ApiError &&
		(error.code === ApiErrorCode.UploadSessionNotFound ||
			error.code === ApiErrorCode.UploadSessionExpired)
	);
}

export async function mapAllSettledWithConcurrency<T, R>(
	items: readonly T[],
	concurrency: number,
	mapper: (item: T) => Promise<R>,
): Promise<PromiseSettledResult<R>[]> {
	const batchSize = Math.max(1, concurrency);
	const results: PromiseSettledResult<R>[] = [];

	for (let start = 0; start < items.length; start += batchSize) {
		const batch = items.slice(start, start + batchSize);
		results.push(...(await Promise.allSettled(batch.map(mapper))));
	}

	return results;
}

export async function completeWithRetry(
	task: UploadTask,
	uploadId: string,
	parts?: CompletedPart[],
): Promise<FileInfo> {
	const MAX_POLL = 30;
	const POLL_INTERVAL_MS = 10_000;

	for (let i = 0; i < MAX_POLL; i++) {
		try {
			return await uploadServiceForTask(task).completeUpload(uploadId, parts);
		} catch (error) {
			if (
				error instanceof ApiError &&
				error.code === ApiErrorCode.UploadAssembling &&
				i < MAX_POLL - 1
			) {
				await new Promise((resolve) => setTimeout(resolve, POLL_INTERVAL_MS));
				continue;
			}
			throw error;
		}
	}

	throw new Error("Upload timed out waiting for assembly");
}

export function createTaskId() {
	return `${Date.now()}-${Math.random().toString(36).slice(2, 10)}`;
}

export function createQueuedUploadTask({
	baseFolderId,
	baseFolderName,
	file,
	relativePath,
	binding,
}: {
	baseFolderId: number | null;
	baseFolderName: string;
	file: File;
	binding: UploadOperationBinding;
	relativePath: string | null;
}): UploadTask {
	const id = createTaskId();
	return {
		id,
		binding,
		file,
		filename: file.name,
		relativePath,
		baseFolderId,
		baseFolderName,
		totalBytes: file.size,
		mode: null,
		status: "queued",
		progress: 0,
		uploadedBytes: 0,
		error: null,
		uploadId: null,
		...(file.size === 0
			? { emptyFileIdempotencyKey: `empty-upload:${id}` }
			: {}),
	};
}

/** 请求前与 await 后都核对同一身份。signal 使底层 HTTP/XHR 在退出时中止；
 * promise guard 则阻止迟到 init 响应继续启动下一阶段或污染新账号的任务状态。 */
export function uploadServiceForTask(
	task: UploadTask,
): ReturnType<typeof createUploadService> {
	assertCurrentAuthOperation(task.binding);
	const service = createUploadService(
		task.binding.workspace,
		task.binding.signal,
	);
	return new Proxy(service, {
		get(target, property) {
			const method = Reflect.get(target, property);
			return (...args: unknown[]) => {
				assertCurrentAuthOperation(task.binding);
				const result = Reflect.apply(method, target, args);
				if (result instanceof Promise) {
					return result.then((value: unknown) => {
						assertCurrentAuthOperation(task.binding);
						return value;
					});
				}
				return result;
			};
		},
	});
}
