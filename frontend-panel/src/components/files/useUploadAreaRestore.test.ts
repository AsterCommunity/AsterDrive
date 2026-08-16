import { describe, expect, it } from "vitest";
import type { UploadTask } from "./uploadAreaManagerShared";
import { mergeRestoredUploadTasks } from "./useUploadAreaRestore";

function task(id: string): UploadTask {
	return {
		id,
		file: null,
		filename: `${id}.txt`,
		relativePath: null,
		baseFolderId: null,
		baseFolderName: "Root",
		totalBytes: 0,
		mode: null,
		status: "queued",
		progress: 0,
		uploadedBytes: 0,
		error: null,
		uploadId: null,
	};
}

describe("mergeRestoredUploadTasks", () => {
	it("does not duplicate a pending empty-file task already queued by this session", () => {
		const current = task("same-id");
		const restored = { ...task("same-id"), filename: "stale-name.txt" };

		expect(mergeRestoredUploadTasks([restored], [current])).toEqual([current]);
	});

	it("prepends persisted tasks that are not in the current queue", () => {
		const current = task("current");
		const restored = task("restored");

		expect(mergeRestoredUploadTasks([restored], [current])).toEqual([
			restored,
			current,
		]);
	});
});
