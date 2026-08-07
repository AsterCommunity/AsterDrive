import type { Dispatch, SetStateAction } from "react";
import { describe, expect, it, vi } from "vitest";
import type { UploadTask } from "./uploadAreaManagerShared";

const { cancelUpload, removeSession } = vi.hoisted(() => ({
	cancelUpload: vi.fn(),
	removeSession: vi.fn(),
}));

vi.mock("@/services/uploadService", () => ({
	uploadService: { cancelUpload },
}));

vi.mock("@/lib/uploadPersistence", () => ({
	loadSessions: vi.fn(() => []),
	removeSession,
	saveSession: vi.fn(),
}));

import {
	clearTerminalUploadTasks,
	retryUploadTask,
	type UploadTaskActionsContext,
} from "./uploadAreaUploadTaskActions";

function task(
	id: string,
	status: UploadTask["status"],
	uploadId: string | null,
): UploadTask {
	return {
		id,
		file: null,
		filename: `${id}.bin`,
		relativePath: null,
		baseFolderId: null,
		baseFolderName: "Root",
		totalBytes: 1,
		mode: uploadId ? "chunked" : "direct",
		status,
		progress: status === "completed" ? 100 : 0,
		error: status === "failed" ? "failed" : null,
		uploadId,
	};
}

describe("clearTerminalUploadTasks", () => {
	it("clears every terminal state, preserves active work, and tolerates cleanup errors", async () => {
		cancelUpload.mockReset();
		cancelUpload.mockRejectedValue(new Error("cleanup unavailable"));
		removeSession.mockReset();
		let tasks = [
			task("completed", "completed", "completed-session"),
			task("failed", "failed", "failed-session"),
			task("cancelled", "cancelled", "cancelled-session"),
			task("active", "uploading", "active-session"),
		];
		const tasksRef = { current: tasks };
		const setTasksMock = vi.fn((update: SetStateAction<UploadTask[]>) => {
			tasks = typeof update === "function" ? update(tasks) : update;
			tasksRef.current = tasks;
		});
		const setTasks: Dispatch<SetStateAction<UploadTask[]>> = setTasksMock;
		const taskOperationLocks = new Map();

		await clearTerminalUploadTasks(
			tasks.map((item) => item.id),
			{ setTasks, taskOperationLocks, tasksRef },
		);

		expect(cancelUpload).toHaveBeenCalledTimes(1);
		expect(cancelUpload).toHaveBeenCalledWith("failed-session");
		expect(removeSession.mock.calls.map(([uploadId]) => uploadId)).toEqual([
			"completed-session",
			"failed-session",
			"cancelled-session",
		]);
		expect(tasks.map((item) => item.id)).toEqual(["active"]);

		await clearTerminalUploadTasks(["active"], {
			setTasks,
			taskOperationLocks,
			tasksRef,
		});
		expect(setTasksMock).toHaveBeenCalledTimes(1);
		expect(cancelUpload).toHaveBeenCalledTimes(1);
	});

	it("blocks retry while delayed cleanup owns the task lock", async () => {
		let finishCleanup: (() => void) | undefined;
		cancelUpload.mockReset();
		cancelUpload.mockImplementation(
			() =>
				new Promise<void>((resolve) => {
					finishCleanup = resolve;
				}),
		);
		removeSession.mockReset();
		const failedTask = task("failed", "failed", "failed-session");
		failedTask.file = new File(["retry"], "failed.bin");
		let tasks = [failedTask];
		const tasksRef = { current: tasks };
		const taskOperationLocks = new Map();
		const setTasks: Dispatch<SetStateAction<UploadTask[]>> = (update) => {
			tasks = typeof update === "function" ? update(tasks) : update;
			tasksRef.current = tasks;
		};
		const patchTask = vi.fn();

		const clearPromise = clearTerminalUploadTasks(["failed"], {
			setTasks,
			taskOperationLocks,
			tasksRef,
		});
		await vi.waitFor(() => expect(cancelUpload).toHaveBeenCalledTimes(1));

		await retryUploadTask("failed", {
			cancelMultipartSession: vi.fn(),
			patchTask,
			resumeCompletionTask: vi.fn(),
			setUploadPanelOpen: vi.fn(),
			taskOperationLocks,
			tasksRef,
			workspace: { kind: "personal" },
		} as unknown as UploadTaskActionsContext);

		expect(patchTask).not.toHaveBeenCalled();
		expect(tasks.map((item) => item.id)).toEqual(["failed"]);
		finishCleanup?.();
		await clearPromise;
		expect(tasks).toEqual([]);
	});

	it("skips clearing a task while retry cancellation is in flight", async () => {
		cancelUpload.mockReset();
		removeSession.mockReset();
		let finishRetryCleanup: (() => void) | undefined;
		const cancelMultipartSession = vi.fn(
			() =>
				new Promise<void>((resolve) => {
					finishRetryCleanup = resolve;
				}),
		);
		const failedTask = task("failed", "failed", "failed-session");
		failedTask.file = new File(["retry"], "failed.bin");
		let tasks = [failedTask];
		const tasksRef = { current: tasks };
		const taskOperationLocks = new Map();
		const setTasksMock = vi.fn((update: SetStateAction<UploadTask[]>) => {
			tasks = typeof update === "function" ? update(tasks) : update;
			tasksRef.current = tasks;
		});
		const setTasks: Dispatch<SetStateAction<UploadTask[]>> = setTasksMock;
		const patchTask = vi.fn((taskId: string, patch: Partial<UploadTask>) => {
			tasks = tasks.map((item) =>
				item.id === taskId ? { ...item, ...patch } : item,
			);
			tasksRef.current = tasks;
		});

		const retryPromise = retryUploadTask("failed", {
			cancelMultipartSession,
			patchTask,
			resumeCompletionTask: vi.fn(),
			setUploadPanelOpen: vi.fn(),
			taskOperationLocks,
			tasksRef,
			workspace: { kind: "personal" },
		} as unknown as UploadTaskActionsContext);
		await vi.waitFor(() =>
			expect(cancelMultipartSession).toHaveBeenCalledTimes(1),
		);

		await clearTerminalUploadTasks(["failed"], {
			setTasks,
			taskOperationLocks,
			tasksRef,
		});
		expect(setTasksMock).not.toHaveBeenCalled();

		finishRetryCleanup?.();
		await retryPromise;
		expect(tasks).toHaveLength(1);
		expect(tasks[0]?.status).toBe("queued");
		expect(removeSession).not.toHaveBeenCalled();
	});
});
