import { beforeEach, describe, expect, it, vi } from "vitest";
import type { InitUploadResponse } from "@/services/uploadService";
import type { UploadTask } from "./uploadAreaManagerShared";
import type { UploadTransportRunnerContext } from "./uploadAreaUploadRunnerShared";

const { completeUpload, getProgress, removeSession, providerResumableUpload } =
	vi.hoisted(() => ({
		completeUpload: vi.fn(),
		getProgress: vi.fn(),
		removeSession: vi.fn(),
		providerResumableUpload: vi.fn(),
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
		providerResumableUpload,
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

describe("provider resumable byte reconciliation", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		providerResumableUpload.mockReset();
		getProgress.mockReset();
		completeUpload.mockReset();
		completeUpload.mockResolvedValue({});
	});
	function setup(ranges: string[] = ["0-"]) {
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
				upload_url: "https://provider.example/session",
				next_expected_ranges: ranges,
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
	it.each([0, -1, 0.5, Number.NaN])(
		"rejects invalid chunk size %s before uploading",
		async (size) => {
			const fixture = setup();
			fixture.init.chunk_size = size;
			await expect(fixture.run()).rejects.toThrow(
				"invalid provider resumable upload configuration",
			);
			expect(providerResumableUpload).not.toHaveBeenCalled();
			expect(completeUpload).not.toHaveBeenCalled();
		},
	);
	it("requires an upload URL and leaves a task without a file untouched", async () => {
		const fixture = setup();
		fixture.init.provider_resumable = undefined;
		await expect(fixture.run()).rejects.toThrow(
			"invalid provider resumable upload configuration",
		);
		fixture.task.file = null;
		await fixture.run();
		expect(providerResumableUpload).not.toHaveBeenCalled();
		expect(completeUpload).not.toHaveBeenCalled();
	});
	it("uses persisted contiguous chunks when byte ranges are absent", async () => {
		const fixture = setup();
		if (fixture.init.provider_resumable)
			fixture.init.provider_resumable.next_expected_ranges = undefined;
		providerResumableUpload.mockImplementation(
			async (_url, _blob, _start, _total, options) => {
				options.onProgress(2, 4);
				return 8;
			},
		);
		await fixture.run([0]);
		expect(providerResumableUpload.mock.calls.map((call) => call[2])).toEqual([
			4,
		]);
		expect(fixture.context.patchTaskThrottled).toHaveBeenCalledWith(
			fixture.task.id,
			expect.objectContaining({ uploadedBytes: 6 }),
		);
		expect(completeUpload).toHaveBeenCalledOnce();
	});
	it("bounds partial acknowledgements even when every response advances", async () => {
		const fixture = setup();
		fixture.task.file = new File([new Uint8Array(128)], "file.bin");
		fixture.task.totalBytes = 128;
		fixture.init.chunk_size = 64;
		providerResumableUpload.mockImplementation(
			async (_url, _blob, start) => start + 1,
		);
		await fixture.run();
		expect(providerResumableUpload).toHaveBeenCalledTimes(32);
		expect(completeUpload).not.toHaveBeenCalled();
		expect(removeSession).not.toHaveBeenCalled();
		expect(fixture.context.markTaskFailed).toHaveBeenCalledWith(
			fixture.task.id,
			expect.objectContaining({
				message: "provider acknowledged too many partial fragments",
				retryable: true,
			}),
		);
	});
	it("does not reconcile terminal or cancelled provider errors", async () => {
		const { UploadRequestError } = await import("@/services/uploadService");
		for (const error of [
			new Error("client failed"),
			new UploadRequestError("permission denied", { status: 403 }),
			new UploadRequestError("cancelled", { isAborted: true }),
		]) {
			const fixture = setup();
			providerResumableUpload.mockRejectedValueOnce(error);
			await fixture.run();
			expect(getProgress).not.toHaveBeenCalled();
			expect(completeUpload).not.toHaveBeenCalled();
		}
	});
	it("resumes partial acknowledgements across logical chunk boundaries", async () => {
		const fixture = setup();
		providerResumableUpload
			.mockResolvedValueOnce(2)
			.mockResolvedValueOnce(6)
			.mockResolvedValueOnce(8);
		await fixture.run();
		expect(
			providerResumableUpload.mock.calls.map((call) => [call[2], call[1].size]),
		).toEqual([
			[0, 4],
			[2, 4],
			[6, 2],
		]);
		expect(completeUpload).toHaveBeenCalledOnce();
		expect(fixture.context.markTaskFailed).not.toHaveBeenCalled();
		expect(fixture.context.patchTask).toHaveBeenLastCalledWith(
			"provider-task",
			expect.objectContaining({ status: "completed", uploadedBytes: 8 }),
		);
	});
	it("restores a byte offset inside a logical chunk after page reload", async () => {
		const fixture = setup(["2-"]);
		providerResumableUpload.mockResolvedValueOnce(6).mockResolvedValueOnce(8);
		await fixture.run();
		expect(
			providerResumableUpload.mock.calls.map((call) => [call[2], call[1].size]),
		).toEqual([
			[2, 4],
			[6, 2],
		]);
		expect(completeUpload).toHaveBeenCalledOnce();
	});
	it("reconciles CORS-hidden acknowledgements through our progress API", async () => {
		const { UploadRequestError } = await import("@/services/uploadService");
		const fixture = setup();
		providerResumableUpload
			.mockRejectedValueOnce(
				new UploadRequestError("header unavailable", {
					status: 308,
					retryable: true,
				}),
			)
			.mockResolvedValueOnce(8);
		getProgress.mockResolvedValue({
			chunks_on_disk: [0],
			provider_resumable: { next_expected_ranges: ["4-"] },
		});
		await fixture.run();
		expect(getProgress).toHaveBeenCalledWith("provider-session");
		expect(providerResumableUpload.mock.calls.map((call) => call[2])).toEqual([
			0, 4,
		]);
		expect(completeUpload).toHaveBeenCalledOnce();
	});
	it("does not finalize after cancellation races with progress reconciliation", async () => {
		const { UploadRequestError } = await import("@/services/uploadService");
		const fixture = setup();
		providerResumableUpload.mockRejectedValue(
			new UploadRequestError("lost response", { retryable: true }),
		);
		getProgress.mockImplementation(async () => {
			fixture.context.abortFlagsRef.current.set(fixture.task.id, true);
			return {
				chunks_on_disk: [0, 1],
				provider_resumable: { next_expected_ranges: [] },
			};
		});
		await fixture.run();
		expect(completeUpload).not.toHaveBeenCalled();
		expect(fixture.context.markTaskFailed).not.toHaveBeenCalled();
		expect(fixture.context.patchTask).toHaveBeenLastCalledWith(
			fixture.task.id,
			expect.objectContaining({ status: "cancelled" }),
		);
	});
	it("stops before sending a retry when cancelled during backoff", async () => {
		vi.useFakeTimers();
		try {
			const { UploadRequestError } = await import("@/services/uploadService");
			const fixture = setup();
			providerResumableUpload.mockRejectedValueOnce(
				new UploadRequestError("lost response", { retryable: true }),
			);
			getProgress.mockImplementation(async () => {
				setTimeout(
					() =>
						fixture.context.abortFlagsRef.current.set(fixture.task.id, true),
					0,
				);
				return {
					chunks_on_disk: [],
					provider_resumable: { next_expected_ranges: ["0-"] },
				};
			});
			const running = fixture.run();
			await vi.runAllTimersAsync();
			await running;
			expect(providerResumableUpload).toHaveBeenCalledOnce();
			expect(getProgress).toHaveBeenCalledOnce();
			expect(completeUpload).not.toHaveBeenCalled();
			expect(fixture.context.patchTask).toHaveBeenLastCalledWith(
				fixture.task.id,
				expect.objectContaining({ status: "cancelled" }),
			);
		} finally {
			vi.useRealTimers();
		}
	});
	it.each([Number.NaN, 5])(
		"reconciles invalid fragment offset %s before publishing",
		async (next) => {
			const fixture = setup();
			providerResumableUpload.mockResolvedValueOnce(next);
			getProgress.mockResolvedValue({
				chunks_on_disk: [0, 1],
				provider_resumable: { next_expected_ranges: [] },
			});
			await fixture.run();
			expect(getProgress).toHaveBeenCalledOnce();
			expect(providerResumableUpload).toHaveBeenCalledOnce();
			expect(completeUpload).toHaveBeenCalledOnce();
		},
	);
	it("reconciles range conflicts using persisted chunks when wire ranges are absent", async () => {
		const { UploadRequestError } = await import("@/services/uploadService");
		const fixture = setup();
		providerResumableUpload
			.mockRejectedValueOnce(
				new UploadRequestError("range conflict", {
					status: 416,
					retryable: false,
				}),
			)
			.mockResolvedValueOnce(8);
		getProgress.mockResolvedValue({ chunks_on_disk: [0] });
		await fixture.run();
		expect(providerResumableUpload.mock.calls.map((call) => call[2])).toEqual([
			0, 4,
		]);
		expect(completeUpload).toHaveBeenCalledOnce();
	});
	it("retries from the provider-confirmed offset after a lost response", async () => {
		vi.useFakeTimers();
		try {
			const { UploadRequestError } = await import("@/services/uploadService");
			const fixture = setup();
			providerResumableUpload
				.mockRejectedValueOnce(
					new UploadRequestError("network error", { retryable: true }),
				)
				.mockResolvedValueOnce(6)
				.mockResolvedValueOnce(8);
			getProgress.mockResolvedValue({
				chunks_on_disk: [],
				provider_resumable: { next_expected_ranges: ["2-"] },
			});
			const running = fixture.run();
			await vi.runAllTimersAsync();
			await running;
			expect(providerResumableUpload.mock.calls.map((call) => call[2])).toEqual(
				[0, 2, 6],
			);
			expect(completeUpload).toHaveBeenCalledOnce();
		} finally {
			vi.useRealTimers();
		}
	});
	it("bounds no-progress retries and preserves recoverable state on failure", async () => {
		vi.useFakeTimers();
		try {
			const fixture = setup();
			providerResumableUpload.mockResolvedValue(0);
			getProgress.mockResolvedValue({
				chunks_on_disk: [],
				provider_resumable: { next_expected_ranges: ["0-"] },
			});
			const running = fixture.run();
			await vi.runAllTimersAsync();
			await running;
			expect(providerResumableUpload).toHaveBeenCalledTimes(3);
			expect(completeUpload).not.toHaveBeenCalled();
			expect(removeSession).not.toHaveBeenCalled();
			expect(fixture.context.markTaskFailed).toHaveBeenCalledOnce();
		} finally {
			vi.useRealTimers();
		}
	});
	it("keeps final object validation failures visible", async () => {
		const fixture = setup();
		providerResumableUpload.mockResolvedValueOnce(4).mockResolvedValueOnce(8);
		completeUpload.mockRejectedValue(new Error("provider size mismatch"));
		await fixture.run();
		expect(fixture.context.markTaskFailed).toHaveBeenCalled();
		expect(fixture.context.patchTask).not.toHaveBeenCalledWith(
			fixture.task.id,
			expect.objectContaining({ status: "completed" }),
		);
	});
});
