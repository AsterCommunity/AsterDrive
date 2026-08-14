import type { Dispatch, MutableRefObject, SetStateAction } from "react";
import { useCallback, useMemo, useRef } from "react";
import type { Workspace } from "@/lib/workspace";
import type {
	UploadAreaManagerTranslationFn,
	UploadTask,
} from "./uploadAreaManagerShared";
import { createUploadModeRunners } from "./uploadAreaUploadModeRunners";
import type { UploadRequestRef } from "./uploadAreaUploadRunnerShared";
import {
	cancelUploadTask,
	clearTerminalUploadTasks,
	retryUploadTask,
	runQueuedUploadTask,
} from "./uploadAreaUploadTaskActions";

interface UseUploadAreaUploadsOptions {
	abortFlagsRef: MutableRefObject<Map<string, boolean>>;
	directAbortRef: MutableRefObject<Map<string, AbortController>>;
	flushProgress: () => void;
	markFolderForRefresh: (task: UploadTask) => void;
	markTaskFailed: (taskId: string, error: unknown) => void;
	multipartInFlightRef: MutableRefObject<Map<string, number>>;
	patchTask: (taskId: string, patch: Partial<UploadTask>) => void;
	patchTaskThrottled: (taskId: string, patch: Partial<UploadTask>) => void;
	setTasks: Dispatch<SetStateAction<UploadTask[]>>;
	setUploadPanelOpen: Dispatch<SetStateAction<boolean>>;
	t: UploadAreaManagerTranslationFn;
	tasksRef: MutableRefObject<UploadTask[]>;
	uploadRequestRef: UploadRequestRef;
	workspace: Workspace;
}

export function useUploadAreaUploads({
	abortFlagsRef,
	directAbortRef,
	flushProgress,
	markFolderForRefresh,
	markTaskFailed,
	multipartInFlightRef,
	patchTask,
	patchTaskThrottled,
	setTasks,
	setUploadPanelOpen,
	t,
	tasksRef,
	uploadRequestRef,
	workspace,
}: UseUploadAreaUploadsOptions) {
	const taskOperationLocksRef = useRef(new Map<string, "clear" | "retry">());
	const modeRunners = useMemo(
		() =>
			createUploadModeRunners({
				abortFlagsRef,
				directAbortRef,
				flushProgress,
				markFolderForRefresh,
				markTaskFailed,
				multipartInFlightRef,
				patchTask,
				patchTaskThrottled,
				uploadRequestRef,
				t,
				workspace,
			}),
		[
			abortFlagsRef,
			directAbortRef,
			flushProgress,
			markFolderForRefresh,
			markTaskFailed,
			multipartInFlightRef,
			patchTask,
			patchTaskThrottled,
			uploadRequestRef,
			t,
			workspace,
		],
	);

	const runTask = useCallback(
		async (taskId: string) => {
			await runQueuedUploadTask(taskId, {
				...modeRunners,
				abortFlagsRef,
				directAbortRef,
				markFolderForRefresh,
				markTaskFailed,
				patchTask,
				setTasks,
				setUploadPanelOpen,
				taskOperationLocks: taskOperationLocksRef.current,
				t,
				tasksRef,
				uploadRequestRef,
				workspace,
			});
		},
		[
			modeRunners,
			abortFlagsRef,
			directAbortRef,
			markFolderForRefresh,
			markTaskFailed,
			patchTask,
			setTasks,
			setUploadPanelOpen,
			t,
			tasksRef,
			uploadRequestRef,
			workspace,
		],
	);

	const cancelTask = useCallback(
		async (taskId: string) => {
			await cancelUploadTask(taskId, {
				...modeRunners,
				abortFlagsRef,
				directAbortRef,
				markTaskFailed,
				patchTask,
				setTasks,
				setUploadPanelOpen,
				taskOperationLocks: taskOperationLocksRef.current,
				t,
				tasksRef,
				uploadRequestRef,
				workspace,
			});
		},
		[
			modeRunners,
			abortFlagsRef,
			directAbortRef,
			markTaskFailed,
			patchTask,
			setTasks,
			setUploadPanelOpen,
			t,
			tasksRef,
			uploadRequestRef,
			workspace,
		],
	);

	const clearTasks = useCallback(
		async (taskIds: readonly string[]) => {
			await clearTerminalUploadTasks(taskIds, {
				setTasks,
				taskOperationLocks: taskOperationLocksRef.current,
				tasksRef,
			});
		},
		[setTasks, tasksRef],
	);

	const retryTask = useCallback(
		async (taskId: string) => {
			await retryUploadTask(taskId, {
				...modeRunners,
				abortFlagsRef,
				directAbortRef,
				markTaskFailed,
				patchTask,
				setTasks,
				setUploadPanelOpen,
				t,
				taskOperationLocks: taskOperationLocksRef.current,
				tasksRef,
				uploadRequestRef,
				workspace,
			});
		},
		[
			modeRunners,
			abortFlagsRef,
			directAbortRef,
			markTaskFailed,
			patchTask,
			setTasks,
			setUploadPanelOpen,
			t,
			tasksRef,
			uploadRequestRef,
			workspace,
		],
	);

	return {
		cancelTask,
		clearTasks,
		resumeCompletionTask: modeRunners.resumeCompletionTask,
		retryTask,
		runTask,
	};
}
