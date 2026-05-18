import { describe, expect, it, vi } from "vitest";
import type { UploadTask } from "@/components/files/uploadAreaManagerShared";
import { buildUploadTaskViews } from "@/components/files/uploadAreaManagerView";

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
					status: "processing",
					progress: 95,
					speedBps: 2_048,
				}),
				createTask({
					id: "task-4",
					status: "completed",
					progress: 100,
					speedBps: 2_048,
				}),
				createTask({
					id: "task-5",
					status: "failed",
					progress: 40,
					speedBps: 2_048,
					error: "network failed",
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
		expect(views[2]?.speed).toBeUndefined();
		expect(views[3]?.speed).toBeUndefined();
		expect(views[4]).toMatchObject({
			detail: "network failed",
			speed: undefined,
		});
	});
});
