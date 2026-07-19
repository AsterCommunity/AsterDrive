import type { MutableRefObject } from "react";
import type { Workspace } from "@/lib/workspace";
import type {
	CompletedPart,
	InitUploadResponse,
} from "@/services/uploadService";
import type {
	UploadAreaManagerTranslationFn,
	UploadTask,
} from "./uploadAreaManagerShared";

export interface UploadModeRunnerContext {
	abortFlagsRef: MutableRefObject<Map<string, boolean>>;
	directAbortRef: MutableRefObject<Map<string, AbortController>>;
	flushProgress: () => void;
	markFolderForRefresh: (task: UploadTask) => void;
	markTaskFailed: (taskId: string, message: string) => void;
	multipartInFlightRef: MutableRefObject<Map<string, number>>;
	patchTask: (taskId: string, patch: Partial<UploadTask>) => void;
	patchTaskThrottled: (taskId: string, patch: Partial<UploadTask>) => void;
	uploadRequestRef: MutableRefObject<Map<string, Set<XMLHttpRequest>>>;
	t: UploadAreaManagerTranslationFn;
	workspace: Workspace;
}

export interface UploadModeRunners {
	cancelMultipartSession: (task: UploadTask) => Promise<void>;
	resumeCompletionTask: (
		task: UploadTask,
		parts?: CompletedPart[],
	) => Promise<void>;
	runChunkedUpload: (
		task: UploadTask,
		init: InitUploadResponse,
		alreadyReceived?: number[],
	) => Promise<void>;
	runDirectUpload: (task: UploadTask) => Promise<void>;
	runMultipartUpload: (
		task: UploadTask,
		init: InitUploadResponse,
		alreadyCompleted?: CompletedPart[],
	) => Promise<void>;
	runPresignedUpload: (
		task: UploadTask,
		init: InitUploadResponse,
	) => Promise<void>;
	runProviderResumableUpload: (
		task: UploadTask,
		init: InitUploadResponse,
		alreadyReceived?: number[],
	) => Promise<void>;
}

export type UploadRequestRef = MutableRefObject<
	Map<string, Set<XMLHttpRequest>>
>;

export function registerUploadRequest(
	requestRef: UploadRequestRef,
	taskId: string,
	xhr: XMLHttpRequest,
): void {
	const tracked = requestRef.current.get(taskId);
	if (tracked) {
		tracked.add(xhr);
		return;
	}
	requestRef.current.set(taskId, new Set([xhr]));
}

function getTrackedUploadRequests(
	requestRef: UploadRequestRef,
	taskId: string,
): Set<XMLHttpRequest> | undefined {
	return requestRef.current.get(taskId);
}

export function unregisterUploadRequest(
	requestRef: UploadRequestRef,
	taskId: string,
	xhr: XMLHttpRequest,
): void {
	const tracked = requestRef.current.get(taskId);
	if (!tracked) return;
	tracked.delete(xhr);
	if (tracked.size === 0) {
		requestRef.current.delete(taskId);
	}
}

export function abortUploadRequests(
	requestRef: UploadRequestRef,
	taskId: string,
): void {
	const tracked = getTrackedUploadRequests(requestRef, taskId);
	if (!tracked) return;
	for (const xhr of tracked) {
		xhr.abort();
	}
	requestRef.current.delete(taskId);
}

export function abortAllUploadRequests(requestRef: UploadRequestRef): void {
	for (const tracked of requestRef.current.values()) {
		for (const xhr of tracked) {
			xhr.abort();
		}
	}
	requestRef.current.clear();
}

export async function withTrackedUploadRequest<T>(
	requestRef: UploadRequestRef,
	taskId: string,
	run: (onCreateXhr: (xhr: XMLHttpRequest) => void) => Promise<T>,
): Promise<T> {
	let trackedXhr: XMLHttpRequest | null = null;
	try {
		return await run((xhr) => {
			trackedXhr = xhr;
			registerUploadRequest(requestRef, taskId, xhr);
		});
	} finally {
		if (trackedXhr) {
			unregisterUploadRequest(requestRef, taskId, trackedXhr);
		}
	}
}
