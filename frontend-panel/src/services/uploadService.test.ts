import { beforeEach, describe, expect, it, vi } from "vitest";
import { ErrorCode } from "@/types/api-helpers";

const mockState = vi.hoisted(() => {
	class MockApiError extends Error {
		code: number;

		constructor(code: number, message: string) {
			super(message);
			this.code = code;
		}
	}

	return {
		ApiError: MockApiError,
		clientPost: vi.fn(),
		delete: vi.fn(),
		get: vi.fn(),
		post: vi.fn(),
	};
});

vi.mock("@/services/http", () => ({
	ApiError: mockState.ApiError,
	api: {
		client: {
			post: mockState.clientPost,
		},
		delete: mockState.delete,
		get: mockState.get,
		post: mockState.post,
	},
}));

function setTestCookie(cookie: string) {
	// biome-ignore lint/suspicious/noDocumentCookie: jsdom tests need direct cookie mutation.
	document.cookie = cookie;
}

class MockXMLHttpRequest {
	static instances: MockXMLHttpRequest[] = [];

	headers: Record<string, string> = {};
	method?: string;
	onabort?: () => void;
	onerror?: () => void;
	onload?: () => void;
	responseHeaders: Record<string, string> = {};
	responseText = "";
	sentBody?: Blob | File;
	status = 0;
	upload: {
		onprogress?: (event: {
			lengthComputable: boolean;
			loaded: number;
			total: number;
		}) => void;
	} = {};
	url?: string;
	withCredentials = false;

	constructor() {
		MockXMLHttpRequest.instances.push(this);
	}

	open(method: string, url: string) {
		this.method = method;
		this.url = url;
	}

	setRequestHeader(name: string, value: string) {
		this.headers[name] = value;
	}

	send(body: Blob | File) {
		this.sentBody = body;
	}

	abort() {
		this.onabort?.();
	}

	getResponseHeader(name: string) {
		return this.responseHeaders[name] ?? null;
	}
}

describe("uploadService", () => {
	beforeEach(() => {
		mockState.clientPost.mockReset();
		mockState.delete.mockReset();
		mockState.get.mockReset();
		mockState.post.mockReset();
		MockXMLHttpRequest.instances = [];
		Object.defineProperty(window, "XMLHttpRequest", {
			configurable: true,
			writable: true,
			value: MockXMLHttpRequest,
		});
		setTestCookie("aster_csrf=; expires=Thu, 01 Jan 1970 00:00:00 GMT; path=/");
	});

	it("uses the expected init/cancel/progress/presign endpoints for the personal workspace", async () => {
		const { createUploadService, uploadService } = await import(
			"@/services/uploadService"
		);

		uploadService.initUpload({
			filename: "hello.txt",
			total_size: 5,
			folder_id: 7,
			relative_path: "docs/hello.txt",
		});
		uploadService.cancelUpload("upload-1");
		uploadService.getProgress("upload-1");
		uploadService.listRecoverableSessions();
		uploadService.presignParts("upload-1", [1, 2, 3]);

		expect(mockState.post).toHaveBeenNthCalledWith(1, "/files/upload/init", {
			filename: "hello.txt",
			total_size: 5,
			folder_id: 7,
			relative_path: "docs/hello.txt",
		});
		expect(mockState.delete).toHaveBeenCalledWith("/files/upload/upload-1");
		expect(mockState.get).toHaveBeenCalledWith("/files/upload/upload-1");
		expect(mockState.get).toHaveBeenCalledWith("/files/upload/sessions");
		expect(mockState.post).toHaveBeenNthCalledWith(
			2,
			"/files/upload/upload-1/presign-parts",
			{
				part_numbers: [1, 2, 3],
			},
		);

		expect(createUploadService).toBeTypeOf("function");
	});

	it("uses the expected init/cancel/progress/presign endpoints for team workspaces", async () => {
		const { createUploadService } = await import("@/services/uploadService");
		const teamUploadService = createUploadService({ kind: "team", teamId: 8 });
		teamUploadService.initUpload({
			filename: "team.txt",
			total_size: 3,
		});
		teamUploadService.cancelUpload("upload-2");
		teamUploadService.getProgress("upload-2");
		teamUploadService.listRecoverableSessions();
		teamUploadService.presignParts("upload-2", [1]);

		expect(mockState.post).toHaveBeenCalledWith("/teams/8/files/upload/init", {
			filename: "team.txt",
			total_size: 3,
		});
		expect(mockState.delete).toHaveBeenCalledWith(
			"/teams/8/files/upload/upload-2",
		);
		expect(mockState.get).toHaveBeenCalledWith(
			"/teams/8/files/upload/upload-2",
		);
		expect(mockState.get).toHaveBeenCalledWith(
			"/teams/8/files/upload/sessions",
		);
		expect(mockState.post).toHaveBeenCalledWith(
			"/teams/8/files/upload/upload-2/presign-parts",
			{
				part_numbers: [1],
			},
		);
	});

	it("uploads chunks via XHR and reports progress", async () => {
		setTestCookie("aster_csrf=csrf-token-1; path=/");
		const { uploadService } = await import("@/services/uploadService");
		const progress = vi.fn();
		const onCreateXhr = vi.fn();
		const blob = new Blob(["hello"]);
		const promise = uploadService.uploadChunk(
			"upload-1",
			3,
			blob,
			progress,
			onCreateXhr,
		);
		const xhr = MockXMLHttpRequest.instances[0];

		xhr.upload.onprogress?.({
			lengthComputable: true,
			loaded: 3,
			total: 5,
		});
		xhr.status = 200;
		xhr.responseText = JSON.stringify({
			code: 0,
			data: { chunk_number: 3, etag: "etag-3" },
		});
		xhr.onload?.();

		await expect(promise).resolves.toEqual({
			chunk_number: 3,
			etag: "etag-3",
		});
		expect(progress).toHaveBeenCalledWith(3, 5);
		expect(onCreateXhr).toHaveBeenCalledWith(xhr);
		expect(xhr.method).toBe("PUT");
		expect(xhr.url).toBe("/api/v1/files/upload/upload-1/3");
		expect(xhr.withCredentials).toBe(true);
		expect(xhr.headers["Content-Type"]).toBe("application/octet-stream");
		expect(xhr.headers["X-CSRF-Token"]).toBe("csrf-token-1");
		expect(xhr.sentBody).toBe(blob);
	});

	it("rejects chunk uploads on API or transport failures", async () => {
		const { UploadRequestError, uploadService } = await import(
			"@/services/uploadService"
		);

		const apiFailure = uploadService.uploadChunk(
			"upload-1",
			1,
			new Blob(["a"]),
		);
		const xhrApi = MockXMLHttpRequest.instances[0];
		xhrApi.status = 200;
		xhrApi.responseText = JSON.stringify({
			code: 3003,
			msg: "upload failed",
		});
		xhrApi.onload?.();
		await expect(apiFailure).rejects.toThrow("upload failed");
		await expect(apiFailure).rejects.toBeInstanceOf(UploadRequestError);

		const statusFailure = uploadService.uploadChunk(
			"upload-1",
			2,
			new Blob(["b"]),
		);
		const xhrStatus = MockXMLHttpRequest.instances[1];
		xhrStatus.status = 500;
		xhrStatus.onload?.();
		await expect(statusFailure).rejects.toThrow("chunk upload failed: 500");
		await expect(statusFailure).rejects.toMatchObject({ retryable: true });

		const networkFailure = uploadService.uploadChunk(
			"upload-1",
			3,
			new Blob(["c"]),
		);
		const xhrNetwork = MockXMLHttpRequest.instances[2];
		xhrNetwork.onerror?.();
		await expect(networkFailure).rejects.toThrow("network error");
		await expect(networkFailure).rejects.toMatchObject({ retryable: true });

		const parseFailure = uploadService.uploadChunk(
			"upload-1",
			4,
			new Blob(["d"]),
		);
		const xhrParse = MockXMLHttpRequest.instances[3];
		xhrParse.status = 200;
		xhrParse.responseText = "";
		xhrParse.onload?.();
		await expect(parseFailure).rejects.toBeInstanceOf(UploadRequestError);
		await expect(parseFailure).rejects.toMatchObject({
			status: 200,
			retryable: false,
		});
	});

	it("rejects chunk uploads when the caller aborts the XHR", async () => {
		const { UploadRequestError, uploadService } = await import(
			"@/services/uploadService"
		);
		const promise = uploadService.uploadChunk(
			"upload-1",
			1,
			new Blob(["hello"]),
		);
		const xhr = MockXMLHttpRequest.instances[0];

		xhr.abort();

		await expect(promise).rejects.toThrow("upload aborted");
		await expect(promise).rejects.toBeInstanceOf(UploadRequestError);
		await expect(promise).rejects.toMatchObject({
			isAborted: true,
			status: 0,
			retryable: false,
		});
	});

	it("rejects presigned uploads when the caller aborts the XHR", async () => {
		const { UploadRequestError, uploadService } = await import(
			"@/services/uploadService"
		);
		const promise = uploadService.presignedUpload(
			"https://storage.example/upload",
			new Blob(["hello"]),
		);
		const xhr = MockXMLHttpRequest.instances[0];

		xhr.abort();

		await expect(promise).rejects.toThrow("upload aborted");
		await expect(promise).rejects.toBeInstanceOf(UploadRequestError);
		await expect(promise).rejects.toMatchObject({
			isAborted: true,
			status: 0,
			retryable: false,
		});
	});

	it("completes uploads with the expected payload and timeout policy", async () => {
		mockState.clientPost.mockResolvedValue({
			data: {
				code: ErrorCode.Success,
				msg: "ok",
				data: { id: 9, name: "done.txt" },
			},
		});
		const { uploadService } = await import("@/services/uploadService");
		const parts = [{ part_number: 1, etag: "etag-1" }];

		await expect(
			uploadService.completeUpload("upload-1", parts),
		).resolves.toEqual({
			id: 9,
			name: "done.txt",
		});
		expect(mockState.clientPost).toHaveBeenCalledWith(
			"/files/upload/upload-1/complete",
			{ parts },
			{ timeout: 0 },
		);
	});

	it("throws ApiError when upload completion fails", async () => {
		mockState.clientPost.mockResolvedValue({
			data: {
				code: ErrorCode.FileUploadFailed,
				msg: "complete failed",
				data: null,
			},
		});
		const { uploadService } = await import("@/services/uploadService");

		await expect(uploadService.completeUpload("upload-1")).rejects.toEqual(
			expect.objectContaining({
				code: ErrorCode.FileUploadFailed,
				message: "complete failed",
			}),
		);
	});

	it("uploads to presigned URLs and requires an ETag", async () => {
		const { uploadService } = await import("@/services/uploadService");
		const progress = vi.fn();
		const onCreateXhr = vi.fn();
		const blob = new Blob(["hello"]);
		const promise = uploadService.presignedUpload(
			"https://s3.example/upload",
			blob,
			progress,
			onCreateXhr,
		);
		const xhr = MockXMLHttpRequest.instances[0];

		xhr.upload.onprogress?.({
			lengthComputable: true,
			loaded: 2,
			total: 5,
		});
		xhr.status = 200;
		xhr.responseHeaders.ETag = '"etag-1"';
		xhr.onload?.();

		await expect(promise).resolves.toBe('"etag-1"');
		expect(progress).toHaveBeenCalledWith(2, 5);
		expect(onCreateXhr).toHaveBeenCalledWith(xhr);
		expect(xhr.method).toBe("PUT");
		expect(xhr.url).toBe("https://s3.example/upload");
		expect(xhr.headers["Content-Type"]).toBe("application/octet-stream");
	});

	it("rejects presigned uploads on missing etags or network failures", async () => {
		const { uploadService } = await import("@/services/uploadService");

		const missingEtag = uploadService.presignedUpload(
			"https://s3.example/upload",
			new Blob(["a"]),
		);
		const xhrMissing = MockXMLHttpRequest.instances[0];
		xhrMissing.status = 200;
		xhrMissing.onload?.();
		await expect(missingEtag).rejects.toThrow(
			"Presigned upload did not return ETag header",
		);

		const failedStatus = uploadService.presignedUpload(
			"https://s3.example/upload",
			new Blob(["b"]),
		);
		const xhrStatus = MockXMLHttpRequest.instances[1];
		xhrStatus.status = 403;
		xhrStatus.onload?.();
		await expect(failedStatus).rejects.toThrow("Presigned upload failed: 403");

		const networkFailure = uploadService.presignedUpload(
			"https://s3.example/upload",
			new Blob(["c"]),
		);
		const xhrNetwork = MockXMLHttpRequest.instances[2];
		xhrNetwork.onerror?.();
		await expect(networkFailure).rejects.toThrow("network error");
	});
});
