import { beforeEach, describe, expect, it } from "vitest";
import {
	DOWNLOAD_TASK_STATUS,
	type DownloadTask,
	requestDownloadSelection,
	useDownloadStore,
} from "@/stores/downloadStore";

function task(id: string, status = DOWNLOAD_TASK_STATUS.queued): DownloadTask {
	return {
		id,
		kind: "file",
		name: `${id}.txt`,
		status,
		createdAt: 1,
		bytesReceived: 0,
		totalBytes: null,
		speedBps: null,
		completedItems: 0,
		failedItems: 0,
		totalItems: 1,
		items: [],
	};
}

describe("downloadStore", () => {
	beforeEach(() => {
		useDownloadStore.setState({ pendingSelection: null, tasks: [] });
	});

	it("requests and dismisses a download selection", () => {
		const selection = {
			workspace: { kind: "team" as const, teamId: 9 },
			files: [{ id: 1, name: "report.txt", size: 4 }],
			folders: [{ id: 2, name: "docs" }],
		};

		requestDownloadSelection(selection);
		expect(useDownloadStore.getState().pendingSelection).toEqual(selection);

		useDownloadStore.getState().dismissSelection();
		expect(useDownloadStore.getState().pendingSelection).toBeNull();
	});

	it("upserts tasks without duplicates and moves the latest value first", () => {
		const store = useDownloadStore.getState();
		store.upsertTask(task("first"));
		store.upsertTask(task("second"));
		store.upsertTask({ ...task("first"), name: "renamed.txt" });

		expect(useDownloadStore.getState().tasks.map(({ id }) => id)).toEqual([
			"first",
			"second",
		]);
		expect(useDownloadStore.getState().tasks[0]?.name).toBe("renamed.txt");
	});

	it("updates only the requested task and preserves unrelated tasks", () => {
		const first = task("first");
		const second = task("second");
		useDownloadStore.setState({ tasks: [first, second] });

		useDownloadStore.getState().updateTask("second", {
			status: DOWNLOAD_TASK_STATUS.downloading,
			bytesReceived: 7,
		});

		expect(useDownloadStore.getState().tasks).toEqual([
			first,
			{
				...second,
				status: DOWNLOAD_TASK_STATUS.downloading,
				bytesReceived: 7,
			},
		]);
	});

	it("ignores updates for unknown task ids without corrupting entries", () => {
		const existing = task("existing");
		useDownloadStore.setState({ tasks: [existing] });

		useDownloadStore.getState().updateTask("missing", {
			status: DOWNLOAD_TASK_STATUS.failed,
		});

		expect(useDownloadStore.getState().tasks).toEqual([existing]);
	});

	it("removes completed and canceled tasks while preserving retryable and active work", () => {
		useDownloadStore.setState({
			tasks: [
				task("queued", DOWNLOAD_TASK_STATUS.queued),
				task("preparing", DOWNLOAD_TASK_STATUS.preparing),
				task("downloading", DOWNLOAD_TASK_STATUS.downloading),
				task("failed", DOWNLOAD_TASK_STATUS.failed),
				task("completed", DOWNLOAD_TASK_STATUS.completed),
				task("canceled", DOWNLOAD_TASK_STATUS.canceled),
			],
		});

		useDownloadStore.getState().removeCompleted();

		expect(useDownloadStore.getState().tasks.map(({ id }) => id)).toEqual([
			"queued",
			"preparing",
			"downloading",
			"failed",
		]);
	});
});
