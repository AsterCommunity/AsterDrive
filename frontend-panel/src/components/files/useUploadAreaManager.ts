import type { ChangeEvent, DragEvent, RefObject } from "react";
import {
	useCallback,
	useEffect,
	useLayoutEffect,
	useRef,
	useState,
} from "react";
import { useTranslation } from "react-i18next";
import { formatBytes } from "@/lib/format";
import { decideUploadQueueSettledRefresh } from "@/lib/storageMutationCoordinator";
import {
	clearDeferredStorageRefresh,
	consumeDeferredStorageRefresh,
	enterStorageRefreshGate,
	leaveStorageRefreshGate,
} from "@/lib/storageRefreshGate";
import { loadSessions } from "@/lib/uploadPersistence";
import {
	normalizeUploadConcurrency,
	readUploadSettings,
	writeUploadAutoClearCompleted,
	writeUploadConcurrency,
} from "@/lib/uploadSettings";
import type { Workspace } from "@/lib/workspace";
import type { MeField } from "@/types/api";
import {
	extractFilesFromDrop,
	extractFilesFromInput,
	type FileWithPath,
	hasDirectoryInDropItems,
} from "@/utils/directoryUtils";
import {
	ACTIVE_QUEUE_STATUSES,
	CONCURRENCY_ACTIVE_STATUSES,
	createQueuedUploadTask,
	PROGRESS_FLUSH_INTERVAL,
	type UploadStatus,
	type UploadTask,
} from "./uploadAreaManagerShared";
import {
	buildUploadTaskViews,
	summarizeUploadTasks,
} from "./uploadAreaManagerView";
import { abortAllUploadRequests } from "./uploadAreaUploadRunnerShared";
import { useUploadAreaRestore } from "./useUploadAreaRestore";
import { useUploadAreaUploads } from "./useUploadAreaUploads";

interface UseUploadAreaManagerOptions {
	breadcrumb: Array<{ id: number | null; name: string }>;
	currentFolderId: number | null;
	refresh: () => Promise<void>;
	refreshUser: (options?: { fields?: MeField[] }) => Promise<void>;
	resumeFileInputRef: RefObject<HTMLInputElement | null>;
	storageEventStreamEnabled: boolean;
	workspace: Workspace;
}

export function useUploadAreaManager({
	breadcrumb,
	currentFolderId,
	refresh,
	refreshUser,
	resumeFileInputRef,
	storageEventStreamEnabled,
	workspace,
}: UseUploadAreaManagerOptions) {
	const { t } = useTranslation(["core", "files"]);
	const currentFolderIdRef = useRef(currentFolderId);
	const [isDragging, setIsDragging] = useState(false);
	const dragCounter = useRef(0);
	const [uploadPanelOpen, setUploadPanelOpen] = useState(true);
	const [uploadSettings, setUploadSettings] = useState(readUploadSettings);
	const [tasks, setTasks] = useState<UploadTask[]>([]);
	const [hasUploadActivity, setHasUploadActivity] = useState(false);
	const tasksRef = useRef<UploadTask[]>([]);
	const abortFlagsRef = useRef(new Map<string, boolean>());
	const directAbortRef = useRef(new Map<string, AbortController>());
	const uploadRequestRef = useRef(new Map<string, Set<XMLHttpRequest>>());
	const multipartInFlightRef = useRef(new Map<string, number>());
	const pendingRefreshFolderIdsRef = useRef(new Set<number | null>());
	const previousTaskCountRef = useRef(0);
	const queueWasActiveRef = useRef(false);
	const resumeTaskIdRef = useRef<string | null>(null);
	const restoredWorkspaceKeysRef = useRef(new Set<string>());
	const progressBufferRef = useRef(new Map<string, Partial<UploadTask>>());
	const progressFlushTimerRef = useRef<number | null>(null);

	useLayoutEffect(() => {
		currentFolderIdRef.current = currentFolderId;
	}, [currentFolderId]);

	useLayoutEffect(() => {
		tasksRef.current = tasks;
	}, [tasks]);

	useEffect(() => {
		const previousTaskCount = previousTaskCountRef.current;
		if (tasks.length > 0) {
			setHasUploadActivity(true);
		} else if (previousTaskCount > 0) {
			setUploadPanelOpen(false);
		}
		previousTaskCountRef.current = tasks.length;
	}, [tasks.length]);

	useEffect(() => {
		const directAbortControllers = directAbortRef.current;
		const progressFlushTimerState = progressFlushTimerRef;
		const queueWasActiveState = queueWasActiveRef;
		const uploadRequests = uploadRequestRef;

		return () => {
			for (const controller of directAbortControllers.values()) {
				controller.abort();
			}
			abortAllUploadRequests(uploadRequests);
			if (progressFlushTimerState.current !== null) {
				window.clearTimeout(progressFlushTimerState.current);
			}
			if (queueWasActiveState.current) {
				leaveStorageRefreshGate();
				queueWasActiveState.current = false;
				clearDeferredStorageRefresh();
			}
		};
	}, []);

	const patchTask = useCallback(
		(taskId: string, patch: Partial<UploadTask>) => {
			const terminalStatuses: UploadStatus[] = ["completed", "cancelled"];
			const normalizedPatch =
				patch.status && patch.status !== "uploading"
					? { ...patch, speedBps: undefined }
					: patch;
			if (patch.status && patch.status !== "uploading") {
				progressBufferRef.current.delete(taskId);
			}
			if (uploadSettings.autoClearCompleted && patch.status === "completed") {
				setTasks((prev) => prev.filter((task) => task.id !== taskId));
				return;
			}
			const finalPatch =
				normalizedPatch.status &&
				terminalStatuses.includes(normalizedPatch.status)
					? { ...normalizedPatch, file: null }
					: normalizedPatch;
			setTasks((prev) =>
				prev.map((task) =>
					task.id === taskId ? { ...task, ...finalPatch } : task,
				),
			);
		},
		[uploadSettings.autoClearCompleted],
	);

	const flushProgress = useCallback(() => {
		progressFlushTimerRef.current = null;
		const buffer = progressBufferRef.current;
		if (buffer.size === 0) return;

		const updates = new Map(buffer);
		buffer.clear();
		setTasks((prev) =>
			prev.map((task) => {
				const patch = updates.get(task.id);
				return patch ? { ...task, ...patch } : task;
			}),
		);
	}, []);

	const patchTaskThrottled = useCallback(
		(taskId: string, patch: Partial<UploadTask>) => {
			const existing = progressBufferRef.current.get(taskId);
			progressBufferRef.current.set(
				taskId,
				existing ? { ...existing, ...patch } : patch,
			);
			if (progressFlushTimerRef.current === null) {
				progressFlushTimerRef.current = window.setTimeout(
					flushProgress,
					PROGRESS_FLUSH_INTERVAL,
				);
			}
		},
		[flushProgress],
	);

	const markFolderForRefresh = useCallback((task: UploadTask) => {
		pendingRefreshFolderIdsRef.current.add(task.baseFolderId);
	}, []);

	useEffect(() => {
		const hasActiveQueue = tasks.some((task) =>
			ACTIVE_QUEUE_STATUSES.includes(task.status),
		);

		if (hasActiveQueue) {
			if (!queueWasActiveRef.current) {
				enterStorageRefreshGate();
			}
			queueWasActiveRef.current = true;
			return;
		}

		if (!queueWasActiveRef.current) {
			return;
		}

		leaveStorageRefreshGate();
		queueWasActiveRef.current = false;

		const pendingRefreshFolderIds = pendingRefreshFolderIdsRef.current;
		const hasDeferredRefresh = consumeDeferredStorageRefresh();
		if (pendingRefreshFolderIds.size === 0 && !hasDeferredRefresh) {
			return;
		}

		const refreshDecision = decideUploadQueueSettledRefresh({
			currentFolderId: currentFolderIdRef.current,
			hasDeferredRemoteRefresh: hasDeferredRefresh,
			pendingRefreshFolderIds,
			storageEventStreamEnabled,
		});
		pendingRefreshFolderIdsRef.current = new Set();

		if (refreshDecision.refreshPersonalQuota) {
			void refreshUser({ fields: ["quota"] });
		}
		if (refreshDecision.refreshCurrentFolder) {
			void refresh();
		}
	}, [refresh, refreshUser, storageEventStreamEnabled, tasks]);

	const clearCompletedTasks = useCallback(() => {
		setTasks((prev) => prev.filter((task) => task.status !== "completed"));
	}, []);

	const setUploadConcurrency = useCallback((value: number) => {
		const concurrency = normalizeUploadConcurrency(value);
		writeUploadConcurrency(concurrency);
		setUploadSettings((prev) => {
			return {
				...prev,
				concurrency,
			};
		});
	}, []);

	const setUploadAutoClearCompleted = useCallback(
		(value: boolean) => {
			writeUploadAutoClearCompleted(value);
			setUploadSettings((prev) => ({
				...prev,
				autoClearCompleted: value,
			}));
			if (value) {
				clearCompletedTasks();
			}
		},
		[clearCompletedTasks],
	);

	const attachFileToTask = useCallback(
		(taskId: string, file: File) => {
			const sessions = loadSessions(workspace);
			setTasks((prev) => {
				const task = prev.find((item) => item.id === taskId);
				if (task?.status !== "pending_file") return prev;

				const session = sessions.find(
					(entry) => entry.uploadId === task.uploadId,
				);
				const patch =
					session &&
					(file.name !== session.filename || file.size !== session.totalSize)
						? {
								error: t("files:upload_resume_mismatch", {
									name: session.filename,
									size: formatBytes(session.totalSize),
								}),
							}
						: {
								file,
								totalBytes: file.size,
								status: "queued" as const,
								error: null,
								progress: 0,
								uploadedBytes: 0,
								speedBps: undefined,
							};

				return prev.map((item) =>
					item.id === taskId ? { ...item, ...patch } : item,
				);
			});
		},
		[t, workspace],
	);

	const handleResumeFileChange = useCallback(
		(event: ChangeEvent<HTMLInputElement>) => {
			const files = event.target.files;
			const taskId = resumeTaskIdRef.current;
			if (!files?.[0] || !taskId) return;
			attachFileToTask(taskId, files[0]);
			event.target.value = "";
			resumeTaskIdRef.current = null;
		},
		[attachFileToTask],
	);

	const triggerResumeFilePicker = useCallback(() => {
		resumeFileInputRef.current?.click();
	}, [resumeFileInputRef]);

	const requestResumeFilePicker = useCallback(
		(taskId: string) => {
			resumeTaskIdRef.current = taskId;
			triggerResumeFilePicker();
		},
		[triggerResumeFilePicker],
	);

	const markTaskFailed = useCallback(
		(taskId: string, message: string) => {
			patchTask(taskId, {
				status: "failed",
				error: message,
				speedBps: undefined,
			});
		},
		[patchTask],
	);

	const { cancelTask, resumeCompletionTask, retryTask, runTask } =
		useUploadAreaUploads({
			abortFlagsRef,
			directAbortRef,
			flushProgress,
			markFolderForRefresh,
			markTaskFailed,
			multipartInFlightRef,
			patchTask,
			patchTaskThrottled,
			uploadRequestRef,
			setTasks,
			setUploadPanelOpen,
			t,
			tasksRef,
			workspace,
		});

	useUploadAreaRestore({
		restoredWorkspaceKeysRef,
		resumeCompletionTask,
		setTasks,
		setUploadPanelOpen,
		t,
		workspace,
	});

	useEffect(() => {
		const activeCount = tasks.filter((task) =>
			CONCURRENCY_ACTIVE_STATUSES.includes(task.status),
		).length;
		const concurrency = uploadSettings.concurrency;
		if (activeCount >= concurrency) return;

		const queued = tasks.filter((task) => task.status === "queued");
		if (queued.length === 0) return;

		const nextTasks = queued.slice(0, concurrency - activeCount);
		for (const task of nextTasks) {
			void runTask(task.id);
		}
	}, [runTask, tasks, uploadSettings.concurrency]);

	const retryFailedTasks = useCallback(() => {
		const failedTaskIds = tasksRef.current.flatMap((task) =>
			task.status === "failed" ? [task.id] : [],
		);
		for (const taskId of failedTaskIds) {
			void retryTask(taskId);
		}
	}, [retryTask]);

	const addFilesWithPath = useCallback(
		(files: FileWithPath[]) => {
			if (files.length === 0) return;
			const baseFolderId = currentFolderIdRef.current;
			const baseFolderName =
				breadcrumb[breadcrumb.length - 1]?.name ?? t("files:root");
			const nextTasks = files.map(({ file, relativePath }) =>
				createQueuedUploadTask({
					baseFolderId,
					baseFolderName,
					file,
					relativePath,
				}),
			);
			setTasks((prev) => [...nextTasks, ...prev]);
			setUploadPanelOpen(true);
		},
		[breadcrumb, t],
	);

	const addFiles = useCallback(
		(files: FileList | null) => {
			if (!files || files.length === 0) return;
			const baseFolderId = currentFolderIdRef.current;
			const baseFolderName =
				breadcrumb[breadcrumb.length - 1]?.name ?? t("files:root");
			const nextTasks = Array.from(files).map((file) =>
				createQueuedUploadTask({
					baseFolderId,
					baseFolderName,
					file,
					relativePath: null,
				}),
			);
			setTasks((prev) => [...nextTasks, ...prev]);
			setUploadPanelOpen(true);
		},
		[breadcrumb, t],
	);

	const handleFileInputChange = useCallback(
		(event: ChangeEvent<HTMLInputElement>) => {
			addFiles(event.target.files);
			event.target.value = "";
		},
		[addFiles],
	);

	const handleFolderInputChange = useCallback(
		(event: ChangeEvent<HTMLInputElement>) => {
			const files = event.target.files;
			if (!files) return;
			addFilesWithPath(extractFilesFromInput(files));
			event.target.value = "";
		},
		[addFilesWithPath],
	);

	const handleDragEnter = useCallback((event: DragEvent<HTMLDivElement>) => {
		event.preventDefault();
		dragCounter.current += 1;
		if (event.dataTransfer.types.includes("Files")) {
			setIsDragging(true);
		}
	}, []);

	const handleDragLeave = useCallback((event: DragEvent<HTMLDivElement>) => {
		event.preventDefault();
		dragCounter.current -= 1;
		if (dragCounter.current === 0) {
			setIsDragging(false);
		}
	}, []);

	const handleDragOver = useCallback((event: DragEvent<HTMLDivElement>) => {
		event.preventDefault();
	}, []);

	const handleDrop = useCallback(
		async (event: DragEvent<HTMLDivElement>) => {
			event.preventDefault();
			dragCounter.current = 0;
			setIsDragging(false);
			if (
				event.dataTransfer.items?.length &&
				(event.dataTransfer.files.length === 0 ||
					hasDirectoryInDropItems(event.dataTransfer.items))
			) {
				const files = await extractFilesFromDrop(event.dataTransfer.items);
				addFilesWithPath(files);
				return;
			}
			addFiles(event.dataTransfer.files);
		},
		[addFiles, addFilesWithPath],
	);

	const {
		activeCount,
		failedCount,
		overallProgress,
		successCount,
		totalCount,
	} = summarizeUploadTasks(tasks);
	const uploadTasks = buildUploadTaskViews({
		cancelTask,
		requestResumeFilePicker,
		retryTask,
		t,
		tasks,
	});

	return {
		activeCount,
		clearCompletedTasks,
		failedCount,
		hasUploadActivity,
		handleDragEnter,
		handleDragLeave,
		handleDragOver,
		handleDrop,
		handleFileInputChange,
		handleFolderInputChange,
		handleResumeFileChange,
		isDragging,
		overallProgress,
		retryFailedTasks,
		setUploadPanelOpen,
		setUploadAutoClearCompleted,
		setUploadConcurrency,
		successCount,
		totalCount,
		uploadAutoClearCompleted: uploadSettings.autoClearCompleted,
		uploadConcurrency: uploadSettings.concurrency,
		uploadPanelOpen,
		uploadTasks,
	};
}
