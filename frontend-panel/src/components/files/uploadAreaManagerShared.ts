import { ApiError } from "@/services/http";
import { type CompletedPart, uploadService } from "@/services/uploadService";
import type { FileInfo } from "@/types/api";
import { ErrorCode } from "@/types/api-helpers";
import type { UploadMode } from "./uploadResume";

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
	id: string;
	file: File | null;
	filename: string;
	relativePath: string | null;
	baseFolderId: number | null;
	baseFolderName: string;
	totalBytes: number;
	mode: UploadMode | null;
	status: UploadStatus;
	progress: number;
	uploadedBytes?: number;
	speedBps?: number;
	error: string | null;
	uploadId: string | null;
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
		(error.code === ErrorCode.UploadSessionNotFound ||
			error.code === ErrorCode.UploadSessionExpired)
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
	uploadId: string,
	parts?: CompletedPart[],
): Promise<FileInfo> {
	const MAX_POLL = 30;
	const POLL_INTERVAL_MS = 10_000;

	for (let i = 0; i < MAX_POLL; i++) {
		try {
			return await uploadService.completeUpload(uploadId, parts);
		} catch (error) {
			if (
				error instanceof ApiError &&
				error.code === ErrorCode.UploadAssembling &&
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
}: {
	baseFolderId: number | null;
	baseFolderName: string;
	file: File;
	relativePath: string | null;
}): UploadTask {
	return {
		id: createTaskId(),
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
	};
}
