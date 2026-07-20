import { beforeEach, describe, expect, it, vi } from "vitest";
import {
	cancelDownloadTask,
	filenameFromContentDisposition,
	retryDownloadTask,
	startDirectoryDownload,
	startProxyArchiveDownload,
	startProxyFileDownload,
	supportsDirectoryDownload,
} from "@/services/downloadCoordinator";
import { DOWNLOAD_TASK_STATUS, useDownloadStore } from "@/stores/downloadStore";

const mocks = vi.hoisted(() => ({
	archiveDownloadUrl: vi.fn(),
	createArchiveDownloadTicket: vi.fn(),
	listFolder: vi.fn(),
	resolveResourceHandle: vi.fn(),
}));

vi.mock("@/services/batchService", () => ({
	createBatchService: () => ({
		archiveDownloadUrl: mocks.archiveDownloadUrl,
		createArchiveDownloadTicket: mocks.createArchiveDownloadTicket,
	}),
}));

vi.mock("@/services/fileService", () => ({
	createFileService: () => ({
		listFolder: mocks.listFolder,
		resolveResourceHandle: mocks.resolveResourceHandle,
	}),
}));

function resetStore() {
	useDownloadStore.setState({
		pendingSelection: null,
		tasks: [],
	});
}

function responseBody(parts: string[]) {
	return new ReadableStream<Uint8Array>({
		start(controller) {
			for (const part of parts)
				controller.enqueue(new TextEncoder().encode(part));
			controller.close();
		},
	});
}

function selection() {
	return {
		workspace: { kind: "personal" as const },
		files: [{ id: 1, name: "a.txt", size: 4 }],
		folders: [],
	};
}

function createDirectoryFixture(initialEntries: string[] = []) {
	const directories = new Set<string>();
	const files = new Set(initialEntries);
	const writes: string[] = [];
	const writableByPath = new Map<
		string,
		{ write: ReturnType<typeof vi.fn>; close: ReturnType<typeof vi.fn> }
	>();
	const directory = (prefix = "") => ({
		getDirectoryHandle: vi.fn(
			(name: string, options?: { create?: boolean }) => {
				const path = `${prefix}${name}`;
				if (files.has(path)) {
					return Promise.reject(
						new DOMException("file exists", "TypeMismatchError"),
					);
				}
				if (options?.create) directories.add(path);
				if (!directories.has(path)) {
					return Promise.reject(new DOMException("missing", "NotFoundError"));
				}
				return Promise.resolve(directory(`${path}/`));
			},
		),
		getFileHandle: vi.fn((name: string, options?: { create?: boolean }) => {
			const path = `${prefix}${name}`;
			if (directories.has(path)) {
				return Promise.reject(
					new DOMException("directory exists", "TypeMismatchError"),
				);
			}
			if (!options?.create && !files.has(path)) {
				return Promise.reject(new DOMException("missing", "NotFoundError"));
			}
			if (options?.create) {
				files.add(path);
				writes.push(path);
			}
			const writable = {
				write: vi.fn().mockResolvedValue(undefined),
				close: vi.fn().mockResolvedValue(undefined),
			};
			writableByPath.set(path, writable);
			return Promise.resolve({
				createWritable: vi.fn().mockResolvedValue(writable),
			});
		}),
	});
	return { directories, directory: directory(), files, writableByPath, writes };
}

describe("downloadCoordinator", () => {
	beforeEach(() => {
		resetStore();
		mocks.archiveDownloadUrl.mockReset();
		mocks.createArchiveDownloadTicket.mockReset();
		mocks.listFolder.mockReset();
		mocks.resolveResourceHandle.mockReset();
		vi.unstubAllGlobals();
		Object.defineProperty(window, "showSaveFilePicker", {
			configurable: true,
			value: undefined,
		});
		Object.defineProperty(window, "showDirectoryPicker", {
			configurable: true,
			value: undefined,
		});
		vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(() => {});
		vi.spyOn(URL, "createObjectURL").mockReturnValue("blob:download");
		vi.spyOn(URL, "revokeObjectURL").mockImplementation(() => {});
	});

	it("uses the backend resource handle credentials and reports streaming progress", async () => {
		mocks.resolveResourceHandle.mockResolvedValue({
			request: {
				url: "https://objects.example.test/file",
				credentials: "omit",
			},
		});
		const fetchMock = vi.fn().mockResolvedValue(
			new Response(responseBody(["abc", "def"]), {
				status: 200,
				headers: {
					"content-disposition":
						"attachment; filename*=UTF-8''notes%20final.txt",
					"content-length": "6",
					"content-type": "text/plain",
				},
			}),
		);
		vi.stubGlobal("fetch", fetchMock);

		await startProxyFileDownload(
			{ kind: "team", teamId: 9 },
			{ id: 7, name: "notes.txt", size: 6 },
		);

		expect(mocks.resolveResourceHandle).toHaveBeenCalledWith(7, {
			purpose: "download",
			delivery_mode: "blob_url",
			representation: "original",
		});
		expect(fetchMock).toHaveBeenCalledWith(
			"https://objects.example.test/file",
			expect.objectContaining({ credentials: "omit", redirect: "follow" }),
		);
		expect(useDownloadStore.getState().tasks[0]).toMatchObject({
			name: "notes final.txt",
			status: "completed",
			bytesReceived: 6,
			totalBytes: 6,
			completedItems: 1,
			warning: "download_memory_fallback",
		});
	});

	it("parses RFC 5987 and quoted content-disposition filenames", () => {
		expect(
			filenameFromContentDisposition(
				"attachment; filename=ignored.txt; filename*=UTF-8''report%20final.csv",
			),
		).toBe("report final.csv");
		expect(
			filenameFromContentDisposition(
				'attachment; filename="../quarterly report.pdf"',
			),
		).toBe("quarterly report.pdf");
		expect(
			filenameFromContentDisposition(
				"attachment; filename*=UTF-8''..%2F..%2Fsecret.txt",
			),
		).toBe("secret.txt");
		expect(
			filenameFromContentDisposition(
				"attachment; filename*=UTF-8''bad%ZZname.txt",
			),
		).toBe("bad%ZZname.txt");
		expect(
			filenameFromContentDisposition("attachment; filename=   "),
		).toBeNull();
		expect(filenameFromContentDisposition(null)).toBeNull();
	});

	it("streams a proxy file to the save picker without creating a blob", async () => {
		const write = vi.fn().mockResolvedValue(undefined);
		const close = vi.fn().mockResolvedValue(undefined);
		const showSaveFilePicker = vi.fn().mockResolvedValue({
			createWritable: vi.fn().mockResolvedValue({ write, close }),
		});
		Object.defineProperty(window, "showSaveFilePicker", {
			configurable: true,
			value: showSaveFilePicker,
		});
		mocks.resolveResourceHandle.mockResolvedValue({
			request: { url: "/files/1", credentials: "include" },
		});
		vi.stubGlobal(
			"fetch",
			vi.fn().mockResolvedValue(
				new Response(responseBody(["data"]), {
					status: 200,
					headers: { "content-length": "4" },
				}),
			),
		);

		await startProxyFileDownload(
			{ kind: "personal" },
			{ id: 1, name: "a.txt", size: 4 },
		);

		expect(showSaveFilePicker).toHaveBeenCalledWith({ suggestedName: "a.txt" });
		expect(write).toHaveBeenCalledTimes(1);
		expect(close).toHaveBeenCalledTimes(1);
		expect(URL.createObjectURL).not.toHaveBeenCalled();
		expect(useDownloadStore.getState().tasks[0]).toMatchObject({
			status: DOWNLOAD_TASK_STATUS.completed,
			warning: undefined,
		});
	});

	it("marks a save picker cancellation as canceled without an error", async () => {
		Object.defineProperty(window, "showSaveFilePicker", {
			configurable: true,
			value: vi
				.fn()
				.mockRejectedValue(new DOMException("user canceled", "AbortError")),
		});

		await startProxyFileDownload(
			{ kind: "personal" },
			{ id: 1, name: "a.txt", size: 4 },
		);

		expect(useDownloadStore.getState().tasks[0]).toMatchObject({
			status: DOWNLOAD_TASK_STATUS.canceled,
			error: undefined,
		});
		expect(mocks.resolveResourceHandle).not.toHaveBeenCalled();
	});

	it("preserves backend API errors and falls back for invalid JSON errors", async () => {
		mocks.resolveResourceHandle.mockResolvedValue({
			request: { url: "/files/1", credentials: "include" },
		});
		const fetchMock = vi
			.fn()
			.mockResolvedValueOnce(
				new Response(JSON.stringify({ code: "file.not_found", msg: "gone" }), {
					status: 404,
					headers: { "content-type": "application/json" },
				}),
			)
			.mockResolvedValueOnce(
				new Response("{bad", {
					status: 502,
					headers: { "content-type": "application/json" },
				}),
			);
		vi.stubGlobal("fetch", fetchMock);

		await startProxyFileDownload(
			{ kind: "personal" },
			{ id: 1, name: "a.txt" },
		);
		expect(useDownloadStore.getState().tasks[0]).toMatchObject({
			status: DOWNLOAD_TASK_STATUS.failed,
			error: "gone",
		});

		await startProxyFileDownload(
			{ kind: "personal" },
			{ id: 1, name: "b.txt" },
		);
		expect(useDownloadStore.getState().tasks[0]).toMatchObject({
			status: DOWNLOAD_TASK_STATUS.failed,
			error: "Download request failed with HTTP 502",
		});
	});

	it("rejects a truncated body, aborts the sink, and never closes it", async () => {
		const write = vi.fn().mockResolvedValue(undefined);
		const close = vi.fn().mockResolvedValue(undefined);
		const abort = vi.fn().mockResolvedValue(undefined);
		Object.defineProperty(window, "showSaveFilePicker", {
			configurable: true,
			value: vi.fn().mockResolvedValue({
				createWritable: vi.fn().mockResolvedValue({ write, close, abort }),
			}),
		});
		mocks.resolveResourceHandle.mockResolvedValue({
			request: { url: "/files/1", credentials: "include" },
		});
		vi.stubGlobal(
			"fetch",
			vi.fn().mockResolvedValue(
				new Response(responseBody(["abc"]), {
					status: 200,
					headers: { "content-length": "5" },
				}),
			),
		);

		await startProxyFileDownload(
			{ kind: "personal" },
			{ id: 1, name: "a.txt" },
		);

		expect(abort).toHaveBeenCalledTimes(1);
		expect(close).not.toHaveBeenCalled();
		expect(useDownloadStore.getState().tasks[0]).toMatchObject({
			status: DOWNLOAD_TASK_STATUS.failed,
			error: "Download size mismatch: expected 5, received 3",
		});
	});

	it("validates content length in the arrayBuffer fallback", async () => {
		const write = vi.fn().mockResolvedValue(undefined);
		const close = vi.fn().mockResolvedValue(undefined);
		const abort = vi.fn().mockResolvedValue(undefined);
		Object.defineProperty(window, "showSaveFilePicker", {
			configurable: true,
			value: vi.fn().mockResolvedValue({
				createWritable: vi.fn().mockResolvedValue({ write, close, abort }),
			}),
		});
		mocks.resolveResourceHandle.mockResolvedValue({
			request: { url: "/files/1", credentials: "include" },
		});
		vi.stubGlobal(
			"fetch",
			vi.fn().mockResolvedValue({
				ok: true,
				body: null,
				headers: new Headers({ "content-length": "5" }),
				arrayBuffer: vi
					.fn()
					.mockResolvedValue(new TextEncoder().encode("abc").buffer),
			}),
		);

		await startProxyFileDownload(
			{ kind: "personal" },
			{ id: 1, name: "a.txt" },
		);

		expect(abort).toHaveBeenCalledTimes(1);
		expect(close).not.toHaveBeenCalled();
		expect(useDownloadStore.getState().tasks[0]).toMatchObject({
			status: DOWNLOAD_TASK_STATUS.failed,
			error: "Download size mismatch: expected 5, received 3",
		});
	});

	it("retries a failed proxy file task without creating a second task", async () => {
		mocks.resolveResourceHandle.mockResolvedValue({
			request: { url: "/files/7", credentials: "include" },
		});
		const fetchMock = vi
			.fn()
			.mockResolvedValueOnce(new Response(null, { status: 503 }))
			.mockResolvedValueOnce(
				new Response(responseBody(["done"]), {
					status: 200,
					headers: { "content-length": "4" },
				}),
			);
		vi.stubGlobal("fetch", fetchMock);

		const taskId = await startProxyFileDownload(
			{ kind: "personal" },
			{ id: 7, name: "retry.txt", size: 4 },
		);
		expect(useDownloadStore.getState().tasks[0]?.status).toBe("failed");

		retryDownloadTask(taskId);
		await vi.waitFor(() => {
			expect(useDownloadStore.getState().tasks[0]).toMatchObject({
				status: "completed",
				bytesReceived: 4,
			});
		});
		expect(fetchMock).toHaveBeenCalledTimes(2);
		expect(useDownloadStore.getState().tasks).toHaveLength(1);
	});

	it("streams archive tickets to a save handle without buffering the ZIP", async () => {
		const write = vi.fn().mockResolvedValue(undefined);
		const close = vi.fn().mockResolvedValue(undefined);
		Object.defineProperty(window, "showSaveFilePicker", {
			configurable: true,
			value: vi.fn().mockResolvedValue({
				createWritable: vi.fn().mockResolvedValue({ write, close }),
			}),
		});
		mocks.createArchiveDownloadTicket.mockResolvedValue({
			token: "ticket",
			download_path: "/api/v1/batch/archive-download/ticket",
			expires_at: "2026-07-20T10:00:00Z",
		});
		mocks.archiveDownloadUrl.mockReturnValue(
			"/api/v1/batch/archive-download/ticket",
		);
		vi.stubGlobal(
			"fetch",
			vi.fn().mockResolvedValue(
				new Response(responseBody(["zip-data"]), {
					status: 200,
					headers: { "content-length": "8" },
				}),
			),
		);

		await startProxyArchiveDownload(
			{
				workspace: { kind: "personal" },
				files: [{ id: 1, name: "a.txt" }],
				folders: [],
			},
			"bundle",
		);

		expect(mocks.createArchiveDownloadTicket).toHaveBeenCalledWith(
			[1],
			[],
			"bundle.zip",
		);
		expect(write).toHaveBeenCalled();
		expect(close).toHaveBeenCalledTimes(1);
		expect(URL.createObjectURL).not.toHaveBeenCalled();
		expect(useDownloadStore.getState().tasks[0]).toMatchObject({
			name: "bundle.zip",
			status: "completed",
			warning: undefined,
		});
		expect(window.showSaveFilePicker).toHaveBeenCalledWith({
			suggestedName: "bundle.zip",
			types: [
				{
					description: "ZIP archive",
					accept: { "application/zip": [".zip"] },
				},
			],
		});
	});

	it("uses include credentials for same-origin archive tickets and omit for remote tickets", async () => {
		mocks.createArchiveDownloadTicket.mockResolvedValue({ token: "ticket" });
		const fetchMock = vi.fn().mockResolvedValue(
			new Response(responseBody(["zip"]), {
				status: 200,
				headers: { "content-length": "3" },
			}),
		);
		vi.stubGlobal("fetch", fetchMock);

		mocks.archiveDownloadUrl.mockReturnValueOnce("/api/v1/archive/ticket");
		await startProxyArchiveDownload(selection(), "same-origin");
		expect(fetchMock.mock.calls[0]?.[1]).toEqual(
			expect.objectContaining({ credentials: "include" }),
		);

		mocks.archiveDownloadUrl.mockReturnValueOnce(
			"https://objects.example.test/archive",
		);
		await startProxyArchiveDownload(selection(), "remote");
		expect(fetchMock.mock.calls[1]?.[1]).toEqual(
			expect.objectContaining({ credentials: "omit" }),
		);
	});

	it("uses selection-based archive names and retries the same archive task", async () => {
		mocks.createArchiveDownloadTicket.mockResolvedValue({ token: "ticket" });
		mocks.archiveDownloadUrl.mockReturnValue("/archive");
		const fetchMock = vi
			.fn()
			.mockResolvedValueOnce(new Response(null, { status: 503 }))
			.mockResolvedValueOnce(
				new Response(responseBody(["zip"]), {
					status: 200,
					headers: { "content-length": "3" },
				}),
			);
		vi.stubGlobal("fetch", fetchMock);

		const taskId = await startProxyArchiveDownload(selection());
		expect(useDownloadStore.getState().tasks[0]).toMatchObject({
			name: "a.txt.zip",
			status: DOWNLOAD_TASK_STATUS.failed,
		});

		retryDownloadTask(taskId);
		await vi.waitFor(() =>
			expect(useDownloadStore.getState().tasks[0]?.status).toBe(
				DOWNLOAD_TASK_STATUS.completed,
			),
		);
		expect(useDownloadStore.getState().tasks).toHaveLength(1);
		expect(mocks.createArchiveDownloadTicket).toHaveBeenCalledWith(
			[1],
			[],
			"a.txt.zip",
		);
	});

	it("reports unsupported and canceled directory picker states", async () => {
		expect(supportsDirectoryDownload()).toBe(false);
		await startDirectoryDownload(selection());
		expect(useDownloadStore.getState().tasks[0]).toMatchObject({
			status: DOWNLOAD_TASK_STATUS.failed,
			error: "download_directory_unsupported",
		});

		Object.defineProperty(window, "showDirectoryPicker", {
			configurable: true,
			value: vi
				.fn()
				.mockRejectedValue(new DOMException("user canceled", "AbortError")),
		});
		expect(supportsDirectoryDownload()).toBe(true);
		await startDirectoryDownload(selection());
		expect(useDownloadStore.getState().tasks[0]).toMatchObject({
			status: DOWNLOAD_TASK_STATUS.canceled,
			error: undefined,
		});
	});

	it("renames duplicate root files including dotfiles and extensionless names", async () => {
		const fixture = createDirectoryFixture(["report.txt", ".env", "README"]);
		Object.defineProperty(window, "showDirectoryPicker", {
			configurable: true,
			value: vi.fn().mockResolvedValue(fixture.directory),
		});
		mocks.resolveResourceHandle.mockImplementation((id: number) =>
			Promise.resolve({
				request: { url: `/files/${id}`, credentials: "include" },
			}),
		);
		vi.stubGlobal(
			"fetch",
			vi.fn().mockResolvedValue(
				new Response(responseBody(["data"]), {
					status: 200,
					headers: { "content-length": "4" },
				}),
			),
		);

		await startDirectoryDownload({
			workspace: { kind: "personal" },
			files: [
				{ id: 1, name: "report.txt", size: 4 },
				{ id: 2, name: "report.txt", size: 4 },
				{ id: 3, name: ".env", size: 4 },
				{ id: 4, name: "README", size: 4 },
			],
			folders: [],
		});

		expect(fixture.writes).toEqual(
			expect.arrayContaining([
				"report (1).txt",
				"report (2).txt",
				".env (1)",
				"README (1)",
			]),
		);
	});

	it("keeps successful directory items and retries only failed files", async () => {
		const fixture = createDirectoryFixture();
		Object.defineProperty(window, "showDirectoryPicker", {
			configurable: true,
			value: vi.fn().mockResolvedValue(fixture.directory),
		});
		mocks.resolveResourceHandle.mockImplementation((id: number) =>
			Promise.resolve({
				request: { url: `/files/${id}`, credentials: "include" },
			}),
		);
		let secondAttempts = 0;
		const fetchMock = vi.fn().mockImplementation((url: string) => {
			if (url.endsWith("/2") && secondAttempts++ === 0) {
				return Promise.resolve(new Response(null, { status: 503 }));
			}
			return Promise.resolve(
				new Response(responseBody(["data"]), {
					status: 200,
					headers: { "content-length": "4" },
				}),
			);
		});
		vi.stubGlobal("fetch", fetchMock);

		const taskId = await startDirectoryDownload({
			workspace: { kind: "personal" },
			files: [
				{ id: 1, name: "first.txt", size: 4 },
				{ id: 2, name: "second.txt", size: 4 },
			],
			folders: [],
		});
		expect(useDownloadStore.getState().tasks[0]).toMatchObject({
			status: DOWNLOAD_TASK_STATUS.failed,
			completedItems: 1,
			failedItems: 1,
			error: "download_items_failed",
		});

		retryDownloadTask(taskId);
		await vi.waitFor(() =>
			expect(useDownloadStore.getState().tasks[0]).toMatchObject({
				status: DOWNLOAD_TASK_STATUS.completed,
				completedItems: 2,
				failedItems: 0,
			}),
		);
		expect(
			fetchMock.mock.calls.filter(([url]) => String(url).endsWith("/1")),
		).toHaveLength(1);
		expect(
			fetchMock.mock.calls.filter(([url]) => String(url).endsWith("/2")),
		).toHaveLength(2);
	});

	it("cancels an active streamed file task and leaves unknown ids as no-ops", async () => {
		mocks.resolveResourceHandle.mockResolvedValue({
			request: { url: "/files/1", credentials: "include" },
		});
		let releaseRead: (() => void) | undefined;
		const body = new ReadableStream<Uint8Array>({
			start(controller) {
				controller.enqueue(new TextEncoder().encode("a"));
				releaseRead = () => controller.close();
			},
		});
		vi.stubGlobal(
			"fetch",
			vi.fn().mockResolvedValue(new Response(body, { status: 200 })),
		);

		const download = startProxyFileDownload(
			{ kind: "personal" },
			{ id: 1, name: "a.txt" },
		);
		await vi.waitFor(() =>
			expect(useDownloadStore.getState().tasks[0]?.status).toBe(
				DOWNLOAD_TASK_STATUS.downloading,
			),
		);
		const taskId = useDownloadStore.getState().tasks[0]?.id ?? "";
		cancelDownloadTask(taskId);
		releaseRead?.();
		await download;
		expect(useDownloadStore.getState().tasks[0]).toMatchObject({
			status: DOWNLOAD_TASK_STATUS.canceled,
			error: undefined,
		});

		cancelDownloadTask("missing");
		retryDownloadTask("missing");
		expect(useDownloadStore.getState().tasks).toHaveLength(1);
	});

	it("expands selected folders and writes files with their relative paths", async () => {
		mocks.listFolder
			.mockResolvedValueOnce({
				folders: [{ id: 12, name: "nested" }],
				files: [],
				folders_total: 1,
				files_total: 0,
				next_file_cursor: null,
			})
			.mockResolvedValueOnce({
				folders: [],
				files: [{ id: 11, name: "root.txt", size: 4 }],
				folders_total: 0,
				files_total: 1,
				next_file_cursor: null,
			})
			.mockResolvedValueOnce({
				folders: [],
				files: [],
				folders_total: 0,
				files_total: 0,
				next_file_cursor: null,
			})
			.mockResolvedValueOnce({
				folders: [],
				files: [{ id: 13, name: "deep.txt", size: 4 }],
				folders_total: 0,
				files_total: 1,
				next_file_cursor: null,
			});
		mocks.resolveResourceHandle.mockImplementation((id: number) =>
			Promise.resolve({
				request: { url: `/files/${id}`, credentials: "include" },
			}),
		);
		vi.stubGlobal(
			"fetch",
			vi.fn().mockImplementation(() =>
				Promise.resolve(
					new Response(responseBody(["data"]), {
						status: 200,
						headers: { "content-length": "4" },
					}),
				),
			),
		);
		const writes: string[] = [];
		const createdDirectories = new Set<string>(["docs"]);
		const createdFiles = new Set<string>();
		const directory = (prefix = "") => ({
			getDirectoryHandle: vi.fn(
				(name: string, options?: { create?: boolean }) => {
					const path = `${prefix}${name}`;
					if (createdFiles.has(path)) {
						return Promise.reject(
							new DOMException("file exists", "TypeMismatchError"),
						);
					}
					if (options?.create) createdDirectories.add(path);
					if (!createdDirectories.has(path)) {
						return Promise.reject(new DOMException("missing", "NotFoundError"));
					}
					return Promise.resolve(directory(`${path}/`));
				},
			),
			getFileHandle: vi.fn((name: string, options?: { create?: boolean }) => {
				const path = `${prefix}${name}`;
				if (createdDirectories.has(path)) {
					return Promise.reject(
						new DOMException("directory exists", "TypeMismatchError"),
					);
				}
				if (!options?.create && !createdFiles.has(path)) {
					return Promise.reject(new DOMException("missing", "NotFoundError"));
				}
				if (options?.create) {
					createdFiles.add(path);
					writes.push(path);
				}
				return Promise.resolve({
					createWritable: vi.fn().mockResolvedValue({
						write: vi.fn().mockResolvedValue(undefined),
						close: vi.fn().mockResolvedValue(undefined),
					}),
				});
			}),
		});
		Object.defineProperty(window, "showDirectoryPicker", {
			configurable: true,
			value: vi.fn().mockResolvedValue(directory()),
		});

		await startDirectoryDownload({
			workspace: { kind: "personal" },
			files: [],
			folders: [{ id: 10, name: "docs" }],
		});

		expect(writes).toEqual(
			expect.arrayContaining(["docs (1)/root.txt", "docs (1)/nested/deep.txt"]),
		);
		expect(createdDirectories).toContain("docs (1)/nested");
		expect(useDownloadStore.getState().tasks[0]).toMatchObject({
			status: "completed",
			completedItems: 2,
			failedItems: 0,
			totalItems: 2,
		});
	});

	it("creates selected empty folders and completes without file transfers", async () => {
		mocks.listFolder
			.mockResolvedValueOnce({
				folders: [],
				files: [],
				folders_total: 0,
				files_total: 0,
				next_file_cursor: null,
			})
			.mockResolvedValueOnce({
				folders: [],
				files: [],
				folders_total: 0,
				files_total: 0,
				next_file_cursor: null,
			});
		const directories = new Set<string>();
		const directory = (prefix = "") => ({
			getDirectoryHandle: vi.fn(
				(name: string, options?: { create?: boolean }) => {
					const path = `${prefix}${name}`;
					if (options?.create) directories.add(path);
					if (!directories.has(path)) {
						return Promise.reject(new DOMException("missing", "NotFoundError"));
					}
					return Promise.resolve(directory(`${path}/`));
				},
			),
			getFileHandle: vi.fn(() =>
				Promise.reject(new DOMException("missing", "NotFoundError")),
			),
		});
		Object.defineProperty(window, "showDirectoryPicker", {
			configurable: true,
			value: vi.fn().mockResolvedValue(directory()),
		});

		await startDirectoryDownload({
			workspace: { kind: "personal" },
			files: [],
			folders: [{ id: 30, name: "empty" }],
		});

		expect(directories).toContain("empty");
		expect(mocks.resolveResourceHandle).not.toHaveBeenCalled();
		expect(useDownloadStore.getState().tasks[0]).toMatchObject({
			status: "completed",
			totalItems: 0,
		});
	});
});
