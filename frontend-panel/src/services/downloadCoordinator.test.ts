import { beforeEach, describe, expect, it, vi } from "vitest";
import {
	filenameFromContentDisposition,
	retryDownloadTask,
	startDirectoryDownload,
	startProxyArchiveDownload,
	startProxyFileDownload,
} from "@/services/downloadCoordinator";
import { useDownloadStore } from "@/stores/downloadStore";

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
		isPanelOpen: false,
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
