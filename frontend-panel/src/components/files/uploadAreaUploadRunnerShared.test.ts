import { describe, expect, it, vi } from "vitest";
import {
	abortAllUploadRequests,
	abortUploadRequests,
	registerUploadRequest,
	type UploadRequestRef,
	unregisterUploadRequest,
	withTrackedUploadRequest,
} from "./uploadAreaUploadRunnerShared";

function createRequestRef(): UploadRequestRef {
	return { current: new Map() };
}

function createXhr() {
	return {
		abort: vi.fn(),
	} as unknown as XMLHttpRequest;
}

describe("uploadAreaUploadRunnerShared request tracking", () => {
	it("tracks multiple requests for one task and unregisters them individually", () => {
		const requestRef = createRequestRef();
		const first = createXhr();
		const second = createXhr();

		registerUploadRequest(requestRef, "task-1", first);
		registerUploadRequest(requestRef, "task-1", second);

		expect(requestRef.current.get("task-1")).toEqual(new Set([first, second]));

		unregisterUploadRequest(requestRef, "missing-task", first);
		unregisterUploadRequest(requestRef, "task-1", first);

		expect(requestRef.current.get("task-1")).toEqual(new Set([second]));

		unregisterUploadRequest(requestRef, "task-1", second);

		expect(requestRef.current.has("task-1")).toBe(false);
	});

	it("aborts tracked requests for one task and leaves other tasks intact", () => {
		const requestRef = createRequestRef();
		const first = createXhr();
		const second = createXhr();
		const other = createXhr();

		registerUploadRequest(requestRef, "task-1", first);
		registerUploadRequest(requestRef, "task-1", second);
		registerUploadRequest(requestRef, "task-2", other);
		abortUploadRequests(requestRef, "task-1");
		abortUploadRequests(requestRef, "missing-task");

		expect(first.abort).toHaveBeenCalledTimes(1);
		expect(second.abort).toHaveBeenCalledTimes(1);
		expect(other.abort).not.toHaveBeenCalled();
		expect(requestRef.current.has("task-1")).toBe(false);
		expect(requestRef.current.get("task-2")).toEqual(new Set([other]));
	});

	it("aborts every tracked request and clears the registry", () => {
		const requestRef = createRequestRef();
		const first = createXhr();
		const second = createXhr();

		registerUploadRequest(requestRef, "task-1", first);
		registerUploadRequest(requestRef, "task-2", second);
		abortAllUploadRequests(requestRef);

		expect(first.abort).toHaveBeenCalledTimes(1);
		expect(second.abort).toHaveBeenCalledTimes(1);
		expect(requestRef.current.size).toBe(0);
	});

	it("registers a created xhr while work is running and unregisters it afterward", async () => {
		const requestRef = createRequestRef();
		const xhr = createXhr();

		await expect(
			withTrackedUploadRequest(requestRef, "task-1", async (onCreateXhr) => {
				onCreateXhr(xhr);
				expect(requestRef.current.get("task-1")).toEqual(new Set([xhr]));
				return "done";
			}),
		).resolves.toBe("done");

		expect(requestRef.current.has("task-1")).toBe(false);
	});

	it("still unregisters a created xhr when the wrapped work fails", async () => {
		const requestRef = createRequestRef();
		const xhr = createXhr();

		await expect(
			withTrackedUploadRequest(requestRef, "task-1", async (onCreateXhr) => {
				onCreateXhr(xhr);
				throw new Error("upload failed");
			}),
		).rejects.toThrow("upload failed");

		expect(requestRef.current.has("task-1")).toBe(false);
	});
});
