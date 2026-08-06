import { describe, expect, it, vi } from "vitest";
import type { UploadTask } from "@/components/files/uploadAreaManagerShared";
import {
	buildUploadTaskViews,
	summarizeUploadTasks,
} from "@/components/files/uploadAreaManagerView";

const t = (key: string, opts?: Record<string, unknown>) => {
	if (key === "files:upload_chunk_status") {
		return `Chunk ${opts?.current}/${opts?.total}`;
	}
	return key;
};

function createTask(overrides: Partial<UploadTask>): UploadTask {
	return {
		id: "task-1",
		file: null,
		filename: "example.bin",
		relativePath: null,
		baseFolderId: 1,
		baseFolderName: "Projects",
		totalBytes: 100,
		mode: "direct",
		status: "queued",
		progress: 0,
		error: null,
		uploadId: null,
		...overrides,
	};
}

describe("buildUploadTaskViews", () => {
	it("formats upload speed only while uploading", () => {
		const views = buildUploadTaskViews({
			cancelTask: vi.fn(),
			clearTask: vi.fn(),
			requestResumeFilePicker: vi.fn(),
			retryTask: vi.fn(),
			t,
			tasks: [
				createTask({
					status: "uploading",
					progress: 40,
					speedBps: 2_048,
				}),
				createTask({
					id: "task-2",
					mode: "chunked",
					status: "uploading",
					progress: 40,
					speedBps: 1_536,
					completedChunks: 2,
					totalChunks: 4,
				}),
				createTask({
					id: "task-3",
					status: "uploading",
					progress: 40,
					speedBps: 0,
				}),
				createTask({
					id: "task-4",
					status: "processing",
					progress: 95,
					speedBps: 2_048,
				}),
				createTask({
					id: "task-5",
					status: "completed",
					progress: 100,
					speedBps: 2_048,
				}),
				createTask({
					id: "task-6",
					status: "failed",
					progress: 40,
					speedBps: 2_048,
					error: "network failed",
				}),
				createTask({
					id: "task-7",
					status: "cancelled",
					progress: 40,
					speedBps: 2_048,
				}),
			],
		});

		expect(views[0]).toMatchObject({
			speed: "2.0 KB/s",
			detail: "files:uploading_to_storage",
		});
		expect(views[1]).toMatchObject({
			speed: "1.5 KB/s",
			detail: "Chunk 2/4",
		});
		expect(views[2]?.speed).toBe("0 B/s");
		expect(views[3]?.speed).toBeUndefined();
		expect(views[4]?.speed).toBeUndefined();
		expect(views[5]).toMatchObject({
			detail: "network failed",
			speed: undefined,
		});
		expect(views[6]).toMatchObject({
			cancelled: true,
			detail: "files:upload_cancelled",
			speed: undefined,
		});
	});

	it("only exposes retry for failed tasks that are not explicitly non-retryable", () => {
		const clearTask = vi.fn();
		const retryTask = vi.fn();
		const views = buildUploadTaskViews({
			cancelTask: vi.fn(),
			clearTask,
			requestResumeFilePicker: vi.fn(),
			retryTask,
			t,
			tasks: [
				createTask({
					id: "retryable",
					status: "failed",
					retryable: true,
				}),
				createTask({
					id: "terminal",
					status: "failed",
					retryable: false,
				}),
				createTask({ id: "completed", status: "completed" }),
				createTask({ id: "cancelled", status: "cancelled" }),
				createTask({ id: "active", status: "uploading" }),
			],
		});

		expect(views[0]).toMatchObject({ failed: true, retryable: true });
		expect(views[0]?.actions?.map((action) => action.icon)).toEqual([
			"ArrowsClockwise",
			"Trash",
		]);
		expect(views[1]).toMatchObject({ failed: true, retryable: false });
		expect(views[1]?.actions?.map((action) => action.icon)).toEqual(["Trash"]);
		expect(views[2]?.actions?.map((action) => action.icon)).toEqual(["Trash"]);
		expect(views[3]?.actions?.map((action) => action.icon)).toEqual(["Trash"]);
		expect(views[4]?.actions?.map((action) => action.icon)).toEqual(["X"]);

		views[0]?.actions?.[0]?.onClick();
		views[1]?.actions?.[0]?.onClick();
		expect(retryTask).toHaveBeenCalledWith("retryable");
		expect(clearTask).toHaveBeenCalledWith("terminal");
	});
});

describe("summarizeUploadTasks", () => {
	it("excludes failed and cancelled tasks from the overall progress denominator", () => {
		const summary = summarizeUploadTasks([
			createTask({
				id: "done",
				status: "completed",
				progress: 100,
				totalBytes: 100,
			}),
			createTask({
				id: "active",
				status: "uploading",
				progress: 50,
				totalBytes: 100,
			}),
			createTask({
				id: "failed",
				status: "failed",
				progress: 0,
				totalBytes: 100,
			}),
			createTask({
				id: "cancelled",
				status: "cancelled",
				progress: 25,
				totalBytes: 100,
			}),
		]);

		expect(summary).toEqual({
			activeCount: 1,
			failedCount: 1,
			overallProgress: 75,
			successCount: 1,
			totalCount: 4,
		});
	});

	it("weights the overall progress by task size", () => {
		const summary = summarizeUploadTasks([
			createTask({
				id: "small",
				status: "completed",
				progress: 100,
				totalBytes: 10,
			}),
			createTask({
				id: "large",
				status: "uploading",
				progress: 0,
				totalBytes: 1000,
			}),
		]);

		expect(summary).toEqual({
			activeCount: 1,
			failedCount: 0,
			overallProgress: 1,
			successCount: 1,
			totalCount: 2,
		});
	});

	it("returns zero overall progress when no task can contribute to progress", () => {
		const summary = summarizeUploadTasks([
			createTask({ id: "failed", status: "failed", progress: 40 }),
			createTask({ id: "cancelled", status: "cancelled", progress: 20 }),
			createTask({ id: "pending", status: "pending_file", progress: 0 }),
		]);

		expect(summary.overallProgress).toBe(0);
		expect(summary.activeCount).toBe(0);
		expect(summary.failedCount).toBe(1);
	});
});
