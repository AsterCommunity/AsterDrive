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

import { clearTerminalUploadTasks } from "./uploadAreaUploadTaskActions";

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

		await clearTerminalUploadTasks(
			tasks.map((item) => item.id),
			{ setTasks, tasksRef },
		);

		expect(cancelUpload).toHaveBeenCalledTimes(1);
		expect(cancelUpload).toHaveBeenCalledWith("failed-session");
		expect(removeSession.mock.calls.map(([uploadId]) => uploadId)).toEqual([
			"completed-session",
			"failed-session",
			"cancelled-session",
		]);
		expect(tasks.map((item) => item.id)).toEqual(["active"]);

		await clearTerminalUploadTasks(["active"], { setTasks, tasksRef });
		expect(setTasksMock).toHaveBeenCalledTimes(1);
		expect(cancelUpload).toHaveBeenCalledTimes(1);
	});
});
