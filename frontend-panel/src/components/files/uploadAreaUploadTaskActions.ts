import type { Dispatch, MutableRefObject, SetStateAction } from "react";
import { getResumePlan } from "@/components/files/uploadResume";
import {
	loadSessions,
	removeSession,
	saveSession,
} from "@/lib/uploadPersistence";
import type { Workspace } from "@/lib/workspace";
import {
	type InitUploadResponse,
	uploadService,
} from "@/services/uploadService";
import {
	shouldRemovePersistedSession,
	TERMINAL_UPLOAD_STATUS_SET,
	type UploadAreaManagerTranslationFn,
	type UploadTask,
} from "./uploadAreaManagerShared";
import {
	abortUploadRequests,
	type UploadModeRunners,
	type UploadRequestRef,
} from "./uploadAreaUploadRunnerShared";

export type UploadTaskOperation = "clear" | "retry";
export type UploadTaskOperationLocks = Map<string, UploadTaskOperation>;

function tryAcquireTaskOperation(
	locks: UploadTaskOperationLocks,
	taskId: string,
	operation: UploadTaskOperation,
) {
	if (locks.has(taskId)) return false;
	locks.set(taskId, operation);
	return true;
}

export interface UploadTaskActionsContext extends UploadModeRunners {
	abortFlagsRef: MutableRefObject<Map<string, boolean>>;
	directAbortRef: MutableRefObject<Map<string, AbortController>>;
	markTaskFailed: (taskId: string, error: unknown) => void;
	patchTask: (taskId: string, patch: Partial<UploadTask>) => void;
	setTasks: Dispatch<SetStateAction<UploadTask[]>>;
	setUploadPanelOpen: Dispatch<SetStateAction<boolean>>;
	taskOperationLocks: UploadTaskOperationLocks;
	t: UploadAreaManagerTranslationFn;
	tasksRef: MutableRefObject<UploadTask[]>;
	uploadRequestRef: UploadRequestRef;
	workspace: Workspace;
}

interface ClearTerminalUploadTasksContext {
	setTasks: Dispatch<SetStateAction<UploadTask[]>>;
	taskOperationLocks: UploadTaskOperationLocks;
	tasksRef: MutableRefObject<UploadTask[]>;
}

function createSavedSession(
	task: UploadTask,
	init: InitUploadResponse,
	workspace: Workspace,
) {
	return {
		uploadId: init.upload_id as string,
		filename: task.file?.name ?? task.filename,
		totalSize: task.file?.size ?? 0,
		totalChunks: init.total_chunks ?? 0,
		chunkSize: init.chunk_size ?? 0,
		baseFolderId: task.baseFolderId,
		baseFolderName: task.baseFolderName,
		relativePath: task.relativePath,
		savedAt: Date.now(),
		workspace,
		mode:
			init.mode === "presigned_multipart"
				? ("presigned_multipart" as const)
				: init.mode === "provider_resumable"
					? ("provider_resumable" as const)
					: ("chunked" as const),
	};
}

export async function runQueuedUploadTask(
	taskId: string,
	{
		markTaskFailed,
		patchTask,
		resumeCompletionTask,
		runChunkedUpload,
		runDirectUpload,
		runMultipartUpload,
		runPresignedUpload,
		runProviderResumableUpload,
		tasksRef,
		workspace,
	}: UploadTaskActionsContext,
) {
	const task = tasksRef.current.find((item) => item.id === taskId);
	if (task?.status !== "queued" || !task.file) return;

	const file = task.file;
	patchTask(taskId, {
		status: "initializing",
		error: null,
		progress: 0,
		uploadedBytes: 0,
		speedBps: undefined,
	});

	try {
		if (
			task.uploadId &&
			(task.mode === "chunked" ||
				task.mode === "presigned_multipart" ||
				task.mode === "provider_resumable")
		) {
			try {
				const progress = await uploadService.getProgress(task.uploadId);
				const plan = getResumePlan(task.mode, progress.status);
				if (plan === "restart") {
					removeSession(task.uploadId);
					patchTask(taskId, {
						uploadId: null,
						completedChunks: 0,
						totalChunks: 0,
						mode: null,
					});
				}
				if (plan !== "restart") {
					const saved = loadSessions(workspace).find(
						(session) => session.uploadId === task.uploadId,
					);
					if (plan === "complete") {
						await resumeCompletionTask(
							task,
							task.mode === "presigned_multipart"
								? (saved?.completedParts ?? [])
								: undefined,
						);
						return;
					}

					const chunkSize =
						(
							progress as typeof progress & {
								chunk_size?: number;
							}
						).chunk_size ?? saved?.chunkSize;
					if (!chunkSize || chunkSize <= 0) {
						throw new Error("missing resumable chunk size");
					}

					if (task.mode === "chunked") {
						await runChunkedUpload(
							task,
							{
								mode: "chunked",
								upload_id: task.uploadId,
								chunk_size: chunkSize,
								total_chunks: progress.total_chunks,
								upload_scheduling: progress.upload_scheduling,
							},
							progress.chunks_on_disk,
						);
					} else if (task.mode === "presigned_multipart") {
						await runMultipartUpload(
							task,
							{
								mode: "presigned_multipart",
								upload_id: task.uploadId,
								chunk_size: chunkSize,
								total_chunks: progress.total_chunks,
							},
							saved?.completedParts ?? [],
						);
					} else if (task.mode === "provider_resumable") {
						await runProviderResumableUpload(
							task,
							{
								mode: "provider_resumable",
								upload_id: task.uploadId,
								chunk_size: chunkSize,
								total_chunks: progress.total_chunks,
								provider_resumable: progress.provider_resumable,
								upload_scheduling: progress.upload_scheduling,
							},
							progress.chunks_on_disk,
						);
					}
					return;
				}
			} catch (error) {
				if (shouldRemovePersistedSession(error)) {
					removeSession(task.uploadId);
					patchTask(taskId, {
						uploadId: null,
						completedChunks: 0,
						totalChunks: 0,
						mode: null,
					});
				} else {
					markTaskFailed(taskId, error);
					return;
				}
			}
		}

		const init = await uploadService.initUpload({
			filename: file.name,
			total_size: file.size,
			folder_id: task.baseFolderId,
			relative_path: task.relativePath ?? undefined,
		});

		if (
			(init.mode === "chunked" ||
				init.mode === "presigned_multipart" ||
				init.mode === "provider_resumable") &&
			init.upload_id
		) {
			saveSession(createSavedSession(task, init, workspace));
		}

		if (init.mode === "chunked") {
			await runChunkedUpload(task, init);
			return;
		}
		if (init.mode === "presigned_multipart") {
			await runMultipartUpload(task, init);
			return;
		}
		if (init.mode === "provider_resumable") {
			await runProviderResumableUpload(task, init);
			return;
		}
		if (init.mode === "presigned") {
			await runPresignedUpload(task, init);
			return;
		}
		await runDirectUpload(task);
	} catch (error) {
		markTaskFailed(taskId, error);
	}
}

export async function cancelUploadTask(
	taskId: string,
	{
		abortFlagsRef,
		cancelMultipartSession,
		directAbortRef,
		patchTask,
		setTasks,
		tasksRef,
		uploadRequestRef,
	}: UploadTaskActionsContext,
) {
	const task = tasksRef.current.find((item) => item.id === taskId);
	if (!task) return;

	if (task.mode === "direct") {
		directAbortRef.current.get(taskId)?.abort();
		patchTask(taskId, { status: "cancelled", error: null });
		return;
	}

	if (task.mode === "presigned") {
		abortUploadRequests(uploadRequestRef, taskId);
		if (task.uploadId) {
			try {
				await uploadService.cancelUpload(task.uploadId);
			} catch {}
		}
		patchTask(taskId, { status: "cancelled", error: null });
		return;
	}

	if (task.status === "pending_file") {
		if (task.uploadId) {
			try {
				await uploadService.cancelUpload(task.uploadId);
			} catch {}
			removeSession(task.uploadId);
		}
		setTasks((prev) => prev.filter((item) => item.id !== taskId));
		return;
	}

	if (
		task.mode === "chunked" ||
		task.mode === "presigned_multipart" ||
		task.mode === "provider_resumable"
	) {
		await cancelMultipartSession(task);
		patchTask(taskId, { status: "cancelled", error: null });
		return;
	}

	abortFlagsRef.current.set(taskId, true);
	if (task.uploadId) {
		try {
			await uploadService.cancelUpload(task.uploadId);
		} catch {}
		removeSession(task.uploadId);
	}
	patchTask(taskId, { status: "cancelled", error: null });
}

export async function clearTerminalUploadTasks(
	taskIds: readonly string[],
	{ setTasks, taskOperationLocks, tasksRef }: ClearTerminalUploadTasksContext,
) {
	const requestedIds = new Set(taskIds);
	const tasksToClear = tasksRef.current.filter(
		(task) =>
			requestedIds.has(task.id) &&
			TERMINAL_UPLOAD_STATUS_SET.has(task.status) &&
			tryAcquireTaskOperation(taskOperationLocks, task.id, "clear"),
	);
	if (tasksToClear.length === 0) return;

	const clearedIds = new Set<string>();
	try {
		await Promise.allSettled(
			tasksToClear.map(async (task) => {
				if (task.status === "failed" && task.uploadId) {
					try {
						await uploadService.cancelUpload(task.uploadId);
					} catch {}
				}
				const currentTask = tasksRef.current.find(
					(item) => item.id === task.id,
				);
				if (
					taskOperationLocks.get(task.id) === "clear" &&
					currentTask?.status === task.status &&
					currentTask.uploadId === task.uploadId
				) {
					clearedIds.add(task.id);
				}
			}),
		);

		for (const task of tasksToClear) {
			if (clearedIds.has(task.id) && task.uploadId) {
				removeSession(task.uploadId);
			}
		}
		if (clearedIds.size > 0) {
			setTasks((prev) => prev.filter((task) => !clearedIds.has(task.id)));
		}
	} finally {
		for (const task of tasksToClear) {
			if (taskOperationLocks.get(task.id) === "clear") {
				taskOperationLocks.delete(task.id);
			}
		}
	}
}

export async function retryUploadTask(
	taskId: string,
	{
		cancelMultipartSession,
		patchTask,
		resumeCompletionTask,
		setUploadPanelOpen,
		tasksRef,
		taskOperationLocks,
		workspace,
	}: UploadTaskActionsContext,
) {
	if (!tryAcquireTaskOperation(taskOperationLocks, taskId, "retry")) return;
	try {
		const task = tasksRef.current.find((item) => item.id === taskId);
		if (!task || (task.status === "failed" && task.retryable === false)) return;

		if (!task.file && task.uploadId) {
			const saved = loadSessions(workspace).find(
				(session) => session.uploadId === task.uploadId,
			);
			await resumeCompletionTask(
				task,
				task.mode === "presigned_multipart"
					? (saved?.completedParts ?? [])
					: undefined,
			);
			setUploadPanelOpen(true);
			return;
		}

		if (task.uploadId) {
			if (
				task.mode === "chunked" ||
				task.mode === "presigned_multipart" ||
				task.mode === "provider_resumable"
			) {
				await cancelMultipartSession(task);
			} else {
				void uploadService.cancelUpload(task.uploadId).catch(() => undefined);
				removeSession(task.uploadId);
			}
		}

		patchTask(taskId, {
			status: "queued",
			progress: 0,
			uploadedBytes: 0,
			speedBps: undefined,
			error: null,
			retryable: undefined,
			uploadId: null,
			completedChunks: 0,
			totalChunks: 0,
			mode: null,
		});
		setUploadPanelOpen(true);
	} finally {
		if (taskOperationLocks.get(taskId) === "retry") {
			taskOperationLocks.delete(taskId);
		}
	}
}
