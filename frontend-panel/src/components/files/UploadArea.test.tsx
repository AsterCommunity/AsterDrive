import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { MeField } from "@/types/api";

const appendCompletedPart = vi.fn();
const cancelUpload = vi.fn();
const completeUpload = vi.fn();
const getProgress = vi.fn();
const initUpload = vi.fn();
const listRecoverableSessions = vi.fn();
const loadSessions = vi.fn(() => []);
const presignedUpload = vi.fn();
const presignParts = vi.fn();
const refresh = vi.fn().mockResolvedValue(undefined);
const refreshToken = vi.fn().mockResolvedValue(undefined);
const refreshUser = vi.fn().mockResolvedValue(undefined);
const removeSession = vi.fn();
const saveSession = vi.fn();
const uploadChunk = vi.fn();
const uploadPanelSpy = vi.fn();
const apiClientPost = vi.fn();

interface MockFileStoreState {
	breadcrumb: Array<{ id: number | null; name: string }>;
	currentFolderId: number | null;
	refresh: () => Promise<void>;
}

interface MockAuthStoreState {
	refreshToken: () => Promise<void>;
	refreshUser: (options?: { fields?: MeField[] }) => Promise<void>;
}

class MockApiError extends Error {
	code: number;

	constructor(code: number, message: string) {
		super(message);
		this.code = code;
	}
}

function createDeferred<T>() {
	let resolve!: (value: T | PromiseLike<T>) => void;
	let reject!: (reason?: unknown) => void;
	const promise = new Promise<T>((res, rej) => {
		resolve = res;
		reject = rej;
	});
	return { promise, resolve, reject };
}

function expectUploadChunkStarted(uploadId: string, chunkNumber: number) {
	const call = uploadChunk.mock.calls.find(
		(args) => args[0] === uploadId && args[1] === chunkNumber,
	);
	expect(call).toBeDefined();
	expect(call?.[2]).toBeInstanceOf(Blob);
	expect(call?.[3]).toEqual(expect.any(Function));
	return call;
}

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string) => key,
	}),
	initReactI18next: {
		type: "3rdParty",
		init: () => undefined,
	},
}));

vi.mock("@/components/files/UploadPanel", () => ({
	UploadPanel: (props: {
		emptyText: string;
		open: boolean;
		overallProgress?: number;
		summary: string;
		tasks: Array<{
			id: string;
			mode: string;
			progress: number;
			speed?: string;
			status: string;
			title: string;
			actions?: Array<{ label: string; onClick: () => void }>;
		}>;
	}) => {
		uploadPanelSpy(props);
		return (
			<div data-testid="upload-panel" data-open={String(props.open)}>
				<div>{`overall:${props.overallProgress ?? 0}`}</div>
				<div>{props.summary}</div>
				{props.tasks.length === 0 ? <div>{props.emptyText}</div> : null}
				{props.tasks.map((task) => (
					<div key={task.id}>
						<div>{`${task.title}:${task.mode}:${task.status}:${task.progress}`}</div>
						{task.speed ? (
							<div>{`${task.title}:speed:${task.speed}`}</div>
						) : null}
						<div>{`${task.title}:${task.mode}:${task.status}`}</div>
						{task.actions?.map((action) => (
							<button key={action.label} type="button" onClick={action.onClick}>
								{action.label}
							</button>
						))}
					</div>
				))}
			</div>
		);
	},
}));

vi.mock("@/stores/fileStore", () => ({
	useFileStore: <T,>(selector: (state: MockFileStoreState) => T) =>
		selector({
			breadcrumb: [
				{ id: null, name: "Root" },
				{ id: 42, name: "Projects" },
			],
			currentFolderId: 42,
			refresh,
		}),
}));

vi.mock("@/stores/authStore", () => ({
	useAuthStore: Object.assign(
		<T,>(selector: (state: MockAuthStoreState) => T) =>
			selector({
				refreshToken,
				refreshUser,
			}),
		{
			getState: () => ({
				refreshToken,
				refreshUser,
			}),
		},
	),
}));

vi.mock("@/lib/uploadPersistence", () => ({
	appendCompletedPart,
	loadSessions,
	removeSession,
	saveSession,
}));

vi.mock("@/services/uploadService", () => ({
	buildUploadPath: (_workspace: unknown, path: string) => path,
	isRetryableUploadError: (error: unknown) =>
		typeof error === "object" &&
		error !== null &&
		"retryable" in error &&
		(error as { retryable?: boolean }).retryable === true,
	uploadService: {
		cancelUpload,
		completeUpload,
		getProgress,
		initUpload,
		listRecoverableSessions,
		presignParts,
		presignedUpload,
		uploadChunk,
	},
}));

vi.mock("@/services/http", () => ({
	ApiError: MockApiError,
	api: {
		client: {
			post: apiClientPost,
		},
	},
}));

async function uploadOneFile() {
	const file = new File(["hello"], "hello.txt", { type: "text/plain" });
	await renderUploadAreaWithFiles([file]);

	await waitFor(() => {
		expect(initUpload).toHaveBeenCalledWith({
			filename: "hello.txt",
			total_size: file.size,
			folder_id: 42,
			relative_path: undefined,
		});
	});

	return file;
}

async function renderUploadAreaWithFiles(files: File[]) {
	const [{ UploadArea }, { UploadAreaHost }] = await Promise.all([
		import("@/components/files/UploadArea"),
		import("@/components/files/UploadAreaHost"),
	]);

	const view = render(
		<>
			<UploadAreaHost workspace={{ kind: "personal" }} />
			<UploadArea>
				<div>content</div>
			</UploadArea>
		</>,
	);

	const fileInput = view.container.querySelectorAll('input[type="file"]')[0] as
		| HTMLInputElement
		| undefined;

	if (!fileInput) {
		throw new Error("file input not found");
	}

	fireEvent.change(fileInput, {
		target: { files },
	});

	return view;
}

async function renderUploadArea() {
	const [{ UploadArea }, { UploadAreaHost }] = await Promise.all([
		import("@/components/files/UploadArea"),
		import("@/components/files/UploadAreaHost"),
	]);

	return render(
		<>
			<UploadAreaHost workspace={{ kind: "personal" }} />
			<UploadArea>
				<div>content</div>
			</UploadArea>
		</>,
	);
}

async function runUploadRestoreTimer() {
	try {
		await vi.advanceTimersByTimeAsync(700);
	} finally {
		vi.useRealTimers();
	}
}

async function renderUploadAreaWithRestoreTimer() {
	vi.useFakeTimers({ shouldAdvanceTime: true });
	const view = await renderUploadArea();
	await runUploadRestoreTimer();
	return view;
}

async function uploadFiles(files: File[]) {
	await renderUploadAreaWithFiles(files);

	await waitFor(() => {
		expect(initUpload).toHaveBeenCalledTimes(files.length);
	});
}

describe("UploadArea", () => {
	beforeEach(() => {
		appendCompletedPart.mockReset();
		apiClientPost.mockReset();
		cancelUpload.mockReset();
		completeUpload.mockReset();
		getProgress.mockReset();
		initUpload.mockReset();
		listRecoverableSessions.mockReset();
		listRecoverableSessions.mockResolvedValue([]);
		loadSessions.mockReset();
		loadSessions.mockReturnValue([]);
		presignedUpload.mockReset();
		presignParts.mockReset();
		refresh.mockReset();
		refresh.mockResolvedValue(undefined);
		refreshToken.mockReset();
		refreshToken.mockResolvedValue(undefined);
		refreshUser.mockReset();
		refreshUser.mockResolvedValue(undefined);
		removeSession.mockReset();
		saveSession.mockReset();
		uploadChunk.mockReset();
		uploadPanelSpy.mockReset();
		vi.unstubAllEnvs();
	});

	afterEach(() => {
		vi.useRealTimers();
	});

	it("hides the upload panel before any upload activity", async () => {
		await renderUploadAreaWithRestoreTimer();

		expect(screen.queryByTestId("upload-panel")).not.toBeInTheDocument();
		await waitFor(() => {
			expect(listRecoverableSessions).toHaveBeenCalledTimes(1);
		});
		expect(screen.queryByTestId("upload-panel")).not.toBeInTheDocument();
	});

	it("handles direct uploads through the form-data endpoint", async () => {
		initUpload.mockResolvedValue({ mode: "direct" });
		apiClientPost.mockResolvedValue({});

		await uploadOneFile();

		await screen.findByText("hello.txt:Direct:files:upload_success");

		expect(apiClientPost).toHaveBeenCalledTimes(1);
		expect(apiClientPost.mock.calls[0]?.[0]).toBe(
			"/files/upload?folder_id=42&declared_size=5",
		);
		expect(apiClientPost.mock.calls[0]?.[1]).toBeInstanceOf(FormData);
		expect(apiClientPost.mock.calls[0]?.[2]).toEqual(
			expect.objectContaining({
				headers: { "Content-Type": "multipart/form-data" },
				timeout: 0,
			}),
		);
		expect(completeUpload).not.toHaveBeenCalled();
		expect(saveSession).not.toHaveBeenCalled();
	});

	it("refreshes once after the whole upload queue settles", async () => {
		const firstUpload = createDeferred<unknown>();
		const secondUpload = createDeferred<unknown>();
		const firstFile = new File(["hello"], "first.txt", { type: "text/plain" });
		const secondFile = new File(["world"], "second.txt", {
			type: "text/plain",
		});

		initUpload.mockResolvedValue({ mode: "direct" });
		apiClientPost
			.mockReturnValueOnce(firstUpload.promise)
			.mockReturnValueOnce(secondUpload.promise);

		await uploadFiles([firstFile, secondFile]);
		await waitFor(() => {
			expect(apiClientPost).toHaveBeenCalledTimes(2);
		});

		firstUpload.resolve({});
		await screen.findByText("first.txt:Direct:files:upload_success");
		expect(refresh).not.toHaveBeenCalled();
		expect(refreshUser).not.toHaveBeenCalled();

		secondUpload.reject(new Error("upload failed"));
		await screen.findByText("second.txt:Direct:files:upload_failed");

		await waitFor(() => {
			expect(refresh).toHaveBeenCalledTimes(1);
			expect(refreshUser).toHaveBeenCalledTimes(1);
		});
	});

	it("keeps active uploads when the file browser route unmounts", async () => {
		const [{ UploadArea }, { UploadAreaHost }] = await Promise.all([
			import("@/components/files/UploadArea"),
			import("@/components/files/UploadAreaHost"),
		]);
		const upload = createDeferred<unknown>();
		const file = new File(["route"], "route-switch.txt", {
			type: "text/plain",
		});

		initUpload.mockResolvedValue({ mode: "direct" });
		apiClientPost.mockReturnValue(upload.promise);

		const view = render(
			<>
				<UploadAreaHost workspace={{ kind: "personal" }} />
				<UploadArea>
					<div>file route</div>
				</UploadArea>
			</>,
		);

		fireEvent.change(screen.getByTestId("upload-file-input"), {
			target: { files: [file] },
		});

		await waitFor(() => {
			expect(apiClientPost).toHaveBeenCalledTimes(1);
		});

		view.rerender(
			<>
				<UploadAreaHost workspace={{ kind: "personal" }} />
				<div>tasks route</div>
			</>,
		);

		expect(screen.getByText("tasks route")).toBeInTheDocument();
		upload.resolve({});

		await screen.findByText("route-switch.txt:Direct:files:upload_success");
		await waitFor(() => {
			expect(refresh).toHaveBeenCalledTimes(1);
			expect(refreshUser).toHaveBeenCalledTimes(1);
		});
	});

	it("does not refresh after an all-failed upload queue", async () => {
		const firstUpload = createDeferred<unknown>();
		const secondUpload = createDeferred<unknown>();
		const firstFile = new File(["hello"], "first.txt", { type: "text/plain" });
		const secondFile = new File(["world"], "second.txt", {
			type: "text/plain",
		});

		initUpload.mockResolvedValue({ mode: "direct" });
		apiClientPost
			.mockReturnValueOnce(firstUpload.promise)
			.mockReturnValueOnce(secondUpload.promise);

		await uploadFiles([firstFile, secondFile]);
		await waitFor(() => {
			expect(apiClientPost).toHaveBeenCalledTimes(2);
		});

		firstUpload.reject(new Error("first upload failed"));
		secondUpload.reject(new Error("second upload failed"));

		await screen.findByText("first.txt:Direct:files:upload_failed");
		await screen.findByText("second.txt:Direct:files:upload_failed");

		expect(refresh).not.toHaveBeenCalled();
		expect(refreshUser).not.toHaveBeenCalled();
	});

	it("uses the stored file upload concurrency setting", async () => {
		window.localStorage.setItem("aster-upload-concurrency", "1");
		const firstUpload = createDeferred<unknown>();
		const secondUpload = createDeferred<unknown>();
		const firstFile = new File(["hello"], "first.txt", { type: "text/plain" });
		const secondFile = new File(["world"], "second.txt", {
			type: "text/plain",
		});

		initUpload.mockResolvedValue({ mode: "direct" });
		apiClientPost
			.mockReturnValueOnce(firstUpload.promise)
			.mockReturnValueOnce(secondUpload.promise);

		await renderUploadAreaWithFiles([firstFile, secondFile]);

		await waitFor(() => {
			expect(apiClientPost).toHaveBeenCalledTimes(1);
		});
		expect(initUpload).toHaveBeenCalledTimes(1);
		expect(apiClientPost.mock.calls[0]?.[0]).toBe(
			"/files/upload?folder_id=42&declared_size=5",
		);

		firstUpload.resolve({});

		await waitFor(() => {
			expect(apiClientPost).toHaveBeenCalledTimes(2);
		});
		expect(initUpload).toHaveBeenCalledTimes(2);

		secondUpload.resolve({});
		await screen.findByText("second.txt:Direct:files:upload_success");
	});

	it("auto-removes completed tasks when the setting is enabled", async () => {
		window.localStorage.setItem("aster-upload-auto-clear-completed", "true");
		const file = new File(["hello"], "hello.txt", { type: "text/plain" });

		initUpload.mockResolvedValue({ mode: "direct" });
		apiClientPost.mockResolvedValue({});

		await renderUploadAreaWithFiles([file]);

		await waitFor(() => {
			expect(apiClientPost).toHaveBeenCalledTimes(1);
		});
		await waitFor(() => {
			expect(screen.getByTestId("upload-panel")).toHaveAttribute(
				"data-open",
				"false",
			);
		});
		expect(screen.getByText("files:upload_summary_empty")).toBeInTheDocument();
		expect(screen.getByText("files:upload_empty")).toBeInTheDocument();
		expect(uploadPanelSpy).toHaveBeenLastCalledWith(
			expect.objectContaining({
				open: false,
				summary: "files:upload_summary_empty",
				tasks: [],
			}),
		);
		await waitFor(() => {
			expect(refresh).toHaveBeenCalledTimes(1);
			expect(refreshUser).toHaveBeenCalledTimes(1);
		});
	});

	it("restores recoverable sessions listed by the backend", async () => {
		listRecoverableSessions.mockResolvedValue([
			{
				upload_id: "server-upload",
				mode: "chunked",
				status: "uploading",
				filename: "server.bin",
				total_size: 10,
				chunk_size: 5,
				total_chunks: 2,
				received_count: 1,
				folder_id: 42,
				chunks_on_disk: [0],
				completed_parts: [],
				expires_at: new Date(Date.now() + 60_000).toISOString(),
				updated_at: new Date().toISOString(),
			},
		]);

		await renderUploadAreaWithRestoreTimer();
		await screen.findByText("server.bin:Chunked:files:upload_pending_file");
		expect(listRecoverableSessions).toHaveBeenCalledTimes(1);
		expect(getProgress).not.toHaveBeenCalled();
	});

	it("handles chunked uploads and persists resumable sessions", async () => {
		initUpload.mockResolvedValue({
			mode: "chunked",
			upload_id: "upload-chunked",
			chunk_size: 5,
			total_chunks: 1,
		});
		uploadChunk.mockResolvedValue({});
		completeUpload.mockResolvedValue({ id: 9001 });

		await uploadOneFile();

		await screen.findByText("hello.txt:Chunked:files:upload_success");

		expect(saveSession).toHaveBeenCalledWith(
			expect.objectContaining({
				uploadId: "upload-chunked",
				mode: "chunked",
				baseFolderId: 42,
				baseFolderName: "Projects",
			}),
		);
		expectUploadChunkStarted("upload-chunked", 0);
		expect(completeUpload).toHaveBeenCalledWith("upload-chunked", undefined);
		expect(removeSession).toHaveBeenCalledWith("upload-chunked");
	});

	it("refreshes the session before retrying a chunk after token auth failure", async () => {
		initUpload.mockResolvedValue({
			mode: "chunked",
			upload_id: "upload-chunked",
			chunk_size: 5,
			total_chunks: 1,
		});
		uploadChunk
			.mockRejectedValueOnce(
				Object.assign(new Error("missing token"), {
					authFailure: true,
					retryable: true,
				}),
			)
			.mockResolvedValueOnce({});
		completeUpload.mockResolvedValue({ id: 9002 });

		await uploadOneFile();

		await screen.findByText("hello.txt:Chunked:files:upload_success");

		expect(uploadChunk).toHaveBeenCalledTimes(2);
		expect(refreshToken).toHaveBeenCalledTimes(1);
		expect(refreshToken.mock.invocationCallOrder[0]).toBeGreaterThan(
			uploadChunk.mock.invocationCallOrder[0],
		);
		expect(refreshToken.mock.invocationCallOrder[0]).toBeLessThan(
			uploadChunk.mock.invocationCallOrder[1],
		);
	});

	it("reports chunked upload progress before a chunk completes", async () => {
		vi.useFakeTimers({ shouldAdvanceTime: true });
		vi.setSystemTime(new Date("2026-05-19T00:00:00Z"));
		const chunkUpload = createDeferred<unknown>();
		initUpload.mockResolvedValue({
			mode: "chunked",
			upload_id: "upload-chunked",
			chunk_size: 100,
			total_chunks: 1,
		});
		uploadChunk.mockReturnValue(chunkUpload.promise);
		completeUpload.mockResolvedValue({ id: 9001 });

		await renderUploadAreaWithFiles([
			new File(["x".repeat(100)], "chunk-progress.bin"),
		]);

		await waitFor(() => {
			expectUploadChunkStarted("upload-chunked", 0);
		});

		const reportProgress = uploadChunk.mock.calls[0]?.[3] as
			| ((loaded: number, total: number) => void)
			| undefined;
		await vi.advanceTimersByTimeAsync(500);
		reportProgress?.(50, 100);
		await vi.advanceTimersByTimeAsync(500);

		await waitFor(() => {
			expect(uploadPanelSpy).toHaveBeenLastCalledWith(
				expect.objectContaining({
					overallProgress: 48,
					tasks: expect.arrayContaining([
						expect.objectContaining({
							progress: 48,
							speed: expect.stringMatching(/^\d+ B\/s$/),
							title: "chunk-progress.bin",
						}),
					]),
				}),
			);
		});

		chunkUpload.resolve({});
		await screen.findByText("chunk-progress.bin:Chunked:files:upload_success");
	});

	it("reports direct upload speed from browser upload progress", async () => {
		vi.useFakeTimers({ shouldAdvanceTime: true });
		vi.setSystemTime(new Date("2026-05-19T00:00:00Z"));
		const directUpload = createDeferred<unknown>();
		initUpload.mockResolvedValue({ mode: "direct" });
		apiClientPost.mockReturnValue(directUpload.promise);

		await renderUploadAreaWithFiles([
			new File(["x".repeat(100)], "direct-speed.bin"),
		]);

		await waitFor(() => {
			expect(apiClientPost).toHaveBeenCalledTimes(1);
		});

		const requestConfig = apiClientPost.mock.calls[0]?.[2] as
			| {
					onUploadProgress?: (event: {
						loaded: number;
						total?: number;
					}) => void;
			  }
			| undefined;
		await vi.advanceTimersByTimeAsync(500);
		requestConfig?.onUploadProgress?.({ loaded: 50, total: 100 });
		await vi.advanceTimersByTimeAsync(500);

		await waitFor(() => {
			expect(uploadPanelSpy).toHaveBeenLastCalledWith(
				expect.objectContaining({
					overallProgress: 50,
					tasks: expect.arrayContaining([
						expect.objectContaining({
							progress: 50,
							speed: expect.stringMatching(/^\d+ B\/s$/),
							title: "direct-speed.bin",
						}),
					]),
				}),
			);
		});

		directUpload.resolve({});
		await screen.findByText("direct-speed.bin:Direct:files:upload_success");
	});

	it("weights the overall upload progress by file size", async () => {
		window.localStorage.setItem("aster-upload-concurrency", "2");
		const smallUpload = createDeferred<unknown>();
		const largeUpload = createDeferred<unknown>();
		initUpload.mockResolvedValue({
			mode: "chunked",
			upload_id: "upload-weighted",
			chunk_size: 100,
			total_chunks: 1,
		});
		uploadChunk
			.mockReturnValueOnce(smallUpload.promise)
			.mockReturnValueOnce(largeUpload.promise);

		await renderUploadAreaWithFiles([
			new File(["x".repeat(10)], "small.bin"),
			new File(["x".repeat(100)], "large.bin"),
		]);

		await waitFor(() => {
			expect(uploadChunk).toHaveBeenCalledTimes(2);
		});

		const reportSmallProgress = uploadChunk.mock.calls.find(
			(call) => (call[2] as Blob).size === 10,
		)?.[3] as ((loaded: number, total: number) => void) | undefined;
		reportSmallProgress?.(10, 10);

		await waitFor(() => {
			expect(uploadPanelSpy).toHaveBeenLastCalledWith(
				expect.objectContaining({
					overallProgress: 9,
					tasks: expect.arrayContaining([
						expect.objectContaining({
							progress: 95,
							title: "small.bin",
						}),
						expect.objectContaining({
							progress: 0,
							title: "large.bin",
						}),
					]),
				}),
			);
		});

		smallUpload.resolve({});
		largeUpload.resolve({});
	});

	it("handles single-request presigned uploads", async () => {
		initUpload.mockResolvedValue({
			mode: "presigned",
			upload_id: "upload-presigned",
			presigned_url: "https://s3.example/upload",
		});
		presignedUpload.mockResolvedValue('"etag-123"');
		completeUpload.mockResolvedValue({ id: 9002 });

		await uploadOneFile();

		await screen.findByText("hello.txt:Presigned:files:upload_success");

		expect(presignedUpload).toHaveBeenCalledWith(
			"https://s3.example/upload",
			expect.any(File),
			expect.any(Function),
			expect.any(Function),
		);
		expect(completeUpload).toHaveBeenCalledWith("upload-presigned", undefined);
		expect(saveSession).not.toHaveBeenCalled();
	});

	it("reports presigned upload speed from PUT progress", async () => {
		vi.useFakeTimers({ shouldAdvanceTime: true });
		vi.setSystemTime(new Date("2026-05-19T00:00:00Z"));
		const presignedPut = createDeferred<string>();
		initUpload.mockResolvedValue({
			mode: "presigned",
			upload_id: "upload-presigned-speed",
			presigned_url: "https://s3.example/upload-speed",
		});
		presignedUpload.mockReturnValue(presignedPut.promise);
		completeUpload.mockResolvedValue({ id: 9011 });

		await renderUploadAreaWithFiles([
			new File(["x".repeat(100)], "presigned-speed.bin"),
		]);

		await waitFor(() => {
			expect(presignedUpload).toHaveBeenCalledWith(
				"https://s3.example/upload-speed",
				expect.any(File),
				expect.any(Function),
				expect.any(Function),
			);
		});

		const reportProgress = presignedUpload.mock.calls[0]?.[2] as
			| ((loaded: number, total: number) => void)
			| undefined;
		await vi.advanceTimersByTimeAsync(500);
		reportProgress?.(50, 100);
		await vi.advanceTimersByTimeAsync(500);

		await waitFor(() => {
			expect(uploadPanelSpy).toHaveBeenLastCalledWith(
				expect.objectContaining({
					tasks: expect.arrayContaining([
						expect.objectContaining({
							progress: 48,
							speed: expect.stringMatching(/^\d+ B\/s$/),
							title: "presigned-speed.bin",
						}),
					]),
				}),
			);
		});

		presignedPut.resolve('"etag-speed"');
		await screen.findByText(
			"presigned-speed.bin:Presigned:files:upload_success",
		);
	});

	it("handles multipart presigned uploads and completes with uploaded parts", async () => {
		initUpload.mockResolvedValue({
			mode: "presigned_multipart",
			upload_id: "upload-multipart",
			chunk_size: 5,
			total_chunks: 1,
		});
		presignParts.mockResolvedValue({
			1: "https://s3.example/upload/part-1",
		});
		presignedUpload.mockResolvedValue('"etag-001"');
		completeUpload.mockResolvedValue({ id: 9003 });

		await uploadOneFile();

		await screen.findByText(
			"hello.txt:Presigned Multipart:files:upload_success",
		);

		expect(saveSession).toHaveBeenCalledWith(
			expect.objectContaining({
				uploadId: "upload-multipart",
				mode: "presigned_multipart",
			}),
		);
		expect(presignParts).toHaveBeenCalledWith("upload-multipart", [1]);
		expect(appendCompletedPart).toHaveBeenCalledWith("upload-multipart", {
			part_number: 1,
			etag: "etag-001",
		});
		expect(completeUpload).toHaveBeenCalledWith("upload-multipart", [
			{
				part_number: 1,
				etag: "etag-001",
			},
		]);
		expect(removeSession).toHaveBeenCalledWith("upload-multipart");
	});

	it("reports multipart presigned upload speed from part progress", async () => {
		vi.useFakeTimers({ shouldAdvanceTime: true });
		vi.setSystemTime(new Date("2026-05-19T00:00:00Z"));
		const multipartPut = createDeferred<string>();
		initUpload.mockResolvedValue({
			mode: "presigned_multipart",
			upload_id: "upload-multipart-speed",
			chunk_size: 100,
			total_chunks: 1,
		});
		presignParts.mockResolvedValue({
			1: "https://s3.example/upload/part-speed",
		});
		presignedUpload.mockReturnValue(multipartPut.promise);
		completeUpload.mockResolvedValue({ id: 9012 });

		await renderUploadAreaWithFiles([
			new File(["x".repeat(100)], "multipart-speed.bin"),
		]);

		await waitFor(() => {
			expect(presignedUpload).toHaveBeenCalledWith(
				"https://s3.example/upload/part-speed",
				expect.any(Blob),
				expect.any(Function),
				expect.any(Function),
			);
		});

		const reportProgress = presignedUpload.mock.calls[0]?.[2] as
			| ((loaded: number, total: number) => void)
			| undefined;
		await vi.advanceTimersByTimeAsync(500);
		reportProgress?.(50, 100);
		await vi.advanceTimersByTimeAsync(500);

		await waitFor(() => {
			expect(uploadPanelSpy).toHaveBeenLastCalledWith(
				expect.objectContaining({
					tasks: expect.arrayContaining([
						expect.objectContaining({
							progress: 48,
							speed: expect.stringMatching(/^\d+ B\/s$/),
							title: "multipart-speed.bin",
						}),
					]),
				}),
			);
		});

		multipartPut.resolve('"etag-multipart-speed"');
		await screen.findByText(
			"multipart-speed.bin:Presigned Multipart:files:upload_success",
		);
	});

	it("batches presigned multipart URL requests", async () => {
		const partNumbers = Array.from({ length: 20 }, (_, index) => index + 1);
		initUpload.mockResolvedValue({
			mode: "presigned_multipart",
			upload_id: "upload-multipart-batched",
			chunk_size: 1,
			total_chunks: partNumbers.length,
		});
		presignParts.mockImplementation(
			async (_uploadId: string, requestedParts: number[]) =>
				Object.fromEntries(
					requestedParts.map((partNumber) => [
						partNumber,
						`https://s3.example/upload/part-${partNumber}`,
					]),
				),
		);
		presignedUpload.mockResolvedValue('"etag-001"');
		completeUpload.mockResolvedValue({ id: 9004 });

		await renderUploadAreaWithFiles([
			new File(["x".repeat(partNumbers.length)], "multipart-large.bin"),
		]);

		await screen.findByText(
			"multipart-large.bin:Presigned Multipart:files:upload_success",
		);

		expect(presignParts).toHaveBeenCalledTimes(2);
		expect(presignParts).toHaveBeenNthCalledWith(
			1,
			"upload-multipart-batched",
			partNumbers.slice(0, 16),
		);
		expect(presignParts).toHaveBeenNthCalledWith(
			2,
			"upload-multipart-batched",
			partNumbers.slice(16),
		);
	});

	it("resumes chunked uploads with the persisted chunk size instead of recomputing it", async () => {
		loadSessions.mockReturnValue([
			{
				uploadId: "upload-resume",
				filename: "resume.txt",
				totalSize: 11,
				totalChunks: 3,
				chunkSize: 5,
				baseFolderId: 42,
				baseFolderName: "Projects",
				relativePath: null,
				savedAt: Date.now(),
				mode: "chunked",
			},
		]);
		getProgress.mockResolvedValue({
			upload_id: "upload-resume",
			status: "uploading",
			received_count: 1,
			chunks_on_disk: [0],
			total_chunks: 3,
			filename: "resume.txt",
		});
		uploadChunk.mockResolvedValue({});
		completeUpload.mockResolvedValue({ id: 9004 });

		const file = new File(["hello world"], "resume.txt", {
			type: "text/plain",
		});

		await renderUploadAreaWithRestoreTimer();
		await screen.findByText("resume.txt:Chunked:files:upload_pending_file");
		fireEvent.click(screen.getByText("files:upload_resume_select"));
		fireEvent.change(screen.getByTestId("resume-input"), {
			target: { files: [file] },
		});

		await waitFor(() => {
			expectUploadChunkStarted("upload-resume", 1);
		});

		const resumedChunk = uploadChunk.mock.calls.find(
			(call) => call[0] === "upload-resume" && call[1] === 1,
		)?.[2];
		expect(resumedChunk).toBeInstanceOf(Blob);
		expect((resumedChunk as Blob).size).toBe(5);
	});

	it("turns failed persisted sessions into fresh reupload tasks", async () => {
		loadSessions.mockReturnValue([
			{
				uploadId: "upload-failed",
				filename: "failed.txt",
				totalSize: 11,
				totalChunks: 3,
				chunkSize: 5,
				baseFolderId: 42,
				baseFolderName: "Projects",
				relativePath: null,
				savedAt: Date.now(),
				mode: "chunked",
			},
		]);
		getProgress.mockResolvedValue({
			upload_id: "upload-failed",
			status: "failed",
			received_count: 2,
			chunks_on_disk: [0, 1],
			total_chunks: 3,
			filename: "failed.txt",
		});

		await renderUploadAreaWithRestoreTimer();
		await waitFor(() => {
			expect(removeSession).toHaveBeenCalledWith("upload-failed");
		});
		await screen.findByText("failed.txt:Chunked:files:upload_pending_file");
	});

	it("keeps persisted sessions when progress polling fails transiently", async () => {
		loadSessions.mockReturnValue([
			{
				uploadId: "upload-transient",
				filename: "transient.txt",
				totalSize: 11,
				totalChunks: 3,
				chunkSize: 5,
				baseFolderId: 42,
				baseFolderName: "Projects",
				relativePath: null,
				savedAt: Date.now(),
				mode: "chunked",
			},
		]);
		getProgress.mockRejectedValue(
			new MockApiError(4001, "temporary storage error"),
		);

		await renderUploadAreaWithRestoreTimer();
		await waitFor(() => {
			expect(getProgress).toHaveBeenCalledWith("upload-transient");
		});
		expect(removeSession).not.toHaveBeenCalled();
		expect(
			screen.queryByText("transient.txt:Chunked:files:upload_pending_file"),
		).not.toBeInTheDocument();
	});

	it("logs restored sessions with missing progress status in development", async () => {
		const warnSpy = vi
			.spyOn(console, "warn")
			.mockImplementation(() => undefined);
		vi.stubEnv("NODE_ENV", "development");

		loadSessions.mockReturnValue([
			{
				uploadId: "upload-missing-status",
				filename: "missing-status.txt",
				totalSize: 11,
				totalChunks: 3,
				chunkSize: 5,
				baseFolderId: 42,
				baseFolderName: "Projects",
				relativePath: null,
				savedAt: Date.now(),
				mode: "chunked",
			},
		]);
		getProgress.mockResolvedValue({
			upload_id: "upload-missing-status",
			received_count: 0,
			total_chunks: 3,
			filename: "missing-status.txt",
		});

		await renderUploadAreaWithRestoreTimer();
		await waitFor(() => {
			expect(getProgress).toHaveBeenCalledWith("upload-missing-status");
		});
		expect(warnSpy).toHaveBeenCalledWith(
			"[AsterDrive]",
			"skipping restored upload session because progress is missing a status",
			expect.objectContaining({
				progress: expect.objectContaining({
					filename: "missing-status.txt",
					received_count: 0,
				}),
				uploadId: "upload-missing-status",
			}),
		);
		expect(removeSession).not.toHaveBeenCalled();
		warnSpy.mockRestore();
	});

	it("limits persisted session progress preflight concurrency during restore", async () => {
		const sessions = Array.from({ length: 6 }, (_, index) => ({
			uploadId: `upload-restore-${index + 1}`,
			filename: `restore-${index + 1}.txt`,
			totalSize: 11,
			totalChunks: 3,
			chunkSize: 5,
			baseFolderId: 42,
			baseFolderName: "Projects",
			relativePath: null,
			savedAt: Date.now() + index,
			mode: "chunked" as const,
		}));
		const deferreds = sessions.map(() =>
			createDeferred<{
				upload_id: string;
				status: string;
				received_count: number;
				chunks_on_disk: number[];
				total_chunks: number;
				filename: string;
			}>(),
		);

		loadSessions.mockReturnValue(sessions);
		getProgress.mockImplementation((uploadId: string) => {
			const sessionIndex = sessions.findIndex(
				(session) => session.uploadId === uploadId,
			);
			if (sessionIndex < 0) {
				throw new Error(`unexpected uploadId ${uploadId}`);
			}
			return deferreds[sessionIndex].promise;
		});

		await renderUploadAreaWithRestoreTimer();
		await waitFor(() => {
			expect(getProgress).toHaveBeenCalledTimes(4);
		});
		expect(getProgress.mock.calls.map((call) => call[0])).toEqual(
			sessions.slice(0, 4).map((session) => session.uploadId),
		);

		for (const [index, deferred] of deferreds.slice(0, 4).entries()) {
			const session = sessions[index];
			deferred.resolve({
				upload_id: session.uploadId,
				status: "uploading",
				received_count: 1,
				chunks_on_disk: [0],
				total_chunks: session.totalChunks,
				filename: session.filename,
			});
		}

		await waitFor(() => {
			expect(getProgress).toHaveBeenCalledTimes(6);
		});
		expect(getProgress.mock.calls.map((call) => call[0])).toEqual(
			sessions.map((session) => session.uploadId),
		);

		for (const [index, deferred] of deferreds.slice(4).entries()) {
			const session = sessions[index + 4];
			deferred.resolve({
				upload_id: session.uploadId,
				status: "uploading",
				received_count: 1,
				chunks_on_disk: [0],
				total_chunks: session.totalChunks,
				filename: session.filename,
			});
		}

		await screen.findByText("restore-6.txt:Chunked:files:upload_pending_file");
	});

	it("does not reinitialize a new upload when resume preflight fails transiently", async () => {
		loadSessions.mockReturnValue([
			{
				uploadId: "upload-resume-transient",
				filename: "resume.txt",
				totalSize: 11,
				totalChunks: 3,
				chunkSize: 5,
				baseFolderId: 42,
				baseFolderName: "Projects",
				relativePath: null,
				savedAt: Date.now(),
				mode: "chunked",
			},
		]);
		getProgress
			.mockResolvedValueOnce({
				upload_id: "upload-resume-transient",
				status: "uploading",
				received_count: 1,
				chunks_on_disk: [0],
				total_chunks: 3,
				filename: "resume.txt",
			})
			.mockRejectedValueOnce(new Error("temporary progress failure"));

		const file = new File(["hello world"], "resume.txt", {
			type: "text/plain",
		});

		await renderUploadAreaWithRestoreTimer();
		await screen.findByText("resume.txt:Chunked:files:upload_pending_file");
		fireEvent.click(screen.getByText("files:upload_resume_select"));
		fireEvent.change(screen.getByTestId("resume-input"), {
			target: { files: [file] },
		});

		await screen.findByText("resume.txt:Chunked:files:upload_failed");

		expect(initUpload).not.toHaveBeenCalled();
		expect(uploadChunk).not.toHaveBeenCalled();
		expect(removeSession).not.toHaveBeenCalled();
	});

	it("continues assembling persisted uploads without asking for file selection", async () => {
		loadSessions.mockReturnValue([
			{
				uploadId: "upload-assembling",
				filename: "assembling.txt",
				totalSize: 11,
				totalChunks: 3,
				chunkSize: 5,
				baseFolderId: 42,
				baseFolderName: "Projects",
				relativePath: null,
				savedAt: Date.now(),
				mode: "chunked",
			},
		]);
		getProgress.mockResolvedValue({
			upload_id: "upload-assembling",
			status: "assembling",
			received_count: 3,
			chunks_on_disk: [0, 1, 2],
			total_chunks: 3,
			filename: "assembling.txt",
		});
		completeUpload.mockResolvedValue({ id: 9006 });

		await renderUploadAreaWithRestoreTimer();
		await screen.findByText("assembling.txt:Chunked:files:upload_success");
		expect(completeUpload).toHaveBeenCalledWith("upload-assembling", undefined);
		expect(removeSession).toHaveBeenCalledWith("upload-assembling");
	});

	it("keeps completion-only sessions retryable when completion fails", async () => {
		loadSessions.mockReturnValue([
			{
				uploadId: "upload-complete-retry",
				filename: "assembling.txt",
				totalSize: 11,
				totalChunks: 3,
				chunkSize: 5,
				baseFolderId: 42,
				baseFolderName: "Projects",
				relativePath: null,
				savedAt: Date.now(),
				mode: "chunked",
			},
		]);
		getProgress.mockResolvedValue({
			upload_id: "upload-complete-retry",
			status: "assembling",
			received_count: 3,
			chunks_on_disk: [0, 1, 2],
			total_chunks: 3,
			filename: "assembling.txt",
		});
		completeUpload
			.mockRejectedValueOnce(new Error("complete failed"))
			.mockResolvedValueOnce({ id: 9008 });

		await renderUploadAreaWithRestoreTimer();
		await screen.findByText("assembling.txt:Chunked:files:upload_failed");
		expect(removeSession).not.toHaveBeenCalled();

		fireEvent.click(screen.getByText("files:upload_retry"));

		await screen.findByText("assembling.txt:Chunked:files:upload_success");
		expect(completeUpload).toHaveBeenCalledTimes(2);
		expect(removeSession).toHaveBeenCalledWith("upload-complete-retry");
	});

	it("finalizes completed persisted uploads through idempotent completion", async () => {
		loadSessions.mockReturnValue([
			{
				uploadId: "upload-completed",
				filename: "completed.txt",
				totalSize: 11,
				totalChunks: 3,
				chunkSize: 5,
				baseFolderId: 42,
				baseFolderName: "Projects",
				relativePath: null,
				savedAt: Date.now(),
				mode: "chunked",
			},
		]);
		getProgress.mockResolvedValue({
			upload_id: "upload-completed",
			status: "completed",
			received_count: 3,
			chunks_on_disk: [0, 1, 2],
			total_chunks: 3,
			filename: "completed.txt",
		});
		completeUpload.mockResolvedValue({ id: 9008 });

		await renderUploadAreaWithRestoreTimer();
		await screen.findByText("completed.txt:Chunked:files:upload_success");
		expect(completeUpload).toHaveBeenCalledWith("upload-completed", undefined);
		expect(removeSession).toHaveBeenCalledWith("upload-completed");
	});

	it("resumes persisted multipart assembly with saved completed parts", async () => {
		loadSessions.mockReturnValue([
			{
				uploadId: "upload-multipart-assembling",
				filename: "multipart.txt",
				totalSize: 11,
				totalChunks: 3,
				chunkSize: 5,
				baseFolderId: 42,
				baseFolderName: "Projects",
				relativePath: null,
				savedAt: Date.now(),
				mode: "presigned_multipart",
				completedParts: [
					{ part_number: 1, etag: "etag-1" },
					{ part_number: 2, etag: "etag-2" },
					{ part_number: 3, etag: "etag-3" },
				],
			},
		]);
		getProgress.mockResolvedValue({
			upload_id: "upload-multipart-assembling",
			status: "assembling",
			received_count: 3,
			chunks_on_disk: [0, 1, 2],
			total_chunks: 3,
			filename: "multipart.txt",
		});
		completeUpload.mockResolvedValue({ id: 9009 });

		await renderUploadAreaWithRestoreTimer();
		await screen.findByText(
			"multipart.txt:Presigned Multipart:files:upload_success",
		);
		expect(completeUpload).toHaveBeenCalledWith("upload-multipart-assembling", [
			{ part_number: 1, etag: "etag-1" },
			{ part_number: 2, etag: "etag-2" },
			{ part_number: 3, etag: "etag-3" },
		]);
		expect(removeSession).toHaveBeenCalledWith("upload-multipart-assembling");
	});

	it("reinitializes instead of reusing an upload that failed before resume starts", async () => {
		loadSessions.mockReturnValue([
			{
				uploadId: "upload-stale",
				filename: "resume.txt",
				totalSize: 11,
				totalChunks: 3,
				chunkSize: 5,
				baseFolderId: 42,
				baseFolderName: "Projects",
				relativePath: null,
				savedAt: Date.now(),
				mode: "chunked",
			},
		]);
		getProgress
			.mockResolvedValueOnce({
				upload_id: "upload-stale",
				status: "uploading",
				received_count: 1,
				chunks_on_disk: [0],
				total_chunks: 3,
				filename: "resume.txt",
			})
			.mockResolvedValueOnce({
				upload_id: "upload-stale",
				status: "failed",
				received_count: 1,
				chunks_on_disk: [0],
				total_chunks: 3,
				filename: "resume.txt",
			});
		initUpload.mockResolvedValue({
			mode: "chunked",
			upload_id: "upload-new",
			chunk_size: 5,
			total_chunks: 3,
		});
		uploadChunk.mockResolvedValue({});
		completeUpload.mockResolvedValue({ id: 9007 });

		const file = new File(["hello world"], "resume.txt", {
			type: "text/plain",
		});

		await renderUploadAreaWithRestoreTimer();
		await screen.findByText("resume.txt:Chunked:files:upload_pending_file");
		fireEvent.click(screen.getByText("files:upload_resume_select"));
		fireEvent.change(screen.getByTestId("resume-input"), {
			target: { files: [file] },
		});

		await screen.findByText("resume.txt:Chunked:files:upload_success");

		expect(removeSession).toHaveBeenCalledWith("upload-stale");
		expect(
			uploadChunk.mock.calls.some((call) => call[0] === "upload-stale"),
		).toBe(false);
		expect(
			uploadChunk.mock.calls.some((call) => call[0] === "upload-new"),
		).toBe(true);
	});

	it("retries failed chunked uploads by reinitializing a new upload session", async () => {
		cancelUpload.mockResolvedValue(undefined);
		initUpload
			.mockResolvedValueOnce({
				mode: "chunked",
				upload_id: "upload-old",
				chunk_size: 5,
				total_chunks: 1,
			})
			.mockResolvedValueOnce({
				mode: "chunked",
				upload_id: "upload-new",
				chunk_size: 5,
				total_chunks: 1,
			});
		uploadChunk
			.mockRejectedValueOnce(
				Object.assign(new Error("upload failed"), { retryable: false }),
			)
			.mockResolvedValueOnce({});
		completeUpload.mockResolvedValue({ id: 9005 });

		await uploadOneFile();

		await screen.findByText("hello.txt:Chunked:files:upload_failed");
		fireEvent.click(screen.getByText("files:upload_retry"));

		await screen.findByText("hello.txt:Chunked:files:upload_success");

		expect(initUpload).toHaveBeenCalledTimes(2);
		expect(cancelUpload).toHaveBeenCalledWith("upload-old");
		expect(removeSession).toHaveBeenCalledWith("upload-old");
		expect(uploadChunk.mock.calls.at(-1)?.[0]).toBe("upload-new");
		expect(
			uploadChunk.mock.calls.filter((call) => call[0] === "upload-old"),
		).toHaveLength(1);
		expect(completeUpload).toHaveBeenCalledWith("upload-new", undefined);
	});

	it("waits for in-flight chunk requests to drain before canceling an old session on retry", async () => {
		const inFlightChunk = createDeferred<unknown>();

		cancelUpload.mockResolvedValue(undefined);
		initUpload
			.mockResolvedValueOnce({
				mode: "chunked",
				upload_id: "upload-old",
				chunk_size: 3,
				total_chunks: 2,
			})
			.mockResolvedValueOnce({
				mode: "chunked",
				upload_id: "upload-new",
				chunk_size: 5,
				total_chunks: 1,
			});
		uploadChunk
			.mockRejectedValueOnce(
				Object.assign(new Error("upload failed"), { retryable: false }),
			)
			.mockImplementationOnce(() => inFlightChunk.promise)
			.mockResolvedValueOnce({});
		completeUpload.mockResolvedValue({ id: 9010 });

		await uploadOneFile();

		await screen.findByText("hello.txt:Chunked:files:upload_failed");
		fireEvent.click(screen.getByText("files:upload_retry"));

		await Promise.resolve();
		expect(cancelUpload).not.toHaveBeenCalled();

		inFlightChunk.resolve({});

		await screen.findByText("hello.txt:Chunked:files:upload_success");

		expect(cancelUpload).toHaveBeenCalledWith("upload-old");
		expect(removeSession).toHaveBeenCalledWith("upload-old");
		expect(uploadChunk.mock.calls.at(-1)?.[0]).toBe("upload-new");
	});
});
