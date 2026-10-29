import { beforeEach, describe, expect, it, vi } from "vitest";
import type { BrowserUploadInstruction } from "@/types/api";
import { ApiErrorCode } from "@/types/api-helpers";

const mockState = vi.hoisted(() => {
	class MockApiError extends Error {
		code: string;

		constructor(code: string, message: string) {
			super(message);
			this.code = code;
		}
	}

	return {
		ApiError: MockApiError,
		clientPost: vi.fn(),
		delete: vi.fn(),
		get: vi.fn(),
		loggerDebug: vi.fn(),
		post: vi.fn(),
		uploadFrontendClientId: "11111111-1111-4111-8111-111111111111",
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

vi.mock("@/lib/uploadClientId", () => ({
	getUploadFrontendClientId: () => mockState.uploadFrontendClientId,
}));

vi.mock("@/lib/logger", () => ({
	logger: {
		debug: mockState.loggerDebug,
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
	ontimeout?: () => void;
	onprogress?: (event: { loaded: number }) => void;
	timeout = 0;
	responseHeaders: Record<string, string> = {};
	responseText = "";
	responseURL = "";
	requestHeaderCalls: Array<[name: string, value: string]> = [];
	sentBody?: Blob | File | null;
	status = 0;
	statusText = "";
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
		this.requestHeaderCalls.push([name, value]);
		this.headers[name] = value;
	}

	send(body: Blob | File | null) {
		this.sentBody = body;
	}

	abort() {
		this.onabort?.();
	}

	getResponseHeader(name: string) {
		return this.responseHeaders[name] ?? null;
	}

	getAllResponseHeaders() {
		return Object.entries(this.responseHeaders)
			.map(([name, value]) => `${name}: ${value}`)
			.join("\r\n");
	}
}

describe("uploadService", () => {
	beforeEach(() => {
		mockState.clientPost.mockReset();
		mockState.delete.mockReset();
		mockState.get.mockReset();
		mockState.loggerDebug.mockReset();
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
			frontend_client_id: mockState.uploadFrontendClientId,
		});
		expect(mockState.delete).toHaveBeenCalledWith("/files/upload/upload-1");
		expect(mockState.get).toHaveBeenCalledWith("/files/upload/upload-1");
		expect(mockState.get).toHaveBeenCalledWith("/files/upload/sessions", {
			params: { frontend_client_id: mockState.uploadFrontendClientId },
		});
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
			frontend_client_id: mockState.uploadFrontendClientId,
		});
		expect(mockState.delete).toHaveBeenCalledWith(
			"/teams/8/files/upload/upload-2",
		);
		expect(mockState.get).toHaveBeenCalledWith(
			"/teams/8/files/upload/upload-2",
		);
		expect(mockState.get).toHaveBeenCalledWith(
			"/teams/8/files/upload/sessions",
			{
				params: { frontend_client_id: mockState.uploadFrontendClientId },
			},
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
			code: ApiErrorCode.Success,
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

	it("keeps a pending chunk response retryable", async () => {
		const { uploadService } = await import("@/services/uploadService");
		const promise = uploadService.uploadChunk(
			"upload-1",
			0,
			new Blob(["hello"]),
		);
		const xhr = MockXMLHttpRequest.instances[0];
		xhr.status = 202;
		xhr.responseText = JSON.stringify({
			code: ApiErrorCode.UploadChunkPending,
			msg: "chunk already being uploaded",
			error: { retryable: true },
		});
		xhr.onload?.();

		await expect(promise).rejects.toMatchObject({
			message: "chunk already being uploaded",
			retryable: true,
			status: 202,
		});
	});

	it("normalizes malformed chunk response envelope fields", async () => {
		const { uploadService } = await import("@/services/uploadService");
		const promise = uploadService.uploadChunk(
			"upload-1",
			0,
			new Blob(["hello"]),
		);
		const xhr = MockXMLHttpRequest.instances[0];
		xhr.status = 202;
		xhr.responseText = JSON.stringify({
			code: 7,
			msg: { unexpected: true },
			error: { retryable: "true" },
		});
		xhr.onload?.();

		await expect(promise).rejects.toMatchObject({
			message: "chunk upload failed: 202",
			retryable: false,
			status: 202,
		});
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
			code: ApiErrorCode.FileUploadFailed,
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

		const authFailure = uploadService.uploadChunk(
			"upload-1",
			5,
			new Blob(["e"]),
		);
		const xhrAuth = MockXMLHttpRequest.instances[2];
		xhrAuth.status = 401;
		xhrAuth.responseText = JSON.stringify({
			code: ApiErrorCode.TokenMissing,
			msg: "missing token",
		});
		xhrAuth.onload?.();
		await expect(authFailure).rejects.toThrow("missing token");
		await expect(authFailure).rejects.toMatchObject({
			authFailure: true,
			retryable: true,
			status: 401,
		});

		const networkFailure = uploadService.uploadChunk(
			"upload-1",
			3,
			new Blob(["c"]),
		);
		const xhrNetwork = MockXMLHttpRequest.instances[3];
		xhrNetwork.onerror?.();
		await expect(networkFailure).rejects.toThrow("network error");
		await expect(networkFailure).rejects.toMatchObject({ retryable: true });

		const parseFailure = uploadService.uploadChunk(
			"upload-1",
			4,
			new Blob(["d"]),
		);
		const xhrParse = MockXMLHttpRequest.instances[4];
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

	it("uploads stream bodies with csrf and progress handling", async () => {
		setTestCookie("aster_csrf=stream-csrf; path=/");
		const { uploadService } = await import("@/services/uploadService");
		const progress = vi.fn();
		const onCreateXhr = vi.fn();
		const body = new Blob(["stream-body"]);
		const promise = uploadService.streamUploadBody(
			"stream-1",
			body,
			progress,
			onCreateXhr,
		);
		const xhr = MockXMLHttpRequest.instances[0];
		xhr.upload.onprogress?.({ lengthComputable: false, loaded: 1, total: 2 });
		xhr.upload.onprogress?.({ lengthComputable: true, loaded: 2, total: 10 });
		xhr.status = 201;
		xhr.responseText = JSON.stringify({
			code: ApiErrorCode.Success,
			data: { id: 12, name: "stream.txt" },
		});
		xhr.onload?.();

		await expect(promise).resolves.toEqual({ id: 12, name: "stream.txt" });
		expect(progress).toHaveBeenCalledWith(2, 10);
		expect(onCreateXhr).toHaveBeenCalledWith(xhr);
		expect(xhr.method).toBe("PUT");
		expect(xhr.url).toBe("/api/v1/files/upload/stream-1/body");
		expect(xhr.withCredentials).toBe(true);
		expect(xhr.headers["Content-Type"]).toBe("application/octet-stream");
		expect(xhr.headers["X-CSRF-Token"]).toBe("stream-csrf");
		expect(xhr.sentBody).toBe(body);
	});

	it("maps stream body API, network, and abort failures", async () => {
		const { uploadService } = await import("@/services/uploadService");
		const apiFailure = uploadService.streamUploadBody(
			"stream-1",
			new Blob(["a"]),
		);
		const xhrApi = MockXMLHttpRequest.instances[0];
		xhrApi.status = 422;
		xhrApi.responseText = JSON.stringify({
			code: ApiErrorCode.FileUploadFailed,
			msg: "stream rejected",
			error: { retryable: true },
		});
		xhrApi.onload?.();
		await expect(apiFailure).rejects.toMatchObject({
			message: "stream rejected",
			retryable: true,
			status: 422,
		});

		const networkFailure = uploadService.streamUploadBody(
			"stream-1",
			new Blob(["b"]),
		);
		MockXMLHttpRequest.instances[1].onerror?.();
		await expect(networkFailure).rejects.toMatchObject({
			message: "network error",
			retryable: true,
		});

		const aborted = uploadService.streamUploadBody("stream-1", new Blob(["c"]));
		const xhrAbort = MockXMLHttpRequest.instances[2];
		xhrAbort.status = 0;
		xhrAbort.abort();
		await expect(aborted).rejects.toMatchObject({
			message: "upload aborted",
			isAborted: true,
			status: 0,
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
				code: ApiErrorCode.Success,
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
				code: ApiErrorCode.FileUploadFailed,
				msg: "complete failed",
				data: null,
			},
		});
		const { uploadService } = await import("@/services/uploadService");

		await expect(uploadService.completeUpload("upload-1")).rejects.toEqual(
			expect.objectContaining({
				code: ApiErrorCode.FileUploadFailed,
				message: "complete failed",
			}),
		);
	});

	it("uploads to presigned URLs without inventing request headers", async () => {
		const { uploadService } = await import("@/services/uploadService");
		const progress = vi.fn();
		const onCreateXhr = vi.fn();
		const blob = new Blob(["hello"]);
		const promise = uploadService.presignedUpload(
			"https://s3.example/upload",
			blob,
			progress,
			{ onCreateXhr },
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
		expect(xhr.requestHeaderCalls).toEqual([]);
	});

	function instruction(): BrowserUploadInstruction {
		return {
			request_id: "opaque-request-id",
			timeout_ms: 60000,
			response_body_limit: 65536,
			request: {
				url: "https://plugin.example/upload",
				method: "PATCH" as const,
				headers: { "X-Plugin-Protocol": "v7" },
				body: { start: 2, end: 6 },
				response_headers: ["X-Plugin-Ack"],
			},
		};
	}
	it("executes an opaque plugin instruction without cookies, CSRF, or provider interpretation", async () => {
		setTestCookie("aster_csrf=must-not-leak; path=/");
		const { uploadService } = await import("@/services/uploadService");
		const progress = vi.fn();
		const pending = uploadService.executeBrowserUpload(
			instruction(),
			new Blob(["abcdefgh"]),
			{ onProgress: progress },
		);
		const xhr = MockXMLHttpRequest.instances[0];
		xhr.upload.onprogress?.({ lengthComputable: true, loaded: 2, total: 4 });
		xhr.status = 299;
		xhr.responseText = "plugin-specific-payload";
		xhr.responseHeaders["X-Plugin-Ack"] = "accepted";
		xhr.onload?.();
		await expect(pending).resolves.toEqual({
			status: 299,
			headers: { "x-plugin-ack": "accepted" },
			body: "plugin-specific-payload",
			body_truncated: false,
		});
		expect(xhr.method).toBe("PATCH");
		expect(xhr.withCredentials).toBe(false);
		expect(xhr.requestHeaderCalls).toEqual([["X-Plugin-Protocol", "v7"]]);
		expect(xhr.sentBody?.size).toBe(4);
		expect(progress).toHaveBeenCalledWith(2, 4);
	});
	it.each([202, 308, 200, 201, 204, 403, 416, 503])(
		"reports status %i verbatim to the backend",
		async (status) => {
			const { uploadService } = await import("@/services/uploadService");
			const pending = uploadService.executeBrowserUpload(
				instruction(),
				new Blob(["abcdefgh"]),
			);
			const xhr = MockXMLHttpRequest.instances[0];
			xhr.status = status;
			xhr.onload?.();
			await expect(pending).resolves.toMatchObject({ status, headers: {} });
		},
	);
	it("reports network failure as unknown and treats cancellation as terminal", async () => {
		const { uploadService } = await import("@/services/uploadService");
		const failed = uploadService.executeBrowserUpload(
			instruction(),
			new Blob(["abcdefgh"]),
		);
		MockXMLHttpRequest.instances[0].onerror?.();
		await expect(failed).resolves.toMatchObject({ status: 0 });
		const aborted = uploadService.executeBrowserUpload(
			instruction(),
			new Blob(["abcdefgh"]),
		);
		MockXMLHttpRequest.instances[1].abort();
		await expect(aborted).rejects.toMatchObject({
			isAborted: true,
			retryable: false,
		});
	});
	it("caps response bodies by UTF-8 bytes", async () => {
		const { uploadService } = await import("@/services/uploadService");
		const command = { ...instruction(), response_body_limit: 8 };
		const pending = uploadService.executeBrowserUpload(
			command,
			new Blob(["abcdefgh"]),
		);
		const xhr = MockXMLHttpRequest.instances[0];
		xhr.status = 202;
		xhr.responseText = "猫".repeat(100);
		xhr.onload?.();
		const result = await pending;
		expect(result.body_truncated).toBe(true);
		expect(new TextEncoder().encode(result.body).length).toBeLessThanOrEqual(8);
	});
	it.each([
		"Authorization",
		"Cookie",
		"X-CSRF-Token",
		"Content-Length",
		"Sec-Fetch-Site",
	])("rejects forbidden header %s before creating XHR", async (name) => {
		const { uploadService } = await import("@/services/uploadService");
		const command = instruction();
		command.request.headers = {
			[name]: "secret",
		} as typeof command.request.headers;
		await expect(
			uploadService.executeBrowserUpload(command, new Blob(["abcdefgh"])),
		).rejects.toThrow("invalid browser upload instruction");
		expect(MockXMLHttpRequest.instances).toHaveLength(0);
	});
	it.each([-1, 0.5, Number.MAX_SAFE_INTEGER + 1, Number.NaN])(
		"rejects unsafe file offsets %s",
		async (start) => {
			const { uploadService } = await import("@/services/uploadService");
			const command = instruction();
			command.request.body.start = start;
			await expect(
				uploadService.executeBrowserUpload(command, new Blob(["abcdefgh"])),
			).rejects.toThrow("invalid browser upload instruction");
			expect(MockXMLHttpRequest.instances).toHaveLength(0);
		},
	);
	it("uses the authenticated workspace API for coordination", async () => {
		const { createUploadService } = await import("@/services/uploadService");
		await createUploadService({
			kind: "team",
			teamId: 42,
		}).coordinateBrowserUpload("session", {});
		expect(mockState.post).toHaveBeenCalledWith(
			"/teams/42/files/upload/session/provider-step",
			{},
		);
	});

	it("refuses same-origin XHR because withCredentials=false cannot omit its cookies", async () => {
		const { uploadService } = await import("@/services/uploadService");
		const command = instruction();
		command.request.url = `${location.origin}/capability`;
		await expect(
			uploadService.executeBrowserUpload(command, new Blob(["abcdefgh"])),
		).rejects.toThrow("invalid browser upload instruction");
		expect(MockXMLHttpRequest.instances).toHaveLength(0);
	});
	it("normalizes lost backend connections into retryable coordination failures", async () => {
		const { AxiosError } = await import("axios");
		const { uploadService } = await import("@/services/uploadService");
		mockState.post.mockRejectedValueOnce(new AxiosError("lost", "ERR_NETWORK"));
		await expect(
			uploadService.coordinateBrowserUpload("session", {}),
		).rejects.toMatchObject({ retryable: true });
	});

	it.each(["ECONNABORTED", "ETIMEDOUT"])(
		"coordinates retryable backend transport failure %s",
		async (code) => {
			const { AxiosError } = await import("axios");
			const { uploadService } = await import("@/services/uploadService");
			mockState.post.mockRejectedValueOnce(new AxiosError("timeout", code));
			await expect(
				uploadService.coordinateBrowserUpload("session"),
			).rejects.toMatchObject({ retryable: true, status: undefined });
		},
	);
	it.each([408, 429, 500, 503])(
		"normalizes backend HTTP %s into a retryable error",
		async (status) => {
			const { AxiosError } = await import("axios");
			const { uploadService } = await import("@/services/uploadService");
			mockState.post.mockRejectedValueOnce(
				Object.assign(new AxiosError("server response"), {
					response: { status },
				}),
			);
			await expect(
				uploadService.coordinateBrowserUpload("session"),
			).rejects.toMatchObject({ retryable: true, status });
		},
	);
	it.each([undefined, 403])(
		"preserves unclassified and terminal Axios failures (%s)",
		async (status) => {
			const { AxiosError } = await import("axios");
			const { uploadService } = await import("@/services/uploadService");
			const error = Object.assign(new AxiosError("terminal"), {
				response: status === undefined ? undefined : { status },
			});
			mockState.post.mockRejectedValueOnce(error);
			await expect(
				uploadService.coordinateBrowserUpload("session"),
			).rejects.toBe(error);
		},
	);
	it("preserves authenticated API validation errors without making them retryable", async () => {
		const { uploadService } = await import("@/services/uploadService");
		const error = new Error("upload session expired");
		mockState.post.mockRejectedValueOnce(error);
		await expect(uploadService.coordinateBrowserUpload("session")).rejects.toBe(
			error,
		);
	});
	it("reports timeout as unknown and ignores late completion or abort events", async () => {
		const { uploadService } = await import("@/services/uploadService");
		const pending = uploadService.executeBrowserUpload(
			instruction(),
			new Blob(["abcdefgh"]),
		);
		const xhr = MockXMLHttpRequest.instances[0];
		expect(xhr.timeout).toBe(60000);
		xhr.ontimeout?.();
		xhr.status = 201;
		xhr.responseText = "late";
		xhr.onload?.();
		xhr.abort();
		await expect(pending).resolves.toEqual({
			status: 0,
			headers: {},
			body: "",
			body_truncated: false,
		});
	});
	it("aborts responses exceeding the download budget while retaining a reconciliation observation", async () => {
		const { uploadService } = await import("@/services/uploadService");
		const pending = uploadService.executeBrowserUpload(
			{ ...instruction(), response_body_limit: 8 },
			new Blob(["abcdefgh"]),
		);
		const xhr = MockXMLHttpRequest.instances[0];
		const abort = vi.spyOn(xhr, "abort");
		xhr.onprogress?.({ loaded: 8 });
		expect(abort).not.toHaveBeenCalled();
		xhr.onprogress?.({ loaded: 9 });
		expect(abort).toHaveBeenCalledOnce();
		xhr.onload?.();
		await expect(pending).resolves.toEqual({
			status: 0,
			headers: {},
			body: "",
			body_truncated: true,
		});
	});
	it("does not send file bytes if cancellation fires while the XHR is registered", async () => {
		const { uploadService } = await import("@/services/uploadService");
		const pending = uploadService.executeBrowserUpload(
			instruction(),
			new Blob(["abcdefgh"]),
			{ onCreateXhr: (xhr) => xhr.abort() },
		);
		const xhr = MockXMLHttpRequest.instances[0];
		expect(xhr.sentBody).toBeUndefined();
		xhr.onerror?.();
		await expect(pending).rejects.toMatchObject({
			isAborted: true,
			retryable: false,
		});
	});
	it("sends empty-body control requests and ignores noncomputable upload progress", async () => {
		const { uploadService } = await import("@/services/uploadService");
		const command = instruction();
		command.request.body = null;
		command.request.method = "GET";
		command.request.headers = {};
		const progress = vi.fn();
		const pending = uploadService.executeBrowserUpload(
			command,
			new Blob(["abcdefgh"]),
			{ onProgress: progress },
		);
		const xhr = MockXMLHttpRequest.instances[0];
		expect(xhr.sentBody).toBeNull();
		xhr.upload.onprogress?.({ lengthComputable: false, loaded: 2, total: 8 });
		expect(progress).not.toHaveBeenCalled();
		xhr.status = 200;
		xhr.onload?.();
		await expect(pending).resolves.toMatchObject({ status: 200 });
	});
	it.each([
		(command: BrowserUploadInstruction) => {
			command.request.url = "file:///tmp/file";
		},
		(command: BrowserUploadInstruction) => {
			command.request.url = "https://user@plugin.example/upload";
		},
		(command: BrowserUploadInstruction) => {
			command.request.url = "https://:secret@plugin.example/upload";
		},
		(command: BrowserUploadInstruction) => {
			command.request.url += "#fragment";
		},
		(command: BrowserUploadInstruction) => {
			command.request.method = "TRACE" as typeof command.request.method;
		},
		(command: BrowserUploadInstruction) => {
			command.response_body_limit = 0;
		},
		(command: BrowserUploadInstruction) => {
			command.response_body_limit = 65537;
		},
		(command: BrowserUploadInstruction) => {
			command.response_body_limit = 1.5;
		},
		(command: BrowserUploadInstruction) => {
			command.timeout_ms = 0;
		},
		(command: BrowserUploadInstruction) => {
			command.timeout_ms = 60001;
		},
		(command: BrowserUploadInstruction) => {
			command.timeout_ms = 1.5;
		},
		(command: BrowserUploadInstruction) => {
			command.request.response_headers = ["Set-Cookie"];
		},
		(command: BrowserUploadInstruction) => {
			command.request.body = { start: 0, end: 0 };
		},
		(command: BrowserUploadInstruction) => {
			command.request.body = { start: 0, end: 9 };
		},
		(command: BrowserUploadInstruction) => {
			command.request.body = { start: 0, end: 1.5 };
		},
		(command: BrowserUploadInstruction) => {
			command.request.method = "GET";
		},
	])(
		"rejects unsafe execution metadata before creating a network request",
		async (mutate) => {
			const { uploadService } = await import("@/services/uploadService");
			const command = instruction();
			mutate(command);
			await expect(
				uploadService.executeBrowserUpload(command, new Blob(["abcdefgh"])),
			).rejects.toThrow("invalid browser upload instruction");
			expect(MockXMLHttpRequest.instances).toHaveLength(0);
		},
	);
	it("applies provider-required presigned PUT headers", async () => {
		const { uploadService } = await import("@/services/uploadService");

		const promise = uploadService.presignedUpload(
			"https://account.blob.core.windows.net/container/blob",
			new Blob(["hello"]),
			undefined,
			{
				requireEtag: false,
				headers: { "x-ms-blob-type": "BlockBlob" },
			},
		);
		const xhr = MockXMLHttpRequest.instances[0];
		xhr.status = 201;
		xhr.onload?.();

		await expect(promise).resolves.toBe("");
		expect(xhr.headers["Content-Type"]).toBeUndefined();
		expect(xhr.headers["x-ms-blob-type"]).toBe("BlockBlob");
	});

	it("sets a provider-signed Content-Type exactly once", async () => {
		const { uploadService } = await import("@/services/uploadService");

		const promise = uploadService.presignedUpload(
			"https://oss.example/upload",
			new Blob(["hello"]),
			undefined,
			{
				requireEtag: false,
				headers: { "content-type": "application/octet-stream" },
			},
		);
		const xhr = MockXMLHttpRequest.instances[0];
		xhr.status = 200;
		xhr.onload?.();

		await expect(promise).resolves.toBe("");
		expect(
			xhr.requestHeaderCalls.filter(
				([name]) => name.toLowerCase() === "content-type",
			),
		).toEqual([["content-type", "application/octet-stream"]]);
	});

	it("does not read ETag when the provider does not require it", async () => {
		const { uploadService } = await import("@/services/uploadService");
		const promise = uploadService.presignedUpload(
			"https://oss.example/upload",
			new Blob(["hello"]),
			undefined,
			{ requireEtag: false },
		);
		const xhr = MockXMLHttpRequest.instances[0];
		const getResponseHeader = vi.spyOn(xhr, "getResponseHeader");

		xhr.status = 200;
		xhr.onload?.();

		await expect(promise).resolves.toBe("");
		expect(getResponseHeader).not.toHaveBeenCalled();
	});

	it("still requires an ETag when headers are provided without an explicit override", async () => {
		const { uploadService } = await import("@/services/uploadService");

		const promise = uploadService.presignedUpload(
			"https://storage.example/upload",
			new Blob(["hello"]),
			undefined,
			{
				headers: { "x-provider-required": "1" },
			},
		);
		const xhr = MockXMLHttpRequest.instances[0];
		xhr.status = 200;
		xhr.onload?.();

		await expect(promise).rejects.toThrow(
			"Presigned upload did not return ETag header",
		);
		expect(xhr.headers["x-provider-required"]).toBe("1");
	});

	it("allows presigned uploads without ETag when explicitly requested", async () => {
		const { uploadService } = await import("@/services/uploadService");

		const promise = uploadService.presignedUpload(
			"https://blob.example/upload",
			new Blob(["hello"]),
			undefined,
			{ requireEtag: false },
		);
		const xhr = MockXMLHttpRequest.instances[0];
		xhr.status = 201;
		xhr.onload?.();

		await expect(promise).resolves.toBe("");
	});

	it("still rejects failed presigned uploads when ETag is optional", async () => {
		const { uploadService } = await import("@/services/uploadService");

		const promise = uploadService.presignedUpload(
			"https://blob.example/upload",
			new Blob(["hello"]),
			undefined,
			{
				requireEtag: false,
				headers: {
					Authorization: "Bearer secret-presigned-token",
					"Content-Type": "application/octet-stream",
					"x-amz-security-token": "temporary-storage-credential",
				},
			},
		);
		const xhr = MockXMLHttpRequest.instances[0];
		xhr.status = 409;
		xhr.statusText = "Conflict";
		xhr.responseURL = "https://blob.example/upload";
		xhr.responseHeaders = { "x-oss-request-id": "request-id" };
		xhr.responseText = "<Error><Code>SignatureDoesNotMatch</Code></Error>";
		xhr.onload?.();

		await expect(promise).rejects.toThrow("Presigned upload failed: 409");
		expect(mockState.loggerDebug).toHaveBeenCalledWith(
			"presigned upload failed",
			{
				event: "http-error",
				method: "PUT",
				url: "https://blob.example/upload",
				requestHeaderNames: [
					"Authorization",
					"Content-Type",
					"x-amz-security-token",
				],
				bodySize: 5,
				status: 409,
				statusText: "Conflict",
				responseUrl: "https://blob.example/upload",
				responseHeaders: "x-oss-request-id: request-id",
				responseText: "<Error><Code>SignatureDoesNotMatch</Code></Error>",
			},
		);
		const logged = JSON.stringify(mockState.loggerDebug.mock.calls);
		expect(logged).not.toContain("secret-presigned-token");
		expect(logged).not.toContain("temporary-storage-credential");
	});

	it("uses Azure block IDs as presigned multipart completion markers", async () => {
		const { uploadService } = await import("@/services/uploadService");

		const promise = uploadService.presignedUpload(
			"https://account.blob.core.windows.net/container/blob?comp=block&blockid=abc%2B%2F%3D",
			new Blob(["hello"]),
		);
		const xhr = MockXMLHttpRequest.instances[0];
		xhr.status = 201;
		xhr.onload?.();

		await expect(promise).resolves.toBe("abc+/=");
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
