import type { Dispatch, SetStateAction } from "react";
import { describe, expect, it, vi } from "vitest";
import type { UploadTask } from "./uploadAreaManagerShared";

const {
	cancelUpload,
	createEmptyFile,
	createFileService,
	initUpload,
	loadSessions,
	removePendingEmptyFile,
	removeSession,
} = vi.hoisted(() => ({
	cancelUpload: vi.fn(),
	createEmptyFile: vi.fn(),
	createFileService: vi.fn(),
	initUpload: vi.fn(),
	loadSessions: vi.fn(() => []),
	removePendingEmptyFile: vi.fn(),
	removeSession: vi.fn(),
}));

vi.mock("@/services/uploadService", () => ({
	uploadService: { cancelUpload, initUpload },
}));

vi.mock("@/services/fileService", () => ({
	createFileService: (workspace: unknown) => {
		createFileService(workspace);
		return { createEmptyFile };
	},
}));

vi.mock("@/lib/uploadPersistence", () => ({
	loadSessions,
	removePendingEmptyFile,
	removeSession,
	saveSession: vi.fn(),
}));

import {
	cancelUploadTask,
	clearTerminalUploadTasks,
	retryUploadTask,
	runQueuedUploadTask,
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

function createZeroByteTaskActionsFixture(taskId: string) {
	let tasks = [task(taskId, "queued", null)];
	tasks[0].file = new File([], `${taskId}.txt`);
	tasks[0].mode = null;
	const tasksRef = { current: tasks };
	const directAbortRef = { current: new Map<string, AbortController>() };
	const markFolderForRefresh = vi.fn();
	const markTaskFailed = vi.fn();
	const patchTask = vi.fn(
		(currentTaskId: string, patch: Partial<UploadTask>) => {
			tasks = tasks.map((item) =>
				item.id === currentTaskId ? { ...item, ...patch } : item,
			);
			tasksRef.current = tasks;
		},
	);
	const setTasks: Dispatch<SetStateAction<UploadTask[]>> = (update) => {
		tasks = typeof update === "function" ? update(tasks) : update;
		tasksRef.current = tasks;
	};
	const context = {
		abortFlagsRef: { current: new Map<string, boolean>() },
		cancelMultipartSession: vi.fn(),
		directAbortRef,
		markFolderForRefresh,
		markTaskFailed,
		patchTask,
		resumeCompletionTask: vi.fn(),
		runChunkedUpload: vi.fn(),
		runDirectUpload: vi.fn(),
		runMultipartUpload: vi.fn(),
		runPresignedUpload: vi.fn(),
		runProviderResumableUpload: vi.fn(),
		setTasks,
		setUploadPanelOpen: vi.fn(),
		taskOperationLocks: new Map(),
		t: vi.fn(),
		tasksRef,
		uploadRequestRef: { current: new Map() },
		workspace: { kind: "personal" },
	} as unknown as UploadTaskActionsContext & {
		markFolderForRefresh: (task: UploadTask) => void;
	};

	return {
		context,
		directAbortRef,
		markFolderForRefresh,
		markTaskFailed,
		tasksRef,
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
			task("failed-local", "failed", null),
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

	it("keeps tasks whose identity changes while cleanup is in flight", async () => {
		let finishCleanup: (() => void) | undefined;
		const cleanupPromise = new Promise<void>((resolve) => {
			finishCleanup = resolve;
		});
		cancelUpload.mockReset();
		cancelUpload.mockReturnValue(cleanupPromise);
		removeSession.mockReset();
		let tasks = [
			task("status-changed", "failed", "status-session"),
			task("upload-changed", "failed", "upload-session"),
			task("removed", "failed", "removed-session"),
			task("lock-changed", "failed", "lock-session"),
		];
		const tasksRef = { current: tasks };
		const taskOperationLocks = new Map();
		const setTasksMock = vi.fn((update: SetStateAction<UploadTask[]>) => {
			tasks = typeof update === "function" ? update(tasks) : update;
			tasksRef.current = tasks;
		});
		const setTasks: Dispatch<SetStateAction<UploadTask[]>> = setTasksMock;

		const clearPromise = clearTerminalUploadTasks(
			tasks.map((item) => item.id),
			{ setTasks, taskOperationLocks, tasksRef },
		);
		await vi.waitFor(() => expect(cancelUpload).toHaveBeenCalledTimes(4));

		tasks = tasks
			.filter((item) => item.id !== "removed")
			.map((item) => {
				if (item.id === "status-changed") {
					return { ...item, status: "completed" as const };
				}
				if (item.id === "upload-changed") {
					return { ...item, uploadId: "replacement-session" };
				}
				return item;
			});
		tasksRef.current = tasks;
		taskOperationLocks.set("lock-changed", "retry");
		finishCleanup?.();

		await clearPromise;

		expect(setTasksMock).not.toHaveBeenCalled();
		expect(removeSession).not.toHaveBeenCalled();
		expect(taskOperationLocks).toEqual(new Map([["lock-changed", "retry"]]));
		expect(tasks.map((item) => item.id)).toEqual([
			"status-changed",
			"upload-changed",
			"lock-changed",
		]);
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

	it("restarts direct sessions after best-effort server cleanup", async () => {
		const failedTask = task("direct", "failed", "direct-session");
		failedTask.file = new File(["retry"], "direct.bin");
		failedTask.mode = "direct";
		const tasksRef = { current: [failedTask] };
		const taskOperationLocks = new Map();
		const patchTask = vi.fn();
		const setUploadPanelOpen = vi.fn();
		cancelUpload.mockReset();
		cancelUpload.mockRejectedValue(new Error("cleanup unavailable"));
		removeSession.mockReset();

		await retryUploadTask("direct", {
			cancelMultipartSession: vi.fn(),
			patchTask,
			resumeCompletionTask: vi.fn(),
			setUploadPanelOpen,
			taskOperationLocks,
			tasksRef,
			workspace: { kind: "personal" },
		} as unknown as UploadTaskActionsContext);

		expect(cancelUpload).toHaveBeenCalledWith("direct-session");
		expect(removeSession).toHaveBeenCalledWith("direct-session");
		expect(patchTask).toHaveBeenCalledWith(
			"direct",
			expect.objectContaining({
				status: "queued",
				uploadId: null,
				retryable: undefined,
			}),
		);
		expect(setUploadPanelOpen).toHaveBeenCalledWith(true);
		expect(taskOperationLocks).toEqual(new Map());
	});

	it("releases retry locks for missing and explicitly terminal tasks", async () => {
		const terminalTask = task("terminal", "failed", "terminal-session");
		terminalTask.retryable = false;
		const tasksRef = { current: [terminalTask] };
		const taskOperationLocks = new Map();
		const patchTask = vi.fn();
		const context = {
			cancelMultipartSession: vi.fn(),
			patchTask,
			resumeCompletionTask: vi.fn(),
			setUploadPanelOpen: vi.fn(),
			taskOperationLocks,
			tasksRef,
			workspace: { kind: "personal" },
		} as unknown as UploadTaskActionsContext;

		await retryUploadTask("terminal", context);
		tasksRef.current = [];
		await retryUploadTask("missing", context);

		expect(patchTask).not.toHaveBeenCalled();
		expect(taskOperationLocks).toEqual(new Map());
	});

	it("preserves a successor lock after retrying multipart completion", async () => {
		const completionTask = task("completion", "failed", "completion-session");
		completionTask.mode = "presigned_multipart";
		const tasksRef = { current: [completionTask] };
		const taskOperationLocks = new Map();
		const completedParts = [{ part_number: 1, etag: "etag-1" }];
		loadSessions.mockReset();
		loadSessions.mockReturnValue([
			{
				uploadId: "completion-session",
				filename: "completion.bin",
				totalSize: 1,
				totalChunks: 1,
				chunkSize: 1,
				baseFolderId: null,
				baseFolderName: "Root",
				relativePath: null,
				savedAt: Date.now(),
				workspace: { kind: "personal" },
				mode: "presigned_multipart",
				completedParts,
			},
		]);
		const resumeCompletionTask = vi.fn(async () => {
			taskOperationLocks.set("completion", "clear");
		});

		await retryUploadTask("completion", {
			cancelMultipartSession: vi.fn(),
			patchTask: vi.fn(),
			resumeCompletionTask,
			setUploadPanelOpen: vi.fn(),
			taskOperationLocks,
			tasksRef,
			workspace: { kind: "personal" },
		} as unknown as UploadTaskActionsContext);

		expect(resumeCompletionTask).toHaveBeenCalledWith(
			completionTask,
			completedParts,
		);
		expect(taskOperationLocks).toEqual(new Map([["completion", "clear"]]));
	});
});

describe("runQueuedUploadTask", () => {
	it("creates zero-byte files without initializing upload data planes", async () => {
		createEmptyFile.mockReset();
		createEmptyFile.mockResolvedValue({ id: 42 });
		createFileService.mockReset();
		initUpload.mockReset();
		const queuedTask = task("empty", "queued", null);
		queuedTask.file = new File([], "empty.txt");
		queuedTask.relativePath = "docs/empty.txt";
		queuedTask.baseFolderId = 7;
		queuedTask.mode = null;
		const patchTask = vi.fn();
		const markFolderForRefresh = vi.fn();
		const runChunkedUpload = vi.fn();
		const runDirectUpload = vi.fn();
		const runMultipartUpload = vi.fn();
		const runPresignedUpload = vi.fn();
		const runProviderResumableUpload = vi.fn();
		const resumeCompletionTask = vi.fn();

		await runQueuedUploadTask("empty", {
			directAbortRef: { current: new Map() },
			markFolderForRefresh,
			markTaskFailed: vi.fn(),
			patchTask,
			runChunkedUpload,
			runDirectUpload,
			runMultipartUpload,
			runPresignedUpload,
			runProviderResumableUpload,
			resumeCompletionTask,
			tasksRef: { current: [queuedTask] },
			workspace: { kind: "personal" },
		} as unknown as UploadTaskActionsContext);

		expect(createEmptyFile).toHaveBeenCalledWith(
			"empty.txt",
			7,
			"docs/empty.txt",
			{
				signal: expect.any(AbortSignal),
				idempotencyKey: "empty-upload:empty",
			},
		);
		expect(createFileService).toHaveBeenCalledWith({ kind: "personal" });
		expect(initUpload).not.toHaveBeenCalled();
		expect(runChunkedUpload).not.toHaveBeenCalled();
		expect(runDirectUpload).not.toHaveBeenCalled();
		expect(runMultipartUpload).not.toHaveBeenCalled();
		expect(runPresignedUpload).not.toHaveBeenCalled();
		expect(runProviderResumableUpload).not.toHaveBeenCalled();
		expect(resumeCompletionTask).not.toHaveBeenCalled();
		expect(patchTask).toHaveBeenLastCalledWith(
			"empty",
			expect.objectContaining({ mode: "direct", status: "completed" }),
		);
		expect(markFolderForRefresh).toHaveBeenCalledWith(queuedTask);
		expect(removePendingEmptyFile).toHaveBeenCalledWith("empty");
	});

	it("replays a restored empty-file task without a browser File handle", async () => {
		createEmptyFile.mockReset();
		createEmptyFile.mockResolvedValue({ id: 43 });
		removePendingEmptyFile.mockReset();
		const restored = task("restored-empty", "queued", null);
		restored.file = null;
		restored.totalBytes = 0;
		restored.mode = null;
		restored.emptyFileIdempotencyKey = "persisted-key";

		await runQueuedUploadTask("restored-empty", {
			directAbortRef: { current: new Map() },
			markFolderForRefresh: vi.fn(),
			markTaskFailed: vi.fn(),
			patchTask: vi.fn(),
			runChunkedUpload: vi.fn(),
			runDirectUpload: vi.fn(),
			runMultipartUpload: vi.fn(),
			runPresignedUpload: vi.fn(),
			runProviderResumableUpload: vi.fn(),
			resumeCompletionTask: vi.fn(),
			tasksRef: { current: [restored] },
			workspace: { kind: "personal" },
		} as unknown as UploadTaskActionsContext);

		expect(createEmptyFile).toHaveBeenCalledWith(
			"restored-empty.bin",
			null,
			undefined,
			{
				signal: expect.any(AbortSignal),
				idempotencyKey: "persisted-key",
			},
		);
		expect(removePendingEmptyFile).toHaveBeenCalledWith("restored-empty");
	});

	it("keeps cancelled zero-byte tasks terminal when abort rejects the request", async () => {
		createEmptyFile.mockReset();
		createEmptyFile.mockImplementation(
			(
				_name: string,
				_folderId: number | null,
				_relativePath: string | undefined,
				options: { signal: AbortSignal },
			) =>
				new Promise((_resolve, reject) => {
					options.signal.addEventListener(
						"abort",
						() => reject(new DOMException("Aborted", "AbortError")),
						{ once: true },
					);
				}),
		);
		const fixture = createZeroByteTaskActionsFixture("cancelled-empty");

		const runPromise = runQueuedUploadTask("cancelled-empty", fixture.context);
		await vi.waitFor(() => expect(createEmptyFile).toHaveBeenCalledTimes(1));

		await cancelUploadTask("cancelled-empty", fixture.context);
		await runPromise;

		expect(fixture.tasksRef.current[0]?.status).toBe("cancelled");
		expect(fixture.markTaskFailed).not.toHaveBeenCalled();
		expect(fixture.markFolderForRefresh).not.toHaveBeenCalled();
		expect(fixture.directAbortRef.current).toEqual(new Map());
	});

	it("reports zero-byte creation failures and releases the abort controller", async () => {
		const error = new Error("empty file creation failed");
		createEmptyFile.mockReset();
		createEmptyFile.mockRejectedValue(error);
		const fixture = createZeroByteTaskActionsFixture("failed-empty");

		await runQueuedUploadTask("failed-empty", fixture.context);

		expect(fixture.markTaskFailed).toHaveBeenCalledWith("failed-empty", error);
		expect(fixture.markFolderForRefresh).not.toHaveBeenCalled();
		expect(fixture.directAbortRef.current).toEqual(new Map());
	});

	it("passes the original initialization error to task failure handling", async () => {
		const error = Object.assign(new Error("init failed"), { retryable: false });
		initUpload.mockReset();
		initUpload.mockRejectedValue(error);
		const queuedTask = task("queued", "queued", null);
		queuedTask.file = new File(["content"], "queued.bin");
		queuedTask.mode = null;
		const markTaskFailed = vi.fn();

		await runQueuedUploadTask("queued", {
			markTaskFailed,
			patchTask: vi.fn(),
			runChunkedUpload: vi.fn(),
			runDirectUpload: vi.fn(),
			runMultipartUpload: vi.fn(),
			runPresignedUpload: vi.fn(),
			runProviderResumableUpload: vi.fn(),
			resumeCompletionTask: vi.fn(),
			tasksRef: { current: [queuedTask] },
			workspace: { kind: "personal" },
		} as unknown as UploadTaskActionsContext);

		expect(markTaskFailed).toHaveBeenCalledWith("queued", error);
	});
});
