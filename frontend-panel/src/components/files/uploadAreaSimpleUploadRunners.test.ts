import { describe, expect, it, vi } from "vitest";
import type { UploadTask } from "./uploadAreaManagerShared";
import type { UploadTransportRunnerContext } from "./uploadAreaUploadRunnerShared";

const { completeUpload, presignedUpload, put, removeSession } = vi.hoisted(
	() => ({
		completeUpload: vi.fn(),
		presignedUpload: vi.fn(),
		put: vi.fn(),
		removeSession: vi.fn(),
	}),
);

vi.mock("@/lib/uploadPersistence", () => ({ removeSession }));

vi.mock("@/services/http", () => ({
	api: {
		client: {
			post: vi.fn(),
			put,
		},
	},
}));

vi.mock("@/services/uploadService", () => ({
	buildUploadPath: (_workspace: unknown, path: string) => path,
	UploadRequestError: class UploadRequestError extends Error {
		isAborted = false;
	},
	uploadService: {
		completeUpload,
		presignedUpload,
		streamUploadBody: (_id: string, data: Blob) =>
			put("/files/upload/stream-session/body", data, { timeout: 0 }),
	},
}));

import { createSimpleUploadRunners } from "./uploadAreaSimpleUploadRunners";

describe("createSimpleUploadRunners", () => {
	it("fails a stream task before issuing a request when its session is missing", async () => {
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
			id: "missing-stream",
			file: new File(["content"], "missing.bin"),
			filename: "missing.bin",
			relativePath: null,
			baseFolderId: null,
			baseFolderName: "Root",
			totalBytes: 7,
			mode: "stream",
			status: "queued",
			progress: 0,
			error: null,
			uploadId: null,
		};
		const { runStreamUpload } = createSimpleUploadRunners(context);
		await runStreamUpload(task);
		expect(markTaskFailed).toHaveBeenCalledWith(
			"missing-stream",
			expect.objectContaining({ message: "Missing stream upload session" }),
		);
		expect(put).not.toHaveBeenCalled();
	});

	it("publishes a stream session through its body endpoint without a second complete", async () => {
		put.mockReset();
		put.mockResolvedValue({});
		completeUpload.mockReset();
		completeUpload.mockResolvedValue({});
		removeSession.mockReset();
		const context = {
			abortFlagsRef: { current: new Map() },
			metadataAbortRef: { current: new Map() },
			flushProgress: vi.fn(),
			markFolderForRefresh: vi.fn(),
			markTaskFailed: vi.fn(),
			multipartInFlightRef: { current: new Map() },
			patchTask: vi.fn(),
			patchTaskThrottled: vi.fn(),
			uploadRequestRef: { current: new Map() },
			t: (key: string) => key,
			workspace: { kind: "personal" as const },
		};
		const task: UploadTask = {
			id: "stream",
			file: new File(["content"], "stream.bin"),
			filename: "stream.bin",
			relativePath: null,
			baseFolderId: null,
			baseFolderName: "Root",
			totalBytes: 7,
			mode: "stream",
			status: "queued",
			progress: 0,
			error: null,
			uploadId: "stream-session",
		};

		const { runStreamUpload } = createSimpleUploadRunners(context);
		await runStreamUpload(task);

		expect(put).toHaveBeenCalledWith(
			"/files/upload/stream-session/body",
			expect.any(File),
			expect.objectContaining({ timeout: 0 }),
		);
		expect(completeUpload).not.toHaveBeenCalled();
		expect(removeSession).toHaveBeenCalledWith("stream-session");
	});

	it("fails before upload when a presigned response omits its request descriptor", async () => {
		presignedUpload.mockReset();
		completeUpload.mockReset();
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
			id: "missing-request",
			file: new File(["content"], "presigned.bin"),
			filename: "presigned.bin",
			relativePath: null,
			baseFolderId: null,
			baseFolderName: "Root",
			totalBytes: 7,
			mode: "presigned",
			status: "uploading",
			progress: 0,
			error: null,
			uploadId: "upload-presigned",
		};

		const { runPresignedUpload } = createSimpleUploadRunners(context);
		await runPresignedUpload(task, {
			mode: "presigned",
			upload_id: "upload-presigned",
		});

		expect(markTaskFailed).toHaveBeenCalledWith(
			"missing-request",
			expect.objectContaining({ message: "Missing presigned upload request" }),
		);
		expect(presignedUpload).not.toHaveBeenCalled();
		expect(completeUpload).not.toHaveBeenCalled();
	});

	it("passes the original presigned upload error to task failure handling", async () => {
		const error = Object.assign(new Error("presigned upload failed"), {
			retryable: false,
		});
		presignedUpload.mockReset();
		presignedUpload.mockRejectedValue(error);
		completeUpload.mockReset();
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
			id: "presigned",
			file: new File(["content"], "presigned.bin"),
			filename: "presigned.bin",
			relativePath: null,
			baseFolderId: null,
			baseFolderName: "Root",
			totalBytes: 7,
			mode: "presigned",
			status: "uploading",
			progress: 0,
			error: null,
			uploadId: "upload-presigned",
		};

		const { runPresignedUpload } = createSimpleUploadRunners(context);
		await runPresignedUpload(task, {
			mode: "presigned",
			upload_id: "upload-presigned",
			presigned_request: {
				url: "https://storage.example/upload",
			},
		});

		expect(markTaskFailed).toHaveBeenCalledWith("presigned", error);
		expect(completeUpload).not.toHaveBeenCalled();
	});
});
