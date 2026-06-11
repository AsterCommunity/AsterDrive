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
	retryUploadTask,
	runQueuedUploadTask,
} from "./uploadAreaUploadTaskActions";

interface UseUploadAreaUploadsOptions {
	abortFlagsRef: MutableRefObject<Map<string, boolean>>;
	directAbortRef: MutableRefObject<Map<string, AbortController>>;
	flushProgress: () => void;
	markFolderForRefresh: (task: UploadTask) => void;
	markTaskFailed: (taskId: string, message: string) => void;
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
	const retryingTaskIdsRef = useRef(new Set<string>());
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
				markTaskFailed,
				patchTask,
				setTasks,
				setUploadPanelOpen,
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

	const retryTask = useCallback(
		async (taskId: string) => {
			if (retryingTaskIdsRef.current.has(taskId)) return;
			retryingTaskIdsRef.current.add(taskId);
			try {
				await retryUploadTask(taskId, {
					...modeRunners,
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
				});
			} finally {
				retryingTaskIdsRef.current.delete(taskId);
			}
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
		resumeCompletionTask: modeRunners.resumeCompletionTask,
		retryTask,
		runTask,
	};
}
