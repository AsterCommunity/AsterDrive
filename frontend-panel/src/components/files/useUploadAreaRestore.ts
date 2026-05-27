import type { Dispatch, MutableRefObject, SetStateAction } from "react";
import { useCallback, useEffect } from "react";
import {
	getProcessingProgress,
	getResumePlan,
	type UploadMode,
} from "@/components/files/uploadResume";
import { logger } from "@/lib/logger";
import {
	loadSessions,
	type ResumableSession,
	removeSession,
} from "@/lib/uploadPersistence";
import type { Workspace } from "@/lib/workspace";
import { workspaceKey } from "@/lib/workspace";
import type {
	CompletedPart,
	RecoverableUploadSession,
	UploadProgressResponse,
} from "@/services/uploadService";
import { uploadService } from "@/services/uploadService";
import {
	createTaskId,
	mapAllSettledWithConcurrency,
	RESTORE_PROGRESS_CONCURRENCY,
	shouldRemovePersistedSession,
	type UploadAreaManagerTranslationFn,
	type UploadTask,
} from "./uploadAreaManagerShared";

interface UseUploadAreaRestoreOptions {
	restoredWorkspaceKeysRef: MutableRefObject<Set<string>>;
	resumeCompletionTask: (
		task: UploadTask,
		parts?: CompletedPart[],
	) => Promise<void>;
	setTasks: Dispatch<SetStateAction<UploadTask[]>>;
	setUploadPanelOpen: Dispatch<SetStateAction<boolean>>;
	t: UploadAreaManagerTranslationFn;
	workspace: Workspace;
}

interface RestoreCandidate {
	progress: UploadProgressResponse;
	session: ResumableSession;
}

function recoverableSessionMode(
	mode: RecoverableUploadSession["mode"],
): NonNullable<ResumableSession["mode"]> | null {
	if (
		mode === "chunked" ||
		mode === "presigned" ||
		mode === "presigned_multipart"
	) {
		return mode;
	}
	return null;
}

function mergeCompletedParts(
	serverParts: CompletedPart[] = [],
	localParts: CompletedPart[] = [],
) {
	const parts = new Map<number, CompletedPart>();
	for (const part of serverParts) {
		parts.set(part.part_number, part);
	}
	for (const part of localParts) {
		parts.set(part.part_number, part);
	}
	return Array.from(parts.values()).toSorted(
		(left, right) => left.part_number - right.part_number,
	);
}

function parseTimestamp(value: string) {
	const parsed = Date.parse(value);
	return Number.isFinite(parsed) ? parsed : Date.now();
}

function serverSessionBaseFolderName(
	session: RecoverableUploadSession,
	t: UploadAreaManagerTranslationFn,
) {
	return (session.folder_id ?? null) === null
		? t("files:root")
		: t("files:upload_target_folder", { id: session.folder_id });
}

function serverSessionToLocalSession({
	localSession,
	serverSession,
	t,
	workspace,
}: {
	localSession?: ResumableSession;
	serverSession: RecoverableUploadSession;
	t: UploadAreaManagerTranslationFn;
	workspace: Workspace;
}): ResumableSession | null {
	const mode = recoverableSessionMode(serverSession.mode);
	if (!mode) return null;

	return {
		uploadId: serverSession.upload_id,
		filename: serverSession.filename,
		totalSize: serverSession.total_size,
		totalChunks: serverSession.total_chunks,
		chunkSize: serverSession.chunk_size,
		baseFolderId: serverSession.folder_id ?? null,
		baseFolderName:
			localSession?.baseFolderName ??
			serverSessionBaseFolderName(serverSession, t),
		relativePath: localSession?.relativePath ?? null,
		savedAt: localSession?.savedAt ?? parseTimestamp(serverSession.updated_at),
		workspace,
		mode,
		completedParts: mergeCompletedParts(
			serverSession.completed_parts,
			localSession?.completedParts,
		),
	};
}

function serverSessionToProgress(
	session: RecoverableUploadSession,
): UploadProgressResponse {
	return {
		upload_id: session.upload_id,
		status: session.status,
		received_count: session.received_count,
		chunks_on_disk: session.chunks_on_disk,
		chunk_size: session.chunk_size,
		total_chunks: session.total_chunks,
		filename: session.filename,
	};
}

function restoredCompletedCount(
	mode: UploadMode,
	progress: UploadProgressResponse,
) {
	if (mode === "presigned_multipart") {
		return progress.chunks_on_disk.length;
	}
	return progress.received_count;
}

export function useUploadAreaRestore({
	restoredWorkspaceKeysRef,
	resumeCompletionTask,
	setTasks,
	setUploadPanelOpen,
	t,
	workspace,
}: UseUploadAreaRestoreOptions) {
	const restorePendingSessions = useCallback(async () => {
		const currentWorkspaceKey = workspaceKey(workspace);
		if (restoredWorkspaceKeysRef.current.has(currentWorkspaceKey)) {
			return;
		}
		restoredWorkspaceKeysRef.current.add(currentWorkspaceKey);

		const localSessions = loadSessions(workspace);
		const localSessionsById = new Map(
			localSessions.map((session) => [session.uploadId, session]),
		);
		const candidates: RestoreCandidate[] = [];
		const restoredServerUploadIds = new Set<string>();

		const ghostTasks: UploadTask[] = [];
		const completionTasks: Array<{
			task: UploadTask;
			parts?: CompletedPart[];
		}> = [];

		try {
			const serverSessions = await uploadService.listRecoverableSessions();
			for (const serverSession of serverSessions) {
				restoredServerUploadIds.add(serverSession.upload_id);
				const session = serverSessionToLocalSession({
					localSession: localSessionsById.get(serverSession.upload_id),
					serverSession,
					t,
					workspace,
				});
				if (!session) {
					logger.warn(
						"skipping recoverable upload session with unsupported mode",
						{
							mode: serverSession.mode,
							uploadId: serverSession.upload_id,
						},
					);
					continue;
				}
				candidates.push({
					progress: serverSessionToProgress(serverSession),
					session,
				});
			}
		} catch (error) {
			logger.warn("failed to list recoverable upload sessions", error);
		}

		const localOnlySessions = localSessions.filter(
			(session) => !restoredServerUploadIds.has(session.uploadId),
		);
		const progressResults = await mapAllSettledWithConcurrency(
			localOnlySessions,
			RESTORE_PROGRESS_CONCURRENCY,
			async (session) => {
				try {
					const progress = await uploadService.getProgress(session.uploadId);
					return { session, progress };
				} catch (error) {
					throw { session, error };
				}
			},
		);

		for (const result of progressResults) {
			if (result.status === "rejected") {
				const { error, session } = result.reason as {
					error: unknown;
					session: ResumableSession;
				};
				if (shouldRemovePersistedSession(error)) {
					removeSession(session.uploadId);
				}
				continue;
			}

			candidates.push(result.value);
		}

		if (candidates.length === 0) return;

		for (const { progress, session } of candidates) {
			if (!progress?.status) {
				if (process.env.NODE_ENV === "development") {
					logger.warn(
						"skipping restored upload session because progress is missing a status",
						{
							progress,
							uploadId: session.uploadId,
						},
					);
				}
				continue;
			}

			const mode = session.mode ?? "chunked";
			const plan = getResumePlan(mode, progress.status);
			const completedChunks = restoredCompletedCount(mode, progress);
			if (plan === "restart") {
				removeSession(session.uploadId);
				if (progress.status === "failed") {
					ghostTasks.push({
						id: createTaskId(),
						file: null,
						filename: session.filename,
						relativePath: session.relativePath,
						baseFolderId: session.baseFolderId,
						baseFolderName: session.baseFolderName,
						totalBytes: session.totalSize,
						mode,
						status: "pending_file",
						progress: 0,
						error: t("files:upload_failed"),
						uploadId: null,
						totalChunks: session.totalChunks,
						completedChunks,
					});
				}
				continue;
			}

			const task: UploadTask = {
				id: createTaskId(),
				file: null,
				filename: session.filename,
				relativePath: session.relativePath,
				baseFolderId: session.baseFolderId,
				baseFolderName: session.baseFolderName,
				totalBytes: session.totalSize,
				mode,
				status: plan === "upload" ? "pending_file" : "processing",
				progress: plan === "upload" ? 0 : getProcessingProgress(mode),
				error: null,
				uploadId: session.uploadId,
				totalChunks: session.totalChunks,
				completedChunks:
					plan === "upload" ? completedChunks : session.totalChunks,
			};
			ghostTasks.push(task);
			if (plan === "complete") {
				completionTasks.push({
					task,
					parts:
						mode === "presigned_multipart"
							? (session.completedParts ?? [])
							: undefined,
				});
			}
		}

		if (ghostTasks.length > 0) {
			setTasks((prev) => [...ghostTasks, ...prev]);
			setUploadPanelOpen(true);
			for (const completionTask of completionTasks) {
				void resumeCompletionTask(completionTask.task, completionTask.parts);
			}
		}
	}, [
		resumeCompletionTask,
		restoredWorkspaceKeysRef,
		setTasks,
		setUploadPanelOpen,
		t,
		workspace,
	]);

	useEffect(() => {
		const timer = window.setTimeout(() => {
			void restorePendingSessions();
		}, 600);
		return () => window.clearTimeout(timer);
	}, [restorePendingSessions]);
}
