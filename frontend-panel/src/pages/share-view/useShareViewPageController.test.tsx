import { act, renderHook, waitFor } from "@testing-library/react";
import { useLayoutEffect } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ApiError } from "@/services/http";
import type {
	FileListItem,
	FolderContents,
	SharePublicInfo,
} from "@/types/api";
import { ApiErrorCode } from "@/types/api-helpers";
import { useShareViewPageController } from "./useShareViewPageController";

const TEST_SHARE_PASSWORD = "TEST_PASSWORD";

const mockState = vi.hoisted(() => ({
	buildShareFolderMusicQueue: vi.fn(),
	buildSingleShareMusicTrack: vi.fn(),
	downloadFolderFileUrl: vi.fn(
		(token: string, fileId: number) =>
			`https://download.example/s/${token}/files/${fileId}`,
	),
	downloadUrl: vi.fn((token: string) => `https://download.example/s/${token}`),
	getInfo: vi.fn(),
	handleApiError: vi.fn(),
	hydrateMusicQueueForPlayback: vi.fn(async (queue: unknown[]) => queue),
	intersectionCallbacks: [] as IntersectionObserverCallback[],
	isMusicFile: vi.fn(
		(file: { mime_type: string; file_category?: string }) =>
			file.file_category === "audio" || file.mime_type.startsWith("audio/"),
	),
	listContent: vi.fn(),
	listSubfolderContent: vi.fn(),
	openWindow: vi.fn(),
	playTracks: vi.fn(),
	previewAppStore: {
		isLoaded: false,
		load: vi.fn(async () => {}),
	},
	toastSuccess: vi.fn(),
	verifyPassword: vi.fn(),
}));

vi.mock("sonner", () => ({
	toast: {
		success: (...args: unknown[]) => mockState.toastSuccess(...args),
	},
}));

vi.mock("@/hooks/useApiError", () => ({
	handleApiError: (...args: unknown[]) => mockState.handleApiError(...args),
}));

vi.mock("@/lib/musicPlayer", () => ({
	buildShareFolderMusicQueue: (...args: unknown[]) =>
		mockState.buildShareFolderMusicQueue(...args),
	buildSingleShareMusicTrack: (...args: unknown[]) =>
		mockState.buildSingleShareMusicTrack(...args),
	hydrateMusicQueueForPlayback: (...args: unknown[]) =>
		mockState.hydrateMusicQueueForPlayback(...args),
	isMusicFile: (...args: unknown[]) => mockState.isMusicFile(...args),
}));

vi.mock("@/services/shareService", () => ({
	shareService: {
		downloadFolderFileUrl: (...args: unknown[]) =>
			mockState.downloadFolderFileUrl(...args),
		downloadUrl: (...args: unknown[]) => mockState.downloadUrl(...args),
		getInfo: (...args: unknown[]) => mockState.getInfo(...args),
		listContent: (...args: unknown[]) => mockState.listContent(...args),
		listSubfolderContent: (...args: unknown[]) =>
			mockState.listSubfolderContent(...args),
		verifyPassword: (...args: unknown[]) => mockState.verifyPassword(...args),
	},
}));

vi.mock("@/stores/musicPlayerStore", () => ({
	useMusicPlayerStore: (
		selector: (state: { playTracks: typeof mockState.playTracks }) => unknown,
	) => selector({ playTracks: mockState.playTracks }),
}));

vi.mock("@/stores/previewAppStore", () => ({
	usePreviewAppStore: (
		selector: (state: typeof mockState.previewAppStore) => unknown,
	) => selector(mockState.previewAppStore),
}));

function t(key: string) {
	return `t:${key}`;
}

function shareInfo(overrides: Partial<SharePublicInfo> = {}): SharePublicInfo {
	return {
		download_count: 0,
		expires_at: null,
		has_password: false,
		is_expired: false,
		max_downloads: 0,
		mime_type: null,
		name: "Shared Root",
		share_type: "folder",
		shared_by: {
			avatar: null,
			name: "Alice",
		},
		size: null,
		token: "share-token",
		view_count: 0,
		...overrides,
	} as SharePublicInfo;
}

function fileItem(
	id: number,
	name: string,
	mimeType = "text/plain",
): FileListItem {
	return {
		compound_extension: null,
		extension: name.includes(".") ? name.split(".").pop()?.toLowerCase() : "",
		file_category: mimeType.startsWith("audio/") ? "audio" : "document",
		id,
		is_locked: false,
		is_shared: false,
		mime_type: mimeType,
		name,
		size: id,
		updated_at: "2026-01-01T00:00:00Z",
	} as FileListItem;
}

function folderContents(
	overrides: Partial<FolderContents> = {},
): FolderContents {
	const files = overrides.files ?? [];
	const folders = overrides.folders ?? [];
	return {
		files,
		files_total: files.length,
		folders,
		folders_total: folders.length,
		next_file_cursor: null,
		...overrides,
	} as FolderContents;
}

function renderController({
	token = "share-token",
	withSentinel = false,
}: {
	token?: string;
	withSentinel?: boolean;
} = {}) {
	const sentinel = document.createElement("div");
	return renderHook(() => {
		const controller = useShareViewPageController({ token, t });
		useLayoutEffect(() => {
			if (withSentinel) {
				controller.sentinelRef.current = sentinel;
			}
		});
		return controller;
	});
}

function installIntersectionObserverMock() {
	class IntersectionObserverMock {
		constructor(callback: IntersectionObserverCallback) {
			mockState.intersectionCallbacks.push(callback);
		}

		disconnect = vi.fn();
		observe = vi.fn();
		takeRecords = vi.fn(() => []);
		unobserve = vi.fn();
	}

	window.IntersectionObserver =
		IntersectionObserverMock as unknown as typeof IntersectionObserver;
	globalThis.IntersectionObserver =
		IntersectionObserverMock as unknown as typeof IntersectionObserver;
}

describe("useShareViewPageController", () => {
	beforeEach(() => {
		mockState.buildShareFolderMusicQueue.mockReset();
		mockState.buildSingleShareMusicTrack.mockReset();
		mockState.downloadFolderFileUrl.mockClear();
		mockState.downloadUrl.mockClear();
		mockState.getInfo.mockReset();
		mockState.handleApiError.mockReset();
		mockState.hydrateMusicQueueForPlayback.mockReset();
		mockState.hydrateMusicQueueForPlayback.mockImplementation(
			async (queue: unknown[]) => queue,
		);
		mockState.intersectionCallbacks = [];
		mockState.isMusicFile.mockClear();
		mockState.listContent.mockReset();
		mockState.listSubfolderContent.mockReset();
		mockState.openWindow.mockReset();
		mockState.playTracks.mockReset();
		mockState.previewAppStore.isLoaded = false;
		mockState.previewAppStore.load.mockReset();
		mockState.previewAppStore.load.mockResolvedValue(undefined);
		mockState.toastSuccess.mockReset();
		mockState.verifyPassword.mockReset();
		mockState.verifyPassword.mockResolvedValue(undefined);
		installIntersectionObserverMock();
		Object.defineProperty(window, "open", {
			configurable: true,
			value: mockState.openWindow,
		});
	});

	it("loads folder shares, registers pagination, and appends cursor results", async () => {
		const firstFile = fileItem(1, "first.txt");
		const secondFile = fileItem(2, "second.txt");
		mockState.getInfo.mockResolvedValueOnce(shareInfo());
		mockState.listContent
			.mockResolvedValueOnce(
				folderContents({
					files: [firstFile],
					next_file_cursor: { id: 1, value: "first.txt" },
				}),
			)
			.mockResolvedValueOnce(folderContents({ files: [secondFile] }));

		const { result } = renderController({ withSentinel: true });

		await waitFor(() => {
			expect(result.current.loading).toBe(false);
		});
		expect(mockState.previewAppStore.load).toHaveBeenCalledTimes(1);
		expect(result.current.breadcrumb).toEqual([
			{ id: null, name: "Shared Root" },
		]);
		expect(result.current.hasMoreFiles).toBe(true);

		await waitFor(() => {
			expect(mockState.intersectionCallbacks).toHaveLength(1);
		});
		act(() => {
			mockState.intersectionCallbacks[0](
				[{ isIntersecting: true } as IntersectionObserverEntry],
				{} as IntersectionObserver,
			);
		});

		await waitFor(() => {
			expect(mockState.listContent).toHaveBeenLastCalledWith("share-token", {
				file_after_id: 1,
				file_after_value: "first.txt",
				file_limit: 100,
				folder_limit: 0,
			});
		});
		await waitFor(() => {
			expect(result.current.folderContents?.files).toEqual([
				firstFile,
				secondFile,
			]);
		});
		expect(result.current.loadingMore).toBe(false);
		expect(result.current.hasMoreFiles).toBe(false);
	});

	it("navigates folder breadcrumbs and reports navigation failures", async () => {
		const rootContents = folderContents({
			folders: [{ id: 10, name: "Docs" } as never],
		});
		const docsContents = folderContents({
			folders: [{ id: 11, name: "Deep" } as never],
		});
		const deepContents = folderContents();
		const error = new Error("network down");

		mockState.getInfo.mockResolvedValueOnce(shareInfo());
		mockState.listContent
			.mockResolvedValueOnce(rootContents)
			.mockResolvedValueOnce(rootContents);
		mockState.listSubfolderContent
			.mockResolvedValueOnce(docsContents)
			.mockResolvedValueOnce(deepContents)
			.mockResolvedValueOnce(docsContents)
			.mockRejectedValueOnce(error);

		const { result } = renderController();

		await waitFor(() => {
			expect(result.current.loading).toBe(false);
		});

		await act(async () => {
			await result.current.navigateToFolder(10, "Docs");
		});
		expect(result.current.breadcrumb).toEqual([
			{ id: null, name: "Shared Root" },
			{ id: 10, name: "Docs" },
		]);

		await act(async () => {
			await result.current.navigateToFolder(11, "Deep");
		});
		expect(result.current.breadcrumb).toEqual([
			{ id: null, name: "Shared Root" },
			{ id: 10, name: "Docs" },
			{ id: 11, name: "Deep" },
		]);

		await act(async () => {
			await result.current.navigateToFolder(10, "Docs");
		});
		expect(result.current.breadcrumb).toEqual([
			{ id: null, name: "Shared Root" },
			{ id: 10, name: "Docs" },
		]);

		await act(async () => {
			await result.current.navigateToFolder(null);
		});
		expect(result.current.breadcrumb).toEqual([
			{ id: null, name: "Shared Root" },
		]);

		await act(async () => {
			await result.current.navigateToFolder(12, "Broken");
		});
		expect(mockState.handleApiError).toHaveBeenCalledWith(error);
		expect(result.current.navigating).toBe(false);
	});

	it.each([
		[
			"not found",
			new ApiError(ApiErrorCode.ShareNotFound, "missing"),
			"t:errors:share_not_found",
		],
		[
			"download limit",
			new ApiError(ApiErrorCode.ShareDownloadLimitReached, "limited"),
			"t:share:download_limit_reached",
		],
		[
			"generic api error",
			new ApiError(ApiErrorCode.BadRequest, "bad request"),
			"bad request",
		],
		["unknown error", new Error("offline"), "t:share:failed_to_load_share"],
	])("maps %s load failures", async (_label, error, expectedMessage) => {
		mockState.getInfo.mockRejectedValueOnce(error);

		const { result } = renderController();

		await waitFor(() => {
			expect(result.current.loading).toBe(false);
		});
		expect(result.current.error).toBe(expectedMessage);
	});

	it("verifies password protected file shares without listing folder contents", async () => {
		mockState.getInfo.mockResolvedValueOnce(
			shareInfo({
				has_password: true,
				mime_type: "application/pdf",
				name: "Manual.pdf",
				share_type: "file",
				size: 512,
			}),
		);

		const { result } = renderController();

		await waitFor(() => {
			expect(result.current.needsPassword).toBe(true);
		});
		act(() => {
			result.current.setPassword(TEST_SHARE_PASSWORD);
		});
		await act(async () => {
			await result.current.handleVerifyPassword({
				preventDefault: vi.fn(),
			} as never);
		});

		expect(mockState.verifyPassword).toHaveBeenCalledWith("share-token", {
			password: TEST_SHARE_PASSWORD,
		});
		expect(mockState.toastSuccess).toHaveBeenCalledWith(
			"t:share:password_verified",
		);
		expect(mockState.listContent).not.toHaveBeenCalled();
		expect(result.current.passwordVerified).toBe(true);
		expect(result.current.needsPassword).toBe(false);
	});

	it("keeps protected shares locked when password verification fails", async () => {
		const error = new Error("wrong password");
		mockState.getInfo.mockResolvedValueOnce(
			shareInfo({ has_password: true, name: "Secret" }),
		);
		mockState.verifyPassword.mockRejectedValueOnce(error);

		const { result } = renderController();

		await waitFor(() => {
			expect(result.current.needsPassword).toBe(true);
		});
		await act(async () => {
			await result.current.handleVerifyPassword({
				preventDefault: vi.fn(),
			} as never);
		});

		expect(mockState.handleApiError).toHaveBeenCalledWith(error);
		expect(result.current.passwordVerified).toBe(false);
		expect(result.current.needsPassword).toBe(true);
	});

	it("opens download URLs with opener isolation and falls back to preview when no music track is active", async () => {
		const audioFile = fileItem(7, "song.mp3", "audio/mpeg");
		mockState.getInfo.mockResolvedValueOnce(shareInfo());
		mockState.listContent.mockResolvedValueOnce(
			folderContents({ files: [audioFile] }),
		);
		mockState.buildShareFolderMusicQueue.mockReturnValueOnce([]);

		const { result } = renderController();

		await waitFor(() => {
			expect(result.current.loading).toBe(false);
		});
		act(() => {
			result.current.handleDownload();
			result.current.handleFolderFileDownload(audioFile);
			result.current.handlePreviewFile(audioFile);
			result.current.setViewMode("list");
		});

		expect(mockState.openWindow).toHaveBeenCalledWith(
			"https://download.example/s/share-token",
			"_blank",
			"noopener,noreferrer",
		);
		expect(mockState.openWindow).toHaveBeenCalledWith(
			"https://download.example/s/share-token/files/7",
			"_blank",
			"noopener,noreferrer",
		);
		expect(result.current.previewFile).toBe(audioFile);
		expect(result.current.viewMode).toBe("list");
	});

	it("plays the active music track after hydrating share stream links", async () => {
		const audioFile = fileItem(8, "ready.mp3", "audio/mpeg");
		const activeTrack = {
			id: "share:share-token:file:8",
			name: "ready.mp3",
		};
		const hydratedTrack = {
			...activeTrack,
			path: "/stream/8",
		};
		mockState.getInfo.mockResolvedValueOnce(shareInfo());
		mockState.listContent.mockResolvedValueOnce(
			folderContents({ files: [audioFile] }),
		);
		mockState.buildShareFolderMusicQueue.mockReturnValueOnce([activeTrack]);
		mockState.hydrateMusicQueueForPlayback.mockResolvedValueOnce([
			hydratedTrack,
		]);

		const { result } = renderController();

		await waitFor(() => {
			expect(result.current.loading).toBe(false);
		});
		act(() => {
			result.current.handlePreviewFile(audioFile);
		});

		await waitFor(() => {
			expect(mockState.playTracks).toHaveBeenCalledWith(
				[hydratedTrack],
				"share:share-token:file:8",
			);
		});
		expect(result.current.previewFile).toBeNull();
	});

	it("opens the preview when music hydration fails", async () => {
		const audioFile = fileItem(9, "broken.mp3", "audio/mpeg");
		const error = new Error("stream unavailable");
		mockState.getInfo.mockResolvedValueOnce(shareInfo());
		mockState.listContent.mockResolvedValueOnce(
			folderContents({ files: [audioFile] }),
		);
		mockState.buildShareFolderMusicQueue.mockReturnValueOnce([
			{ id: "share:share-token:file:9", name: "broken.mp3" },
		]);
		mockState.hydrateMusicQueueForPlayback.mockRejectedValueOnce(error);

		const { result } = renderController();

		await waitFor(() => {
			expect(result.current.loading).toBe(false);
		});
		act(() => {
			result.current.handlePreviewFile(audioFile);
		});

		await waitFor(() => {
			expect(mockState.handleApiError).toHaveBeenCalledWith(error);
		});
		expect(result.current.previewFile).toBe(audioFile);
	});
});
