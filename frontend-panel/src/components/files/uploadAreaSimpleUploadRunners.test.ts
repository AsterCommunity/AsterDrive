import { describe, expect, it, vi } from "vitest";
import type { UploadTask } from "./uploadAreaManagerShared";
import type { UploadModeRunnerContext } from "./uploadAreaUploadRunnerShared";

const { completeUpload, presignedFormUpload, presignedUpload } = vi.hoisted(
	() => ({
		completeUpload: vi.fn(),
		presignedFormUpload: vi.fn(),
		presignedUpload: vi.fn(),
	}),
);

vi.mock("@/services/http", () => ({
	api: {
		client: {
			post: vi.fn(),
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
		presignedFormUpload,
		presignedUpload,
	},
}));

import { createSimpleUploadRunners } from "./uploadAreaSimpleUploadRunners";

describe("createSimpleUploadRunners", () => {
	it("fails before upload when a presigned response omits its request descriptor", async () => {
		presignedUpload.mockReset();
		completeUpload.mockReset();
		const markTaskFailed = vi.fn();
		const context: UploadModeRunnerContext = {
			abortFlagsRef: { current: new Map() },
			directAbortRef: { current: new Map() },
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
		const context: UploadModeRunnerContext = {
			abortFlagsRef: { current: new Map() },
			directAbortRef: { current: new Map() },
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

	it("uses the provider form request without requiring an ETag", async () => {
		presignedFormUpload.mockReset();
		presignedFormUpload.mockResolvedValue(undefined);
		presignedUpload.mockReset();
		completeUpload.mockReset();
		completeUpload.mockResolvedValue({ id: 1 });
		const context: UploadModeRunnerContext = {
			abortFlagsRef: { current: new Map() },
			directAbortRef: { current: new Map() },
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
			id: "qiniu-form",
			file: new File(["content"], "qiniu.bin"),
			filename: "qiniu.bin",
			relativePath: null,
			baseFolderId: null,
			baseFolderName: "Root",
			totalBytes: 7,
			mode: "presigned",
			status: "uploading",
			progress: 0,
			error: null,
			uploadId: "upload-qiniu",
		};

		const { runPresignedUpload } = createSimpleUploadRunners(context);
		await runPresignedUpload(task, {
			mode: "presigned",
			upload_id: "upload-qiniu",
			presigned_form_request: {
				url: "https://up-z0.qiniup.com",
				fields: { token: "ak:signature:policy", key: "files/upload-qiniu" },
			},
		});

		expect(presignedFormUpload).toHaveBeenCalledWith(
			expect.objectContaining({ url: "https://up-z0.qiniup.com" }),
			task.file,
			expect.any(Function),
			expect.any(Function),
		);
		expect(presignedUpload).not.toHaveBeenCalled();
		expect(completeUpload).toHaveBeenCalledWith("upload-qiniu", undefined);
	});
});
