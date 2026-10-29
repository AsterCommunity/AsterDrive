import { beforeEach, describe, expect, it, vi } from "vitest";
import type { InitUploadResponse } from "@/services/uploadService";
import type { UploadTask } from "./uploadAreaManagerShared";
import type { UploadTransportRunnerContext } from "./uploadAreaUploadRunnerShared";

const {
	completeUpload,
	getProgress,
	removeSession,
	executeBrowserUpload,
	coordinateBrowserUpload,
} = vi.hoisted(() => ({
	completeUpload: vi.fn(),
	getProgress: vi.fn(),
	removeSession: vi.fn(),
	executeBrowserUpload: vi.fn(),
	coordinateBrowserUpload: vi.fn(),
}));

vi.mock("@/services/uploadService", async (importOriginal) => ({
	...(await importOriginal<typeof import("@/services/uploadService")>()),
	uploadService: {
		cancelUpload: vi.fn(),
		completeUpload,
		getProgress,
		presignParts: vi.fn(),
		presignedUpload: vi.fn(),
		uploadChunk: vi.fn(),
		executeBrowserUpload,
		coordinateBrowserUpload,
	},
}));

vi.mock("@/lib/uploadPersistence", () => ({
	appendCompletedPart: vi.fn(),
	removeSession,
}));

import {
	createResumableUploadRunners,
	resolveChunkConcurrency,
} from "./uploadAreaResumableUploadRunners";

function initWithMax(
	max?: number,
	chunkOrdering: "unordered" | "sequential" = "unordered",
): InitUploadResponse {
	return {
		mode: "chunked",
		upload_id: "upload-1",
		chunk_size: 1024,
		total_chunks: 2,
		upload_scheduling:
			max === undefined
				? undefined
				: {
						chunk_ordering: chunkOrdering,
						max_chunk_concurrency: max,
					},
	};
}

describe("resolveChunkConcurrency", () => {
	it("clamps the client worker count to the backend maximum", () => {
		expect(resolveChunkConcurrency(initWithMax(1), 3)).toBe(1);
		expect(resolveChunkConcurrency(initWithMax(2), 3)).toBe(2);
		expect(resolveChunkConcurrency(initWithMax(16), 3)).toBe(3);
	});

	it("honors sequential ordering even if the maximum is larger", () => {
		expect(resolveChunkConcurrency(initWithMax(4, "sequential"), 3)).toBe(1);
	});

	it("keeps the compatibility fallback when scheduling metadata is absent", () => {
		expect(resolveChunkConcurrency(initWithMax(), 3)).toBe(3);
		expect(resolveChunkConcurrency(initWithMax(0), 3)).toBe(3);
	});

	it("normalizes invalid and fractional compatibility fallbacks", () => {
		for (const fallback of [0, -1, Number.NaN, Number.POSITIVE_INFINITY]) {
			expect(resolveChunkConcurrency(initWithMax(), fallback)).toBe(1);
		}
		expect(resolveChunkConcurrency(initWithMax(), 3.9)).toBe(3);
	});

	it("ignores invalid backend maxima and floors fractional maxima", () => {
		for (const backendMax of [-1, 0, Number.NaN, Number.POSITIVE_INFINITY]) {
			expect(resolveChunkConcurrency(initWithMax(backendMax), 4)).toBe(4);
		}
		expect(resolveChunkConcurrency(initWithMax(2.9), 4)).toBe(2);
		expect(resolveChunkConcurrency(initWithMax(2), 1)).toBe(1);
	});

	it("keeps sequential scheduling at one for invalid limits and fallbacks", () => {
		expect(
			resolveChunkConcurrency(
				initWithMax(Number.NaN, "sequential"),
				Number.POSITIVE_INFINITY,
			),
		).toBe(1);
	});
});

describe("resumeCompletionTask", () => {
	it("publishes an already completed stream session", async () => {
		getProgress.mockReset();
		getProgress.mockResolvedValue({ status: "completed" });
		completeUpload.mockReset();
		removeSession.mockReset();
		const markFolderForRefresh = vi.fn();
		const patchTask = vi.fn();
		const context: UploadTransportRunnerContext = {
			abortFlagsRef: { current: new Map() },
			metadataAbortRef: { current: new Map() },
			flushProgress: vi.fn(),
			markFolderForRefresh,
			markTaskFailed: vi.fn(),
			multipartInFlightRef: { current: new Map() },
			patchTask,
			patchTaskThrottled: vi.fn(),
			uploadRequestRef: { current: new Map() },
			t: (key) => key,
			workspace: { kind: "personal" },
		};
		const task: UploadTask = {
			id: "stream-completed",
			file: new File(["body"], "stream.bin"),
			filename: "stream.bin",
			relativePath: null,
			baseFolderId: null,
			baseFolderName: "Root",
			totalBytes: 4,
			mode: "stream",
			status: "processing",
			progress: 95,
			error: null,
			uploadId: "stream-session",
		};

		const { resumeCompletionTask } = createResumableUploadRunners(context);
		await resumeCompletionTask(task);

		expect(getProgress).toHaveBeenCalledWith("stream-session");
		expect(removeSession).toHaveBeenCalledWith("stream-session");
		expect(patchTask).toHaveBeenLastCalledWith(
			"stream-completed",
			expect.objectContaining({ status: "completed", uploadedBytes: 4 }),
		);
		expect(markFolderForRefresh).toHaveBeenCalledWith(task);
	});

	it("reports a terminal stream status for a file-backed task", async () => {
		getProgress.mockReset();
		getProgress.mockResolvedValue({ status: "uploading" });
		const markTaskFailed = vi.fn();
		const context: UploadTransportRunnerContext = {
			abortFlagsRef: { current: new Map() },
			metadataAbortRef: { current: new Map() },
			flushProgress: vi.fn(),
			markFolderForRefresh: vi.fn(),
			markTaskFailed,
			multipartInFlightRef: { current: new Map() },
			patchTask: vi.fn(),
			patchTaskThrottled: vi.fn(),
			uploadRequestRef: { current: new Map() },
			t: (key) => key,
			workspace: { kind: "personal" },
		};
		const task: UploadTask = {
			id: "stream-uploading",
			file: new File(["body"], "stream.bin"),
			filename: "stream.bin",
			relativePath: null,
			baseFolderId: null,
			baseFolderName: "Root",
			totalBytes: 4,
			mode: "stream",
			status: "processing",
			progress: 95,
			error: null,
			uploadId: "stream-session",
		};

		const { resumeCompletionTask } = createResumableUploadRunners(context);
		await resumeCompletionTask(task);

		expect(markTaskFailed).toHaveBeenCalledWith(
			"stream-uploading",
			expect.objectContaining({
				message: "stream publication ended in uploading",
			}),
		);
	});

	it("passes the original completion error to file-backed task failure handling", async () => {
		const error = Object.assign(new Error("complete failed"), {
			retryable: false,
		});
		completeUpload.mockReset();
		completeUpload.mockRejectedValue(error);
		removeSession.mockReset();
		const markTaskFailed = vi.fn();
		const patchTask = vi.fn();
		const abortFlagsRef = { current: new Map<string, boolean>() };
		const context: UploadTransportRunnerContext = {
			abortFlagsRef,
			metadataAbortRef: { current: new Map() },
			flushProgress: vi.fn(),
			markFolderForRefresh: vi.fn(),
			markTaskFailed,
			multipartInFlightRef: { current: new Map() },
			patchTask,
			patchTaskThrottled: vi.fn(),
			uploadRequestRef: { current: new Map() },
			t: (key) => key,
			workspace: { kind: "personal" },
		};
		const task: UploadTask = {
			id: "file-backed",
			file: new File(["content"], "file-backed.bin"),
			filename: "file-backed.bin",
			relativePath: null,
			baseFolderId: null,
			baseFolderName: "Root",
			totalBytes: 7,
			mode: "chunked",
			status: "processing",
			progress: 95,
			error: null,
			uploadId: "upload-file-backed",
		};

		const { resumeCompletionTask } = createResumableUploadRunners(context);
		await resumeCompletionTask(task);

		expect(markTaskFailed).toHaveBeenCalledWith("file-backed", error);
		expect(removeSession).not.toHaveBeenCalled();
		expect(abortFlagsRef.current.has("file-backed")).toBe(false);
	});
});

describe("backend coordinated provider upload", () => {
	function step(offset: number, completed = false) {
		return {
			executor_version: 1,
			protocol: { id: "third-party.plugin", version: 7 },
			confirmed_offset: offset,
			progress_revision: offset,
			provider_completed: completed,
			instruction: completed
				? null
				: {
						request_id: `request-${offset}`,
						timeout_ms: 60000,
						response_body_limit: 65536,
						request: {
							url: "https://plugin.example/upload",
							method: "PATCH" as const,
							headers: {},
							body: { start: offset, end: Math.min(offset + 4, 8) },
							response_headers: [],
						},
					},
		};
	}
	beforeEach(() => {
		vi.clearAllMocks();
		executeBrowserUpload.mockReset();
		coordinateBrowserUpload.mockReset();
		completeUpload.mockReset();
		completeUpload.mockResolvedValue({});
		executeBrowserUpload.mockResolvedValue({
			status: 299,
			headers: {},
			body: "opaque",
			body_truncated: false,
		});
	});
	function setup() {
		const context: UploadTransportRunnerContext = {
			abortFlagsRef: { current: new Map() },
			metadataAbortRef: { current: new Map() },
			flushProgress: vi.fn(),
			markFolderForRefresh: vi.fn(),
			markTaskFailed: vi.fn(),
			multipartInFlightRef: { current: new Map() },
			patchTask: vi.fn(),
			patchTaskThrottled: vi.fn(),
			uploadRequestRef: { current: new Map() },
			t: (key) => key,
			workspace: { kind: "personal" },
		};
		const task: UploadTask = {
			id: "provider-task",
			file: new File(["abcdefgh"], "file.bin"),
			filename: "file.bin",
			relativePath: null,
			baseFolderId: null,
			baseFolderName: "Root",
			totalBytes: 8,
			mode: "provider_resumable",
			status: "uploading",
			progress: 0,
			error: null,
			uploadId: "provider-session",
		};
		const init: InitUploadResponse = {
			mode: "provider_resumable",
			upload_id: "provider-session",
			chunk_size: 4,
			total_chunks: 2,
			provider_resumable: {
				browser_upload: step(0),
			},
			upload_scheduling: {
				chunk_ordering: "sequential",
				max_chunk_concurrency: 1,
			},
		};
		return {
			context,
			task,
			init,
			run: (alreadyReceived: number[] = []) =>
				createResumableUploadRunners(context).runProviderResumableUpload(
					task,
					init,
					alreadyReceived,
				),
		};
	}

	it("runs an opaque third-party protocol and only publishes after backend completion", async () => {
		const fixture = setup();
		coordinateBrowserUpload
			.mockResolvedValueOnce(step(2))
			.mockResolvedValueOnce(step(6))
			.mockResolvedValueOnce(step(8, true));
		await fixture.run();
		expect(
			executeBrowserUpload.mock.calls.map((call) => call[0].request.body.start),
		).toEqual([0, 2, 6]);
		expect(completeUpload).toHaveBeenCalledOnce();
		expect(removeSession).toHaveBeenCalledOnce();
		expect(fixture.context.markTaskFailed).not.toHaveBeenCalled();
	});
	it("restores from a partial offset without reading provider wire ranges", async () => {
		const fixture = setup();
		if (fixture.init.provider_resumable)
			fixture.init.provider_resumable.browser_upload = step(2);
		coordinateBrowserUpload
			.mockResolvedValueOnce(step(6))
			.mockResolvedValueOnce(step(8, true));
		await fixture.run();
		expect(executeBrowserUpload.mock.calls[0][0].request.body.start).toBe(2);
		expect(completeUpload).toHaveBeenCalledOnce();
	});
	it("preserves final object verification failures", async () => {
		const fixture = setup();
		coordinateBrowserUpload.mockResolvedValueOnce(step(8, true));
		completeUpload.mockRejectedValue(new Error("size mismatch"));
		await fixture.run();
		expect(fixture.context.markTaskFailed).toHaveBeenCalledOnce();
		expect(removeSession).not.toHaveBeenCalled();
	});
	it("does not finalize when cancellation races with a coordination response", async () => {
		const fixture = setup();
		coordinateBrowserUpload.mockImplementation(async () => {
			fixture.context.abortFlagsRef.current.set(fixture.task.id, true);
			return step(8, true);
		});
		await fixture.run();
		expect(completeUpload).not.toHaveBeenCalled();
		expect(removeSession).not.toHaveBeenCalled();
		expect(fixture.context.markTaskFailed).not.toHaveBeenCalled();
	});
	it.each([0, -1, 0.5, Number.NaN])(
		"rejects invalid chunk size %s",
		async (size) => {
			const fixture = setup();
			fixture.init.chunk_size = size;
			await expect(fixture.run()).rejects.toThrow(
				"invalid provider resumable upload configuration",
			);
			expect(executeBrowserUpload).not.toHaveBeenCalled();
		},
	);
});
